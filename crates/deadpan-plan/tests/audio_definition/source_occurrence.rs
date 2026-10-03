use super::*;
use deadpan_plan::{AudioSourceOccurrence, AudioSourceVoiceRecipe};
use std::sync::Arc;

fn q(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn catalog(doc: ProjectDocument) -> ProjectDocument {
    let base = SourceTimeBase::new(1, 48_000).unwrap();
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["assets"]["sound"] = serde_json::to_value(AssetRecord {
        label: "Catalog sound".into(),
        content_hash: "c".repeat(64),
        video: None,
        audio: Some(
            SourceSpan::new(
                SourceTimestamp {
                    ticks: 10,
                    time_base: base,
                },
                SourceTimestamp {
                    ticks: 48_010,
                    time_base: base,
                },
            )
            .unwrap(),
        ),
        still_image: false,
        frame_count: None,
        source_qualification: Some(SourceQualificationId::new("d".repeat(64)).unwrap()),
    })
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn recipe(plan: &RenderPlan, start: ExactRatio, count: i64) -> AudioSourceVoiceRecipe {
    let base = SourceTimeBase::new(1, 48_000).unwrap();
    let source = SourceAudio {
        asset: AssetId::new("sound").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 10,
                time_base: base,
            },
            SourceTimestamp {
                ticks: 10 + count,
                time_base: base,
            },
        )
        .unwrap(),
    };
    let duration = SourceAudioMapping::natural_rate(
        source.span,
        plan.metadata().presentation_basis.frame_rate,
    )
    .unwrap()
    .duration_frames(FrameDuration::ZERO)
    .unwrap();
    AudioSourceVoiceRecipe {
        source,
        mapping: SourceAudioMapping::Placement {
            start,
            frames: duration,
        },
        offset: AudioSample(0),
    }
}

fn occurrence(node: &str, plays: &[(&str, u32)]) -> InstancePath {
    InstancePath {
        node: id(node),
        repeats: plays
            .iter()
            .map(|(node, ordinal)| RepeatInstance {
                node: id(node),
                iteration: play(*ordinal),
            })
            .collect(),
    }
}

fn voice<'p>(plan: &'p RenderPlan, owner: &str) -> AudioSourceOccurrence<'p> {
    plan.source_voice_occurrence(
        occurrence(owner, &[]),
        recipe(plan, q(1, 1), 4),
        Default::default(),
    )
    .unwrap()
}

fn projected<'p>(voice: &AudioSourceOccurrence<'p>) -> Arc<deadpan_plan::AudioStageProjection<'p>> {
    let query = voice
        .processing(voice.samples(), Default::default())
        .unwrap();
    let AudioSignalContent::ProjectedStage(stage) = &query.spans[0].content else {
        panic!("expected independent Preserve");
    };
    Arc::clone(stage)
}

#[test]
fn source_voice_occurrences_validate_empty_windows_and_aggregate_construction_limits() {
    let doc = catalog(document(
        FrameRate::new(48_000, 1).unwrap(),
        &["prefix", "owner"],
        [("prefix", source(4)), ("owner", source(12))],
        BTreeMap::new(),
    ));
    let plan = RenderPlan::compile(&doc).unwrap();
    let empty = plan
        .source_voice_occurrences(
            &id("owner"),
            recipe(&plan, q(0, 1), 4),
            AudioSample(0)..AudioSample(4),
            Default::default(),
        )
        .unwrap();
    assert!(empty.voices().is_empty());
    assert!(
        plan.source_voice_occurrences(
            &id("owner"),
            recipe(&plan, q(11, 1), 4),
            AudioSample(0)..AudioSample(4),
            Default::default()
        )
        .is_err()
    );
    let mut missing = recipe(&plan, q(0, 1), 4);
    missing.source.asset = AssetId::new("missing").unwrap();
    assert!(
        plan.source_voice_occurrences(
            &id("owner"),
            missing,
            AudioSample(0)..AudioSample(4),
            Default::default()
        )
        .is_err()
    );
    for limits in [
        AudioQueryLimits {
            maximum_spans: 1,
            ..Default::default()
        },
        AudioQueryLimits {
            maximum_work: 1,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            plan.source_voice_occurrences(
                &id("owner"),
                recipe(&plan, q(0, 1), 4),
                AudioSample(4)..AudioSample(16),
                limits
            ),
            Err(PlanError::AudioQueryLimit(_))
        ));
    }
}

