use super::*;
use deadpan_plan::{AudioMixGate, AudioSignalMix};
use std::ops::Range;

fn voice<'plan>(
    plan: &'plan RenderPlan,
    node: &str,
    support: Range<ExactRatio>,
) -> AudioSignalTape<'plan> {
    AudioSignalTape::new(
        plan,
        support.clone(),
        vec![run(support, ratio(0, 1)..ratio(4, 1), signal(plan, node))],
    )
    .unwrap()
}

#[test]
fn mix_gates_select_voices_and_intersect_silence_without_hiding_content() {
    let plan = make_plan(FrameRate::new(48_000, 1).unwrap(), 4);
    let support = ratio(0, 1)..ratio(4, 1);
    let make_voice = |node| voice(&plan, node, support.clone());
    let mix = AudioSignalMix::new(
        &plan,
        vec![make_voice("left"), make_voice("source")],
        vec![AudioMixGate::new(ratio(1, 2)..ratio(5, 2), vec![1])],
    )
    .unwrap();
    let query = mix
        .query(SignalSample(0)..SignalSample(4), Default::default())
        .unwrap();
    assert_eq!(query.voices.len(), 2);
    assert_eq!(query.voices[0].index, 0);
    assert_eq!(query.voices[1].index, 1);
    assert_eq!(
        query.voices[0].policy.suppressed,
        vec![SignalSample(0)..SignalSample(4)]
    );
    assert!(query.voices[0].gates.is_empty());
    assert!(query.voices[1].policy.suppressed.is_empty());
    assert_eq!(
        query.voices[1].gates,
        vec![SignalSample(1)..SignalSample(3)]
    );
    assert_eq!(query.suppressed, vec![SignalSample(1)..SignalSample(3)]);
    assert!(matches!(
        query.voices[1].signal.spans[0].content,
        AudioSignalContent::Leaf(AudioContent::Source { .. })
    ));
    // The ungated source remains audible even when the other voice is both
    // intrinsically silent and explicitly gated over the complete support.
    let ungated = AudioSignalMix::new(
        &plan,
        vec![make_voice("left"), make_voice("source")],
        vec![AudioMixGate::new(support.clone(), vec![0])],
    )
    .unwrap();
    assert!(
        ungated
            .query(SignalSample(0)..SignalSample(4), Default::default())
            .unwrap()
            .suppressed
            .is_empty()
    );
    let shared = AudioSignalMix::new(
        &plan,
        vec![make_voice("source"), make_voice("source")],
        vec![AudioMixGate::new(support, vec![0, 1])],
    )
    .unwrap();
    let query = shared
        .query(SignalSample(0)..SignalSample(4), Default::default())
        .unwrap();
    assert_eq!(query.suppressed, vec![SignalSample(0)..SignalSample(4)]);
    assert!(
        query
            .voices
            .iter()
            .all(|voice| !voice.signal.spans.is_empty() && !voice.policy.contents.is_empty())
    );
}

#[test]
fn exact_ntsc_gates_use_common_point_clock_and_crop_without_restarting_phase() {
    let plan = make_plan(FrameRate::new(30_000, 1001).unwrap(), 4);
    let mix = AudioSignalMix::new(
        &plan,
        vec![voice(&plan, "source", ratio(0, 1)..ratio(4, 1))],
        vec![
            AudioMixGate::new(ratio(3, 2)..ratio(5, 2), vec![0]),
            // Positive exact interval with no grid point does not fabricate a
            // one-sample gate or affect the neighboring voice phase.
            AudioMixGate::new(ratio(1, 100_000)..ratio(2, 100_000), vec![0]),
        ],
    )
    .unwrap();
    assert_eq!(mix.sample_count().unwrap(), SignalSample(6407));
    let whole = mix
        .query(SignalSample(0)..SignalSample(6407), Default::default())
        .unwrap();
    assert_eq!(
        whole.suppressed,
        vec![SignalSample(2403)..SignalSample(4004)]
    );
    let crop = mix
        .query(SignalSample(2404)..SignalSample(2408), Default::default())
        .unwrap();
    assert_eq!(
        crop.suppressed,
        vec![SignalSample(2404)..SignalSample(2408)]
    );
    let full = &whole.voices[0].signal.spans[0];
    let cropped = &crop.voices[0].signal.spans[0];
    assert_eq!(full.sampling, cropped.sampling);
    assert_eq!(full.content, cropped.content);
    assert_eq!(full.allocated_samples, cropped.allocated_samples);
}

