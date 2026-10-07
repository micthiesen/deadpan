#![cfg(target_os = "macos")]

//! Bridge conditioning records what the project picture path actually showed
//! on each side of a pause, measured from the decoded stream (DP-12).
//!
//! Real one-Original projects built through the headless CLI: the
//! `black_pause` recipe over `cfr-bframes.mp4`, an HDR PQ Original that no
//! stated model-input conversion covers, and (with the synthetic worker) a
//! pause whose left neighbour is an accepted generated Hold.

#[allow(dead_code)]
#[path = "preview_export/recipes.rs"]
mod recipes;

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_cli::generation::conditioning;
use deadpan_cli::picture::{PreparedPicture, ProjectPictureSession};
use deadpan_core::{
    AudioTimingId, Command, FrameDuration, HoldAudio, HoldRecipe, HoldVideo, NodeId, ProjectFrame,
    RevisionId, SourceFrameId, SplitIdentities,
};
use deadpan_models::{
    BoundaryPicture, BridgeContext, BridgeMatrix, BridgePrimaries, BridgeRange, BridgeTransfer,
    CANONICAL_BRIDGE_COLOR, ModelInputConversion,
};
use deadpan_source::{DecodeControl, DecodeLimits, SourceDecoder};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::json;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const FIXTURES: &str = "../../native/deadpan-source/tests/fixtures";

fn manifest(inputs: &conditioning::BridgeInputs) -> Result<BridgeContext> {
    Ok(serde_json::from_slice(&inputs.manifest)?)
}

/// The decoder's own stream description and presented PTS list for `file`.
fn decoder_truth(
    file: &Path,
    frames: usize,
) -> Result<(deadpan_source::SourceStreamInfo, Vec<i64>)> {
    let cancelled = AtomicBool::new(false);
    let control = DecodeControl {
        timeout: Duration::from_secs(30),
        cancelled: &cancelled,
    };
    let mut decoder =
        SourceDecoder::open(std::fs::File::open(file)?, DecodeLimits::default(), control)?;
    let mut pts = Vec::new();
    while pts.len() < frames {
        pts.push(
            decoder
                .next_metadata(control)?
                .ok_or("fixture ended early")?
                .pts,
        );
    }
    Ok((decoder.info().clone(), pts))
}

/// The same picture prepared afresh through the project picture path at
/// `revision`, reduced to the evidence conditioning records.
fn picture_path(package: &Path, revision: &RevisionId, frame: i64) -> Result<BoundaryPicture> {
    let cancelled = AtomicBool::new(false);
    let mut session = ProjectPictureSession::open_revision(package, revision, None, &cancelled)?;
    let prepared = session.prepare(ProjectFrame(frame), &cancelled)?;
    let info = session.source_info().cloned();
    let decoded = |id: SourceFrameId, picture: &deadpan_render::Rgba8Frame| -> Result<_> {
        let stream =
            conditioning::measured_stream(info.as_ref().ok_or("retained stream")?, picture);
        Ok(deadpan_models::DecodedBoundary {
            source_frame: id,
            pts: picture.metadata().pts,
            model_input: deadpan_models::model_input_conversion(&stream)
                .map_err(|refusal| refusal.to_string())?,
            stream,
        })
    };
    Ok(match &prepared.picture {
        PreparedPicture::Frame {
            asset,
            qualification,
            id,
            frame: picture,
        } => BoundaryPicture::Original {
            project_frame: frame,
            asset: asset.clone(),
            qualification: qualification.clone(),
            picture: decoded(*id, picture)?,
        },
        PreparedPicture::Generated {
            artifact,
            id,
            frame: picture,
        } => BoundaryPicture::Generated {
            project_frame: frame,
            sampled_asset: artifact.sampled_asset.clone(),
            sampled_object: artifact.sampled_object.clone(),
            provenance: artifact.provenance.clone(),
            picture: decoded(*id, picture)?,
        },
        PreparedPicture::Background => BoundaryPicture::AuthoredBlack {
            project_frame: frame,
        },
    })
}

