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
    AudioTimingId, Command, CommandRequest, ExactRatio, FrameDuration, HoldAudio, HoldRecipe,
    HoldVideo, IterationId, NodeId, NodeKind, PitchPolicy, ProjectFrame, RepeatEditBranch,
    RepeatEditStep, RevisionId, ScopedNodeTarget, SourceFrameId, SplitIdentities,
};
use deadpan_models::{
    BoundaryClock, BoundaryPicture, BridgeContext, BridgeMatrix, BridgePrimaries, BridgeRange,
    BridgeTransfer, CANONICAL_BRIDGE_COLOR, ModelInputConversion,
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
    let document = ProjectStore::open(package, AccessMode::ReadOnly)?.snapshot_at(revision)?;
    let clock = BoundaryClock::Definition {
        project_id: document.project_id().clone(),
        revision_id: revision.clone(),
        definition: document.root().clone(),
        position: ExactRatio::new(i128::from(frame) * 2 + 1, 2)?,
    };
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
            clock: clock.clone(),
            asset: asset.clone(),
            qualification: qualification.clone(),
            picture: decoded(*id, picture)?,
        },
        PreparedPicture::Generated {
            artifact,
            id,
            frame: picture,
        } => BoundaryPicture::Generated {
            clock: clock.clone(),
            sampled_asset: artifact.sampled_asset.clone(),
            sampled_object: artifact.sampled_object.clone(),
            provenance: artifact.provenance.clone(),
            picture: decoded(*id, picture)?,
        },
        PreparedPicture::Background => BoundaryPicture::AuthoredBlack {
            clock: clock.clone(),
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
    assert_eq!(context.schema_version(), 5);
    let geometry = context.geometry().ok_or("captured geometry")?;
    let expected_crop = deadpan_models::RasterRect::new(99, 0, 569, 320)?;
    assert_eq!(geometry.presentation, expected_crop);
    assert_eq!(geometry.left_content, Some(expected_crop));
    assert_eq!(geometry.right_content, Some(expected_crop));
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
            clock: recorded,
            picture,
            ..
        } = side
        else {
            panic!("an Original frame borders the pause: {side:?}")
        };
        assert!(
            matches!(recorded, BoundaryClock::Definition { position, revision_id, .. }
            if *position == ExactRatio::new(i128::from(project_frame) * 2 + 1, 2)? && revision_id == &revision)
        );
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
fn saved_target_capture_is_bound_to_the_revision_and_retained_decoded_pictures() -> Result {
    use deadpan_core::{AttentionTarget, TargetId, TargetRegion};
    use deadpan_jobs::{GenerationOptions, GenerationTarget};
    use deadpan_models::{CapturedRegionBoundary, RegionCapture};
    let root = tempfile::tempdir()?;
    let fixture = recipes::black_pause(&root.path().join("fixture"))?;
    let before = RevisionId::new(fixture.revision.clone())?;
    let hold = NodeId::new("black")?;
    let initial = conditioning::prepare(&fixture.package, &before, &hold, &AtomicBool::new(false))?;
    let context = manifest(&initial)?;
    let BoundaryPicture::Original { asset, .. } = &context.boundaries().ok_or("boundaries")?.left
    else {
        return Err("Original boundary".into());
    };
    let document = ProjectStore::open(&fixture.package, AccessMode::ReadOnly)?.snapshot()?;
    let target_id = TargetId::new("hand")?;
    let target = AttentionTarget {
        label: "Hand".into(),
        asset: asset.clone(),
        span: document.assets()[asset].video.ok_or("video span")?,
        region: TargetRegion {
            center: [500_000, 500_000],
            size: [200_000, 200_000],
        },
        samples: vec![],
        corrections: vec![],
        provenance: None,
    };
    let revision = RevisionId::new("target-captured")?;
    let request = root.path().join("set-target.json");
    std::fs::write(
        &request,
        serde_json::to_vec(
            &json!({"protocol":1,"project_id":document.project_id(),"expected_revision":before,"new_revision":revision,"command":Command::SetTarget {id:target_id.clone(),target:target.clone()}}),
        )?,
    )?;
    recipes::success(&[
        "command",
        fixture.package.to_str().ok_or("path")?,
        "--json",
        request.to_str().ok_or("path")?,
    ])?;
    let controls = GenerationOptions {
        region_target: GenerationTarget::Saved(target_id.clone()),
        ..Default::default()
    };
    let inputs = conditioning::prepare_with_options(
        &fixture.package,
        &revision,
        &hold,
        &controls,
        &AtomicBool::new(false),
    )?;
    assert_eq!(inputs.constraints.region_target, Some(target_id));
    let captured = manifest(&inputs)?;
    assert_eq!(inputs.left_png, initial.left_png);
    assert_eq!(inputs.right_png, initial.right_png);
    let region = captured.region().ok_or("region capture")?;
    let RegionCapture::Selected { left, right, .. } = region else {
        return Err("selected capture".into());
    };
    let (
        CapturedRegionBoundary::Available { point: lp, .. },
        CapturedRegionBoundary::Available { point: rp, .. },
    ) = (left.as_ref(), right.as_ref())
    else {
        return Err("available selected capture".into());
    };
    let boundaries = captured.boundaries().ok_or("boundaries")?;
    assert_eq!(
        lp.ticks,
        deadpan_core::ExactRatio::integer(boundaries.left.decoded().ok_or("left")?.pts.ticks)
    );
    assert_eq!(
        rp.ticks,
        deadpan_core::ExactRatio::integer(boundaries.right.decoded().ok_or("right")?.pts.ticks)
    );
    let seeds = region
        .seeds(
            boundaries,
            captured.geometry().ok_or("geometry")?,
            [768, 320],
        )?
        .ok_or("seeds")?;
    assert!((seeds.left.width() - 0.2 * 569.0 / 768.0).abs() < 1e-12);
    let missing = conditioning::prepare_with_options(
        &fixture.package,
        &before,
        &hold,
        &controls,
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(missing.contains("not saved in this revision"), "{missing}");
    assert_eq!(
        ProjectStore::open(&fixture.package, AccessMode::ReadOnly)?.head_revision()?,
        revision
    );
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
    let BoundaryPicture::Generated { clock, picture, .. } = &boundaries.left else {
        panic!("the accepted Hold borders the pause: {:?}", boundaries.left)
    };
    assert!(
        matches!(clock, BoundaryClock::Definition { position, .. } if *position == ExactRatio::new(53, 2)?)
    );
    assert_eq!(picture.stream.color, CANONICAL_BRIDGE_COLOR);
    assert_eq!(picture.stream.codec, "ffv1");
    assert_eq!(
        picture.model_input,
        ModelInputConversion::SrgbCodesUnchanged
    );
    assert!(matches!(boundaries.right, BoundaryPicture::Original { .. }));
    for (side, frame) in [(&boundaries.left, 26), (&boundaries.right, 39)] {
        assert_eq!(&picture_path(&fixture.package, &revision, frame)?, side);
    }
    let BoundaryPicture::Generated { sampled_object, .. } = &boundaries.left else {
        unreachable!()
    };
    // A cold definition read must verify accepted media again, never replace a
    // missing generated neighbor with the Hold's deterministic fallback.
    std::fs::remove_file(
        fixture
            .package
            .join("Media/Generated")
            .join(format!("blake3-{}", sampled_object.content().digest())),
    )?;
    assert!(
        conditioning::prepare(
            &fixture.package,
            &revision,
            &NodeId::new("after")?,
            &AtomicBool::new(false)
        )
        .is_err()
    );
    Ok(())
}

fn commit(package: &Path, name: &str, command: Command) -> Result<RevisionId> {
    let mut store = ProjectStore::open(package, AccessMode::ReadWrite)?;
    let document = store.snapshot()?;
    let revision = RevisionId::new(name)?;
    store.commit(&CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision.clone(),
        command,
    })?;
    Ok(revision)
}

