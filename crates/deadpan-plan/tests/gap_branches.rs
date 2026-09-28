use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{
    AudioContent, AudioDefinitionSelector, AudioReferencePlan, AudioSignalContent, Picture,
    ReferenceAudioContent, ReferenceSample, RenderPlan, SignalSample, SilenceReason,
};

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn play(ordinal: u32) -> IterationId {
    IterationId {
        allocation: RevisionId::new("plays").unwrap(),
        ordinal,
    }
}
fn hold(frames: i64, video: HoldVideo, audio: HoldAudio) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: duration(frames),
            video,
            audio,
            picture_context: None,
        },
    )
}
fn source_audio() -> SourceAudio {
    let base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: AssetId::new("original").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base: base,
            },
            SourceTimestamp {
                ticks: 48_000,
                time_base: base,
            },
        )
        .unwrap(),
    }
}
fn fixture(plays: u32, branches: &[(u32, &str)]) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("gap-branches").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let freeze = HoldVideo::Freeze {
        asset: AssetId::new("original").unwrap(),
        timestamp: SourceTimestamp {
            ticks: 1000,
            time_base: SourceTimeBase::new(1, 30_000).unwrap(),
        },
    };
    let mut nodes = BTreeMap::from([
        (
            id("root"),
            BeatNode::sequence("Root", vec![id("outer-repeat")]),
        ),
        (
            id("outer-repeat"),
            BeatNode {
                label: "Outer".into(),
                framing: Some(
                    Framing::creep(
                        FramingPose::identity(),
                        FramingPose::identity(),
                        FramingCurve::Linear,
                    )
                    .unwrap(),
                ),
                audio_treatments: Default::default(),
                audio_edges: AudioEdgePolicies {
                    repeat_gap_start: AudioEdgePolicy::Hard,
                    ..Default::default()
                },
                kind: NodeKind::Repeat {
                    child: id("play-child"),
                    iterations: IterationOrder::new(play(0).allocation, plays).unwrap(),
                    gap: Some(HoldRecipe {
                        duration: duration(2),
                        video: HoldVideo::Background,
                        audio: HoldAudio::Silence,
                        picture_context: None,
                    }),
                },
            },
        ),
        (
            id("play-child"),
            hold(1, HoldVideo::Background, HoldAudio::Silence),
        ),
        (
            id("gap-root"),
            BeatNode::sequence("Gap branch", vec![id("inner-repeat")]),
        ),
        (
            id("inner-repeat"),
            BeatNode {
                label: "Nested".into(),
                framing: None,
                audio_treatments: Default::default(),
                audio_edges: Default::default(),
                kind: NodeKind::Repeat {
                    child: id("branch-hold"),
                    iterations: IterationOrder::new(RevisionId::new("inner-plays").unwrap(), 2)
                        .unwrap(),
                    gap: Some(HoldRecipe {
                        duration: duration(1),
                        video: HoldVideo::Background,
                        audio: HoldAudio::Silence,
                        picture_context: None,
                    }),
                },
            },
        ),
        (
            id("branch-hold"),
            hold(
                1,
                freeze,
                HoldAudio::RoomTone {
                    source: source_audio(),
                },
            ),
        ),
        (id("empty-gap"), BeatNode::sequence("Empty gap", vec![])),
        (
            id("empty-final"),
            BeatNode::sequence("Empty final gap", vec![]),
        ),
    ]);
    for name in ["empty-gap", "empty-final"] {
        if !branches.iter().any(|(_, root)| *root == name) {
            nodes.remove(&id(name));
        }
    }
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(std::mem::take(&mut nodes)).unwrap();
    wire["gap_overrides"] = serde_json::to_value(BTreeMap::from([(
        id("outer-repeat"),
        PlayOverrides::try_from(
            branches
                .iter()
                .map(|(ordinal, root)| PlayOverride {
                    iteration: play(*ordinal),
                    root: id(root),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    )]))
    .unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("original").unwrap(),
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: Some(
                SourceSpan::new(
                    SourceTimestamp {
                        ticks: 0,
                        time_base: SourceTimeBase::new(1, 30_000).unwrap(),
                    },
                    SourceTimestamp {
                        ticks: 30_000,
                        time_base: SourceTimeBase::new(1, 30_000).unwrap(),
                    },
                )
                .unwrap(),
            ),
            audio: Some(source_audio().span),
            still_image: false,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn nested_gap_branch_has_its_own_picture_audio_and_reference_path() {
    let document = fixture(3, &[(0, "gap-root")]);
    let plan = RenderPlan::compile(&document).unwrap();
    assert_eq!(plan.duration(), duration(8));
    assert_eq!(plan.metadata().storage.sparse_override_entries, 1);
    for frame in [1, 3] {
        let sample = plan.picture(ProjectFrame(frame)).unwrap();
        assert!(matches!(sample.picture, Picture::Freeze { .. }));
        assert_eq!(sample.instance.node, id("branch-hold"));
        assert_eq!(sample.instance.repeats[0].iteration, play(0));
        assert_eq!(sample.instance.repeats[0].node, id("outer-repeat"));
        assert_eq!(sample.instance.repeats[1].node, id("inner-repeat"));
        assert_eq!(sample.gap_after, None);
        assert_eq!(sample.framing[0].instance.node, id("branch-hold"));
        assert!(
            sample
                .framing
                .iter()
                .any(|scope| scope.instance.node == id("outer-repeat"))
        );
        assert!(
            !sample
                .framing
                .iter()
                .any(|scope| scope.instance.node == id("play-child"))
        );
        sample.instance.validate(&document).unwrap();
    }
    assert_eq!(
        plan.picture(ProjectFrame(2)).unwrap().picture,
        Picture::Background
    );
    let default = plan.picture(ProjectFrame(5)).unwrap();
    assert_eq!(default.gap_after, Some(play(1)));
    assert_eq!(default.instance.node, id("outer-repeat"));
    assert_eq!(default.picture, Picture::Background);

    let room = plan
        .audio_processing(AudioSample(1600)..AudioSample(3200), Default::default())
        .unwrap();
    assert!(matches!(
        room.spans[0].content,
        AudioSignalContent::Leaf(AudioContent::RoomTone { .. })
    ));
    assert_eq!(room.spans[0].instance.node, id("branch-hold"));
    let signal = plan
        .audio_signal()
        .query(SignalSample(1600)..SignalSample(3200), Default::default())
        .unwrap();
    assert!(matches!(
        signal.spans[0].content,
        AudioSignalContent::Leaf(AudioContent::RoomTone { .. })
    ));
    assert_eq!(signal.spans[0].instance.node, id("branch-hold"));
    let domain = plan
        .audio_domain_at(AudioSample(1600), Default::default())
        .unwrap();
    assert_eq!(domain.instance().node, id("branch-hold"));
    assert!(
        plan.audio_policy(AudioSample(1600)..AudioSample(3200), Default::default())
            .unwrap()
            .suppressed
            .is_empty()
    );
    let fade = plan
        .audio_fades(AudioSample(1600)..AudioSample(3200), Default::default())
        .unwrap();
    assert!(fade.spans[0].boundaries.start.iter().any(|edge| {
        edge.kind == AudioBoundaryKind::RepeatGapStart
            && edge.policy == AudioEdgePolicy::Hard
            && edge.gap_after == Some(play(0))
    }));
    let quiet = plan
        .audio_processing(AudioSample(8000)..AudioSample(9600), Default::default())
        .unwrap();
    assert!(matches!(
        quiet.spans[0].content,
        AudioSignalContent::Leaf(AudioContent::Silence {
            reason: SilenceReason::SilentHold
        })
    ));
    let frozen =
        AudioReferencePlan::compile(&FrozenAudioLayout::capture(&document).unwrap()).unwrap();
    let reference = frozen
        .root_clock()
        .query(
            ReferenceSample(1600)..ReferenceSample(3200),
            Default::default(),
        )
        .unwrap();
    assert_eq!(reference.spans[0].content, ReferenceAudioContent::RoomTone);
    assert_eq!(reference.spans[0].instance.node, id("branch-hold"));
    let retained =
        RenderPlan::compile_audio_context(&FrozenAudioContext::capture(&document).unwrap())
            .unwrap();
    let retained_room = retained
        .audio_processing(AudioSample(1600)..AudioSample(3200), Default::default())
        .unwrap();
    assert!(matches!(
        retained_room.spans[0].content,
        AudioSignalContent::Leaf(AudioContent::RoomTone { .. })
    ));
    assert_eq!(retained_room.spans[0].instance.node, id("branch-hold"));
    let definition = plan
        .audio_definition(AudioDefinitionSelector::RepeatGap {
            repeat: id("outer-repeat"),
        })
        .unwrap();
    let gap = definition
        .signal()
        .query(SignalSample(0)..SignalSample(1600), Default::default())
        .unwrap();
    assert!(matches!(
        gap.spans[0].content,
        AudioSignalContent::Leaf(AudioContent::Silence {
            reason: SilenceReason::SilentHold
        })
    ));
}

#[test]
fn empty_gap_branch_suppresses_one_gap_and_final_branch_stays_dormant() {
    let document = fixture(3, &[(0, "gap-root"), (1, "empty-gap"), (2, "empty-final")]);
    let plan = RenderPlan::compile(&document).unwrap();
    assert_eq!(plan.duration(), duration(6));
    assert_eq!(plan.metadata().storage.sparse_override_entries, 3);
    assert_eq!(
        plan.picture(ProjectFrame(5)).unwrap().instance.node,
        id("play-child")
    );
    assert!(matches!(
        plan.picture(ProjectFrame(6)),
        Err(deadpan_plan::PlanError::FrameOutOfRange { .. })
    ));
}

#[test]
fn pause_at_a_split_composite_seam_preserves_every_suffix_picture_and_owned_scope() {
    let original = fixture(3, &[(0, "gap-root"), (1, "empty-gap")]);
    let split = CommandRequest {
        project_id: original.project_id().clone(),
        expected_revision: original.revision_id().clone(),
        new_revision: RevisionId::new("split-composite").unwrap(),
        command: Command::Split {
            node: id("outer-repeat"),
            at: duration(2),
            identities: SplitIdentities {
                nodes: (0..20).map(|index| id(&format!("copy-{index}"))).collect(),
            },
        },
    };
    let document = apply(&original, &split)
        .unwrap()
        .forward
        .apply(&original)
        .unwrap();
    let before = RenderPlan::compile(&document).unwrap();
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("insert-composite").unwrap(),
        command: Command::InsertTime {
            at: ProjectFrame(2),
            hold: HoldRecipe {
                duration: duration(3),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
            id: id("pause"),
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: RevisionId::new("insert-composite").unwrap(),
                ordinal: 0,
            },
        },
    };
    let transaction = apply(&document, &request).unwrap();
    let edited = transaction.forward.apply(&document).unwrap();
    assert_eq!(transaction.inverse.apply(&edited).unwrap(), document);
    assert_eq!(edited.nodes().len(), document.nodes().len() + 1);
    let after = RenderPlan::compile(&edited).unwrap();
    assert_eq!(after.duration().frames(), before.duration().frames() + 3);
    for frame in 0..after.duration().frames() {
        let actual = after.picture(ProjectFrame(frame)).unwrap();
        if (2..5).contains(&frame) {
            assert_eq!(actual.picture, Picture::Background);
            assert_eq!(actual.instance.node, id("pause"));
        } else {
            let expected = before
                .picture(ProjectFrame(if frame < 2 { frame } else { frame - 3 }))
                .unwrap();
            assert_eq!(actual.picture, expected.picture, "picture at {frame}");
            assert_eq!(
                actual.instance, expected.instance,
                "stable occurrence at {frame}"
            );
            assert_eq!(actual.gap_after, expected.gap_after, "gap at {frame}");
            assert_eq!(
                actual.picture_context, expected.picture_context,
                "capture at {frame}"
            );
            // The root's clock lengthens; every lower provider/partition/
            // Repeat scope keeps its exact local time and evaluated pose.
            let (root, lower) = actual.framing.split_last().unwrap();
            let (_, old_lower) = expected.framing.split_last().unwrap();
            assert_eq!(lower, old_lower, "owned framing at {frame}");
            assert_eq!(root.instance.node, *edited.root());
            assert_eq!(root.duration, after.duration());
        }
    }
}

#[test]
fn sparse_branch_lookup_stays_compact_at_a_billion_plays() {
    let document = fixture(1_000_000_000, &[(999_999_998, "gap-root")]);
    let plan = RenderPlan::compile(&document).unwrap();
    assert_eq!(plan.duration().frames(), 2_999_999_999);
    assert!(plan.metadata().storage.repeat_segment_entries < 8);
    assert_eq!(plan.metadata().storage.sparse_override_entries, 1);
    let sample = plan.picture(ProjectFrame(2_999_999_995)).unwrap();
    assert_eq!(sample.instance.node, id("branch-hold"));
    assert_eq!(sample.instance.repeats[0].iteration, play(999_999_998));
    assert!(sample.lookup.iteration_run_comparisons < 32);
    let final_play = plan.picture(ProjectFrame(2_999_999_998)).unwrap();
    assert_eq!(final_play.instance.node, id("play-child"));
}
