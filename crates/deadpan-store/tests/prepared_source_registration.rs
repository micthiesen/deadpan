#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Barrier, mpsc};
use std::thread;
use std::time::Duration;

use deadpan_core::{
    AssetId, BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, NodeKind, ProjectDocument, ProjectId, RevisionId, Subtree,
};
use deadpan_jobs::{
    ConditioningMode, HoldConstraints, MotionAmount, ProviderPackId, ProviderPackVersion,
    ProviderSelection, Relevance, RequestId, RuntimeId, RuntimeVersion, Sha256, VideoSpec,
};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_store::generation::{
    ContextObservation, GenerationRequestInput, RelevanceObservation, RelevancePlan,
};
use deadpan_store::original_media::{
    LinkedOriginal, OriginalMediaLimits, OriginalMediaRecord, OriginalOwnership,
};
use deadpan_store::source_registration::{
    PreparedSourceRegistration, SourceInsertionPurpose, SourceInsertionRequest, SourceRegistration,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn cancelled() -> AtomicBool {
    AtomicBool::new(false)
}
fn limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(2_000_000, Duration::from_secs(10)).unwrap()
}
fn asset(value: &str) -> AssetId {
    AssetId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures")
        .join(name)
        .canonicalize()
        .unwrap()
}
fn project(parent: &Path) -> Result<(PathBuf, ProjectStore)> {
    let path = parent.join("prepared.deadpan");
    let document = ProjectDocument::new(
        ProjectId::new("prepared-source")?,
        revision("initial"),
        deadpan_core::PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: FrameRate::new(30000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    Ok((path.clone(), ProjectStore::create(&path, &document)?))
}
fn automatic_project(parent: &Path) -> Result<(PathBuf, ProjectStore)> {
    let path = parent.join("automatic.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("automatic-source")?,
        revision("initial"),
        NodeId::new("root")?,
    )?;
    Ok((path.clone(), ProjectStore::create(&path, &document)?))
}
fn retain(store: &mut ProjectStore, name: &str) -> Result<OriginalMediaRecord> {
    Ok(store
        .retain_original(
            &fixture(name),
            OriginalOwnership::Managed,
            limits(),
            &cancelled(),
        )?
        .record)
}
fn decoded_from_handle(
    handle: deadpan_store::original_media::OriginalImportHandle,
    record: OriginalMediaRecord,
) -> Result<PreparedSourceRegistration> {
    let mut snapshot = handle.snapshot_original(&record, limits(), &cancelled())?;
    let input = VerifiedSourceInput::copy_verified(
        &mut snapshot,
        SourceContentIdentity::new(record.sha256(), record.object().byte_length())?,
        2_000_000,
        Duration::from_secs(10),
        &cancelled(),
    )?;
    let video = SourceSession::open_input(
        input.clone(),
        asset("decode"),
        SourceSessionLimits::default(),
        &cancelled(),
    )?;
    let audio = AudioSession::open_input(input, 1, AudioSessionLimits::default(), &cancelled())?;
    let decoded = DecodedSourceQualification::from_sessions(Some(&video), Some(&audio))?;
    Ok(PreparedSourceRegistration::from_decoded(
        snapshot,
        &decoded,
        &cancelled(),
    )?)
}
fn registration(
    store: &ProjectStore,
    record: &OriginalMediaRecord,
    next: &str,
    name: &str,
) -> SourceRegistration {
    SourceRegistration {
        expected_revision: store.snapshot().unwrap().revision_id().clone(),
        new_revision: revision(next),
        original: record.object().content().clone(),
        new_asset_id: asset(name),
        label: name.into(),
        insertion: None,
    }
}
fn primary_registration(
    store: &ProjectStore,
    record: &OriginalMediaRecord,
    next: &str,
    name: &str,
    node: &str,
) -> SourceRegistration {
    SourceRegistration {
        insertion: Some(SourceInsertionRequest {
            parent: NodeId::new("root").unwrap(),
            index: 0,
            node: NodeId::new(node).unwrap(),
            label: node.into(),
            purpose: SourceInsertionPurpose::Primary,
        }),
        ..registration(store, record, next, name)
    }
}
fn counts(path: &Path) -> Result<(i64, i64, i64)> {
    let db = Connection::open(path.join("project.sqlite"))?;
    Ok(db.query_row(
        "SELECT (SELECT count(*) FROM revisions),(SELECT count(*) FROM history),(SELECT count(*) FROM source_qualifications)",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?)
}
fn command(store: &ProjectStore, next: &str, command: Command) -> CommandRequest {
    let current = store.snapshot().unwrap();
    CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: revision(next),
        command,
    }
}

#[test]
fn preparation_can_run_on_worker_while_writer_edits_and_does_not_author_state() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let handle = store.original_import_handle()?;
    let gate = Arc::new(Barrier::new(2));
    let (sender, receiver) =
        mpsc::channel::<std::result::Result<PreparedSourceRegistration, String>>();
    let worker_gate = Arc::clone(&gate);
    let worker = thread::spawn(move || {
        worker_gate.wait();
        sender
            .send(decoded_from_handle(handle, original).map_err(|error| error.to_string()))
            .unwrap();
    });
    gate.wait();
    store.commit(&command(
        &store,
        "canvas",
        Command::SetCanvas {
            width: 640,
            height: 360,
        },
    ))?;
    let prepared = receiver.recv()?.map_err(std::io::Error::other)?;
    worker.join().unwrap();
    assert_eq!(counts(&path)?, (2, 1, 0));
    let record = store
        .original_record(prepared.receipt().original().content())?
        .unwrap();
    let input = registration(&store, &record, "registered", "camera");
    store.register_prepared_source(&input, &prepared, None, &cancelled())?;
    assert_eq!(counts(&path)?, (3, 2, 1));
    Ok(())
}

#[test]
fn stale_target_is_rejected_then_same_token_registers_against_current_revision() -> Result {
    let scratch = tempfile::tempdir()?;
    let (_, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "cfr-bframes.mp4")?;
    let prepared = decoded_from_handle(store.original_import_handle()?, original.clone())?;
    let stale = registration(&store, &original, "stale", "camera");
    store.commit(&command(
        &store,
        "canvas",
        Command::SetCanvas {
            width: 800,
            height: 450,
        },
    ))?;
    assert!(
        store
            .register_prepared_source(&stale, &prepared, None, &cancelled())
            .is_err()
    );
    let current = registration(&store, &original, "current", "camera");
    let result = store.register_prepared_source(&current, &prepared, None, &cancelled())?;
    assert_eq!(result.asset_id, asset("camera"));
    Ok(())
}

#[test]
fn preview_is_read_only_and_prepared_token_is_bound_to_issuing_writer() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "cfr-bframes.mp4")?;
    let prepared = decoded_from_handle(store.original_import_handle()?, original.clone())?;
    let input = registration(&store, &original, "registered", "camera");
    let before = counts(&path)?;
    let preview = store.preview_prepared_source_registration(&input, &prepared, &cancelled())?;
    assert_eq!(preview.qualification, *prepared.receipt().id());
    assert_eq!(counts(&path)?, before);
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(
        reader
            .preview_prepared_source_registration(&input, &prepared, &cancelled())
            .is_err()
    );
    Ok(())
}

