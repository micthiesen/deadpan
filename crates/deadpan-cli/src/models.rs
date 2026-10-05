//! Headless model-pack management: list, show licenses, install (download or
//! offline import) with a smoke test, export, remove.
//!
//! Installation shows the pack's size and licenses before any byte is staged.
//! A license that requires acceptance must be accepted explicitly
//! (`--accept-license`, after `models license <pack>`); nothing downloads
//! silently. Every file is staged and verified, and the pack activates only
//! after the runtime successfully loads the staged copy. Interrupted
//! downloads resume on the next attempt.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_jobs::Sha256;
use deadpan_jobs::transcription::{Language, ModelInput};
use deadpan_models::packs::{
    HttpsTransport, ImportSource, InstallProgress, InstalledPack, Operation, PackFile,
    PackManifest, PackState, PackStore, StagedPack, approved_packs, available_space, license_text,
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

const USAGE: &str = "usage: models list | models license <pack> | models install <pack> [--accept-license] | models import <pack> <folder-or-archive.tar> [--accept-license] | models export <pack> <archive.tar> | models remove <pack> [--partial]; every command takes [--root <dir>]";

fn usage() -> CliError {
    CliError::Usage(USAGE.into())
}

fn pack(id: &str) -> Result<PackManifest, CliError> {
    approved_packs()
        .into_iter()
        .find(|pack| pack.pack_id == id)
        .ok_or_else(|| CliError::Usage(format!("no approved model pack named {id}")))
}

struct Options<'a> {
    positional: Vec<&'a str>,
    root: Option<PathBuf>,
    accept_license: bool,
    partial: bool,
}

fn options<'a>(arguments: &[&'a str]) -> Result<Options<'a>, CliError> {
    let mut parsed = Options {
        positional: Vec::new(),
        root: None,
        accept_license: false,
        partial: false,
    };
    let mut rest = arguments;
    while let Some((first, tail)) = rest.split_first() {
        match *first {
            "--root" => {
                let (root, tail) = tail.split_first().ok_or_else(usage)?;
                parsed.root = Some(PathBuf::from(root));
                rest = tail;
                continue;
            }
            "--accept-license" => parsed.accept_license = true,
            "--partial" => parsed.partial = true,
            flag if flag.starts_with("--") => return Err(usage()),
            value => parsed.positional.push(value),
        }
        rest = tail;
    }
    Ok(parsed)
}

pub fn run(arguments: &[&str]) -> Result<(), CliError> {
    let parsed = options(arguments)?;
    let root = match parsed.root.clone() {
        Some(root) => root,
        None => default_root()?,
    };
    let store = PackStore::new(root.clone());
    match parsed.positional.as_slice() {
        ["list"] => list(&store),
        ["license", id] => license(&pack(id)?),
        ["install", id] => install(&store, &pack(id)?, None, parsed.accept_license),
        ["import", id, source] => {
            let source = ImportSource::at(Path::new(source))?;
            install(&store, &pack(id)?, Some(source), parsed.accept_license)
        }
        ["export", id, archive] => {
            let manifest = pack(id)?;
            store.export(
                &manifest,
                Path::new(archive),
                &AtomicBool::new(false),
                progress_lines(),
            )?;
            crate::write_json(&serde_json::json!({
                "protocol": 1, "exported": manifest.pack_id, "archive": archive,
                "bytes": manifest.total_bytes(),
            }))
        }
        ["remove", id] => {
            let manifest = pack(id)?;
            if parsed.partial {
                store.discard_partial(&manifest)?;
            } else {
                store.remove(&manifest)?;
            }
            crate::write_json(&serde_json::json!({
                "protocol": 1, "removed": manifest.pack_id, "partial": parsed.partial,
            }))
        }
        _ => Err(usage()),
    }
}

fn license_json(manifest: &PackManifest) -> Vec<serde_json::Value> {
    manifest
        .licenses
        .iter()
        .map(|license| {
            serde_json::json!({
                "id": license.id, "title": license.title, "spdx": license.spdx,
                "attribution": license.attribution, "url": license.url,
                "terms": license.terms, "redistribution": license.redistribution,
                "access": license.access, "acceptance_required": license.acceptance_required,
                "bytes": manifest.license_bytes(license),
            })
        })
        .collect()
}

