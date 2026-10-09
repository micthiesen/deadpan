//! Rotating verified backups, restore, and read-only viewing of packages a
//! newer Deadpan saved. See docs/BACKUPS.md.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
    Subtree,
};
use deadpan_store::backups::{
    self, BackupLimits, BackupPolicy, BackupReason, create_backup, list_backups, verify_backup,
};
use deadpan_store::{AccessMode, DATABASE_SCHEMA_VERSION, ProjectStore, StoreError};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn document(project: &str) -> Result<ProjectDocument> {
    Ok(ProjectDocument::new(
        ProjectId::new(project)?,
        RevisionId::new("r0")?,
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?)
}

/// Insert one 12-frame pause at the start of the root.
fn insert(store: &mut ProjectStore, revision: &str) -> Result<RevisionId> {
    let current = store.snapshot()?;
    let id = NodeId::new(format!("pause-{revision}"))?;
    let outcome = store.commit(&CommandRequest {
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
    })?;
    Ok(outcome.revision_id)
}

fn backup(path: &Path, reason: BackupReason) -> Result<backups::BackupOutcome> {
    Ok(create_backup(
        path,
        reason,
        &BackupPolicy::default(),
        BackupLimits::default(),
        &AtomicBool::new(false),
    )?)
}

fn project(name: &str) -> Result<(tempfile::TempDir, PathBuf, ProjectStore)> {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join(format!("{name}.deadpan"));
    let store = ProjectStore::create(&path, &document(name)?)?;
    let path = path.canonicalize()?;
    Ok((scratch, path, store))
}

#[test]
fn a_backup_is_a_verified_standalone_copy_of_the_committed_state() -> Result {
    let (_scratch, path, mut store) = project("standalone")?;
    let saved = insert(&mut store, "r1")?;
    let outcome = backup(&path, BackupReason::Manual)?;
    assert_eq!(outcome.revision_id.as_ref(), Some(&saved));
    assert!(outcome.removed.is_empty());
    let listed = list_backups(&path)?;
    assert_eq!(listed, vec![outcome.backup.clone()]);
    assert_eq!(listed[0].reason, BackupReason::Manual);
    assert!(listed[0].path.starts_with(path.join("Backups")));
    // Standalone: DELETE journal mode and no sidecars beside it.
    let connection =
        Connection::open_with_flags(&listed[0].path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mode: String = connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    assert_eq!(mode, "delete");
    drop(connection);
    let names: Vec<String> = fs::read_dir(path.join("Backups"))?
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with(".backups"))
        .collect();
    assert_eq!(names, vec![format!("{}.sqlite", listed[0].id)]);
    let preview = verify_backup(&listed[0])?;
    assert_eq!(preview.revision_id, saved);
    assert_eq!(preview.schema, DATABASE_SCHEMA_VERSION);
    assert_eq!((preview.revisions, preview.edits, preview.beats), (2, 1, 1));
    assert_eq!(preview.duration_frames, 12);
    // Backups are counted in the storage report.
    let report = store.storage_report(Duration::from_secs(0))?;
    assert!(report.auxiliary_bytes >= listed[0].database_bytes);
    // And the history's own size is measured.
    assert_eq!(
        (
            report.history.revisions,
            report.history.edits,
            report.history.keyframes
        ),
        (2, 1, 1)
    );
    assert!(report.history.keyframe_bytes > 0 && report.history.patch_bytes > 0);
    Ok(())
}

#[test]
fn backups_run_beside_a_committing_writer_and_capture_a_saved_revision() -> Result {
    let (_scratch, path, mut store) = project("concurrent")?;
    insert(&mut store, "r1")?;
    let copier = {
        let path = path.clone();
        std::thread::spawn(move || -> std::result::Result<_, String> {
            let mut outcomes = Vec::new();
            for _ in 0..4 {
                outcomes.push(
                    create_backup(
                        &path,
                        BackupReason::Periodic,
                        &BackupPolicy::default(),
                        BackupLimits {
                            pages_per_step: 1,
                            ..BackupLimits::default()
                        },
                        &AtomicBool::new(false),
                    )
                    .map_err(|error| error.to_string())?,
                );
            }
            Ok(outcomes)
        })
    };
    let mut saved = vec![RevisionId::new("r0")?, RevisionId::new("r1")?];
    for index in 2..30 {
        saved.push(insert(&mut store, &format!("r{index}"))?);
    }
    let outcomes = copier.join().map_err(|_| "backup thread panicked")??;
    for outcome in outcomes {
        let revision = outcome.revision_id.ok_or("missing revision")?;
        assert!(saved.contains(&revision), "{revision}");
        let preview = verify_backup(&outcome.backup)?;
        assert_eq!(preview.revision_id, revision);
    }
    store.validate_full()?;
    Ok(())
}

#[test]
fn restore_replaces_the_database_and_is_itself_reversible() -> Result {
    let (_scratch, path, mut store) = project("restore")?;
    let early = insert(&mut store, "early")?;
    let early_document = store.snapshot()?;
    let chosen = backup(&path, BackupReason::Periodic)?;
    insert(&mut store, "late-1")?;
    let late = insert(&mut store, "late-2")?;
    let late_document = store.snapshot()?;
    let outcome = store.restore_backup(
        &chosen.backup.id,
        &BackupPolicy::default(),
        BackupLimits::default(),
        &AtomicBool::new(false),
    )?;
    assert_eq!(outcome.restored.revision_id, early);
    assert_eq!(outcome.replaced_revision, late);
    assert_eq!(outcome.safety.backup.reason, BackupReason::BeforeRestore);
    assert_eq!(outcome.safety.revision_id.as_ref(), Some(&late));
    assert_eq!(store.snapshot()?, early_document);
    assert_eq!(store.head_revision()?, early);
    // History is the backup's: one edit to undo, nothing to redo.
    assert_eq!(store.history_availability()?, (true, false));
    // The restored writer keeps working and its state survives reopening.
    let after = insert(&mut store, "after-restore")?;
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.head_revision()?, after);
    assert!(store.open_recovery().unclean_previous_writer.is_none());
    store.validate_full()?;
    // Restoring the safety backup brings back the replaced edits.
    store.restore_backup(
        &outcome.safety.backup.id,
        &BackupPolicy::default(),
        BackupLimits::default(),
        &AtomicBool::new(false),
    )?;
    assert_eq!(store.snapshot()?, late_document);
    store.validate_full()?;
    Ok(())
}

