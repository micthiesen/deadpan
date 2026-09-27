use super::*;
use deadpan_plan::{AudioBoundaryKind, ReferenceSample, SilenceReason};

fn gap_selection(repeat: &str) -> AudioDefinitionSelector {
    AudioDefinitionSelector::RepeatGap { repeat: id(repeat) }
}

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn gap_repeat(child: &str, count: u32, duration: i64, audio: HoldAudio) -> BeatNode {
    let mut node = repeat(child, count);
    let NodeKind::Repeat { gap, .. } = &mut node.kind else {
        unreachable!()
    };
    *gap = Some(HoldRecipe {
        duration: frames(duration),
        video: HoldVideo::Background,
        picture_context: None,
        audio,
    });
    node
}

fn room_source() -> SourceAudio {
    let NodeKind::Source { source } = source(1).kind else {
        unreachable!()
    };
    source.audio.unwrap()
}

#[test]
fn one_play_gap_definition_exists_without_a_rendered_gap_or_child_audio() {
    let doc = document(
        FrameRate::new(30_000, 1001).unwrap(),
        &["repeat"],
        [
            ("child", source(8)),
            ("repeat", gap_repeat("child", 1, 2, HoldAudio::Silence)),
        ],
        BTreeMap::new(),
    );
    for plan in [
        RenderPlan::compile(&doc).unwrap(),
        RenderPlan::compile_audio_context(&FrozenAudioContext::capture(&doc).unwrap()).unwrap(),
    ] {
        let selector = gap_selection("repeat");
        let definition = plan.audio_definition(selector.clone()).unwrap();
        assert_eq!(plan.duration(), frames(8));
        assert_eq!(definition.duration(), frames(2));
        assert_eq!(definition.root(), &id("repeat"));
        assert!(definition.belongs_to(&plan));
        assert!(!definition.belongs_to(&plan.clone()));
        let signal = definition.signal();
        assert_eq!(signal.support(), ExactRatio::ZERO..ExactRatio::integer(2));
        assert_eq!(signal.sample_count().unwrap(), SignalSample(3204));
        let query = signal
            .query(
                SignalSample(0)..SignalSample(3204),
                AudioQueryLimits {
                    maximum_spans: 1,
                    maximum_work: 1,
                },
            )
            .unwrap();
        assert_eq!(query.spans.len(), 1);
        assert_eq!(query.definition.as_ref(), Some(&selector));
        let span = &query.spans[0];
        assert_eq!(span.definition.as_ref(), Some(&selector));
        assert_eq!(
            span.instance,
            InstancePath {
                node: id("repeat"),
                repeats: vec![]
            }
        );
        assert_eq!(span.gap_after, None);
        assert_eq!(span.transform.signal_origin, ExactRatio::ZERO);
        assert_eq!(span.grid.frame_origin(), ExactRatio::ZERO);
        assert_eq!(span.grid.boundary_rule(), AudioBoundaryRule::PointCeil);
        assert_eq!(
            span.sampling.local_at(SignalSample(0)).unwrap(),
            ExactRatio::ZERO
        );
        assert!(span.retimes.is_empty());
        assert_eq!(
            span.content,
            AudioSignalContent::Leaf(AudioContent::Silence {
                reason: SilenceReason::SilentHold
            })
        );
        assert_eq!(query.lookup.iteration_run_comparisons, 0);
        assert_eq!(query.lookup.visited_nodes, 1);
        assert!(
            plan.audio(
                AudioSample(0)..plan.audio_duration().unwrap(),
                Default::default()
            )
            .unwrap()
            .spans
            .iter()
            .all(|span| span.instance.node == id("child") && span.gap_after.is_none())
        );
        let whole_repeat = plan.audio_definition(selection("repeat")).unwrap();
        assert_ne!(whole_repeat.selector(), definition.selector());
        assert_eq!(whole_repeat.duration(), frames(8));
        assert!(matches!(
            whole_repeat
                .signal()
                .query(SignalSample(0)..SignalSample(1), Default::default())
                .unwrap()
                .spans[0]
                .content,
            AudioSignalContent::Leaf(AudioContent::Source { .. })
        ));
        let wire = serde_json::to_value(&selector).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({"type": "repeat_gap", "repeat": "repeat"})
        );
        assert_eq!(
            serde_json::from_value::<AudioDefinitionSelector>(wire).unwrap(),
            selector
        );
    }
}

