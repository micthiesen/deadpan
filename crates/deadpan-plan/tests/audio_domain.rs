use std::{collections::BTreeMap, ops::Range};

use deadpan_core::*;
use deadpan_plan::{
    AudioBoundaryRule, AudioContent, AudioQueryLimits, AudioSignalContent, PlanError, RenderPlan,
    SilenceReason,
};

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn samples(start: i64, end: i64) -> Range<AudioSample> {
    AudioSample(start)..AudioSample(end)
}
fn exact(start: i64, end: i64) -> Range<ExactRatio> {
    ExactRatio::integer(start)..ExactRatio::integer(end)
}

fn selected(start: i64, end: i64) -> SourceAudio {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: AssetId::new("original").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: start,
                time_base,
            },
            SourceTimestamp {
                ticks: end,
                time_base,
            },
        )
        .unwrap(),
    }
}

fn source(length: i64, input: Option<SourceAudio>, offset: i64) -> BeatNode {
    BeatNode {
        label: "Source".into(),
        audio_edges: AudioEdgePolicies {
            node_start: AudioEdgePolicy::Hard,
            node_end: AudioEdgePolicy::Hard,
            ..Default::default()
        },
        kind: NodeKind::Source {
            source: SourceNode {
                duration: frames(length),
                video: if input.is_none() {
                    SourceVideo::Stream {
                        asset: AssetId::new("original").unwrap(),
                        span: selected(0, 100).span,
                    }
                } else {
                    SourceVideo::Blank
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: input,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(offset),
                link: LinkRelation::Independent,
            },
        },
    }
}

fn hold(length: i64, audio: HoldAudio) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: frames(length),
            video: HoldVideo::Background,
            audio,
        },
    )
}

fn retime(
    child: &str,
    length: i64,
    selection: Range<i64>,
    pitch: PitchPolicy,
    purpose: RetimePurpose,
) -> BeatNode {
    BeatNode {
        label: "Retime".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            purpose,
            child: id(child),
            duration: frames(length),
            mapping: FrameRange::new(ProjectFrame(selection.start), ProjectFrame(selection.end))
                .unwrap(),
            pitch,
        },
    }
}

fn partition(child: &str, selection: Range<i64>) -> BeatNode {
    retime(
        child,
        selection.end - selection.start,
        selection,
        PitchPolicy::FollowSpeed,
        RetimePurpose::Partition,
    )
}

fn document(
    rate: FrameRate,
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("domains").unwrap(),
        RevisionId::new("captured").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", children.iter().map(|name| id(name)).collect()),
    );
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["overrides"] = serde_json::to_value(overrides).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("original").unwrap(),
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: Some(selected(0, 48_000).span),
            audio: Some(selected(0, 48_000).span),
            still_image: false,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn compile(
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
) -> RenderPlan {
    RenderPlan::compile(&document(
        FrameRate::new(48_000, 1).unwrap(),
        children,
        nodes,
        BTreeMap::new(),
    ))
    .unwrap()
}