fn list(store: &PackStore) -> Result<(), CliError> {
    let free = std::fs::create_dir_all(store.root())
        .ok()
        .and_then(|()| available_space(store.root()).ok());
    let packs = approved_packs()
        .into_iter()
        .map(|manifest| {
            let state = store.state(&manifest)?;
            Ok(serde_json::json!({
                "pack_id": manifest.pack_id,
                "pack_version": manifest.pack_version,
                "title": manifest.title,
                "bytes": manifest.total_bytes(),
                "files": manifest.files.len(),
                "operations": manifest.operations,
                "licenses": license_json(&manifest),
                "memory_bytes": manifest.memory_bytes,
                "installed": match &state {
                    PackState::Installed(pack) => Some(&pack.directory),
                    _ => None,
                },
                "staged_bytes": match state {
                    PackState::Partial { bytes } => bytes,
                    _ => 0,
                },
                "remaining_bytes": store.remaining_bytes(&manifest),
            }))
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    crate::write_json(&serde_json::json!({
        "protocol": 1, "root": store.root(), "free_bytes": free, "packs": packs,
    }))
}

/// Print every license of a pack: summary, link and the full compiled text.
fn license(manifest: &PackManifest) -> Result<(), CliError> {
    use std::io::Write;
    let mut output = std::io::stdout().lock();
    for license in &manifest.licenses {
        writeln!(
            output,
            "{}\n{}\n\n{}\n\nAttribution: {}\nAccess: {}\nFull text: {}\n",
            license.title,
            "=".repeat(license.title.chars().count()),
            license.terms,
            license.attribution,
            license.access,
            license.url
        )?;
        if let Some(text) = license.text.as_deref().and_then(license_text) {
            writeln!(output, "{text}")?;
        }
    }
    output.flush()?;
    Ok(())
}

/// JSON progress lines, one per 5% of the pack.
fn progress_lines() -> impl FnMut(InstallProgress) {
    let mut reported = 0_u64;
    move |progress: InstallProgress| {
        let step = (progress.total_bytes / 20).max(1);
        if progress.completed_bytes >= reported + step
            || progress.completed_bytes == progress.total_bytes
        {
            reported = progress.completed_bytes;
            let _ = emit(&serde_json::json!({
                "event": "progress",
                "completed_bytes": progress.completed_bytes,
                "total_bytes": progress.total_bytes,
            }));
        }
    }
}

fn install(
    store: &PackStore,
    manifest: &PackManifest,
    source: Option<ImportSource>,
    accept_license: bool,
) -> Result<(), CliError> {
    if let Some(installed) = store.installed(manifest)? {
        return crate::write_json(&serde_json::json!({
            "protocol": 1, "installed": installed.directory, "already_installed": true
        }));
    }
    // The size and licenses are shown before any byte is staged.
    emit(&serde_json::json!({
        "event": "installing", "pack_id": manifest.pack_id, "bytes": manifest.total_bytes(),
        "remaining_bytes": store.remaining_bytes(manifest),
        "source": source.as_ref().map_or("download", |_| "import"),
        "licenses": license_json(manifest),
    }))?;
    let accepted = if accept_license {
        manifest.license_ids()
    } else {
        Vec::new()
    };
    // Refuses with ModelPackLicense; `models license <pack>` shows the texts.
    manifest.check_acceptance(&accepted)?;
    let cancelled = AtomicBool::new(false);
    let installed = install_pack(
        store,
        manifest,
        &accepted,
        source.as_ref(),
        &cancelled,
        progress_lines(),
        |event| {
            let _ = emit(&serde_json::json!({ "event": event }));
        },
    )?;
    crate::write_json(&serde_json::json!({ "protocol": 1, "installed": installed.directory }))
}

/// Download (or import offline), verify, smoke-test and activate one
/// approved pack. `accepted_licenses` must name every license that requires
/// acceptance; the user has seen them. A failed smoke test leaves installed
/// versions intact and keeps the hash-verified staged copy, so a retry after
/// fixing the cause repeats only the test.
pub fn install_pack(
    store: &PackStore,
    manifest: &PackManifest,
    accepted_licenses: &[String],
    source: Option<&ImportSource>,
    cancelled: &AtomicBool,
    progress: impl FnMut(InstallProgress),
    mut phase: impl FnMut(&'static str),
) -> Result<InstalledPack, CliError> {
    let staged = match source {
        None => store.stage(
            manifest,
            accepted_licenses,
            &HttpsTransport::default(),
            available_space,
            cancelled,
            progress,
        )?,
        Some(source) => store.import(
            manifest,
            accepted_licenses,
            source,
            available_space,
            cancelled,
            progress,
        )?,
    };
    phase("smoke_test");
    smoke_test(&staged, cancelled)?;
    phase("activating");
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

/// Load the staged models in their real runtime. A pack that cannot do this
/// never replaces a known-good one.
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
    if manifest.supports(Operation::BridgeHold) {
        // The bundled (or development) runtime imports its pinned sources,
        // runs Metal and reads every staged model header; no inference.
        let runtime =
            crate::generation::runtime::BridgeRuntime::with_model_data(staged.directory())
                .map_err(|error| CliError::Usage(error.to_string()))?;
        let report = runtime.check(cancelled).map_err(CliError::Usage)?;
        let _ = emit(&serde_json::json!({ "event": "smoke_test_passed", "report": report }));
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
