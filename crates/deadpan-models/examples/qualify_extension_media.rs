//! Developer qualification of a completed, reaped extension worker's bundle.
//! Includes provenance and all output checks against retained inputs.
//! It does not admit Ready or edit a project.

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Seek, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    use deadpan_jobs::artifact::ArtifactWorkspace;
    use deadpan_jobs::{CandidateDeclaration, HostMessage, NativeCandidateManifest};
    use deadpan_models::{
        ConditioningLimits, ExtensionConditioningReceipt, ExtensionQualification,
        QualificationLimits, SelectedExtensionProvider, capture_extension_conditioning,
        qualify_extension,
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
        selected_provider: SelectedExtensionProvider,
        output_directory: PathBuf,
        limits: QualificationLimits,
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
    config.limits.media.validate()?;
    let started = Instant::now();
    let deadline = started + Duration::from_millis(config.limits.media.timeout_ms);
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
    let conditioning = capture_extension_conditioning(
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
    let workspace = ArtifactWorkspace::open(&config.workspace)?;
    let bundle = qualify_extension(
        &config.codec,
        &config.tracker,
        &workspace,
        ExtensionQualification {
            request: &config.request,
            declaration: &CandidateDeclaration::NativeExtensionV3(config.candidate),
            selected_provider: &config.selected_provider,
            conditioning,
        },
        QualificationLimits {
            media: deadpan_media::protocol::ConversionLimits {
                timeout_ms: remaining_ms()?,
                ..config.limits.media
            },
            ..config.limits
        },
        &cancelled,
    )?;
    let report = json!({
        "scope": "complete extension media, provenance and rejection checks; no selected-Ready persistence or acceptance",
        "binding": bundle.binding(), "declaration": bundle.declaration(),
        "native": {"object": bundle.native().object(), "report": bundle.native().report()},
        "sampled": {"object": bundle.sampled().object(), "report": bundle.sampled().report()},
        "provenance": bundle.provenance().object(),
        "conditioning": bundle.conditioning().receipt(),
        "native_span": bundle.native_span(), "sampled_span": bundle.sampled_span(),
        "elapsed_seconds": started.elapsed().as_secs_f64(),
    });
    fs::create_dir(&config.output_directory)?;
    let provenance_length = bundle.provenance().object().byte_length();
    let (mut native, mut sampled, mut provenance, conditioning) = bundle.into_parts();
    let (manifest, frames, opposite, signatures) = conditioning.into_parts();
    let retained_directory = config.output_directory.join("conditioning");
    fs::create_dir(&retained_directory)?;
    let mut written = std::collections::BTreeSet::new();
    for mut input in std::iter::once(manifest)
        .chain(frames)
        .chain(opposite)
        .chain(std::iter::once(signatures))
    {
        if !written.insert(input.declaration().reference().clone()) {
            continue;
        }
        let path = retained_directory.join(input.declaration().reference().as_str());
        fs::create_dir_all(path.parent().ok_or("input path lacks a parent")?)?;
        let length = input.object().byte_length();
        input.rewind()?;
        write_object(&path, &mut input, length)?;
    }
    let native_length = native.object().byte_length();
    let sampled_length = sampled.object().byte_length();
    write_object(
        &config.output_directory.join("provenance.json"),
        &mut provenance,
        provenance_length,
    )?;
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
