#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

use deadpan_core::{
    ColorPolicy, Command, CommandRequest, FrameRate, NodeId, PresentationBasis, ProjectDocument,
    ProjectId, RevisionId,
};
use deadpan_store::checkpoint::{CheckpointError, CheckpointLimits, CheckpointReceipt};
use deadpan_store::{AccessMode, ProjectStore};
use rusqlite::{Connection, OpenFlags};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn document() -> Result<ProjectDocument> {
    Ok(ProjectDocument::new(
        ProjectId::new("project")?,
        RevisionId::new("r0")?,
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1_001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?)
}

fn rename(store: &mut ProjectStore, revision: &str) -> Result<ProjectDocument> {
    let current = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: RevisionId::new(revision)?,
        command: Command::Rename {
            node: current.root().clone(),
            label: revision.into(),
        },
    })?;
    Ok(store.snapshot()?)
}

fn assert_empty_snapshots(package: &Path) -> Result {
    assert_eq!(fs::read_dir(package.join("Snapshots"))?.count(), 0);
    Ok(())
}

#[test]
fn worker_pins_actual_revision_while_writer_keeps_editing() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("concurrent.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let handle = store.checkpoint_handle()?;
    // Admission predates this edit. The receipt must report the snapshot the
    // worker actually captures, rather than the admission or publication head.
    let captured = rename(&mut store, "captured")?;
    let (started_tx, started_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut first = true;
        handle.prepare_with_progress(
            CheckpointLimits {
                pages_per_step: 1,
                ..Default::default()
            },
            &AtomicBool::new(false),
            |progress| {
                if first {
                    first = false;
                    assert_eq!(progress.pages_copied, 0);
                    started_tx.send(()).expect("writer remains available");
                    continue_rx
                        .recv_timeout(Duration::from_secs(10))
                        .expect("writer can commit while checkpoint has its read transaction");
                }
            },
        )
    });
    started_rx.recv_timeout(Duration::from_secs(10))?;
    let later = rename(&mut store, "later")?;
    continue_tx.send(())?;
    let prepared = worker.join().expect("checkpoint worker did not panic")?;
    assert_eq!(prepared.project_id(), captured.project_id());
    assert_eq!(prepared.revision_id(), captured.revision_id());
    let receipt = store.publish_prepared_checkpoint(prepared, &AtomicBool::new(false))?;
    assert_eq!(receipt.revision_id, *captured.revision_id());
    assert_eq!(store.snapshot()?, later);
    assert_eq!(fs::metadata(&receipt.path)?.len(), receipt.database_bytes);
    assert_eq!(
        fs::metadata(&receipt.path)?.permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::read_dir(package.join("Snapshots"))?.count(), 1);
    let db = Connection::open_with_flags(&receipt.path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    // The head may be stored as a patch; read it through a package around
    // the standalone database copy.
    let copy = tempfile::tempdir()?;
    assert_eq!(
        open_database_copy(&receipt.path, copy.path())?.snapshot()?,
        captured
    );
    assert_eq!(
        db.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))?,
        "ok"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM revisions WHERE id='later'",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        0
    );
    assert_eq!(
        serde_json::from_str::<CheckpointReceipt>(&serde_json::to_string(&receipt)?)?,
        receipt
    );
    Ok(())
}

#[test]
fn cancellation_before_and_during_backup_leaves_no_checkpoint() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("cancel.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    assert!(matches!(
        store
            .checkpoint_handle()?
            .prepare(Default::default(), &AtomicBool::new(true)),
        Err(CheckpointError::Cancelled)
    ));
    let cancelled = AtomicBool::new(false);
    let mut copied = 0;
    let result = store.checkpoint_handle()?.prepare_with_progress(
        CheckpointLimits {
            pages_per_step: 1,
            ..Default::default()
        },
        &cancelled,
        |progress| {
            copied = progress.pages_copied;
            if copied > 0 {
                cancelled.store(true, Ordering::Release);
            }
        },
    );
    assert!(copied > 0, "cancellation exercised an in-progress backup");
    assert!(matches!(result, Err(CheckpointError::Cancelled)));
    assert_eq!(store.snapshot()?, document()?);
    assert_empty_snapshots(&package)
}

#[test]
fn cancellation_after_preparation_denies_publication() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("late-cancel.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let prepared = store
        .checkpoint_handle()?
        .prepare(Default::default(), &AtomicBool::new(false))?;
    assert!(matches!(
        store.publish_prepared_checkpoint(prepared, &AtomicBool::new(true)),
        Err(CheckpointError::Cancelled)
    ));
    assert_empty_snapshots(&package)
}

#[test]
fn bounds_and_deadline_stop_backup_without_publication() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("bounded.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    for limits in [
        CheckpointLimits {
            pages_per_step: 0,
            ..Default::default()
        },
        CheckpointLimits {
            pages_per_step: 257,
            ..Default::default()
        },
        CheckpointLimits {
            timeout: Duration::ZERO,
            ..Default::default()
        },
        CheckpointLimits {
            max_database_bytes: u64::MAX,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            store
                .checkpoint_handle()?
                .prepare(limits, &AtomicBool::new(false)),
            Err(CheckpointError::InvalidLimits)
        ));
    }
    assert!(matches!(
        store.checkpoint_handle()?.prepare(
            CheckpointLimits {
                max_database_bytes: 1,
                ..Default::default()
            },
            &AtomicBool::new(false),
        ),
        Err(CheckpointError::TooLarge)
    ));
    assert!(matches!(
        store.checkpoint_handle()?.prepare_with_progress(
            CheckpointLimits {
                timeout: Duration::from_millis(1),
                ..Default::default()
            },
            &AtomicBool::new(false),
            |_| std::thread::sleep(Duration::from_millis(2)),
        ),
        Err(CheckpointError::Deadline)
    ));
    assert_empty_snapshots(&package)
}