#[test]
fn stage_owned_voice_enters_after_processing_while_child_has_independent_input() {
    let doc = catalog(document(
        FrameRate::new(48_000, 1).unwrap(),
        &["prefix", "stage"],
        [
            ("prefix", source(1)),
            ("child", source(12)),
            (
                "stage",
                retime(
                    "child",
                    6,
                    0,
                    12,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
        ],
        BTreeMap::new(),
    ));
    let plan = RenderPlan::compile(&doc).unwrap();
    let child = voice(&plan, "child");
    let on_stage = voice(&plan, "stage");
    assert_eq!(child.samples(), AudioSample(1)..AudioSample(7));
    assert_eq!(on_stage.samples(), child.samples());
    let projection = projected(&child);
    assert_eq!(
        projection.stage().descriptor().instance,
        occurrence("stage", &[])
    );
    assert_eq!(projection.input_support(), q(0, 1)..q(12, 1));
    let input = projection
        .input_tape()
        .unwrap()
        .query(SignalSample(1)..SignalSample(2), Default::default())
        .unwrap();
    assert_eq!(
        input.spans[0].source_point(SignalSample(1)).unwrap().ticks,
        q(10, 1)
    );
    assert!(
        matches!(&input.spans[0].content, AudioSignalContent::Leaf(AudioContent::Source { source, .. }) if source.asset == AssetId::new("sound").unwrap())
    );
    assert!(projection.input_mix().is_none());
    let direct = on_stage
        .processing(AudioSample(2)..AudioSample(3), Default::default())
        .unwrap();
    assert_eq!(
        direct.spans[0].source_point(AudioSample(2)).unwrap().ticks,
        q(10, 1)
    );
    assert!(matches!(
        direct.spans[0].content,
        AudioSignalContent::Leaf(_)
    ));
}

#[test]
fn nested_preserve_retains_full_history_identity_and_exact_independent_source() {
    let plan = RenderPlan::compile(&catalog(document(
        FrameRate::new(48_000, 1).unwrap(),
        &["outer"],
        [
            ("child", source(16)),
            (
                "inner",
                retime(
                    "child",
                    8,
                    0,
                    16,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            (
                "outer",
                retime("inner", 4, 0, 8, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
        ],
        BTreeMap::new(),
    )))
    .unwrap();
    let child = voice(&plan, "child");
    let outer = projected(&child);
    let query = outer
        .input_tape()
        .unwrap()
        .query(SignalSample(3)..SignalSample(4), Default::default())
        .unwrap();
    let AudioSignalContent::ProjectedStage(inner) = &query.spans[0].content else {
        panic!("nested stage absent");
    };
    assert_eq!(
        outer.stage().descriptor().instance,
        occurrence("outer", &[])
    );
    assert_eq!(
        inner.stage().descriptor().instance,
        occurrence("inner", &[])
    );
    assert_eq!(inner.input_sample_count().unwrap(), SignalSample(16));
    assert_eq!(inner.output_frames(), 8);
    let raw = inner
        .input_tape()
        .unwrap()
        .query(SignalSample(2)..SignalSample(3), Default::default())
        .unwrap();
    assert_eq!(
        raw.spans[0].source_point(SignalSample(2)).unwrap().ticks,
        q(11, 1)
    );
    let narrow = child
        .processing(AudioSample(2)..AudioSample(3), Default::default())
        .unwrap();
    let AudioSignalContent::ProjectedStage(same) = &narrow.spans[0].content else {
        panic!("stage lost");
    };
    assert_eq!(same.identity(), outer.identity());
    assert_eq!(child.source_identity(), child.clone().source_identity());
    assert_ne!(
        child.source_identity(),
        voice(&plan, "child").source_identity()
    );
    assert_eq!(child.source(), &recipe(&plan, q(1, 1), 4).source);
    assert_eq!(child.extent(), q(0, 1)..q(4, 1));
}

#[test]
fn sequence_padding_and_nonzero_preserve_selection_keep_exact_source_phase() {
    let plan = RenderPlan::compile(&catalog(document(
        FrameRate::new(48_000, 1).unwrap(),
        &["stage"],
        [
            ("left", source(4)),
            ("target", hold(8)),
            ("right", source(4)),
            (
                "input",
                BeatNode::sequence("Input", vec![id("left"), id("target"), id("right")]),
            ),
            (
                "stage",
                retime(
                    "input",
                    6,
                    2,
                    14,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
        ],
        BTreeMap::new(),
    )))
    .unwrap();
    let voice = voice(&plan, "target");
    let projection = projected(&voice);
    assert_eq!(projection.input_support(), q(0, 1)..q(12, 1));
    let raw = projection
        .input_tape()
        .unwrap()
        .query(SignalSample(0)..SignalSample(12), Default::default())
        .unwrap();
    let source = raw
        .spans
        .iter()
        .find(|span| {
            matches!(
                span.content,
                AudioSignalContent::Leaf(AudioContent::Source { .. })
            )
        })
        .unwrap();
    assert_eq!(source.samples, SignalSample(3)..SignalSample(7));
    assert_eq!(
        source.source_point(SignalSample(3)).unwrap().ticks,
        q(10, 1)
    );
    assert!(raw.spans.iter().all(|span| !matches!(
        span.content,
        AudioSignalContent::Stage(_) | AudioSignalContent::Bound(_)
    )));
    assert!(
        projection
            .output_policy()
            .policy_after_preserve(SignalSample(0)..SignalSample(6), Default::default())
            .unwrap()
            .suppressed
            .is_empty()
    );
    assert_eq!(
        voice
            .policy(voice.samples(), Default::default())
            .unwrap()
            .suppressed,
        vec![AudioSample(1)..AudioSample(5)]
    );
    let policy = voice
        .hold_policy(voice.samples(), Default::default())
        .unwrap();
    assert!(
        matches!(&policy.rules[0].issuer, deadpan_plan::AudioHoldIssuer::Node { definition: None, instance } if instance == &occurrence("target", &[]))
    );
}

#[test]
fn selected_repeat_overrides_and_million_play_lookup_do_not_expand_voices() {
    let overrides = BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play(1),
            root: id("other"),
        }])
        .unwrap(),
    )]);
    let plan = RenderPlan::compile(&catalog(document(
        FrameRate::new(48_000, 1).unwrap(),
        &["repeat"],
        [
            ("child", source(8)),
            ("other", source(12)),
            ("repeat", repeat("child", 1_000_000)),
        ],
        overrides,
    )))
    .unwrap();
    let create = |node, ordinal| {
        plan.source_voice_occurrence(
            occurrence(node, &[("repeat", ordinal)]),
            recipe(&plan, q(1, 1), 4),
            Default::default(),
        )
    };
    let replaced = create("other", 1).unwrap();
    assert_eq!(replaced.extent(), q(8, 1)..q(20, 1));
    assert_eq!(replaced.instance(), &occurrence("other", &[("repeat", 1)]));
    assert!(create("child", 1).is_err());
    assert!(create("other", 0).is_err());
    let last = create("child", 999_999).unwrap();
    assert_eq!(
        last.samples(),
        AudioSample(7_999_996)..AudioSample(8_000_004)
    );
    assert!(last.construction_work() < 64);
    assert_eq!(plan.metadata().storage.authored_nodes, 4);
    assert_eq!(plan.metadata().storage.referenced_plays, 1_000_000);
    let raw = last
        .processing(
            AudioSample(7_999_997)..AudioSample(7_999_998),
            Default::default(),
        )
        .unwrap();
    assert_eq!(
        raw.spans[0]
            .source_point(AudioSample(7_999_997))
            .unwrap()
            .ticks,
        q(10, 1)
    );
    assert_eq!(
        raw.spans[0].instance,
        occurrence("child", &[("repeat", 999_999)])
    );
}

#[test]
fn nested_repeat_override_retains_both_scopes_across_preserve() {
    let overrides = BTreeMap::from([(
        id("inner-repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play(1),
            root: id("override"),
        }])
        .unwrap(),
    )]);
    let plan = RenderPlan::compile(&catalog(document(
        FrameRate::new(48_000, 1).unwrap(),
        &["prefix", "outer-repeat"],
        [
            ("prefix", source(3)),
            ("child", source(8)),
            ("override", source(10)),
            ("inner-repeat", repeat("child", 3)),
            (
                "stage",
                retime(
                    "inner-repeat",
                    13,
                    0,
                    26,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            ("outer-repeat", repeat("stage", 2)),
        ],
        overrides,
    )))
    .unwrap();
    let path = occurrence("override", &[("outer-repeat", 1), ("inner-repeat", 1)]);
    let voice = plan
        .source_voice_occurrence(path.clone(), recipe(&plan, q(1, 1), 4), Default::default())
        .unwrap();
    // Independent Preserve output retains the entire second outer play,
    // including decay outside the inner owner's mapped picture interval.
    assert_eq!(voice.samples(), AudioSample(16)..AudioSample(29));
    let stage = projected(&voice);
    assert_eq!(
        stage.stage().descriptor().instance,
        occurrence("stage", &[("outer-repeat", 1)])
    );
    assert_eq!(stage.input_support(), q(0, 1)..q(26, 1));
    let input = stage
        .input_tape()
        .unwrap()
        .query(SignalSample(9)..SignalSample(10), Default::default())
        .unwrap();
    assert_eq!(input.spans[0].instance, path);
    assert_eq!(
        input.spans[0].source_point(SignalSample(9)).unwrap().ticks,
        q(10, 1)
    );
    for invalid in [
        occurrence("override", &[("inner-repeat", 1)]),
        occurrence("override", &[("outer-repeat", 1), ("inner-repeat", 0)]),
        occurrence("child", &[("outer-repeat", 1), ("inner-repeat", 1)]),
    ] {
        assert!(
            plan.source_voice_occurrence(invalid, recipe(&plan, q(1, 1), 4), Default::default())
                .is_err()
        );
    }
}

#[test]
fn explicit_gap_branch_resolves_but_final_gap_and_fabricated_default_gap_do_not() {
    let mut repeated = repeat("child", 3);
    let NodeKind::Repeat { gap, .. } = &mut repeated.kind else {
        unreachable!()
    };
    *gap = Some(HoldRecipe {
        duration: frames(2),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
        picture_context: None,
    });
    let initial = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["repeat"],
        [("child", source(8)), ("repeat", repeated)],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(initial).unwrap();
    wire["nodes"]["gap-child"] = serde_json::to_value(hold(6)).unwrap();
    wire["nodes"]["final-gap"] = serde_json::to_value(hold(6)).unwrap();
    wire["gap_overrides"] = serde_json::to_value(BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![
            PlayOverride {
                iteration: play(0),
                root: id("gap-child"),
            },
            PlayOverride {
                iteration: play(2),
                root: id("final-gap"),
            },
        ])
        .unwrap(),
    )]))
    .unwrap();
    let plan = RenderPlan::compile(&catalog(
        ProjectDocument::from_json(&wire.to_string()).unwrap(),
    ))
    .unwrap();
    let create = |node, ordinal| {
        plan.source_voice_occurrence(
            occurrence(node, &[("repeat", ordinal)]),
            recipe(&plan, q(1, 1), 4),
            Default::default(),
        )
    };
    let gap = create("gap-child", 0).unwrap();
    assert_eq!(gap.samples(), AudioSample(8)..AudioSample(14));
    assert_eq!(
        gap.policy(gap.samples(), Default::default())
            .unwrap()
            .suppressed,
        vec![AudioSample(8)..AudioSample(14)]
    );
    assert!(create("gap-child", 1).is_err());
    assert!(create("final-gap", 2).is_err());
    assert!(create("repeat", 1).is_err());
    assert!(create("missing-default-gap", 1).is_err());
}

