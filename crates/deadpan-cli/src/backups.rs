//! `project backups`, `project backup`, `project restore` and `project view`.
//! See docs/BACKUPS.md.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use deadpan_core::RevisionId;
use deadpan_store::backups::{
    BackupLimits, BackupPolicy, BackupReason, RestoreOutcome, create_backup, list_backups,
    preview_backup, verify_backup,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};

use crate::live_project::{LiveError, ShortOperation};
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

/// `project restore <package> <backup-id> [--expected <revision>] [--dry-run]`:
/// replace the project's database with a verified backup after backing up
/// the current state. While the app has the project open, the restore runs
/// on its writer through the live endpoint, which then serves the restored
/// state from a new owner.
pub fn run_restore(arguments: &[&str]) -> Result<(), CliError> {
    let usage = || {
        CliError::Usage(
            "usage: project restore <project.deadpan> <backup-id> [--expected <revision>] [--dry-run] | project restore <project.deadpan> <backup-id> --damaged [--force-project <id>]"
                .into(),
        )
    };
    let (path, id, rest) = match arguments {
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
        [path, id, rest @ ..] => (*path, *id, rest),
        _ => return Err(usage()),
    };
    let (expected, dry_run) = match rest {
        [] => (None, false),
        ["--dry-run"] => (None, true),
        ["--expected", revision] => (Some(RevisionId::new(*revision)?), false),
        ["--expected", revision, "--dry-run"] | ["--dry-run", "--expected", revision] => {
            (Some(RevisionId::new(*revision)?), true)
        }
        _ => return Err(usage()),
    };
    if dry_run {
        // Read-only: works beside an open app and writes nothing.
        let store = ProjectStore::open(Path::new(path), AccessMode::ReadOnly)?;
        let current = store.head_revision()?;
        check_expected(&current, expected.as_ref())?;
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
            "current_revision": current,
        }));
    }
    write_json(&crate::live_project::dispatch_short(
        Path::new(path),
        None,
        ShortOperation::RestoreBackup {
            id: id.to_owned(),
            expected_revision: expected,
        },
    )?)
}

fn check_expected(current: &RevisionId, expected: Option<&RevisionId>) -> Result<(), LiveError> {
    match expected {
        Some(expected) if expected != current => {
            Err(LiveError::store(StoreError::RevisionConflict {
                expected: expected.as_str().into(),
                current: current.as_str().into(),
            }))
        }
        _ => Ok(()),
    }
}

/// The restore itself, on whichever process holds the writer.
pub(crate) fn restore_on(
    store: &mut ProjectStore,
    id: &str,
    expected: Option<&RevisionId>,
) -> Result<RestoreOutcome, LiveError> {
    check_expected(&store.head_revision().map_err(LiveError::store)?, expected)?;
    store
        .restore_backup(
            id,
            &BackupPolicy::default(),
            BackupLimits::default(),
            &AtomicBool::new(false),
        )
        .map_err(|error| LiveError::new(error.code(), &error))
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
