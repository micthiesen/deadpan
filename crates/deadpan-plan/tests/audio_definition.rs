use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{
    AudioBoundaryRule, AudioContent, AudioDefinitionSelector, AudioQueryLimits, AudioRootPlacement,
    AudioSignalContent, AudioStage, PlanError, RenderPlan, SignalSample,
};

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn selection(node: &str) -> AudioDefinitionSelector {
    AudioDefinitionSelector::Node { node: id(node) }
}
fn default_selection(repeat: &str) -> AudioDefinitionSelector {
    AudioDefinitionSelector::RepeatDefault { repeat: id(repeat) }
}
fn play(ordinal: u32) -> IterationId {
    IterationId {
        allocation: RevisionId::new("plays").unwrap(),
        ordinal,
    }
}

fn source(length: i64) -> BeatNode {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    BeatNode {
        label: "Source".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: frames(length),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(SourceAudio {
                    asset: AssetId::new("original").unwrap(),
                    span: SourceSpan::new(
                        SourceTimestamp {
                            ticks: 0,
                            time_base,
                        },
                        SourceTimestamp {
                            ticks: 256,
                            time_base,
                        },
                    )
                    .unwrap(),
                }),
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
    }
}

fn hold(length: i64) -> BeatNode {
    BeatNode::hold(
        "Silence",
        HoldRecipe {
            duration: frames(length),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}

fn repeat(child: &str, count: u32) -> BeatNode {
    BeatNode {
        label: "Repeat".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(play(0).allocation, count).unwrap(),
            gap: None,
        },
    }
}

fn retime(
    child: &str,
    length: i64,
    start: i64,
    end: i64,
    pitch: PitchPolicy,
    purpose: RetimePurpose,
) -> BeatNode {
    BeatNode {
        label: "Retime".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: frames(length),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch,
            purpose,
        },
    }
}

fn document(
    rate: FrameRate,
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("definitions").unwrap(),
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
    let NodeKind::Source { source: original } = source(1).kind else {
        unreachable!()
    };
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["overrides"] = serde_json::to_value(overrides).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("original").unwrap(),
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(original.audio.unwrap().span),
            still_image: true,
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

fn first_stage<'plan>(signal: &deadpan_plan::AudioSignal<'plan>) -> AudioStage<'plan> {
    let query = signal
        .query(SignalSample(0)..SignalSample(1), Default::default())
        .unwrap();
    let AudioSignalContent::Stage(stage) = query.spans.into_iter().next().unwrap().content else {
        panic!("expected Preserve")
    };
    stage
}

fn placement(origin: i64, scale: i64, start: i64, end: i64) -> AudioRootPlacement {
    AudioRootPlacement::new(
        ExactRatio::integer(origin),
        ExactRatio::integer(scale),
        ExactRatio::integer(start)..ExactRatio::integer(end),
    )
    .unwrap()
}

#[test]
fn owned_source_uses_absolute_signed_root_phase_without_selecting_root_siblings() {
    let plan = RenderPlan::compile(&document(
        FrameRate::new(30_000, 1001).unwrap(),
        &["silence", "source"],
        [("silence", hold(4)), ("source", source(4))],
        BTreeMap::new(),
    ))
    .unwrap();
    let definition = plan.audio_definition(selection("source")).unwrap();
    for (origin, start, end, first_ticks) in [
        (1, 1602, 8008, ExactRatio::new(16, 1001).unwrap()),
        (-1, -1602, 4805, ExactRatio::new(-16, 1001).unwrap()),
    ] {
        let placement = placement(origin, 1, 0, 4);
        let domain = definition.in_root_clock(placement.clone()).unwrap();
        assert_eq!(domain.root_samples(), AudioSample(start)..AudioSample(end));
        assert_eq!(domain.visible_samples(), domain.root_samples());
        assert_eq!(domain.placement(), Some(&placement));
        assert_eq!(domain.definition(), Some(definition.selector()));
        assert!(domain.belongs_to(&plan));
        assert!(!domain.belongs_to(&plan.clone()));
        let query = domain
            .audio(
                AudioSample(start)..AudioSample(start + 1),
                Default::default(),
            )
            .unwrap();
        let span = &query.spans[0];
        assert_eq!(
            span.source_point(AudioSample(start)).unwrap().ticks,
            first_ticks
        );
        assert_eq!(span.definition.as_ref(), Some(definition.selector()));
        assert_eq!(span.instance.repeats, vec![]);
        assert_eq!(span.grid.frame_origin(), ExactRatio::ZERO);
        assert_eq!(span.grid.boundary_rule(), AudioBoundaryRule::RoundEven);
        assert_eq!(span.transform.project_origin, ExactRatio::integer(origin));
        assert_eq!(
            serde_json::to_value(span).unwrap()["definition"]["node"],
            "source"
        );
    }
    let ordinary = plan
        .audio_domain_at(AudioSample(1602), Default::default())
        .unwrap();
    assert_eq!(ordinary.definition(), None);
    assert_eq!(ordinary.placement(), None);
    let ordinary = ordinary
        .audio(AudioSample(1602)..AudioSample(1603), Default::default())
        .unwrap();
    assert!(matches!(
        ordinary.spans[0].content,
        AudioContent::Silence { .. }
    ));
    assert!(
        serde_json::to_value(&ordinary.spans[0])
            .unwrap()
            .get("definition")
            .is_none()
    );
}

