#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::error::Error;
use std::fs::{self, File, FileTimes};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Duration;

use deadpan_core::{
    ColorPolicy, Command, CommandRequest, FrameRate, NodeId, PresentationBasis, ProjectDocument,
    ProjectId, RevisionId,
};
use deadpan_store::original_media::{
    LinkedOriginal, OriginalMediaError, OriginalMediaLimits, OriginalMediaRecord, OriginalOwnership,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

const ORIGINAL: &[u8] = b"complete original container bytes";

fn active() -> AtomicBool {
    AtomicBool::new(false)
}

fn limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(1_048_576, Duration::from_secs(10)).unwrap()
}

fn project(parent: &Path, name: &str) -> Result<(PathBuf, ProjectStore)> {
    let path = parent.join(format!("{name}.deadpan"));
    let document = ProjectDocument::new(
        ProjectId::new(name)?,
        RevisionId::new("r0")?,
        PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    Ok((path.clone(), ProjectStore::create(&path, &document)?))
}

fn retain(store: &mut ProjectStore, path: &Path, managed: bool) -> Result<OriginalMediaRecord> {
    fs::write(path, ORIGINAL)?;
    Ok(store
        .retain_original(
            path,
            if managed {
                OriginalOwnership::Managed
            } else {
                OriginalOwnership::Linked {
                    bookmark: Some(vec![1]),
                }
            },
            limits(),
            &active(),
        )?
        .record)
}

fn replacement(parent: &Path, name: &str, bookmark: u8) -> Result<LinkedOriginal> {
    let path = parent.join(name);
    fs::write(&path, ORIGINAL)?;
    Ok(LinkedOriginal::new(path, Some(vec![bookmark]))?)
}

#[test]
fn preparation_is_connection_free_and_commit_preserves_later_authored_edits() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, mut store) = project(scratch.path(), "threaded")?;
    let old_path = scratch.path().join("old.mov");
    let record = retain(&mut store, &old_path, false)?;
    let location = replacement(scratch.path(), "new.mov", 2)?;
    fs::remove_file(&old_path)?;
    let handle = store.original_import_handle()?;
    let captured = record.clone();
    let requested = location.clone();
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute_batch("BEGIN IMMEDIATE;")?;
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        assert!(
            sender
                .send(handle.prepare_relink(
                    &captured,
                    captured.version(),
                    requested,
                    limits(),
                    &active(),
                ))
                .is_ok()
        );
    });
    // Complete preparation while SQLite is write-locked proves the worker does
    // not depend on a writer transaction. No elapsed-time guess is needed.
    let prepared = receiver.recv_timeout(Duration::from_secs(10))??;
    worker.join().unwrap();
    assert_eq!(
        store.original_record(record.object().content())?,
        Some(record.clone())
    );
    database.execute_batch("ROLLBACK;")?;

    let before = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("edited-during-relink")?,
        command: Command::Rename {
            node: before.root().clone(),
            label: "An authored edit while relinking".into(),
        },
    })?;
    let edited = store.snapshot()?;
    let relinked = store.relink_prepared_original(&prepared, &active())?;
    assert_eq!(relinked.version(), record.version() + 1);
    assert_eq!(relinked.linked(), Some(&location));
    assert_eq!(relinked.object(), record.object());
    assert_eq!(relinked.sha256(), record.sha256());
    assert_eq!(relinked.label(), record.label());
    assert!(!relinked.managed());
    assert_eq!(store.snapshot()?, edited);
    let undone = store.undo(edited.revision_id(), RevisionId::new("undo-after-relink")?)?;
    assert_ne!(undone.revision_id, *before.revision_id());
    assert_eq!(
        store.original_record(record.object().content())?,
        Some(relinked)
    );
    Ok(())
}