#[test]
fn ntsc_follow_speed_and_offset_round_once_on_root_preserving_source_phase() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let plan = RenderPlan::compile(&catalog(document(
        rate,
        &["prefix", "speed"],
        [
            ("prefix", source(1)),
            ("child", source(6)),
            (
                "speed",
                retime(
                    "child",
                    4,
                    0,
                    6,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Edit,
                ),
            ),
        ],
        BTreeMap::new(),
    )))
    .unwrap();
    let mut recipe = recipe(&plan, q(1, 2), 1000);
    recipe.offset = AudioSample(-1);
    let voice = plan
        .source_voice_occurrence(occurrence("child", &[]), recipe.clone(), Default::default())
        .unwrap();
    assert_eq!(voice.extent(), q(1, 1)..q(5, 1));
    assert_eq!(voice.samples(), AudioSample(1602)..AudioSample(8008));
    let raw = voice
        .processing(AudioSample(2135)..AudioSample(2136), Default::default())
        .unwrap();
    let span = &raw.spans[0];
    assert_eq!(span.grid.boundary_rule(), AudioBoundaryRule::RoundEven);
    assert_eq!(span.transform.project_origin, q(1, 1));
    assert_eq!(span.transform.project_frames_per_local_frame, q(2, 3));
    assert_eq!(span.sampling.local_frames_per_sample(), q(15, 16016));
    // Root point 2135 -> host 8001/16016; recipe starts at 7998/16016.
    assert_eq!(
        span.source_point(AudioSample(2135)).unwrap().ticks,
        q(103, 10)
    );
    assert_eq!(voice.source(), &recipe.source);
    let one = voice
        .processing(AudioSample(2136)..AudioSample(2137), Default::default())
        .unwrap();
    assert_eq!(
        one.spans[0].source_point(AudioSample(2136)).unwrap().ticks,
        q(59, 5)
    );
}

