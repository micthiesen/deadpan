use super::*;
use std::collections::BTreeMap;

use deadpan_core::{
    AssetId, AssetRecord, AttentionTarget, AudioSample, AudioTreatments, BeatNode, ClipGain,
    ColorPolicy, Cutaway, CutawayFit, ExactRatio, ExactSourceSpan, FrameRange, Framing,
    FramingPose, GainDb, HoldAudio, HoldRecipe, HoldVideo, IndexedSourceFrame, LinkRelation,
    NodeId, PitchPolicy, PresentationBasis, ProjectFrame, ProjectId, RetimePurpose, RevisionId,
    SourceAudioMapping, SourceFrameId, SourceFrameIndex, SourceNode, SourceQualificationId,
    SourceSpan, SourceTimeBase, SourceTimestamp, SourceVideo, SourceVideoMapping, TargetCorrection,
    TargetRegion, TerminalProvenance,
};
use deadpan_plan::{DefinitionPictureSpan, Picture};
use serde_json::json;

use crate::generation_pictures::GenerationPictureSupport;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn asset() -> AssetId {
    AssetId::new("original").unwrap()
}
fn q(n: i64, d: i64) -> ExactRatio {
    ExactRatio::new(i128::from(n), i128::from(d)).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn rate() -> FrameRate {
    FrameRate::new(24, 1).unwrap()
}
fn clock() -> SourceTimeBase {
    SourceTimeBase::new(1, 48).unwrap()
}
fn span(start: i64, end: i64) -> SourceSpan {
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base: clock(),
        },
        SourceTimestamp {
            ticks: end,
            time_base: clock(),
        },
    )
    .unwrap()
}
fn target() -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: id("pause"),
        repeats: vec![],
    }
}
fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Pause",
        HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}
