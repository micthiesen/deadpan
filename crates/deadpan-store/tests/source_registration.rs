#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::{
    AssetId, ColorPolicy, Command, CommandRequest, FrameRate, NodeId, NodeKind, PresentationBasis,
    ProjectDocument, ProjectId, RevisionId, SourceQualificationId,
};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_store::original_media::{OriginalMediaLimits, OriginalMediaRecord, OriginalOwnership};
use deadpan_store::source_registration::{SourceInsertionRequest, SourceRegistration};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn active() -> AtomicBool {
    AtomicBool::new(false)
}
fn limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(2_000_000, Duration::from_secs(10)).unwrap()
}
fn id(value: &str) -> AssetId {
    AssetId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn project(parent: &Path) -> Result<(PathBuf, ProjectStore)> {
    let path = parent.join("registration.deadpan");
    let document = ProjectDocument::new(
        ProjectId::new("source-import")?,
        revision("initial"),
        PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: FrameRate::new(30000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    Ok((path.clone(), ProjectStore::create(&path, &document)?))
}
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures")
        .join(name)
        .canonicalize()
        .unwrap()
}
fn retain(store: &mut ProjectStore, name: &str) -> Result<OriginalMediaRecord> {
    Ok(store
        .retain_original(
            &fixture(name),
            OriginalOwnership::Managed,
            limits(),
            &active(),
        )?
        .record)
}
fn decode(
    store: &ProjectStore,
    original: &OriginalMediaRecord,
) -> Result<DecodedSourceQualification> {
    let mut snapshot = store.snapshot_original(original.object().content(), limits(), &active())?;
    let input = VerifiedSourceInput::copy_verified(
        &mut snapshot,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
        2_000_000,
        Duration::from_secs(10),
        &active(),
    )?;
    let video = SourceSession::open_input(
        input.clone(),
        id("temporary-decode-alias"),
        SourceSessionLimits::default(),
        &active(),
    )?;
    let audio = AudioSession::open_input(input, 1, AudioSessionLimits::default(), &active())?;
    Ok(DecodedSourceQualification::from_sessions(
        Some(&video),
        Some(&audio),
    )?)
}
fn request(
    store: &ProjectStore,
    original: &OriginalMediaRecord,
    next: &str,
    asset: &str,
    node: Option<&str>,
) -> Result<SourceRegistration> {
    Ok(SourceRegistration {
        expected_revision: store.snapshot()?.revision_id().clone(),
        new_revision: revision(next),
        original: original.object().content().clone(),
        new_asset_id: id(asset),
        label: "Measured source".into(),
        insertion: node.map(|node| SourceInsertionRequest {
            parent: NodeId::new("root").unwrap(),
            index: 0,
            node: NodeId::new(node).unwrap(),
            label: "Inserted source".into(),
        }),
    })
}
fn counts(path: &Path) -> Result<(i64, i64, i64)> {
    let db = Connection::open(path.join("project.sqlite"))?;
    Ok(db.query_row("SELECT (SELECT count(*) FROM revisions),(SELECT count(*) FROM history),(SELECT count(*) FROM source_qualifications)", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)))?)
}

