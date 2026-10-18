use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::{
    AssetRecord, AttentionTarget, BeatNode, CapturedCanvas, CapturedFit, CapturedFraming,
    ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, Framing, FramingClock,
    FramingPose, FramingValue, GeneratedContentId, HoldAudio, HoldRecipe, HoldVideo, NodeId,
    PitchPolicy, PresentationBasis, ProjectFrame, ProjectId, RevisionId, SourceFrameId, SourceSpan,
    Subtree, TargetId, TargetRegion,
};
use deadpan_jobs::{
    AxisLimits, BridgeCapability, BridgeGenerationPlan, DimensionLimits, FrameCountFormula,
    NativeDimensions,
};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_store::original_media::{OriginalMediaLimits, OriginalOwnership};
use deadpan_store::source_registration::{SourceInsertionRequest, SourceRegistration};

use super::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn asset(value: &str) -> AssetId {
    AssetId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn q(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn active() -> AtomicBool {
    AtomicBool::new(false)
}
fn initial() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new("semantic-framing").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap()
}
fn staged(document: &ProjectDocument, command: Command, name: &str) -> ProjectDocument {
    deadpan_core::apply_with_result(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        },
    )
    .unwrap()
    .1
}
fn site(owner: &str, frame: i64, original_center: bool) -> SemanticFramingSite {
    SemanticFramingSite {
        node: node(owner),
        frame: ProjectFrame(frame),
        original_center,
    }
}

struct Fixture {
    _scratch: tempfile::TempDir,
    source: PathBuf,
    store: ProjectStore,
    decoded_metadata: FrameMetadata,
}

impl Fixture {
    /// Only setup opens and decodes media. Tests remove the linked Original
    /// before entering the decoder-free resolver and can stage uncommitted edits.
    fn source(name: &str) -> Result<Self> {
        let scratch = tempfile::tempdir()?;
        let source = scratch.path().join(name);
        std::fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/fixtures")
                .join(name),
            &source,
        )?;
        let mut store = ProjectStore::create(&scratch.path().join("framing.deadpan"), &initial())?;
        let limits = OriginalMediaLimits::new(2_000_000, Duration::from_secs(10))?;
        let original = store
            .retain_original(
                &source,
                OriginalOwnership::Linked { bookmark: None },
                limits,
                &active(),
            )?
            .record;
        let mut snapshot =
            store.snapshot_original(original.object().content(), limits, &active())?;
        let input = VerifiedSourceInput::copy_verified(
            &mut snapshot,
            SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
            2_000_000,
            Duration::from_secs(10),
            &active(),
        )?;
        let mut video = SourceSession::open_input(
            input.clone(),
            asset("original"),
            SourceSessionLimits::default(),
            &active(),
        )?;
        let audio = video
            .info()
            .audio_streams
            .first()
            .map(|stream| {
                AudioSession::open_input(
                    input,
                    stream.stream_index,
                    AudioSessionLimits::default(),
                    &active(),
                )
            })
            .transpose()?;
        let decoded = DecodedSourceQualification::from_sessions(Some(&video), audio.as_ref())?;
        let picture = video.frame(SourceFrameId(0), Duration::from_secs(10), &active())?;
        let decoded_metadata =
            *crate::picture::source_to_render_frame(picture, video.info())?.metadata();
        store.register_source(
            &SourceRegistration {
                expected_revision: revision("initial"),
                new_revision: revision("registered"),
                original: original.object().content().clone(),
                new_asset_id: asset("original"),
                label: name.into(),
                insertion: Some(SourceInsertionRequest {
                    parent: node("root"),
                    index: 0,
                    node: node("source"),
                    label: "Original".into(),
                    purpose: Default::default(),
                }),
            },
            &decoded,
            None,
            limits,
            &active(),
        )?;
        Ok(Self {
            _scratch: scratch,
            source,
            store,
            decoded_metadata,
        })
    }
}