/// Commit a silent Background Hold `id` of `frames` at Edit `at` through the
/// headless `command` API, returning the new revision.
fn insert_hold(package: &Path, at: i64, id: &str, frames: i64) -> Result<RevisionId> {
    let document = ProjectStore::open(package, AccessMode::ReadOnly)?.snapshot()?;
    let revision = RevisionId::new(format!("{id}-inserted"))?;
    let at = ProjectFrame(at);
    let identities = match document.insert_time_target(at)?.split {
        Some(split) => (0..split.required_ids)
            .map(|index| NodeId::new(format!("{id}-split-{index}")))
            .collect::<std::result::Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };
    let command = Command::InsertTime {
        at,
        hold: HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(frames)?,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
        id: NodeId::new(id)?,
        identities: SplitIdentities { nodes: identities },
        timing: AudioTimingId {
            allocation: revision.clone(),
            ordinal: 0,
        },
    };
    let request = package.with_extension(format!("{id}.json"));
    std::fs::write(
        &request,
        serde_json::to_vec(&json!({
            "protocol": 1,
            "project_id": document.project_id(),
            "expected_revision": document.revision_id(),
            "new_revision": revision,
            "command": command,
        }))?,
    )?;
    recipes::success(&[
        "command",
        package.to_str().ok_or("UTF-8")?,
        "--json",
        request.to_str().ok_or("UTF-8")?,
    ])?;
    Ok(revision)
}

#[test]
fn manifest_records_the_measured_original_pictures_on_both_sides() -> Result {
    let root = tempfile::tempdir()?;
    let fixture = recipes::black_pause(&root.path().join("fixture"))?;
    let revision = RevisionId::new(fixture.revision.clone())?;
    let hold = NodeId::new("black")?;
    let inputs =
        conditioning::prepare(&fixture.package, &revision, &hold, &AtomicBool::new(false))?;
    let context = manifest(&inputs)?;
    assert_eq!(context.schema_version(), 2);
    assert_eq!(context.model_color_space(), CANONICAL_BRIDGE_COLOR);
    assert_eq!(
        context.input_color_interpretation(),
        conditioning::INPUT_COLOR_INTERPRETATION
    );
    let boundaries = context.boundaries().ok_or("measured boundaries")?;

    // Independent decoder truth for the Original (Edit 14 shows ordinal 26,
    // Edit 27 shows ordinal 27).
    let (info, pts) = decoder_truth(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(FIXTURES)
            .join("cfr-bframes.mp4"),
        28,
    )?;
    for (side, project_frame, ordinal) in [(&boundaries.left, 14, 26), (&boundaries.right, 27, 27)]
    {
        let BoundaryPicture::Original {
            project_frame: recorded,
            picture,
            ..
        } = side
        else {
            panic!("an Original frame borders the pause: {side:?}")
        };
        assert_eq!(*recorded, project_frame);
        assert_eq!(picture.source_frame, SourceFrameId(ordinal));
        assert_eq!(picture.pts.ticks, pts[ordinal as usize]);
        assert_eq!(
            (
                picture.pts.time_base.numerator(),
                picture.pts.time_base.denominator()
            ),
            {
                let base =
                    deadpan_core::SourceTimeBase::new(info.time_base_num, info.time_base_den)?;
                (base.numerator(), base.denominator())
            }
        );
        let stream = &picture.stream;
        assert_eq!(stream.codec, info.codec);
        assert_eq!(stream.pixel_format, info.pixel_format);
        assert_eq!((stream.width, stream.height), (info.width, info.height));
        assert_eq!(
            stream.sample_aspect,
            [info.sample_aspect_num, info.sample_aspect_den]
        );
        assert_eq!(stream.rotation_quarter_turns, info.rotation_quarter_turns);
        assert_eq!(stream.decoded_sample_bits, 8);
        let expected_transfer = match info.color.transfer {
            deadpan_source::ColorTransfer::Bt709 => BridgeTransfer::Bt709,
            deadpan_source::ColorTransfer::Srgb => BridgeTransfer::Srgb,
            other => panic!("fixture transfer {other:?}"),
        };
        assert_eq!(stream.color.transfer, expected_transfer);
        assert_eq!(stream.color.primaries, BridgePrimaries::Bt709);
        assert_eq!(info.color.primaries, deadpan_source::ColorPrimaries::Bt709);
        assert_eq!(
            stream.color.matrix,
            match info.color.matrix {
                deadpan_source::ColorMatrix::Rgb => BridgeMatrix::Rgb,
                deadpan_source::ColorMatrix::Bt709 => BridgeMatrix::Bt709,
                deadpan_source::ColorMatrix::Bt601 => BridgeMatrix::Bt601,
                deadpan_source::ColorMatrix::Bt2020NonConstant => BridgeMatrix::Bt2020Ncl,
            }
        );
        assert_eq!(
            stream.color.range,
            match info.color.range {
                deadpan_source::ColorRange::Limited => BridgeRange::Limited,
                deadpan_source::ColorRange::Full => BridgeRange::Full,
            }
        );
        assert_eq!(
            picture.model_input,
            if expected_transfer == BridgeTransfer::Srgb {
                ModelInputConversion::SrgbCodesUnchanged
            } else {
                ModelInputConversion::Rec709ToSrgb
            }
        );
        // Re-derivation: the picture path at the request's origin revision
        // reports exactly the recorded evidence, receipt identity included.
        assert_eq!(
            &picture_path(&fixture.package, &revision, project_frame)?,
            side
        );
    }
    // Conditioning is deterministic for one revision.
    let again = conditioning::prepare(&fixture.package, &revision, &hold, &AtomicBool::new(false))?;
    assert_eq!(again.manifest, inputs.manifest);
    Ok(())
}