#[test]
fn closed_owner_revokes_both_unstarted_and_prepared_workers() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("reopen.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let unstarted = store.checkpoint_handle()?;
    let prepared = store
        .checkpoint_handle()?
        .prepare(Default::default(), &AtomicBool::new(false))?;
    drop(store);
    // The retained worker descriptors do not keep the writer lock alive.
    let mut reopened = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert!(matches!(
        unstarted.prepare(Default::default(), &AtomicBool::new(false)),
        Err(CheckpointError::SessionClosed)
    ));
    assert!(matches!(
        reopened.publish_prepared_checkpoint(prepared, &AtomicBool::new(false)),
        Err(CheckpointError::SessionClosed)
    ));
    assert_empty_snapshots(&package)
}

#[test]
fn foreign_writer_cannot_publish_even_when_project_ids_match() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("original.deadpan");
    let other_package = scratch.path().join("other.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let mut other = ProjectStore::create(&other_package, &document()?)?;
    let prepared = store
        .checkpoint_handle()?
        .prepare(Default::default(), &AtomicBool::new(false))?;
    assert!(
        other
            .publish_prepared_checkpoint(prepared, &AtomicBool::new(false))
            .is_err()
    );
    assert_empty_snapshots(&package)?;
    assert_empty_snapshots(&other_package)
}

#[test]
fn replacing_snapshot_directory_cannot_redirect_publication_or_cleanup() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("directory.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let prepared = store
        .checkpoint_handle()?
        .prepare(Default::default(), &AtomicBool::new(false))?;
    let old = package.join("old-snapshots");
    fs::rename(package.join("Snapshots"), &old)?;
    fs::create_dir(package.join("Snapshots"))?;
    fs::write(package.join("Snapshots/unrelated"), b"keep")?;
    assert!(matches!(
        store.publish_prepared_checkpoint(prepared, &AtomicBool::new(false)),
        Err(CheckpointError::IdentityChanged)
    ));
    assert_eq!(fs::read(package.join("Snapshots/unrelated"))?, b"keep");
    assert_eq!(fs::read_dir(old)?.count(), 0);
    Ok(())
}

#[test]
fn database_replacement_before_or_after_admission_is_rejected() -> Result {
    for before in [true, false] {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("database.deadpan");
        let mut store = ProjectStore::create(&package, &document()?)?;
        let handle = if before {
            None
        } else {
            Some(store.checkpoint_handle()?)
        };
        fs::rename(
            package.join("project.sqlite"),
            package.join("original.sqlite"),
        )?;
        fs::write(package.join("project.sqlite"), b"replacement")?;
        if let Some(handle) = handle {
            // Identity is checked before SQLite opens the replacement bytes.
            assert!(matches!(
                handle.prepare(Default::default(), &AtomicBool::new(false)),
                Err(CheckpointError::IdentityChanged)
            ));
        } else {
            assert!(store.checkpoint_handle().is_err());
        }
        assert_eq!(fs::read(package.join("project.sqlite"))?, b"replacement");
        assert_empty_snapshots(&package)?;
    }
    Ok(())
}

#[test]
fn readonly_store_and_symlink_snapshot_directory_are_rejected() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("readonly.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let mut reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert!(reader.checkpoint_handle().is_err());
    fs::rename(package.join("Snapshots"), package.join("real-snapshots"))?;
    symlink("real-snapshots", package.join("Snapshots"))?;
    assert!(store.checkpoint_handle().is_err());
    assert_eq!(fs::read_dir(package.join("real-snapshots"))?.count(), 0);
    Ok(())
}

#[test]
fn changed_prepared_database_is_never_published() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("changed.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let prepared = store
        .checkpoint_handle()?
        .prepare(Default::default(), &AtomicBool::new(false))?;
    let staging = fs::read_dir(package.join("Snapshots"))?
        .next()
        .ok_or("missing staging")??;
    let mut changed = OpenOptions::new()
        .append(true)
        .open(staging.path().join("checkpoint.sqlite"))?;
    changed.write_all(b"changed")?;
    drop(changed);
    assert!(matches!(
        store.publish_prepared_checkpoint(prepared, &AtomicBool::new(false)),
        Err(CheckpointError::IdentityChanged)
    ));
    assert_empty_snapshots(&package)
}

/// Open a standalone database copy read-only inside a fresh package.
fn open_database_copy(
    database: &std::path::Path,
    scratch: &std::path::Path,
) -> std::result::Result<deadpan_store::ProjectStore, Box<dyn std::error::Error>> {
    let package = scratch.join("database-copy.deadpan");
    for directory in [
        "Media/Originals",
        "Media/Generated",
        "Media/RenderCandidates",
    ] {
        std::fs::create_dir_all(package.join(directory))?;
    }
    std::fs::copy(database, package.join("project.sqlite"))?;
    Ok(deadpan_store::ProjectStore::open(
        &package,
        deadpan_store::AccessMode::ReadOnly,
    )?)
}
