use deadpan_core::*;
use deadpan_plan::{
    AudioBoundDomain, AudioContent, AudioDefinitionSelector, AudioQueryLimits, AudioRootPlacement,
    AudioSignalContent, ReferenceSample, RenderPlan, SignalSample, SilenceReason,
};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn recipe(duration: i64) -> HoldRecipe {
    HoldRecipe {
        duration: frames(duration),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
        picture_context: None,
    }
}
fn play(ordinal: u32) -> IterationId {
    IterationId {
        allocation: RevisionId::new("plays").unwrap(),
        ordinal,
    }
}
fn captured(plays: u32) -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("bound-gaps").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    for (name, node) in [
        (
            "root",
            BeatNode::sequence("Root", vec![id("prefix"), id("repeat")]),
        ),
        ("prefix", BeatNode::hold("Prefix", recipe(1))),
        ("child", BeatNode::hold("Child", recipe(1))),
        (
            "repeat",
            BeatNode {
                label: "Repeat".into(),
                framing: None,
                audio_treatments: Default::default(),
                audio_editorial_edges: Default::default(),
                audio_edges: AudioEdgePolicies {
                    node_start: AudioEdgePolicy::Hard,
                    node_end: AudioEdgePolicy::Hard,
                    repeat_gap_start: AudioEdgePolicy::Hard,
                    ..Default::default()
                },
                kind: NodeKind::Repeat {
                    child: id("child"),
                    iterations: IterationOrder::new(play(0).allocation, plays).unwrap(),
                    gap: Some(recipe(2)),
                    escalation: None,
                },
            },
        ),
    ] {
        wire["nodes"][name] = serde_json::to_value(node).unwrap();
    }
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    wire["audio_bindings"] = serde_json::to_value(
        capture_unbound_audio_bindings(
            &document,
            AudioTimingId {
                allocation: RevisionId::new("capture").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap(),
    )
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn edit(document: &ProjectDocument, revision: &str, command: Command) -> ProjectDocument {
    apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(revision).unwrap(),
            command,
        },
    )
    .unwrap()
    .forward
    .apply(document)
    .unwrap()
}

#[test]
fn moved_gap_is_bound_in_root_and_captured_domains_with_its_own_edges() {
    let moved = edit(
        &captured(3),
        "moved",
        Command::SetHoldDuration {
            node: id("prefix"),
            duration: frames(2),
        },
    );
    let plan = RenderPlan::compile(&moved).unwrap();
    // Original first gap [2,4), now [3,5), on the absolute NTSC sample grid.
    let samples = AudioSample(4805)..AudioSample(8008);
    let query = plan
        .audio_processing(samples.clone(), Default::default())
        .unwrap();
    assert_eq!(query.spans.len(), 1);
    let span = &query.spans[0];
    assert_eq!(span.instance.node, id("repeat"));
    assert!(span.instance.repeats.is_empty());
    assert_eq!(span.gap_after, Some(play(0)));
    let AudioSignalContent::Bound(bound) = &span.content else {
        panic!("gap binding dropped")
    };
    assert_eq!(
        bound.reference_at_offset(0).unwrap(),
        ExactRatio::integer(3203)
    );
    let AudioBoundDomain::Root(raw) = bound.raw_domain().unwrap() else {
        panic!("lost root clock")
    };
    assert_eq!(raw.root_samples(), AudioSample(3203)..AudioSample(6406));
    assert_eq!(raw.gap_after(), Some(&play(0)));
    let raw_query = raw
        .processing(raw.root_samples(), Default::default())
        .unwrap();
    assert!(matches!(
        raw_query.spans[0].content,
        AudioSignalContent::Leaf(AudioContent::Silence {
            reason: SilenceReason::SilentHold
        })
    ));
    let domain = plan
        .audio_domain_at(samples.start, Default::default())
        .unwrap();
    assert_eq!(domain.gap_after(), Some(&play(0)));
    let seeded = domain
        .processing(samples.clone(), Default::default())
        .unwrap();
    let AudioSignalContent::Bound(seeded) = &seeded.spans[0].content else {
        panic!("seed bypassed binding")
    };
    assert_eq!(
        seeded.reference_at_offset(0).unwrap(),
        bound.reference_at_offset(0).unwrap()
    );
    assert_eq!(
        plan.audio_policy(samples.clone(), Default::default())
            .unwrap()
            .suppressed,
        vec![samples.clone()]
    );
    let fade = plan.audio_fades(samples, Default::default()).unwrap();
    assert_eq!(fade.spans.len(), 1);
    assert_eq!(fade.spans[0].start[0].length, 3203);
    assert_eq!(fade.spans[0].start[0].distance_at_start, ExactRatio::ZERO);
    assert!(
        fade.spans[0]
            .start
            .iter()
            .flat_map(|edge| &edge.origins)
            .any(|edge| edge.policy == AudioEdgePolicy::Hard)
    );
    assert!(
        fade.spans[0]
            .end
            .iter()
            .flat_map(|edge| &edge.origins)
            .all(|edge| edge.policy == AudioEdgePolicy::Automatic)
    );
}

