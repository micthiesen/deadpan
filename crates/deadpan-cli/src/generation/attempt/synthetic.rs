//! A deterministic stand-in for the model worker, for tests and UI replay.
//!
//! It never runs a model. It writes native footage that blends the attempt's
//! own two conditioning pictures, with a band whose colour follows the
//! attempt's seed so seeded variants are visibly different, and a worker
//! provenance report whose claims name this synthetic origin. Everything else
//! is the production path: the private workspace and captured inputs, the
//! store's durable lifecycle through the caller's `records`, host
//! qualification by `deadpan-media-worker` after the footage is complete, and
//! [`super::finish`]'s publication and Ready. A host must never offer it as a
//! generation provider.
//!
//! The footage is encoded like the development worker's lossless RGB
//! intermediate (`libx264rgb`, full-range sRGB tags) by an external `ffmpeg`,
//! which tests and replays must supply.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use deadpan_core::FrameDuration;
use deadpan_jobs::VideoSpec;
use deadpan_models::{BridgeContext, GenerationBinding};

use super::*;

/// Where the synthetic worker finds its two external tools.
#[derive(Debug, Clone)]
pub struct SyntheticWorker {
    /// An `ffmpeg` with `libx264rgb`, used only to encode the footage.
    pub ffmpeg: PathBuf,
    /// The host qualification helper, as for a real run.
    pub media_worker: PathBuf,
}

/// Run `allocated` with synthetic footage in place of the model worker.
/// Durable transitions reach `records` exactly as a real run's do; an `Err`
/// from it fails the attempt.
pub fn run(
    allocated: &Allocated,
    worker: &SyntheticWorker,
    progress: impl FnMut(AttemptProgress),
    records: impl FnMut(AttemptRecord) -> Result<(), String>,
    cancelled: &AtomicBool,
) -> WorkerRun {
    run_inner(allocated, worker, progress, records, cancelled, false)
}

/// Test-only path for exercising host qualification with a one-frame flash.
/// The normal synthetic worker and its public configuration stay unchanged.
#[cfg(test)]
pub(super) fn run_with_lighting_flash(
    allocated: &Allocated,
    worker: &SyntheticWorker,
    progress: impl FnMut(AttemptProgress),
    records: impl FnMut(AttemptRecord) -> Result<(), String>,
    cancelled: &AtomicBool,
) -> WorkerRun {
    run_inner(allocated, worker, progress, records, cancelled, true)
}