fn source(frames: i64, first: i64) -> BeatNode {
    let mut node = hold(frames);
    node.kind = NodeKind::Source {
        source: SourceNode {
            duration: duration(frames),
            edit_window: None,
            video: SourceVideo::Stream {
                asset: asset(),
                span: span(first, first + frames * 2),
            },
            video_mapping: SourceVideoMapping::FitBeat,
            audio: None,
            audio_mapping: SourceAudioMapping::FitBeat,
            link: LinkRelation::Independent,
            audio_offset: AudioSample(0),
        },
    };
    node
}
fn document(roots: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("input-fixture").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 640,
            height: 480,
            frame_rate: rate(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", roots.iter().map(|name| id(name)).collect()),
    );
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = json!({"original": AssetRecord {
        source_qualification: None, label: "Synthetic measured fixture".into(),
        content_hash: "a".repeat(64), video: Some(span(0, 256)), audio: None,
        still_image: false, frame_count: Some(duration(128)),
    }});
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn ordinary() -> ProjectDocument {
    document(
        &["group"],
        vec![
            (
                "group",
                BeatNode::sequence("Group", vec![id("left"), id("pause"), id("right")]),
            ),
            ("left", source(12, 0)),
            ("pause", hold(2)),
            ("right", source(12, 60)),
        ],
    )
}
fn edit(
    document: &ProjectDocument,
    revision: &str,
    change: impl FnOnce(&mut serde_json::Value),
) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["revision_id"] = json!(revision);
    change(&mut wire);
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

/// A pure fixture observer over explicitly supplied presentation observations.
/// It neither persists a qualification nor grants source or generated admission.
struct MeasuredPictures {
    index: SourceFrameIndex,
}
impl MeasuredPictures {
    fn new() -> Self {
        Self {
            index: SourceFrameIndex::new(
                asset(),
                clock(),
                (0..128)
                    .map(|ordinal| IndexedSourceFrame {
                        identity: SourceFrameId(ordinal),
                        pts: (ordinal * 2) as i64,
                        reported_duration: Some(2),
                        keyframe: true,
                        seek_from: None,
                        decode_timestamp: None,
                    })
                    .collect(),
                256,
                TerminalProvenance::Explicit,
            )
            .unwrap(),
        }
    }
    fn at(frame: u64) -> GenerationPictureIdentity {
        GenerationPictureIdentity::Original {
            qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
            frame: SourceFrameId(frame),
        }
    }
}
impl GenerationPictures for MeasuredPictures {
    fn identity(
        &self,
        _: &ProjectDocument,
        picture: &Picture,
    ) -> Result<GenerationPictureIdentity, StoreError> {
        match picture {
            Picture::Background | Picture::Blank => Ok(GenerationPictureIdentity::AuthoredBlack),
            Picture::Source { .. } | Picture::Freeze { .. } => Ok(Self::at(
                picture
                    .select_source_frame(&self.index)
                    .map_err(plan_error)?
                    .identity
                    .0,
            )),
            _ => Err(invalid("fixture only observes its supplied Original index")),
        }
    }
    fn support(
        &self,
        document: &ProjectDocument,
        span: &DefinitionPictureSpan,
    ) -> Result<GenerationPictureSupport, StoreError> {
        match &span.start.picture {
            Picture::Background | Picture::Blank => {
                let picture = self.identity(document, &span.start.picture)?;
                Ok(GenerationPictureSupport {
                    first: picture.clone(),
                    last: picture,
                })
            }
            Picture::Source { .. } | Picture::Freeze { .. } => {
                let (first, last) = span.source_ordinals(&self.index).map_err(plan_error)?;
                Ok(GenerationPictureSupport {
                    first: Self::at(first.0),
                    last: Self::at(last.0),
                })
            }
            _ => Err(invalid("fixture only observes its supplied Original index")),
        }
    }
}
fn capture_spec(direction: ExtensionDirection) -> GenerationCaptureSpec {
    GenerationCaptureSpec::Extension {
        direction,
        native_rate: FrameRate::new(12, 1).unwrap(),
        context_frames: 3,
        policy: ExtensionCapturePolicy::TemporalContextV1,
    }
}
fn context(document: &ProjectDocument, direction: ExtensionDirection) -> ScopedHoldContext {
    RenderPlan::compile(document)
        .unwrap()
        .scoped_hold_context(
            &ScopedHoldContextRequest {
                target: target(),
                direction,
                native_rate: FrameRate::new(12, 1).unwrap(),
                frame_count: 3,
            },
            BoundaryQueryLimits::default(),
        )
        .unwrap()
}
fn capture(document: &ProjectDocument, direction: ExtensionDirection) -> GenerationInputBinding {
    crate::generation_inputs::GenerationInputCapture::capture_with_plan(
        document,
        &RenderPlan::compile(document).unwrap(),
        &target(),
        capture_spec(direction),
        None,
        &MeasuredPictures::new(),
        &mut InputCaptureBudget::default(),
    )
    .unwrap()
}
fn samples(binding: &GenerationInputBinding) -> &[RelativeGenerationPicture] {
    let GenerationInputs::Extension { samples, .. } = &binding.inputs else {
        panic!("expected extension")
    };
    samples
}
fn support(binding: &GenerationInputBinding) -> &[GenerationInputSupport] {
    let GenerationInputs::Extension { support, .. } = &binding.inputs else {
        panic!("expected extension")
    };
    support
}

#[test]
fn both_extension_directions_capture_exact_relative_native_spacing_and_opposite() {
    let document = ordinary();
    for (direction, positions, frames, opposite_position, opposite_frame) in [
        (ExtensionDirection::FromLeft, [-4, -2, 0], [7, 9, 11], 3, 30),
        (
            ExtensionDirection::FromRight,
            [0, 2, 4],
            [30, 32, 34],
            -3,
            11,
        ),
    ] {
        let captured = capture(&document, direction);
        assert_eq!(captured.duration, duration(2));
        assert_eq!(captured.frame_rate, rate());
        assert_eq!(captured.canvas, [640, 480]);
        assert_eq!(captured.capture_spec(), capture_spec(direction));
        assert_eq!(
            captured,
            crate::generation_inputs::GenerationInputCapture::from_context(
                &document,
                &context(&document, direction),
                capture_spec(direction),
                &MeasuredPictures::new()
            )
            .unwrap()
        );
        let GenerationInputs::Extension {
            samples,
            opposite,
            support,
            terminal,
            ..
        } = captured.inputs
        else {
            unreachable!()
        };
        let expected: Vec<_> = positions
            .into_iter()
            .zip(frames)
            .map(|(position, frame)| RelativeGenerationPicture {
                position: q(position, 1),
                picture: MeasuredPictures::at(frame),
            })
            .collect();
        assert_eq!(samples, expected);
        assert_eq!(
            opposite,
            Some(RelativeGenerationPicture {
                position: q(opposite_position, 1),
                picture: MeasuredPictures::at(opposite_frame)
            })
        );
        assert_eq!(terminal, *expected.last().unwrap());
        assert_eq!(
            support,
            vec![GenerationInputSupport {
                start: q(positions[0], 1),
                end_exclusive: q(positions[2], 1),
                first: MeasuredPictures::at(frames[0]),
                last: MeasuredPictures::at(frames[2])
            }]
        );
    }
}

#[test]
fn group_move_gain_and_camera_do_not_change_model_input_binding() {
    let before = ordinary();
    let moved = edit(&before, "moved", |wire| {
        wire["nodes"]["root"]["kind"]["children"] = json!(["prefix", "group"]);
        wire["nodes"]["prefix"] = serde_json::to_value(hold(7)).unwrap();
    });
    let gain = edit(&moved, "gain", |wire| {
        wire["nodes"]["group"]["audio_treatments"] =
            serde_json::to_value(AudioTreatments::from_clip_gain(
                ClipGain::new(GainDb::new(-6000).unwrap(), false, vec![], vec![]).unwrap(),
            ))
            .unwrap();
    });
    let camera = edit(&gain, "camera", |wire| {
        wire["nodes"]["group"]["framing"] = serde_json::to_value(
            Framing::static_pose(FramingPose::new(q(2, 3), q(1, 2), q(3, 2)).unwrap()).unwrap(),
        )
        .unwrap();
    });
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let expected = capture(&before, direction);
        for after in [&moved, &gain, &camera] {
            assert_eq!(capture(after, direction), expected);
        }
        assert_ne!(
            context(&before, direction).pictures[0].position,
            context(&moved, direction).pictures[0].position
        );
        assert_ne!(
            context(&gain, direction).pictures[0].framing,
            context(&camera, direction).pictures[0].framing
        );
    }
}