#[test]
fn gap_definition_stays_available_when_every_play_uses_an_override() {
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["repeat"],
        [
            ("default", source(8)),
            ("override", hold(1)),
            (
                "repeat",
                gap_repeat(
                    "default",
                    1,
                    4,
                    HoldAudio::RoomTone {
                        source: room_source(),
                    },
                ),
            ),
        ],
        BTreeMap::from([(
            id("repeat"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: play(0),
                root: id("override"),
            }])
            .unwrap(),
        )]),
    );
    for plan in [
        RenderPlan::compile(&doc).unwrap(),
        RenderPlan::compile_audio_context(&FrozenAudioContext::capture(&doc).unwrap()).unwrap(),
    ] {
        assert_eq!(plan.duration(), frames(1));
        let definition = plan.audio_definition(gap_selection("repeat")).unwrap();
        assert_eq!(definition.duration(), frames(4));
        let span = definition
            .signal()
            .query(SignalSample(0)..SignalSample(4), Default::default())
            .unwrap()
            .spans
            .remove(0);
        assert_eq!(
            span.content,
            AudioSignalContent::Leaf(AudioContent::RoomTone {
                source: room_source(),
                duration: frames(4)
            })
        );
        assert_eq!(span.instance.node, id("repeat"));
        assert!(span.instance.repeats.is_empty());
        assert_eq!(span.gap_after, None);
        assert_eq!(
            plan.audio_definition(default_selection("repeat"))
                .unwrap()
                .duration(),
            frames(8)
        );
    }
}

#[test]
fn gap_definition_requires_a_positive_configured_repeat_gap() {
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["repeat"],
        [("child", source(2)), ("repeat", repeat("child", 1))],
        BTreeMap::new(),
    );
    for plan in [
        RenderPlan::compile(&doc).unwrap(),
        RenderPlan::compile_audio_context(&FrozenAudioContext::capture(&doc).unwrap()).unwrap(),
    ] {
        for selector in [
            gap_selection("repeat"),
            gap_selection("child"),
            gap_selection("missing"),
        ] {
            assert!(
                matches!(plan.audio_definition(selector.clone()), Err(PlanError::InvalidAudioDefinitionSelector(actual)) if actual == selector)
            );
        }
    }
    // Authored zero gaps fail admission before a plan can exist.
    let mut zero = serde_json::to_value(&doc).unwrap();
    zero["nodes"]["repeat"] =
        serde_json::to_value(gap_repeat("child", 2, 0, HoldAudio::Silence)).unwrap();
    assert!(ProjectDocument::from_json(&zero.to_string()).is_err());
    for wire in [
        r#"{"type":"repeat_gap","repeat":"repeat","node":"child"}"#,
        r#"{"type":"repeat_gap","repeat":"repeat","after":{"allocation":"plays","ordinal":0}}"#,
    ] {
        assert!(serde_json::from_str::<AudioDefinitionSelector>(wire).is_err());
    }
}

