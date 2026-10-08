//! Pre-launch immutable input retention for extension worker qualification.
//! Source-clock truth and decoded image qualification belong to preparation.

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;

    use deadpan_jobs::artifact::ArtifactWorkspace;
    use deadpan_jobs::{HostMessage, WorkspaceArtifact, WorkspaceRef};
    use deadpan_models::{ConditioningLimits, capture_extension_conditioning};
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Configuration {
        workspace: PathBuf,
        request: HostMessage,
        input_scope: WorkspaceRef,
        manifest: WorkspaceArtifact,
        limits: ConditioningLimits,
        output_directory: PathBuf,
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
    if !config.workspace.is_absolute() || !config.output_directory.is_absolute() {
        return Err("host paths must be absolute".into());
    }
    let workspace = ArtifactWorkspace::open(&config.workspace)?;
    let retained = capture_extension_conditioning(
        &workspace,
        &config.request,
        &config.manifest,
        &config.input_scope,
        config.limits,
        &AtomicBool::new(false),
    )?;
    let report = serde_json::json!({
        "scope": "pre-launch input byte retention; no decoded image or source-clock qualification",
        "conditioning": retained.receipt(),
    });
    fs::create_dir(&config.output_directory)?;
    let (manifest, frames, opposite) = retained.into_parts();
    let mut written = std::collections::BTreeSet::new();
    for mut object in std::iter::once(manifest).chain(frames).chain(opposite) {
        if !written.insert(object.declaration().reference().clone()) {
            continue;
        }
        let path = config
            .output_directory
            .join(object.declaration().reference().as_str());
        fs::create_dir_all(path.parent().ok_or("input path lacks a parent")?)?;
        let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
        if std::io::copy(&mut object, &mut file)? != object.object().byte_length() {
            return Err("retained object length changed".into());
        }
        file.flush()?;
        file.sync_all()?;
    }
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(config.output_directory.join("retention-report.json"))?;
    serde_json::to_writer_pretty(&mut output, &report)?;
    output.write_all(b"\n")?;
    output.flush()?;
    output.sync_all()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn main() {
    eprintln!("conditioning retention requires a supported Unix host");
    std::process::exit(1);
}