#[test]
fn owned_source_support_limits_filter_and_envelope_without_rebasing_its_clock() {
    let plan = compile(&["source"], [("source", source(4))]);
    let definition = plan.audio_definition(selection("source")).unwrap();
    let domain = definition.in_root_clock(placement(-1, 2, 1, 3)).unwrap();
    assert_eq!(domain.root_samples(), AudioSample(1)..AudioSample(5));
    let query = domain
        .audio(AudioSample(1)..AudioSample(5), Default::default())
        .unwrap();
    let span = &query.spans[0];
    let AudioContent::Source { support, .. } = &span.content else {
        panic!("Source")
    };
    assert_eq!(support.start.ticks, ExactRatio::integer(64));
    assert_eq!(support.end.ticks, ExactRatio::integer(192));
    assert_eq!(span.envelope_samples, AudioSample(1)..AudioSample(5));
    for boundary in [&span.boundaries.start, &span.boundaries.end] {
        assert_eq!(boundary.len(), 1);
        assert!(boundary[0].placement_support);
        assert_eq!(boundary[0].policy, AudioEdgePolicy::Automatic);
    }
    assert_eq!(
        span.source_point(AudioSample(1)).unwrap().ticks,
        ExactRatio::integer(64)
    );
    assert_eq!(
        span.source_point(AudioSample(2)).unwrap().ticks,
        ExactRatio::integer(96)
    );
    assert!(matches!(
        domain.audio(AudioSample(0)..AudioSample(1), Default::default()),
        Err(PlanError::AudioRangeOutOfRange)
    ));
    assert!(matches!(
        domain.processing(AudioSample(4)..AudioSample(6), Default::default()),
        Err(PlanError::AudioRangeOutOfRange)
    ));
    assert!(matches!(
        domain.processing(
            AudioSample(1)..AudioSample(2),
            AudioQueryLimits {
                maximum_spans: 0,
                maximum_work: 1
            }
        ),
        Err(PlanError::InvalidAudioLimits)
    ));
}

#[test]
fn placement_support_never_moves_a_noncoincident_hard_edge_into_the_crop() {
    let mut source = source(4);
    source.audio_edges.node_start = AudioEdgePolicy::Hard;
    source.audio_edges.node_end = AudioEdgePolicy::Hard;
    let plan = compile(&["source"], [("source", source)]);
    let definition = plan.audio_definition(selection("source")).unwrap();
    for (start, end, expect_hard) in [(1, 3, false), (0, 4, true)] {
        let domain = definition
            .in_root_clock(placement(0, 1, start, end))
            .unwrap();
        let query = domain
            .audio(AudioSample(start)..AudioSample(end), Default::default())
            .unwrap();
        for origins in [
            &query.spans[0].boundaries.start,
            &query.spans[0].boundaries.end,
        ] {
            assert!(
                origins.iter().any(|origin| origin.placement_support
                    && origin.policy == AudioEdgePolicy::Automatic)
            );
            assert_eq!(
                origins
                    .iter()
                    .any(|origin| !origin.placement_support
                        && origin.policy == AudioEdgePolicy::Hard),
                expect_hard
            );
            for origin in origins.iter().filter(|origin| !origin.placement_support) {
                assert!(
                    serde_json::to_value(origin)
                        .unwrap()
                        .get("placement_support")
                        .is_none()
                );
            }
        }
        assert!(matches!(
            domain.audio(
                AudioSample(start)..AudioSample(start + 1),
                AudioQueryLimits {
                    maximum_spans: 1,
                    maximum_work: 1
                }
            ),
            Err(PlanError::AudioQueryLimit(_))
        ));
    }
}