#[test]
fn an_hdr_original_is_refused_with_a_truthful_reason() -> Result {
    let root = tempfile::tempdir()?;
    let directory = root.path().canonicalize()?;
    let media = directory.join("original.mp4");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(FIXTURES)
            .join("hdr-pq-av.mp4"),
        &media,
    )?;
    let package = directory.join("hdr.deadpan");
    recipes::success(&[
        "project",
        "create-original",
        package.to_str().ok_or("UTF-8")?,
        media.to_str().ok_or("UTF-8")?,
    ])?;
    let revision = insert_hold(&package, 30, "pause", 12)?;
    let error = conditioning::prepare(
        &package,
        &revision,
        &NodeId::new("pause")?,
        &AtomicBool::new(false),
    )
    .expect_err("PQ BT.2020 pictures have no stated model-input conversion");
    assert!(error.contains("before the pause"), "{error}");
    assert!(error.contains("HDR (PQ/HLG)"), "{error}");
    Ok(())
}

/// A pause right after an accepted generated Hold records that Hold's sampled
/// master (canonical sRGB) as its left picture, not invented source metadata.
#[cfg(feature = "synthetic-worker")]
#[test]
fn a_generated_neighbour_is_recorded_as_its_artifact() -> Result {
    let root = tempfile::tempdir()?;
    let fixture = match recipes::generated::generated_pause(&root.path().join("fixture")) {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("skipped: {error}");
            return Ok(());
        }
    };
    // The generated Hold occupies Edit [15, 27); a new pause at 27 has it on
    // its left and Original 27 (Edit 39) on its right.
    let revision = insert_hold(&fixture.package, 27, "after", 12)?;
    let inputs = conditioning::prepare(
        &fixture.package,
        &revision,
        &NodeId::new("after")?,
        &AtomicBool::new(false),
    )?;
    let context = manifest(&inputs)?;
    let boundaries = context.boundaries().ok_or("measured boundaries")?;
    let BoundaryPicture::Generated {
        project_frame,
        picture,
        ..
    } = &boundaries.left
    else {
        panic!("the accepted Hold borders the pause: {:?}", boundaries.left)
    };
    assert_eq!(*project_frame, 26);
    assert_eq!(picture.stream.color, CANONICAL_BRIDGE_COLOR);
    assert_eq!(picture.stream.codec, "ffv1");
    assert_eq!(
        picture.model_input,
        ModelInputConversion::SrgbCodesUnchanged
    );
    assert!(matches!(
        boundaries.right,
        BoundaryPicture::Original {
            project_frame: 39,
            ..
        }
    ));
    for (side, frame) in [(&boundaries.left, 26), (&boundaries.right, 39)] {
        assert_eq!(&picture_path(&fixture.package, &revision, frame)?, side);
    }
    Ok(())
}
