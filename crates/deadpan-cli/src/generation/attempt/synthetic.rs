//! A deterministic stand-in for the model worker, for tests and UI replay.
//!
//! It never runs a model. It writes native footage that blends the attempt's
//! own two Bridge conditioning pictures, with a band whose colour follows the
//! attempt's seed so seeded variants are visibly different. Extension footage
//! preserves its chronological context and repeats its conditioned seam. A worker
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
use deadpan_models::{
    BridgeContext, ExtensionContext, ExtensionGenerationBinding, GenerationBinding,
};

use super::*;

/// Where the synthetic worker finds its encoder and qualification helpers.
#[derive(Debug, Clone)]
pub struct SyntheticWorker {
    /// An `ffmpeg` with `libx264rgb`, used only to encode the footage.
    pub ffmpeg: PathBuf,
    /// The host qualification helper, as for a real run.
    pub media_worker: PathBuf,
    /// Required native landmark and face inspection helper beside the media
    /// worker.
    pub landmark_worker: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum SyntheticMode {
    Blend,
    #[cfg(test)]
    LightingFlash,
    #[cfg(test)]
    Uniform([u8; 3]),
}

impl SyntheticMode {
    fn write_override(self, index: u64, count: u32, frame: &mut [u8]) -> bool {
        #[cfg(test)]
        match self {
            Self::LightingFlash if index == u64::from(count / 2) => {
                frame.fill(u8::MAX);
                true
            }
            Self::Uniform(rgb) => {
                for pixel in frame.chunks_exact_mut(3) {
                    pixel.copy_from_slice(&rgb);
                }
                true
            }
            _ => false,
        }
        #[cfg(not(test))]
        {
            let _ = (index, count, frame);
            false
        }
    }
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
    run_inner(
        allocated,
        worker,
        progress,
        records,
        cancelled,
        SyntheticMode::Blend,
    )
}

/// Test-only path for exercising host qualification with deliberate defects.
/// The normal synthetic worker and its public configuration stay unchanged.
#[cfg(test)]
pub(super) fn run_with_mode(
    allocated: &Allocated,
    worker: &SyntheticWorker,
    progress: impl FnMut(AttemptProgress),
    records: impl FnMut(AttemptRecord) -> Result<(), String>,
    cancelled: &AtomicBool,
    mode: SyntheticMode,
) -> WorkerRun {
    run_inner(allocated, worker, progress, records, cancelled, mode)
}

fn run_inner(
    allocated: &Allocated,
    worker: &SyntheticWorker,
    mut progress: impl FnMut(AttemptProgress),
    mut records: impl FnMut(AttemptRecord) -> Result<(), String>,
    cancelled: &AtomicBool,
    mode: SyntheticMode,
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
    let prepared = deadpan_models::packs::approved_pack(allocated.provider().pack_id.as_str())
        .ok_or_else(|| "Synthetic execution requires an approved operation manifest.".to_owned())
        .and_then(|manifest| {
            super::super::runtime::validate_constraints_for_manifest(
                &manifest,
                allocated.inputs.constraints(),
            )?;
            super::super::runtime::selected_provider_for_manifest(
                &manifest,
                &allocated.inputs.plan(),
                allocated.provider().seed,
            )
        })
        .and_then(|selected| prepare_workspace(allocated, None, selected, cancelled));
    let prepared = match prepared {
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
            protocol: allocated.protocol(),
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
    let declaration = match synthesize(allocated, worker, &prepared.worker, cancelled, mode) {
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
    let completed = match &allocated.inputs {
        PreparedInputs::Bridge(_) => WorkerMessage::CompletedBridge {
            protocol: ProtocolVersion::V2,
            identity: identity.clone(),
            candidate: declaration.clone(),
        },
        PreparedInputs::Extension(_) => WorkerMessage::CompletedExtension {
            protocol: ProtocolVersion::V3,
            identity: identity.clone(),
            candidate: declaration.clone(),
        },
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
        &worker.landmark_worker,
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
pub(super) fn synthesize(
    allocated: &Allocated,
    worker: &SyntheticWorker,
    root: &Path,
    cancelled: &AtomicBool,
    mode: SyntheticMode,
) -> Result<NativeCandidateManifest, String> {
    let text = |error: &dyn std::fmt::Display| error.to_string();
    let plan = allocated.inputs.plan();
    let width = plan.native_dimensions().width();
    let height = plan.native_dimensions().height();
    let count = native_frame_count(&plan);
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
    let pictures = match &allocated.inputs {
        PreparedInputs::Bridge(inputs) => {
            vec![picture(&inputs.left_png)?, picture(&inputs.right_png)?]
        }
        PreparedInputs::Extension(inputs) => inputs
            .context_pngs
            .iter()
            .map(|png| picture(png))
            .collect::<Result<Vec<_>, _>>()?,
    };
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
            if !mode.write_override(index, count, &mut frame) {
                match &plan {
                    GenerationPlan::Bridge(_) => {
                        let left = &pictures[0];
                        let right = &pictures[1];
                        for (offset, (a, b)) in left.as_raw().iter().zip(right.as_raw()).enumerate()
                        {
                            let mixed = (u64::from(*a) * (last - index.min(last))
                                + u64::from(*b) * index.min(last)
                                + last / 2)
                                / last;
                            frame[offset] = mixed as u8;
                        }
                        for row in band_rows.clone() {
                            let start = (row * width * 3) as usize;
                            for pixel in
                                frame[start..start + (width * 3) as usize].chunks_exact_mut(3)
                            {
                                pixel.copy_from_slice(&band);
                            }
                        }
                    }
                    GenerationPlan::Extension(plan) => {
                        let context_index = match plan.direction() {
                            deadpan_core::ExtensionDirection::FromLeft => {
                                index.min(u64::from(plan.context_frame_count() - 1))
                            }
                            deadpan_core::ExtensionDirection::FromRight => {
                                index.saturating_sub(u64::from(plan.generated_frame_count()))
                            }
                        };
                        frame.copy_from_slice(pictures[context_index as usize].as_raw());
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
    let receipt = super::super::conditioning::sha256(b"deadpan synthetic worker")?;
    let revision = "0".repeat(40);
    let mut provenance = serde_json::json!({
        "runtime_commit": revision,
        "pack_revision": revision,
        "gemma_revision": revision,
        "adapter_sources_sha256": {"synthetic/worker": receipt},
        "loaded_ltx_sources_sha256": {"synthetic/none": receipt},
        "verified_assets": [{"repository": "synthetic", "path": "none", "size": 1, "sha256": receipt}],
        "prompt_version": "synthetic-1",
        "prompt": format!("Synthetic test footage using {mode:?}; no model ran."),
        "seed": seed,
        "configuration": {"backend": "synthetic", "mode": format!("{mode:?}")},
        "model_color_interpretation": "synthetic encoded sRGB",
        "temporal_interpolation": "synthetic blend or seam repetition; no model was loaded",
        "conditioning_preprocessing": "none",
        "native_sha256": native_sha,
        "native_bytes": footage.len(),
    });
    match &allocated.inputs {
        PreparedInputs::Bridge(inputs) => {
            let binding = GenerationBinding::from_request(&allocated.host_message)
                .map_err(|error| text(&error))?;
            let context: BridgeContext =
                serde_json::from_slice(&inputs.manifest).map_err(|error| text(&error))?;
            provenance["schema_version"] = serde_json::json!(2);
            provenance["request_binding"] = serde_json::json!(binding);
            provenance["context"] = serde_json::json!(context);
        }
        PreparedInputs::Extension(inputs) => {
            let binding = ExtensionGenerationBinding::from_request(&allocated.host_message)
                .map_err(|error| text(&error))?;
            let context: ExtensionContext =
                serde_json::from_slice(&inputs.manifest).map_err(|error| text(&error))?;
            let plan = &inputs.plan;
            let interval = plan.sampling_map().generated_interval();
            let manifest =
                deadpan_models::packs::approved_pack(allocated.provider().pack_id.as_str())
                    .ok_or("synthetic operation manifest is unavailable")?;
            let manifest_bytes = serde_json::to_vec(&manifest).map_err(|error| text(&error))?;
            let extension = serde_json::json!({
                "schema_version": 3, "operation": "extension", "direction": plan.direction(),
                "request_binding": binding, "context": context,
                "generated_interval": [interval.start, interval.end],
                "pack_id": allocated.provider().pack_id, "pack_version": allocated.provider().pack_version,
                "runtime_id": allocated.provider().runtime_id, "runtime_version": allocated.provider().runtime_version,
                "model_manifest_sha256": super::super::conditioning::sha256(&manifest_bytes)?,
                "timing": {
                    "requested_duration": plan.requested_duration(),
                    "generated_duration": plan.generated_duration(),
                    "native_movie_duration": plan.native_movie_duration(),
                    "context_duration": plan.context_duration(),
                    "context_anchor_span": plan.context_anchor_span(),
                    "speed_conversion": plan.speed(),
                    "retime_deviation": plan.retime_deviation(),
                },
            });
            provenance
                .as_object_mut()
                .expect("synthetic report is an object")
                .extend(
                    extension
                        .as_object()
                        .expect("extension report is an object")
                        .clone(),
                );
        }
    }
    let provenance = serde_json::to_vec(&provenance).map_err(|error| text(&error))?;
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