#[test]
fn restore_refuses_damaged_and_foreign_backups_without_changing_the_project() -> Result {
    let (_scratch, path, mut store) = project("guarded")?;
    insert(&mut store, "r1")?;
    let chosen = backup(&path, BackupReason::Manual)?;
    insert(&mut store, "r2")?;
    let current = store.snapshot()?;
    let restore = |store: &mut ProjectStore, id: &str| {
        store.restore_backup(
            id,
            &BackupPolicy::default(),
            BackupLimits::default(),
            &AtomicBool::new(false),
        )
    };
    // Unknown names, including paths, are not found.
    for id in ["nothing", "../project", &format!("{}/x", chosen.backup.id)] {
        let error = restore(&mut store, id).expect_err("unknown backup");
        assert_eq!(error.code(), "BackupNotFound", "{id}");
    }
    // A backup of another project, placed under a valid name.
    let (_other_scratch, other_path, mut other) = project("other")?;
    insert(&mut other, "o1")?;
    let foreign = backup(&other_path, BackupReason::Manual)?;
    drop(other);
    let foreign_name = foreign.backup.path.file_name().ok_or("no name")?.to_owned();
    fs::copy(
        &foreign.backup.path,
        path.join("Backups").join(&foreign_name),
    )?;
    let error = restore(&mut store, &foreign.backup.id).expect_err("foreign backup");
    assert_eq!(error.code(), "BackupOtherProject");
    // A damaged copy fails verification.
    let mut bytes = fs::read(&chosen.backup.path)?;
    let middle = bytes.len() / 2;
    for byte in &mut bytes[middle..middle + 4096] {
        *byte ^= 0x5a;
    }
    fs::set_permissions(&chosen.backup.path, fs::Permissions::from_mode(0o600))?;
    fs::write(&chosen.backup.path, bytes)?;
    let error = restore(&mut store, &chosen.backup.id).expect_err("damaged backup");
    assert!(
        matches!(error.code(), "BackupInvalid" | "BackupDatabaseFailure"),
        "{error}"
    );
    // Nothing changed, and no safety backup was taken for a refused restore.
    assert_eq!(store.snapshot()?, current);
    assert_eq!(
        list_backups(&path)?
            .iter()
            .filter(|backup| backup.reason == BackupReason::BeforeRestore)
            .count(),
        0
    );
    store.validate_full()?;
    Ok(())
}