#[test]
fn hidden_cutaways_change_support_while_all_model_samples_remain_equal() {
    let before = ordinary();
    for (direction, node, start) in [
        (ExtensionDirection::FromLeft, "left", 8),
        (ExtensionDirection::FromRight, "right", 1),
    ] {
        let changed = edit(&before, "hidden-cutaway", |wire| {
            wire["nodes"][node]["cutaways"] = json!([Cutaway {
                range: FrameRange::new(ProjectFrame(start), ProjectFrame(start + 1)).unwrap(),
                asset: asset(),
                selection: ExactSourceSpan::from(span(120, 122)),
                fit: CutawayFit::Hold,
                removed: false,
            }]);
        });
        let original = capture(&before, direction);
        let changed = capture(&changed, direction);
        assert_eq!(samples(&changed), samples(&original));
        assert_ne!(support(&changed), support(&original));
        assert!(
            support(&changed)
                .iter()
                .any(|span| span.first == MeasuredPictures::at(60))
        );
        assert_ne!(changed, original);
    }
}

#[test]
fn hidden_source_jump_and_non_anchor_sample_changes_remain_relevant() {
    let before = ordinary();
    let jump = edit(&before, "hidden-source-jump", |wire| {
        wire["nodes"]["left"] = serde_json::to_value(BeatNode::sequence(
            "Pieces",
            vec![id("a"), id("jump"), id("b")],
        ))
        .unwrap();
        for (name, value) in [
            ("a", source(8, 0)),
            ("jump", source(1, 120)),
            ("b", source(3, 18)),
        ] {
            wire["nodes"][name] = serde_json::to_value(value).unwrap();
        }
    });
    let original = capture(&before, ExtensionDirection::FromLeft);
    let changed = capture(&jump, ExtensionDirection::FromLeft);
    assert_eq!(samples(&changed), samples(&original));
    assert_ne!(support(&changed), support(&original));
    let non_anchor = edit(&before, "non-anchor", |wire| {
        wire["nodes"]["left"]["cutaways"] = json!([Cutaway {
            range: FrameRange::new(ProjectFrame(7), ProjectFrame(8)).unwrap(),
            asset: asset(),
            selection: ExactSourceSpan::from(span(120, 122)),
            fit: CutawayFit::Hold,
            removed: false,
        }]);
    });
    let changed = capture(&non_anchor, ExtensionDirection::FromLeft);
    assert_eq!(samples(&changed).last(), samples(&original).last());
    assert_ne!(samples(&changed)[0], samples(&original)[0]);
    assert_ne!(changed, original);
}