#[test]
fn missing_extra_or_foreign_play_paths_empty_and_invisible_hosts_fail() {
    let plan = RenderPlan::compile(&catalog(document(
        FrameRate::new(48_000, 1).unwrap(),
        &["repeat", "crop", "empty"],
        [
            ("child", source(8)),
            ("repeat", repeat("child", 2)),
            ("hidden", source(8)),
            ("visible", source(8)),
            (
                "input",
                BeatNode::sequence("Input", vec![id("hidden"), id("visible")]),
            ),
            (
                "crop",
                retime(
                    "input",
                    4,
                    8,
                    16,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            ("empty", BeatNode::sequence("Empty", vec![])),
        ],
        BTreeMap::new(),
    )))
    .unwrap();
    let create =
        |path| plan.source_voice_occurrence(path, recipe(&plan, q(1, 1), 4), Default::default());
    for path in [
        occurrence("absent", &[]),
        occurrence("empty", &[]),
        occurrence("hidden", &[]),
        occurrence("child", &[]),
        occurrence("child", &[("wrong-repeat", 0)]),
        occurrence("child", &[("repeat", 2)]),
        occurrence("child", &[("repeat", 0), ("repeat", 1)]),
        occurrence("root", &[("repeat", 0)]),
    ] {
        assert!(create(path).is_err());
    }
    let mut foreign = occurrence("child", &[("repeat", 0)]);
    foreign.repeats[0].iteration.allocation = RevisionId::new("foreign-allocation").unwrap();
    assert!(create(foreign).is_err());
    let valid = create(occurrence("child", &[("repeat", 0)])).unwrap();
    assert!(valid.belongs_to(&plan));
    assert!(!valid.belongs_to(&plan.clone()));
    assert_eq!(valid.source_identity(), valid.clone().source_identity());
}

#[test]
fn construction_queries_and_recipe_containment_are_bounded() {
    let plan = RenderPlan::compile(&catalog(document(
        FrameRate::new(48_000, 1).unwrap(),
        &["stage"],
        [
            ("child", source(8)),
            (
                "stage",
                retime("child", 4, 0, 8, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
        ],
        BTreeMap::new(),
    )))
    .unwrap();
    let create = |limits| {
        plan.source_voice_occurrence(occurrence("child", &[]), recipe(&plan, q(1, 1), 4), limits)
    };
    assert!(matches!(
        create(AudioQueryLimits {
            maximum_work: 1,
            maximum_spans: 4096
        }),
        Err(PlanError::AudioQueryLimit(_))
    ));
    assert!(matches!(
        create(AudioQueryLimits {
            maximum_work: 65_536,
            maximum_spans: 2
        }),
        Err(PlanError::AudioQueryLimit(_))
    ));
    let voice = create(Default::default()).unwrap();
    assert!(matches!(
        voice.processing(
            voice.samples(),
            AudioQueryLimits {
                maximum_work: 1,
                maximum_spans: 8
            }
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
    assert!(
        voice
            .processing(AudioSample(-1)..AudioSample(0), Default::default())
            .is_err()
    );
    assert!(
        voice
            .policy(AudioSample(3)..AudioSample(5), Default::default())
            .is_err()
    );
    for start in [q(-1, 1), q(5, 1)] {
        assert!(
            plan.source_voice_occurrence(
                occurrence("child", &[]),
                recipe(&plan, start, 4),
                Default::default()
            )
            .is_err()
        );
    }
    let mut foreign = recipe(&plan, q(1, 1), 4);
    foreign.source.asset = AssetId::new("missing").unwrap();
    assert!(
        plan.source_voice_occurrence(occurrence("child", &[]), foreign, Default::default())
            .is_err()
    );
}

#[test]
fn new_voice_ignores_original_retained_lattice_and_resume_but_uses_current_hold_policy() {
    let doc = catalog(document(
        FrameRate::new(48_000, 1).unwrap(),
        &["owner"],
        [("owner", hold(16))],
        BTreeMap::new(),
    ));
    let timing = AudioTimingId {
        allocation: RevisionId::new("retained").unwrap(),
        ordinal: 0,
    };
    let bindings = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing.clone(),
            layout: FrozenAudioLayout::capture(&doc).unwrap(),
        }],
        BTreeMap::from([(
            id("owner"),
            OwnedAudioBinding {
                reanchors: Vec::new(),
                lattice: AudioPlacementTemplate {
                    reference_local_offset: q(0, 1),
                    gap_after: None,
                    reference: AudioReferenceClock {
                        recipe: AudioRecipeKind::Node,
                        timing,
                        root: AudioClockRoot::ProjectRootRoundEven,
                        physical: id("owner"),
                    },
                    arguments: vec![],
                    births: vec![],
                },
                resume: Some(AudioResume {
                    local_boundary: q(1, 1),
                    phase: AudioLocalPhase {
                        constant: q(3, 7),
                        terms: vec![],
                    },
                }),
            },
        )]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(&doc).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let bound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&bound).unwrap();
    assert!(matches!(
        plan.audio_processing(AudioSample(2)..AudioSample(3), Default::default())
            .unwrap()
            .spans[0]
            .content,
        AudioSignalContent::Bound(_)
    ));
    let voice = voice(&plan, "owner");
    let raw = voice
        .processing(AudioSample(2)..AudioSample(3), Default::default())
        .unwrap();
    assert_eq!(
        raw.spans[0].source_point(AudioSample(2)).unwrap().ticks,
        q(11, 1)
    );
    assert!(matches!(
        raw.spans[0].content,
        AudioSignalContent::Leaf(AudioContent::Source { .. })
    ));
    assert_eq!(
        voice
            .policy(AudioSample(2)..AudioSample(3), Default::default())
            .unwrap()
            .suppressed,
        vec![AudioSample(2)..AudioSample(3)]
    );
}
