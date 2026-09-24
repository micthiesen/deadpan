use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{
    AudioBoundaryRule, AudioContent, AudioQueryLimits, AudioReferencePlan, AudioSignalContent,
    PlanError, ReferenceAudioContent, ReferenceAudioSpan, ReferenceSample, RenderPlan,
    SignalSample, SilenceReason,
};

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn samples(start: i64, end: i64) -> std::ops::Range<ReferenceSample> {
    ReferenceSample(start)..ReferenceSample(end)
}
fn instance(name: &str) -> InstancePath {
    InstancePath {
        node: id(name),
        repeats: Vec::new(),
    }
}

fn audio() -> SourceAudio {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: AssetId::new("original").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 48_000,
                time_base,
            },
        )
        .unwrap(),
    }
}

fn source(frames: i64) -> BeatNode {
    BeatNode {
        label: "Original".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(frames),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(audio()),
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
    }
}

fn hold(frames: i64, audio: HoldAudio) -> BeatNode {
    BeatNode::hold(
        "Policy",
        HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Background,
            audio,
        },
    )
}

fn retime(child: &str, frames: i64, start: i64, end: i64, pitch: PitchPolicy) -> BeatNode {
    BeatNode {
        label: "Retime".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: duration(frames),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch,
            purpose: RetimePurpose::Edit,
        },
    }
}