#[test]
fn closed_context_terminal_at_a_cut_is_distinct_from_half_open_support() {
    let before = ordinary();
    let cut = edit(&before, "terminal-cut", |wire| {
        let mut speed = hold(12);
        speed.kind = NodeKind::Retime {
            purpose: RetimePurpose::Edit,
            child: id("pieces"),
            duration: duration(12),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(24)).unwrap(),
            pitch: PitchPolicy::Preserve,
        };
        wire["nodes"]["right"] = serde_json::to_value(speed).unwrap();
        wire["nodes"]["pieces"] =
            serde_json::to_value(BeatNode::sequence("Cut", vec![id("a"), id("b")])).unwrap();
        wire["nodes"]["a"] = serde_json::to_value(source(9, 0)).unwrap();
        wire["nodes"]["b"] = serde_json::to_value(source(15, 100)).unwrap();
    });
    let binding = capture(&cut, ExtensionDirection::FromRight);
    let GenerationInputs::Extension {
        samples,
        support,
        terminal,
        ..
    } = binding.inputs
    else {
        unreachable!()
    };
    assert_eq!(support.len(), 1);
    assert_eq!(support[0].end_exclusive, q(4, 1));
    assert_eq!(support[0].last, MeasuredPictures::at(8));
    assert_eq!(
        terminal,
        RelativeGenerationPicture {
            position: q(4, 1),
            picture: MeasuredPictures::at(50)
        }
    );
    assert_eq!(samples.last(), Some(&terminal));
}