#[test]
fn changed_managed_original_invalidates_prepared_token_without_history_changes() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "cfr-bframes.mp4")?;
    let prepared = decoded_from_handle(store.original_import_handle()?, original.clone())?;
    let input = registration(&store, &original, "registered", "camera");
    let before = counts(&path)?;
    let object = path
        .join("Media/Originals")
        .join(format!("blake3-{}", original.object().content().digest()));
    fs::remove_file(&object)?;
    fs::copy(fixture("offset-bframes.mp4"), &object)?;
    assert!(
        store
            .register_prepared_source(&input, &prepared, None, &cancelled())
            .is_err()
    );
    assert_eq!(counts(&path)?, before);
    Ok(())
}

#[test]
fn prepared_token_rejects_delete_same_bytes_and_in_place_rewrite() -> Result {
    for mode in ["delete", "replace", "rewrite"] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store) = project(scratch.path())?;
        let original = retain(&mut store, "cfr-bframes.mp4")?;
        let prepared = decoded_from_handle(store.original_import_handle()?, original.clone())?;
        let input = registration(&store, &original, "registered", "camera");
        let object = path
            .join("Media/Originals")
            .join(format!("blake3-{}", original.object().content().digest()));
        let bytes = fs::read(&object)?;
        match mode {
            "delete" => fs::remove_file(&object)?,
            "replace" => {
                fs::remove_file(&object)?;
                fs::write(&object, &bytes)?;
                let mut permissions = fs::metadata(&object)?.permissions();
                permissions.set_readonly(true);
                fs::set_permissions(&object, permissions)?;
            }
            "rewrite" => {
                use std::io::{Seek, SeekFrom, Write};
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&object, fs::Permissions::from_mode(0o600))?;
                let mut file = fs::OpenOptions::new().write(true).open(&object)?;
                file.seek(SeekFrom::Start(0))?;
                file.write_all(&bytes)?;
                file.sync_all()?;
                let mut permissions = fs::metadata(&object)?.permissions();
                permissions.set_readonly(true);
                fs::set_permissions(&object, permissions)?;
            }
            _ => unreachable!(),
        }
        let before = counts(&path)?;
        assert!(
            store
                .register_prepared_source(&input, &prepared, None, &cancelled())
                .is_err(),
            "mutation mode {mode}"
        );
        assert_eq!(counts(&path)?, before);
    }
    Ok(())
}

