//! Real-encoder measurement of the probe content oracle on this host. Ignored
//! by default: run with `--ignored --nocapture` on Apple Silicon. This runs
//! encode plus the content oracle in-process; it does not replace supervised
//! admission or finished-file verification.

use std::{
    os::unix::fs::OpenOptionsExt,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_core::ColorPolicy;
use deadpan_encode::{BFramePolicy, EncoderMode, EncoderSession, NextInput};

use super::{AdmissionLimits, ProbeSpec, content};
use crate::encoded_render::protocol::EncoderChoice;

fn run(spec: &ProbeSpec) -> Result<(content::ProbeContentReport, i32, u64), String> {
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let limits = AdmissionLimits::default().encode;
    let generator = spec.generator()?;
    let native = spec.contract()?.native_contract()?;
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let path = directory.path().join("movie.mp4");
    let output = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|error| error.to_string())?;
    let mut encoder = EncoderSession::open(output, native, limits, &cancelled, deadline)
        .map_err(|error| format!("{:?}: {error}", error.kind()))?;
    let mut picture = vec![0; usize::try_from(generator.config().picture_bytes).unwrap()];
    let (mut left, mut right) = ([0_f32; 1024], [0_f32; 1024]);
    loop {
        match encoder
            .next_input()
            .map_err(|e| format!("{:?}: {e}", e.kind()))?
        {
            NextInput::Picture {
                ordinal,
                pts,
                duration,
            } => {
                generator
                    .fill_picture(ordinal, &mut picture)
                    .map_err(|error| error.to_string())?;
                encoder
                    .push_picture(ordinal, pts, duration, &picture)
                    .map_err(|e| format!("{:?}: {e}", e.kind()))?;
            }
            NextInput::Audio {
                first_sample,
                samples,
            } => {
                let count = usize::try_from(samples).unwrap();
                generator
                    .fill_audio(first_sample, &mut left[..count], &mut right[..count])
                    .map_err(|error| error.to_string())?;
                encoder
                    .push_audio(first_sample, &left[..count], &right[..count])
                    .map_err(|e| format!("{:?}: {e}", e.kind()))?;
            }
            NextInput::Finish => break,
        }
    }
    let (file, report) = encoder
        .finish_with_light(content::declared_light(&generator, &cancelled, deadline)?)
        .map_err(|e| format!("{:?}: {e}", e.kind()))?
        .into_parts();
    if let Some(root) = std::env::var_os("DEADPAN_PROBE_RETAIN") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        std::fs::copy(
            &path,
            root.join(format!(
                "{:?}-{:?}-{}x{}.mp4",
                spec.choice.mode, spec.choice.b_frames, spec.raster[0], spec.raster[1]
            )),
        )
        .map_err(|e| e.to_string())?;
    }
    let measured = content::inspect(
        &file,
        spec,
        limits.maximum_output_bytes,
        limits.maximum_packets,
        &cancelled,
        deadline,
    )?;
    Ok((measured, report.info.video_profile, report.output_bytes))
}

#[test]
#[ignore = "measurement of real small-raster VideoToolbox encoders"]
fn measure_small_sdr_probe_geometry() {
    for mode in [EncoderMode::Hardware, EncoderMode::Software] {
        for b_frames in [BFramePolicy::None, BFramePolicy::TargetTwo] {
            let spec = ProbeSpec {
                raster: [96, 64],
                frame_rate: [50, 1],
                choice: EncoderChoice { mode, b_frames },
                color_policy: ColorPolicy::SdrRec709,
            };
            eprintln!("{mode:?} {b_frames:?}: {:?}", run(&spec));
        }
    }
}

#[test]
#[ignore = "requires the host's real VideoToolbox/OS encoders"]
fn measure_hdr_probe_content_on_this_host() {
    let mut failures = Vec::new();
    for color in [
        ColorPolicy::HdrRec2020Pq,
        ColorPolicy::HdrRec2020Hlg,
        ColorPolicy::SdrRec709,
    ] {
        for (raster, rate) in [([320, 180], [30_000, 1001]), ([1920, 1080], [30, 1])] {
            for mode in [EncoderMode::Hardware, EncoderMode::Software] {
                for b_frames in [BFramePolicy::None, BFramePolicy::TargetTwo] {
                    let spec = ProbeSpec {
                        raster,
                        frame_rate: rate,
                        choice: EncoderChoice { mode, b_frames },
                        color_policy: color,
                    };
                    match run(&spec) {
                        Ok((report, profile, bytes)) => eprintln!(
                            "{color:?} {raster:?} {mode:?} {b_frames:?}: profile={profile} bytes={bytes} max={:?} mae_milli={:?} mse_milli={:?}",
                            report.maximum_plane_error,
                            report.worst_frame_mean_absolute_error_milli,
                            report.worst_frame_mean_squared_error_milli,
                        ),
                        Err(error) => {
                            eprintln!("{color:?} {raster:?} {mode:?} {b_frames:?}: FAILED {error}");
                            failures.push(error);
                        }
                    }
                }
            }
        }
    }
    eprintln!("{} failures", failures.len());
}

#[test]
#[ignore = "measurement"]
fn measure_hdr_probe_declared_light() {
    for raster in [[320, 180], [1920, 1080], [3840, 2160]] {
        let spec = ProbeSpec {
            raster,
            frame_rate: [30_000, 1001],
            choice: EncoderChoice {
                mode: EncoderMode::Hardware,
                b_frames: BFramePolicy::None,
            },
            color_policy: ColorPolicy::HdrRec2020Pq,
        };
        let light = content::declared_light(
            &spec.generator().unwrap(),
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap();
        eprintln!("{raster:?}: {light:?}");
    }
}