#[test]
fn repeat_and_retime_condition_the_complete_definition_with_identical_raw_pixels() -> Result {
    for repeat in [false, true] {
        let root = tempfile::tempdir()?;
        let fixture = recipes::black_pause(&root.path().join("fixture"))?;
        let original_revision = RevisionId::new(fixture.revision.clone())?;
        let hold = NodeId::new("black")?;
        let original = conditioning::prepare(
            &fixture.package,
            &original_revision,
            &hold,
            &AtomicBool::new(false),
        )?;
        let document = ProjectStore::open(&fixture.package, AccessMode::ReadOnly)?.snapshot()?;
        let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
            panic!("root Sequence")
        };
        let local = NodeId::new("local")?;
        commit(
            &fixture.package,
            "grouped",
            Command::Group {
                parent: document.root().clone(),
                start: 0,
                end: children.len(),
                id: local.clone(),
                label: "Local".into(),
            },
        )?;
        let wrapper = NodeId::new("wrapper")?;
        let command = if repeat {
            Command::WrapRepeat {
                node: local.clone(),
                id: wrapper.clone(),
                plays: 3,
                gap: None,
                anchor_policy: Default::default(),
            }
        } else {
            Command::WrapRetime {
                node: local.clone(),
                id: wrapper.clone(),
                duration: FrameDuration::new(21)?,
                pitch: PitchPolicy::Preserve,
            }
        };
        let revision = commit(&fixture.package, "wrapped", command)?;
        let branches = if repeat {
            vec![
                RepeatEditBranch::Default,
                RepeatEditBranch::Play {
                    iteration: IterationId {
                        allocation: revision.clone(),
                        ordinal: 2,
                    },
                },
            ]
        } else {
            Vec::new()
        };
        let targets = if repeat {
            branches
                .into_iter()
                .map(|branch| ScopedNodeTarget {
                    node: hold.clone(),
                    repeats: vec![RepeatEditStep {
                        repeat: wrapper.clone(),
                        branch,
                    }],
                })
                .collect::<Vec<_>>()
        } else {
            vec![ScopedNodeTarget {
                node: hold.clone(),
                repeats: Vec::new(),
            }]
        };
        for target in targets {
            let inputs = conditioning::prepare_scoped_with_options(
                &fixture.package,
                &revision,
                &target,
                &Default::default(),
                &AtomicBool::new(false),
            )?;
            assert_eq!(inputs.plan, original.plan);
            assert_eq!(inputs.constraints.video.frames(), FrameDuration::new(12)?);
            assert_eq!(inputs.left_png, original.left_png);
            assert_eq!(inputs.right_png, original.right_png);
            let context = manifest(&inputs)?;
            assert_eq!(context.schema_version(), 5);
            for (side, position) in [
                (&context.boundaries().unwrap().left, 29),
                (&context.boundaries().unwrap().right, 55),
            ] {
                assert_eq!(
                    side.clock(),
                    &BoundaryClock::Definition {
                        project_id: document.project_id().clone(),
                        revision_id: revision.clone(),
                        definition: local.clone(),
                        position: ExactRatio::new(position, 2)?,
                    }
                );
            }
            assert!(
                conditioning::prepare_scoped_with_options(
                    &fixture.package,
                    &revision,
                    &target,
                    &Default::default(),
                    &AtomicBool::new(true)
                )
                .unwrap_err()
                .contains("cancelled")
            );
        }
        if !repeat {
            // An ordinary caller under Retime consumes exactly the same scoped path.
            let inputs =
                conditioning::prepare(&fixture.package, &revision, &hold, &AtomicBool::new(false))?;
            assert_eq!(inputs.left_png, original.left_png);
        } else {
            // No omitted Repeat scope may silently choose one of the three plays.
            assert!(
                conditioning::prepare(&fixture.package, &revision, &hold, &AtomicBool::new(false))
                    .is_err()
            );
        }
    }
    Ok(())
}