#[test]
fn prepared_duplicate_reuses_receipt_but_corrupted_existing_payload_is_rejected() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "cfr-bframes.mp4")?;
    let prepared = decoded_from_handle(store.original_import_handle()?, original.clone())?;
    let first = registration(&store, &original, "first", "camera");
    let outcome = store.register_prepared_source(&first, &prepared, None, &cancelled())?;
    assert!(outcome.commit.is_some());
    let before = counts(&path)?;
    let duplicate = registration(&store, &original, "duplicate", "different-alias");
    let duplicate = store.register_prepared_source(&duplicate, &prepared, None, &cancelled())?;
    assert_eq!(duplicate.asset_id, asset("camera"));
    assert!(duplicate.commit.is_none());
    assert_eq!(counts(&path)?, before);

    let db = Connection::open(path.join("project.sqlite"))?;
    db.execute(
        "UPDATE source_qualifications SET snapshot=CAST(CAST(snapshot AS TEXT)||' ' AS BLOB) WHERE id=?1",
        [outcome.qualification.as_str()],
    )?;
    drop(db);
    let before_corrupt = counts(&path)?;
    let retry = registration(&store, &original, "retry", "another-alias");
    assert!(
        store
            .register_prepared_source(&retry, &prepared, None, &cancelled())
            .is_err()
    );
    assert_eq!(counts(&path)?, before_corrupt);
    Ok(())
}

