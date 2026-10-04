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
    HttpsTransport, InstallProgress, InstalledPack, PackFile, PackManifest, PackStore, StagedPack,
    approved_packs, available_space,
};

use crate::CliError;
use crate::transcription::{AnalysisInput, TranscriptionRuntime, transcribe};

/// The per-user model directory shared by every project:
/// `~/Library/Application Support/Deadpan/Models` on macOS and
/// `$XDG_DATA_HOME/deadpan/models` (default `~/.local/share`) on Linux.
pub fn default_root() -> Result<PathBuf, CliError> {
    let home = || {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| CliError::Usage("HOME is not set; pass --root".into()))
    };
    if cfg!(target_os = "macos") {
        return Ok(home()?.join("Library/Application Support/Deadpan/Models"));
    }
    let data = match std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        Some(data) if data.is_absolute() => data,
        _ => home()?.join(".local/share"),
    };
    Ok(data.join("deadpan/models"))
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
                    let installed = store.installed(&manifest)?.map(|pack| pack.directory);
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
            store.remove(&manifest)?;
            crate::write_json(&serde_json::json!({ "protocol": 1, "removed": manifest.pack_id }))
        }
        _ => Err(usage()),
    }
}

fn install(store: &PackStore, manifest: &PackManifest) -> Result<(), CliError> {
    if let Some(installed) = store.installed(manifest)? {
        return crate::write_json(&serde_json::json!({
            "protocol": 1, "installed": installed.directory, "already_installed": true
        }));
    }
    // The size and license are shown before any byte is downloaded.
    emit(&serde_json::json!({
        "event": "installing", "pack_id": manifest.pack_id, "bytes": manifest.total_bytes(),
        "license": manifest.license.id, "attribution": manifest.license.attribution,
    }))?;
    let mut reported = 0_u64;
    let mut output_error = None;
    let installed = install_pack(store, manifest, &AtomicBool::new(false), |progress| {
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
    })?;
    if let Some(error) = output_error {
        return Err(error);
    }
    crate::write_json(&serde_json::json!({ "protocol": 1, "installed": installed.directory }))
}

/// Download, verify, smoke-test and activate one approved pack. A failed
/// smoke test leaves installed versions intact and keeps the hash-verified
/// staged copy, so a retry after fixing the cause repeats only the test.
pub fn install_pack(
    store: &PackStore,
    manifest: &PackManifest,
    cancelled: &AtomicBool,
    progress: impl FnMut(InstallProgress),
) -> Result<InstalledPack, CliError> {
    let staged = store.stage(
        manifest,
        &HttpsTransport::default(),
        available_space,
        cancelled,
        progress,
    )?;
    smoke_test(&staged, cancelled)?;
    Ok(store.activate(staged)?)
}

/// One complete JSON object per stdout line, as `render` reports progress.
fn emit(value: &serde_json::Value) -> Result<(), CliError> {
    use std::io::Write;
    let mut output = std::io::stdout().lock();
    writeln!(output, "{value}")?;
    output.flush()?;
    Ok(())
}

/// Load the staged models in the real runtime: recognize one second of
/// silence and detect speech in it. A pack that cannot do this never replaces
/// a known-good one.
fn smoke_test(staged: &StagedPack, cancelled: &AtomicBool) -> Result<(), CliError> {
    let manifest = staged.manifest();
    let silence = AnalysisInput {
        samples: vec![0.0; 16_000],
        origin: 0,
        source_rate: 16_000,
    };
    if let Some(file) = manifest.transcription_file() {
        let model = model_input(file, staged.file(&file.name))?;
        transcribe(
            &TranscriptionRuntime::beside_current_executable()?,
            &model,
            &silence,
            Language::Automatic,
            "pack-smoke",
            cancelled,
            Instant::now() + Duration::from_secs(300),
            |_| {},
        )?;
    }
    if let Some(file) = manifest.speech_activity_file() {
        let model = model_input(file, staged.file(&file.name))?;
        crate::activity::detect_speech(
            &TranscriptionRuntime::beside_current_executable()?,
            &model,
            &silence,
            "pack-smoke-activity",
            cancelled,
            Instant::now() + Duration::from_secs(60),
        )?;
    }
    Ok(())
}

fn model_input(file: &PackFile, path: Option<PathBuf>) -> Result<ModelInput, CliError> {
    let path = path.ok_or_else(|| CliError::Usage(format!("pack lacks {}", file.name)))?;
    Ok(ModelInput {
        path: std::fs::canonicalize(&path)?,
        sha256: Sha256::new(file.sha256.clone()).map_err(|e| CliError::Usage(e.to_string()))?,
        byte_length: file.bytes,
    })
}

/// The installed model file for transcription, if its pack is active.
pub fn installed_transcription_model(root: &Path) -> Result<Option<ModelInput>, CliError> {
    installed_model(root, PackManifest::transcription_file)
}

/// The installed Silero model for speech detection, if its pack is active.
pub fn installed_speech_activity_model(root: &Path) -> Result<Option<ModelInput>, CliError> {
    installed_model(root, PackManifest::speech_activity_file)
}

fn installed_model(
    root: &Path,
    select: fn(&PackManifest) -> Option<&PackFile>,
) -> Result<Option<ModelInput>, CliError> {
    let store = PackStore::new(root.to_path_buf());
    for manifest in approved_packs() {
        let Some(file) = select(&manifest) else {
            continue;
        };
        if let Some(installed) = store.installed(&manifest)? {
            return model_input(file, installed.file(&file.name)).map(Some);
        }
    }
    Ok(None)
}
