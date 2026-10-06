//! Deterministic crashes at the critical windows of backups, restores,
//! release migrations and damaged-database replacement
//! (docs/BACKUPS.md#process-kills). Each case re-executes this binary as a
//! child that aborts at one named failpoint (debug builds only), checks the
//! child really reached it, then checks the package afterwards. Random kills
//! (`chaos_kills.rs`) cover the time between these windows.
#![cfg(all(debug_assertions, any(target_os = "macos", target_os = "linux")))]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::AtomicBool;

use deadpan_core::*;
use deadpan_store::backups::{
    BackupLimits, BackupPolicy, BackupReason, create_backup, list_backups,
    replace_damaged_database, verify_backup,
};
use deadpan_store::{AccessMode, DATABASE_SCHEMA_VERSION, ProjectStore, StoreError};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

const OPERATION: &str = "DEADPAN_TEST_FAILPOINT_OPERATION";
const PACKAGE: &str = "DEADPAN_TEST_FAILPOINT_PACKAGE";
const ARGUMENT: &str = "DEADPAN_TEST_FAILPOINT_ARGUMENT";

fn document() -> Result<ProjectDocument> {
    Ok(ProjectDocument::new(
        ProjectId::new("failpoints")?,
        RevisionId::new("r0")?,
        PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?)
}

fn insert(store: &mut ProjectStore, revision: &str) -> Result<RevisionId> {
    let current = store.snapshot()?;
    let id = NodeId::new(format!("pause-{revision}"))?;
    Ok(store
        .commit(&CommandRequest {
            project_id: current.project_id().clone(),
            expected_revision: current.revision_id().clone(),
            new_revision: RevisionId::new(revision)?,
            command: Command::Insert {
                parent: current.root().clone(),
                index: 0,
                subtree: Subtree {
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                    root: id.clone(),
                    nodes: BTreeMap::from([(
                        id,
                        BeatNode::hold(
                            "Pause",
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(12)?,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                },
            },
        })?
        .revision_id)
}

fn backup(path: &Path, reason: BackupReason) -> Result<String> {
    Ok(create_backup(
        path,
        reason,
        &BackupPolicy::default(),
        BackupLimits::default(),
        &AtomicBool::new(false),
    )?
    .backup
    .id)
}

fn add_table(
    transaction: &rusqlite::Transaction<'_>,
) -> std::result::Result<(), deadpan_store::StoreError> {
    transaction.execute_batch("CREATE TABLE synthetic_release(id INTEGER PRIMARY KEY) STRICT;")?;
    Ok(())
}

fn accept(_: &rusqlite::Connection) -> std::result::Result<(), deadpan_store::StoreError> {
    Ok(())
}

/// What the child does until its failpoint aborts it.
fn child(operation: &str, package: &Path, argument: &str) -> Result {
    match operation {
        "backup" => {
            backup(package, BackupReason::Manual)?;
        }
        "restore" => {
            let mut store = ProjectStore::open(package, AccessMode::ReadWrite)?;
            store.restore_backup(
                argument,
                &BackupPolicy::default(),
                BackupLimits::default(),
                &AtomicBool::new(false),
            )?;
        }
        "migrate" => {
            let steps = [deadpan_store::migration::Migration {
                from: DATABASE_SCHEMA_VERSION,
                apply: add_table,
            }];
            deadpan_store::migration::migrate_package_with(
                package,
                &steps,
                DATABASE_SCHEMA_VERSION + 1,
                accept,
            )?;
        }
        "damaged" => {
            replace_damaged_database(package, argument, None)?;
        }
        other => return Err(format!("unknown operation {other}").into()),
    }
    Err("the failpoint was not reached".into())
}

/// Run `operation` in a child that aborts at `failpoint`; return whether the
/// failpoint was hit (the child must not finish normally).
fn crash_at(failpoint: &str, operation: &str, package: &Path, argument: &str) -> Result {
    let log = package
        .parent()
        .ok_or("no parent")?
        .join(format!("{failpoint}.log"));
    let output = ProcessCommand::new(std::env::current_exe()?)
        .args(["--exact", "child_entry", "--nocapture"])
        .env(OPERATION, operation)
        .env(PACKAGE, package)
        .env(ARGUMENT, argument)
        .env("DEADPAN_STORE_FAILPOINT", failpoint)
        .env("DEADPAN_STORE_FAILPOINT_LOG", &log)
        .output()?;
    let hit = fs::read_to_string(&log).unwrap_or_default();
    if hit.trim() != format!("hit {failpoint}") || output.status.success() {
        return Err(format!(
            "{failpoint}: not reached ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

#[test]
fn child_entry() -> Result {
    let Ok(operation) = std::env::var(OPERATION) else {
        return Ok(());
    };
    let package = PathBuf::from(std::env::var_os(PACKAGE).ok_or("package")?);
    let argument = std::env::var(ARGUMENT).unwrap_or_default();
    child(&operation, &package, &argument)
}

/// A package with two saved edits, a backup after the first and the head.
fn fixture(root: &Path, name: &str) -> Result<(PathBuf, String, RevisionId, RevisionId)> {
    let path = root.join(format!("{name}.deadpan"));
    let mut store = ProjectStore::create(&path, &document()?)?;
    let first = insert(&mut store, "first")?;
    for index in 0..12 {
        insert(&mut store, &format!("filler-{index}"))?;
    }
    drop(store);
    let id = backup(&path, BackupReason::Manual)?;
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let head = insert(&mut store, "second")?;
    drop(store);
    let _ = first;
    let backed = verify_backup(
        &list_backups(&path)?
            .into_iter()
            .find(|backup| backup.id == id)
            .ok_or("backup missing")?,
    )?
    .revision_id;
    Ok((path, id, backed, head))
}

#[test]
fn a_crash_right_after_publishing_a_backup_leaves_a_verified_backup() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, _, _, head) = fixture(&scratch.path().canonicalize()?, "backup")?;
    let before = list_backups(&path)?.len();
    crash_at("backup-after-rename", "backup", &path, "")?;
    let listed = list_backups(&path)?;
    assert_eq!(listed.len(), before + 1);
    for listed in &listed {
        verify_backup(listed)?;
    }
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.head_revision()?, head);
    store.validate_full()?;
    Ok(())
}

#[test]
fn a_crash_inside_the_restore_copy_leaves_the_project_unchanged() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, id, _, head) = fixture(&scratch.path().canonicalize()?, "mid-copy")?;
    crash_at("restore-mid-copy", "restore", &path, &id)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.head_revision()?, head, "the copy rolled back");
    store.validate_full()?;
    // The safety backup taken before the copy is published and valid.
    let safety = list_backups(&path)?
        .into_iter()
        .find(|backup| backup.reason == BackupReason::BeforeRestore)
        .ok_or("no safety backup")?;
    assert_eq!(verify_backup(&safety)?.revision_id, head);
    Ok(())
}

#[test]
fn a_crash_right_after_the_restore_copy_leaves_the_restored_project() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, id, backed, head) = fixture(&scratch.path().canonicalize()?, "after-copy")?;
    crash_at("restore-after-copy", "restore", &path, &id)?;
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.head_revision()?, backed);
    store.validate_full()?;
    // The replaced head's identity stays retired.
    let document = store.snapshot()?;
    let reused = store.commit(&CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: head.clone(),
        command: Command::Insert {
            parent: document.root().clone(),
            index: 0,
            subtree: Subtree {
                overrides: Default::default(),
                gap_overrides: Default::default(),
                root: NodeId::new("again")?,
                nodes: BTreeMap::from([(
                    NodeId::new("again")?,
                    BeatNode::hold(
                        "Pause",
                        HoldRecipe {
                            picture_context: None,
                            duration: FrameDuration::new(12)?,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
            },
        },
    });
    assert!(
        matches!(reused, Err(StoreError::RevisionReused(_))),
        "{reused:?}"
    );
    Ok(())
}

#[test]
fn a_crash_between_folding_the_wal_and_promoting_keeps_the_old_database() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, _, _, head) = fixture(&scratch.path().canonicalize()?, "wal-fold")?;
    let expected = ProjectStore::open(&path, AccessMode::ReadOnly)?.snapshot()?;
    crash_at("migration-after-wal-fold", "migrate", &path, "")?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.head_revision()?, head);
    assert_eq!(store.snapshot()?, expected);
    store.validate_full()?;
    Ok(())
}

#[test]
fn a_crash_right_after_promoting_keeps_the_new_database() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, _, _, _) = fixture(&scratch.path().canonicalize()?, "promoted")?;
    let expected = ProjectStore::open(&path, AccessMode::ReadOnly)?.snapshot()?;
    crash_at("migration-after-rename", "migrate", &path, "")?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadWrite),
        Err(StoreError::NewerSchema { .. })
    ));
    let viewer = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(viewer.snapshot()?, expected);
    Ok(())
}

#[test]
fn crashes_while_replacing_a_damaged_database_fail_loudly_and_a_rerun_completes() -> Result {
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().canonicalize()?;
    for failpoint in ["damaged-after-main-moved", "damaged-after-quarantine"] {
        let (path, id, backed, _) = fixture(&root, failpoint)?;
        // Leave a live WAL beside the database, as a crashed writer would.
        let connection = rusqlite::Connection::open(path.join("project.sqlite"))?;
        connection.execute_batch(
            "PRAGMA wal_autocheckpoint=0; CREATE TABLE scratch(x); DROP TABLE scratch;",
        )?;
        std::mem::forget(connection);
        crash_at(failpoint, "damaged", &path, &id)?;
        // No database at all: opening refuses instead of reading a main file
        // without its WAL.
        assert!(!path.join("project.sqlite").exists(), "{failpoint}");
        assert!(ProjectStore::open(&path, AccessMode::ReadWrite).is_err());
        let replaced = replace_damaged_database(&path, &id, None)?;
        assert!(!path.join("project.sqlite-wal").exists());
        let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(store.head_revision()?, backed, "{failpoint}");
        store.validate_full()?;
        let _ = replaced;
    }
    Ok(())
}
