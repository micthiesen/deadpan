use super::support::{Result, hex, require};
use deadpan_cli::picture::{PreparedPicture, ProjectPictureError, ProjectPictureSession};
use deadpan_core::{
    AssetId, Command, CommandRequest, NodeId, ProjectDocument, ProjectFrame, ProjectId, RevisionId,
    SourceFrameId,
};
use deadpan_media::{
    audio_session::{AudioSession, AudioSessionLimits},
    source_index::SourceContentIdentity,
    source_input::VerifiedSourceInput,
    source_qualification::DecodedSourceQualification,
    source_session::{SourceSession, SourceSessionLimits},
};
use deadpan_store::{
    AccessMode, ProjectStore,
    original_media::{
        OriginalAvailability, OriginalMediaLimits, OriginalMediaRecord, OriginalOwnership,
    },
    single_source::SingleSourceInitialization,
    source_registration::PreparedSourceRegistration,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

pub const FIXTURE_SHA256: &str = "5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918";
pub fn fixture_path() -> Result<PathBuf> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        .canonicalize()?)
}
pub fn limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(2_000_000, Duration::from_secs(30)).expect("fixed fixture limits")
}
fn active() -> AtomicBool {
    AtomicBool::new(false)
}

pub fn seed(package: &Path, source: &Path) -> Result<OriginalMediaRecord> {
    let initial = RevisionId::new("relink-initial")?;
    let baseline = RevisionId::new("relink-baseline")?;
    let renamed = RevisionId::new("relink-renamed")?;
    let document = ProjectDocument::new_automatic(
        ProjectId::new("relink-cross-volume")?,
        initial.clone(),
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create_single_source(package, &document)?;
    let record = store
        .retain_original(
            source,
            OriginalOwnership::linked_at(source),
            limits(),
            &active(),
        )?
        .record;
    require(
        !record.managed()
            && record.version() == 1
            && record
                .linked()
                .and_then(|link| link.bookmark())
                .is_some_and(|bookmark| !bookmark.is_empty()),
        "fixture is not a linked-only Original with a system bookmark",
    )?;
    let mut original =
        store
            .original_import_handle()?
            .snapshot_original(&record, limits(), &active())?;
    // Admission retains the prepared snapshot's freshness guard. This separate
    // bounded input supplies real picture/audio measurements before committing.
    let input = VerifiedSourceInput::copy_verified(
        &mut original,
        SourceContentIdentity::new(record.sha256(), record.object().byte_length())?,
        2_000_000,
        Duration::from_secs(30),
        &active(),
    )?;
    let video = SourceSession::open_input(
        input.clone(),
        AssetId::new("original")?,
        SourceSessionLimits {
            opening_timeout: Duration::from_secs(30),
            maximum_index_frames: 1000,
            maximum_index_bytes: 4 * 1024 * 1024,
            maximum_seek_frames: 1000,
            ..SourceSessionLimits::default()
        },
        &active(),
    )?;
    require(
        video.index().index().frames().len() == 120,
        "pinned Original picture count changed",
    )?;
    let audio = AudioSession::open_input(
        input,
        1,
        AudioSessionLimits {
            opening_timeout: Duration::from_secs(30),
            maximum_cache_bytes: 16 * 1024 * 1024,
            ..AudioSessionLimits::default()
        },
        &active(),
    )?;
    let decoded = DecodedSourceQualification::from_sessions(Some(&video), Some(&audio))?;
    let prepared = PreparedSourceRegistration::from_decoded(original, &decoded, &active())?;
    let outcome = store.initialize_prepared_source(
        &SingleSourceInitialization {
            expected_revision: initial,
            new_revision: baseline.clone(),
            new_asset_id: AssetId::new("original")?,
            node: NodeId::new("full-original")?,
            label: "Volume Original".into(),
        },
        &prepared,
        &active(),
    )?;
    require(
        outcome.commit.is_some(),
        "single-Original initialization did not commit",
    )?;
    store.commit(&CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: baseline,
        new_revision: renamed.clone(),
        command: Command::Rename {
            node: NodeId::new("full-original")?,
            label: "Temporary edit".into(),
        },
    })?;
    store.undo(&renamed, RevisionId::new("relink-undone")?)?;
    require(
        store.history_availability()? == (false, true),
        "fixture lacks its protected baseline and redo path",
    )?;
    store.validate_full()?;
    Ok(record)
}

pub struct Pictures {
    pub evidence: Value,
    frames: Vec<deadpan_render::Rgba8Frame>,
}
impl Pictures {
    pub fn equals(&self, other: &Self) -> bool {
        self.evidence == other.evidence
            && self.frames.len() == other.frames.len()
            && self.frames.iter().zip(&other.frames).all(|(left, right)| {
                left.metadata() == right.metadata()
                    && left.sample_depth() == right.sample_depth()
                    && left.bytes() == right.bytes()
            })
    }
}

