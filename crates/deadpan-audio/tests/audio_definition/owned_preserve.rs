use super::*;
use deadpan_plan::{AudioSignalContent, AudioSignalTape};

fn input_run(
    destination: Range<i64>,
    source: Range<i64>,
    target: &str,
    intrinsic: bool,
) -> PreserveInputRun {
    let target = InstancePath {
        node: id(target),
        repeats: vec![],
    };
    PreserveInputRun {
        destination: ExactFrameRange::new(
            ratio(destination.start.into(), 1),
            ratio(destination.end.into(), 1),
        )
        .unwrap(),
        source: ExactFrameRange::new(ratio(source.start.into(), 1), ratio(source.end.into(), 1))
            .unwrap(),
        provider: if intrinsic {
            PreserveInputProvider::Intrinsic { target }
        } else {
            PreserveInputProvider::Current { target }
        },
    }
}

fn fixture(derived: bool, shifted: bool, fractional: bool) -> ProjectDocument {
    let (rate, a, b, inner, outer, pause, selected_a, selected_b) = if fractional {
        (
            FrameRate::new(30_000, 1001).unwrap(),
            2,
            3,
            4,
            2,
            1,
            0..3203,
            3203..8008,
        )
    } else {
        (
            FrameRate::new(48_000, 1).unwrap(),
            1024,
            2048,
            2048,
            1024,
            768,
            0..1024,
            if shifted { 3072..5120 } else { 4096..6144 },
        )
    };
    let original = document(
        rate,
        &["outer"],
        [
            ("a", source(rate, a, selected_a)),
            ("b", source(rate, b, selected_b)),
            (
                "group",
                BeatNode::sequence("Live group", vec![id("a"), id("b")]),
            ),
            (
                "inner",
                retime("group", inner, 0..a + b, PitchPolicy::Preserve),
            ),
            (
                "outer",
                retime("inner", outer, 0..inner, PitchPolicy::Preserve),
            ),
        ],
        BTreeMap::new(),
    );
    if !derived {
        return original;
    }
    let mut wire = serde_json::to_value(original).unwrap();
    wire["nodes"]["pause"] = serde_json::to_value(hold(pause, HoldAudio::Silence)).unwrap();
    wire["nodes"]["group"] = serde_json::to_value(BeatNode::sequence(
        "Live group",
        vec![id("a"), id("pause"), id("b")],
    ))
    .unwrap();
    let inner_extent = if fractional {
        ratio(24, 5)
    } else {
        ratio(2560, 1)
    };
    wire["derived_clocks"] = serde_json::to_value(BTreeMap::from([
        (
            id("inner"),
            DerivedRetimeClock {
                extent: inner_extent,
                child_start: ExactRatio::ZERO,
                child_end: ratio((a + b + pause).into(), 1),
                preserve_input: Some(
                    PreserveInputProjection::new(vec![
                        input_run(0..a, 0..a, "a", false),
                        input_run(a..a + b, 0..b, "b", false),
                    ])
                    .unwrap(),
                ),
            },
        ),
        (
            id("outer"),
            DerivedRetimeClock {
                extent: ratio(if fractional { 3 } else { 1280 }, 1),
                child_start: ExactRatio::ZERO,
                child_end: inner_extent,
                preserve_input: Some(
                    PreserveInputProjection::new(vec![input_run(
                        0..inner,
                        0..inner,
                        "inner",
                        true,
                    )])
                    .unwrap(),
                ),
            },
        ),
    ]))
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let reopened = ProjectDocument::from_json(&document.to_compact_json().unwrap()).unwrap();
    assert_eq!(reopened, document);
    reopened
}