#[test]
fn rotation_bounds_the_folder_and_keeps_the_newest_and_safety_backups() -> Result {
    let (_scratch, path, mut store) = project("rotation")?;
    let policy = BackupPolicy {
        keep_recent: 2,
        hourly_hours: 0,
        daily_days: 0,
        weekly_weeks: 0,
        keep_safety: 1,
        max_count: 3,
        ..BackupPolicy::default()
    };
    let mut newest = None;
    for (index, reason) in [
        BackupReason::Periodic,
        BackupReason::BeforeMigration,
        BackupReason::Periodic,
        BackupReason::Periodic,
        BackupReason::Close,
    ]
    .into_iter()
    .enumerate()
    {
        insert(&mut store, &format!("rotation-{index}"))?;
        // Distinct millisecond stamps order the backups.
        std::thread::sleep(Duration::from_millis(3));
        newest = Some(create_backup(
            &path,
            reason,
            &policy,
            BackupLimits::default(),
            &AtomicBool::new(false),
        )?);
    }
    let listed = list_backups(&path)?;
    let reasons: Vec<_> = listed.iter().map(|backup| backup.reason).collect();
    assert_eq!(
        reasons,
        vec![
            BackupReason::Close,
            BackupReason::Periodic,
            BackupReason::BeforeMigration
        ]
    );
    assert_eq!(listed[0], newest.ok_or("no backup")?.backup);
    // An abandoned staging file older than the limit is removed; a fresh one
    // (a copy in progress) is not.
    let stale = path.join("Backups/.staging-abandoned.sqlite");
    let fresh = path.join("Backups/.staging-in-progress.sqlite");
    fs::write(&stale, b"partial")?;
    fs::write(&fresh, b"partial")?;
    let old = std::time::SystemTime::now() - backups::STALE_STAGING - Duration::from_secs(60);
    fs::File::options()
        .write(true)
        .open(&stale)?
        .set_modified(old)?;
    let (removed, staging) = backups::rotate_backups(&path, &policy)?;
    assert!(removed.is_empty());
    assert_eq!(staging, 1);
    assert!(!stale.exists() && fresh.exists());
    Ok(())
}