fn document(rate: FrameRate, children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("reference-project").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 640,
            height: 360,
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
        BeatNode::sequence("Edit", children.iter().map(|name| id(name)).collect()),
    );
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        audio().asset,
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(audio().span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn compile(document: &ProjectDocument) -> AudioReferencePlan {
    AudioReferencePlan::compile(&FrozenAudioLayout::capture(document).unwrap()).unwrap()
}

fn policy(content: &AudioContent) -> ReferenceAudioContent {
    match content {
        AudioContent::Source { .. } => ReferenceAudioContent::Source,
        AudioContent::RoomTone { .. } => ReferenceAudioContent::RoomTone,
        AudioContent::Tail { maximum, .. } => ReferenceAudioContent::Tail { maximum: *maximum },
        AudioContent::Silence { reason } => ReferenceAudioContent::Silence { reason: *reason },
    }
}

fn at(spans: &[ReferenceAudioSpan], sample: i64) -> ReferenceAudioContent {
    spans
        .iter()
        .find(|span| span.samples.contains(&ReferenceSample(sample)))
        .unwrap()
        .content
}

#[test]
fn ntsc_old_root_and_preserve_output_keep_both_rounding_phases() {
    for silent_start in [4, 6] {
        let doc = document(
            FrameRate::new(30_000, 1001).unwrap(),
            &["preserve"],
            vec![
                (
                    "preserve",
                    retime("sequence", 6, 0, 12, PitchPolicy::Preserve),
                ),
                (
                    "sequence",
                    BeatNode::sequence("Context", vec![id("a"), id("quiet"), id("b")]),
                ),
                ("a", source(silent_start)),
                ("quiet", hold(2, HoldAudio::Silence)),
                ("b", source(10 - silent_start)),
            ],
        );
        let frozen = compile(&doc);
        let live = RenderPlan::compile(&doc).unwrap();
        let root = frozen.root_clock();
        let output = frozen.preserve_output_clock(&instance("preserve")).unwrap();
        assert_eq!(root.grid().boundary_rule(), AudioBoundaryRule::RoundEven);
        assert_eq!(output.grid().boundary_rule(), AudioBoundaryRule::PointCeil);
        let root_query = root.query(samples(3000, 6800), Default::default()).unwrap();
        let live_query = live
            .audio(AudioSample(3000)..AudioSample(6800), Default::default())
            .unwrap();
        for (old, current) in root_query.spans.iter().zip(&live_query.spans) {
            assert_eq!(old.content, policy(&current.content));
            assert_eq!(
                old.allocated_samples,
                samples(
                    current.allocated_samples.start.0,
                    current.allocated_samples.end.0
                )
            );
            assert_eq!(old.extent, current.project_extent);
            assert_eq!(old.instance, current.instance);
        }
        assert_eq!(root_query.spans.len(), live_query.spans.len());
        let point_query = output
            .query(samples(3000, 6800), Default::default())
            .unwrap();
        let boundary = if silent_start == 4 { 3203 } else { 6406 };
        let silent = ReferenceAudioContent::Silence {
            reason: SilenceReason::SilentHold,
        };
        if silent_start == 4 {
            assert_eq!(at(&root_query.spans, boundary), silent);
            assert_eq!(
                at(&point_query.spans, boundary),
                ReferenceAudioContent::Source
            );
        } else {
            assert_eq!(
                at(&root_query.spans, boundary),
                ReferenceAudioContent::Source
            );
            assert_eq!(at(&point_query.spans, boundary), silent);
        }
        // Query order and cropping cannot alter full old allocated boundaries.
        for range in [
            samples(6400, 6800),
            samples(3000, 3204),
            samples(3204, 6400),
        ] {
            for partial in root.query(range.clone(), Default::default()).unwrap().spans {
                let full = root_query
                    .spans
                    .iter()
                    .find(|full| full.allocated_samples == partial.allocated_samples)
                    .unwrap();
                assert_eq!(partial.content, full.content);
                assert_eq!(partial.extent, full.extent);
                assert_eq!(partial.instance, full.instance);
            }
        }
    }
}

#[test]
fn preserve_input_uses_selected_origin_and_flattens_nested_policies() {
    let doc = document(
        FrameRate::new(30_000, 1001).unwrap(),
        &["outer"],
        vec![
            ("outer", retime("inner", 6, 1, 13, PitchPolicy::Preserve)),
            ("inner", retime("sequence", 14, 0, 7, PitchPolicy::Preserve)),
            (
                "sequence",
                BeatNode::sequence("Context", vec![id("a"), id("quiet"), id("b")]),
            ),
            ("a", source(2)),
            ("quiet", hold(1, HoldAudio::Silence)),
            ("b", source(4)),
        ],
    );
    let frozen = compile(&doc);
    let live = RenderPlan::compile(&doc).unwrap();
    let clock = frozen.preserve_input_clock(&instance("outer")).unwrap();
    assert_eq!(clock.grid().frame_origin(), ExactRatio::ONE);
    assert_eq!(
        clock.grid().at(ReferenceSample(0)).unwrap(),
        ExactRatio::ONE
    );
    assert_eq!(clock.support(), ExactRatio::ONE..ExactRatio::integer(13));
    let query = clock
        .query(
            samples(0, clock.sample_count().unwrap().0),
            Default::default(),
        )
        .unwrap();
    let processing = live
        .audio_processing(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let AudioSignalContent::Stage(stage) = &processing.spans[0].content else {
        panic!("outer Preserve")
    };
    let expected = stage
        .input_signal()
        .query_flattened(
            SignalSample(0)..SignalSample(clock.sample_count().unwrap().0),
            Default::default(),
        )
        .unwrap();
    assert_eq!(query.spans.len(), expected.spans.len());
    for (old, current) in query.spans.iter().zip(expected.spans) {
        let AudioSignalContent::Leaf(content) = current.content else {
            panic!("flattened policy")
        };
        assert_eq!(old.content, policy(&content));
        assert_eq!(
            old.allocated_samples,
            samples(
                current.allocated_samples.start.0,
                current.allocated_samples.end.0
            )
        );
        assert_eq!(old.extent, current.signal_extent);
    }
}

#[test]
fn tiny_zero_allocated_leaves_and_source_placement_remain_explicit() {
    let tiny = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["preserve"],
        vec![
            (
                "preserve",
                retime("sequence", 1, 0, 3, PitchPolicy::Preserve),
            ),
            (
                "sequence",
                BeatNode::sequence("Tiny", vec![id("a"), id("quiet"), id("tone")]),
            ),
            ("a", source(1)),
            ("quiet", hold(1, HoldAudio::Silence)),
            ("tone", hold(1, HoldAudio::RoomTone { source: audio() })),
        ],
    );
    let frozen = compile(&tiny);
    let root = frozen
        .root_clock()
        .query(samples(0, 1), Default::default())
        .unwrap();
    assert_eq!(root.spans.len(), 1);
    assert_eq!(root.spans[0].instance.node, id("quiet"));
    assert_eq!(root.spans[0].extent, ratio(1, 3)..ratio(2, 3));
    let point = frozen
        .preserve_output_clock(&instance("preserve"))
        .unwrap()
        .query(samples(0, 1), Default::default())
        .unwrap();
    assert_eq!(point.spans[0].instance.node, id("a"));

    let mut placed = source(10);
    let NodeKind::Source { source } = &mut placed.kind else {
        unreachable!()
    };
    source.audio_mapping = SourceAudioMapping::Placement {
        start: ExactRatio::integer(2),
        frames: ExactRatio::integer(3),
    };
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["placed", "tail"],
        vec![
            ("placed", placed),
            (
                "tail",
                hold(
                    2,
                    HoldAudio::Tail {
                        source: audio(),
                        maximum: duration(2),
                    },
                ),
            ),
        ],
    );
    let plan = compile(&doc);
    let query = plan
        .root_clock()
        .query(samples(0, 12), Default::default())
        .unwrap();
    assert_eq!(
        query
            .spans
            .iter()
            .map(|span| (span.allocated_samples.clone(), span.content))
            .collect::<Vec<_>>(),
        vec![
            (
                samples(0, 2),
                ReferenceAudioContent::Silence {
                    reason: SilenceReason::OutsideSourcePlacement
                }
            ),
            (samples(2, 5), ReferenceAudioContent::Source),
            (
                samples(5, 10),
                ReferenceAudioContent::Silence {
                    reason: SilenceReason::OutsideSourcePlacement
                }
            ),
            (
                samples(10, 12),
                ReferenceAudioContent::Tail {
                    maximum: duration(2)
                }
            ),
        ]
    );
}