#[test]
fn prepared_primary_uses_current_timing_after_automatic_basis_becomes_explicit() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = automatic_project(scratch.path())?;
    let original = retain(&mut store, "cfr-bframes.mp4")?;
    let prepared = decoded_from_handle(store.original_import_handle()?, original.clone())?;
    let before = store.snapshot()?;
    store.commit(&command(
        &store,
        "canvas",
        Command::SetCanvas {
            width: 640,
            height: 360,
        },
    ))?;
    let input = primary_registration(&store, &original, "registered", "camera", "clip");
    store.register_prepared_source(&input, &prepared, None, &cancelled())?;
    let after = store.snapshot()?;
    assert_eq!(
        after.presentation_basis().frame_rate,
        FrameRate::new(30, 1)?
    );
    assert_eq!(
        (
            after.presentation_basis().width,
            after.presentation_basis().height
        ),
        (640, 360)
    );
    assert_eq!(after.nodes().len(), before.nodes().len() + 1);
    let NodeKind::Source { source } = &after.nodes()[&NodeId::new("clip")?].kind else {
        panic!("prepared primary insertion did not create a Source node")
    };
    assert_eq!(source.duration.frames(), 121);
    assert_eq!(
        source.video_mapping.duration_frames(source.duration)?,
        deadpan_core::ExactRatio::new(3003, 25)?
    );
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?, after);

    let untimed_dir = tempfile::tempdir()?;
    let (_, mut untimed) = automatic_project(untimed_dir.path())?;
    let original = retain(&mut untimed, "cfr-bframes.mp4")?;
    let prepared = decoded_from_handle(untimed.original_import_handle()?, original.clone())?;
    let input = primary_registration(&untimed, &original, "untimed-register", "camera", "clip");
    untimed.register_prepared_source(&input, &prepared, None, &cancelled())?;
    let untimed_snapshot = untimed.snapshot()?;
    assert_eq!(
        untimed_snapshot.presentation_basis().frame_rate,
        FrameRate::new(30000, 1001)?
    );
    let NodeKind::Source { source } = &untimed_snapshot.nodes()[&NodeId::new("clip")?].kind else {
        panic!("untimed insertion did not create a Source node")
    };
    assert_eq!(source.duration.frames(), 120);
    Ok(())
}

#[test]
fn prepared_insertion_rolls_back_on_history_failure_and_retries_same_token() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "cfr-bframes.mp4")?;
    let prepared = decoded_from_handle(store.original_import_handle()?, original.clone())?;
    let input = primary_registration(&store, &original, "registered", "camera", "clip");
    let before = counts(&path)?;
    let before_document = store.snapshot()?;
    let db = Connection::open(path.join("project.sqlite"))?;
    db.execute_batch(
        "CREATE TRIGGER fail_prepared_insert BEFORE INSERT ON history
         BEGIN SELECT RAISE(ABORT, 'injected history failure'); END;",
    )?;
    drop(db);
    assert!(matches!(
        store.register_prepared_source(&input, &prepared, None, &cancelled()),
        Err(deadpan_store::StoreError::Database(_))
    ));
    assert_eq!(counts(&path)?, before);
    assert_eq!(store.snapshot()?, before_document);
    let db = Connection::open(path.join("project.sqlite"))?;
    db.execute_batch("DROP TRIGGER fail_prepared_insert")?;
    drop(db);
    let outcome = store.register_prepared_source(&input, &prepared, None, &cancelled())?;
    assert!(outcome.commit.is_some());
    let committed = store.snapshot()?;
    assert!(committed.nodes().contains_key(&NodeId::new("clip")?));
    store.undo(committed.revision_id(), revision("undo-prepared"))?;
    store.redo(store.snapshot()?.revision_id(), revision("redo-prepared"))?;
    store.validate()?;
    Ok(())
}

