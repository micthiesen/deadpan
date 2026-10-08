//! Developer verification of a completed, reaped extension worker's media.
//! Includes pixel, face, mouth and region checks against retained inputs.
//! It does not admit Ready or edit a project.

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    use deadpan_jobs::artifact::{ArtifactLimits, ArtifactWorkspace, SnapshotInterruption};
    use deadpan_jobs::{HostMessage, NativeCandidateManifest};
    use deadpan_media::protocol::{
        ConversionLimits, EXTENSION_PROTOCOL_VERSION, ExtensionConversionRequest,
        ExtensionOperation, VideoContract,
    };
    use deadpan_media::{InputIdentity, canonicalize_extension};
    use deadpan_models::{
        ConditioningLimits, ExtensionConditioningReceipt, capture_extension_conditioning,
        inspect_extension_geometry, inspect_extension_pixels,
    };
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Configuration {
        codec: PathBuf,
        tracker: PathBuf,
        workspace: PathBuf,
        request: HostMessage,
        candidate: NativeCandidateManifest,
        output_directory: PathBuf,
        limits: ConversionLimits,
        retained_inputs: RetainedInputs,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RetainedInputs {
        directory: PathBuf,
        /// The host's pre-launch receipt, not a receipt supplied by the worker.
        receipt: ExtensionConditioningReceipt,
        input_scope: deadpan_jobs::WorkspaceRef,
        limits: ConditioningLimits,
    }

    fn write_object(path: &Path, source: &mut impl Read, length: u64) -> std::io::Result<()> {
        let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
        if std::io::copy(source, &mut file)? != length {
            return Err(std::io::Error::other("verified object length changed"));
        }
        file.flush()?;
        file.sync_all()
    }

    let mut arguments = std::env::args_os().skip(1);
    let path = arguments.next().ok_or("expected configuration path")?;
    if arguments.next().is_some() {
        return Err("expected exactly one configuration path".into());
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(1_048_577).read_to_end(&mut bytes)?;
    if bytes.len() > 1_048_576 {
        return Err("configuration exceeds 1 MiB".into());
    }
    let config: Configuration = serde_json::from_slice(&bytes)?;
    if [
        &config.codec,
        &config.tracker,
        &config.workspace,
        &config.output_directory,
        &config.retained_inputs.directory,
    ]
    .iter()
    .any(|path| !path.is_absolute())
    {
        return Err("host paths must be absolute".into());
    }
    config.request.validate()?;
    config.candidate.validate()?;
    config.limits.validate()?;
    let started = Instant::now();
    let deadline = started + Duration::from_millis(config.limits.timeout_ms);
    let remaining_ms = || -> Result<u64, Box<dyn std::error::Error>> {
        let remaining = u64::try_from(
            deadline
                .saturating_duration_since(Instant::now())
                .as_millis(),
        )?;
        if remaining == 0 {
            return Err("extension inspection exceeded its shared deadline".into());
        }
        Ok(remaining)
    };
    let cancelled = AtomicBool::new(false);
    let retained_workspace = ArtifactWorkspace::open(&config.retained_inputs.directory)?;
    let mut conditioning = capture_extension_conditioning(
        &retained_workspace,
        &config.request,
        config.retained_inputs.receipt.manifest().declaration(),
        &config.retained_inputs.input_scope,
        ConditioningLimits {
            timeout_ms: remaining_ms()?.min(config.retained_inputs.limits.timeout_ms),
            ..config.retained_inputs.limits
        },
        &cancelled,
    )?;
    if conditioning.receipt() != &config.retained_inputs.receipt {
        return Err("retained inputs differ from the host's pre-launch receipt".into());
    }
    let HostMessage::GenerateExtension {
        plan,
        provider,
        output_workspace,
        ..
    } = &config.request
    else {
        return Err("expected an extension request".into());
    };
    let dimensions = plan.native_dimensions();
    let rate = plan.native_frame_rate();
    let video = &config.candidate.video;
    if video.frames().frames() != i64::from(plan.native_frame_count())
        || video.frame_rate() != rate
        || video.width() != dimensions.width()
        || video.height() != dimensions.height()
        || &config.candidate.provider != provider.as_ref()
    {
        return Err("candidate differs from captured request".into());
    }
    let workspace = ArtifactWorkspace::open(&config.workspace)?;
    let mut input = workspace.snapshot_with_control(
        output_workspace,
        &config.candidate.native,
        ArtifactLimits::new(config.limits.max_input_bytes)?,
        || {
            if Instant::now() >= deadline {
                Err(SnapshotInterruption::Deadline)
            } else {
                Ok(())
            }
        },
    )?;
    let mut digest = [0_u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(
            &config.candidate.native.sha256().as_str()[2 * index..2 * index + 2],
            16,
        )?;
    }
    let canonical = canonicalize_extension(
        &config.codec,
        &mut input,
        InputIdentity { sha256: digest },
        &ExtensionConversionRequest {
            protocol: EXTENSION_PROTOCOL_VERSION,
            operation: ExtensionOperation::SampleExtension,
            native: VideoContract {
                width: dimensions.width(),
                height: dimensions.height(),
                frames: plan.native_frame_count(),
                rate_num: rate.numerator(),
                rate_den: rate.denominator(),
            },
            sampling: plan.sampling_map().clone(),
            input_byte_length: config.candidate.native.byte_length(),
            limits: ConversionLimits {
                timeout_ms: remaining_ms()?,
                ..config.limits
            },
        },
        &cancelled,
    )?;
    let pixels = inspect_extension_pixels(
        &canonical,
        &mut conditioning,
        &config.request,
        deadline,
        &cancelled,
    )?;
    let (mut native, mut sampled, sampling) = canonical.into_parts();
    let geometry = inspect_extension_geometry(
        &config.tracker,
        &mut native,
        &mut conditioning,
        &config.request,
        deadline,
        &cancelled,
    )?;
    let report = json!({
        "scope": "decoded native and generated-only sampled media with motion, lighting, real-join, single-anchor face/region and chronological mouth rejection; provenance admission, Ready and acceptance remain open",
        "native": {"object": native.object(), "report": native.report()},
        "sampled": {"object": sampled.object(), "report": sampled.report()},
        "sampling": sampling,
        "pixels": pixels,
        "geometry": geometry,
        "elapsed_seconds": started.elapsed().as_secs_f64(),
    });
    fs::create_dir(&config.output_directory)?;
    let native_length = native.object().byte_length();
    let sampled_length = sampled.object().byte_length();
    write_object(
        &config.output_directory.join("native.mkv"),
        &mut native,
        native_length,
    )?;
    write_object(
        &config.output_directory.join("sampled.mkv"),
        &mut sampled,
        sampled_length,
    )?;
    let report_bytes = serde_json::to_vec_pretty(&report)?;
    write_object(
        &config.output_directory.join("report.json"),
        &mut report_bytes.as_slice(),
        u64::try_from(report_bytes.len())?,
    )?;
    println!("{}", String::from_utf8(report_bytes)?);
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn main() {
    eprintln!("extension media qualification requires a supported Unix host");
    std::process::exit(1);
}