#[test]
fn semantic_framing_uses_qualified_sar_rotation_aperture_and_staged_nested_poses_without_media()
-> Result {
    for name in ["anamorphic.mkv", "rotated90.mp4", "aperture-fractional.mp4"] {
        let fixture = Fixture::source(name)?;
        let document = fixture.store.snapshot()?;
        let plan = RenderPlan::compile(&document)?;
        let picture = plan.picture(ProjectFrame(0))?;
        assert_eq!(
            provider_metadata(&fixture.store, &document, &picture.picture)?,
            fixture.decoded_metadata,
            "{name}"
        );
        let grouped = staged(
            &document,
            Command::Group {
                parent: node("root"),
                start: 0,
                end: 1,
                id: node("owner"),
                label: "Owner".into(),
            },
            "grouped",
        );
        let inner = staged(
            &grouped,
            Command::SetFraming {
                node: node("source"),
                framing: Some(Framing::static_pose(FramingPose::new(
                    q(1, 4),
                    q(1, 2),
                    ExactRatio::ONE,
                )?)?),
            },
            "inner",
        );
        let owner_pose = FramingPose::new(q(2, 3), q(1, 2), q(3, 2))?;
        let framed = staged(
            &inner,
            Command::SetFraming {
                node: node("owner"),
                framing: Some(Framing::static_pose(owner_pose)?),
            },
            "owner-framed",
        );
        std::fs::remove_file(&fixture.source)?;
        let state = resolve(&fixture.store, &framed, &site("owner", 0, true))?;
        assert_eq!(
            state.current, owner_pose,
            "{name}: staged pose, not store head"
        );
        assert_eq!(
            state.original_center,
            Some([q(3, 4), q(1, 2)]),
            "{name}: input before owner's own pose"
        );
        assert!(resolve(&fixture.store, &framed, &site("absent", 0, true)).is_err());
        assert_eq!(fixture.store.head_revision()?, revision("registered"));
    }
    Ok(())
}