#[test]
fn delayed_relink_cannot_replace_a_newer_location_or_bookmark() -> Result {
    let scratch = tempfile::tempdir()?;
    let (_, mut store) = project(scratch.path(), "version")?;
    let record = retain(&mut store, &scratch.path().join("old.mov"), false)?;
    let location = replacement(scratch.path(), "new.mov", 2)?;
    let updated_bookmark = LinkedOriginal::new(location.path().to_owned(), Some(vec![3]))?;
    let handle = store.original_import_handle()?;
    let delayed =
        handle.prepare_relink(&record, record.version(), location, limits(), &active())?;
    let current = handle.prepare_relink(
        &record,
        record.version(),
        updated_bookmark.clone(),
        limits(),
        &active(),
    )?;
    let relinked = store.relink_prepared_original(&current, &active())?;
    for stale in [&delayed, &current] {
        assert!(matches!(
            store.relink_prepared_original(stale, &active()),
            Err(StoreError::OriginalMedia(OriginalMediaError::VersionConflict { current }))
                if current == relinked.version()
        ));
    }
    assert_eq!(
        store.original_record(record.object().content())?,
        Some(relinked.clone())
    );
    assert_eq!(relinked.linked(), Some(&updated_bookmark));
    // The synchronous compatibility path preserves the established no-op rule.
    assert_eq!(
        store.relink_original(
            record.object().content(),
            relinked.version(),
            updated_bookmark,
            limits(),
            &active(),
        )?,
        relinked
    );
    Ok(())
}

#[test]
fn final_commit_rejects_missing_replaced_modified_and_unsafe_replacements() -> Result {
    for change in [
        "missing",
        "same-bytes-replacement",
        "modified",
        "symlink",
        "directory",
    ] {
        let scratch = tempfile::tempdir()?;
        let (_, mut store) = project(scratch.path(), "freshness")?;
        let old_path = scratch.path().join("old.mov");
        let record = retain(&mut store, &old_path, false)?;
        let location = replacement(scratch.path(), "new.mov", 2)?;
        let prepared = store.original_import_handle()?.prepare_relink(
            &record,
            record.version(),
            location.clone(),
            limits(),
            &active(),
        )?;
        match change {
            "missing" => fs::remove_file(location.path())?,
            "same-bytes-replacement" => {
                fs::remove_file(location.path())?;
                fs::write(location.path(), ORIGINAL)?;
            }
            "modified" => {
                let modified = fs::metadata(location.path())?.modified()?;
                fs::write(location.path(), vec![b'x'; ORIGINAL.len()])?;
                File::open(location.path())?
                    .set_times(FileTimes::new().set_modified(modified + Duration::from_secs(2)))?;
            }
            "symlink" => {
                fs::remove_file(location.path())?;
                symlink(&old_path, location.path())?;
            }
            "directory" => {
                fs::remove_file(location.path())?;
                fs::create_dir(location.path())?;
            }
            _ => unreachable!(),
        }
        let before = store.snapshot()?;
        assert!(
            store
                .relink_prepared_original(&prepared, &active())
                .is_err(),
            "{change}"
        );
        assert_eq!(
            store.original_record(record.object().content())?,
            Some(record)
        );
        assert_eq!(store.snapshot()?, before);
    }
    Ok(())
}

#[test]
fn prepared_relink_is_bound_to_a_live_writable_owner_without_retaining_its_lock() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, mut store) = project(scratch.path(), "owner")?;
    let (_, mut other) = project(scratch.path(), "other")?;
    let record = retain(&mut store, &scratch.path().join("old.mov"), false)?;
    let location = replacement(scratch.path(), "new.mov", 2)?;
    let handle = store.original_import_handle()?;
    let prepared = handle.prepare_relink(
        &record,
        record.version(),
        location.clone(),
        limits(),
        &active(),
    )?;
    assert_eq!(
        other
            .relink_prepared_original(&prepared, &active())
            .unwrap_err()
            .code(),
        "OriginalImportSessionMismatch"
    );
    let mut reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert!(matches!(
        reader.relink_prepared_original(&prepared, &active()),
        Err(StoreError::ReadOnly)
    ));
    assert!(matches!(
        reader.original_import_handle(),
        Err(StoreError::ReadOnly)
    ));
    drop(store);
    let mut reopened = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(
        reopened
            .relink_prepared_original(&prepared, &active())
            .unwrap_err()
            .code(),
        "OriginalImportClosed"
    );
    assert_eq!(
        handle
            .prepare_relink(&record, record.version(), location, limits(), &active())
            .err()
            .unwrap()
            .code(),
        "OriginalImportClosed"
    );
    assert_eq!(
        reopened.original_record(record.object().content())?,
        Some(record)
    );
    Ok(())
}

