use std::{collections::BTreeMap, ops::Range};

use deadpan_core::*;
use deadpan_plan::{
    AudioDefinitionSelector, AudioHoldIssuer, AudioHoldRule, AudioQueryLimits, AudioRootPlacement,
    AudioSignalContent, PlanError, RenderPlan, SignalSample,
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
fn points(start: i64, end: i64) -> Range<SignalSample> {
    SignalSample(start)..SignalSample(end)
}
fn play(ordinal: u32) -> IterationId {
    IterationId {
        allocation: RevisionId::new("plays").unwrap(),
        ordinal,
    }
}
fn recipe(length: i64, audio: HoldAudio) -> HoldRecipe {
    HoldRecipe {
        duration: frames(length),
        video: HoldVideo::Background,
        picture_context: None,
        audio,
    }
}
fn hold(length: i64) -> BeatNode {
    BeatNode::hold("Silence", recipe(length, HoldAudio::Silence))
}
fn sequence(children: &[&str]) -> BeatNode {
    BeatNode::sequence("Sequence", children.iter().map(|name| id(name)).collect())
}
fn repeat(child: &str, count: u32, gap: Option<i64>) -> BeatNode {
    BeatNode {
        label: "Repeat".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(play(0).allocation, count).unwrap(),
            gap: gap.map(|length| recipe(length, HoldAudio::Silence)),
        },
    }
}
fn retime(child: &str, length: i64, start: i64, end: i64, pitch: PitchPolicy) -> BeatNode {
    BeatNode {
        label: "Retime".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: frames(length),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch,
            purpose: RetimePurpose::Edit,
        },
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
                ticks: 16,
                time_base,
            },
        )
        .unwrap(),
    }
}
fn source(length: i64, audio: Option<SourceAudio>, mapping: SourceAudioMapping) -> BeatNode {
    BeatNode {
        label: "Source".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: frames(length),
                video: SourceVideo::Still {
                    asset: AssetId::new("original").unwrap(),
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio,
                audio_mapping: mapping,
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
    }
}
fn document(
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("hold-policy").unwrap(),
        RevisionId::new("current").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(id("root"), sequence(children));
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
fn compile(
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
) -> RenderPlan {
    RenderPlan::compile(&document(children, nodes)).unwrap()
}
fn node_issuer(name: &str, repeats: Vec<RepeatInstance>) -> AudioHoldIssuer {
    AudioHoldIssuer::Node {
        definition: None,
        instance: InstancePath {
            node: id(name),
            repeats,
        },
    }
}
fn occurrence(repeat: &str, ordinal: u32) -> RepeatInstance {
    RepeatInstance {
        node: id(repeat),
        iteration: play(ordinal),
    }
}

#[test]
fn adjacent_issuers_survive_complete_nested_retime_maps() {
    let plan = compile(
        &["outer"],
        [
            ("a", hold(2)),
            ("b", hold(2)),
            ("c", hold(4)),
            ("group", sequence(&["a", "b", "c"])),
            ("inner", retime("group", 4, 0, 8, PitchPolicy::FollowSpeed)),
            ("outer", retime("inner", 6, 1, 4, PitchPolicy::Preserve)),
        ],
    );
    let result = plan
        .audio_hold_policy(samples(0, 6), Default::default())
        .unwrap();
    assert_eq!(result.project_id, ProjectId::new("hold-policy").unwrap());
    assert_eq!(result.revision_id, RevisionId::new("current").unwrap());
    assert_eq!(result.samples, samples(0, 6));
    assert_eq!(
        result.rules,
        vec![
            AudioHoldRule {
                samples: samples(0, 2),
                issuer: node_issuer("b", vec![])
            },
            AudioHoldRule {
                samples: samples(2, 6),
                issuer: node_issuer("c", vec![])
            },
        ]
    );
    let point = plan
        .audio_signal()
        .hold_policy(points(0, 6), Default::default())
        .unwrap();
    assert_eq!(point.rules[0].samples, points(0, 2));
    assert_eq!(point.rules[1].samples, points(2, 6));
    assert_eq!(point.rules[0].issuer, result.rules[0].issuer);
    assert_eq!(point.rules[1].issuer, result.rules[1].issuer);
    // The ordinary single-voice adapter still deliberately unions these ranges.
    assert_eq!(
        plan.audio_policy(samples(0, 6), Default::default())
            .unwrap()
            .suppressed,
        vec![samples(0, 6)]
    );
}

#[test]
fn each_grid_rounds_complete_extents_once_and_omits_empty_allocations() {
    let plan = compile(
        &["speed"],
        [
            ("a", hold(1)),
            ("b", hold(3)),
            ("group", sequence(&["a", "b"])),
            ("speed", retime("group", 2, 0, 4, PitchPolicy::Preserve)),
        ],
    );
    // The first Hold ends at exactly one half sample: RoundEven allocates no
    // root samples, while PointCeil assigns sample zero to that same issuer.
    let root = plan
        .audio_hold_policy(samples(0, 2), Default::default())
        .unwrap();
    assert_eq!(
        root.rules,
        vec![AudioHoldRule {
            samples: samples(0, 2),
            issuer: node_issuer("b", vec![])
        }]
    );
    let point = plan
        .audio_signal()
        .hold_policy(points(0, 2), Default::default())
        .unwrap();
    assert_eq!(
        point.rules,
        vec![
            AudioHoldRule {
                samples: points(0, 1),
                issuer: node_issuer("a", vec![])
            },
            AudioHoldRule {
                samples: points(1, 2),
                issuer: node_issuer("b", vec![])
            },
        ]
    );
}

#[test]
fn repeated_hold_and_gap_issuers_follow_stable_reordered_plays() {
    let mut repeated = repeat("hold", 3, Some(1));
    let NodeKind::Repeat { iterations, .. } = &mut repeated.kind else {
        unreachable!()
    };
    *iterations = iterations.moved(0, 1, 2).unwrap();
    let plan = compile(&["repeat"], [("hold", hold(2)), ("repeat", repeated)]);
    let query = plan
        .audio_hold_policy(samples(0, 8), Default::default())
        .unwrap();
    assert_eq!(query.rules.len(), 5);
    for (index, ordinal) in [1, 2, 0].into_iter().enumerate() {
        assert_eq!(
            query.rules[index * 2].issuer,
            node_issuer("hold", vec![occurrence("repeat", ordinal)])
        );
        if index < 2 {
            assert_eq!(
                query.rules[index * 2 + 1].issuer,
                AudioHoldIssuer::RepeatGap {
                    definition: None,
                    instance: InstancePath {
                        node: id("repeat"),
                        repeats: vec![]
                    },
                    gap_after: Some(play(ordinal)),
                }
            );
        }
    }
    assert_eq!(query.rules.last().unwrap().samples, samples(6, 8));
    let point = plan
        .audio_signal()
        .hold_policy(points(0, 8), Default::default())
        .unwrap();
    assert_eq!(
        point
            .rules
            .iter()
            .map(|rule| &rule.issuer)
            .collect::<Vec<_>>(),
        query
            .rules
            .iter()
            .map(|rule| &rule.issuer)
            .collect::<Vec<_>>()
    );
}

#[test]
fn one_play_gap_definition_retains_its_namespace_without_a_fabricated_play() {
    let plan = compile(
        &["repeat"],
        [
            ("source", source(2, None, SourceAudioMapping::FitBeat)),
            ("repeat", repeat("source", 1, Some(3))),
        ],
    );
    assert!(
        plan.audio_hold_policy(samples(0, 2), Default::default())
            .unwrap()
            .rules
            .is_empty()
    );
    let selector = AudioDefinitionSelector::RepeatGap {
        repeat: id("repeat"),
    };
    let definition = plan.audio_definition(selector.clone()).unwrap();
    let issuer = AudioHoldIssuer::RepeatGap {
        definition: Some(selector),
        instance: InstancePath {
            node: id("repeat"),
            repeats: vec![],
        },
        gap_after: None,
    };
    let limits = AudioQueryLimits {
        maximum_spans: 1,
        maximum_work: 1,
    };
    let intrinsic = definition
        .signal()
        .hold_policy(points(0, 3), limits)
        .unwrap();
    assert_eq!(
        intrinsic.rules,
        vec![AudioHoldRule {
            samples: points(0, 3),
            issuer: issuer.clone()
        }]
    );
    assert_eq!(intrinsic.work, 1);
    assert!(
        intrinsic.rules[0].issuer.sound_issuer().is_none(),
        "an unplayed intrinsic gap cannot authorize a root occurrence"
    );
    assert_eq!(intrinsic.lookup.iteration_run_comparisons, 0);
    let placement = AudioRootPlacement::new(
        ExactRatio::integer(-2),
        ExactRatio::integer(2),
        ExactRatio::ZERO..ExactRatio::integer(3),
    )
    .unwrap();
    let domain = definition.in_root_clock(placement.clone()).unwrap();
    let root = domain
        .hold_policy(
            samples(-2, 4),
            AudioQueryLimits {
                maximum_work: 128,
                ..limits
            },
        )
        .unwrap();
    assert_eq!(
        root.rules,
        vec![AudioHoldRule {
            samples: samples(-2, 4),
            issuer: issuer.clone()
        }]
    );
    let point = definition
        .in_point_clock(placement, ExactRatio::new(1, 3).unwrap())
        .unwrap();
    let point = point.signal().hold_policy(points(0, 6), limits).unwrap();
    assert_eq!(
        point.rules,
        vec![AudioHoldRule {
            samples: points(0, 6),
            issuer
        }]
    );
}

#[test]
fn borrowed_stage_input_keeps_definition_and_outer_occurrence_scopes() {
    let plan = compile(
        &["outer"],
        [
            ("hold", hold(2)),
            ("inner", repeat("hold", 2, Some(1))),
            ("stage", retime("inner", 10, 0, 5, PitchPolicy::Preserve)),
            ("outer", repeat("stage", 2, None)),
        ],
    );
    for (signal, definition, outer) in [
        (plan.audio_signal(), None, true),
        (
            plan.audio_definition(AudioDefinitionSelector::RepeatDefault {
                repeat: id("outer"),
            })
            .unwrap()
            .signal(),
            Some(AudioDefinitionSelector::RepeatDefault {
                repeat: id("outer"),
            }),
            false,
        ),
    ] {
        let start = if outer { 10 } else { 0 };
        let mut query = signal
            .query(points(start, start + 1), Default::default())
            .unwrap();
        let AudioSignalContent::Stage(stage) = query.spans.remove(0).content else {
            panic!("expected Preserve stage")
        };
        let rules = stage
            .input_signal()
            .hold_policy(points(0, 5), Default::default())
            .unwrap()
            .rules;
        let enclosing = if outer {
            vec![occurrence("outer", 1)]
        } else {
            vec![]
        };
        let mut leaf = enclosing.clone();
        leaf.push(occurrence("inner", 0));
        assert_eq!(
            rules[0].issuer,
            AudioHoldIssuer::Node {
                definition: definition.clone(),
                instance: InstancePath {
                    node: id("hold"),
                    repeats: leaf
                },
            }
        );
        assert_eq!(
            rules[1].issuer,
            AudioHoldIssuer::RepeatGap {
                definition,
                instance: InstancePath {
                    node: id("inner"),
                    repeats: enclosing
                },
                gap_after: Some(play(0)),
            }
        );
    }
}

#[test]
fn coincident_placed_definitions_keep_distinct_issuers_through_preserve() {
    let plan = compile(
        &["a-stage", "b-stage"],
        [
            ("a", hold(4)),
            ("b", hold(4)),
            ("a-stage", retime("a", 2, 0, 4, PitchPolicy::Preserve)),
            ("b-stage", retime("b", 2, 0, 4, PitchPolicy::Preserve)),
        ],
    );
    let mut issuers = Vec::new();
    for (stage, leaf) in [("a-stage", "a"), ("b-stage", "b")] {
        let selector = AudioDefinitionSelector::Node { node: id(stage) };
        let definition = plan.audio_definition(selector.clone()).unwrap();
        let domain = definition
            .in_root_clock(
                AudioRootPlacement::new(
                    ExactRatio::integer(-1),
                    ExactRatio::integer(2),
                    ExactRatio::ZERO..ExactRatio::integer(2),
                )
                .unwrap(),
            )
            .unwrap();
        let query = domain
            .hold_policy(samples(-1, 3), Default::default())
            .unwrap();
        assert_eq!(query.rules.len(), 1);
        assert_eq!(query.rules[0].samples, samples(-1, 3));
        assert_eq!(
            query.rules[0].issuer,
            AudioHoldIssuer::Node {
                definition: Some(selector),
                instance: InstancePath {
                    node: id(leaf),
                    repeats: vec![]
                }
            }
        );
        issuers.push(query.rules[0].issuer.clone());
    }
    assert_ne!(issuers[0], issuers[1]);
}

#[test]
fn source_absence_exhaustion_room_tone_and_tail_do_not_issue_silent_hold_rules() {
    let plan = compile(
        &["absent", "exhausted", "room", "tail", "hold"],
        [
            ("absent", source(2, None, SourceAudioMapping::FitBeat)),
            (
                "exhausted",
                source(
                    4,
                    Some(audio()),
                    SourceAudioMapping::SelectedPlacement {
                        start: ExactRatio::ZERO,
                        frames: ExactRatio::integer(4),
                        selection: ExactFrameRange::new(ExactRatio::ONE, ExactRatio::integer(2))
                            .unwrap(),
                    },
                ),
            ),
            (
                "room",
                BeatNode::hold(
                    "Room tone",
                    recipe(2, HoldAudio::RoomTone { source: audio() }),
                ),
            ),
            (
                "tail",
                BeatNode::hold(
                    "Tail",
                    recipe(
                        2,
                        HoldAudio::Tail {
                            source: audio(),
                            maximum: frames(2),
                        },
                    ),
                ),
            ),
            ("hold", hold(2)),
        ],
    );
    let root = plan
        .audio_hold_policy(samples(0, 12), Default::default())
        .unwrap();
    assert_eq!(
        root.rules,
        vec![AudioHoldRule {
            samples: samples(10, 12),
            issuer: node_issuer("hold", vec![])
        }]
    );
    let point = plan
        .audio_signal()
        .hold_policy(points(0, 12), Default::default())
        .unwrap();
    assert_eq!(
        point.rules,
        vec![AudioHoldRule {
            samples: points(10, 12),
            issuer: node_issuer("hold", vec![])
        }]
    );
}

#[test]
fn partitions_clip_rules_without_restarting_identity_and_share_query_limits() {
    let plan = compile(
        &["a", "b", "c"],
        [("a", hold(2)), ("b", hold(3)), ("c", hold(2))],
    );
    let whole = plan
        .audio_hold_policy(samples(0, 7), Default::default())
        .unwrap();
    for range in [samples(0, 1), samples(1, 4), samples(4, 7), samples(3, 3)] {
        let query = plan
            .audio_hold_policy(range.clone(), Default::default())
            .unwrap();
        let expected: Vec<_> = whole
            .rules
            .iter()
            .filter_map(|rule| {
                let start = rule.samples.start.max(range.start);
                let end = rule.samples.end.min(range.end);
                (start < end).then(|| AudioHoldRule {
                    samples: start..end,
                    issuer: rule.issuer.clone(),
                })
            })
            .collect();
        assert_eq!(query.samples, range);
        assert_eq!(query.rules, expected);
        assert!(query.work <= AudioQueryLimits::default().maximum_work);
    }
    let exact = AudioQueryLimits {
        maximum_spans: 3,
        maximum_work: whole.work,
    };
    assert_eq!(plan.audio_hold_policy(samples(0, 7), exact).unwrap(), whole);
    assert!(matches!(
        plan.audio_hold_policy(
            samples(0, 7),
            AudioQueryLimits {
                maximum_work: whole.work - 1,
                ..exact
            }
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
    assert!(matches!(
        plan.audio_hold_policy(
            samples(0, 7),
            AudioQueryLimits {
                maximum_spans: 2,
                ..exact
            }
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
    assert!(matches!(
        plan.audio_hold_policy(samples(0, 8), exact),
        Err(PlanError::AudioRangeOutOfRange)
    ));
    assert!(matches!(
        plan.audio_hold_policy(
            samples(0, 0),
            AudioQueryLimits {
                maximum_work: 0,
                ..exact
            }
        ),
        Err(PlanError::InvalidAudioLimits)
    ));
    let no_holds = compile(
        &["a", "b"],
        [
            ("a", source(1, None, SourceAudioMapping::FitBeat)),
            ("b", source(1, None, SourceAudioMapping::FitBeat)),
        ],
    );
    assert!(matches!(
        no_holds.audio_hold_policy(
            samples(0, 2),
            AudioQueryLimits {
                maximum_spans: 1,
                ..Default::default()
            }
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
}

#[test]
fn retained_timing_does_not_invent_an_old_silent_hold_rule() {
    let doc = document(&["hold"], [("hold", hold(4))]);
    let bindings = capture_unbound_audio_bindings(
        &doc,
        AudioTimingId {
            allocation: RevisionId::new("capture").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    wire["nodes"]["hold"] = serde_json::to_value(BeatNode::hold(
        "Room tone",
        recipe(4, HoldAudio::RoomTone { source: audio() }),
    ))
    .unwrap();
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    assert!(
        plan.audio_hold_policy(samples(0, 4), Default::default())
            .unwrap()
            .rules
            .is_empty()
    );
    assert!(
        plan.audio_signal()
            .hold_policy(points(0, 4), Default::default())
            .unwrap()
            .rules
            .is_empty()
    );
}

#[test]
fn narrow_billion_repeat_lookup_keeps_gap_and_hold_identity_without_expansion() {
    let plan = compile(
        &["repeat"],
        [
            ("hold", hold(1)),
            ("repeat", repeat("hold", 1_000_000_000, Some(1))),
        ],
    );
    let limits = AudioQueryLimits {
        maximum_spans: 2,
        maximum_work: 128,
    };
    let query = plan
        .audio_hold_policy(samples(1_999_999_995, 1_999_999_997), limits)
        .unwrap();
    assert_eq!(query.rules.len(), 2);
    assert_eq!(
        query.rules[0].issuer,
        AudioHoldIssuer::RepeatGap {
            definition: None,
            instance: InstancePath {
                node: id("repeat"),
                repeats: vec![]
            },
            gap_after: Some(play(999_999_997)),
        }
    );
    assert_eq!(
        query.rules[1].issuer,
        node_issuer("hold", vec![occurrence("repeat", 999_999_998)])
    );
    assert!(query.work < 128);
    assert!(query.lookup.visited_nodes < 10);
    assert!(query.lookup.iteration_run_comparisons < 10);
    let point = plan
        .audio_signal()
        .hold_policy(points(1_999_999_995, 1_999_999_997), limits)
        .unwrap();
    assert_eq!(point.rules[0].issuer, query.rules[0].issuer);
    assert_eq!(point.rules[1].issuer, query.rules[1].issuer);
}
