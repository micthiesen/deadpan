use super::*;
use std::sync::Arc;

use deadpan_plan::{AudioProjectedRoot, AudioSignalTape, AudioSignalTapeRun, AudioStageProjection};

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn preserve_projection<'plan>(
    plan: &'plan RenderPlan,
    stage: AudioStage<'plan>,
) -> Arc<AudioStageProjection<'plan>> {
    let input_signal = stage.input_signal();
    let input_support = input_signal.support();
    let input_length = input_support.end.checked_sub(input_support.start).unwrap();
    let input = AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..input_length,
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..input_length,
            input_support.clone(),
            input_signal.clone(),
        )],
    )
    .unwrap();
    let duration = stage.descriptor().duration;
    let output_signal = stage.input_signal();
    let output = AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..ExactRatio::integer(duration.frames()),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ExactRatio::integer(duration.frames()),
            input_support,
            output_signal,
        )],
    )
    .unwrap();
    AudioStageProjection::new(stage, input, output, duration).unwrap()
}

fn grouped_hold_plan() -> RenderPlan {
    compile(
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
    )
}

#[test]
fn projected_root_regrids_subsample_hold_and_resumes_old_policy_phase() {
    let plan = grouped_hold_plan();
    let signal = plan.audio_definition(selection("stage")).unwrap().signal();
    let projection = preserve_projection(&plan, first_stage(&signal));

    // The child Hold maps to output [1/2, 1). Neither PointCeil label 0 nor 1
    // owns it, but the root RoundEven probe for sample 0 is at frame 1/2.
    assert!(
        projection
            .output_policy()
            .policy_after_preserve(
                SignalSample(0)..SignalSample(2),
                AudioQueryLimits::default(),
            )
            .unwrap()
            .suppressed
            .is_empty()
    );
    let placement =
        AudioRootPlacement::new(ratio(-1, 4), ExactRatio::ONE, ExactRatio::ZERO..ratio(2, 1))
            .unwrap();
    let root = AudioProjectedRoot::new(projection, placement).unwrap();
    assert_eq!(root.samples(), AudioSample(0)..AudioSample(2));
    assert_eq!(
        root.policy(AudioSample(0)..AudioSample(1), Default::default())
            .unwrap()
            .suppressed,
        vec![AudioSample(0)..AudioSample(1)]
    );

    // Resume three absolute samples later from old cut zero. The new label's
    // parity differs, so this also proves policy follows old reference labels.
    let resumed = root
        .resume(AudioSample(0), ratio(3, 1)..ratio(5, 1))
        .unwrap();
    assert_eq!(
        resumed
            .policy(AudioSample(3)..AudioSample(4), Default::default())
            .unwrap()
            .suppressed,
        vec![AudioSample(3)..AudioSample(4)]
    );
    assert_eq!(
        resumed.sampling().local_at(AudioSample(3)).unwrap(),
        root.sampling().local_at(AudioSample(0)).unwrap()
    );
    let exhausted = root
        .resume(AudioSample(2), ratio(5, 1)..ratio(7, 1))
        .unwrap();
    assert_eq!(
        exhausted
            .policy(AudioSample(5)..AudioSample(7), Default::default())
            .unwrap()
            .suppressed,
        vec![AudioSample(5)..AudioSample(7)]
    );
}