#[test]
fn prepared_insertion_reconciles_generation_atomically_and_retries_after_cursor_failure() -> Result
{
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let hold = NodeId::new("hold")?;
    let duration = FrameDuration::new(12)?;
    store.commit(&command(
        &store,
        "with-hold",
        Command::Insert {
            parent: NodeId::new("root")?,
            index: 0,
            subtree: Subtree {
                root: hold.clone(),
                nodes: BTreeMap::from([(
                    hold.clone(),
                    BeatNode::hold(
                        "Pause",
                        HoldRecipe {
                            duration,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
                overrides: BTreeMap::new(),
            },
        },
    ))?;
    let original = retain(&mut store, "cfr-bframes.mp4")?;
    let prepared = decoded_from_handle(store.original_import_handle()?, original.clone())?;
    let before_document = store.snapshot()?;
    let current = store.allocate_generation_request(GenerationRequestInput {
        request_id: RequestId::new("bridge")?,
        expected_revision: before_document.revision_id().clone(),
        hold_id: hold,
        context_sha256: Sha256::new("a".repeat(64))?,
        constraints: HoldConstraints {
            video: VideoSpec::new(
                duration,
                before_document.presentation_basis().frame_rate,
                512,
                320,
            )?,
            conditioning: ConditioningMode::Bridge,
            motion: MotionAmount::Still,
        },
        provider: ProviderSelection {
            pack_id: ProviderPackId::new("pack")?,
            pack_version: ProviderPackVersion::new("v1")?,
            runtime_id: RuntimeId::new("runtime")?,
            runtime_version: RuntimeVersion::new("v1")?,
            seed: 1,
        },
    })?;
    assert_eq!(current.relevance, Relevance::Current);
    let input = primary_registration(&store, &original, "registered", "camera", "clip");
    let preview = store.preview_prepared_source_registration(&input, &prepared, &cancelled())?;
    let expected_document = preview.edit.unwrap().forward.apply(&before_document)?;
    let relevance = RelevancePlan {
        from_revision: input.expected_revision.clone(),
        to_revision: input.new_revision.clone(),
        observations: vec![RelevanceObservation {
            request_id: current.request_id.clone(),
            binding: current.binding.clone(),
            // The inserted picture changes the host's resolved bridge context.
            after_context: ContextObservation::Resolved(Sha256::new("b".repeat(64))?),
        }],
    };
    let before_counts = counts(&path)?;
    let db = Connection::open(path.join("project.sqlite"))?;
    let before_state: (String, Option<i64>) = db.query_row(
        "SELECT head_revision,cursor FROM state WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert!(matches!(
        store.register_prepared_source(&input, &prepared, None, &cancelled()),
        Err(StoreError::GenerationRelevanceRequired)
    ));
    assert_eq!(counts(&path)?, before_counts);
    assert_eq!(store.snapshot()?, before_document);
    assert_eq!(store.current_generation_requests()?, vec![current.clone()]);

    // This trigger confirms all four tentative writes precede the cursor update.
    db.execute_batch(
        "CREATE TRIGGER fail_prepared_cursor BEFORE UPDATE OF head_revision,cursor ON state
         WHEN NEW.head_revision='registered'
         BEGIN
             SELECT CASE WHEN
                 (SELECT relevance FROM generation_requests WHERE request_id='bridge')='stale'
                 AND (SELECT count(*) FROM source_qualifications)=1
                 AND EXISTS(SELECT 1 FROM revisions WHERE id='registered')
                 AND EXISTS(SELECT 1 FROM history WHERE revision_id='registered')
             THEN RAISE(ABORT, 'injected cursor failure after tentative writes')
             ELSE RAISE(ABORT, 'tentative writes missing before cursor update') END;
         END;",
    )?;
    assert!(matches!(
        store.register_prepared_source(&input, &prepared, Some(&relevance), &cancelled()),
        Err(StoreError::Database(rusqlite::Error::SqliteFailure(_, Some(message))))
            if message == "injected cursor failure after tentative writes"
    ));
    assert_eq!(counts(&path)?, before_counts);
    assert_eq!(store.snapshot()?, before_document);
    assert_eq!(store.current_generation_requests()?, vec![current.clone()]);
    let after_failure_state: (String, Option<i64>) = db.query_row(
        "SELECT head_revision,cursor FROM state WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(after_failure_state, before_state);

    db.execute_batch("DROP TRIGGER fail_prepared_cursor")?;
    drop(db);
    let outcome =
        store.register_prepared_source(&input, &prepared, Some(&relevance), &cancelled())?;
    assert_eq!(outcome.commit.unwrap().revision_id, input.new_revision);
    assert_eq!(outcome.asset_id, asset("camera"));
    assert_eq!(outcome.qualification, *prepared.receipt().id());
    assert_eq!(
        store.source_qualification(&outcome.qualification)?,
        *prepared.receipt()
    );
    assert_eq!(
        counts(&path)?,
        (
            before_counts.0 + 1,
            before_counts.1 + 1,
            before_counts.2 + 1
        )
    );
    let committed = store.snapshot()?;
    assert_eq!(committed, expected_document);
    assert!(matches!(
        committed.nodes()[&NodeId::new("clip")?].kind,
        NodeKind::Source { .. }
    ));
    assert_eq!(
        committed.assets()[&asset("camera")]
            .source_qualification
            .as_ref(),
        Some(prepared.receipt().id())
    );
    let mut stale = current;
    stale.relevance = Relevance::Stale;
    assert_eq!(
        store.generation_request(&stale.request_id)?,
        Some(stale.clone())
    );
    assert!(store.current_generation_requests()?.is_empty());
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?, committed);
    assert_eq!(reopened.generation_request(&stale.request_id)?, Some(stale));
    Ok(())
}

#[test]
fn prepared_commit_rejects_read_only_wrong_project_and_reopened_sessions() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "cfr-bframes.mp4")?;
    let prepared = decoded_from_handle(store.original_import_handle()?, original.clone())?;
    let input = registration(&store, &original, "registered", "camera");
    let before = counts(&path)?;
    let mut reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(matches!(
        reader.register_prepared_source(&input, &prepared, None, &cancelled()),
        Err(deadpan_store::StoreError::ReadOnly)
    ));
    assert_eq!(counts(&path)?, before);
    drop(reader);
    let other_dir = tempfile::tempdir()?;
    let (_, mut other) = project(other_dir.path())?;
    let other_input = registration(&other, &original, "other", "camera");
    assert!(
        other
            .register_prepared_source(&other_input, &prepared, None, &cancelled())
            .is_err()
    );
    assert_eq!(counts(&path)?, before);
    drop(store);
    let mut reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let reopened_input = registration(&reopened, &original, "reopened", "camera");
    assert!(
        reopened
            .register_prepared_source(&reopened_input, &prepared, None, &cancelled())
            .is_err()
    );
    assert_eq!(counts(&path)?, before);
    Ok(())
}

#[test]
fn linked_relocation_requires_fresh_inventory_proof_even_when_old_path_is_unchanged() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original_path = fixture("cfr-bframes.mp4");
    let original = store
        .retain_original(
            &original_path,
            OriginalOwnership::Linked { bookmark: None },
            limits(),
            &cancelled(),
        )?
        .record;
    let prepared = decoded_from_handle(store.original_import_handle()?, original.clone())?;
    let input = primary_registration(&store, &original, "registered", "camera", "clip");
    let before = counts(&path)?;
    let relocated = scratch.path().join("relocated.mp4");
    fs::copy(&original_path, &relocated)?;
    let current = store.relink_original(
        original.object().content(),
        original.version(),
        LinkedOriginal::new(relocated, None)?,
        limits(),
        &cancelled(),
    )?;
    assert_eq!(current.version(), original.version() + 1);
    assert!(matches!(
        store.register_prepared_source(&input, &prepared, None, &cancelled()),
        Err(deadpan_store::StoreError::OriginalMedia(
            deadpan_store::original_media::OriginalMediaError::VersionConflict { .. }
        ))
    ));
    assert_eq!(counts(&path)?, before);
    let fresh = decoded_from_handle(store.original_import_handle()?, current)?;
    store.register_prepared_source(&input, &fresh, None, &cancelled())?;
    store.validate()?;
    Ok(())
}

#[test]
fn cancelled_prepared_insertion_leaves_no_receipt_or_edit_and_allows_retry() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "cfr-bframes.mp4")?;
    let prepared = decoded_from_handle(store.original_import_handle()?, original.clone())?;
    let input = primary_registration(&store, &original, "registered", "camera", "clip");
    let before = counts(&path)?;
    let document = store.snapshot()?;
    let stop = AtomicBool::new(true);
    assert_eq!(
        store
            .register_prepared_source(&input, &prepared, None, &stop)
            .unwrap_err()
            .code(),
        "OriginalCancelled"
    );
    assert_eq!(counts(&path)?, before);
    assert_eq!(store.snapshot()?, document);
    store.register_prepared_source(&input, &prepared, None, &cancelled())?;
    Ok(())
}
