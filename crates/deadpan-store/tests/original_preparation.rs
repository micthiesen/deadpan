#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs::{self, File, FileTimes};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Duration;

use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
    Subtree,
};
use deadpan_store::original_media::{
    LinkedOriginal, OriginalMediaError, OriginalMediaLimits, OriginalObjectRef, OriginalOwnership,
    OriginalRetentionMethod,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

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

fn limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(1_048_576, Duration::from_secs(10)).unwrap()
}

fn active() -> AtomicBool {
    AtomicBool::new(false)
}

fn object_path(package: &Path, object: &OriginalObjectRef) -> PathBuf {
    package
        .join("Media/Originals")
        .join(format!("blake3-{}", object.content().digest()))
}

#[test]
fn worker_prepares_without_borrowing_or_blocking_the_writer() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, mut store) = project(scratch.path(), "threaded")?;
    let source = scratch.path().join("source.mov");
    fs::write(&source, b"complete container bytes")?;
    let handle = store.original_import_handle()?;
    // A connection-free preparation must complete even while another SQLite
    // connection holds its write transaction. No scheduling assumption is used.
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute_batch("BEGIN IMMEDIATE;")?;
    let (ready_tx, ready_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let prepared = handle
            .prepare_retention(&source, OriginalOwnership::Managed, limits(), &active())
            .unwrap();
        ready_tx.send(()).unwrap();
        continue_rx.recv().unwrap();
        prepared
    });
    ready_rx.recv_timeout(Duration::from_secs(10))?;
    assert!(store.original_records(None, 10)?.is_empty());
    database.execute_batch("ROLLBACK;")?;
    let before = store.snapshot()?;
    let node = NodeId::new("hold")?;
    store.commit(&CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("editing-during-import")?,
        command: Command::Insert {
            parent: before.root().clone(),
            index: 0,
            subtree: Subtree {
                overrides: Default::default(),
                root: node.clone(),
                nodes: BTreeMap::from([(
                    node,
                    BeatNode::hold(
                        "Pause",
                        HoldRecipe {
                            duration: FrameDuration::new(12)?,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
            },
        },
    })?;
    let edited = store.snapshot()?;
    continue_tx.send(())?;
    let prepared = worker.join().unwrap();
    assert!(object_path(&package, prepared.object()).exists());
    assert!(store.original_records(None, 10)?.is_empty());
    let retained = store.retain_prepared_original(&prepared, &active())?;
    assert_eq!(store.original_records(None, 10)?, vec![retained.record]);
    assert_eq!(store.snapshot()?, edited);
    Ok(())
}

#[test]
fn prepared_retention_merges_current_locations_and_deduplicates() -> Result {
    let scratch = tempfile::tempdir()?;
    let (_, mut store) = project(scratch.path(), "merge")?;
    let source = scratch.path().join("source.mov");
    fs::write(&source, b"shared bytes")?;
    let handle = store.original_import_handle()?;
    let managed =
        handle.prepare_retention(&source, OriginalOwnership::Managed, limits(), &active())?;
    let linked = handle.prepare_retention(
        &source,
        OriginalOwnership::Linked {
            bookmark: Some(vec![1, 2]),
        },
        limits(),
        &active(),
    )?;
    let first = store.retain_prepared_original(&linked, &active())?;
    let second = store.retain_prepared_original(&managed, &active())?;
    assert_eq!(first.record.version(), 1);
    assert_eq!(second.record.version(), 2);
    assert!(second.record.managed());
    assert_eq!(second.record.linked(), first.record.linked());
    assert_eq!(
        store.retain_prepared_original(&linked, &active())?.record,
        second.record
    );
    let again =
        handle.prepare_retention(&source, OriginalOwnership::Managed, limits(), &active())?;
    let final_result = store.retain_prepared_original(&again, &active())?;
    assert_eq!(final_result.method, OriginalRetentionMethod::Existing);
    assert_eq!(final_result.record, second.record);
    assert_eq!(store.original_records(None, 10)?.len(), 1);
    Ok(())
}

#[test]
fn delayed_linked_retention_cannot_replace_a_newer_location() -> Result {
    let scratch = tempfile::tempdir()?;
    let (_, mut store) = project(scratch.path(), "relink-during-retention")?;
    let first = scratch.path().join("first.mov");
    let later = scratch.path().join("later.mov");
    fs::write(&first, b"same original bytes")?;
    fs::copy(&first, &later)?;
    let initial = store
        .retain_original(
            &first,
            OriginalOwnership::Linked { bookmark: None },
            limits(),
            &active(),
        )?
        .record;
    let handle = store.original_import_handle()?;
    let delayed = handle.prepare_retention(
        &first,
        OriginalOwnership::Linked { bookmark: None },
        limits(),
        &active(),
    )?;
    let newer = store.relink_original(
        initial.object().content(),
        initial.version(),
        LinkedOriginal::new(later.clone(), Some(vec![7]))?,
        limits(),
        &active(),
    )?;
    assert_eq!(newer.version(), 2);
    let document = store.snapshot()?;
    assert!(matches!(
        store.retain_prepared_original(&delayed, &active()),
        Err(StoreError::OriginalMedia(
            OriginalMediaError::VersionConflict { current: 2 }
        ))
    ));
    assert_eq!(store.original_records(None, 10)?, vec![newer.clone()]);
    assert_eq!(store.snapshot()?, document);

    // A freshly prepared competing location also needs the explicit versioned
    // relink operation. Retention alone cannot replace an established location.
    assert!(matches!(
        store.retain_original(
            &first,
            OriginalOwnership::Linked { bookmark: None },
            limits(),
            &active(),
        ),
        Err(StoreError::OriginalMedia(
            OriginalMediaError::VersionConflict { current: 2 }
        ))
    ));
    let same = handle.prepare_retention(
        &later,
        OriginalOwnership::Linked {
            bookmark: Some(vec![7]),
        },
        limits(),
        &active(),
    )?;
    assert_eq!(
        store.retain_prepared_original(&same, &active())?.record,
        newer
    );
    Ok(())
}

#[test]
fn cancellation_preserves_publication_without_registering_it() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, mut store) = project(scratch.path(), "cancel")?;
    let source = scratch.path().join("source.mov");
    fs::write(&source, b"cancelled bytes")?;
    let handle = store.original_import_handle()?;
    let cancelled = AtomicBool::new(true);
    assert_eq!(
        handle
            .prepare_retention(&source, OriginalOwnership::Managed, limits(), &cancelled)
            .err()
            .unwrap()
            .code(),
        "OriginalCancelled"
    );
    assert_eq!(fs::read_dir(package.join("Media/Originals"))?.count(), 0);
    let prepared =
        handle.prepare_retention(&source, OriginalOwnership::Managed, limits(), &active())?;
    assert_eq!(
        store
            .retain_prepared_original(&prepared, &cancelled)
            .unwrap_err()
            .code(),
        "OriginalCancelled"
    );
    assert!(store.original_records(None, 10)?.is_empty());
    assert!(object_path(&package, prepared.object()).exists());
    store.retain_prepared_original(&prepared, &active())?;
    Ok(())
}

#[test]
fn handles_and_tokens_are_writer_session_bound_and_do_not_keep_the_lock() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, store) = project(scratch.path(), "owner")?;
    let (_, mut other) = project(scratch.path(), "other")?;
    let source = scratch.path().join("source.mov");
    fs::write(&source, b"session bytes")?;
    let handle = store.original_import_handle()?;
    let prepared =
        handle.prepare_retention(&source, OriginalOwnership::Managed, limits(), &active())?;
    assert_eq!(
        other
            .retain_prepared_original(&prepared, &active())
            .unwrap_err()
            .code(),
        "OriginalImportSessionMismatch"
    );
    let mut reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert!(matches!(
        reader.original_import_handle(),
        Err(StoreError::ReadOnly)
    ));
    assert!(matches!(
        reader.retain_prepared_original(&prepared, &active()),
        Err(StoreError::ReadOnly)
    ));
    drop(store);
    let mut reopened = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(
        reopened
            .retain_prepared_original(&prepared, &active())
            .unwrap_err()
            .code(),
        "OriginalImportClosed"
    );
    assert_eq!(
        handle
            .prepare_retention(&source, OriginalOwnership::Managed, limits(), &active())
            .err()
            .unwrap()
            .code(),
        "OriginalImportClosed"
    );
    assert!(reopened.original_records(None, 10)?.is_empty());
    assert_eq!(
        handle
            .prepare_retention(
                &source,
                OriginalOwnership::Managed,
                limits(),
                &AtomicBool::new(true),
            )
            .err()
            .unwrap()
            .code(),
        "OriginalImportClosed"
    );
    Ok(())
}

