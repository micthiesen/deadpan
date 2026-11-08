#![cfg(any(target_os = "macos", target_os = "linux"))]

//! Named Original moments admitted against a real measured Original.

use std::{
    error::Error,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

use deadpan_core::{AssetId, NodeId, ProjectDocument, ProjectId, RevisionId};
use deadpan_media::{
    audio_session::{AudioSession, AudioSessionLimits},
    source_index::SourceContentIdentity,
    source_input::VerifiedSourceInput,
    source_qualification::DecodedSourceQualification,
    source_session::{SourceSession, SourceSessionLimits},
};
use deadpan_store::{
    AccessMode, ProjectStore,
    moments::{Action, MomentId, MomentName, MomentTags, Request},
    original_media::{OriginalMediaLimits, OriginalOwnership},
    single_source::SingleSourceInitialization,
    source_registration::PreparedSourceRegistration,
};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn active() -> AtomicBool {
    AtomicBool::new(false)
}

/// A single-Original project initialized from the real offset fixture.
fn ready(parent: &Path) -> Result<(PathBuf, ProjectStore, u64)> {
    let path = parent.join("moments.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("moments")?,
        RevisionId::new("initial")?,
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create_single_source(&path, &document)?;
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/offset-bframes.mp4")
        .canonicalize()?;
    let limits = OriginalMediaLimits::new(2_000_000, Duration::from_secs(10))?;
    let record = store
        .retain_original(&fixture, OriginalOwnership::Managed, limits, &active())?
        .record;
    let mut original =
        store
            .original_import_handle()?
            .snapshot_original(&record, limits, &active())?;
    let input = VerifiedSourceInput::copy_verified(
        &mut original,
        SourceContentIdentity::new(record.sha256(), record.object().byte_length())?,
        2_000_000,
        Duration::from_secs(10),
        &active(),
    )?;
    let video = SourceSession::open_input(
        input.clone(),
        AssetId::new("decode")?,
        SourceSessionLimits::default(),
        &active(),
    )?;
    let audio = AudioSession::open_input(input, 1, AudioSessionLimits::default(), &active())?;
    let decoded = DecodedSourceQualification::from_sessions(Some(&video), Some(&audio))?;
    let source = PreparedSourceRegistration::from_decoded(original, &decoded, &active())?;
    let pictures = source
        .receipt()
        .snapshot()
        .video()
        .ok_or("fixture has pictures")?
        .index()
        .index()
        .frames()
        .len() as u64;
    store.initialize_prepared_source(
        &SingleSourceInitialization {
            expected_revision: RevisionId::new("initial")?,
            new_revision: RevisionId::new("baseline")?,
            new_asset_id: AssetId::new("original")?,
            node: NodeId::new("full-original")?,
            label: "The original".into(),
        },
        &source,
        &active(),
    )?;
    Ok((path, store, pictures))
}

fn create(store: &ProjectStore, id: &str, name: &str, start: u64, end: u64) -> Result<Request> {
    let catalog = store.moment_catalog()?;
    Ok(Request {
        project_id: catalog.project_id,
        expected_version: catalog.version,
        action: Action::Create {
            id: MomentId::new(id)?,
            name: MomentName::new(name)?,
            tags: MomentTags::from_input(["reaction"])?,
            start,
            end,
        },
    })
}

#[test]
fn moments_are_admitted_within_the_ready_originals_pictures_and_persist() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, pictures) = ready(scratch.path())?;
    let head = store.snapshot()?.revision_id().clone();
    for (start, end) in [(3, 3), (5, 2), (0, pictures + 1)] {
        let request = create(&store, "bad", "Bad", start, end)?;
        assert!(store.preview_moment(&request).is_err(), "[{start},{end})");
        assert!(store.apply_moment(&request).is_err(), "[{start},{end})");
    }
    let whole = create(&store, "whole", "Whole Original", 0, pictures)?;
    let preview = store.preview_moment(&whole)?;
    assert_eq!(
        store.moment_catalog()?.version,
        0,
        "preview reserves nothing"
    );
    assert_eq!(store.apply_moment(&whole)?, preview);
    let duplicate = create(&store, "other", "Whole Original", 1, 2)?;
    assert!(store.apply_moment(&duplicate).is_err());
    store.apply_moment(&create(&store, "glance", "Glance", 1, 2)?)?;
    let catalog = store.moment_catalog()?;
    assert_eq!(catalog.version, 2);
    assert_eq!(store.snapshot()?.revision_id(), &head);
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(reopened.moment_catalog()?, catalog);
    reopened.validate_full()?;
    Ok(())
}
