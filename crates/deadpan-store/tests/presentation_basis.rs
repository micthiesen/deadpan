#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::*;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_store::original_media::{OriginalMediaLimits, OriginalMediaRecord, OriginalOwnership};
use deadpan_store::source_registration::{
    PrimaryGeometryAdoption, SourceInsertionPurpose, SourceInsertionRequest, SourceRegistration,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn asset(value: &str) -> AssetId {
    AssetId::new(value).unwrap()
}
fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn active() -> AtomicBool {
    AtomicBool::new(false)
}
fn limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(2_000_000, Duration::from_secs(10)).unwrap()
}

fn project(parent: &Path) -> Result<(PathBuf, ProjectStore)> {
    let path = parent.join("basis.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("basis")?,
        revision("initial"),
        node("root"),
    )?;
    Ok((path.clone(), ProjectStore::create(&path, &document)?))
}

fn qualify(
    store: &mut ProjectStore,
    folder: &str,
    name: &str,
    video: bool,
    audio: Option<u32>,
) -> Result<(OriginalMediaRecord, DecodedSourceQualification)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests")
        .join(folder)
        .join(name)
        .canonicalize()?;
    let original = store
        .retain_original(&path, OriginalOwnership::Managed, limits(), &active())?
        .record;
    let mut snapshot = store.snapshot_original(original.object().content(), limits(), &active())?;
    let bytes = VerifiedSourceInput::copy_verified(
        &mut snapshot,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
        2_000_000,
        Duration::from_secs(10),
        &active(),
    )?;
    let video = if video {
        Some(SourceSession::open_input(
            bytes.clone(),
            asset("decode"),
            SourceSessionLimits::default(),
            &active(),
        )?)
    } else {
        None
    };
    let audio = audio
        .map(|stream| {
            AudioSession::open_input(bytes, stream, AudioSessionLimits::default(), &active())
        })
        .transpose()?;
    Ok((
        original,
        DecodedSourceQualification::from_sessions(video.as_ref(), audio.as_ref())?,
    ))
}

fn registration(
    store: &ProjectStore,
    original: &OriginalMediaRecord,
    next: &str,
    alias: &str,
    insertion: Option<(&str, SourceInsertionPurpose)>,
) -> Result<SourceRegistration> {
    let current = store.snapshot()?;
    let NodeKind::Sequence { children } = &current.nodes()[current.root()].kind else {
        panic!()
    };
    Ok(SourceRegistration {
        expected_revision: current.revision_id().clone(),
        new_revision: revision(next),
        original: original.object().content().clone(),
        new_asset_id: asset(alias),
        label: alias.into(),
        insertion: insertion.map(|(name, purpose)| SourceInsertionRequest {
            parent: node("root"),
            index: children.len(),
            node: node(name),
            label: name.into(),
            purpose,
        }),
    })
}

fn command(store: &ProjectStore, next: &str, command: Command) -> Result<CommandRequest> {
    let current = store.snapshot()?;
    Ok(CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: revision(next),
        command,
    })
}

#[test]
fn inserted_primary_not_registration_order_selects_basis_and_undo_restores_provisional() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let (first, first_decode) = qualify(&mut store, "fixtures", "cfr-bframes.mp4", true, Some(1))?;
    let (portrait, portrait_decode) = qualify(&mut store, "fixtures", "rotated90.mp4", true, None)?;
    for (original, decoded, next, alias) in [
        (&first, &first_decode, "register-first", "first"),
        (&portrait, &portrait_decode, "register-portrait", "portrait"),
    ] {
        store.register_source(
            &registration(&store, original, next, alias, None)?,
            decoded,
            None,
            limits(),
            &active(),
        )?;
        assert_eq!(
            store.snapshot()?.basis_state().rate_origin,
            FrameRateOrigin::Provisional
        );
    }
    let before = store.snapshot()?;
    let input = registration(
        &store,
        &portrait,
        "insert-portrait",
        "unused",
        Some(("picture", SourceInsertionPurpose::Primary)),
    )?;
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let preview =
        reader.preview_source_registration(&input, &portrait_decode, limits(), &active())?;
    assert_eq!(store.snapshot()?, before);
    let proposed = preview.edit.unwrap().forward.apply(&before)?;
    assert_eq!(
        proposed.presentation_basis().frame_rate,
        FrameRate::new(24, 1)?
    );
    assert_eq!(
        (
            proposed.presentation_basis().width,
            proposed.presentation_basis().height
        ),
        (2, 4)
    );
    assert_eq!(
        proposed.basis_state().primary.as_ref().unwrap().asset,
        asset("portrait")
    );
    assert_eq!(
        proposed.basis_state().rate_origin,
        FrameRateOrigin::PrimarySource
    );
    store.register_source(&input, &portrait_decode, None, limits(), &active())?;
    assert_eq!(store.snapshot()?, proposed);
    let primary = store.snapshot()?;
    store.register_source(
        &registration(
            &store,
            &first,
            "later-video",
            "first",
            Some(("later", SourceInsertionPurpose::Primary)),
        )?,
        &first_decode,
        None,
        limits(),
        &active(),
    )?;
    assert_eq!(store.snapshot()?.basis_state(), primary.basis_state());
    assert_eq!(
        store.snapshot()?.presentation_basis(),
        primary.presentation_basis()
    );
    store.undo(&revision("later-video"), revision("undo-later"))?;
    store.undo(&revision("undo-later"), revision("undo-first"))?;
    assert_eq!(store.snapshot()?.basis_state(), before.basis_state());
    assert_eq!(
        store.snapshot()?.presentation_basis(),
        before.presentation_basis()
    );
    store.redo(&revision("undo-first"), revision("redo-primary"))?;
    store.validate()?;
    drop(reader);
    drop(store);
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?
            .snapshot()?
            .basis_state(),
        primary.basis_state()
    );
    Ok(())
}