fn edit(doc: &ProjectDocument, revision: &str, command: Command) -> ProjectDocument {
    apply(
        doc,
        &CommandRequest {
            project_id: doc.project_id().clone(),
            expected_revision: doc.revision_id().clone(),
            new_revision: RevisionId::new(revision).unwrap(),
            command,
        },
    )
    .unwrap()
    .forward
    .apply(doc)
    .unwrap()
}

#[test]
fn tail_maximum_retains_local_hold_and_gap_units_through_retime() {
    let tail = HoldAudio::Tail {
        source: audio(),
        maximum: duration(2),
    };
    for gap in [false, true] {
        let (frames, nodes) = if gap {
            (
                12,
                vec![
                    (
                        "context",
                        BeatNode {
                            label: "Tail gap".into(),
                            audio_edges: Default::default(),
                            kind: NodeKind::Repeat {
                                child: id("a"),
                                iterations: IterationOrder::new(
                                    RevisionId::new("plays").unwrap(),
                                    2,
                                )
                                .unwrap(),
                                gap: Some(HoldRecipe {
                                    duration: duration(8),
                                    video: HoldVideo::Background,
                                    audio: tail.clone(),
                                }),
                            },
                        },
                    ),
                    ("a", source(2)),
                ],
            )
        } else {
            (8, vec![("context", hold(8, tail.clone()))])
        };
        let mut nodes = nodes;
        nodes.push((
            "preserve",
            retime("context", frames / 2, 0, frames, PitchPolicy::Preserve),
        ));
        let doc = document(FrameRate::new(24, 1).unwrap(), &["preserve"], nodes);
        let frozen = compile(&doc);
        let live = RenderPlan::compile(&doc).unwrap();
        let root = frozen.root_clock();
        let query = root
            .query(
                samples(0, root.sample_count().unwrap().0),
                Default::default(),
            )
            .unwrap();
        let expected = live
            .audio(
                AudioSample(0)..AudioSample(root.sample_count().unwrap().0),
                Default::default(),
            )
            .unwrap();
        for (actual, expected) in query.spans.iter().zip(expected.spans) {
            assert_eq!(actual.content, policy(&expected.content));
        }
        let retained = query
            .spans
            .iter()
            .find(|span| matches!(span.content, ReferenceAudioContent::Tail { .. }))
            .unwrap();
        assert_eq!(
            retained.content,
            ReferenceAudioContent::Tail {
                maximum: duration(2)
            }
        );
        assert_eq!(
            retained.allocated_samples.end.0 - retained.allocated_samples.start.0,
            8000
        );
        assert_eq!(retained.gap_after.is_some(), gap);
        // The final sample still reports policy, even beyond the maximum. The
        // reference planner does not implement the currently unsupported DSP.
        let final_tail_sample = retained.samples.end.0 - 1;
        assert_eq!(
            at(
                &root
                    .query(
                        samples(final_tail_sample, final_tail_sample + 1),
                        Default::default()
                    )
                    .unwrap()
                    .spans,
                final_tail_sample
            ),
            retained.content
        );
        for clock in [
            frozen.preserve_input_clock(&instance("preserve")).unwrap(),
            frozen.preserve_output_clock(&instance("preserve")).unwrap(),
        ] {
            let query = clock
                .query(
                    samples(0, clock.sample_count().unwrap().0),
                    Default::default(),
                )
                .unwrap();
            let tail = query
                .spans
                .iter()
                .find(|span| matches!(span.content, ReferenceAudioContent::Tail { .. }))
                .unwrap();
            assert_eq!(tail.content, retained.content);
        }
    }
}

