use std::collections::BTreeMap;

use deadpan_core::*;

use super::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn hold(value: i64) -> BeatNode {
    BeatNode::hold(
        "Voice",
        HoldRecipe {
            duration: frames(value),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        },
    )
}
fn sequence(children: &[&str]) -> BeatNode {
    BeatNode::sequence("Group", children.iter().map(|name| id(name)).collect())
}
fn retime(
    child: &str,
    duration: i64,
    range: Range<i64>,
    pitch: PitchPolicy,
    purpose: RetimePurpose,
) -> BeatNode {
    let mut node = hold(duration);
    node.kind = NodeKind::Retime {
        child: id(child),
        duration: frames(duration),
        mapping: FrameRange::new(ProjectFrame(range.start), ProjectFrame(range.end)).unwrap(),
        pitch,
        purpose,
    };
    node
}
fn repeat(child: &str, count: u32, gap: i64) -> BeatNode {
    let mut node = hold(1);
    node.kind = NodeKind::Repeat {
        child: id(child),
        iterations: IterationOrder::new(revision("plays"), count).unwrap(),
        gap: (gap > 0).then(|| HoldRecipe {
            duration: frames(gap),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        }),
    };
    node
}
fn document(rate: u32, children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    document_at(FrameRate::new(rate, 1).unwrap(), children, nodes)
}
fn document_at(
    rate: FrameRate,
    children: &[&str],
    nodes: Vec<(&str, BeatNode)>,
) -> ProjectDocument {
    let base = ProjectDocument::new(
        ProjectId::new("owner-clocks").unwrap(),
        revision("current"),
        PresentationBasis {
            width: 16,
            height: 16,
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
    nodes.insert(id("root"), sequence(children));
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn bind(
    current: &ProjectDocument,
    old: &ProjectDocument,
    owner: &str,
    clock: AudioClockRoot,
    phase: Option<ExactRatio>,
) -> ProjectDocument {
    let timing = AudioTimingId {
        allocation: revision("retained-clock"),
        ordinal: 0,
    };
    let state = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing.clone(),
            layout: FrozenAudioLayout::capture(old).unwrap(),
        }],
        BTreeMap::from([(
            id(owner),
            OwnedAudioBinding {
                reanchors: vec![],
                lattice: AudioPlacementTemplate {
                    reference_local_offset: deadpan_core::ExactRatio::ZERO,
                    gap_after: None,
                    reference: AudioReferenceClock {
                        recipe: AudioRecipeKind::Node,
                        timing,
                        root: clock,
                        physical: id(owner),
                    },
                    arguments: vec![],
                    births: vec![],
                },
                resume: phase.map(|constant| AudioResume {
                    local_boundary: ExactRatio::ZERO,
                    phase: AudioLocalPhase {
                        constant,
                        terms: vec![],
                    },
                }),
            },
        )]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(current).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn clocks(plan: &RenderPlan, sample: i64) -> Vec<(String, AudioOwnerKind, ExactRatio, ExactRatio)> {
    let query = plan
        .audio_owners(
            AudioSample(sample)..AudioSample(sample + 1),
            Default::default(),
        )
        .unwrap();
    query.spans()[0]
        .owners()
        .iter()
        .map(|owner| {
            assert!(owner.belongs_to(plan));
            (
                owner.instance().node.as_str().to_owned(),
                owner.kind(),
                owner.sampling().local_at(AudioSample(sample)).unwrap(),
                owner.sampling().local_frames_per_sample(),
            )
        })
        .collect()
}
fn values(plan: &RenderPlan, sample: i64) -> Vec<(String, ExactRatio)> {
    clocks(plan, sample)
        .into_iter()
        .map(|(node, _, at, _)| (node, at))
        .collect()
}

#[test]
fn sequence_repeat_and_preserve_owners_keep_independent_nominal_clocks() {
    let doc = document(
        48_000,
        &["prefix", "group"],
        vec![
            ("prefix", hold(2)),
            ("voice", hold(6)),
            (
                "speed",
                retime("voice", 4, 0..6, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
            ("repeat", repeat("speed", 2, 2)),
            ("group", sequence(&["repeat"])),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    assert_eq!(
        values(&plan, 3),
        vec![
            ("root".into(), ratio(3, 1)),
            ("group".into(), ratio(1, 1)),
            ("repeat".into(), ratio(1, 1)),
            ("speed".into(), ratio(1, 1)),
            ("voice".into(), ratio(3, 2))
        ]
    );
    assert_eq!(
        values(&plan, 9),
        vec![
            ("root".into(), ratio(9, 1)),
            ("group".into(), ratio(7, 1)),
            ("repeat".into(), ratio(7, 1)),
            ("speed".into(), ratio(1, 1)),
            ("voice".into(), ratio(3, 2))
        ]
    );
    let gap = plan
        .audio_owners(AudioSample(6)..AudioSample(8), Default::default())
        .unwrap();
    let owners = gap.spans()[0].owners();
    assert_eq!(owners.len(), 4);
    assert_eq!(owners[2].kind(), AudioOwnerKind::Node);
    assert_eq!(
        owners[2].sampling().local_at(AudioSample(6)).unwrap(),
        ratio(4, 1)
    );
    assert_eq!(owners[3].kind(), AudioOwnerKind::DefaultGap);
    assert_eq!(
        owners[3].sampling().local_at(AudioSample(6)).unwrap(),
        ExactRatio::ZERO
    );
    assert!(owners[3].gap_after().is_some());
    let whole = plan
        .audio_owners(AudioSample(0)..AudioSample(12), Default::default())
        .unwrap();
    assert_eq!(
        whole
            .spans()
            .iter()
            .map(|span| span.samples())
            .collect::<Vec<_>>(),
        vec![
            AudioSample(0)..AudioSample(2),
            AudioSample(2)..AudioSample(6),
            AudioSample(6)..AudioSample(8),
            AudioSample(8)..AudioSample(12)
        ]
    );
    assert_eq!(
        whole.spans()[1]
            .owners()
            .last()
            .unwrap()
            .instance()
            .repeats
            .len(),
        1
    );
    assert_ne!(
        whole.spans()[1].owners().last().unwrap().instance(),
        whole.spans()[3].owners().last().unwrap().instance()
    );
}

#[test]
fn partitions_and_mixed_retimes_keep_owner_origins_and_cropped_queries() {
    let doc = document(
        48_000,
        &["outer"],
        vec![
            ("voice", hold(12)),
            (
                "partition",
                retime(
                    "voice",
                    4,
                    4..8,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Partition,
                ),
            ),
            (
                "inner",
                retime(
                    "partition",
                    8,
                    0..4,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            (
                "outer",
                retime(
                    "inner",
                    4,
                    0..8,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Edit,
                ),
            ),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    assert_eq!(
        values(&plan, 2),
        vec![
            ("root".into(), ratio(2, 1)),
            ("outer".into(), ratio(2, 1)),
            ("inner".into(), ratio(4, 1)),
            ("partition".into(), ratio(2, 1)),
            ("voice".into(), ratio(6, 1))
        ]
    );
    let full = plan
        .audio_owners(AudioSample(0)..AudioSample(4), Default::default())
        .unwrap();
    let suffix = plan
        .audio_owners(AudioSample(2)..AudioSample(4), Default::default())
        .unwrap();
    for (before, after) in full.spans()[0]
        .owners()
        .iter()
        .zip(suffix.spans()[0].owners())
    {
        assert_eq!(before.instance(), after.instance());
        assert_eq!(
            before.sampling().local_at(AudioSample(3)).unwrap(),
            after.sampling().local_at(AudioSample(3)).unwrap()
        );
    }
}

#[test]
fn moved_odd_sample_binding_keeps_ancestor_time_separate_from_resumed_voice() {
    let old = document(32_000, &["voice"], vec![("voice", hold(3))]);
    let current = document(
        32_000,
        &["prefix", "voice"],
        vec![("prefix", hold(1)), ("voice", hold(3))],
    );
    let current = bind(
        &current,
        &old,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ratio(1, 3)),
    );
    let plan = RenderPlan::compile(&current).unwrap();
    let query = plan
        .audio_owners(AudioSample(2)..AudioSample(5), Default::default())
        .unwrap();
    let owners = query.spans()[0].owners();
    assert_eq!(owners[0].origin(), &AudioOwnerClockOrigin::Current);
    assert_eq!(
        owners[0].sampling().local_at(AudioSample(2)).unwrap(),
        ratio(4, 3)
    );
    assert!(
        matches!(owners[1].origin(), AudioOwnerClockOrigin::Retained { instance, .. } if instance.node == id("voice"))
    );
    assert_eq!(
        owners[1].sampling().local_at(AudioSample(2)).unwrap(),
        ratio(1, 3)
    );
    assert_eq!(
        owners[1].sampling().local_at(AudioSample(4)).unwrap(),
        ratio(5, 3)
    );
    assert_eq!(owners[1].sampling().local_frames_per_sample(), ratio(2, 3));
}

#[test]
fn resumed_preserve_walks_current_children_at_the_retained_nominal_time() {
    let nodes = || {
        vec![
            ("a", hold(2)),
            ("b", hold(2)),
            ("group", sequence(&["a", "b"])),
            (
                "stage",
                retime("group", 8, 0..4, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
        ]
    };
    let old = document(48_000, &["stage"], nodes());
    let mut moved = nodes();
    moved.push(("prefix", hold(1)));
    let current = document(48_000, &["prefix", "stage"], moved);
    let current = bind(
        &current,
        &old,
        "stage",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ratio(3, 1)),
    );
    let plan = RenderPlan::compile(&current).unwrap();
    assert_eq!(
        values(&plan, 1),
        vec![
            ("root".into(), ratio(1, 1)),
            ("stage".into(), ratio(3, 1)),
            ("group".into(), ratio(3, 2)),
            ("a".into(), ratio(3, 2))
        ]
    );
    assert_eq!(
        values(&plan, 2),
        vec![
            ("root".into(), ratio(2, 1)),
            ("stage".into(), ratio(4, 1)),
            ("group".into(), ratio(2, 1)),
            ("b".into(), ratio(0, 1))
        ]
    );
}

#[test]
fn descendant_binding_uses_preserve_input_point_ceil_not_the_output_grid() {
    let nodes = |prefix, input, output| {
        vec![
            ("prefix", hold(prefix)),
            ("voice", hold(2)),
            ("group", sequence(&["prefix", "voice"])),
            (
                "stage",
                retime(
                    "group",
                    output,
                    0..input,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
        ]
    };
    let old = document(32_000, &["stage"], nodes(1, 3, 6));
    let current = document(32_000, &["stage"], nodes(2, 4, 8));
    let current = bind(
        &current,
        &old,
        "voice",
        AudioClockRoot::PreserveInputPointCeil { stage: id("stage") },
        None,
    );
    let plan = RenderPlan::compile(&current).unwrap();
    assert_eq!(
        values(&plan, 6),
        vec![
            ("root".into(), ratio(4, 1)),
            ("stage".into(), ratio(4, 1)),
            ("group".into(), ratio(2, 1)),
            ("voice".into(), ratio(1, 3))
        ]
    );
    assert_eq!(clocks(&plan, 6).last().unwrap().3, ratio(1, 3));
}

#[test]
fn definition_scope_and_default_gap_are_branded_without_an_invented_play() {
    let doc = document(
        48_000,
        &["repeat"],
        vec![("voice", hold(4)), ("repeat", repeat("voice", 2, 2))],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    for selector in [
        AudioDefinitionSelector::RepeatDefault {
            repeat: id("repeat"),
        },
        AudioDefinitionSelector::RepeatGap {
            repeat: id("repeat"),
        },
    ] {
        let definition = plan.audio_definition(selector.clone()).unwrap();
        let query = definition
            .owners(SignalSample(0)..SignalSample(2), Default::default())
            .unwrap();
        let owner = &query.spans()[0].owners()[0];
        assert_eq!(owner.definition(), Some(&selector));
        assert!(owner.instance().repeats.is_empty());
        assert!(owner.gap_after().is_none());
        assert_eq!(
            owner.sampling().local_at(SignalSample(1)).unwrap(),
            ExactRatio::ONE
        );
        assert_eq!(
            owner.kind(),
            if matches!(selector, AudioDefinitionSelector::RepeatGap { .. }) {
                AudioOwnerKind::DefaultGap
            } else {
                AudioOwnerKind::Node
            }
        );
    }
}

#[test]
fn owner_query_stays_bounded_for_compact_repeats_and_rejects_partial_results() {
    let doc = document(
        48_000,
        &["repeat"],
        vec![
            ("voice", hold(1)),
            ("repeat", repeat("voice", 1_000_000_000, 0)),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let query = plan
        .audio_owners(
            AudioSample(999_999_999)..AudioSample(1_000_000_000),
            Default::default(),
        )
        .unwrap();
    assert!(query.work() < 100);
    assert_eq!(
        query.spans()[0]
            .owners()
            .last()
            .unwrap()
            .sampling()
            .local_at(AudioSample(999_999_999))
            .unwrap(),
        ExactRatio::ZERO
    );
    assert!(
        plan.audio_owners(
            AudioSample(0)..AudioSample(2),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 100
            }
        )
        .is_err()
    );
    assert!(
        plan.audio_owners(
            AudioSample(0)..AudioSample(1),
            AudioQueryLimits {
                maximum_spans: 10,
                maximum_work: 1
            }
        )
        .is_err()
    );
    assert!(
        plan.audio_owners(
            AudioSample(0)..AudioSample(1),
            AudioQueryLimits {
                maximum_spans: 0,
                maximum_work: 100
            }
        )
        .is_err()
    );
    assert!(
        plan.audio_owners(AudioSample(2)..AudioSample(1), Default::default())
            .is_err()
    );
    assert!(
        plan.audio_owners(AudioSample(-1)..AudioSample(1), Default::default())
            .is_err()
    );
    assert!(
        plan.audio_owners(AudioSample(0)..AudioSample(0), Default::default())
            .unwrap()
            .spans()
            .is_empty()
    );
}

#[test]
fn repeat_play_and_gap_overrides_keep_live_owners_and_stable_scope() {
    let base = document(
        48_000,
        &["repeat"],
        vec![("voice", hold(4)), ("repeat", repeat("voice", 3, 2))],
    );
    let play = |ordinal| IterationId {
        allocation: revision("plays"),
        ordinal,
    };
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"]["alternate"] = serde_json::to_value(hold(3)).unwrap();
    wire["nodes"]["gap-group"] = serde_json::to_value(sequence(&["gap-voice"])).unwrap();
    wire["nodes"]["gap-voice"] = serde_json::to_value(hold(1)).unwrap();
    wire["overrides"] = serde_json::to_value(BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play(1),
            root: id("alternate"),
        }])
        .unwrap(),
    )]))
    .unwrap();
    wire["gap_overrides"] = serde_json::to_value(BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play(0),
            root: id("gap-group"),
        }])
        .unwrap(),
    )]))
    .unwrap();
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    assert_eq!(
        values(&plan, 4),
        vec![
            ("root".into(), ratio(4, 1)),
            ("repeat".into(), ratio(4, 1)),
            ("gap-group".into(), ratio(0, 1)),
            ("gap-voice".into(), ratio(0, 1))
        ]
    );
    assert_eq!(
        values(&plan, 5),
        vec![
            ("root".into(), ratio(5, 1)),
            ("repeat".into(), ratio(5, 1)),
            ("alternate".into(), ratio(0, 1))
        ]
    );
    let gap = plan
        .audio_owners(AudioSample(4)..AudioSample(5), Default::default())
        .unwrap();
    let gap_owner = gap.spans()[0].owners().last().unwrap();
    assert_eq!(gap_owner.kind(), AudioOwnerKind::Node);
    assert_eq!(gap_owner.instance().repeats[0].iteration, play(0));
    let later = plan
        .audio_owners(AudioSample(8)..AudioSample(10), Default::default())
        .unwrap();
    let owner = later.spans()[0].owners().last().unwrap();
    assert_eq!(owner.kind(), AudioOwnerKind::DefaultGap);
    assert_eq!(owner.gap_after(), Some(&play(1)));
    let definition = plan
        .audio_definition(AudioDefinitionSelector::RepeatDefault {
            repeat: id("repeat"),
        })
        .unwrap();
    let default = definition
        .owners(SignalSample(0)..SignalSample(1), Default::default())
        .unwrap();
    assert_eq!(default.spans()[0].owners()[0].instance().node, id("voice"));
}

#[test]
fn ntsc_play_boundaries_do_not_replace_exact_coordinates_with_rounded_phase() {
    let doc = document_at(
        FrameRate::new(30_000, 1001).unwrap(),
        &["prefix", "repeat"],
        vec![
            ("prefix", hold(1)),
            ("voice", hold(2)),
            ("repeat", repeat("voice", 3, 0)),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    for (sample, phase) in [
        (1602, ratio(1, 4004)),
        (4805, ratio(1, 8008)),
        (8008, ExactRatio::ZERO),
    ] {
        let query = plan
            .audio_owners(
                AudioSample(sample)..AudioSample(sample + 1),
                Default::default(),
            )
            .unwrap();
        let voice = query.spans()[0].owners().last().unwrap();
        assert_eq!(
            voice.sampling().local_at(AudioSample(sample)).unwrap(),
            phase
        );
        assert_eq!(voice.sampling().local_frames_per_sample(), ratio(5, 8008));
    }
}

#[test]
fn checked_physical_domains_and_frozen_plans_keep_their_namespace() {
    let doc = document(48_000, &["voice"], vec![("voice", hold(8))]);
    let plan = RenderPlan::compile(&doc).unwrap();
    let definition = plan
        .audio_definition(AudioDefinitionSelector::Node { node: id("voice") })
        .unwrap();
    let domain = definition
        .in_root_clock(
            crate::AudioRootPlacement::new(ratio(-8, 1), ratio(2, 1), ratio(2, 1)..ratio(6, 1))
                .unwrap(),
        )
        .unwrap();
    let query = domain
        .owners(AudioSample(-3)..AudioSample(-1), Default::default())
        .unwrap();
    assert_eq!(query.spans()[0].owners().len(), 1);
    let owner = &query.spans()[0].owners()[0];
    assert_eq!(owner.definition(), Some(definition.selector()));
    assert_eq!(
        owner.sampling().local_at(AudioSample(-3)).unwrap(),
        ratio(5, 2)
    );
    assert_eq!(owner.sampling().local_frames_per_sample(), ratio(1, 2));
    let frozen =
        RenderPlan::compile_audio_context(&FrozenAudioContext::capture(&doc).unwrap()).unwrap();
    assert_eq!(values(&frozen, 3), values(&plan, 3));
    let frozen_query = frozen
        .audio_owners(AudioSample(3)..AudioSample(4), Default::default())
        .unwrap();
    assert!(!frozen_query.belongs_to(&plan));
    assert!(!frozen_query.spans()[0].owners()[0].belongs_to(&plan));
}

#[test]
fn exhausted_bound_descendants_are_rejected_instead_of_inventing_a_clock() {
    let old = document(48_000, &["voice"], vec![("voice", hold(3))]);
    let current = bind(
        &old,
        &old,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ratio(3, 1)),
    );
    let plan = RenderPlan::compile(&current).unwrap();
    assert!(matches!(
        plan.audio_owners(AudioSample(0)..AudioSample(1), Default::default()),
        Err(PlanError::InvalidPlan(
            "owner coordinate is outside its allocated interval"
        ))
    ));
    let gain = plan
        .audio_gain_owners(AudioSample(0)..AudioSample(3), Default::default())
        .unwrap();
    assert_eq!(gain.spans().len(), 1);
    assert_eq!(gain.spans()[0].support(), AudioOwnerSupport::Inactive);
    assert_eq!(gain.spans()[0].owners().len(), 1);
    assert_eq!(gain.spans()[0].owners()[0].instance().node, id("root"));
}

#[test]
fn gain_query_does_not_treat_invalid_current_coordinates_as_inactive_support() {
    let doc = document(48_000, &["voice"], vec![("voice", hold(3))]);
    let plan = RenderPlan::compile(&doc).unwrap();
    let mut seed = Walk::root(&plan).unwrap();
    seed.allow_inactive = true;
    // Only retained bound support may be inactive. An inconsistent current
    // walk remains an error even when the caller requests gain-support spans.
    seed.extent = ratio(1, 1)..ratio(3, 1);
    assert!(matches!(
        query(&plan, 0..1, Default::default(), seed, AudioSample),
        Err(PlanError::InvalidPlan(
            "owner coordinate is outside its allocated interval"
        ))
    ));
}

#[test]
fn retained_default_gap_phase_does_not_replace_its_current_repeat_clock() {
    let old = document_at(
        FrameRate::new(30_000, 1001).unwrap(),
        &["prefix", "repeat"],
        vec![
            ("prefix", hold(1)),
            ("voice", hold(1)),
            ("repeat", repeat("voice", 3, 2)),
        ],
    );
    let bindings = capture_unbound_audio_bindings(
        &old,
        AudioTimingId {
            allocation: revision("capture"),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let bound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let changed = apply(
        &bound,
        &CommandRequest {
            project_id: bound.project_id().clone(),
            expected_revision: bound.revision_id().clone(),
            new_revision: revision("move-prefix"),
            command: Command::SetHoldDuration {
                node: id("prefix"),
                duration: frames(2),
            },
        },
    )
    .unwrap()
    .forward
    .apply(&bound)
    .unwrap();
    let plan = RenderPlan::compile(&changed).unwrap();
    let query = plan
        .audio_owners(AudioSample(4805)..AudioSample(8008), Default::default())
        .unwrap();
    assert_eq!(query.spans().len(), 1);
    let owners = query.spans()[0].owners();
    assert_eq!(owners.len(), 3);
    assert_eq!(owners[1].instance().node, id("repeat"));
    assert_eq!(owners[1].kind(), AudioOwnerKind::Node);
    assert_eq!(owners[1].origin(), &AudioOwnerClockOrigin::Current);
    assert_eq!(
        owners[1].sampling().local_at(AudioSample(4805)).unwrap(),
        ratio(8009, 8008)
    );
    assert_eq!(owners[2].kind(), AudioOwnerKind::DefaultGap);
    assert_eq!(
        owners[2].sampling().local_at(AudioSample(4805)).unwrap(),
        ratio(-1, 8008)
    );
    assert_eq!(
        owners[2].sampling().local_frames_per_sample(),
        ratio(5, 8008)
    );
    assert!(matches!(
        owners[2].origin(),
        AudioOwnerClockOrigin::Retained {
            kind: AudioOwnerKind::DefaultGap,
            gap_after: Some(_),
            ..
        }
    ));
}

#[test]
fn outer_crop_does_not_remove_a_preserve_inputs_resumed_binding_support() {
    let nodes = || {
        vec![
            ("voice", hold(4)),
            (
                "stage",
                retime("voice", 8, 0..4, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
        ]
    };
    let old = document(48_000, &["stage"], nodes());
    let mut cropped = nodes();
    cropped.push((
        "outer",
        retime(
            "stage",
            2,
            4..6,
            PitchPolicy::FollowSpeed,
            RetimePurpose::Edit,
        ),
    ));
    let current = document(48_000, &["outer"], cropped);
    let current = bind(
        &current,
        &old,
        "voice",
        AudioClockRoot::PreserveInputPointCeil { stage: id("stage") },
        Some(ExactRatio::ONE),
    );
    let plan = RenderPlan::compile(&current).unwrap();
    assert_eq!(
        values(&plan, 0),
        vec![
            ("root".into(), ratio(0, 1)),
            ("outer".into(), ratio(0, 1)),
            ("stage".into(), ratio(4, 1)),
            ("voice".into(), ratio(3, 1))
        ]
    );
    assert_eq!(
        values(&plan, 1),
        vec![
            ("root".into(), ratio(1, 1)),
            ("outer".into(), ratio(1, 1)),
            ("stage".into(), ratio(5, 1)),
            ("voice".into(), ratio(7, 2))
        ]
    );

    // Another enclosing Preserve still prepares the cropped inner stage's full
    // selected input. The original outer crop remains the delivery boundary.
    let mut nested = serde_json::to_value(&current).unwrap();
    nested["nodes"]["root"] = serde_json::to_value(sequence(&["slow"])).unwrap();
    nested["nodes"]["slow"] = serde_json::to_value(retime(
        "outer",
        4,
        0..2,
        PitchPolicy::Preserve,
        RetimePurpose::Edit,
    ))
    .unwrap();
    let nested = ProjectDocument::from_json(&nested.to_string()).unwrap();
    let nested = RenderPlan::compile(&nested).unwrap();
    assert_eq!(
        values(&nested, 1),
        vec![
            ("root".into(), ratio(1, 1)),
            ("slow".into(), ratio(1, 1)),
            ("outer".into(), ratio(1, 2)),
            ("stage".into(), ratio(9, 2)),
            ("voice".into(), ratio(13, 4))
        ]
    );
    assert_eq!(nested.audio_duration().unwrap(), AudioSample(4));
}

#[derive(Debug, PartialEq, Eq)]
struct OwnerObservation {
    kind: AudioOwnerKind,
    instance: InstancePath,
    definition: Option<AudioDefinitionSelector>,
    gap_after: Option<IterationId>,
    origin: AudioOwnerClockOrigin,
    local: ExactRatio,
    step: ExactRatio,
}

fn observations(query: &AudioOwnerQuery<'_>) -> BTreeMap<i64, Vec<OwnerObservation>> {
    query
        .spans()
        .iter()
        .flat_map(|span| {
            (span.samples().start.0..span.samples().end.0).map(|sample| {
                (
                    sample,
                    span.owners()
                        .iter()
                        .map(|owner| OwnerObservation {
                            kind: owner.kind(),
                            instance: owner.instance().clone(),
                            definition: owner.definition().cloned(),
                            gap_after: owner.gap_after().cloned(),
                            origin: owner.origin().clone(),
                            local: owner.sampling().local_at(AudioSample(sample)).unwrap(),
                            step: owner.sampling().local_frames_per_sample(),
                        })
                        .collect(),
                )
            })
        })
        .collect()
}

#[test]
fn nested_repeat_retime_and_resumed_odd_grid_owners_are_query_partition_invariant() {
    let old = document(
        32_000,
        &["prefix", "outer-repeat"],
        vec![
            ("prefix", hold(1)),
            ("voice", hold(4)),
            ("inner-repeat", repeat("voice", 2, 2)),
            (
                "stage",
                retime(
                    "inner-repeat",
                    8,
                    0..10,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            (
                "fast",
                retime(
                    "stage",
                    4,
                    0..8,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Edit,
                ),
            ),
            ("outer-repeat", repeat("fast", 2, 2)),
        ],
    );
    let timing = AudioTimingId {
        allocation: revision("coherence-clock"),
        ordinal: 0,
    };
    let captured = capture_unbound_audio_bindings(&old, timing.clone()).unwrap();
    let mut stage = captured.bindings()[&id("stage")].clone();
    stage.resume = Some(AudioResume {
        local_boundary: ExactRatio::ZERO,
        phase: AudioLocalPhase {
            constant: ratio(1, 3),
            terms: vec![],
        },
    });
    let bindings = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing,
            layout: FrozenAudioLayout::capture(&old).unwrap(),
        }],
        BTreeMap::from([(id("stage"), stage)]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&current).unwrap();
    assert_eq!(plan.audio_duration().unwrap(), AudioSample(16));
    let full = observations(
        &plan
            .audio_owners(AudioSample(0)..AudioSample(16), Default::default())
            .unwrap(),
    );
    assert_eq!(full.len(), 16);
    assert!(
        full.values()
            .flatten()
            .any(|owner| matches!(owner.origin, AudioOwnerClockOrigin::Retained { .. }))
    );
    assert!(
        full.values()
            .flatten()
            .any(|owner| owner.instance.repeats.len() == 2)
    );
    let mut blocks = BTreeMap::new();
    for (start, end) in [(9, 12), (0, 3), (15, 16), (3, 7), (12, 15), (7, 9)] {
        for (sample, owners) in observations(
            &plan
                .audio_owners(AudioSample(start)..AudioSample(end), Default::default())
                .unwrap(),
        ) {
            assert!(blocks.insert(sample, owners).is_none());
        }
    }
    assert_eq!(blocks, full);
    for sample in 0..16 {
        let single = observations(
            &plan
                .audio_owners(
                    AudioSample(sample)..AudioSample(sample + 1),
                    Default::default(),
                )
                .unwrap(),
        );
        assert_eq!(single.get(&sample), full.get(&sample), "sample {sample}");
    }
}

#[test]
fn selected_source_resume_retains_owner_clock_and_rejects_exhausted_selection() {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 4,
            time_base,
        },
    )
    .unwrap();
    let asset = AssetId::new("original").unwrap();
    let source = BeatNode {
        label: "Selected Original audio".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: frames(4),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(SourceAudio {
                    asset: asset.clone(),
                    span,
                }),
                audio_mapping: SourceAudioMapping::SelectedPlacement {
                    start: ExactRatio::ZERO,
                    frames: ratio(4, 1),
                    selection: ExactFrameRange {
                        start: ratio(2, 1),
                        end: ratio(4, 1),
                    },
                },
                audio_offset: AudioSample(-1),
                link: LinkRelation::Independent,
            },
        },
    };
    let make = |moved: bool| {
        let base = if moved {
            document(
                48_000,
                &["prefix", "voice"],
                vec![("prefix", hold(1)), ("voice", hold(4))],
            )
        } else {
            document(48_000, &["voice"], vec![("voice", hold(4))])
        };
        let mut wire = serde_json::to_value(base).unwrap();
        wire["nodes"]["voice"] = serde_json::to_value(&source).unwrap();
        wire["assets"] = serde_json::to_value(BTreeMap::from([(
            asset.clone(),
            AssetRecord {
                label: "Original".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(span),
                still_image: false,
                frame_count: None,
                source_qualification: None,
            },
        )]))
        .unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    };
    let current = bind(
        &make(true),
        &make(false),
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ratio(3, 2)),
    );
    let plan = RenderPlan::compile(&current).unwrap();
    let query = plan
        .audio_owners(AudioSample(1)..AudioSample(3), Default::default())
        .unwrap();
    assert_eq!(query.spans().len(), 1);
    let owners = query.spans()[0].owners();
    assert_eq!(owners.len(), 2);
    assert_eq!(owners[0].origin(), &AudioOwnerClockOrigin::Current);
    assert_eq!(
        owners[0].sampling().local_at(AudioSample(1)).unwrap(),
        ExactRatio::ONE
    );
    // The selected source is audible on owner [1, 3), after its signed offset.
    // Its retained clock starts at 1.5; neither selection nor resume resets it.
    assert_eq!(owners[1].instance().node, id("voice"));
    assert_eq!(
        owners[1].sampling().local_at(AudioSample(1)).unwrap(),
        ratio(3, 2)
    );
    assert_eq!(
        owners[1].sampling().local_at(AudioSample(2)).unwrap(),
        ratio(5, 2)
    );
    assert!(matches!(
        owners[1].origin(),
        AudioOwnerClockOrigin::Retained { .. }
    ));
    assert_eq!(
        observations(&query).get(&2),
        observations(
            &plan
                .audio_owners(AudioSample(2)..AudioSample(3), Default::default())
                .unwrap()
        )
        .get(&2)
    );
    assert!(matches!(
        plan.audio_owners(AudioSample(3)..AudioSample(4), Default::default()),
        Err(PlanError::InvalidPlan(
            "owner coordinate is outside its allocated interval"
        ))
    ));
    let gain = plan
        .audio_gain_owners(AudioSample(1)..AudioSample(5), Default::default())
        .unwrap();
    assert_eq!(gain.spans().len(), 2);
    assert_eq!(gain.spans()[0].samples(), AudioSample(1)..AudioSample(3));
    assert_eq!(gain.spans()[0].support(), AudioOwnerSupport::Active);
    assert_eq!(gain.spans()[1].samples(), AudioSample(3)..AudioSample(5));
    assert_eq!(gain.spans()[1].support(), AudioOwnerSupport::Inactive);
    assert_eq!(gain.spans()[1].owners().len(), 1);
    assert_eq!(gain.spans()[1].owners()[0].instance().node, id("root"));
}

#[test]
fn treatments_are_checked_plan_owners_and_default_gap_never_duplicates_repeat_gain() {
    use deadpan_core::{AudioTreatments, ClipGain, GainDb};
    let treatment = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(3000).unwrap(), false, vec![], vec![]).unwrap(),
    );
    let mut repeated = repeat("voice", 2, 2);
    repeated.audio_treatments = treatment.clone();
    let doc = document(
        48_000,
        &["repeat"],
        vec![("voice", hold(2)), ("repeat", repeated)],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    assert!(plan.has_audio_treatments());
    let query = plan
        .audio_gain_owners(AudioSample(2)..AudioSample(4), Default::default())
        .unwrap();
    let owners = query.spans()[0].owners();
    assert_eq!(owners.len(), 3);
    assert_eq!(owners[1].treatments(), Some(&treatment));
    assert_eq!(owners[2].kind(), AudioOwnerKind::DefaultGap);
    assert_eq!(owners[2].treatments(), None);
    let frozen =
        RenderPlan::compile_audio_context(&FrozenAudioContext::capture(&doc).unwrap()).unwrap();
    assert!(frozen.has_audio_treatments());
    let retained = frozen
        .audio_gain_owners(AudioSample(2)..AudioSample(4), Default::default())
        .unwrap();
    let owner = &retained.spans()[0].owners()[1];
    assert!(owner.belongs_to(&frozen));
    assert!(!owner.belongs_to(&plan));
    assert_eq!(owner.treatments(), Some(&treatment));
}
