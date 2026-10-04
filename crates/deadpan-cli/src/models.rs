//! Headless model-pack management: list, install with a smoke test, remove.
//!
//! Installation shows the pack's size and license before downloading, stages
//! and verifies every file, and activates only after the runtime successfully
//! loads the staged model. Interrupted installs resume on the next attempt.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_jobs::Sha256;
use deadpan_jobs::transcription::{Language, ModelInput};
use deadpan_models::packs::{
    HttpsTransport, Operation, PackManifest, PackStore, StagedPack, approved_packs, available_space,
};

use crate::CliError;
use crate::transcription::{AnalysisInput, TranscriptionRuntime, transcribe};

/// `~/Library/Application Support/Deadpan/Models`, shared by every project.
pub fn default_root() -> Result<PathBuf, CliError> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| CliError::Usage("HOME is not set; pass --root".into()))?;
    Ok(PathBuf::from(home).join("Library/Application Support/Deadpan/Models"))
}

fn usage() -> CliError {
    CliError::Usage(
        "usage: models list [--root <dir>] | models install <pack> [--root <dir>] | models remove <pack> [--root <dir>]"
            .into(),
    )
}

fn pack(id: &str) -> Result<PackManifest, CliError> {
    approved_packs()
        .into_iter()
        .find(|pack| pack.pack_id == id)
        .ok_or_else(|| CliError::Usage(format!("no approved model pack named {id}")))
}

fn models_error(error: deadpan_models::packs::PackError) -> CliError {
    CliError::Usage(error.to_string())
}

pub fn run(arguments: &[&str]) -> Result<(), CliError> {
    let (command, rest) = arguments.split_first().ok_or_else(usage)?;
    let (target, options) = match *command {
        "list" => (None, rest),
        "install" | "remove" => {
            let (target, options) = rest.split_first().ok_or_else(usage)?;
            (Some(*target), options)
        }
        _ => return Err(usage()),
    };
    let root = match options {
        [] => default_root()?,
        ["--root", root] => PathBuf::from(root),
        _ => return Err(usage()),
    };
    let store = PackStore::new(root.clone());
    match (*command, target) {
        ("list", None) => {
            let packs = approved_packs()
                .into_iter()
                .map(|manifest| {
                    let installed = store
                        .installed(&manifest)
                        .map_err(models_error)?
                        .map(|pack| pack.directory);
                    Ok(serde_json::json!({
                        "pack_id": manifest.pack_id,
                        "pack_version": manifest.pack_version,
                        "title": manifest.title,
                        "bytes": manifest.total_bytes(),
                        "license": manifest.license,
                        "operations": manifest.operations,
                        "installed": installed,
                    }))
                })
                .collect::<Result<Vec<_>, CliError>>()?;
            crate::write_json(&serde_json::json!({ "protocol": 1, "root": root, "packs": packs }))
        }
        ("install", Some(id)) => install(&store, &pack(id)?),
        ("remove", Some(id)) => {
            let manifest = pack(id)?;
            store.remove(&manifest).map_err(models_error)?;
            crate::write_json(&serde_json::json!({ "protocol": 1, "removed": manifest.pack_id }))
        }
        _ => Err(usage()),
    }
}

fn install(store: &PackStore, manifest: &PackManifest) -> Result<(), CliError> {
    if let Some(installed) = store.installed(manifest).map_err(models_error)? {
        return crate::write_json(&serde_json::json!({
            "protocol": 1, "installed": installed.directory, "already_installed": true
        }));
    }
    // The size and license are shown before any byte is downloaded.
    emit(&serde_json::json!({
            "event": "installing", "pack_id": manifest.pack_id, "bytes": manifest.total_bytes(),
            "license": manifest.license.id, "attribution": manifest.license.attribution,
    }))?;
    let cancelled = AtomicBool::new(false);
    let mut reported = 0_u64;
    let mut output_error = None;
    let staged = store
        .stage(
            manifest,
            &HttpsTransport::default(),
            available_space,
            &cancelled,
            |progress| {
                // One progress line per 5% keeps output bounded.
                let step = (progress.total_bytes / 20).max(1);
                if progress.completed_bytes >= reported + step
                    || progress.completed_bytes == progress.total_bytes
                {
                    reported = progress.completed_bytes;
                    if let Err(error) = emit(&serde_json::json!({
                        "event": "progress",
                        "completed_bytes": progress.completed_bytes,
                        "total_bytes": progress.total_bytes,
                    })) {
                        output_error.get_or_insert(error);
                    }
                }
            },
        )
        .map_err(models_error)?;
    if let Some(error) = output_error {
        return Err(error);
    }
    emit(&serde_json::json!({ "event": "smoke_test" }))?;
    if let Err(error) = smoke_test(&staged) {
        store.discard(staged).map_err(models_error)?;
        return Err(error);
    }
    let installed = store.activate(staged).map_err(models_error)?;
    crate::write_json(&serde_json::json!({ "protocol": 1, "installed": installed.directory }))
}

/// One complete JSON object per stdout line, as `render` reports progress.
fn emit(value: &serde_json::Value) -> Result<(), CliError> {
    use std::io::Write;
    let mut output = std::io::stdout().lock();
    writeln!(output, "{value}")?;
    output.flush()?;
    Ok(())
}

/// Load the staged model in the real runtime and recognize one second of
/// silence. A pack that cannot do this never replaces a known-good one.
fn smoke_test(staged: &StagedPack) -> Result<(), CliError> {
    let manifest = staged.manifest();
    if !manifest.operations.contains(&Operation::Transcribe) {
        return Ok(());
    }
    let file = &manifest.files[0];
    let path = staged
        .file(&file.name)
        .ok_or_else(|| CliError::Usage("staged pack lacks its model file".into()))?;
    let model = ModelInput {
        path: std::fs::canonicalize(&path)?,
        sha256: Sha256::new(file.sha256.clone()).map_err(|e| CliError::Usage(e.to_string()))?,
        byte_length: file.bytes,
    };
    transcribe(
        &TranscriptionRuntime::beside_current_executable()?,
        &model,
        &AnalysisInput {
            samples: vec![0.0; 16_000],
            origin: 0,
            source_rate: 16_000,
        },
        Language::Automatic,
        "pack-smoke",
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(300),
        |_| {},
    )?;
    Ok(())
}

/// The installed model file for transcription, if its pack is active.
pub fn installed_transcription_model(root: &Path) -> Result<Option<ModelInput>, CliError> {
    let store = PackStore::new(root.to_path_buf());
    for manifest in approved_packs() {
        if !manifest.operations.contains(&Operation::Transcribe) {
            continue;
        }
        if let Some(installed) = store.installed(&manifest).map_err(models_error)? {
            let file = &manifest.files[0];
            let path = installed
                .file(&file.name)
                .ok_or_else(|| CliError::Usage("installed pack lacks its model file".into()))?;
            return Ok(Some(ModelInput {
                path,
                sha256: Sha256::new(file.sha256.clone())
                    .map_err(|e| CliError::Usage(e.to_string()))?,
                byte_length: file.bytes,
            }));
        }
    }
    Ok(None)
}