fn run_inner(
    allocated: &Allocated,
    worker: &SyntheticWorker,
    mut progress: impl FnMut(AttemptProgress),
    mut records: impl FnMut(AttemptRecord) -> Result<(), String>,
    cancelled: &AtomicBool,
    flash_middle_frame: bool,
) -> WorkerRun {
    let mut timings = RunTimings::default();
    let started = Instant::now();
    let cancel = |records: &mut dyn FnMut(AttemptRecord) -> Result<(), String>| {
        let _ = records(AttemptRecord::CancelRequested);
        RunResult::Cancelled
    };
    let failed =
        |code, reason: &str| RunResult::Failed(JobFailure::Host(host_failure(code, reason)));
    progress(AttemptProgress::Preparing);
    if cancelled.load(Ordering::Acquire) {
        return WorkerRun::early(cancel(&mut records), timings);
    }
    let prepared = match prepare_workspace(allocated, None, cancelled) {
        Ok(prepared) => prepared,
        Err(_) if cancelled.load(Ordering::Acquire) => {
            return WorkerRun::early(cancel(&mut records), timings);
        }
        Err(error) => {
            return WorkerRun::early(
                failed(
                    HostFailureCode::Io,
                    &format!("could not prepare the AI pause inputs: {error}"),
                ),
                timings,
            );
        }
    };
    timings.preparation = started.elapsed();
    let finish = |result, timings, directory| WorkerRun {
        result,
        timings,
        worker_log: String::new(),
        worker_log_discarded_bytes: 0,
        _directory: Some(directory),
    };
    let launched = Instant::now();
    let identity = &allocated.identity;
    for stage in [
        WorkerStage::Preflight,
        WorkerStage::ModelLoading,
        WorkerStage::Inference,
        WorkerStage::Encoding,
    ] {
        if cancelled.load(Ordering::Acquire) {
            return finish(cancel(&mut records), timings, prepared.directory);
        }
        progress(AttemptProgress::Stage(stage));
        let message = WorkerMessage::Stage {
            protocol: ProtocolVersion::V2,
            identity: identity.clone(),
            stage,
        };
        if let Err(error) = records(AttemptRecord::Worker(Box::new(message))) {
            return finish(
                failed(
                    HostFailureCode::Io,
                    &format!("could not save the attempt's progress: {error}"),
                ),
                timings,
                prepared.directory,
            );
        }
    }
    let declaration = match synthesize(
        allocated,
        worker,
        &prepared.worker,
        cancelled,
        flash_middle_frame,
    ) {
        Ok(declaration) => declaration,
        Err(_) if cancelled.load(Ordering::Acquire) => {
            return finish(cancel(&mut records), timings, prepared.directory);
        }
        Err(error) => {
            return finish(
                failed(HostFailureCode::WorkerExited, &error),
                timings,
                prepared.directory,
            );
        }
    };
    let completed = WorkerMessage::CompletedBridge {
        protocol: ProtocolVersion::V2,
        identity: identity.clone(),
        candidate: declaration.clone(),
    };
    if let Err(error) = records(AttemptRecord::Worker(Box::new(completed))) {
        return finish(
            failed(
                HostFailureCode::Io,
                &format!("could not save the attempt's completion: {error}"),
            ),
            timings,
            prepared.directory,
        );
    }
    timings.worker = launched.elapsed();
    progress(AttemptProgress::Qualifying);
    let qualifying = Instant::now();
    let qualified = qualify_declared(
        allocated,
        &worker.media_worker,
        &prepared.pinned,
        prepared.conditioning,
        declaration,
        cancelled,
    );
    timings.qualification = qualifying.elapsed();
    let result = match qualified {
        _ if cancelled.load(Ordering::Acquire) => cancel(&mut records),
        Ok(run) => RunResult::Qualified(Box::new(run)),
        Err(error) => failed(HostFailureCode::OutputValidationFailed, &error),
    };
    finish(result, timings, prepared.directory)
}