#[test]
fn prepared_retention_rejects_missing_replaced_and_in_place_modified_originals() -> Result {
    for managed in [false, true] {
        for change in ["missing", "replaced", "modified"] {
            let scratch = tempfile::tempdir()?;
            let (package, mut store) = project(scratch.path(), "freshness")?;
            let source = scratch.path().join("source.mov");
            fs::write(&source, b"initial bytes")?;
            let ownership = if managed {
                OriginalOwnership::Managed
            } else {
                OriginalOwnership::Linked { bookmark: None }
            };
            let prepared = store.original_import_handle()?.prepare_retention(
                &source,
                ownership,
                limits(),
                &active(),
            )?;
            let path = if managed {
                object_path(&package, prepared.object())
            } else {
                source
            };
            match change {
                "missing" => fs::remove_file(&path)?,
                "replaced" => {
                    fs::remove_file(&path)?;
                    fs::write(&path, b"initial bytes")?;
                    if managed {
                        fs::set_permissions(&path, fs::Permissions::from_mode(0o444))?;
                    }
                }
                "modified" => {
                    let original_time = fs::metadata(&path)?.modified()?;
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
                    fs::write(&path, b"changed bytes")?;
                    File::open(&path)?.set_times(
                        FileTimes::new().set_modified(original_time + Duration::from_secs(2)),
                    )?;
                    if managed {
                        fs::set_permissions(&path, fs::Permissions::from_mode(0o444))?;
                    }
                }
                _ => unreachable!(),
            }
            assert!(
                store
                    .retain_prepared_original(&prepared, &active())
                    .is_err(),
                "{managed}: {change}"
            );
            assert!(store.original_records(None, 10)?.is_empty());
        }
    }
    Ok(())
}