#[test]
fn root_placed_preserve_keeps_full_nested_history_and_definition_scope() {
    let plan = compile(
        &["outer"],
        [
            ("source", source(8)),
            (
                "inner",
                retime(
                    "source",
                    12,
                    0,
                    8,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            (
                "outer",
                retime(
                    "inner",
                    18,
                    0,
                    12,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
        ],
    );
    let definition = plan.audio_definition(selection("outer")).unwrap();
    let domain = definition.in_root_clock(placement(-4, 2, 6, 12)).unwrap();
    let query = domain
        .processing(AudioSample(8)..AudioSample(10), Default::default())
        .unwrap();
    let span = &query.spans[0];
    assert_eq!(span.definition.as_ref(), Some(definition.selector()));
    let AudioSignalContent::Stage(outer) = &span.content else {
        panic!("Preserve")
    };
    assert_eq!(
        outer.descriptor().definition.as_ref(),
        Some(definition.selector())
    );
    assert_eq!(
        outer.input_signal().sample_count().unwrap(),
        SignalSample(12)
    );
    assert_eq!(
        outer.output_signal().sample_count().unwrap(),
        SignalSample(18)
    );
    let inner = first_stage(&outer.input_signal());
    assert_eq!(
        inner.descriptor().definition.as_ref(),
        Some(definition.selector())
    );
    assert_eq!(
        inner.input_signal().sample_count().unwrap(),
        SignalSample(8)
    );
    let flattened = domain
        .audio(AudioSample(8)..AudioSample(10), Default::default())
        .unwrap();
    assert_eq!(
        flattened.spans[0].definition.as_ref(),
        Some(definition.selector())
    );
    assert_eq!(flattened.spans[0].retimes.len(), 2);
    assert!(matches!(
        domain.processing(
            AudioSample(8)..AudioSample(9),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 1
            }
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
}

#[test]
fn root_placement_is_closed_checked_and_rejects_nonphysical_definition_roots() {
    let plan = compile(
        &["repeat", "unity"],
        [
            ("source", source(4)),
            ("repeat", repeat("source", 2)),
            ("hold", hold(4)),
            (
                "unity",
                retime("hold", 4, 0, 4, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
        ],
    );
    for name in ["root", "repeat", "unity"] {
        assert!(matches!(
            plan.audio_definition(selection(name))
                .unwrap()
                .in_root_clock(placement(0, 1, 0, 4)),
            Err(PlanError::InvalidAudioRootPlacement(_))
        ));
    }
    let definition = plan.audio_definition(default_selection("repeat")).unwrap();
    assert!(definition.in_root_clock(placement(0, 1, 0, 4)).is_ok());
    assert!(matches!(
        definition.in_root_clock(placement(0, 1, 0, 5)),
        Err(PlanError::InvalidAudioRootPlacement(_))
    ));
    for (scale, start, end) in [(0, 0, 4), (-1, 0, 4), (1, -1, 4), (1, 2, 2), (1, 3, 2)] {
        assert!(
            AudioRootPlacement::new(
                ExactRatio::ZERO,
                ExactRatio::integer(scale),
                ExactRatio::integer(start)..ExactRatio::integer(end)
            )
            .is_err()
        );
    }
    let valid = placement(-8, 2, 1, 3);
    let wire = serde_json::to_value(&valid).unwrap();
    assert_eq!(
        serde_json::from_value::<AudioRootPlacement>(wire.clone()).unwrap(),
        valid
    );
    let mut extra = wire.clone();
    extra["media"] = serde_json::json!("unrecognized");
    assert!(serde_json::from_value::<AudioRootPlacement>(extra).is_err());
    let mut extra_support = wire.clone();
    extra_support["local_support"]["media"] = serde_json::json!("unrecognized");
    assert!(serde_json::from_value::<AudioRootPlacement>(extra_support).is_err());
    let mut zero = wire;
    zero["root_frames_per_local_frame"] = serde_json::to_value(ExactRatio::ZERO).unwrap();
    assert!(serde_json::from_value::<AudioRootPlacement>(zero).is_err());
    assert!(
        AudioRootPlacement::new(
            ExactRatio::new(i128::MAX, 1).unwrap(),
            ExactRatio::ONE,
            ExactRatio::ZERO..ExactRatio::ONE
        )
        .is_err()
    );
}

#[test]
fn all_overridden_repeat_selects_its_authored_default_without_a_fabricated_play() {
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["repeat"],
        [
            ("default", source(8)),
            ("first", hold(1)),
            ("second", hold(2)),
            ("repeat", repeat("default", 2)),
        ],
        BTreeMap::from([(
            id("repeat"),
            PlayOverrides::try_from(vec![
                PlayOverride {
                    iteration: play(0),
                    root: id("first"),
                },
                PlayOverride {
                    iteration: play(1),
                    root: id("second"),
                },
            ])
            .unwrap(),
        )]),
    );
    for plan in [
        RenderPlan::compile(&doc).unwrap(),
        RenderPlan::compile_audio_context(&FrozenAudioContext::capture(&doc).unwrap()).unwrap(),
    ] {
        assert_eq!(plan.duration(), frames(3));
        let definition = plan.audio_definition(default_selection("repeat")).unwrap();
        assert_eq!(definition.selector(), &default_selection("repeat"));
        assert_eq!(definition.root(), &id("default"));
        assert_eq!(definition.duration(), frames(8));
        assert!(definition.belongs_to(&plan));
        assert!(!definition.belongs_to(&plan.clone()));
        let signal = definition.signal();
        assert_eq!(signal.sample_count().unwrap(), SignalSample(8));
        assert!(signal.belongs_to(&plan));
        assert!(!signal.belongs_to(&plan.clone()));
        assert_eq!(signal.definition(), Some(definition.selector()));
        let query = signal
            .query(SignalSample(0)..SignalSample(8), Default::default())
            .unwrap();
        assert_eq!(query.definition.as_ref(), Some(definition.selector()));
        assert_eq!(query.spans.len(), 1);
        let span = &query.spans[0];
        assert_eq!(span.definition.as_ref(), Some(definition.selector()));
        assert_eq!(
            span.instance,
            InstancePath {
                node: id("default"),
                repeats: vec![]
            }
        );
        assert_eq!(span.gap_after, None);
        assert!(matches!(
            span.content,
            AudioSignalContent::Leaf(AudioContent::Source { .. })
        ));
        assert_eq!(
            span.source_point(SignalSample(0)).unwrap().ticks,
            ExactRatio::ZERO
        );
        assert!(
            plan.audio(AudioSample(0)..AudioSample(3), Default::default())
                .unwrap()
                .spans
                .iter()
                .all(|span| span.instance.node != id("default"))
        );
        let wire = serde_json::to_value(query).unwrap();
        assert_eq!(
            wire["definition"],
            serde_json::json!({"type": "repeat_default", "repeat": "repeat"})
        );
        assert_eq!(wire["spans"][0]["definition"], wire["definition"]);
    }
}

#[test]
fn definition_output_uses_local_zero_and_point_ceil_at_ntsc() {
    let plan = RenderPlan::compile(&document(
        FrameRate::new(30_000, 1001).unwrap(),
        &["prefix", "crop"],
        [
            ("prefix", hold(1)),
            ("source", source(2)),
            (
                "crop",
                retime(
                    "source",
                    1,
                    1,
                    2,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Partition,
                ),
            ),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    let definition = plan.audio_definition(selection("source")).unwrap();
    let signal = definition.signal();
    assert_eq!(definition.duration(), frames(2));
    assert_eq!(signal.support(), ExactRatio::ZERO..ExactRatio::integer(2));
    assert_eq!(signal.sample_count().unwrap(), SignalSample(3204));
    assert_eq!(plan.audio_duration().unwrap(), AudioSample(3203));
    let query = signal
        .query(SignalSample(3203)..SignalSample(3204), Default::default())
        .unwrap();
    assert_eq!(query.spans[0].transform.signal_origin, ExactRatio::ZERO);
    assert_eq!(query.spans[0].grid.frame_origin(), ExactRatio::ZERO);
    assert_eq!(
        query.spans[0].grid.boundary_rule(),
        AudioBoundaryRule::PointCeil
    );
    assert_eq!(
        query.spans[0].sampling.local_at(SignalSample(0)).unwrap(),
        ExactRatio::ZERO
    );
    assert!(matches!(
        plan.audio(AudioSample(3203)..AudioSample(3204), Default::default()),
        Err(PlanError::AudioRangeOutOfRange)
    ));
}

#[test]
fn nested_preserve_signals_retain_scope_and_cannot_alias_project_stage_descriptors() {
    let plan = compile(
        &["outer"],
        [
            ("source", source(8)),
            (
                "inner",
                retime(
                    "source",
                    12,
                    0,
                    8,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            (
                "outer",
                retime(
                    "inner",
                    18,
                    0,
                    12,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
        ],
    );
    let selected = selection("outer");
    let definition = plan.audio_definition(selected.clone()).unwrap();
    let outer = first_stage(&definition.signal());
    assert_eq!(outer.descriptor().definition, Some(selected.clone()));
    assert_eq!(
        outer.descriptor().instance,
        InstancePath {
            node: id("outer"),
            repeats: vec![]
        }
    );
    for signal in [outer.input_signal(), outer.output_signal()] {
        assert_eq!(signal.definition(), Some(&selected));
        assert!(signal.belongs_to(&plan));
        let flat = signal
            .query_flattened(SignalSample(0)..SignalSample(1), Default::default())
            .unwrap();
        assert_eq!(flat.definition, Some(selected.clone()));
        assert!(
            flat.spans
                .iter()
                .all(|span| span.definition == Some(selected.clone()))
        );
    }
    let inner = first_stage(&outer.input_signal());
    assert_eq!(inner.descriptor().definition, Some(selected.clone()));
    assert_eq!(inner.input_signal().definition(), Some(&selected));
    assert_eq!(inner.output_signal().definition(), Some(&selected));
    assert_eq!(
        inner.input_signal().sample_count().unwrap(),
        SignalSample(8)
    );
    let root = first_stage(&plan.audio_signal());
    assert_eq!(root.descriptor().definition, None);
    assert_eq!(root.descriptor().instance, outer.descriptor().instance);
    assert_ne!(root.descriptor(), outer.descriptor());
    assert_ne!(root, outer);
    let domain = plan
        .audio_domain_at(AudioSample(0), Default::default())
        .unwrap();
    let query = domain
        .processing(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let AudioSignalContent::Stage(domain_stage) = &query.spans[0].content else {
        panic!("stage")
    };
    assert_eq!(domain_stage.descriptor(), root.descriptor());
    let wire = serde_json::to_value(
        plan.audio_signal()
            .query(SignalSample(0)..SignalSample(1), Default::default())
            .unwrap(),
    )
    .unwrap();
    assert!(wire.get("definition").is_none());
    assert!(wire["spans"][0].get("definition").is_none());
    assert!(
        wire["spans"][0]["content"]["value"]
            .get("definition")
            .is_none()
    );
    assert_eq!(root.input_signal().definition(), None);
    assert_eq!(root.output_signal().definition(), None);
}

#[test]
fn selector_scope_distinguishes_node_definition_from_repeat_default_definition() {
    let plan = compile(
        &["repeat"],
        [
            ("source", source(8)),
            (
                "stage",
                retime(
                    "source",
                    12,
                    0,
                    8,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            ("repeat", repeat("stage", 2)),
        ],
    );
    let node_definition = plan.audio_definition(selection("stage")).unwrap();
    let default_definition = plan.audio_definition(default_selection("repeat")).unwrap();
    assert_eq!(node_definition.root(), default_definition.root());
    let node_stage = first_stage(&node_definition.signal());
    let default_stage = first_stage(&default_definition.signal());
    assert_eq!(
        node_stage.descriptor().instance,
        default_stage.descriptor().instance
    );
    assert_ne!(node_stage.descriptor(), default_stage.descriptor());
    let root_stage = first_stage(&plan.audio_signal());
    assert_eq!(root_stage.descriptor().instance.repeats.len(), 1);
    assert!(node_stage.descriptor().instance.repeats.is_empty());
    assert!(default_stage.descriptor().instance.repeats.is_empty());
}

#[test]
fn nested_billion_repeat_queries_stay_compact_and_use_only_definition_relative_plays() {
    let plays = 1_000_000_000;
    let plan = compile(
        &["outer"],
        [
            ("source", source(1)),
            ("inner", repeat("source", plays)),
            ("outer", repeat("inner", 2)),
        ],
    );
    let definition = plan.audio_definition(default_selection("outer")).unwrap();
    assert_eq!(definition.root(), &id("inner"));
    assert_eq!(
        definition.signal().sample_count().unwrap(),
        SignalSample(i64::from(plays))
    );
    let query = definition
        .signal()
        .query(
            SignalSample(i64::from(plays) - 1)..SignalSample(i64::from(plays)),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 16,
            },
        )
        .unwrap();
    assert_eq!(
        query.spans[0].instance,
        InstancePath {
            node: id("source"),
            repeats: vec![RepeatInstance {
                node: id("inner"),
                iteration: play(plays - 1)
            }],
        }
    );
    assert_eq!(
        query.spans[0].definition.as_ref(),
        Some(definition.selector())
    );
    assert!(query.lookup.visited_nodes <= 2);
    assert_eq!(plan.metadata().storage.iteration_run_entries, 2);
    assert_eq!(plan.metadata().storage.repeat_segment_entries, 2);
}

#[test]
fn invalid_selectors_and_signal_requests_fail_without_guessing_an_occurrence() {
    let plan = compile(&["source"], [("source", source(2))]);
    for selected in [
        selection("missing"),
        default_selection("missing"),
        default_selection("source"),
    ] {
        assert!(
            matches!(plan.audio_definition(selected.clone()), Err(PlanError::InvalidAudioDefinitionSelector(actual)) if actual == selected)
        );
    }
    for wire in [
        r#"{"type":"node","node":"source","repeat":"source"}"#,
        r#"{"type":"repeat_default","repeat":"source","node":"source"}"#,
        r#"{"type":"occurrence","node":"source"}"#,
    ] {
        assert!(serde_json::from_str::<AudioDefinitionSelector>(wire).is_err());
    }
    let definition = plan.audio_definition(selection("root")).unwrap();
    let signal = definition.signal();
    for range in [
        SignalSample(-1)..SignalSample(0),
        SignalSample(1)..SignalSample(0),
        SignalSample(0)..SignalSample(3),
    ] {
        assert!(matches!(
            signal.query(range, Default::default()),
            Err(PlanError::AudioRangeOutOfRange)
        ));
    }
    assert!(matches!(
        signal.query(
            SignalSample(0)..SignalSample(1),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 1
            }
        ),
        Err(PlanError::AudioQueryLimit("structural work"))
    ));
    assert!(matches!(
        signal.query(
            SignalSample(0)..SignalSample(0),
            AudioQueryLimits {
                maximum_spans: 0,
                maximum_work: 1
            }
        ),
        Err(PlanError::InvalidAudioLimits)
    ));
    let empty = signal
        .query(SignalSample(2)..SignalSample(2), Default::default())
        .unwrap();
    assert!(empty.spans.is_empty());
    assert_eq!(empty.definition.as_ref(), Some(definition.selector()));
    let plan = compile(&["a", "b"], [("a", hold(1)), ("b", hold(1))]);
    assert!(matches!(
        plan.audio_definition(selection("root"))
            .unwrap()
            .signal()
            .query(
                SignalSample(0)..SignalSample(2),
                AudioQueryLimits {
                    maximum_spans: 1,
                    maximum_work: 100
                },
            ),
        Err(PlanError::AudioQueryLimit("span count"))
    ));
}