#[test]
fn one_play_gap_definition_uses_its_own_point_clock_and_current_duration() {
    let original = captured(1);
    let changed = edit(
        &original,
        "extend",
        Command::SetRepeat {
            node: id("repeat"),
            plays: 1,
            gap: Some(recipe(3)),
        },
    );
    let plan = RenderPlan::compile(&changed).unwrap();
    assert_eq!(plan.duration(), frames(2));
    let definition = plan
        .audio_definition(AudioDefinitionSelector::RepeatGap {
            repeat: id("repeat"),
        })
        .unwrap();
    let signal = definition.signal();
    assert_eq!(signal.sample_count().unwrap(), SignalSample(4805));
    let query = signal
        .query(SignalSample(0)..SignalSample(4805), Default::default())
        .unwrap();
    let AudioSignalContent::Bound(bound) = &query.spans[0].content else {
        panic!("direct gap binding dropped")
    };
    assert_eq!(query.spans[0].gap_after, None);
    assert!(query.spans[0].instance.repeats.is_empty());
    let AudioBoundDomain::Point(raw) = bound.raw_domain().unwrap() else {
        panic!("invented occurrence clock")
    };
    assert_eq!(raw.definition(), definition.selector());
    assert_eq!(
        raw.reference_samples(),
        ReferenceSample(0)..ReferenceSample(4805)
    );
    assert_eq!(
        raw.signal()
            .query(SignalSample(0)..SignalSample(1), Default::default())
            .unwrap()
            .spans[0]
            .gap_after,
        None
    );
    let whole = plan
        .audio_definition(AudioDefinitionSelector::Node { node: id("repeat") })
        .unwrap();
    assert_eq!(whole.duration(), frames(1));
    let whole_query = whole
        .signal()
        .query(SignalSample(0)..SignalSample(1), Default::default())
        .unwrap();
    assert_eq!(whole_query.spans[0].instance.node, id("child"));
    assert_eq!(whole_query.spans[0].gap_after, None);
    // Explicit placement may use signed labels without changing the retained gap clock.
    let placed = definition
        .in_root_clock(
            AudioRootPlacement::new(ratio(-1, 3), ratio(3, 2), ratio(1, 4)..ratio(7, 4)).unwrap(),
        )
        .unwrap();
    let placed_query = placed
        .processing(placed.root_samples(), Default::default())
        .unwrap();
    assert!(matches!(
        placed_query.spans[0].content,
        AudioSignalContent::Bound(_)
    ));
    assert_eq!(placed_query.spans[0].gap_after, None);
}

#[test]
fn billion_play_gap_binding_queries_stay_bounded_and_reject_insufficient_work() {
    let plan = RenderPlan::compile(&captured(1_000_000_000)).unwrap();
    let samples = AudioSample(3203)..AudioSample(3204);
    let limits = AudioQueryLimits {
        maximum_work: 256,
        maximum_spans: 1,
    };
    let query = plan.audio_processing(samples.clone(), limits).unwrap();
    assert_eq!(plan.metadata().storage.iteration_run_entries, 1);
    assert!(query.work < 256);
    assert_eq!(query.spans[0].gap_after, Some(play(0)));
    assert!(matches!(
        query.spans[0].content,
        AudioSignalContent::Bound(_)
    ));
    assert!(
        plan.audio_processing(
            samples,
            AudioQueryLimits {
                maximum_work: 2,
                maximum_spans: 1
            }
        )
        .is_err()
    );
}