#[test]
fn actual_source_registration_deduplicates_then_inserts_and_survives_undo_relocation() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let baseline = store.snapshot()?;
    let register = request(&store, &original, "register", "camera", None)?;
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let preview = reader.preview_source_registration(&register, &decoded, limits(), &active())?;
    assert!(preview.edit.is_some());
    assert_eq!(counts(&path)?, (1, 0, 0));
    drop(reader);
    let outcome = store.register_source(&register, &decoded, None, limits(), &active())?;
    assert!(outcome.commit.is_some());
    assert_eq!(counts(&path)?, (2, 1, 1));
    let registered = store.snapshot()?;
    assert_eq!(registered.duration()?.frames(), 0);
    assert_eq!(
        registered.presentation_basis(),
        baseline.presentation_basis()
    );
    assert_eq!(
        registered.assets()[&id("camera")]
            .source_qualification
            .as_ref(),
        Some(&outcome.qualification)
    );
    let duplicate = request(
        &store,
        &original,
        "unused-duplicate-revision",
        "different-proposed-id",
        None,
    )?;
    let duplicate = store.register_source(&duplicate, &decoded, None, limits(), &active())?;
    assert_eq!(duplicate.asset_id, id("camera"));
    assert!(duplicate.commit.is_none());
    assert_eq!(counts(&path)?, (2, 1, 1));
    let insertion = request(
        &store,
        &original,
        "insert",
        "another-proposed-id",
        Some("clip"),
    )?;
    store.register_source(&insertion, &decoded, None, limits(), &active())?;
    let inserted = store.snapshot()?;
    assert_eq!(inserted.assets().len(), 1);
    assert_eq!(inserted.duration()?.frames(), 121);
    let NodeKind::Source { source } = &inserted.nodes()[&NodeId::new("clip")?].kind else {
        panic!()
    };
    assert_eq!(
        source.video_mapping.start_frames(),
        deadpan_core::ExactRatio::new(640, 1001)?
    );
    assert_eq!(source.audio.as_ref().unwrap().span.start().ticks, 95072);
    let index = store.source_video_index(inserted.revision_id(), &id("camera"))?;
    assert_eq!(index.asset(), &id("camera"));
    assert_eq!(index.frames()[0].pts, 60060);
    let receipt = store.registered_source(inserted.revision_id(), &id("camera"))?;
    assert_eq!(receipt.id(), &outcome.qualification);
    assert_eq!(
        receipt.snapshot().video().unwrap().index().index().asset(),
        &id("qualified-source")
    );
    store.undo(inserted.revision_id(), revision("undo-insert"))?;
    store.undo(store.snapshot()?.revision_id(), revision("undo-register"))?;
    assert!(store.snapshot()?.assets().is_empty());
    assert_eq!(store.source_qualification(&outcome.qualification)?, receipt);
    store.redo(store.snapshot()?.revision_id(), revision("redo-register"))?;
    store.redo(store.snapshot()?.revision_id(), revision("redo-insert"))?;
    assert_eq!(store.snapshot()?.nodes(), inserted.nodes());
    assert_eq!(store.snapshot()?.assets(), inserted.assets());
    store.validate()?;
    drop(store);
    let moved = scratch.path().join("moved.deadpan");
    fs::rename(path, &moved)?;
    let store = ProjectStore::open(&moved, AccessMode::ReadOnly)?;
    assert_eq!(
        store.source_video_index(&revision("insert"), &id("camera"))?,
        index
    );
    assert_eq!(
        store.registered_source(&revision("insert"), &id("camera"))?,
        receipt
    );
    let reopened = decode(&store, &original)?;
    assert_eq!(reopened.snapshot(), decoded.snapshot());
    Ok(())
}

#[test]
fn reused_asset_alias_retains_both_branch_receipts_and_revision_specific_indexes() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let first = retain(&mut store, "cfr-bframes.mp4")?;
    let first_decode = decode(&store, &first)?;
    let input = request(&store, &first, "first-import", "camera", Some("first-clip"))?;
    let first_result = store.register_source(&input, &first_decode, None, limits(), &active())?;
    let first_index = store.source_video_index(&revision("first-import"), &id("camera"))?;
    store.undo(&revision("first-import"), revision("undo-first"))?;
    let second = retain(&mut store, "vfr.mp4")?;
    let second_decode = decode(&store, &second)?;
    let input = request(
        &store,
        &second,
        "second-import",
        "camera",
        Some("second-clip"),
    )?;
    let second_result = store.register_source(&input, &second_decode, None, limits(), &active())?;
    assert_ne!(first_result.qualification, second_result.qualification);
    let second_index = store.source_video_index(&revision("second-import"), &id("camera"))?;
    assert_ne!(first_index.terminal_end(), second_index.terminal_end());
    assert_eq!(
        store.source_video_index(&revision("first-import"), &id("camera"))?,
        first_index
    );
    store.validate()?;
    drop(store);
    let store = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(
        store
            .registered_source(&revision("first-import"), &id("camera"))?
            .id(),
        &first_result.qualification
    );
    assert_eq!(
        store
            .registered_source(&revision("second-import"), &id("camera"))?
            .id(),
        &second_result.qualification
    );
    drop(store);
    // Only an abandoned branch references this receipt. Current head and core
    // history remain internally consistent; source validation must still fail.
    Connection::open(path.join("project.sqlite"))?.execute(
        "DELETE FROM source_qualifications WHERE id=?1",
        [first_result.qualification.as_str()],
    )?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::SourceRegistration(_))
    ));
    Ok(())
}

