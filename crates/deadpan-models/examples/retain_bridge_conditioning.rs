//! Developer pre-launch conditioning capture. No runtime, model, or codec loads.
//! Outputs are diagnostic files in a new directory, not an atomic project write.

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;

    use deadpan_jobs::artifact::ArtifactWorkspace;
    use deadpan_jobs::{HostMessage, WorkspaceArtifact, WorkspaceRef};
    use deadpan_models::{ConditioningLimits, capture_bridge_conditioning};
    use serde::Deserialize;
    use serde_json::json;

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

    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 1 {
        return Err("expected one conditioning capture configuration path".into());
    }
    let mut bytes = Vec::new();
    File::open(&arguments[0])?
        .take(1_048_577)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 1_048_576 {
        return Err("configuration exceeds 1 MiB".into());
    }
    let config: Configuration = serde_json::from_slice(&bytes)?;
    if !config.workspace.is_absolute() || !config.output_directory.is_absolute() {
        return Err("host paths must be absolute".into());
    }
    let workspace = ArtifactWorkspace::open(&config.workspace)?;
    let conditioning = capture_bridge_conditioning(
        &workspace,
        &config.request,
        &config.manifest,
        &config.input_scope,
        config.limits,
        &AtomicBool::new(false),
    )?;
    let report = json!({
        "scope": "pre-launch prepared input byte retention; no image decode, source-clock or color qualification",
        "conditioning": conditioning.receipt(),
    });
    fs::create_dir(&config.output_directory)?;
    let (manifest, left, right) = conditioning.into_parts();
    let mut written = std::collections::BTreeSet::new();
    for mut input in [manifest, left, right] {
        if !written.insert(input.declaration().reference().clone()) {
            continue;
        }
        let path = config
            .output_directory
            .join(input.declaration().reference().as_str());
        fs::create_dir_all(path.parent().ok_or("input path lacks a parent")?)?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let actual = std::io::copy(&mut input, &mut file)?;
        if actual != input.object().byte_length() {
            return Err("retained input length changed".into());
        }
        file.flush()?;
        file.sync_all()?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(config.output_directory.join("retention-report.json"))?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.write_all(b"\n")?;
    file.flush()?;
    file.sync_all()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn main() {
    eprintln!("conditioning retention requires a supported Unix host");
    std::process::exit(1);
}
