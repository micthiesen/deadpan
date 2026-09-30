use super::*;
use crate::live_project::{Operation, Request};
use deadpan_core::{Command, CommandRequest, NodeId, ProjectDocument};
use deadpan_store::AccessMode;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn document() -> TestResult<ProjectDocument> {
    Ok(ProjectDocument::new_automatic(
        ProjectId::new("preparation")?,
        RevisionId::new("initial")?,
        NodeId::new("root")?,
    )?)
}
fn store(root: &std::path::Path) -> TestResult<ProjectStore> {
    Ok(ProjectStore::create(
        &root.join("project.deadpan"),
        &document()?,
    )?)
}
fn retain(path: PathBuf) -> PreparationCommand {
    PreparationCommand::Retain {
        path,
        ownership: PreparationOwnership::Managed {},
    }
}
fn apply(store: &mut ProjectStore, command: &PreparationCommand) -> TestResult<PreparationCommit> {
    let cancelled = AtomicBool::new(false);
    let work = admit(store, command)?;
    let prepared = prepare(work, &cancelled)?;
    Ok(commit(store, command, prepared, &cancelled)?)
}
fn rename(store: &mut ProjectStore, next: &str) -> TestResult {
    let current = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: RevisionId::new(next)?,
        command: Command::Rename {
            node: current.root().clone(),
            label: next.into(),
        },
    })?;
    Ok(())
}

#[test]
fn protocol_rejects_unknown_fields_at_each_preparation_boundary() -> TestResult {
    let target = PreparationTarget::fresh();
    let commands = [
        retain("/tmp/source.mp4".into()),
        PreparationCommand::Retain {
            path: "/tmp/source.mp4".into(),
            ownership: PreparationOwnership::Linked { bookmark: None },
        },
        PreparationCommand::Checkpoint {},
    ];
    for command in commands {
        let valid = serde_json::to_value(Request::new(Operation::Prepare {
            project_id: document()?.project_id().clone(),
            target: target.clone(),
            command: Box::new(command),
        }))?;
        Request::from_value(valid.clone())?;
        for pointer in ["", "/operation", "/operation/target", "/operation/command"] {
            let mut invalid = valid.clone();
            invalid.pointer_mut(pointer).unwrap()["extra"] = json!(true);
            assert!(Request::from_value(invalid).is_err(), "{pointer}");
        }
        if valid.pointer("/operation/command/ownership").is_some() {
            let mut invalid = valid.clone();
            invalid["operation"]["command"]["ownership"]["extra"] = json!(true);
            assert!(Request::from_value(invalid).is_err());
        }
    }
    for state in ["preparing", "awaiting_commit", "cancelling", "cancelled"] {
        assert!(serde_json::from_value::<PreparationState>(json!({"state":state})).is_ok());
        assert!(
            serde_json::from_value::<PreparationState>(json!({"state":state,"extra":true}))
                .is_err()
        );
    }
    for kind in ["video_only", "video_and_audio", "audio_only"] {
        assert!(
            serde_json::from_value::<SourceStreams>(
                json!({"type":kind,"stream":0,"audio_stream":0,"extra":true})
            )
            .is_err()
        );
    }
    assert!(
        PreparationTarget {
            operation_id: Uuid::nil(),
            cancellation_token: Uuid::new_v4()
        }
        .validate()
        .is_err()
    );
    assert!(
        PreparationTarget {
            operation_id: Uuid::new_v4(),
            cancellation_token: Uuid::nil()
        }
        .validate()
        .is_err()
    );
    Ok(())
}