#[test]
fn semantic_framing_keeps_captured_hold_geometry_and_refuses_a_clipped_center() -> Result {
    let fixture = Fixture::source("cfr-bframes.mp4")?;
    let document = fixture.store.snapshot()?;
    let timestamp = fixture.decoded_metadata.pts;
    let captured = |pose| {
        CapturedFraming::capture(
            None,
            CapturedCanvas {
                width: 320,
                height: 180,
                fit: CapturedFit::Fit,
                layers: vec![Some(pose)],
            },
        )
        .unwrap()
    };
    let insert = |document: &ProjectDocument, name: &str, context| {
        staged(
            document,
            Command::Insert {
                parent: node("root"),
                index: 0,
                subtree: Subtree {
                    root: node(name),
                    nodes: BTreeMap::from([(
                        node(name),
                        BeatNode::hold(
                            name,
                            HoldRecipe {
                                duration: frames(3),
                                picture_context: Some(context),
                                video: HoldVideo::Freeze {
                                    asset: asset("original"),
                                    timestamp,
                                },
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
            name,
        )
    };
    let held = insert(
        &document,
        "hold",
        captured(FramingPose::new(q(1, 4), q(1, 2), ExactRatio::ONE)?),
    );
    let changed = staged(
        &held,
        Command::SetFraming {
            node: node("source"),
            framing: Some(Framing::static_pose(FramingPose::new(
                ExactRatio::ZERO,
                q(1, 2),
                ExactRatio::integer(4),
            )?)?),
        },
        "provider-framing-changed",
    );
    std::fs::remove_file(&fixture.source)?;
    assert_eq!(
        resolve(&fixture.store, &changed, &site("hold", 0, true))?.original_center,
        Some([q(3, 4), q(1, 2)])
    );
    let clipped = insert(
        &document,
        "clipped",
        captured(FramingPose::new(
            ExactRatio::ZERO,
            q(1, 2),
            ExactRatio::integer(4),
        )?),
    );
    let error = resolve(&fixture.store, &clipped, &site("clipped", 0, true)).unwrap_err();
    assert!(error.message.contains("center is clipped"));
    assert_eq!(
        resolve(&fixture.store, &clipped, &site("clipped", 0, false))?.current,
        FramingPose::identity()
    );
    Ok(())
}

#[test]
fn semantic_framing_uses_live_follow_pose_at_retimed_source_clock_instead_of_fallback() -> Result {
    let fixture = Fixture::source("cfr-bframes.mp4")?;
    let document = fixture.store.snapshot()?;
    let span = document.assets()[&asset("original")].video.unwrap();
    let target = TargetId::new("subject")?;
    let targeted = staged(
        &document,
        Command::SetTarget {
            id: target.clone(),
            target: AttentionTarget {
                label: "Subject".into(),
                asset: asset("original"),
                span,
                region: TargetRegion {
                    center: [250_000, 500_000],
                    size: [100_000, 100_000],
                },
                samples: vec![],
                corrections: vec![],
                provenance: None,
                display_aspect: None,
            },
        },
        "targeted",
    );
    let fallback = FramingPose::new(q(9, 10), q(1, 2), ExactRatio::integer(3))?;
    let followed = staged(
        &targeted,
        Command::SetFraming {
            node: node("source"),
            framing: Some(Framing {
                clock: FramingClock::OwnerOutput,
                value: FramingValue::Follow {
                    target,
                    scale: ExactRatio::integer(2),
                    fallback,
                },
            }),
        },
        "followed",
    );
    let retimed = staged(
        &followed,
        Command::WrapRetime {
            node: node("source"),
            id: node("retime"),
            duration: frames(60),
            pitch: PitchPolicy::Preserve,
        },
        "retimed",
    );
    std::fs::remove_file(&fixture.source)?;
    let state = resolve(&fixture.store, &retimed, &site("source", 10, false))?;
    assert_eq!(
        state.current,
        FramingPose::new(q(1, 4), q(1, 2), ExactRatio::integer(2))?
    );
    assert_ne!(state.current, fallback);
    let sample = RenderPlan::compile(&retimed)?.picture(ProjectFrame(10))?;
    let layer = sample
        .framing
        .iter()
        .find(|layer| layer.instance.node == node("source"))
        .unwrap();
    assert_eq!(state.current, layer.pose.unwrap());
    assert_ne!(layer.local_position, q(21, 2));
    Ok(())
}

#[test]
fn semantic_framing_projects_through_default_gap_provider_clip_before_ordinary_owner() -> Result {
    let fixture = Fixture::source("cfr-bframes.mp4")?;
    let document = fixture.store.snapshot()?;
    let gap_frame = RenderPlan::compile(&document)?.duration().frames();
    let repeated = staged(
        &document,
        Command::WrapRepeat {
            node: node("source"),
            id: node("repeat"),
            plays: 2,
            gap: Some(HoldRecipe {
                duration: frames(2),
                picture_context: None,
                video: HoldVideo::Freeze {
                    asset: asset("original"),
                    timestamp: fixture.decoded_metadata.pts,
                },
                audio: HoldAudio::Silence,
            }),
            anchor_policy: Default::default(),
        },
        "repeated",
    );
    let grouped = staged(
        &repeated,
        Command::Group {
            parent: node("root"),
            start: 0,
            end: 1,
            id: node("owner"),
            label: "Owner".into(),
        },
        "gap-grouped",
    );
    let framed = staged(
        &grouped,
        Command::SetFraming {
            node: node("repeat"),
            framing: Some(Framing::static_pose(FramingPose::new(
                q(1, 4),
                q(1, 2),
                ExactRatio::ONE,
            )?)?),
        },
        "repeat-framed",
    );
    std::fs::remove_file(&fixture.source)?;
    let sample = RenderPlan::compile(&framed)?.picture(ProjectFrame(gap_frame))?;
    assert!(sample.gap_after.is_some());
    let state = resolve(&fixture.store, &framed, &site("owner", gap_frame, true))?;
    assert_eq!(state.original_center, Some([q(3, 4), q(1, 2)]));
    assert_eq!(state.current, FramingPose::identity());
    Ok(())
}

#[test]
fn semantic_framing_rejects_blank_and_changed_qualified_provider_contracts() -> Result {
    let fixture = Fixture::source("cfr-bframes.mp4")?;
    let document = fixture.store.snapshot()?;
    assert!(provider_metadata(&fixture.store, &document, &Picture::Background).is_err());
    assert!(provider_metadata(&fixture.store, &document, &Picture::Blank).is_err());
    let mut changed: serde_json::Value = serde_json::from_str(&document.to_json()?)?;
    changed["assets"]["original"]["content_hash"] =
        serde_json::json!(object('d').content().to_string());
    let changed = ProjectDocument::from_json(&serde_json::to_string(&changed)?)?;
    let error = resolve(&fixture.store, &changed, &site("source", 0, false)).unwrap_err();
    assert!(
        error
            .message
            .contains("differs from its saved qualification")
    );
    Ok(())
}

fn object(value: char) -> GeneratedObjectRef {
    GeneratedObjectRef::new(
        GeneratedContentId::new(value.to_string().repeat(64)).unwrap(),
        1,
    )
    .unwrap()
}

#[test]
fn semantic_generated_framing_uses_retained_raster_and_exact_crop_and_rejects_changed_admission()
-> Result {
    // This is a metadata-contract test, not evidence of generated media or an
    // acceptance. The production caller obtains `accepted` and `plan` only
    // from the store's validated immutable accepted-origin receipt.
    let rate = FrameRate::new(30_000, 1001)?;
    let capability = BridgeCapability::new(
        true,
        FrameRate::new(24, 1)?,
        FrameCountFormula::new(8, 1, 25, 97)?,
        DimensionLimits::new(AxisLimits::new(8, 8, 1)?, AxisLimits::new(4, 4, 1)?),
    );
    let bridge =
        BridgeGenerationPlan::new(frames(30), rate, &capability, NativeDimensions::new(8, 4)?)?;
    let artifact = GeneratedArtifact {
        sampled_asset: asset("sampled"),
        sampled_object: object('a'),
        native_asset: asset("native"),
        native_object: object('b'),
        provenance: object('c'),
        sampling: bridge.sampling_map()?.into(),
        content_aspect: Some([1, 1]),
    };
    let add = |document: &ProjectDocument,
               id: AssetId,
               object: &GeneratedObjectRef,
               count: FrameDuration,
               end| {
        staged(
            document,
            Command::AddAsset {
                id,
                asset: AssetRecord {
                    label: "Generated".into(),
                    source_qualification: None,
                    content_hash: object.content().to_string(),
                    video: Some(
                        SourceSpan::new(
                            SourceTimestamp {
                                ticks: 0,
                                time_base: SourceTimeBase::new(1, 1000).unwrap(),
                            },
                            SourceTimestamp {
                                ticks: end,
                                time_base: SourceTimeBase::new(1, 1000).unwrap(),
                            },
                        )
                        .unwrap(),
                    ),
                    audio: None,
                    still_image: false,
                    frame_count: Some(count),
                },
            },
            if object == &artifact.sampled_object {
                "sampled-added"
            } else {
                "native-added"
            },
        )
    };
    let accepted = add(
        &initial(),
        artifact.sampled_asset.clone(),
        &artifact.sampled_object,
        frames(30),
        1001,
    );
    let accepted = add(
        &accepted,
        artifact.native_asset.clone(),
        &artifact.native_object,
        artifact.sampling.native_frame_count(),
        1041,
    );
    let plan = GenerationPlan::Bridge(bridge);
    let metadata = generated_metadata(
        &accepted,
        &accepted,
        &plan,
        &artifact,
        SourceTimeBase::new(1, 1000)?,
        29,
    )?;
    assert_eq!([metadata.width, metadata.height], [4, 4]);
    assert_eq!(metadata.pts.ticks, 968);
    let raw = FrameMetadata {
        width: 8,
        height: 4,
        row_stride_bytes: 32,
        ..metadata
    };
    let picture = deadpan_render::Rgba8Frame::new(raw, vec![255; 8 * 4 * 4])?;
    assert_eq!(
        *crate::picture::fill_canvas_aspect(picture, [1, 1])?.metadata(),
        metadata
    );
    assert!(
        generated_metadata(
            &accepted,
            &accepted,
            &plan,
            &artifact,
            SourceTimeBase::new(1, 1000)?,
            30
        )
        .is_err()
    );
    let mut changed: serde_json::Value = serde_json::from_str(&accepted.to_json()?)?;
    changed["assets"]["sampled"]["content_hash"] =
        serde_json::json!(object('d').content().to_string());
    let changed = ProjectDocument::from_json(&serde_json::to_string(&changed)?)?;
    assert!(
        generated_metadata(
            &changed,
            &accepted,
            &plan,
            &artifact,
            SourceTimeBase::new(1, 1000)?,
            0
        )
        .is_err()
    );
    Ok(())
}