#[test]
fn cancellation_publishes_nothing() -> Result {
    let (_scratch, path, mut store) = project("cancel")?;
    insert(&mut store, "r1")?;
    let error = create_backup(
        &path,
        BackupReason::Periodic,
        &BackupPolicy::default(),
        BackupLimits::default(),
        &AtomicBool::new(true),
    )
    .expect_err("cancelled before copying");
    assert_eq!(error.code(), "BackupCancelled");
    assert!(list_backups(&path)?.is_empty());
    let leftovers = fs::read_dir(path.join("Backups"))
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.file_name().to_string_lossy().starts_with(".staging-"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(leftovers, 0);
    Ok(())
}

/// Make a package look like a newer Deadpan saved it: a later schema
/// number and a table this build does not know.
fn make_newer(path: &Path) -> Result<u32> {
    let newer = DATABASE_SCHEMA_VERSION + 1;
    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute_batch(
        "CREATE TABLE future_feature(id INTEGER PRIMARY KEY, value TEXT NOT NULL) STRICT;
         INSERT INTO future_feature(value) VALUES ('not understood by this build');",
    )?;
    connection.pragma_update(None, "user_version", newer)?;
    connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    Ok(newer)
}

#[test]
fn a_newer_schema_opens_read_only_for_viewing_and_is_never_written() -> Result {
    let (_scratch, path, mut store) = project("future")?;
    insert(&mut store, "r1")?;
    let saved = store.snapshot()?;
    drop(store);
    let newer = make_newer(&path)?;
    let before = fs::read(path.join("project.sqlite"))?;
    // Writers refuse with the explanation.
    let error = ProjectStore::open(&path, AccessMode::ReadWrite)
        .err()
        .ok_or("opened")?;
    assert!(matches!(error, StoreError::NewerSchema { found, .. } if found == newer));
    assert_eq!(error.code(), "SchemaNewer");
    assert!(error.to_string().contains("newer Deadpan"));
    // Viewing works and shows the saved document.
    let mut viewer = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(viewer.newer_schema(), Some(newer));
    assert_eq!(viewer.snapshot()?, saved);
    assert_eq!(viewer.head_revision()?, *saved.revision_id());
    // Nothing writes: commits, history and validation of a later build's
    // history all refuse.
    assert!(
        matches!(insert(&mut viewer, "r2"), Err(error) if error.to_string().contains("read-only"))
    );
    assert!(matches!(
        viewer.validate(),
        Err(StoreError::NewerSchema { .. })
    ));
    assert!(matches!(
        ProjectStore::migrate(&path),
        Err(StoreError::NewerSchema { .. })
    ));
    assert!(
        create_backup(
            &path,
            BackupReason::Manual,
            &BackupPolicy::default(),
            BackupLimits::default(),
            &AtomicBool::new(false)
        )
        .is_err()
    );
    // Refused before creating anything in the package, and storage
    // accounting refuses a later build's tables it cannot read.
    assert!(!path.join("Backups").exists());
    assert!(matches!(
        viewer.storage_report(Duration::from_secs(0)),
        Err(StoreError::NewerSchema { .. })
    ));
    drop(viewer);
    assert_eq!(fs::read(path.join("project.sqlite"))?, before);
    assert!(
        !path.join("project.sqlite-wal").exists()
            || fs::metadata(path.join("project.sqlite-wal"))?.len() == 0
    );
    Ok(())
}

#[test]
fn a_newer_document_this_build_cannot_read_is_refused_with_an_explanation() -> Result {
    let (_scratch, path, mut store) = project("unreadable")?;
    insert(&mut store, "r1")?;
    let head = store.head_revision()?;
    drop(store);
    make_newer(&path)?;
    // A later build added a document field this build rejects. Store the
    // head as a keyframe with that field.
    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute(
        "UPDATE revisions SET document=json_set((SELECT document FROM revisions WHERE parent_id IS NULL),'$.future_field',1,'$.revision_id',id), depth=0 WHERE id=?1",
        [head.as_str()],
    )?;
    connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    drop(connection);
    let before = fs::read(path.join("project.sqlite"))?;
    let error = ProjectStore::open(&path, AccessMode::ReadOnly)
        .err()
        .ok_or("opened")?;
    let message = error.to_string();
    assert!(message.contains("newer Deadpan"), "{message}");
    assert!(message.contains("nothing was changed"), "{message}");
    assert_eq!(fs::read(path.join("project.sqlite"))?, before);
    Ok(())
}

use std::os::unix::fs::PermissionsExt;

#[test]
fn inspected_damaged_recovery_keeps_every_old_database_sidecar() -> Result {
    let (_scratch, path, mut store) = project("captured-damage")?;
    let saved = insert(&mut store, "saved")?;
    let chosen = backup(&path, BackupReason::Manual)?;
    drop(store);
    fs::write(path.join("project.sqlite"), b"damaged database")?;
    fs::write(
        path.join("project.sqlite-journal"),
        b"retained rollback journal",
    )?;
    let captured = backups::inspect_damaged_database(&path, &chosen.backup.id)?;
    assert_eq!(captured.preview.revision_id, saved);
    assert!(!captured.requires_project_confirmation);
    let replaced = backups::restore_damaged_database(&captured, None)?;
    assert_eq!(
        fs::read(replaced.quarantine.join("project.sqlite"))?,
        b"damaged database"
    );
    assert_eq!(
        fs::read(replaced.quarantine.join("project.sqlite-journal"))?,
        b"retained rollback journal"
    );
    assert!(!path.join("project.sqlite-journal").exists());
    let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(reopened.head_revision()?, saved);
    reopened.validate_full()?;
    Ok(())
}

#[test]
fn inspected_recovery_refuses_changed_project_backup_and_readable_database() -> Result {
    let (_scratch, path, store) = project("recovery-changed")?;
    let chosen = backup(&path, BackupReason::Manual)?;
    drop(store);
    assert!(backups::inspect_damaged_database(&path, &chosen.backup.id).is_err());
    fs::write(path.join("project.sqlite"), b"broken")?;
    let captured = backups::inspect_damaged_database(&path, &chosen.backup.id)?;
    fs::write(path.join("project.sqlite"), b"different broken database")?;
    assert!(
        backups::restore_damaged_database(&captured, None)
            .unwrap_err()
            .to_string()
            .contains("changed after inspection")
    );
    assert_eq!(
        fs::read(path.join("project.sqlite"))?,
        b"different broken database"
    );
    let captured = backups::inspect_damaged_database(&path, &chosen.backup.id)?;
    // Even replacing a backup with identical bytes invalidates that inspection.
    let bytes = fs::read(&chosen.backup.path)?;
    let other = chosen.backup.path.with_extension("replacement");
    fs::write(&other, bytes)?;
    fs::rename(&other, &chosen.backup.path)?;
    assert!(
        backups::restore_damaged_database(&captured, None)
            .unwrap_err()
            .to_string()
            .contains("changed after inspection")
    );
    assert_eq!(
        fs::read(path.join("project.sqlite"))?,
        b"different broken database"
    );
    Ok(())
}

#[test]
fn inspected_recovery_requires_explicit_project_identity_for_an_unreadable_manifest() -> Result {
    let (_scratch, path, store) = project("recovery-identity")?;
    let chosen = backup(&path, BackupReason::Manual)?;
    drop(store);
    fs::write(path.join("project.sqlite"), b"broken")?;
    fs::write(path.join("manifest.json"), b"unreadable manifest")?;
    let captured = backups::inspect_damaged_database(&path, &chosen.backup.id)?;
    assert!(captured.requires_project_confirmation);
    assert!(backups::restore_damaged_database(&captured, None).is_err());
    assert!(backups::restore_damaged_database(&captured, Some(&ProjectId::new("wrong")?)).is_err());
    assert_eq!(fs::read(path.join("project.sqlite"))?, b"broken");
    backups::restore_damaged_database(&captured, Some(&ProjectId::new("recovery-identity")?))?;
    ProjectStore::open(&path, AccessMode::ReadWrite)?.validate_full()?;
    Ok(())
}

#[test]
fn a_database_that_no_longer_opens_is_replaced_by_a_verified_backup() -> Result {
    let (_scratch, path, mut store) = project("damaged")?;
    let saved = insert(&mut store, "r1")?;
    let chosen = backup(&path, BackupReason::Manual)?;
    insert(&mut store, "r2")?;
    drop(store);
    // Damage the live database so that opening fails.
    let database = path.join("project.sqlite");
    let mut bytes = fs::read(&database)?;
    let middle = bytes.len() / 2;
    for byte in &mut bytes[middle..] {
        *byte = 0xa5;
    }
    fs::write(&database, &bytes)?;
    assert!(ProjectStore::open(&path, AccessMode::ReadWrite).is_err());
    // Restoring through an open writer is impossible; the damaged path works.
    let replaced = backups::replace_damaged_database(&path, &chosen.backup.id, None)?;
    assert_eq!(replaced.restored.revision_id, saved);
    assert_eq!(
        fs::read(replaced.quarantine.join("project.sqlite"))?,
        bytes,
        "damaged copy kept"
    );
    let leftovers = fs::read_dir(&path)?
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".restoring-")
        })
        .count();
    assert_eq!(leftovers, 0);
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.head_revision()?, saved);
    store.validate_full()?;
    // An open writer refuses it.
    assert_eq!(
        backups::replace_damaged_database(&path, &chosen.backup.id, None)
            .err()
            .ok_or("replaced under a writer")?
            .code(),
        "ProjectAlreadyOpen"
    );
    Ok(())
}