#[test]
fn absent_edges_are_distinct_from_authored_black_and_single_context_has_no_fake_support() {
    let pictures = MeasuredPictures::new();
    let alone = document(&["pause"], vec![("pause", hold(2))]);
    let black = document(
        &["black", "pause"],
        vec![("black", hold(12)), ("pause", hold(2))],
    );
    let absent =
        crate::generation_inputs::GenerationInputCapture::capture(&alone, &target(), &pictures)
            .unwrap();
    let present =
        crate::generation_inputs::GenerationInputCapture::capture(&black, &target(), &pictures)
            .unwrap();
    assert_eq!(
        absent.inputs,
        GenerationInputs::Bridge {
            left: None,
            right: None
        }
    );
    assert_eq!(
        present.inputs,
        GenerationInputs::Bridge {
            left: Some(GenerationPictureIdentity::AuthoredBlack),
            right: None
        }
    );
    assert_ne!(absent, present);
    assert_eq!(present.capture_spec(), GenerationCaptureSpec::Bridge);
    let plan = RenderPlan::compile(&black).unwrap();
    let boundaries = plan
        .scoped_hold_boundaries(&target(), BoundaryQueryLimits::default())
        .unwrap();
    assert_eq!(
        crate::generation_inputs::GenerationInputCapture::from_boundaries(
            &black,
            &boundaries,
            &pictures
        )
        .unwrap(),
        present
    );
    let capture = GenerationCaptureSpec::Extension {
        direction: ExtensionDirection::FromLeft,
        native_rate: rate(),
        context_frames: 1,
        policy: ExtensionCapturePolicy::TemporalContextV1,
    };
    let one = crate::generation_inputs::GenerationInputCapture::capture_with_plan(
        &black,
        &plan,
        &target(),
        capture,
        None,
        &pictures,
        &mut InputCaptureBudget::default(),
    )
    .unwrap();
    let GenerationInputs::Extension {
        samples,
        support,
        opposite,
        terminal,
        ..
    } = one.inputs
    else {
        unreachable!()
    };
    assert_eq!(
        samples,
        vec![RelativeGenerationPicture {
            position: ExactRatio::ZERO,
            picture: GenerationPictureIdentity::AuthoredBlack
        }]
    );
    assert_eq!(terminal, samples[0]);
    assert!(support.is_empty());
    assert!(opposite.is_none());
    assert!(
        crate::generation_inputs::GenerationInputCapture::capture_with_plan(
            &alone,
            &RenderPlan::compile(&alone).unwrap(),
            &target(),
            capture,
            None,
            &pictures,
            &mut InputCaptureBudget::default()
        )
        .is_err()
    );
}

#[test]
fn region_corrections_are_bound_even_when_raw_context_is_unchanged() {
    let before = ordinary();
    let region_id = TargetId::new("speaker").unwrap();
    let region = TargetRegion {
        center: [500_000, 500_000],
        size: [200_000, 300_000],
    };
    let with_region = edit(&before, "target", |wire| {
        wire["targets"][region_id.as_str()] = json!(AttentionTarget {
            label: "Speaker".into(),
            asset: asset(),
            span: span(0, 256),
            region,
            samples: vec![],
            corrections: vec![],
            provenance: None,
        });
    });
    let corrected = edit(&with_region, "corrected", |wire| {
        wire["targets"][region_id.as_str()]["corrections"] = json!([TargetCorrection {
            at: 15,
            region: TargetRegion {
                center: [600_000, 500_000],
                ..region
            },
        }]);
    });
    let before = crate::generation_inputs::GenerationInputCapture::with_region(
        capture(&with_region, ExtensionDirection::FromLeft),
        &with_region,
        Some(&region_id),
    )
    .unwrap();
    let after = crate::generation_inputs::GenerationInputCapture::with_region(
        capture(&corrected, ExtensionDirection::FromLeft),
        &corrected,
        Some(&region_id),
    )
    .unwrap();
    assert_eq!(before.inputs, after.inputs);
    assert_ne!(before.region, after.region);
    assert_eq!(after.region.as_ref().unwrap().id, region_id);
    let direct = crate::generation_inputs::GenerationInputCapture::capture_with_plan(
        &corrected,
        &RenderPlan::compile(&corrected).unwrap(),
        &target(),
        capture_spec(ExtensionDirection::FromLeft),
        Some(&region_id),
        &MeasuredPictures::new(),
        &mut InputCaptureBudget::default(),
    )
    .unwrap();
    assert_eq!(direct, after);
    let missing = crate::generation_inputs::GenerationInputCapture::with_region(
        after.clone(),
        &corrected,
        Some(&TargetId::new("absent").unwrap()),
    )
    .unwrap();
    assert!(missing.region.as_ref().unwrap().record.is_none());
    assert_ne!(missing.region, None);
    assert_eq!(
        crate::generation_inputs::GenerationInputCapture::with_region(after, &corrected, None)
            .unwrap()
            .region,
        None
    );
}