/// Write `outputs/native.mp4` and `outputs/provenance.json` and declare them.
fn synthesize(
    allocated: &Allocated,
    worker: &SyntheticWorker,
    root: &Path,
    cancelled: &AtomicBool,
    flash_middle_frame: bool,
) -> Result<NativeCandidateManifest, String> {
    let text = |error: &dyn std::fmt::Display| error.to_string();
    let plan = &allocated.inputs.plan;
    let width = plan.native_dimensions().width();
    let height = plan.native_dimensions().height();
    let count = plan.native_frame_count();
    let rate = plan.native_frame_rate();
    let picture = |bytes: &[u8]| -> Result<image::RgbImage, String> {
        let image = image::load_from_memory(bytes)
            .map_err(|error| text(&error))?
            .to_rgb8();
        if image.dimensions() != (width, height) {
            return Err("a conditioning picture differs from the native raster".into());
        }
        Ok(image)
    };
    let left = picture(&allocated.inputs.left_png)?;
    let right = picture(&allocated.inputs.right_png)?;
    let seed = allocated.provider().seed;
    // A band whose colour depends only on the seed.
    let band = [
        (seed.wrapping_mul(97) % 200 + 40) as u8,
        (seed.wrapping_mul(57) % 200 + 40) as u8,
        (seed.wrapping_mul(29) % 200 + 40) as u8,
    ];
    let band_rows = (height * 5 / 8)..(height * 5 / 8 + (height / 8).max(1));
    let native = root.join("outputs/native.mp4");
    let mut command = Command::new(&worker.ffmpeg);
    command
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-n"])
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24"])
        .args(["-video_size", &format!("{width}x{height}")])
        .args([
            "-framerate",
            &format!("{}/{}", rate.numerator(), rate.denominator()),
        ])
        .args(["-i", "pipe:0", "-an"])
        .args([
            "-vf",
            "setparams=range=full:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=gbr",
        ])
        .args(["-c:v", "libx264rgb", "-crf", "0", "-preset", "ultrafast"])
        .args([
            "-pix_fmt",
            "rgb24",
            "-color_range",
            "pc",
            "-colorspace",
            "rgb",
        ])
        .args(["-color_trc", "iec61966-2-1", "-color_primaries", "bt709"])
        .args(["-movflags", "+write_colr"])
        .args(["-video_track_timescale", &rate.numerator().to_string()])
        .arg(&native)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = deadpan_native_process::spawn(&mut command)
        .map_err(|error| format!("could not start {}: {error}", worker.ffmpeg.display()))?;
    let written = (|| -> Result<(), String> {
        let mut stdin = child.stdin.take().ok_or("ffmpeg has no input")?;
        let last = u64::from(count.saturating_sub(1)).max(1);
        let mut frame = vec![0_u8; (width * height * 3) as usize];
        for index in 0..u64::from(count) {
            if cancelled.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            if flash_middle_frame && index == u64::from(count / 2) {
                frame.fill(u8::MAX);
            } else {
                for (offset, (a, b)) in left.as_raw().iter().zip(right.as_raw()).enumerate() {
                    let mixed = (u64::from(*a) * (last - index.min(last))
                        + u64::from(*b) * index.min(last)
                        + last / 2)
                        / last;
                    frame[offset] = mixed as u8;
                }
                for row in band_rows.clone() {
                    let start = (row * width * 3) as usize;
                    for pixel in frame[start..start + (width * 3) as usize].chunks_exact_mut(3) {
                        pixel.copy_from_slice(&band);
                    }
                }
            }
            stdin.write_all(&frame).map_err(|error| text(&error))?;
        }
        Ok(())
    })();
    let status = child.wait().map_err(|error| text(&error))?;
    written?;
    if !status.success() {
        return Err(format!("ffmpeg exited with {status}"));
    }
    let footage = std::fs::read(&native).map_err(|error| text(&error))?;
    let native_sha = super::super::conditioning::sha256(&footage)?;
    let binding =
        GenerationBinding::from_request(&allocated.host_message).map_err(|error| text(&error))?;
    let context: BridgeContext =
        serde_json::from_slice(&allocated.inputs.manifest).map_err(|error| text(&error))?;
    let receipt = super::super::conditioning::sha256(b"deadpan synthetic worker")?;
    let revision = "0".repeat(40);
    let provenance = serde_json::to_vec(&serde_json::json!({
        "schema_version": 2,
        "request_binding": binding,
        "runtime_commit": revision,
        "pack_revision": revision,
        "gemma_revision": revision,
        "adapter_sources_sha256": {"synthetic/worker": receipt},
        "loaded_ltx_sources_sha256": {"synthetic/none": receipt},
        "verified_assets": [{"repository": "synthetic", "path": "none", "size": 1, "sha256": receipt}],
        "prompt_version": "synthetic-1",
        "prompt": "Synthetic test footage: a blend of the two boundary pictures; no model ran.",
        "seed": seed,
        "context": context,
        "configuration": {"backend": "synthetic"},
        "model_color_interpretation": "synthetic encoded sRGB",
        "temporal_interpolation": "synthetic linear blend",
        "conditioning_preprocessing": "none",
        "native_sha256": native_sha,
        "native_bytes": footage.len(),
    }))
    .map_err(|error| text(&error))?;
    let provenance_path = root.join("outputs/provenance.json");
    write_new(&provenance_path, &provenance).map_err(|error| text(&error))?;
    let provenance_sha = super::super::conditioning::sha256(&provenance)?;
    Ok(NativeCandidateManifest {
        native: workspace_artifact("outputs/native.mp4", &footage, &native_sha)?,
        provenance: workspace_artifact("outputs/provenance.json", &provenance, &provenance_sha)?,
        video: VideoSpec::new(
            FrameDuration::new(i64::from(count)).map_err(|error| text(&error))?,
            rate,
            width,
            height,
        )
        .map_err(|error| text(&error))?,
        provider: allocated.provider().clone(),
    })
}