#[test]
fn a_newer_package_with_a_live_wal_is_viewed_at_its_latest_commit_without_writing_it() -> Result {
    let (_scratch, path, mut store) = project("live-wal")?;
    insert(&mut store, "r1")?;
    drop(store);
    // A newer build still has the package open: its last commits are only
    // in the WAL.
    let writer = Connection::open(path.join("project.sqlite"))?;
    writer.pragma_update(None, "wal_autocheckpoint", 0)?;
    writer.execute_batch("CREATE TABLE future_feature(id INTEGER PRIMARY KEY) STRICT;")?;
    writer.pragma_update(None, "user_version", DATABASE_SCHEMA_VERSION + 1)?;
    let main = fs::read(path.join("project.sqlite"))?;
    let wal = fs::read(path.join("project.sqlite-wal"))?;
    assert!(!wal.is_empty());
    let viewer = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(viewer.newer_schema(), Some(DATABASE_SCHEMA_VERSION + 1));
    assert_eq!(viewer.head_revision()?.as_str(), "r1");
    drop(viewer);
    // The database and its WAL are untouched; only SQLite's shared-memory
    // index (-shm) is used, as for any reader.
    assert_eq!(fs::read(path.join("project.sqlite"))?, main);
    assert_eq!(fs::read(path.join("project.sqlite-wal"))?, wal);
    drop(writer);
    Ok(())
}