fn attempt(
    document: &ProjectDocument,
    capture: GenerationCaptureSpec,
    budget: &mut InputCaptureBudget,
) -> Result<GenerationInputBinding, StoreError> {
    crate::generation_inputs::GenerationInputCapture::capture_with_plan(
        document,
        &RenderPlan::compile(document).unwrap(),
        &target(),
        capture,
        None,
        &MeasuredPictures::new(),
        budget,
    )
}
fn assert_poisoned(budget: &mut InputCaptureBudget, document: &ProjectDocument) {
    assert_eq!(budget.queries.max_scopes, 0);
    assert_eq!(budget.queries.max_comparisons, 0);
    assert_eq!(budget.bytes, 0);
    assert_eq!(budget.spans, 0);
    assert!(attempt(document, GenerationCaptureSpec::Bridge, budget).is_err());
}

#[test]
fn shared_query_and_span_ledgers_exhaust_without_refunding_failed_work() {
    let document = ordinary();
    let context = context(&document, ExtensionDirection::FromLeft);
    let capture = capture_spec(ExtensionDirection::FromLeft);
    let mut query = InputCaptureBudget {
        queries: BoundaryQueryLimits {
            max_scopes: context.lookup.visited_nodes,
            max_comparisons: context.lookup.sequence_comparisons
                + context.lookup.iteration_run_comparisons,
        },
        ..InputCaptureBudget::default()
    };
    assert!(attempt(&document, capture, &mut query).is_ok());
    assert!(attempt(&document, capture, &mut query).is_err());
    assert_poisoned(&mut query, &document);
    let mut spans = InputCaptureBudget {
        spans: context.coverage.spans.len() + 1,
        ..InputCaptureBudget::default()
    };
    assert!(attempt(&document, capture, &mut spans).is_ok());
    assert_eq!(spans.spans, 0);
    assert!(attempt(&document, capture, &mut spans).is_err());
    assert_poisoned(&mut spans, &document);
    let mut failed = InputCaptureBudget {
        queries: BoundaryQueryLimits {
            max_scopes: 1,
            max_comparisons: 1,
        },
        ..InputCaptureBudget::default()
    };
    assert!(attempt(&document, capture, &mut failed).is_err());
    assert_poisoned(&mut failed, &document);
}

#[test]
fn failed_metadata_charge_poisons_the_batch_before_a_smaller_capture() {
    let document = ordinary();
    let capture = capture_spec(ExtensionDirection::FromLeft);
    let binding = attempt(&document, capture, &mut InputCaptureBudget::default()).unwrap();
    let bytes = serde_json::to_vec(&binding).unwrap().len();
    let bridge = attempt(
        &document,
        GenerationCaptureSpec::Bridge,
        &mut InputCaptureBudget::default(),
    )
    .unwrap();
    assert!(serde_json::to_vec(&bridge).unwrap().len() < bytes - 1);
    let mut budget = InputCaptureBudget {
        bytes: bytes - 1,
        ..InputCaptureBudget::default()
    };
    assert!(attempt(&document, capture, &mut budget).is_err());
    assert_poisoned(&mut budget, &document);
    let mut exact = InputCaptureBudget {
        bytes,
        ..InputCaptureBudget::default()
    };
    assert_eq!(attempt(&document, capture, &mut exact).unwrap(), binding);
    assert_eq!(exact.bytes, 0);
    assert!(attempt(&document, GenerationCaptureSpec::Bridge, &mut exact).is_err());
    assert_poisoned(&mut exact, &document);
}

