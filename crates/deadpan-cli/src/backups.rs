//! `project backups`, `project backup`, `project restore` and `project view`.
//! See docs/BACKUPS.md.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use deadpan_store::backups::{
    BackupLimits, BackupPolicy, BackupReason, create_backup, list_backups, preview_backup,
    verify_backup,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};

use crate::{CliError, write_json};

/// `project backups <package> [--verify]`: every backup, newest first, with
/// what it contains. Read-only; works while the app has the project open.
pub fn run_list(arguments: &[&str]) -> Result<(), CliError> {
    let (path, verify) = match arguments {
        [path] => (*path, false),
        [path, "--verify"] => (*path, true),
        _ => {
            return Err(CliError::Usage(
                "usage: project backups <project.deadpan> [--verify]".into(),
            ));
        }
    };
    let mut backups = Vec::new();
    for info in list_backups(Path::new(path))? {
        let preview = if verify {
            verify_backup(&info)
        } else {
            preview_backup(&info)
        };
        backups.push(match preview {
            Ok(preview) => {
                serde_json::json!({ "backup": info, "contents": preview, "verified": verify })
            }
            Err(error) => serde_json::json!({
                "backup": info,
                "error": { "code": error.code(), "message": error.to_string() },
            }),
        });
    }
    write_json(&serde_json::json!({ "protocol": 1, "backups": backups }))
}

/// `project backup <package>`: one verified manual backup, then rotation.
/// Reads through its own connection, so it also works beside an open app.
pub fn run_create(arguments: &[&str]) -> Result<(), CliError> {
    let [path] = arguments else {
        return Err(CliError::Usage(
            "usage: project backup <project.deadpan>".into(),
        ));
    };
    let outcome = create_backup(
        Path::new(path),
        BackupReason::Manual,
        &BackupPolicy::default(),
        BackupLimits::default(),
        &AtomicBool::new(false),
    )?;
    write_json(&serde_json::json!({ "protocol": 1, "created": outcome }))
}

/// `project restore <package> <backup-id> [--dry-run]`: replace the project's
/// database with a verified backup after backing up the current state.
/// Needs the writer: refused while the app has the project open.
pub fn run_restore(arguments: &[&str]) -> Result<(), CliError> {
    let usage = || {
        CliError::Usage(
            "usage: project restore <project.deadpan> <backup-id> [--dry-run | --damaged [--force-project <id>]]"
                .into(),
        )
    };
    let (path, id, dry_run) = match arguments {
        [path, id] => (*path, *id, false),
        [path, id, "--dry-run"] => (*path, *id, true),
        [path, id, "--damaged", rest @ ..] => {
            // The database no longer opens: replace it without reading it.
            let force = match rest {
                [] => None,
                ["--force-project", project] => Some(deadpan_core::ProjectId::new(*project)?),
                _ => return Err(usage()),
            };
            let replaced = deadpan_store::backups::replace_damaged_database(
                Path::new(path),
                id,
                force.as_ref(),
            )?;
            return write_json(&serde_json::json!({ "protocol": 1, "replaced_damaged": replaced }));
        }
        _ => return Err(usage()),
    };
    if dry_run {
        // Read-only: works beside an open app and writes nothing.
        let store = ProjectStore::open(Path::new(path), AccessMode::ReadOnly)?;
        let info = store
            .backups()?
            .into_iter()
            .find(|backup| backup.id == id)
            .ok_or_else(|| deadpan_store::backups::BackupError::NotFound(id.into()))?;
        let preview = verify_backup(&info)?;
        if preview.project_id != *store.snapshot()?.project_id() {
            return Err(deadpan_store::backups::BackupError::OtherProject.into());
        }
        return write_json(&serde_json::json!({
            "protocol": 1,
            "dry_run": true,
            "would_restore": preview,
            "current_revision": store.head_revision()?,
        }));
    }
    let mut store = ProjectStore::open(Path::new(path), AccessMode::ReadWrite)?;
    let outcome = store.restore_backup(
        id,
        &BackupPolicy::default(),
        BackupLimits::default(),
        &AtomicBool::new(false),
    )?;
    write_json(&serde_json::json!({ "protocol": 1, "restored": outcome }))
}

/// `project view <package>`: a read-only summary that also opens packages a
/// newer Deadpan saved, saying so instead of refusing.
pub fn run_view(arguments: &[&str]) -> Result<(), CliError> {
    let [path] = arguments else {
        return Err(CliError::Usage(
            "usage: project view <project.deadpan>".into(),
        ));
    };
    let store = ProjectStore::open(Path::new(path), AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    let read_only = store.newer_schema().map(|found| {
        StoreError::NewerSchema {
            found,
            supported: deadpan_store::DATABASE_SCHEMA_VERSION,
        }
        .to_string()
    });
    write_json(&serde_json::json!({
        "protocol": 1,
        "project_id": document.project_id(),
        "revision_id": document.revision_id(),
        "duration_frames": document.duration()?.frames(),
        "node_count": document.nodes().len(),
        "presentation_basis": document.presentation_basis(),
        "schema": store.newer_schema().unwrap_or(deadpan_store::DATABASE_SCHEMA_VERSION),
        "newer_schema": store.newer_schema(),
        "read_only_reason": read_only,
        "backups": list_backups(Path::new(path)).map(|backups| backups.len()).unwrap_or(0),
    }))
}
