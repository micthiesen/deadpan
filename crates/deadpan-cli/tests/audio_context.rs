#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_cli::audio::{ProjectAudioError, ProjectAudioSession};
use deadpan_core::{
    AssetId, AudioSample, ColorPolicy, ExactRatio, FrameRate, FrozenAudioContext, FrozenAudioInput,
    NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId, SourceAudioMapping,
};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits, SourceAudioSample};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_store::original_media::{OriginalMediaLimits, OriginalOwnership};
use deadpan_store::source_registration::{SourceInsertionRequest, SourceRegistration};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::Value;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn active() -> AtomicBool {
    AtomicBool::new(false)
}

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn asset() -> AssetId {
    AssetId::new("camera").unwrap()
}

fn limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(2_000_000, Duration::from_secs(10)).unwrap()
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures")
        .join(name)
        .canonicalize()
        .unwrap()
}

fn project(parent: &Path) -> Result<(PathBuf, ProjectStore)> {
    let path = parent.join("context.deadpan");
    let document = ProjectDocument::new(
        ProjectId::new("audio-context")?,
        revision("initial"),
        PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    Ok((path.clone(), ProjectStore::create(&path, &document)?))
}

fn register(
    store: &mut ProjectStore,
    path: &Path,
    ownership: OriginalOwnership,
    next: &str,
) -> Result<Vec<[f32; 2]>> {
    let original = store
        .retain_original(path, ownership, limits(), &active())?
        .record;
    let mut snapshot = store.snapshot_original(original.object().content(), limits(), &active())?;
    let audio = AudioSession::open_verified(
        &mut snapshot,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
        1,
        AudioSessionLimits::default(),
        &active(),
    )?;
    let first = audio
        .index()
        .frames()
        .iter()
        .find(|frame| frame.valid_start < frame.valid_end)
        .unwrap()
        .valid_start;
    let original_block = audio.read_samples(
        SourceAudioSample(first),
        256,
        Duration::from_secs(2),
        &active(),
    )?;
    let expected = original_block
        .samples
        .chunks_exact(2)
        .map(|frame| [frame[0], frame[1]])
        .collect();
    let decoded = DecodedSourceQualification::from_sessions(None, Some(&audio))?;
    store.register_source(
        &SourceRegistration {
            expected_revision: store.snapshot()?.revision_id().clone(),
            new_revision: revision(next),
            original: original.object().content().clone(),
            new_asset_id: asset(),
            label: "Measured audio".into(),
            insertion: Some(SourceInsertionRequest {
                parent: node("root"),
                index: 0,
                node: node("clip"),
                label: "Full source audio".into(),
                purpose: Default::default(),
            }),
        },
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    Ok(expected)
}

fn counts(path: &Path) -> Result<(i64, i64, i64)> {
    let database = rusqlite::Connection::open(path.join("project.sqlite"))?;
    Ok(database.query_row(
        "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history), (SELECT count(*) FROM source_qualifications)",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?)
}

fn mismatch(path: &Path, context: &FrozenAudioContext) -> bool {
    matches!(
        ProjectAudioSession::open_context(path, context),
        Err(ProjectAudioError::ContextMismatch)
    )
}

#[test]
fn captured_context_reopens_abandoned_source_after_alias_reuse_without_moving_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = register(
        &mut store,
        &fixture("cfr-bframes.mp4"),
        OriginalOwnership::Managed,
        "first-import",
    )?;
    let first_document = store.snapshot()?;
    let context = FrozenAudioContext::capture(&first_document)?;
    let restored = FrozenAudioContext::from_json(&context.to_json()?)?;
    assert_eq!(restored, context);
    assert_eq!(restored.project_id(), first_document.project_id());
    assert_eq!(restored.revision_id(), first_document.revision_id());
    assert_eq!(
        restored.assets().get(&asset()),
        first_document.assets().get(&asset())
    );

    // The first revision becomes an abandoned branch; the same asset alias then
    // refers to different qualified bytes in the live document.
    store.undo(&revision("first-import"), revision("undone"))?;
    let replacement = register(
        &mut store,
        &fixture("offset-bframes.mp4"),
        OriginalOwnership::Managed,
        "second-import",
    )?;
    assert_ne!(original, replacement);
    let latest = store.snapshot()?;
    assert_ne!(
        latest.assets().get(&asset()),
        first_document.assets().get(&asset())
    );
    assert_eq!(
        store.snapshot_at(&revision("first-import"))?,
        first_document
    );
    assert_eq!(store.snapshot()?, latest);
    let before_counts = counts(&path)?;
    drop(store);

    // Reopening also checks that the committed historical row survives writer
    // shutdown and that read-only context access changes no history state.
    let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(
        reopened.snapshot_at(&revision("first-import"))?,
        first_document
    );
    assert_eq!(reopened.snapshot()?, latest);
    let mut historical = ProjectAudioSession::open_context(&path, &restored)?;
    assert_eq!(historical.revision(), &revision("first-import"));
    assert_eq!(
        historical.read(AudioSample(0), 256, &active())?.samples,
        original
    );
    let time_mapped = historical.read_time_mapped(AudioSample(0), 256, &active())?;
    assert_eq!(time_mapped.samples, original);
    assert_eq!(
        historical.read(AudioSample(90), 21, &active())?.samples,
        original[90..111]
    );
    let mut current = ProjectAudioSession::open(&path)?;
    assert_eq!(
        current.read(AudioSample(0), 256, &active())?.samples,
        replacement
    );
    assert_eq!(reopened.snapshot()?, latest);
    assert_eq!(counts(&path)?, before_counts);
    Ok(())
}

#[test]
fn context_host_rejects_valid_forgery_and_unavailable_revision() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    register(
        &mut store,
        &fixture("cfr-bframes.mp4"),
        OriginalOwnership::Managed,
        "registered",
    )?;
    let before = store.snapshot()?;
    let before_counts = counts(&path)?;
    let context = FrozenAudioContext::capture(&before)?;
    let mut wire: Value = serde_json::from_str(&context.to_json()?)?;

    // The changed label leaves every structural and media bound valid, so the
    // host must compare the entire capture against committed history.
    wire["assets"]["camera"]["label"] = Value::String("Forged label".into());
    let forged_asset = FrozenAudioContext::from_json(&wire.to_string())?;
    assert!(mismatch(&path, &forged_asset));

    // Shift one sample from authored placement into the separate mix offset.
    // At this fixture's 30 fps, one 48 kHz sample is 1/1600 frame. The
    // effective placement remains identical, but authored intent changes.
    let mut wire: Value = serde_json::from_str(&context.to_json()?)?;
    let FrozenAudioInput::Source {
        mapping, offset, ..
    } = &context.inputs()[&node("clip")]
    else {
        panic!("registered clip must have source audio");
    };
    let frames = mapping.duration_frames(context.layout().nodes()[&node("clip")].duration)?;
    wire["inputs"]["clip"]["mapping"] = serde_json::to_value(SourceAudioMapping::Placement {
        start: mapping
            .start_frames()
            .checked_sub(ExactRatio::new(1, 1600)?)?,
        frames,
    })?;
    wire["inputs"]["clip"]["offset"] = serde_json::to_value(AudioSample(offset.0 + 1))?;
    let forged_mapping = FrozenAudioContext::from_json(&wire.to_string())?;
    assert!(mismatch(&path, &forged_mapping));

    let mut wire: Value = serde_json::from_str(&context.to_json()?)?;
    wire["project_id"] = Value::String("other-project".into());
    let wrong_project = FrozenAudioContext::from_json(&wire.to_string())?;
    assert!(mismatch(&path, &wrong_project));

    let mut wire: Value = serde_json::from_str(&context.to_json()?)?;
    wire["revision_id"] = Value::String("initial".into());
    let wrong_existing_revision = FrozenAudioContext::from_json(&wire.to_string())?;
    assert!(mismatch(&path, &wrong_existing_revision));

    let mut wire: Value = serde_json::from_str(&context.to_json()?)?;
    wire["revision_id"] = Value::String("missing-revision".into());
    let missing = FrozenAudioContext::from_json(&wire.to_string())?;
    assert!(ProjectAudioSession::open_context(&path, &missing).is_err());
    assert!(store.snapshot_at(&revision("missing-revision")).is_err());
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    Ok(())
}

#[test]
fn context_read_refuses_changed_linked_original_bytes() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let linked = scratch.path().join("linked.mp4");
    fs::copy(fixture("cfr-bframes.mp4"), &linked)?;
    register(
        &mut store,
        &linked,
        OriginalOwnership::Linked { bookmark: None },
        "registered",
    )?;
    let before = store.snapshot()?;
    let context = FrozenAudioContext::capture(&before)?;
    let before_counts = counts(&path)?;
    let mut bytes = fs::read(&linked)?;
    bytes[100] ^= 1;
    fs::write(&linked, bytes)?;

    let mut session = ProjectAudioSession::open_context(&path, &context)?;
    assert!(session.read(AudioSample(0), 256, &active()).is_err());
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    Ok(())
}
