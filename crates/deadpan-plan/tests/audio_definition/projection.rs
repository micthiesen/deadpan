use super::*;
use std::ops::Range;
use std::sync::Arc;

use deadpan_plan::{
    AudioMixGate, AudioSignal, AudioSignalMix, AudioSignalTape, AudioSignalTapeRun,
    AudioStageProjection,
};

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

#[test]
fn mixed_projection_checks_every_voice_scope_even_when_fully_gated() {
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["repeat"],
        [
            ("source", source(4)),
            (
                "stage",
                retime(
                    "source",
                    2,
                    0,
                    4,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            ("repeat", repeat("stage", 2)),
        ],
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let scoped = stages(plan.audio_signal());
    let input = |index: usize| {
        tape(
            &plan,
            ratio(0, 1)..ratio(4, 1),
            ratio(0, 1)..ratio(4, 1),
            scoped[index].input_signal(),
        )
    };
    let policy = || {
        tape(
            &plan,
            ratio(0, 1)..ratio(2, 1),
            ratio(0, 1)..ratio(4, 1),
            scoped[0].input_signal(),
        )
    };
    let mixed = || AudioSignalMix::new(&plan, vec![input(0); 2], Vec::new()).unwrap();
    let projection =
        AudioStageProjection::new_mixed_input(scoped[0].clone(), mixed(), policy(), frames(2))
            .unwrap();
    assert!(projection.input_tape().is_none());
    assert_eq!(projection.input_mix().unwrap().voices().len(), 2);
    assert_eq!(projection.input_support(), ratio(0, 1)..ratio(4, 1));
    assert_eq!(projection.input_sample_count().unwrap(), SignalSample(4));
    assert_eq!(
        projection.output_policy().sample_count().unwrap(),
        SignalSample(2)
    );
    let other =
        AudioStageProjection::new_mixed_input(scoped[0].clone(), mixed(), policy(), frames(2))
            .unwrap();
    assert_ne!(projection.identity(), other.identity());

    let wrong_play = AudioSignalMix::new(
        &plan,
        vec![input(0), input(1)],
        vec![AudioMixGate::new(ratio(0, 1)..ratio(4, 1), vec![1])],
    )
    .unwrap();
    assert!(
        AudioStageProjection::new_mixed_input(scoped[0].clone(), wrong_play, policy(), frames(2))
            .is_err()
    );
    let self_voice = tape(
        &plan,
        ratio(0, 1)..ratio(4, 1),
        ratio(0, 1)..ratio(2, 1),
        scoped[0].output_signal(),
    );
    let self_mix = AudioSignalMix::new(&plan, vec![input(0), self_voice], Vec::new()).unwrap();
    assert!(
        AudioStageProjection::new_mixed_input(scoped[0].clone(), self_mix, policy(), frames(2))
            .is_err()
    );
}

#[test]
fn mixed_projection_retains_shared_nested_dag_and_aggregate_scope_budget() {
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["outer"],
        [
            ("source", source(4)),
            (
                "inner",
                retime(
                    "source",
                    2,
                    0,
                    4,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            (
                "outer",
                retime("inner", 1, 0, 2, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
        ],
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let outer = stages(plan.audio_signal()).remove(0);
    let inner = stages(outer.input_signal()).remove(0);
    let inner_input = tape(
        &plan,
        ratio(0, 1)..ratio(4, 1),
        ratio(0, 1)..ratio(4, 1),
        inner.input_signal(),
    );
    let inner_policy = || {
        tape(
            &plan,
            ratio(0, 1)..ratio(2, 1),
            ratio(0, 1)..ratio(4, 1),
            inner.input_signal(),
        )
    };
    let child = AudioStageProjection::new_mixed_input(
        inner.clone(),
        AudioSignalMix::new(&plan, vec![inner_input.clone(); 2], Vec::new()).unwrap(),
        inner_policy(),
        frames(2),
    )
    .unwrap();
    let routed = || {
        AudioSignalTape::new(
            &plan,
            ratio(0, 1)..ratio(2, 1),
            vec![AudioSignalTapeRun::intrinsic(
                ratio(0, 1)..ratio(2, 1),
                ratio(0, 1)..ratio(2, 1),
                Arc::clone(&child),
            )],
        )
        .unwrap()
    };
    let outer_policy = tape(
        &plan,
        ratio(0, 1)..ratio(1, 1),
        ratio(0, 1)..ratio(2, 1),
        outer.input_signal(),
    );
    let parent = AudioStageProjection::new_mixed_input(
        outer,
        AudioSignalMix::new(&plan, vec![routed(), routed()], Vec::new()).unwrap(),
        outer_policy,
        frames(1),
    )
    .unwrap();
    let query = parent
        .input_mix()
        .unwrap()
        .query(SignalSample(0)..SignalSample(2), Default::default())
        .unwrap();
    for voice in query.voices {
        let AudioSignalContent::ProjectedStage(projection) = &voice.signal.spans[0].content else {
            panic!("expected projected child")
        };
        assert_eq!(projection.identity(), child.identity());
    }

    // Each tape independently fits the run cap. Their scope edges must still
    // consume one enclosing validation allowance, including all mixed voices.
    let many_runs = AudioSignalTape::new(
        &plan,
        ratio(0, 1)..ratio(4, 1),
        (0..1024)
            .map(|index| {
                AudioSignalTapeRun::new(
                    ratio(index, 256)..ratio(index + 1, 256),
                    ratio(0, 1)..ratio(4, 1),
                    inner.input_signal(),
                )
            })
            .collect(),
    )
    .unwrap();
    let too_many = AudioSignalMix::new(&plan, vec![many_runs; 64], Vec::new()).unwrap();
    assert!(matches!(
        AudioStageProjection::new_mixed_input(inner.clone(), too_many, inner_policy(), frames(2)),
        Err(PlanError::AudioQueryLimit(_))
    ));
}

fn tape<'plan>(
    plan: &'plan RenderPlan,
    destination: Range<ExactRatio>,
    source: Range<ExactRatio>,
    signal: AudioSignal<'plan>,
) -> AudioSignalTape<'plan> {
    AudioSignalTape::new(
        plan,
        destination.clone(),
        vec![AudioSignalTapeRun::new(destination, source, signal)],
    )
    .unwrap()
}

fn stages(signal: AudioSignal<'_>) -> Vec<AudioStage<'_>> {
    signal
        .query(
            SignalSample(0)..signal.sample_count().unwrap(),
            AudioQueryLimits::default(),
        )
        .unwrap()
        .spans
        .into_iter()
        .map(|span| {
            let AudioSignalContent::Stage(stage) = span.content else {
                panic!("expected stage");
            };
            stage
        })
        .collect()
}

#[test]
fn intrinsic_policy_is_reclocked_before_rounding_a_hold_with_no_native_point() {
    let rate = FrameRate::new(96_000, 1).unwrap();
    let doc = document(
        rate,
        &["stage"],
        [
            ("a", source(1)),
            ("pause", hold(1)),
            ("b", source(2)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("a"), id("pause"), id("b")]),
            ),
            (
                "stage",
                retime("group", 2, 0, 4, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
        ],
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let stage = stages(plan.audio_signal()).remove(0);
    let input = tape(
        &plan,
        ExactRatio::ZERO..ratio(4, 1),
        ExactRatio::ZERO..ratio(4, 1),
        stage.input_signal(),
    );
    let policy = tape(
        &plan,
        ExactRatio::ZERO..ratio(2, 1),
        ExactRatio::ZERO..ratio(4, 1),
        stage.input_signal(),
    );
    assert!(
        input
            .policy(
                SignalSample(0)..SignalSample(2),
                AudioQueryLimits::default()
            )
            .unwrap()
            .suppressed
            .is_empty()
    );
    assert!(
        policy
            .policy(
                SignalSample(0)..SignalSample(1),
                AudioQueryLimits::default()
            )
            .unwrap()
            .suppressed
            .is_empty()
    );
    let projection = AudioStageProjection::new(stage, input, policy, frames(2)).unwrap();
    let schedule = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(8, 1),
        vec![AudioSignalTapeRun::intrinsic(
            ExactRatio::ZERO..ratio(8, 1),
            ExactRatio::ZERO..ratio(2, 1),
            Arc::clone(&projection),
        )],
    )
    .unwrap();
    let policy = schedule
        .policy(
            SignalSample(0)..SignalSample(4),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(policy.suppressed, vec![SignalSample(1)..SignalSample(2)]);
    let query = schedule
        .query(
            SignalSample(1)..SignalSample(3),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(query.spans.len(), 1);
    assert_eq!(
        query.spans[0].sampling.local_at(SignalSample(1)).unwrap(),
        ratio(1, 2)
    );
    assert!(
        matches!(&query.spans[0].content, AudioSignalContent::ProjectedStage(found) if found.identity() == projection.identity())
    );
    let wire = serde_json::to_value(&query).unwrap();
    assert!(
        !wire.to_string().contains("input_tape"),
        "serialization is descriptor-only"
    );
}

#[test]
fn projection_rejects_foreign_scopes_clocks_and_self_providers() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let doc = document(
        rate,
        &["repeat"],
        [
            ("source", source(4)),
            (
                "stage",
                retime(
                    "source",
                    2,
                    0,
                    4,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            ("repeat", repeat("stage", 2)),
        ],
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let scoped = stages(plan.audio_signal());
    assert_eq!(scoped.len(), 2);
    let first = &scoped[0];
    let second = &scoped[1];
    let input = || {
        tape(
            &plan,
            ExactRatio::ZERO..ratio(4, 1),
            ExactRatio::ZERO..ratio(4, 1),
            first.input_signal(),
        )
    };
    let policy = || {
        tape(
            &plan,
            ExactRatio::ZERO..ratio(2, 1),
            ExactRatio::ZERO..ratio(4, 1),
            first.input_signal(),
        )
    };
    assert!(AudioStageProjection::new(first.clone(), input(), policy(), frames(2)).is_ok());
    let wrong_play = tape(
        &plan,
        ExactRatio::ZERO..ratio(4, 1),
        ExactRatio::ZERO..ratio(4, 1),
        second.input_signal(),
    );
    assert!(AudioStageProjection::new(first.clone(), wrong_play, policy(), frames(2)).is_err());
    let detached = tape(
        &plan,
        ExactRatio::ZERO..ratio(4, 1),
        ExactRatio::ZERO..ratio(4, 1),
        plan.audio_definition(selection("source")).unwrap().signal(),
    );
    assert!(AudioStageProjection::new(first.clone(), detached, policy(), frames(2)).is_err());
    let self_provider = tape(
        &plan,
        ExactRatio::ZERO..ratio(4, 1),
        ExactRatio::ZERO..ratio(2, 1),
        first.output_signal(),
    );
    assert!(AudioStageProjection::new(first.clone(), self_provider, policy(), frames(2)).is_err());
    let wrong_clock = tape(
        &plan,
        ExactRatio::ZERO..ratio(3, 1),
        ExactRatio::ZERO..ratio(4, 1),
        first.input_signal(),
    );
    assert!(AudioStageProjection::new(first.clone(), wrong_clock, policy(), frames(2)).is_err());
    let foreign = RenderPlan::compile(&doc).unwrap();
    let foreign_stage = stages(foreign.audio_signal()).remove(0);
    let foreign_input = tape(
        &foreign,
        ExactRatio::ZERO..ratio(4, 1),
        ExactRatio::ZERO..ratio(4, 1),
        foreign_stage.input_signal(),
    );
    assert!(AudioStageProjection::new(first.clone(), foreign_input, policy(), frames(2)).is_err());
}

#[test]
fn projection_keeps_nonzero_source_selection_on_a_normalized_input_clock() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let doc = document(
        rate,
        &["stage"],
        [
            ("source", source(7)),
            (
                "stage",
                retime(
                    "source",
                    2,
                    3,
                    7,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
        ],
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let stage = stages(plan.audio_signal()).remove(0);
    let input = tape(
        &plan,
        ExactRatio::ZERO..ratio(4, 1),
        ratio(3, 1)..ratio(7, 1),
        stage.input_signal(),
    );
    let policy = tape(
        &plan,
        ExactRatio::ZERO..ratio(2, 1),
        ratio(3, 1)..ratio(7, 1),
        stage.input_signal(),
    );
    let projection = AudioStageProjection::new(stage, input, policy, frames(2)).unwrap();
    assert_eq!(projection.input_sample_count().unwrap(), SignalSample(6407));
    assert_eq!(
        projection.output_policy().sample_count().unwrap(),
        SignalSample(3204)
    );
    assert_eq!(plan.audio_duration().unwrap(), AudioSample(3203));
    let query = projection
        .input_tape()
        .unwrap()
        .query(
            SignalSample(0)..SignalSample(1),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(
        query.spans[0].sampling.local_at(SignalSample(0)).unwrap(),
        ratio(3, 1)
    );
}

#[test]
fn mapped_policy_expansion_is_bounded_before_shared_children_are_duplicated() {
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["outer"],
        [
            ("source", source(4)),
            (
                "inner",
                retime(
                    "source",
                    2,
                    0,
                    4,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            (
                "outer",
                retime("inner", 1, 0, 2, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
        ],
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let outer = stages(plan.audio_signal()).remove(0);
    let inner = stages(outer.input_signal()).remove(0);
    let input = tape(
        &plan,
        ExactRatio::ZERO..ratio(4, 1),
        ExactRatio::ZERO..ratio(4, 1),
        inner.input_signal(),
    );
    let policy = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(2, 1),
        (0..256)
            .map(|index| {
                AudioSignalTapeRun::new(
                    ratio(index, 128)..ratio(index + 1, 128),
                    ExactRatio::ZERO..ratio(4, 1),
                    inner.input_signal(),
                )
            })
            .collect(),
    )
    .unwrap();
    let child = AudioStageProjection::new(inner, input, policy, frames(2)).unwrap();
    // Only one unique child projection, but 256*256 policy runs would unfold.
    let expanded = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(2, 1),
        (0..256)
            .map(|index| {
                AudioSignalTapeRun::intrinsic(
                    ratio(index, 128)..ratio(index + 1, 128),
                    ExactRatio::ZERO..ratio(2, 1),
                    Arc::clone(&child),
                )
            })
            .collect(),
    );
    assert!(matches!(expanded, Err(PlanError::AudioQueryLimit(_))));
}

#[test]
fn intrinsic_policy_window_preserves_bound_support_outside_the_visible_run() {
    let old = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["inner"],
        [
            ("hold", hold(2)),
            ("source", source(14)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("hold"), id("source")]),
            ),
            (
                "inner",
                retime(
                    "group",
                    8,
                    0,
                    16,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
        ],
        BTreeMap::new(),
    );
    let timing = AudioTimingId {
        allocation: RevisionId::new("bound-policy").unwrap(),
        ordinal: 0,
    };
    let bindings = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing.clone(),
            layout: FrozenAudioLayout::capture(&old).unwrap(),
        }],
        BTreeMap::from([(
            id("inner"),
            OwnedAudioBinding {
                reanchors: vec![],
                resume: None,
                lattice: AudioPlacementTemplate {
                    reference_local_offset: deadpan_core::ExactRatio::ZERO,
                    gap_after: None,
                    arguments: vec![],
                    births: vec![],
                    reference: AudioReferenceClock {
                        recipe: AudioRecipeKind::Node,
                        timing,
                        root: AudioClockRoot::ProjectRootRoundEven,
                        physical: id("inner"),
                    },
                },
            },
        )]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["nodes"]["outer"] = serde_json::to_value(retime(
        "inner",
        4,
        0,
        8,
        PitchPolicy::Preserve,
        RetimePurpose::Edit,
    ))
    .unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("outer")])).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&current).unwrap();
    let outer = stages(plan.audio_signal()).remove(0);
    let input = tape(
        &plan,
        ExactRatio::ZERO..ratio(8, 1),
        ExactRatio::ZERO..ratio(8, 1),
        outer.input_signal(),
    );
    let policy = tape(
        &plan,
        ExactRatio::ZERO..ratio(4, 1),
        ExactRatio::ZERO..ratio(8, 1),
        outer.input_signal(),
    );
    let projection = AudioStageProjection::new(outer, input, policy, frames(4)).unwrap();
    let schedule = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ExactRatio::ONE,
        vec![AudioSignalTapeRun::intrinsic(
            ExactRatio::ZERO..ExactRatio::ONE,
            ratio(11, 20)..ratio(7, 10),
            projection,
        )],
    )
    .unwrap();
    let policy = schedule
        .policy(
            SignalSample(0)..SignalSample(1),
            AudioQueryLimits::default(),
        )
        .unwrap();
    // Current B-local1.1 is Source. Retained reference0.9 still owns Hold sample0.
    // Cropping meaningful support to [1.1,1.4) rounds it empty and loses the mask.
    assert_eq!(policy.suppressed, vec![SignalSample(0)..SignalSample(1)]);
}