#[test]
fn projected_root_signed_roundeven_ties_allow_empty_allocations_and_bound_crop() {
    let plan = grouped_hold_plan();
    let signal = plan.audio_definition(selection("stage")).unwrap().signal();
    let projection = preserve_projection(&plan, first_stage(&signal));
    let tied_empty = AudioProjectedRoot::new(
        Arc::clone(&projection),
        AudioRootPlacement::new(
            ratio(-5, 2),
            ExactRatio::ONE,
            ExactRatio::ZERO..ExactRatio::ONE,
        )
        .unwrap(),
    )
    .unwrap();
    // Negative ties -2.5 and -1.5 both round to the even label -2.
    assert_eq!(tied_empty.samples(), AudioSample(-2)..AudioSample(-2));
    assert!(
        tied_empty
            .policy(AudioSample(-2)..AudioSample(-2), Default::default())
            .unwrap()
            .suppressed
            .is_empty()
    );

    let tied_nonempty = AudioProjectedRoot::new(
        Arc::clone(&projection),
        AudioRootPlacement::new(
            ratio(-7, 2),
            ExactRatio::ONE,
            ExactRatio::ZERO..ExactRatio::ONE,
        )
        .unwrap(),
    )
    .unwrap();
    // -3.5 and -2.5 round to -4 and -2, respectively.
    assert_eq!(tied_nonempty.samples(), AudioSample(-4)..AudioSample(-2));
    assert_eq!(
        tied_nonempty
            .policy(AudioSample(-4)..AudioSample(-2), Default::default())
            .unwrap()
            .suppressed,
        vec![AudioSample(-3)..AudioSample(-2)]
    );
    let resumed_empty = tied_empty
        .resume(AudioSample(-2), ExactRatio::ZERO..ExactRatio::ONE)
        .unwrap();
    assert_eq!(
        resumed_empty
            .policy(AudioSample(0)..AudioSample(1), Default::default())
            .unwrap()
            .suppressed,
        vec![AudioSample(0)..AudioSample(1)]
    );

    let root = AudioProjectedRoot::new(
        Arc::clone(&projection),
        AudioRootPlacement::new(ratio(-1, 4), ExactRatio::ONE, ExactRatio::ZERO..ratio(2, 1))
            .unwrap(),
    )
    .unwrap();
    let cropped = root.crop(ExactRatio::ZERO..ExactRatio::ONE).unwrap();
    assert_eq!(cropped.samples(), AudioSample(0)..AudioSample(1));
    assert_eq!(
        cropped.sampling().local_at(AudioSample(0)).unwrap(),
        root.sampling().local_at(AudioSample(0)).unwrap()
    );
    assert!(root.crop(ExactRatio::ZERO..ratio(3, 1)).is_err());
    assert!(root.crop(ExactRatio::ONE..ExactRatio::ONE).is_err());
    assert!(
        root.resume(AudioSample(3), ratio(3, 1)..ratio(4, 1))
            .is_err()
    );

    let overflow = AudioRootPlacement::new(
        ExactRatio::integer(i64::MAX),
        ExactRatio::ONE,
        ExactRatio::ZERO..ExactRatio::ONE,
    )
    .unwrap();
    assert!(AudioProjectedRoot::new(Arc::clone(&projection), overflow).is_err());

    let value = serde_json::to_value(&root).unwrap();
    assert!(value.get("policy").is_none());
    assert!(value["projection"].get("input_tape").is_none());
    assert!(value["projection"].get("output_policy").is_none());
}

#[test]
fn projected_root_regrids_bound_retained_hold_context_on_absolute_grid() {
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
        allocation: RevisionId::new("root-bound-policy").unwrap(),
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
    let outer = first_stage(&plan.audio_signal());
    let input = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(8, 1),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ratio(8, 1),
            ExactRatio::ZERO..ratio(8, 1),
            outer.input_signal(),
        )],
    )
    .unwrap();
    let output_policy_signal = outer.input_signal();
    let output_policy = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(4, 1),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ratio(4, 1),
            ExactRatio::ZERO..ratio(8, 1),
            output_policy_signal,
        )],
    )
    .unwrap();
    let projection = AudioStageProjection::new(outer, input, output_policy, frames(4)).unwrap();
    let root = AudioProjectedRoot::new(
        projection,
        AudioRootPlacement::new(ratio(-11, 8), ratio(5, 2), ratio(11, 20)..ratio(2, 1)).unwrap(),
    )
    .unwrap();
    assert_eq!(root.samples(), AudioSample(0)..AudioSample(4));
    // Current root probe 1/2 reaches B-local 3/2 (Source), but B's rounded
    // allocation begins at -1 and its retained sample step is 4/5. The old
    // root label 4/5 still belongs to Hold sample zero. Cropping policy support
    // to the routed window would lose that retained context.
    assert_eq!(
        root.policy(AudioSample(0)..AudioSample(1), Default::default())
            .unwrap()
            .suppressed,
        vec![AudioSample(0)..AudioSample(1)]
    );
}
