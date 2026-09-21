//! Developer harness for a completed, reaped model workspace. Does not select
//! candidates or change a project. All output paths must be new.

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

    use deadpan_jobs::artifact::ArtifactWorkspace;
    use deadpan_jobs::{HostMessage, NativeCandidateManifest};
    use deadpan_models::{QualificationLimits, SelectedBridgeProvider, qualify_bridge};
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Configuration {
        codec: PathBuf,
        workspace: PathBuf,
        request: HostMessage,
        candidate: NativeCandidateManifest,
        selected_provider: SelectedBridgeProvider,
        output_directory: PathBuf,
        limits: QualificationLimits,
    }

    fn write_media(path: &Path, source: &mut impl Read, length: u64) -> std::io::Result<()> {
        let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
        let actual = std::io::copy(source, &mut file)?;
        if actual != length {
            return Err(std::io::Error::other("qualified object length changed"));
        }
        file.flush()?;
        file.sync_all()
    }

    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 1 {
        return Err("expected one qualification configuration path".into());
    }
    let mut bytes = Vec::new();
    File::open(&arguments[0])?
        .take(1_048_577)
        .read_to_end(&mut bytes)?;
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
    // This standalone harness runs only after the generation supervisor has
    // reported clean teardown; live app code must pin before worker launch.
    let workspace = ArtifactWorkspace::open(&config.workspace)?;
    let started = std::time::Instant::now();
    let bundle = qualify_bridge(
        &config.codec,
        &workspace,
        &config.request,
        &config.candidate,
        &config.selected_provider,
        config.limits,
        &AtomicBool::new(false),
    )?;
    let report = json!({
        "scope": "complete media/provenance qualification; no selected-Ready persistence or acceptance",
        "binding": bundle.binding(), "declaration": bundle.declaration(),
        "native": {"object":bundle.native().object(), "report":bundle.native().report()},
        "sampled": {"object":bundle.sampled().object(), "report":bundle.sampled().report()},
        "provenance": bundle.provenance().object(),
        "elapsed_seconds": started.elapsed().as_secs_f64(),
    });
    fs::create_dir(&config.output_directory)?;
    let native_length = bundle.native().object().byte_length();
    let sampled_length = bundle.sampled().object().byte_length();
    let provenance_length = bundle.provenance().object().byte_length();
    let (mut native, mut sampled, mut provenance) = bundle.into_parts();
    write_media(
        &config.output_directory.join("native.mkv"),
        &mut native,
        native_length,
    )?;
    write_media(
        &config.output_directory.join("sampled.mkv"),
        &mut sampled,
        sampled_length,
    )?;
    write_media(
        &config.output_directory.join("provenance.json"),
        &mut provenance,
        provenance_length,
    )?;
    let report_bytes = serde_json::to_vec_pretty(&report)?;
    write_media(
        &config.output_directory.join("report.json"),
        &mut report_bytes.as_slice(),
        report_bytes.len() as u64,
    )?;
    println!("{}", String::from_utf8(report_bytes)?);
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn main() {
    eprintln!("bridge qualification requires a supported Unix host");
    std::process::exit(1);
}