#[test]
fn failed_transaction_retains_original_but_rolls_back_receipt_asset_history_and_cursor() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let input = request(
        &store,
        &original,
        "retry-same-revision",
        "camera",
        Some("clip"),
    )?;
    let before = store.snapshot()?;
    let db = Connection::open(path.join("project.sqlite"))?;
    db.execute_batch("CREATE TRIGGER fail_source_history BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'forced history failure'); END;")?;
    assert!(
        store
            .register_source(&input, &decoded, None, limits(), &active())
            .is_err()
    );
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, (1, 0, 0));
    store.snapshot_original(original.object().content(), limits(), &active())?;
    db.execute_batch("DROP TRIGGER fail_source_history")?;
    store.register_source(&input, &decoded, None, limits(), &active())?;
    assert_eq!(counts(&path)?, (2, 1, 1));
    Ok(())
}

#[test]
fn missing_bytes_identity_mismatch_cancelled_stale_and_read_only_requests_preserve_history()
-> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "cfr-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let another = retain(&mut store, "vfr.mp4")?;
    let wrong = decode(&store, &another)?;
    let input = request(&store, &original, "register", "camera", Some("clip"))?;
    assert!(matches!(
        store.register_source(&input, &wrong, None, limits(), &active()),
        Err(StoreError::SourceRegistration(_))
    ));
    assert_eq!(
        store
            .register_source(&input, &decoded, None, limits(), &AtomicBool::new(true))
            .unwrap_err()
            .code(),
        "OriginalCancelled"
    );
    let mut stale = input.clone();
    stale.expected_revision = revision("wrong-head");
    assert!(matches!(
        store.preview_source_registration(&stale, &decoded, limits(), &active()),
        Err(StoreError::RevisionConflict { .. })
    ));
    let mut reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(matches!(
        reader.register_source(&input, &decoded, None, limits(), &active()),
        Err(StoreError::ReadOnly)
    ));
    drop(reader);
    assert_eq!(counts(&path)?, (1, 0, 0));
    // Remove only a test-owned managed object, leaving the immutable evidence
    // token alive. A prior successful decode must not bypass presence checks.
    let object = path
        .join("Media/Originals")
        .join(format!("blake3-{}", original.object().content().digest()));
    assert!(object.exists(), "actual managed namespace: {object:?}");
    fs::remove_file(object)?;
    assert_eq!(
        store
            .register_source(&input, &decoded, None, limits(), &active())
            .unwrap_err()
            .code(),
        "OriginalOffline"
    );
    assert_eq!(counts(&path)?, (1, 0, 0));
    Ok(())
}

#[test]
fn generic_asset_and_initial_snapshot_cannot_bypass_source_admission() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "cfr-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let input = request(&store, &original, "register", "camera", None)?;
    store.register_source(&input, &decoded, None, limits(), &active())?;
    let current = store.snapshot()?;
    let asset = current.assets()[&id("camera")].clone();
    let request = CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: revision("bypass"),
        command: Command::AddAsset {
            id: id("forged-alias"),
            asset,
        },
    };
    assert!(matches!(
        store.preview(&request),
        Err(StoreError::SourceAdmissionUnavailable)
    ));
    assert!(matches!(
        store.commit(&request),
        Err(StoreError::SourceAdmissionUnavailable)
    ));
    let new_path = scratch.path().join("forged.deadpan");
    assert!(matches!(
        ProjectStore::create(&new_path, &current),
        Err(StoreError::SourceAdmissionUnavailable)
    ));
    assert!(!new_path.exists());
    assert_eq!(counts(&path)?, (2, 1, 1));
    Ok(())
}

#[test]
fn tampered_qualification_payload_and_missing_receipt_fail_reopen() -> Result {
    for sql in [
        "UPDATE source_qualifications SET snapshot=CAST(CAST(snapshot AS TEXT)||' ' AS BLOB)",
        "UPDATE source_qualifications SET original_ref=json_set(original_ref,'$.byte_length',1234)",
        "DELETE FROM source_qualifications",
    ] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store) = project(scratch.path())?;
        let original = retain(&mut store, "cfr-bframes.mp4")?;
        let decoded = decode(&store, &original)?;
        let input = request(&store, &original, "register", "camera", None)?;
        let outcome = store.register_source(&input, &decoded, None, limits(), &active())?;
        assert!(
            store
                .source_qualification(&SourceQualificationId::new("0".repeat(64))?)
                .is_err()
        );
        assert!(store.source_qualification(&outcome.qualification).is_ok());
        drop(store);
        Connection::open(path.join("project.sqlite"))?.execute_batch(sql)?;
        assert!(
            ProjectStore::open(&path, AccessMode::ReadOnly).is_err(),
            "{sql}"
        );
    }
    Ok(())
}
