//! Developer verification of a completed, reaped extension worker's media.
//! Does not admit a Ready candidate, validate visual quality, or edit a project.

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

    use deadpan_jobs::artifact::{ArtifactLimits, ArtifactWorkspace};
    use deadpan_jobs::{HostMessage, NativeCandidateManifest};
    use deadpan_media::protocol::{
        ConversionLimits, EXTENSION_PROTOCOL_VERSION, ExtensionConversionRequest,
        ExtensionOperation, VideoContract,
    };
    use deadpan_media::{InputIdentity, canonicalize_extension};
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Configuration {
        codec: PathBuf,
        workspace: PathBuf,
        request: HostMessage,
        candidate: NativeCandidateManifest,
        output_directory: PathBuf,
        limits: ConversionLimits,
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
    if [&config.codec, &config.workspace, &config.output_directory]
        .iter()
        .any(|path| !path.is_absolute())
    {
        return Err("host paths must be absolute".into());
    }
    config.request.validate()?;
    config.candidate.validate()?;
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
    let mut input = workspace.snapshot(
        output_workspace,
        &config.candidate.native,
        ArtifactLimits::new(config.limits.max_input_bytes)?,
    )?;
    let mut digest = [0_u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(
            &config.candidate.native.sha256().as_str()[2 * index..2 * index + 2],
            16,
        )?;
    }
    let started = std::time::Instant::now();
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
            limits: config.limits,
        },
        &AtomicBool::new(false),
    )?;
    let report = json!({
        "scope": "decoded native and generated-only sampled media; no quality/provenance admission, Ready state or acceptance",
        "native": {"object": canonical.native().object(), "report": canonical.native().report()},
        "sampled": {"object": canonical.sampled().object(), "report": canonical.sampled().report()},
        "sampling": canonical.sampling(),
        "elapsed_seconds": started.elapsed().as_secs_f64(),
    });
    fs::create_dir(&config.output_directory)?;
    let (mut native, mut sampled, _) = canonical.into_parts();
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