fn read_projected(
    plan: &Arc<RenderPlan>,
    output: &AudioSignalTape<'_>,
    chunk: u32,
) -> Vec<[f32; 2]> {
    let mut renderer = StageAudio::with_limits(
        Arc::clone(plan),
        StageLimits {
            maximum_prepared_stages: 2,
            ..Default::default()
        },
    )
    .unwrap();
    let mut provider = FixtureProvider::new();
    let count = output.sample_count().unwrap().0;
    let mut result = Vec::new();
    while (result.len() as i64) < count {
        let start = result.len() as i64;
        let count = chunk.min((count - start) as u32);
        result.extend(
            renderer
                .read_tape(
                    &mut provider,
                    output,
                    SignalSample(start),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    assert_eq!(
        renderer.cached_stage_count(),
        0,
        "owned input does not use an authored-descriptor cache"
    );
    result
}

#[test]
fn owned_preserve_saved_nested_inputs_match_complete_preinsertion_pcm_and_live_recipes() {
    let a = sample_reference(
        0..1024,
        ExactRatio::ZERO,
        ExactRatio::ONE,
        1024,
        fixture_sample,
    );
    let mut previous = None;
    for shifted in [false, true] {
        let selected = if shifted { 3072..5120 } else { 4096..6144 };
        let b = sample_reference(
            selected.clone(),
            ratio(selected.start.into(), 1),
            ExactRatio::ONE,
            2048,
            fixture_sample,
        );
        let input: Vec<_> = a.iter().copied().chain(b).collect();
        let expected = stretch(&stretch(&input, 2048, 3, 2), 1024, 2, 1);
        let plan = compile(&fixture(true, shifted, false), false);
        let projected = plan.owned_preserve_projection(node("outer")).unwrap();
        assert_eq!(projected.stage().descriptor().duration, frames(1024));
        assert_eq!(projected.stage().descriptor().rate, ratio(2, 1));
        assert_eq!(plan.node_exact_duration(&id("outer")), Some(ratio(1280, 1)));
        assert_eq!(projected.input_sample_count().unwrap(), SignalSample(2048));
        let child = projected
            .input_tape()
            .unwrap()
            .query(SignalSample(0)..SignalSample(1), Default::default())
            .unwrap();
        let AudioSignalContent::ProjectedStage(inner) = &child.spans[0].content else {
            panic!("intrinsic child")
        };
        assert_eq!(inner.input_sample_count().unwrap(), SignalSample(3072));
        assert_eq!(
            inner.output_policy().sample_count().unwrap(),
            SignalSample(2048)
        );
        let tape = super::projection::output(&plan, projected);
        for chunk in [127, 256] {
            assert_eq!(read_projected(&plan, &tape, chunk), expected);
        }
        if let Some(previous) = previous {
            assert_ne!(
                previous, expected,
                "a current source edit changes the complete nested result"
            );
        }
        previous = Some(expected);
    }
}

#[test]
fn owned_preserve_fractional_frame_grid_keeps_original_phase_across_input_runs() {
    let original = compile(&fixture(false, false, true), false);
    let derived = compile(&fixture(true, false, true), false);
    let definition = original.audio_definition(node("outer")).unwrap();
    let signal = definition.signal();
    let count = signal.sample_count().unwrap().0;
    assert_eq!(
        count, 3204,
        "intrinsic PointCeil differs from root RoundEven"
    );
    let mut renderer = StageAudio::new(Arc::clone(&original));
    let mut provider = FixtureProvider::new();
    let mut expected = Vec::new();
    while (expected.len() as i64) < count {
        let start = expected.len() as i64;
        expected.extend(
            renderer
                .read_definition(
                    &mut provider,
                    &definition,
                    SignalSample(start),
                    256.min((count - start) as u32),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    let projection = derived.owned_preserve_projection(node("outer")).unwrap();
    assert_eq!(projection.input_sample_count().unwrap(), SignalSample(6407));
    let tape = super::projection::output(&derived, projection);
    assert_eq!(read_projected(&derived, &tape, 127), expected);
}

#[test]
fn owned_preserve_unplaced_outputs_and_legacy_child_input_fail_explicitly() {
    let mut wire = serde_json::to_value(fixture(true, false, false)).unwrap();
    // Effective picture scale may be unity even though the retained intrinsic
    // recipe is nonunity. It must not bypass the preparation/placement boundary.
    wire["derived_clocks"]["outer"]["extent"] = serde_json::to_value(ratio(2560, 1)).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = compile(&document, false);
    let projection = plan.owned_preserve_projection(node("outer")).unwrap();
    let requested = SignalSample(0)..SignalSample(1);
    assert!(
        projection
            .stage()
            .input_signal()
            .query(requested.clone(), Default::default())
            .is_err()
    );
    assert!(
        projection
            .stage()
            .output_signal()
            .query_flattened(requested.clone(), Default::default())
            .is_err()
    );
    assert!(
        plan.audio_signal()
            .query(requested.clone(), Default::default())
            .is_err()
    );
    assert!(
        plan.audio_signal()
            .query_flattened(requested, Default::default())
            .is_err()
    );
    assert!(
        plan.audio(AudioSample(0)..AudioSample(1), Default::default())
            .is_err()
    );
    assert!(plan.owned_preserve_projection(node("group")).is_err());
    assert!(plan.owned_preserve_projection(node("absent")).is_err());
    assert!(FrozenAudioContext::capture(&document).is_err());
}

#[test]
fn owned_preserve_unity_picture_maps_cannot_bypass_independent_voice_processing() {
    let mut wire = serde_json::to_value(fixture(true, false, false)).unwrap();
    for owner in ["inner", "outer"] {
        wire["derived_clocks"][owner]["extent"] = serde_json::to_value(ratio(3840, 1)).unwrap();
        wire["derived_clocks"][owner]["child_end"] = serde_json::to_value(ratio(3840, 1)).unwrap();
    }
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("d".repeat(64));
    wire["assets"]["media"]["still_image"] = false.into();
    let plan = compile(
        &ProjectDocument::from_json(&wire.to_string()).unwrap(),
        false,
    );
    let selected = audio(0..512);
    let recipe = deadpan_plan::AudioSourceVoiceRecipe {
        mapping: SourceAudioMapping::natural_rate(
            selected.span,
            FrameRate::new(48_000, 1).unwrap(),
        )
        .unwrap(),
        source: selected,
        offset: AudioSample(0),
    };
    for owner in ["a", "inner", "outer"] {
        let result = plan.source_voice_occurrence(
            InstancePath {
                node: id(owner),
                repeats: vec![],
            },
            recipe.clone(),
            Default::default(),
        );
        assert!(
            matches!(
                result,
                Err(deadpan_plan::PlanError::InvalidAudioSourceOccurrence(
                    "owned Preserve occurrence requires its retained output placement"
                ))
            ),
            "{owner}: {result:?}"
        );
    }
}

#[test]
fn owned_preserve_output_policy_keeps_silence_with_no_input_grid_point() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let original = document(
        rate,
        &["stage"],
        [
            ("a", source(rate, 1024, 0..1024)),
            ("b", source(rate, 1024, 4096..5120)),
            ("pause", hold(1, HoldAudio::Silence)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("a"), id("pause"), id("b")]),
            ),
            (
                "stage",
                retime("group", 4096, 0..2048, PitchPolicy::Preserve),
            ),
        ],
        BTreeMap::new(),
    );
    let mut runs = vec![
        input_run(0..1024, 0..1024, "a", false),
        input_run(1024..1025, 0..1, "pause", false),
        input_run(1025..2048, 0..1024, "b", false),
    ];
    runs[0].destination.end = ratio(4097, 4);
    runs[1].destination = ExactFrameRange::new(ratio(4097, 4), ratio(4099, 4)).unwrap();
    runs[2].destination.start = ratio(4099, 4);
    let mut wire = serde_json::to_value(original).unwrap();
    wire["derived_clocks"] = serde_json::to_value(BTreeMap::from([(
        id("stage"),
        DerivedRetimeClock {
            extent: ratio(4098, 1),
            child_start: ExactRatio::ZERO,
            child_end: ratio(2049, 1),
            preserve_input: Some(PreserveInputProjection::new(runs).unwrap()),
        },
    )]))
    .unwrap();
    let plan = compile(
        &ProjectDocument::from_json(&wire.to_string()).unwrap(),
        false,
    );
    let projection = plan.owned_preserve_projection(node("stage")).unwrap();
    assert!(
        projection
            .input_tape()
            .unwrap()
            .policy(SignalSample(1023)..SignalSample(1027), Default::default())
            .unwrap()
            .suppressed
            .is_empty()
    );
    assert_eq!(
        projection
            .output_policy()
            .policy_after_preserve(SignalSample(2048)..SignalSample(2052), Default::default())
            .unwrap()
            .suppressed,
        vec![SignalSample(2049)..SignalSample(2050)]
    );
    let output = super::projection::output(&plan, projection);
    let block = StageAudio::new(Arc::clone(&plan))
        .read_tape(
            &mut FixtureProvider::new(),
            &output,
            SignalSample(2048),
            4,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.samples[1], [0.0; 2]);
    assert!(block.samples.iter().any(|sample| *sample != [0.0; 2]));
    assert_eq!(
        block.suppressed,
        vec![SignalSample(2049)..SignalSample(2050)]
    );
}

#[test]
fn owned_preserve_nested_repeat_paths_keep_each_intrinsic_occurrence_scoped() {
    let mut wire = serde_json::to_value(fixture(true, false, false)).unwrap();
    let iterations = IterationOrder::new(RevisionId::new("plays").unwrap(), 2).unwrap();
    let mut repeat = BeatNode::sequence("Repeat", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: id("inner"),
        iterations: iterations.clone(),
        gap: None,
        escalation: None,
    };
    wire["nodes"]["repeat"] = serde_json::to_value(repeat).unwrap();
    wire["nodes"]["outer"] =
        serde_json::to_value(retime("repeat", 2048, 0..4096, PitchPolicy::Preserve)).unwrap();
    let mut runs = Vec::new();
    for play in 0..2 {
        let mut run = input_run(
            i64::from(play) * 2048..(i64::from(play) + 1) * 2048,
            0..2048,
            "inner",
            true,
        );
        let PreserveInputProvider::Intrinsic { target } = &mut run.provider else {
            panic!()
        };
        target.repeats.push(RepeatInstance {
            node: id("repeat"),
            iteration: iterations.at(play).unwrap(),
        });
        runs.push(run);
    }
    wire["derived_clocks"]["outer"] = serde_json::to_value(DerivedRetimeClock {
        extent: ratio(2560, 1),
        child_start: ExactRatio::ZERO,
        child_end: ratio(5120, 1),
        preserve_input: Some(PreserveInputProjection::new(runs).unwrap()),
    })
    .unwrap();
    let plan = compile(
        &ProjectDocument::from_json(&wire.to_string()).unwrap(),
        false,
    );
    let projection = plan.owned_preserve_projection(node("outer")).unwrap();
    for play in 0..2 {
        let sample = i64::from(play) * 2048;
        let query = projection
            .input_tape()
            .unwrap()
            .query(
                SignalSample(sample)..SignalSample(sample + 1),
                Default::default(),
            )
            .unwrap();
        let AudioSignalContent::ProjectedStage(inner) = &query.spans[0].content else {
            panic!()
        };
        let expected = vec![RepeatInstance {
            node: id("repeat"),
            iteration: iterations.at(play).unwrap(),
        }];
        assert_eq!(inner.stage().descriptor().instance.repeats, expected);
        let leaf = inner
            .input_tape()
            .unwrap()
            .query(SignalSample(0)..SignalSample(1), Default::default())
            .unwrap();
        assert_eq!(leaf.spans[0].instance.repeats, expected);
    }
    let a = sample_reference(
        0..1024,
        ExactRatio::ZERO,
        ExactRatio::ONE,
        1024,
        fixture_sample,
    );
    let b = sample_reference(
        4096..6144,
        ratio(4096, 1),
        ExactRatio::ONE,
        2048,
        fixture_sample,
    );
    let input: Vec<_> = a.into_iter().chain(b).collect();
    let inner = stretch(&input, 2048, 3, 2);
    let expected = stretch(&inner.repeat(2), 2048, 2, 1);
    let output = super::projection::output(&plan, projection);
    let mut reader = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_prepared_stages: 3,
            ..Default::default()
        },
    )
    .unwrap();
    for start in [0, 768, 1536] {
        let block = reader
            .read_tape(
                &mut FixtureProvider::new(),
                &output,
                SignalSample(start),
                256,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            block.samples,
            expected[start as usize..start as usize + 256]
        );
    }
}