#[test]
fn command_admission_bounds_paths_bookmarks_and_labels() -> TestResult {
    for path in [
        "relative.mp4".into(),
        "/tmp/../video.mp4".into(),
        format!("/{}", "x".repeat(MAX_PATH_BYTES)),
        "/tmp/zero\0.mp4".into(),
    ] {
        assert!(retain(path.into()).validate().is_err());
    }
    let large = PreparationCommand::Retain {
        path: "/tmp/video.mp4".into(),
        ownership: PreparationOwnership::Linked {
            bookmark: Some(vec![255; MAX_BOOKMARK_BYTES + 1]),
        },
    };
    assert!(large.validate().is_err());
    // A bounded bookmark may still overflow the whole JSON envelope.
    let full = PreparationCommand::Retain {
        path: "/tmp/video.mp4".into(),
        ownership: PreparationOwnership::Linked {
            bookmark: Some(vec![255; MAX_BOOKMARK_BYTES]),
        },
    };
    assert_eq!(full.validate().unwrap_err().code, "HostPreparationLimit");
    assert!(validate_label(&"x".repeat(MAX_LABEL_BYTES + 1)).is_err());
    assert!(validate_label("").is_err());
    Ok(())
}

#[test]
fn retention_prepares_off_writer_and_preserves_full_operational_receipt() -> TestResult {
    let root = tempfile::tempdir()?;
    let mut store = store(root.path())?;
    let before = store.snapshot()?;
    let source = root.path().join("bytes.bin");
    std::fs::write(&source, b"whole retained bytes")?;
    let command = retain(source);
    let work = admit(&mut store, &command)?;
    assert!(store.original_records(None, 100)?.is_empty());
    let prepared = std::thread::spawn(move || prepare(work, &AtomicBool::new(false)))
        .join()
        .unwrap()?;
    assert!(store.original_records(None, 100)?.is_empty());
    let committed = commit(&mut store, &command, prepared, &AtomicBool::new(false))?;
    assert!(committed.inventory_changed);
    assert!(committed.committed_revision.is_none());
    let PreparationReceipt::Retained {
        content,
        location_version,
    } = committed.receipt
    else {
        panic!("wrong receipt")
    };
    assert_eq!(location_version, 1);
    assert_eq!(store.original_record(&content)?.unwrap().version(), 1);
    assert_eq!(
        committed.output["retained_original"]["record"]["object"]["content"],
        serde_json::to_value(content)?
    );
    assert_eq!(store.snapshot()?, before);
    Ok(())
}

#[test]
fn prepared_result_cannot_be_committed_under_another_command_or_owner() -> TestResult {
    let root = tempfile::tempdir()?;
    let other = tempfile::tempdir()?;
    let mut first = store(root.path())?;
    let mut second = store(other.path())?;
    let path = root.path().join("bytes.bin");
    std::fs::write(&path, b"original bytes")?;
    let command = retain(path);
    let work = admit(&mut first, &command)?;
    let prepared = prepare(work, &AtomicBool::new(false))?;
    let error = commit(
        &mut first,
        &PreparationCommand::Checkpoint {},
        prepared,
        &AtomicBool::new(false),
    )
    .err()
    .unwrap();
    assert_eq!(error.code, "HostPreparationChanged");
    assert!(first.original_records(None, 100)?.is_empty());
    let work = admit(&mut first, &command)?;
    let prepared = prepare(work, &AtomicBool::new(false))?;
    let error = commit(&mut second, &command, prepared, &AtomicBool::new(false))
        .err()
        .unwrap();
    assert_eq!(error.code, "OriginalImportSessionMismatch");
    assert!(second.original_records(None, 100)?.is_empty());
    Ok(())
}

#[test]
fn cancellation_before_commit_and_closed_session_preserve_inventory() -> TestResult {
    let root = tempfile::tempdir()?;
    let mut store = store(root.path())?;
    let path = root.path().join("bytes.bin");
    std::fs::write(&path, b"original bytes")?;
    let command = retain(path);
    let prepared = prepare(admit(&mut store, &command)?, &AtomicBool::new(false))?;
    assert!(commit(&mut store, &command, prepared, &AtomicBool::new(true)).is_err());
    assert!(store.original_records(None, 100)?.is_empty());
    let work = admit(&mut store, &command)?;
    drop(store);
    assert_eq!(
        prepare(work, &AtomicBool::new(false)).err().unwrap().code,
        "OriginalImportClosed"
    );
    Ok(())
}