#[test]
fn preparation_checks_version_identity_limits_and_cancellation_without_changes() -> Result {
    let scratch = tempfile::tempdir()?;
    let (_, mut store) = project(scratch.path(), "admission")?;
    let record = retain(&mut store, &scratch.path().join("old.mov"), false)?;
    let location = replacement(scratch.path(), "new.mov", 2)?;
    let handle = store.original_import_handle()?;
    let cancelled = AtomicBool::new(true);
    assert_eq!(
        handle
            .prepare_relink(
                &record,
                record.version(),
                location.clone(),
                limits(),
                &cancelled
            )
            .err()
            .unwrap()
            .code(),
        "OriginalCancelled"
    );
    // A stale version is rejected before trying to open the supplied path.
    let absent = LinkedOriginal::new(scratch.path().join("absent.mov"), None)?;
    assert!(matches!(
        handle.prepare_relink(&record, record.version() + 1, absent, limits(), &active()),
        Err(StoreError::OriginalMedia(
            OriginalMediaError::VersionConflict { .. }
        ))
    ));
    assert!(matches!(
        handle.prepare_relink(
            &record,
            record.version(),
            location.clone(),
            OriginalMediaLimits::new(1, Duration::from_secs(10))?,
            &active(),
        ),
        Err(StoreError::OriginalMedia(OriginalMediaError::ByteLimit))
    ));
    fs::write(location.path(), vec![b'x'; ORIGINAL.len()])?;
    assert!(matches!(
        handle.prepare_relink(&record, record.version(), location, limits(), &active()),
        Err(StoreError::OriginalMedia(
            OriginalMediaError::IdentityMismatch
        ))
    ));
    assert_eq!(
        store.original_record(record.object().content())?,
        Some(record)
    );
    Ok(())
}

#[test]
fn serialized_record_does_not_grant_authority_over_current_inventory() -> Result {
    let scratch = tempfile::tempdir()?;
    let (_, mut store) = project(scratch.path(), "record")?;
    let record = retain(&mut store, &scratch.path().join("old.mov"), false)?;
    let location = replacement(scratch.path(), "new.mov", 2)?;
    let mut json = serde_json::to_value(&record)?;
    json["label"] = serde_json::json!("untrusted substituted label");
    let forged: OriginalMediaRecord = serde_json::from_value(json)?;
    let prepared = store.original_import_handle()?.prepare_relink(
        &forged,
        record.version(),
        location,
        limits(),
        &active(),
    )?;
    assert!(matches!(
        store.relink_prepared_original(&prepared, &active()),
        Err(StoreError::OriginalMedia(
            OriginalMediaError::IdentityMismatch
        ))
    ));
    assert_eq!(
        store.original_record(record.object().content())?,
        Some(record)
    );
    Ok(())
}

#[test]
fn cancelled_and_failed_transactions_preserve_inventory_and_managed_ownership() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, mut store) = project(scratch.path(), "rollback")?;
    let record = retain(&mut store, &scratch.path().join("old.mov"), true)?;
    let location = replacement(scratch.path(), "new.mov", 2)?;
    let prepared = store.original_import_handle()?.prepare_relink(
        &record,
        record.version(),
        location.clone(),
        limits(),
        &active(),
    )?;
    let before = store.snapshot()?;
    assert_eq!(
        store
            .relink_prepared_original(&prepared, &AtomicBool::new(true))
            .unwrap_err()
            .code(),
        "OriginalCancelled"
    );
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER reject_relink BEFORE UPDATE ON original_media BEGIN SELECT RAISE(ABORT, 'injected relink failure'); END;")?;
    assert!(matches!(
        store.relink_prepared_original(&prepared, &active()),
        Err(StoreError::Database(_))
    ));
    assert_eq!(
        store.original_record(record.object().content())?,
        Some(record.clone())
    );
    assert_eq!(fs::read(location.path())?, ORIGINAL);
    database.execute_batch("DROP TRIGGER reject_relink;")?;
    let relinked = store.relink_prepared_original(&prepared, &active())?;
    assert_eq!(relinked.version(), record.version() + 1);
    assert_eq!(relinked.linked(), Some(&location));
    assert_eq!(relinked.object(), record.object());
    assert_eq!(relinked.sha256(), record.sha256());
    assert_eq!(relinked.label(), record.label());
    assert!(relinked.managed());
    assert_eq!(store.snapshot()?, before);
    Ok(())
}