#[test]
fn restore_revokes_handles_and_never_frees_issued_identities() -> Result {
    let (_scratch, path, mut store) = project("revoked")?;
    insert(&mut store, "kept")?;
    let chosen = backup(&path, BackupReason::Manual)?;
    let discarded = insert(&mut store, "discarded")?;
    // Capabilities handed out before the restore.
    let import = store.original_import_handle()?;
    let checkpoint = store.checkpoint_handle()?;
    store.restore_backup(
        &chosen.backup.id,
        &BackupPolicy::default(),
        BackupLimits::default(),
        &AtomicBool::new(false),
    )?;
    assert!(import.check_live(&AtomicBool::new(false)).is_err());
    assert!(
        checkpoint
            .prepare(Default::default(), &AtomicBool::new(false))
            .is_err()
    );
    // Fresh capabilities work.
    store
        .original_import_handle()?
        .check_live(&AtomicBool::new(false))?;
    // The revision the restore discarded is never issued again, also after
    // reopening.
    assert!(matches!(
        insert(&mut store, discarded.as_str()),
        Err(error) if error.to_string().contains("already been used")
    ));
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert!(insert(&mut store, discarded.as_str()).is_err());
    insert(&mut store, "fresh")?;
    store.validate_full()?;
    Ok(())
}

fn request(
    store: &mut ProjectStore,
    request: &str,
) -> std::result::Result<deadpan_store::generation::StoredGenerationRequest, StoreError> {
    use deadpan_jobs::*;
    store.allocate_generation_request(deadpan_store::generation::GenerationRequestInput {
        request_id: RequestId::new(request).unwrap(),
        expected_revision: store.snapshot()?.revision_id().clone(),
        hold_id: NodeId::new("pause-hold").unwrap(),
        context_sha256: Sha256::new("c".repeat(64)).unwrap(),
        constraints: HoldConstraints {
            video: VideoSpec::new(
                FrameDuration::new(12).unwrap(),
                FrameRate::new(30, 1).unwrap(),
                512,
                320,
            )
            .unwrap(),
            conditioning: ConditioningMode::Bridge,
            motion: MotionAmount::Still,
            instructions: None,
            region_target: None,
        },
        provider: ProviderSelection {
            pack_id: ProviderPackId::new("pack").unwrap(),
            pack_version: ProviderPackVersion::new("v1").unwrap(),
            runtime_id: RuntimeId::new("runtime").unwrap(),
            runtime_version: RuntimeVersion::new("v1").unwrap(),
            seed: 1,
        },
    })
}

#[test]
fn restore_carries_ai_request_identities_and_hold_versions_forward() -> Result {
    let (_scratch, path, mut store) = project("requests")?;
    insert(&mut store, "hold")?;
    let first = request(&mut store, "request-1")?;
    let chosen = backup(&path, BackupReason::Manual)?;
    let second = request(&mut store, "request-2")?;
    assert!(second.binding.request_version > first.binding.request_version);
    store.restore_backup(
        &chosen.backup.id,
        &BackupPolicy::default(),
        BackupLimits::default(),
        &AtomicBool::new(false),
    )?;
    // The discarded request's identity and version are not issued again.
    assert!(matches!(
        request(&mut store, "request-2"),
        Err(StoreError::GenerationRequestReused(_))
    ));
    let third = request(&mut store, "request-3")?;
    assert!(third.binding.request_version > second.binding.request_version);
    store.validate_full()?;
    Ok(())
}

#[test]
fn cleanup_fails_closed_on_unreadable_backups_and_waits_for_copies() -> Result {
    use deadpan_store::storage::CleanupPolicy;
    let (_scratch, path, mut store) = project("pinning")?;
    insert(&mut store, "r1")?;
    backup(&path, BackupReason::Manual)?;
    let policy = CleanupPolicy::everything(Duration::from_secs(0), false);
    store.clean_storage(policy)?;
    // A copy in progress holds the active lock shared: cleanup waits a
    // bounded time and then refuses instead of computing references.
    let active = fs::File::open(path.join("Backups/.backups-active.lock"))?;
    active.lock_shared()?;
    let refused = store
        .clean_storage(policy)
        .err()
        .ok_or("cleaned during a copy")?;
    assert!(
        refused.to_string().contains("backup is being made"),
        "{refused}"
    );
    // A dry run never waits.
    store.clean_storage(CleanupPolicy::everything(Duration::from_secs(0), true))?;
    drop(active);
    // Backups that cannot be listed pin unknown objects: refuse.
    let folder = path.join("Backups");
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o000))?;
    let refused = store.clean_storage(policy);
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
    assert!(refused.is_err(), "cleaned with unreadable backups");
    store.clean_storage(policy)?;
    Ok(())
}
