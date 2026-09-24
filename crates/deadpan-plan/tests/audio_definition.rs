use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{
    AudioBoundaryRule, AudioContent, AudioDefinitionSelector, AudioQueryLimits, AudioSignalContent,
    AudioStage, PlanError, RenderPlan, SignalSample,
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