#[test]
fn final_admission_rejects_unsafe_namespaces_and_objects() -> Result {
    for change in [
        "namespace",
        "namespace-symlink",
        "writable",
        "hardlink",
        "object-symlink",
    ] {
        let scratch = tempfile::tempdir()?;
        let (package, mut store) = project(scratch.path(), "namespace")?;
        let source = scratch.path().join("source.mov");
        fs::write(&source, b"safe bytes")?;
        let prepared = store.original_import_handle()?.prepare_retention(
            &source,
            OriginalOwnership::Managed,
            limits(),
            &active(),
        )?;
        let path = object_path(&package, prepared.object());
        match change {
            "namespace" => fs::set_permissions(
                package.join("Media/Originals"),
                fs::Permissions::from_mode(0o770),
            )?,
            "namespace-symlink" => {
                fs::rename(
                    package.join("Media/Originals"),
                    package.join("Media/Elsewhere"),
                )?;
                symlink("Elsewhere", package.join("Media/Originals"))?;
            }
            "writable" => fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?,
            "hardlink" => fs::hard_link(&path, scratch.path().join("alias"))?,
            "object-symlink" => {
                fs::remove_file(&path)?;
                symlink(&source, &path)?;
            }
            _ => unreachable!(),
        }
        assert!(
            store
                .retain_prepared_original(&prepared, &active())
                .is_err(),
            "{change}"
        );
        assert!(store.original_records(None, 10)?.is_empty());
    }
    Ok(())
}

#[test]
fn database_rollback_keeps_published_bytes_and_the_prepared_token_can_retry() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, mut store) = project(scratch.path(), "rollback")?;
    let source = scratch.path().join("source.mov");
    fs::write(&source, b"retained after rollback")?;
    let prepared = store.original_import_handle()?.prepare_retention(
        &source,
        OriginalOwnership::Managed,
        limits(),
        &active(),
    )?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER reject_original BEFORE INSERT ON original_media BEGIN SELECT RAISE(ABORT, 'injected retention failure'); END;")?;
    assert!(matches!(
        store.retain_prepared_original(&prepared, &active()),
        Err(StoreError::Database(_))
    ));
    assert!(store.original_records(None, 10)?.is_empty());
    assert_eq!(
        fs::read(object_path(&package, prepared.object()))?,
        b"retained after rollback"
    );
    database.execute_batch("DROP TRIGGER reject_original;")?;
    store.retain_prepared_original(&prepared, &active())?;
    assert_eq!(store.original_records(None, 10)?.len(), 1);
    Ok(())
}

#[test]
fn prepared_snapshots_keep_private_bytes_after_source_change_and_session_close() -> Result {
    for managed in [false, true] {
        let scratch = tempfile::tempdir()?;
        let (package, mut store) = project(scratch.path(), "snapshot")?;
        let source = scratch.path().join("source.mov");
        fs::write(&source, b"frozen verified bytes")?;
        let ownership = if managed {
            OriginalOwnership::Managed
        } else {
            OriginalOwnership::Linked { bookmark: None }
        };
        let record = store
            .retain_original(&source, ownership, limits(), &active())?
            .record;
        let handle = store.original_import_handle()?;
        let mut snapshot = handle.snapshot_original(&record, limits(), &active())?;
        assert_eq!(snapshot.record(), &record);
        let path = if managed {
            object_path(&package, record.object())
        } else {
            source
        };
        fs::remove_file(&path)?;
        fs::write(&path, b"changed external bytes")?;
        drop(store);
        let mut observed = Vec::new();
        snapshot.read_to_end(&mut observed)?;
        assert_eq!(observed, b"frozen verified bytes");
        snapshot.seek(SeekFrom::Start(0))?;
        assert_eq!(
            handle
                .snapshot_original(&record, limits(), &active())
                .err()
                .unwrap()
                .code(),
            "OriginalImportClosed"
        );
    }
    Ok(())
}