#[test]
fn relink_rechecks_captured_version_and_retains_exact_mapping() -> TestResult {
    let root = tempfile::tempdir()?;
    let mut store = store(root.path())?;
    let path = root.path().join("source.bin");
    std::fs::write(&path, b"original bytes")?;
    let receipt = apply(
        &mut store,
        &PreparationCommand::Retain {
            path: path.clone(),
            ownership: PreparationOwnership::Linked { bookmark: None },
        },
    )?
    .receipt;
    let PreparationReceipt::Retained { content, .. } = receipt else {
        panic!("wrong receipt")
    };
    let moved = root.path().join("moved.bin");
    std::fs::rename(path, &moved)?;
    let command = PreparationCommand::Relink {
        content: content.clone(),
        expected_version: 1,
        location: LinkedOriginal::new(moved.clone(), Some(vec![1, 2, 3]))?,
    };
    let first = prepare(admit(&mut store, &command)?, &AtomicBool::new(false))?;
    let second = prepare(admit(&mut store, &command)?, &AtomicBool::new(false))?;
    let committed = commit(&mut store, &command, first, &AtomicBool::new(false))?;
    assert_eq!(
        committed.receipt,
        PreparationReceipt::Relinked {
            content: content.clone(),
            location_version: 2
        }
    );
    assert_eq!(
        store
            .original_record(&content)?
            .unwrap()
            .linked()
            .unwrap()
            .path(),
        moved
    );
    assert_eq!(
        commit(&mut store, &command, second, &AtomicBool::new(false))
            .err()
            .unwrap()
            .code,
        "OriginalLocationConflict"
    );
    assert_eq!(
        admit(&mut store, &command).err().unwrap().code,
        "OriginalLocationConflict"
    );
    Ok(())
}