#[test]
fn billion_play_reference_keeps_old_order_gap_identity_after_current_edits() {
    let iterations = IterationOrder::new(RevisionId::new("plays").unwrap(), 1_000_000_000).unwrap();
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["repeat"],
        vec![
            (
                "repeat",
                BeatNode {
                    label: "Compact".into(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("a"),
                        iterations: iterations.clone(),
                        gap: Some(HoldRecipe {
                            duration: duration(1),
                            video: HoldVideo::Background,
                            audio: HoldAudio::RoomTone { source: audio() },
                        }),
                    },
                },
            ),
            ("a", source(1)),
        ],
    );
    let old = compile(&doc);
    let clock = old.root_clock();
    let expected = clock
        .query(
            samples(1_999_999_994, 1_999_999_999),
            AudioQueryLimits {
                maximum_spans: 5,
                maximum_work: 100,
            },
        )
        .unwrap();
    assert_eq!(expected.spans.len(), 5);
    assert!(expected.lookup.visited_nodes < 20);
    assert!(expected.lookup.iteration_run_comparisons <= 5);
    assert_eq!(expected.spans[1].content, ReferenceAudioContent::RoomTone);
    assert_eq!(expected.spans[1].gap_after, iterations.at(999_999_997));
    let split = edit(
        &doc,
        "split",
        Command::Split {
            node: id("repeat"),
            at: duration(5),
            identities: SplitIdentities {
                nodes: (0..16).map(|n| id(&format!("split-{n}"))).collect(),
            },
        },
    );
    assert!(split.nodes().len() > doc.nodes().len());
    assert_eq!(
        clock
            .query(samples(1_999_999_994, 1_999_999_999), Default::default())
            .unwrap()
            .spans,
        expected.spans
    );
    let moved = edit(
        &doc,
        "move",
        Command::MovePlays {
            node: id("repeat"),
            start: 999_999_999,
            end: 1_000_000_000,
            destination: 0,
        },
    );
    let shrunk = edit(
        &moved,
        "shrink",
        Command::SetRepeat {
            node: id("repeat"),
            plays: 2,
            gap: None,
        },
    );
    let removed = edit(&shrunk, "delete", Command::Delete { node: id("repeat") });
    assert_eq!(removed.duration().unwrap(), FrameDuration::ZERO);
    assert_eq!(
        clock
            .query(samples(1_999_999_994, 1_999_999_999), Default::default())
            .unwrap()
            .spans,
        expected.spans
    );
}