#[test]
fn audio_locks_time_and_explicit_geometry_changes_no_frame_sample_or_mark_coordinate() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let (audio, decoded) = qualify(
        &mut store,
        "audio-fixtures",
        "pcm-mono-44100.wav",
        false,
        Some(0),
    )?;
    store.register_source(
        &registration(
            &store,
            &audio,
            "audio",
            "audio",
            Some(("sound", SourceInsertionPurpose::Primary)),
        )?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    assert_eq!(
        store.snapshot()?.basis_state().rate_origin,
        FrameRateOrigin::TimedEdit
    );
    let original_audio = store.snapshot()?.nodes()[&node("sound")].clone();
    store.commit(&command(
        &store,
        "mark",
        Command::SetMark {
            id: MarkId::new("timing")?,
            owner: node("root"),
            label: "Timed coordinate".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Sequence {
                    frame: ProjectFrame(15),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    )?)?;
    let (portrait, decoded) = qualify(&mut store, "fixtures", "rotated90.mp4", true, None)?;
    store.register_source(
        &registration(
            &store,
            &portrait,
            "picture",
            "portrait",
            Some(("picture", SourceInsertionPurpose::Primary)),
        )?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    let before = store.snapshot()?;
    assert_eq!(before.nodes()[&node("sound")], original_audio);
    assert_eq!(
        before.presentation_basis().frame_rate,
        FrameRate::new(30, 1)?
    );
    assert_eq!(
        (
            before.presentation_basis().width,
            before.presentation_basis().height
        ),
        (1920, 1080)
    );
    let sample = before
        .presentation_basis()
        .frame_rate
        .audio_boundary(ProjectFrame(15))?;
    let input = PrimaryGeometryAdoption {
        expected_revision: before.revision_id().clone(),
        new_revision: revision("geometry"),
    };
    // Metadata-based geometry remains previewable when the managed test media
    // is offline. No decoding or byte-availability claim is made here.
    std::fs::remove_file(
        path.join("Media/Originals")
            .join(format!("blake3-{}", portrait.object().content().digest())),
    )?;
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let preview = reader.preview_primary_geometry(&input)?;
    assert_eq!(store.snapshot()?, before);
    assert!(preview.forward.nodes.is_empty());
    assert!(preview.forward.marks.is_empty());
    store.adopt_primary_geometry(&input, None)?;
    let after = store.snapshot()?;
    assert_eq!(
        (
            after.presentation_basis().width,
            after.presentation_basis().height
        ),
        (2, 4)
    );
    assert_eq!(
        after.presentation_basis().frame_rate,
        before.presentation_basis().frame_rate
    );
    assert_eq!(
        after
            .presentation_basis()
            .frame_rate
            .audio_boundary(ProjectFrame(15))?,
        sample
    );
    assert_eq!(after.nodes(), before.nodes());
    assert_eq!(after.marks(), before.marks());
    assert_eq!(
        after.basis_state().geometry_origin,
        GeometryOrigin::PrimarySource
    );
    store.undo(&revision("geometry"), revision("undo-geometry"))?;
    assert_eq!(
        store.snapshot()?.presentation_basis(),
        before.presentation_basis()
    );
    store.redo(&revision("undo-geometry"), revision("redo-geometry"))?;
    store.validate()?;
    Ok(())
}

#[test]
fn secondary_picture_cannot_choose_basis_and_generic_commands_cannot_claim_qualified_adoption()
-> Result {
    let scratch = tempfile::tempdir()?;
    let (_, mut store) = project(scratch.path())?;
    let (original, decoded) = qualify(&mut store, "fixtures", "rotated90.mp4", true, None)?;
    store.register_source(
        &registration(&store, &original, "registered", "portrait", None)?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    let before = store.snapshot()?;
    let record = before.assets()[&asset("portrait")].clone();
    let candidate = decoded.snapshot().basis_candidate()?.unwrap().basis;
    let bypass = command(
        &store,
        "bypass",
        Command::ImportSource {
            id: asset("portrait"),
            asset: record,
            insertion: Some(Box::new(SourceInsertion {
                parent: node("root"),
                index: 0,
                node: node("forged"),
                label: "Forged host claim".into(),
                source: decoded
                    .snapshot()
                    .derive_timing(candidate.frame_rate)?
                    .source_node(asset("portrait")),
            })),
            primary: Some(PrimarySourceImport::Adopt { basis: candidate }),
        },
    )?;
    assert!(matches!(
        store.preview(&bypass),
        Err(StoreError::SourceBasisAdmissionUnavailable)
    ));
    assert!(matches!(
        store.commit(&bypass),
        Err(StoreError::SourceBasisAdmissionUnavailable)
    ));
    assert_eq!(store.snapshot()?, before);
    store.register_source(
        &registration(
            &store,
            &original,
            "secondary",
            "portrait",
            Some(("reaction", SourceInsertionPurpose::Secondary)),
        )?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    let secondary = store.snapshot()?;
    assert_eq!(secondary.presentation_basis(), before.presentation_basis());
    assert_eq!(
        secondary.basis_state().rate_origin,
        FrameRateOrigin::TimedEdit
    );
    assert!(secondary.basis_state().primary.is_none());
    store.register_source(
        &registration(
            &store,
            &original,
            "primary",
            "portrait",
            Some(("primary", SourceInsertionPurpose::Primary)),
        )?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    assert_eq!(
        store
            .snapshot()?
            .basis_state()
            .primary
            .as_ref()
            .unwrap()
            .asset,
        asset("portrait")
    );
    let bypass = command(
        &store,
        "geometry-bypass",
        Command::AdoptPrimaryGeometry {
            width: 2,
            height: 4,
        },
    )?;
    assert!(matches!(
        store.commit(&bypass),
        Err(StoreError::SourceBasisAdmissionUnavailable)
    ));
    store.commit(&command(
        &store,
        "canvas",
        Command::SetCanvas {
            width: 1080,
            height: 1920,
        },
    )?)?;
    assert_eq!(
        store.snapshot()?.presentation_basis().frame_rate,
        FrameRate::new(30, 1)?
    );
    assert_eq!(
        store.snapshot()?.basis_state().geometry_origin,
        GeometryOrigin::Explicit
    );
    Ok(())
}

#[test]
fn failed_insertion_rolls_back_basis_origin_receipt_and_history_together() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let (original, decoded) = qualify(&mut store, "fixtures", "rotated90.mp4", true, None)?;
    let before = store.snapshot()?;
    let input = registration(
        &store,
        &original,
        "retry",
        "portrait",
        Some(("picture", SourceInsertionPurpose::Primary)),
    )?;
    let db = Connection::open(path.join("project.sqlite"))?;
    db.execute_batch("CREATE TRIGGER fail_basis BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'basis history failure'); END;")?;
    assert!(
        store
            .register_source(&input, &decoded, None, limits(), &active())
            .is_err()
    );
    assert_eq!(store.snapshot()?, before);
    let count: i64 = db.query_row("SELECT count(*) FROM source_qualifications", [], |row| {
        row.get(0)
    })?;
    assert_eq!(count, 0);
    db.execute_batch("DROP TRIGGER fail_basis")?;
    store.register_source(&input, &decoded, None, limits(), &active())?;
    assert_eq!(
        store.snapshot()?.basis_state().rate_origin,
        FrameRateOrigin::PrimarySource
    );
    Ok(())
}

#[test]
fn self_consistent_history_cannot_claim_geometry_different_from_source_evidence() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let (original, decoded) = qualify(&mut store, "fixtures", "rotated90.mp4", true, None)?;
    store.register_source(
        &registration(
            &store,
            &original,
            "primary",
            "portrait",
            Some(("picture", SourceInsertionPurpose::Primary)),
        )?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    drop(store);
    Connection::open(path.join("project.sqlite"))?.execute_batch("UPDATE revisions SET document=json_set(document,'$.presentation_basis.width',18) WHERE id='primary'; UPDATE history SET request=json_set(request,'$.command.primary.basis.width',18), edit=json_set(edit,'$.forward.presentation.after.basis.width',18,'$.inverse.presentation.before.basis.width',18) WHERE revision_id='primary';")?;
    assert!(
        matches!(ProjectStore::open(&path,AccessMode::ReadOnly),Err(StoreError::SourceRegistration(message)) if message.contains("measured geometry"))
    );
    Ok(())
}

#[test]
fn imported_initial_snapshot_cannot_claim_false_default_canvas_or_timed_rate() -> Result {
    for corruption in [
        "UPDATE revisions SET document=json_set(document,'$.basis_state.rate_origin','timed_edit','$.presentation_basis.width',1280)",
        "UPDATE revisions SET document=json_set(document,'$.basis_state.rate_origin','timed_edit','$.presentation_basis.frame_rate.numerator',25)",
        "UPDATE revisions SET document=json_set(document,'$.basis_state.rate_origin','explicit')",
    ] {
        let scratch = tempfile::tempdir()?;
        let (path, store) = project(scratch.path())?;
        drop(store);
        Connection::open(path.join("project.sqlite"))?.execute_batch(corruption)?;
        assert!(
            ProjectStore::open(&path, AccessMode::ReadOnly).is_err(),
            "{corruption}"
        );
    }
    Ok(())
}