#[test]
fn gap_signed_root_and_point_clocks_keep_exact_support_and_phase() {
    let plan = RenderPlan::compile(&document(
        FrameRate::new(30_000, 1001).unwrap(),
        &["ancestor"],
        [
            ("child", source(8)),
            (
                "repeat",
                gap_repeat(
                    "child",
                    1,
                    2,
                    HoldAudio::RoomTone {
                        source: room_source(),
                    },
                ),
            ),
            (
                "ancestor",
                retime(
                    "repeat",
                    12,
                    0,
                    8,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    let definition = plan.audio_definition(gap_selection("repeat")).unwrap();
    let clock =
        AudioRootPlacement::new(ratio(-1, 3), ratio(3, 2), ratio(1, 4)..ratio(7, 4)).unwrap();
    let root = definition.in_root_clock(clock.clone()).unwrap();
    assert_eq!(root.definition(), Some(definition.selector()));
    assert_eq!(root.placement(), Some(&clock));
    assert_eq!(root.root_extent(), ratio(1, 24)..ratio(55, 24));
    assert_eq!(root.root_samples(), AudioSample(67)..AudioSample(3670));
    assert_eq!(root.gap_after(), None);
    let query = root.audio(root.root_samples(), Default::default()).unwrap();
    let span = &query.spans[0];
    assert_eq!(span.envelope_extent, root.root_extent());
    assert_eq!(span.transform.project_origin, ratio(-1, 3));
    assert_eq!(
        span.sampling.local_at(AudioSample(67)).unwrap(),
        ratio(9013, 36036)
    );
    assert!(span.retimes.is_empty());
    assert!(span.instance.repeats.is_empty());
    assert_eq!(
        span.content,
        AudioContent::RoomTone {
            source: room_source(),
            duration: frames(2)
        }
    );
    let processing = root
        .processing(root.root_samples(), Default::default())
        .unwrap();
    assert_eq!(
        processing.spans[0].definition.as_ref(),
        Some(definition.selector())
    );
    assert_eq!(
        processing.spans[0].content,
        AudioSignalContent::Leaf(span.content.clone())
    );
    assert_eq!(processing.spans[0].gap_after, None);
    let point = definition.in_point_clock(clock, ratio(1, 7)).unwrap();
    assert_eq!(point.root(), &id("repeat"));
    assert_eq!(point.reference_grid().frame_origin(), ratio(1, 7));
    assert_eq!(
        point.reference_samples(),
        ReferenceSample(-162)..ReferenceSample(3442)
    );
    assert_eq!(point.signal().sample_count().unwrap(), SignalSample(3604));
    assert_eq!(
        point
            .reference_sampling()
            .local_at(ReferenceSample(-162))
            .unwrap(),
        ratio(4505, 18018)
    );
    let query = point
        .signal()
        .query(SignalSample(0)..SignalSample(3604), Default::default())
        .unwrap();
    assert_eq!(query.spans.len(), 1);
    assert_eq!(
        query.spans[0].definition.as_ref(),
        Some(definition.selector())
    );
    assert_eq!(query.spans[0].signal_extent, root.root_extent());
    assert_eq!(
        query.spans[0].sampling.local_at(SignalSample(0)).unwrap(),
        ratio(4505, 18018)
    );
    assert_eq!(query.spans[0].gap_after, None);
    assert!(query.spans[0].retimes.is_empty());
    for support in [placement(0, 1, 0, 3), placement(0, 1, 2, 3)] {
        assert!(matches!(
            definition.in_root_clock(support.clone()),
            Err(PlanError::InvalidAudioRootPlacement(_))
        ));
        assert!(matches!(
            definition.in_point_clock(support, ExactRatio::ZERO),
            Err(PlanError::InvalidAudioRootPlacement(_))
        ));
    }
}

#[test]
fn gap_policies_follow_current_recipe_and_exact_gap_edges_without_node_edges() {
    for (audio, expected) in [
        (
            HoldAudio::Silence,
            AudioContent::Silence {
                reason: SilenceReason::SilentHold,
            },
        ),
        (
            HoldAudio::RoomTone {
                source: room_source(),
            },
            AudioContent::RoomTone {
                source: room_source(),
                duration: frames(4),
            },
        ),
        (
            HoldAudio::Tail {
                source: room_source(),
                maximum: frames(2),
            },
            AudioContent::Tail {
                source: room_source(),
                maximum: frames(2),
            },
        ),
    ] {
        let mut repeat = gap_repeat("child", 1, 4, audio);
        repeat.audio_edges.node_start = AudioEdgePolicy::Hard;
        repeat.audio_edges.node_end = AudioEdgePolicy::Hard;
        repeat.audio_edges.repeat_gap_start = AudioEdgePolicy::Hard;
        let plan = compile(&["repeat"], [("child", source(8)), ("repeat", repeat)]);
        let definition = plan.audio_definition(gap_selection("repeat")).unwrap();
        let intrinsic = definition
            .signal()
            .query_flattened(SignalSample(0)..SignalSample(4), Default::default())
            .unwrap();
        assert_eq!(
            intrinsic.spans[0].content,
            AudioSignalContent::Leaf(expected.clone())
        );
        for (start, end, start_hard) in [(0, 4, true), (1, 3, false)] {
            let domain = definition
                .in_root_clock(placement(-8, 2, start, end))
                .unwrap();
            let query = domain
                .audio(domain.root_samples(), Default::default())
                .unwrap();
            let span = &query.spans[0];
            assert_eq!(span.content, expected);
            assert_eq!(
                span.boundaries
                    .start
                    .iter()
                    .any(|edge| edge.policy == AudioEdgePolicy::Hard),
                start_hard
            );
            assert!(
                span.boundaries
                    .end
                    .iter()
                    .all(|edge| edge.policy == AudioEdgePolicy::Automatic)
            );
            for edge in span.boundaries.start.iter().chain(&span.boundaries.end) {
                assert!(edge.instance.repeats.is_empty());
                assert_eq!(edge.gap_after, None);
                assert!(
                    edge.placement_support
                        || matches!(
                            edge.kind,
                            AudioBoundaryKind::RepeatGapStart | AudioBoundaryKind::RepeatGapEnd
                        )
                );
            }
            let suppressed = if matches!(expected, AudioContent::Silence { .. }) {
                vec![domain.root_samples()]
            } else {
                vec![]
            };
            assert_eq!(
                domain
                    .policy(domain.root_samples(), Default::default())
                    .unwrap()
                    .suppressed,
                suppressed
            );
            let point = definition
                .in_point_clock(placement(-8, 2, start, end), ratio(1, 3))
                .unwrap();
            let signal = point.signal();
            let range = SignalSample(0)..signal.sample_count().unwrap();
            let suppressed = if matches!(expected, AudioContent::Silence { .. }) {
                vec![range.clone()]
            } else {
                vec![]
            };
            assert_eq!(
                signal.policy(range, Default::default()).unwrap().suppressed,
                suppressed
            );
        }
    }
}

#[test]
fn gap_positive_support_can_own_no_point_and_gain_points_under_explicit_placement() {
    let plan = RenderPlan::compile(&document(
        FrameRate::new(192_000, 1).unwrap(),
        &["repeat"],
        [
            ("child", source(8)),
            ("repeat", gap_repeat("child", 1, 2, HoldAudio::Silence)),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    let definition = plan.audio_definition(gap_selection("repeat")).unwrap();
    let empty = definition
        .in_point_clock(placement(1, 1, 0, 1), ExactRatio::ZERO)
        .unwrap();
    assert_eq!(
        empty.reference_samples(),
        ReferenceSample(1)..ReferenceSample(1)
    );
    assert_eq!(
        empty.signal().support(),
        ExactRatio::ONE..ExactRatio::integer(2)
    );
    assert_eq!(empty.signal().sample_count().unwrap(), SignalSample(0));
    assert!(
        empty
            .signal()
            .policy(SignalSample(0)..SignalSample(0), Default::default())
            .unwrap()
            .contents
            .is_empty()
    );
    let expanded = definition
        .in_point_clock(placement(8, 8, 0, 1), ExactRatio::ZERO)
        .unwrap();
    assert_eq!(
        expanded.reference_samples(),
        ReferenceSample(2)..ReferenceSample(4)
    );
    assert_eq!(
        expanded
            .signal()
            .policy(SignalSample(0)..SignalSample(2), Default::default())
            .unwrap()
            .suppressed,
        vec![SignalSample(0)..SignalSample(2)]
    );
}

#[test]
fn billion_play_gap_definition_has_constant_work_and_occurrences_keep_their_identity() {
    let plan = compile(
        &["repeat"],
        [
            ("child", source(2)),
            (
                "repeat",
                gap_repeat("child", 1_000_000_000, 3, HoldAudio::Silence),
            ),
        ],
    );
    let definition = plan.audio_definition(gap_selection("repeat")).unwrap();
    let limits = AudioQueryLimits {
        maximum_spans: 1,
        maximum_work: 1,
    };
    let query = definition
        .signal()
        .query(SignalSample(2)..SignalSample(3), limits)
        .unwrap();
    assert_eq!(query.work, 1);
    assert_eq!(query.lookup.visited_nodes, 1);
    assert_eq!(query.lookup.iteration_run_comparisons, 0);
    assert!(query.spans[0].instance.repeats.is_empty());
    assert_eq!(query.spans[0].gap_after, None);
    assert_eq!(plan.metadata().storage.iteration_run_entries, 1);
    let occurrence = plan
        .audio_domain_at(AudioSample(2), Default::default())
        .unwrap();
    assert_eq!(occurrence.definition(), None);
    assert_eq!(occurrence.gap_after(), Some(&play(0)));
    let span = occurrence
        .audio(AudioSample(2)..AudioSample(5), Default::default())
        .unwrap()
        .spans
        .remove(0);
    assert_eq!(span.gap_after, Some(play(0)));
    assert_eq!(
        span.content,
        AudioContent::Silence {
            reason: SilenceReason::SilentHold
        }
    );
    assert_eq!(span.transform.project_origin, ExactRatio::integer(2));
    assert_eq!(
        span.boundaries.start.last().unwrap().gap_after,
        Some(play(0))
    );
}