#[test]
fn sparse_override_clock_validates_its_effective_play_and_retains_partition_allocation() {
    let doc = document(
        FrameRate::new(24, 1).unwrap(),
        &["repeat"],
        vec![
            (
                "repeat",
                BeatNode {
                    label: "Repeated".into(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("default"),
                        iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 3)
                            .unwrap(),
                        gap: None,
                    },
                },
            ),
            ("default", retime("a", 2, 0, 4, PitchPolicy::Preserve)),
            ("a", source(4)),
        ],
    );
    let iteration = IterationId {
        allocation: RevisionId::new("plays").unwrap(),
        ordinal: 1,
    };
    let overridden = edit(
        &doc,
        "override",
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration: iteration.clone(),
            subtree: Subtree {
                root: id("override"),
                nodes: BTreeMap::from([
                    (
                        id("override"),
                        retime("quiet", 3, 0, 4, PitchPolicy::Preserve),
                    ),
                    (id("quiet"), hold(4, HoldAudio::Silence)),
                ]),
                overrides: BTreeMap::new(),
            },
        },
    );
    let frozen = compile(&overridden);
    let path = InstancePath {
        node: id("override"),
        repeats: vec![RepeatInstance {
            node: id("repeat"),
            iteration: iteration.clone(),
        }],
    };
    let clock = frozen.preserve_input_clock(&path).unwrap();
    assert_eq!(clock.sample_count().unwrap(), ReferenceSample(8000));
    let query = clock.query(samples(0, 1), Default::default()).unwrap();
    assert_eq!(query.spans[0].instance.node, id("quiet"));
    assert_eq!(query.spans[0].instance.repeats, path.repeats);
    let wrong = InstancePath {
        node: id("default"),
        ..path
    };
    assert!(frozen.preserve_output_clock(&wrong).is_err());
    let split = edit(
        &overridden,
        "split",
        Command::Split {
            node: id("repeat"),
            at: duration(3),
            identities: SplitIdentities {
                nodes: (0..20).map(|n| id(&format!("split-{n}"))).collect(),
            },
        },
    );
    let split_reference = compile(&split);
    let live = RenderPlan::compile(&split).unwrap();
    let actual = split_reference
        .root_clock()
        .query(samples(0, 14000), Default::default())
        .unwrap();
    let expected = live
        .audio(AudioSample(0)..AudioSample(14000), Default::default())
        .unwrap();
    assert_eq!(actual.spans.len(), expected.spans.len());
    for (actual, expected) in actual.spans.iter().zip(expected.spans) {
        assert_eq!(actual.content, policy(&expected.content));
        assert_eq!(
            actual.allocated_samples,
            samples(
                expected.allocated_samples.start.0,
                expected.allocated_samples.end.0
            )
        );
        assert_eq!(actual.instance, expected.instance);
    }
}

#[test]
fn invalid_owner_paths_ranges_and_budgets_fail_before_results() {
    let doc = document(
        FrameRate::new(24, 1).unwrap(),
        &["repeat"],
        vec![
            (
                "repeat",
                BeatNode {
                    label: "Repeated".into(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("preserve"),
                        iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2)
                            .unwrap(),
                        gap: None,
                    },
                },
            ),
            ("preserve", retime("a", 2, 0, 4, PitchPolicy::Preserve)),
            ("a", source(4)),
        ],
    );
    let plan = compile(&doc);
    assert!(plan.preserve_input_clock(&instance("a")).is_err());
    assert!(plan.preserve_input_clock(&instance("preserve")).is_err());
    assert!(plan.preserve_output_clock(&instance("missing")).is_err());
    let mut selected = instance("preserve");
    selected.repeats.push(RepeatInstance {
        node: id("repeat"),
        iteration: IterationId {
            allocation: RevisionId::new("plays").unwrap(),
            ordinal: 1,
        },
    });
    assert!(plan.preserve_input_clock(&selected).is_ok());
    selected.repeats[0].iteration.ordinal = 2;
    assert!(plan.preserve_input_clock(&selected).is_err());
    let root = plan.root_clock();
    for range in [samples(-1, 0), samples(2, 1), samples(0, 8001)] {
        assert!(matches!(
            root.query(range, Default::default()),
            Err(PlanError::AudioRangeOutOfRange)
        ));
    }
    assert!(
        root.query(samples(0, 0), Default::default())
            .unwrap()
            .spans
            .is_empty()
    );
    assert!(matches!(
        root.query(
            samples(0, 1),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 1
            }
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
    assert!(matches!(
        root.query(
            samples(0, 8000),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 100
            }
        ),
        Err(PlanError::AudioQueryLimit("span count"))
    ));
    for limits in [
        AudioQueryLimits {
            maximum_spans: 0,
            maximum_work: 100,
        },
        AudioQueryLimits {
            maximum_spans: 1,
            maximum_work: 65_537,
        },
    ] {
        assert!(matches!(
            root.query(samples(0, 1), limits),
            Err(PlanError::InvalidAudioLimits)
        ));
    }
}