#[test]
fn checkpoint_captures_actual_worker_revision_without_mutating_authored_history() -> TestResult {
    let root = tempfile::tempdir()?;
    let mut store = store(root.path())?;
    let command = PreparationCommand::Checkpoint {};
    let work = admit(&mut store, &command)?;
    rename(&mut store, "before-backup")?;
    let prepared = prepare(work, &AtomicBool::new(false))?;
    rename(&mut store, "after-backup")?;
    let result = commit(&mut store, &command, prepared, &AtomicBool::new(false))?;
    let PreparationReceipt::Checkpoint {
        path,
        project_id,
        revision_id,
    } = result.receipt
    else {
        panic!("wrong receipt")
    };
    assert_eq!(revision_id.as_str(), "before-backup");
    assert_eq!(&project_id, document()?.project_id());
    assert_eq!(store.snapshot()?.revision_id().as_str(), "after-backup");
    assert!(path.is_file());
    assert_eq!(
        result.output["database_checkpoint"],
        serde_json::to_value(&path)?
    );
    assert!(result.committed_revision.is_none());
    assert!(!result.inventory_changed);
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let captured: String = connection.query_row(
        "SELECT head_revision FROM state WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(captured, "before-backup");
    Ok(())
}

#[test]
fn registration_retains_exact_intent_rejects_stale_then_accepts_noop_receipt() -> TestResult {
    let root = tempfile::tempdir()?;
    let mut store = store(root.path())?;
    let path = root.path().join("source.mp4");
    std::fs::write(
        &path,
        include_bytes!("../../../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
    )?;
    let PreparationReceipt::Retained { content, .. } = apply(&mut store, &retain(path))?.receipt
    else {
        panic!("wrong receipt")
    };
    let mut registration = SourceRegistration {
        expected_revision: store.snapshot()?.revision_id().clone(),
        new_revision: RevisionId::new("registered")?,
        original: content,
        new_asset_id: AssetId::new("explicit-asset")?,
        label: "Captured label".into(),
        insertion: None,
    };
    let command = PreparationCommand::Register {
        registration: registration.clone(),
        streams: SourceStreams::VideoAndAudio { audio_stream: 1 },
    };
    let prepared = prepare(admit(&mut store, &command)?, &AtomicBool::new(false))?;
    rename(&mut store, "later-edit")?;
    assert_eq!(
        commit(&mut store, &command, prepared, &AtomicBool::new(false))
            .err()
            .unwrap()
            .code,
        "RevisionConflict"
    );
    assert!(store.snapshot()?.assets().is_empty());
    registration.expected_revision = store.snapshot()?.revision_id().clone();
    let command = PreparationCommand::Register {
        registration: registration.clone(),
        streams: SourceStreams::VideoAndAudio { audio_stream: 1 },
    };
    let committed = apply(&mut store, &command)?;
    assert_eq!(
        committed.committed_revision.as_ref(),
        Some(&registration.new_revision)
    );
    assert_eq!(committed.output["outcome"]["asset_id"], "explicit-asset");
    let before = store.snapshot()?;
    registration.expected_revision = before.revision_id().clone();
    registration.new_revision = RevisionId::new("unused-noop-revision")?;
    let command = PreparationCommand::Register {
        registration,
        streams: SourceStreams::VideoAndAudio { audio_stream: 1 },
    };
    let noop = apply(&mut store, &command)?;
    assert_eq!(noop.receipt, committed.receipt);
    assert!(noop.committed_revision.is_none());
    assert_eq!(noop.output["committed"], false);
    assert_eq!(store.snapshot()?, before);
    let read_only = ProjectStore::open(&root.path().join("project.deadpan"), AccessMode::ReadOnly)?;
    assert_eq!(read_only.snapshot()?, before);
    Ok(())
}

#[test]
fn checkpoint_publication_failure_keeps_a_completed_operational_receipt() -> TestResult {
    let receipt = deadpan_store::checkpoint::CheckpointReceipt {
        path: "/tmp/checkpoint-visible.sqlite".into(),
        project_id: document()?.project_id().clone(),
        revision_id: document()?.revision_id().clone(),
        database_bytes: 4096,
    };
    let committed = checkpoint_result(Err(
        deadpan_store::checkpoint::CheckpointError::PublishedUnconfirmed {
            receipt: Box::new(receipt.clone()),
            source: std::io::Error::other("injected namespace sync failure"),
        },
    ))?;
    assert_eq!(
        committed.completion_error.as_ref().unwrap().code,
        "CheckpointPublishedUnconfirmed"
    );
    assert_eq!(
        committed.output["database_checkpoint"],
        serde_json::to_value(&receipt.path)?
    );
    assert_eq!(
        committed.receipt,
        PreparationReceipt::Checkpoint {
            path: receipt.path,
            project_id: receipt.project_id,
            revision_id: receipt.revision_id
        }
    );
    assert!(committed.committed_revision.is_none());
    Ok(())
}

#[test]
fn relinking_identical_location_is_a_successful_noop() -> TestResult {
    let root = tempfile::tempdir()?;
    let mut store = store(root.path())?;
    let path = root.path().join("source.bin");
    std::fs::write(&path, b"retained same path")?;
    let PreparationReceipt::Retained {
        content,
        location_version,
    } = apply(
        &mut store,
        &PreparationCommand::Retain {
            path: path.clone(),
            ownership: PreparationOwnership::Linked { bookmark: None },
        },
    )?
    .receipt
    else {
        panic!("wrong receipt")
    };
    let command = PreparationCommand::Relink {
        content: content.clone(),
        expected_version: location_version,
        location: LinkedOriginal::new(path, None)?,
    };
    let result = apply(&mut store, &command)?;
    assert_eq!(
        result.receipt,
        PreparationReceipt::Relinked {
            content,
            location_version
        }
    );
    assert!(result.committed_revision.is_none());
    assert_eq!(store.snapshot()?, document()?);
    Ok(())
}