#[test]
fn hidden_source_context_cannot_select_the_visible_root_sibling() {
    let plan = compile(
        &["x", "partition"],
        [
            ("x", source(6, Some(selected(100, 106)), 0)),
            ("a", source(8, Some(selected(0, 8)), 0)),
            ("partition", partition("a", 4..8)),
        ],
    );
    let domain = plan
        .audio_domain_at(AudioSample(6), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(domain.instance().node, id("a"));
    assert_eq!(domain.root_extent(), exact(2, 10));
    assert_eq!(domain.root_samples(), samples(2, 10));
    assert_eq!(domain.visible_samples(), samples(6, 10));
    assert_eq!(domain.gap_after(), None);
    assert!(domain.belongs_to(&plan));
    assert!(!domain.belongs_to(&plan.clone()));
    let whole_root = plan
        .audio(samples(2, 3), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(
        whole_root.spans[0]
            .source_point(AudioSample(2))
            .unwrap()
            .ticks,
        ExactRatio::integer(102)
    );
    let hidden = domain
        .audio(samples(2, 3), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(
        hidden.spans[0].source_point(AudioSample(2)).unwrap().ticks,
        ExactRatio::ZERO
    );
    assert_eq!(hidden.spans[0].envelope_samples, samples(2, 10));
    assert_eq!(hidden.spans[0].boundaries.start[0].instance.node, id("a"));
    let processing = domain
        .processing(samples(2, 10), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(
        processing.spans[0]
            .source_point(AudioSample(2))
            .unwrap()
            .ticks,
        ExactRatio::ZERO
    );
    assert_eq!(processing.spans[0].grid.frame_origin(), ExactRatio::ZERO);
    assert_eq!(
        processing.spans[0].grid.boundary_rule(),
        AudioBoundaryRule::RoundEven
    );
    assert_eq!(processing.spans[0].allocated_samples, samples(2, 10));
}

#[test]
fn negative_and_ntsc_domains_keep_the_absolute_root_phase() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    for prefix in [0, 3] {
        let mut nodes = vec![
            ("a", source(4, Some(selected(0, 6400)), 0)),
            ("partition", partition("a", 2..4)),
        ];
        let children = if prefix == 0 {
            vec!["partition"]
        } else {
            nodes.push(("x", source(3, Some(selected(10_000, 14_800)), 0)));
            vec!["x", "partition"]
        };
        let doc = document(rate, &children, nodes, BTreeMap::new());
        for plan in [
            RenderPlan::compile(&doc).unwrap(),
            RenderPlan::compile_audio_context(&FrozenAudioContext::capture(&doc).unwrap()).unwrap(),
        ] {
            let visible = rate.audio_boundary(ProjectFrame(prefix)).unwrap();
            let domain = plan
                .audio_domain_at(visible, AudioQueryLimits::default())
                .unwrap();
            assert_eq!(domain.root_extent(), exact(prefix - 2, prefix + 2));
            let start = rate.audio_boundary(ProjectFrame(prefix - 2)).unwrap();
            assert_eq!(domain.root_samples().start, start);
            let read = domain
                .processing(start..AudioSample(start.0 + 1), AudioQueryLimits::default())
                .unwrap();
            assert_eq!(
                read.spans[0].transform.project_origin,
                ExactRatio::integer(prefix - 2)
            );
            let local = read.spans[0].sampling.local_at(start).unwrap();
            if prefix == 3 {
                assert_eq!(start, AudioSample(1602));
                assert_eq!(local, ExactRatio::new(1, 4004).unwrap());
                assert_eq!(
                    local
                        .checked_div(read.spans[0].transform.project_frames_per_sample)
                        .unwrap(),
                    ExactRatio::new(2, 5).unwrap()
                );
            } else {
                assert_eq!(start, AudioSample(-3203));
                assert_eq!(local, ExactRatio::new(1, 8008).unwrap());
            }
        }
    }
}

#[test]
fn ordinary_crop_and_source_placement_constrain_hidden_filter_support() {
    let mut a = source(12, Some(selected(0, 8)), 2);
    let NodeKind::Source { source } = &mut a.kind else {
        unreachable!()
    };
    source.audio_mapping = SourceAudioMapping::Duration {
        frames: ExactRatio::integer(8),
    };
    let plan = compile(
        &["x", "partition"],
        [
            ("x", hold(6, HoldAudio::Silence)),
            ("a", a),
            (
                "crop",
                retime("a", 8, 3..11, PitchPolicy::FollowSpeed, RetimePurpose::Edit),
            ),
            ("partition", partition("crop", 4..8)),
        ],
    );
    let domain = plan
        .audio_domain_at(AudioSample(6), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(domain.root_extent(), exact(2, 9));
    let query = domain
        .audio(domain.root_samples(), AudioQueryLimits::default())
        .unwrap();
    let AudioContent::Source { support, .. } = query.spans[0].content else {
        panic!("source")
    };
    assert_eq!(support.start.ticks, ExactRatio::ONE);
    assert_eq!(support.end.ticks, ExactRatio::integer(8));
    assert_eq!(query.spans[0].boundaries.start[0].instance.node, id("crop"));
    let absent = plan
        .audio_domain_at(AudioSample(9), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(absent.root_samples(), samples(9, 10));
    assert!(matches!(
        absent
            .audio(absent.root_samples(), AudioQueryLimits::default())
            .unwrap()
            .spans[0]
            .content,
        AudioContent::Silence {
            reason: SilenceReason::OutsideSourcePlacement
        }
    ));
}

#[test]
fn preserve_retains_input_history_and_flattened_policies_from_its_own_subtree() {
    let plan = compile(
        &["x", "partition"],
        [
            ("x", source(6, Some(selected(100, 106)), 0)),
            ("a", source(4, Some(selected(0, 4)), 0)),
            ("silent", hold(2, HoldAudio::Silence)),
            ("picture", source(2, None, 0)),
            (
                "input",
                BeatNode::sequence("Input", vec![id("a"), id("silent"), id("picture")]),
            ),
            (
                "stage",
                retime(
                    "input",
                    12,
                    0..8,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            ("partition", partition("stage", 6..12)),
        ],
    );
    let domain = plan
        .audio_domain_at(AudioSample(7), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(domain.instance().node, id("stage"));
    assert_eq!(domain.root_samples(), samples(0, 12));
    assert_eq!(domain.visible_samples(), samples(6, 12));
    let processing = domain
        .processing(domain.root_samples(), AudioQueryLimits::default())
        .unwrap();
    let AudioSignalContent::Stage(stage) = &processing.spans[0].content else {
        panic!("Preserve")
    };
    assert_eq!(stage.descriptor().selection, exact(0, 8));
    assert_eq!(stage.descriptor().duration, frames(12));
    assert_eq!(stage.input_signal().support(), exact(0, 8));
    let policies = domain
        .audio(domain.root_samples(), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(
        policies
            .spans
            .iter()
            .map(|span| span.instance.node.clone())
            .collect::<Vec<_>>(),
        vec![id("a"), id("silent"), id("picture")]
    );
    assert_eq!(
        policies.spans[0]
            .source_point(AudioSample(0))
            .unwrap()
            .ticks,
        ExactRatio::ZERO
    );
    assert!(matches!(
        policies.spans[1].content,
        AudioContent::Silence {
            reason: SilenceReason::SilentHold
        }
    ));
    assert!(matches!(
        policies.spans[2].content,
        AudioContent::Silence {
            reason: SilenceReason::NoSourceAudio
        }
    ));
    assert!(matches!(
        domain.audio(
            domain.root_samples(),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 100
            }
        ),
        Err(PlanError::AudioQueryLimit("span count"))
    ));
}

#[test]
fn room_tone_keeps_intrinsic_duration_and_origin_through_transparent_retimes() {
    let plan = compile(
        &["partition"],
        [
            (
                "tone",
                hold(
                    8,
                    HoldAudio::RoomTone {
                        source: selected(0, 2),
                    },
                ),
            ),
            (
                "unity",
                retime("tone", 8, 0..8, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
            (
                "speed",
                retime(
                    "unity",
                    16,
                    0..8,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Edit,
                ),
            ),
            ("partition", partition("speed", 8..16)),
        ],
    );
    let domain = plan
        .audio_domain_at(AudioSample(0), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(domain.instance().node, id("tone"));
    assert_eq!(domain.root_samples(), samples(-8, 8));
    let query = domain
        .processing(domain.root_samples(), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(
        query.spans[0].sampling.local_at(AudioSample(-8)).unwrap(),
        ExactRatio::ZERO
    );
    assert_eq!(query.spans[0].retimes.len(), 2);
    assert!(
        matches!(query.spans[0].content, AudioSignalContent::Leaf(AudioContent::RoomTone { duration, .. }) if duration == frames(8))
    );
}

#[test]
fn billion_play_sparse_domains_and_gaps_keep_complete_occurrence_identity() {
    let plays = 1_000_000_000;
    let allocation = RevisionId::new("allocation").unwrap();
    let last = IterationId {
        allocation: allocation.clone(),
        ordinal: plays - 1,
    };
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["repeat"],
        [
            ("ordinary", source(1, Some(selected(0, 1)), 0)),
            ("a", source(4, Some(selected(10, 14)), 0)),
            ("partition", partition("a", 2..4)),
            (
                "repeat",
                BeatNode {
                    label: "Repeat".into(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("ordinary"),
                        iterations: IterationOrder::new(allocation, plays).unwrap(),
                        gap: Some(HoldRecipe {
                            duration: frames(1),
                            video: HoldVideo::Background,
                            audio: HoldAudio::RoomTone {
                                source: selected(20, 22),
                            },
                        }),
                    },
                },
            ),
        ],
        BTreeMap::from([(
            id("repeat"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: last.clone(),
                root: id("partition"),
            }])
            .unwrap(),
        )]),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let limits = AudioQueryLimits {
        maximum_spans: 1,
        maximum_work: 64,
    };
    let start = i64::from(plays - 1) * 2;
    let domain = plan.audio_domain_at(AudioSample(start), limits).unwrap();
    assert_eq!(domain.root_samples(), samples(start - 2, start + 2));
    assert_eq!(domain.instance().node, id("a"));
    assert_eq!(
        domain.instance().repeats,
        vec![RepeatInstance {
            node: id("repeat"),
            iteration: last
        }]
    );
    let read = domain.audio(samples(start - 2, start - 1), limits).unwrap();
    assert_eq!(
        read.spans[0]
            .source_point(AudioSample(start - 2))
            .unwrap()
            .ticks,
        ExactRatio::integer(10)
    );
    assert!(read.lookup.visited_nodes < 8);
    let gap = plan
        .audio_domain_at(AudioSample(start - 1), limits)
        .unwrap();
    assert_eq!(gap.instance().node, id("repeat"));
    assert_eq!(gap.gap_after().unwrap().ordinal, plays - 2);
    assert_eq!(gap.root_samples(), samples(start - 1, start));
    let read = gap.audio(gap.root_samples(), limits).unwrap();
    assert_eq!(read.spans[0].gap_after, gap.gap_after().cloned());
    assert_eq!(
        read.spans[0]
            .sampling
            .local_at(AudioSample(start - 1))
            .unwrap(),
        ExactRatio::ZERO
    );
    assert!(
        matches!(read.spans[0].content, AudioContent::RoomTone { duration, .. } if duration == frames(1))
    );
    assert_eq!(
        gap.processing(gap.root_samples(), limits).unwrap().spans[0].gap_after,
        gap.gap_after().cloned()
    );
}

#[test]
fn domain_queries_reject_outside_support_and_enforce_work_and_span_limits() {
    let plan = compile(
        &["partition"],
        [
            ("a", source(8, Some(selected(0, 8)), 0)),
            ("partition", partition("a", 4..8)),
        ],
    );
    let limits = AudioQueryLimits::default();
    let domain = plan.audio_domain_at(AudioSample(0), limits).unwrap();
    for range in [samples(-5, 0), samples(0, 5), samples(1, 0)] {
        assert!(matches!(
            domain.audio(range.clone(), limits),
            Err(PlanError::AudioRangeOutOfRange)
        ));
        assert!(matches!(
            domain.processing(range, limits),
            Err(PlanError::AudioRangeOutOfRange)
        ));
    }
    for sample in [-1, 4] {
        assert!(matches!(
            plan.audio_domain_at(AudioSample(sample), limits),
            Err(PlanError::AudioRangeOutOfRange)
        ));
    }
    let no_work = AudioQueryLimits {
        maximum_spans: 1,
        maximum_work: 1,
    };
    assert!(matches!(
        plan.audio_domain_at(AudioSample(0), no_work),
        Err(PlanError::AudioQueryLimit("structural work"))
    ));
    assert!(matches!(
        domain.audio(samples(0, 1), no_work),
        Err(PlanError::AudioQueryLimit("structural work"))
    ));
    assert!(
        domain
            .audio(samples(-4, -4), limits)
            .unwrap()
            .spans
            .is_empty()
    );
    assert!(
        domain
            .processing(samples(4, 4), limits)
            .unwrap()
            .spans
            .is_empty()
    );
    assert!(matches!(
        domain.audio(
            samples(0, 0),
            AudioQueryLimits {
                maximum_spans: 0,
                maximum_work: 1
            }
        ),
        Err(PlanError::InvalidAudioLimits)
    ));
}

#[test]
fn cropped_nested_gap_keeps_its_outer_play_and_its_own_original_zero() {
    let allocation = RevisionId::new("allocation").unwrap();
    let plan = compile(
        &["outer"],
        [
            ("a", source(4, Some(selected(0, 4)), 0)),
            (
                "inner",
                BeatNode {
                    label: "Inner".into(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("a"),
                        iterations: IterationOrder::new(allocation.clone(), 2).unwrap(),
                        gap: Some(HoldRecipe {
                            duration: frames(2),
                            video: HoldVideo::Background,
                            audio: HoldAudio::RoomTone {
                                source: selected(20, 22),
                            },
                        }),
                    },
                },
            ),
            ("partition", partition("inner", 5..6)),
            (
                "outer",
                BeatNode {
                    label: "Outer".into(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("partition"),
                        iterations: IterationOrder::new(allocation.clone(), 2).unwrap(),
                        gap: None,
                    },
                },
            ),
        ],
    );
    let domain = plan
        .audio_domain_at(AudioSample(1), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(domain.root_samples(), samples(0, 2));
    assert_eq!(domain.visible_samples(), samples(1, 2));
    assert_eq!(
        domain.instance(),
        &InstancePath {
            node: id("inner"),
            repeats: vec![RepeatInstance {
                node: id("outer"),
                iteration: IterationId {
                    allocation: allocation.clone(),
                    ordinal: 1
                }
            }],
        }
    );
    assert_eq!(
        domain.gap_after(),
        Some(&IterationId {
            allocation,
            ordinal: 0
        })
    );
    let query = domain
        .audio(samples(0, 1), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(query.spans[0].instance, *domain.instance());
    assert_eq!(
        query.spans[0].sampling.local_at(AudioSample(0)).unwrap(),
        ExactRatio::ZERO
    );
    assert_eq!(query.spans[0].envelope_samples, samples(0, 2));
    assert!(
        query.spans[0]
            .boundaries
            .start
            .iter()
            .any(|edge| edge.instance == *domain.instance()
                && edge.gap_after.as_ref() == domain.gap_after())
    );
}