pub fn pictures(package: &Path, record: &OriginalMediaRecord) -> Result<Pictures> {
    let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
    require(
        store.original_record(record.object().content())?.as_ref() == Some(record),
        "picture Original identity changed",
    )?;
    let document = store.snapshot()?;
    require(
        document.duration()?.frames() >= 120,
        "Original baseline does not cover the three selected frames",
    )?;
    let mut session =
        ProjectPictureSession::open_revision(package, document.revision_id(), None, &active())?;
    let mut records = Vec::new();
    let mut frames = Vec::new();
    for ordinal in [0, 59, 119] {
        let prepared = session.prepare(ProjectFrame(ordinal), &active())?;
        let PreparedPicture::Frame {
            asset,
            qualification,
            id,
            frame,
        } = prepared.picture
        else {
            return Err(
                "Original picture preparation invented a background or generated picture".into(),
            );
        };
        require(
            id == SourceFrameId(u64::try_from(ordinal)?) && asset == AssetId::new("original")?,
            "prepared project picture names another source frame or asset",
        )?;
        let metadata = frame.metadata();
        records.push(
            json!({"asset":asset,"qualification":qualification,"source_frame":id,
            "project_id":prepared.project_id,"revision_id":prepared.revision_id,"project_frame":prepared.project_frame,
            "canvas":prepared.canvas,"frame_rate":prepared.frame_rate,"pts":metadata.pts,
            "width":metadata.width,"height":metadata.height,"sample_depth":format!("{:?}",frame.sample_depth()),
            "row_stride_bytes":metadata.row_stride_bytes,"color":format!("{:?}",metadata.color),
            "sample_aspect_ratio":[metadata.sample_aspect_ratio.numerator(),metadata.sample_aspect_ratio.denominator()],
            "rotation":format!("{:?}",metadata.rotation),"pixel_bytes":frame.bytes().len(),
            "pixels_sha256":hex(Sha256::digest(frame.bytes()))}),
        );
        frames.push(frame);
    }
    Ok(Pictures {
        evidence: json!({"original_object":record.object(),"original_sha256":hex(record.sha256()),"frames":records}),
        frames,
    })
}

pub fn offline(package: &Path, record: &OriginalMediaRecord) -> Result<Value> {
    let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
    let current = store
        .original_record(record.object().content())?
        .ok_or("Original record missing")?;
    require(
        current == *record,
        "offline inspection changed Original record",
    )?;
    let availability = store.original_availability(&current)?;
    require(
        availability == OriginalAvailability::Missing,
        "unmounted Original is not Missing",
    )?;
    let error = store
        .snapshot_original(record.object().content(), limits(), &active())
        .err()
        .ok_or("offline Original invented a readable snapshot")?;
    require(
        error.code() == "OriginalOffline",
        "offline Original returned an unexpected diagnostic",
    )?;
    let picture_error =
        ProjectPictureSession::open_revision(package, &store.head_revision()?, None, &active())
            .and_then(|mut session| session.prepare(ProjectFrame(0), &active()))
            .err()
            .ok_or("offline Original invented a prepared project picture")?;
    require(
        matches!(&picture_error,ProjectPictureError::Store(error) if error.code()=="OriginalOffline"),
        "offline project picture returned an unexpected diagnostic",
    )?;
    require(
        store.history_availability()? == (false, true),
        "offline inspection lost redo or baseline",
    )?;
    store.validate_full()?;
    Ok(
        json!({"availability":availability,"error":{"code":error.code(),"message":error.to_string()},
        "readable_snapshot":false,"prepared_picture":false,"picture_error":picture_error.to_string(),
        "original":current,"can_undo":false,"can_redo":true}),
    )
}

pub fn relinked(
    package: &Path,
    before: &OriginalMediaRecord,
    path: &Path,
) -> Result<OriginalMediaRecord> {
    let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
    let record = store
        .original_record(before.object().content())?
        .ok_or("Original record missing")?;
    require(
        record.version() == 2
            && !record.managed()
            && record.object() == before.object()
            && record.sha256() == before.sha256()
            && record.label() == before.label()
            && record.linked().is_some_and(|link| {
                link.path() == path && link.bookmark().is_some_and(|bookmark| !bookmark.is_empty())
            }),
        "relink changed Original identity or did not move location to version 2",
    )?;
    require(
        store.original_availability(&record)? == OriginalAvailability::Present,
        "relinked Original is not Present",
    )?;
    require(
        store.history_availability()? == (false, true),
        "relink changed history availability",
    )?;
    store.validate_full()?;
    Ok(record)
}