#[test]
fn unavailable_support_retains_charged_work_and_wrong_context_branding_refuses_explicitly() {
    struct EndpointsOnly(MeasuredPictures);
    impl GenerationPictures for EndpointsOnly {
        fn identity(
            &self,
            document: &ProjectDocument,
            picture: &Picture,
        ) -> Result<GenerationPictureIdentity, StoreError> {
            self.0.identity(document, picture)
        }
    }
    let document = ordinary();
    let plan = RenderPlan::compile(&document).unwrap();
    let capture = capture_spec(ExtensionDirection::FromLeft);
    let mut budget = InputCaptureBudget::default();
    let error = crate::generation_inputs::GenerationInputCapture::capture_with_plan(
        &document,
        &plan,
        &target(),
        capture,
        None,
        &EndpointsOnly(MeasuredPictures::new()),
        &mut budget,
    )
    .unwrap_err();
    assert!(error.to_string().contains("support is unavailable"));
    assert!(budget.queries.max_scopes < InputCaptureBudget::default().queries.max_scopes);
    assert!(attempt(&document, GenerationCaptureSpec::Bridge, &mut budget).is_ok());
    let context = context(&document, ExtensionDirection::FromLeft);
    assert!(
        crate::generation_inputs::GenerationInputCapture::from_context(
            &document,
            &context,
            GenerationCaptureSpec::Bridge,
            &MeasuredPictures::new()
        )
        .is_err()
    );
    assert!(
        crate::generation_inputs::GenerationInputCapture::from_context(
            &document,
            &context,
            capture_spec(ExtensionDirection::FromRight),
            &MeasuredPictures::new()
        )
        .is_err()
    );
    let later = edit(&document, "later", |_| {});
    assert!(
        crate::generation_inputs::GenerationInputCapture::from_context(
            &later,
            &context,
            capture,
            &MeasuredPictures::new()
        )
        .is_err()
    );
    let mut wrong = context;
    wrong.pictures[0].definition = id("other");
    assert!(
        crate::generation_inputs::GenerationInputCapture::from_context(
            &document,
            &wrong,
            capture,
            &MeasuredPictures::new()
        )
        .is_err()
    );
}

#[test]
fn strict_input_wire_refuses_unknown_operation_policy_version_and_fields() {
    let document = ordinary();
    let binding = capture(&document, ExtensionDirection::FromLeft);
    let wire = serde_json::to_value(&binding).unwrap();
    assert_eq!(
        serde_json::from_value::<GenerationInputBinding>(wire.clone()).unwrap(),
        binding
    );
    for (path, value) in [
        ("/unexpected", json!(true)),
        ("/inputs/operation", json!("future")),
        ("/inputs/unexpected", json!(true)),
        ("/inputs/capture/operation", json!("future")),
        ("/inputs/capture/policy", json!("temporal_context_v2")),
        ("/inputs/capture/direction", json!("both")),
        ("/inputs/capture/unexpected", json!(1)),
        ("/inputs/samples/0/unexpected", json!(1)),
        ("/inputs/samples/0/picture/unexpected", json!(1)),
        ("/inputs/support/0/unexpected", json!(1)),
        ("/inputs/terminal/unexpected", json!(1)),
    ] {
        let mut changed = wire.clone();
        let (parent, key) = path.rsplit_once('/').unwrap();
        changed
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.into(), value);
        assert!(
            serde_json::from_value::<GenerationInputBinding>(changed).is_err(),
            "{path}"
        );
    }
    for frames in [0, deadpan_plan::MAX_HOLD_CONTEXT_FRAMES + 1] {
        let capture = GenerationCaptureSpec::Extension {
            direction: ExtensionDirection::FromLeft,
            native_rate: rate(),
            context_frames: frames,
            policy: ExtensionCapturePolicy::TemporalContextV1,
        };
        assert!(attempt(&document, capture, &mut InputCaptureBudget::default()).is_err());
    }
    assert!(
        serde_json::from_value::<GenerationCaptureSpec>(
            json!({"operation":"bridge", "policy":"temporal_context_v1"})
        )
        .is_err()
    );
}