#[test]
fn definition_edges_refuse_even_when_outer_root_neighbors_exist() -> Result {
    for missing_left in [false, true] {
        let root = tempfile::tempdir()?;
        let fixture = recipes::black_pause(&root.path().join("fixture"))?;
        let document = ProjectStore::open(&fixture.package, AccessMode::ReadOnly)?.snapshot()?;
        let hold = NodeId::new("black")?;
        let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
            panic!("root Sequence")
        };
        let index = children
            .iter()
            .position(|child| child == &hold)
            .ok_or("Hold child")?;
        assert!(index > 0 && index + 1 < children.len());
        commit(
            &fixture.package,
            "grouped",
            Command::Group {
                parent: document.root().clone(),
                start: if missing_left { index } else { 0 },
                end: if missing_left {
                    children.len()
                } else {
                    index + 1
                },
                id: NodeId::new("local")?,
                label: "At edge".into(),
            },
        )?;
        let revision = commit(
            &fixture.package,
            "retimed",
            Command::WrapRetime {
                node: NodeId::new("local")?,
                id: NodeId::new("retime")?,
                duration: FrameDuration::new(40)?,
                pitch: PitchPolicy::Preserve,
            },
        )?;
        let error =
            conditioning::prepare(&fixture.package, &revision, &hold, &AtomicBool::new(false))
                .unwrap_err();
        assert!(error.contains("definition edge"), "{error}");
        assert!(
            error.contains(if missing_left { "before" } else { "after" }),
            "{error}"
        );
    }
    Ok(())
}