#[test]
fn mix_rejects_foreign_plan_clocks_gate_references_and_unbounded_inputs() {
    let plan = make_plan(FrameRate::new(48_000, 1).unwrap(), 4);
    let foreign = make_plan(FrameRate::new(48_000, 1).unwrap(), 4);
    let support = ratio(0, 1)..ratio(4, 1);
    let valid = || voice(&plan, "source", support.clone());
    assert!(AudioSignalMix::new(&plan, Vec::new(), Vec::new()).is_err());
    assert!(AudioSignalMix::new(&plan, vec![valid(); 65], Vec::new()).is_err());
    assert!(
        AudioSignalMix::new(
            &plan,
            vec![valid(), voice(&foreign, "source", support.clone())],
            Vec::new()
        )
        .is_err()
    );
    assert!(
        AudioSignalMix::new(
            &plan,
            vec![valid(), voice(&plan, "source", ratio(1, 1)..ratio(5, 1))],
            Vec::new()
        )
        .is_err()
    );
    for gate in [
        AudioMixGate::new(ratio(1, 1)..ratio(1, 1), vec![0]),
        AudioMixGate::new(ratio(-1, 1)..ratio(1, 1), vec![0]),
        AudioMixGate::new(ratio(1, 1)..ratio(5, 1), vec![0]),
        AudioMixGate::new(support.clone(), Vec::new()),
        AudioMixGate::new(support.clone(), vec![1]),
        AudioMixGate::new(support.clone(), vec![0, 0]),
    ] {
        assert!(AudioSignalMix::new(&plan, vec![valid()], vec![gate]).is_err());
    }
    assert!(
        AudioSignalMix::new(
            &plan,
            vec![valid()],
            vec![AudioMixGate::new(support.clone(), vec![0]); 4097]
        )
        .is_err()
    );
    assert!(
        AudioSignalMix::new(
            &plan,
            vec![valid(); 64],
            vec![AudioMixGate::new(support, (0..64).collect()); 1025]
        )
        .is_err()
    );
}

#[test]
fn voice_traversal_and_gate_composition_share_one_work_and_span_allowance() {
    let plan = make_plan(FrameRate::new(48_000, 1).unwrap(), 4);
    let support = ratio(0, 1)..ratio(4, 1);
    let mix = AudioSignalMix::new(
        &plan,
        vec![voice(&plan, "source", support.clone()); 3],
        vec![AudioMixGate::new(support, vec![0, 2])],
    )
    .unwrap();
    let range = SignalSample(0)..SignalSample(4);
    let complete = mix.query(range.clone(), limits(3, 65_536)).unwrap();
    assert_eq!(
        complete
            .voices
            .iter()
            .map(|voice| voice.signal.spans.len())
            .sum::<usize>(),
        3
    );
    assert!(matches!(
        mix.query(range.clone(), limits(2, 65_536)),
        Err(PlanError::AudioQueryLimit(_))
    ));
    assert!(matches!(
        mix.query(range.clone(), limits(3, complete.work - 1)),
        Err(PlanError::AudioQueryLimit(_))
    ));
    assert_eq!(
        mix.query(range, limits(3, complete.work)).unwrap().work,
        complete.work
    );
    assert!(
        mix.query(SignalSample(-1)..SignalSample(1), Default::default())
            .is_err()
    );
    assert!(
        mix.query(SignalSample(3)..SignalSample(5), Default::default())
            .is_err()
    );
}

#[test]
fn overlapping_gates_and_voice_silence_match_a_scalar_interval_oracle() {
    let plan = make_plan(FrameRate::new(48_000, 1).unwrap(), 4);
    let policies = [
        [true, false, false, true, false, false, false, true],
        [false, true, false, false, true, false, true, false],
        [false, false, true, false, false, true, false, false],
    ];
    let voices = policies
        .iter()
        .map(|policy| {
            AudioSignalTape::new(
                &plan,
                ratio(0, 1)..ratio(8, 1),
                policy
                    .iter()
                    .enumerate()
                    .map(|(sample, silent)| {
                        let sample = i128::try_from(sample).unwrap();
                        run(
                            ratio(sample, 1)..ratio(sample + 1, 1),
                            ratio(0, 1)..ratio(1, 1),
                            signal(&plan, if *silent { "left" } else { "source" }),
                        )
                    })
                    .collect(),
            )
            .unwrap()
        })
        .collect();
    let gates = [
        (ratio(1, 2)..ratio(7, 2), vec![0, 2]),
        (ratio(5, 2)..ratio(11, 2), vec![1]),
        (ratio(9, 2)..ratio(13, 2), vec![0, 1, 2]),
        (ratio(11, 2)..ratio(15, 2), vec![0]),
    ];
    let mix = AudioSignalMix::new(
        &plan,
        voices,
        gates
            .iter()
            .map(|(range, voices)| AudioMixGate::new(range.clone(), voices.clone()))
            .collect(),
    )
    .unwrap();
    let query = mix
        .query(SignalSample(0)..SignalSample(8), Default::default())
        .unwrap();
    for sample in 0..8 {
        let point = ratio(sample, 1);
        let expected = policies.iter().enumerate().all(|(voice, policy)| {
            policy[usize::try_from(sample).unwrap()]
                || gates.iter().any(|(range, voices)| {
                    voices.contains(&voice)
                        && point
                            .checked_sub(range.start)
                            .unwrap()
                            .compare_integer(0)
                            .is_ge()
                        && point
                            .checked_sub(range.end)
                            .unwrap()
                            .compare_integer(0)
                            .is_lt()
                })
        });
        let actual = query
            .suppressed
            .iter()
            .any(|range| range.contains(&SignalSample(sample as i64)));
        assert_eq!(actual, expected, "sample {sample}");
    }
}
