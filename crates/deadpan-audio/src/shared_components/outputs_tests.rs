use super::*;
use crate::stages::shared_components::program::tests::{plan, render_selection};
use deadpan_core::{TailEffect, TailSends};

type Calls = Vec<(String, u8, usize, i64, Option<usize>, [bool; 2])>;

fn work_control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work,
    }
}

fn selection(voice: Option<usize>, alias: Option<usize>) -> OutputSelection {
    let mut aliases = Permissions::default();
    if let Some(alias) = alias {
        aliases.insert(alias).unwrap();
    }
    OutputSelection { voice, aliases }
}

fn callback(
    calls: &mut Calls,
    clock: &AudioOwnerClock<'_>,
    temporal: Temporal,
    sample: AudioSample,
    permissions: Permissions,
    voice: Option<usize>,
) -> Result<[f64; 2], StageAudioError> {
    calls.push((
        clock.instance().node.as_str().into(),
        match temporal {
            Temporal::Equalizer(_) => 0,
            Temporal::Wet(_) => 1,
        },
        match temporal {
            Temporal::Equalizer(stage) => stage,
            Temporal::Wet(TailEffect::Reverb) => 0,
            Temporal::Wet(TailEffect::Delay) => 1,
        },
        sample.0,
        voice,
        [permissions.contains(0), permissions.contains(1)],
    ));
    let base = if temporal == Temporal::Wet(TailEffect::Delay) {
        0.031
    } else {
        -0.127
    };
    let value = base + voice.unwrap_or(0) as f64 * 0.013 + sample.0 as f64 * 0.001;
    Ok([value, -value])
}

/// Same admitted four samples and concrete clocks as the existing program
/// oracle, without its separate preparation-inspection work. No media provider
/// is involved; source admission is outside this pure Components fixture.
fn make_components<'plan>(
    plan: &'plan RenderPlan,
    send: bool,
    skipped: bool,
    stopped: bool,
    control: WorkControl<'_>,
) -> Components<'plan> {
    let query = plan
        .audio_gain_owners(AudioSample(12)..AudioSample(16), Default::default())
        .unwrap();
    assert_eq!(query.spans().len(), 1);
    let layers: Vec<_> = query.spans()[0]
        .owners()
        .iter()
        .cloned()
        .map(|clock| {
            let until = clock.treatments().map_or(0, |recipe| recipe.order().len());
            (clock, until)
        })
        .collect();
    let root = &layers[0].0;
    let inner = layers
        .iter()
        .find(|(clock, _)| clock.instance().node.as_str() == "inner")
        .unwrap();
    let stop = stopped.then(|| OwnerKey::new(&inner.0));
    let skip = if skipped {
        vec![authored_gain::identity(root)]
    } else {
        vec![]
    };
    let mut components =
        Components::new(AudioSample(12), 4, send.then_some(TailEffect::Delay), &skip);
    let original = components
        .route(
            (0, Contribution::Original),
            layers.clone(),
            0..4,
            stop.as_ref(),
            control,
        )
        .unwrap();
    let sound = components
        .route(
            (
                1,
                Contribution::Sound(TailSends::default().with_level(
                    TailEffect::Delay,
                    Some(deadpan_core::GainDb::new(-6000).unwrap()),
                )),
            ),
            layers,
            0..4,
            stop.as_ref(),
            control,
        )
        .unwrap();
    components.allocate();
    for at in 0..4 {
        if at != 1 {
            components
                .add(original, at, ([0.17, -0.23], [0.01, -0.02]), 0, control)
                .unwrap();
        }
        if at != 3 {
            components
                .add(sound, at, ([-0.31, 0.19], [0.02, 0.03]), 1500, control)
                .unwrap();
        }
    }
    components
}

fn bits(samples: &[[f64; 2]]) -> Vec<[u64; 2]> {
    samples
        .iter()
        .map(|sample| sample.map(f64::to_bits))
        .collect()
}

#[test]
fn borrowed_layouts_preserve_channel_addition_and_selection() {
    let selected = selection(Some(1), Some(0));
    assert!(selected.selects(0) && selected.selects(1));
    assert!(!selected.selects(2));
    assert!(selection(None, None).selects(17));
    let values = [[1.0e16, -1.0e16], [1.0, -1.0], [-1.0e16, 1.0e16]];
    let mut interleaved = [[-0.0, 0.0]; 2];
    let mut left = [-0.0; 2];
    let mut right = [0.0; 2];
    let mut a = OutputSamples::Interleaved(&mut interleaved);
    let mut b = OutputSamples::from((&mut left[..], &mut right[..]));
    assert_eq!(a.frames(), Some(2));
    assert_eq!(b.frames(), Some(2));
    a.clear();
    b.clear();
    for value in values {
        a.add(0, value);
        b.add(0, value);
    }
    a.set(1, [-0.0, 0.125]);
    b.set(1, [-0.0, 0.125]);
    assert_eq!(a.frame(0).map(f64::to_bits), b.frame(0).map(f64::to_bits));
    assert_eq!(a.frame(0), [0.0; 2]);
    assert_eq!(a.frame(1).map(f64::to_bits), b.frame(1).map(f64::to_bits));
    assert_eq!(a.frame(1)[0].to_bits(), (-0.0f64).to_bits());
}

#[test]
fn multiple_outputs_match_independent_dynamic_oracle_for_aliases_sends_skip_and_stop() {
    let selections = [
        selection(Some(1), None),
        selection(Some(0), Some(1)),
        selection(None, None),
        selection(Some(17), None),
    ];
    for batch_dynamic in [false, true] {
        for gain_first in [false, true] {
            for muted in [false, true] {
                for send in [false, true] {
                    for (skip, stop) in [(false, false), (true, false), (false, true)] {
                        let constant = plan(batch_dynamic, muted, gain_first);
                        let dynamic = plan(true, muted, gain_first);
                        let cancelled = AtomicBool::new(false);
                        let work = RefCell::new(ReadWork::default());
                        let control = work_control(&cancelled, &work);
                        let components = make_components(&constant, send, skip, stop, control);
                        let mut first = [[f64::NAN; 2]; 4];
                        let mut left = [f64::NAN; 4];
                        let mut right = [f64::NAN; 4];
                        let mut all = [[f64::NAN; 2]; 4];
                        let mut absent = [[f64::NAN; 2]; 4];
                        let mut suppressed: [Vec<Range<AudioSample>>; 4] =
                            std::array::from_fn(|_| Vec::with_capacity(4));
                        let capacities = suppressed.each_ref().map(|ranges| ranges.capacity());
                        let [one, two, three, four] = &mut suppressed;
                        let mut outputs = [
                            Output {
                                selection: selections[0],
                                samples: OutputSamples::Interleaved(&mut first),
                                suppressed: Some(one),
                            },
                            Output {
                                selection: selections[1],
                                samples: OutputSamples::Planar {
                                    left: &mut left,
                                    right: &mut right,
                                },
                                suppressed: Some(two),
                            },
                            Output {
                                selection: selections[2],
                                samples: OutputSamples::Interleaved(&mut all),
                                suppressed: Some(three),
                            },
                            Output {
                                selection: selections[3],
                                samples: OutputSamples::Interleaved(&mut absent),
                                suppressed: Some(four),
                            },
                        ];
                        let mut calls = Vec::new();
                        components
                            .finish_into(
                                &mut outputs,
                                control,
                                |clock, temporal, sample, permissions, voice| {
                                    callback(
                                        &mut calls,
                                        clock,
                                        temporal,
                                        sample,
                                        permissions,
                                        voice,
                                    )
                                },
                            )
                            .unwrap();
                        let planar: Vec<_> =
                            left.into_iter().zip(right).map(|(l, r)| [l, r]).collect();
                        let actual = [&first[..], &planar[..], &all[..], &absent[..]];
                        let mut expected_calls = None;
                        for (index, target) in selections.iter().enumerate() {
                            let (expected, reference_calls, _) = render_selection(
                                &dynamic,
                                send,
                                skip,
                                stop,
                                target.voice,
                                target.aliases,
                            );
                            assert_eq!(
                                bits(actual[index]),
                                bits(&expected.samples),
                                "target={index} send={send} skip={skip} stop={stop} muted={muted} gain_first={gain_first} dynamic={batch_dynamic}"
                            );
                            assert_eq!(
                                merged_suppression(suppressed[index].clone()),
                                expected.suppressed
                            );
                            assert_eq!(suppressed[index].capacity(), capacities[index]);
                            if let Some(expected) = &expected_calls {
                                assert_eq!(&reference_calls, expected);
                            } else {
                                expected_calls = Some(reference_calls);
                            }
                        }
                        calls.sort_by_key(|call| call.3);
                        assert_eq!(calls, expected_calls.unwrap());
                    }
                }
            }
        }
    }
}

#[test]
fn multiple_outputs_traverse_once_and_spend_less_than_independent_constant_calls() {
    let plan = plan(false, false, true);
    let targets = [selection(Some(1), None), selection(Some(0), None)];
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = work_control(&cancelled, &work);
    let components = make_components(&plan, false, false, false, control);
    let mut a = [[0.0; 2]; 4];
    let mut b = [[0.0; 2]; 4];
    let mut outputs = [
        Output {
            selection: targets[0],
            samples: OutputSamples::Interleaved(&mut a),
            suppressed: None,
        },
        Output {
            selection: targets[1],
            samples: OutputSamples::Interleaved(&mut b),
            suppressed: None,
        },
    ];
    let mut calls = Vec::new();
    components
        .finish_into(
            &mut outputs,
            control,
            |clock, temporal, sample, permissions, voice| {
                callback(&mut calls, clock, temporal, sample, permissions, voice)
            },
        )
        .unwrap();
    let batch_work = work.borrow().plan_work;
    let mut scalar_work = 0;
    for target in targets {
        let work = RefCell::new(ReadWork::default());
        let control = work_control(&cancelled, &work);
        let components = make_components(&plan, false, false, false, control);
        let mut scalar_calls = Vec::new();
        let result = components
            .finish(
                target.voice,
                target.aliases,
                control,
                |clock, temporal, sample, permissions, voice| {
                    callback(
                        &mut scalar_calls,
                        clock,
                        temporal,
                        sample,
                        permissions,
                        voice,
                    )
                },
            )
            .unwrap();
        assert_eq!(
            scalar_calls, calls,
            "the batch must execute exactly one common callback sequence"
        );
        assert_eq!(
            bits(&result.samples),
            bits(if target.voice == Some(1) { &a } else { &b })
        );
        scalar_work += work.borrow().plan_work;
    }
    assert!(
        batch_work < scalar_work,
        "batch={batch_work} scalar={scalar_work}"
    );
}

#[test]
fn output_validation_precedes_callbacks_and_all_buffer_mutation() {
    let plan = plan(false, false, true);
    let cancelled = AtomicBool::new(false);
    for invalid in 0..4 {
        let work = RefCell::new(ReadWork::default());
        let control = work_control(&cancelled, &work);
        let components = make_components(&plan, false, false, false, control);
        let mut first = [[23.0, -19.0]; 4];
        let mut second = [[11.0, -7.0]; 3];
        let mut left = [31.0; 4];
        let mut right = [37.0; 3];
        let mut valid = [[41.0; 2]; 4];
        let mut suppressed = if invalid == 2 {
            vec![AudioSample(0)..AudioSample(1)]
        } else {
            Vec::with_capacity(if invalid == 3 { 3 } else { 4 })
        };
        let before_ranges = suppressed.clone();
        let mut outputs = [
            Output {
                selection: selection(Some(0), None),
                samples: OutputSamples::Interleaved(&mut first),
                suppressed: None,
            },
            Output {
                selection: selection(Some(1), None),
                samples: match invalid {
                    0 => OutputSamples::Interleaved(&mut second),
                    1 => OutputSamples::Planar {
                        left: &mut left,
                        right: &mut right,
                    },
                    _ => OutputSamples::Interleaved(&mut valid),
                },
                suppressed: Some(&mut suppressed),
            },
        ];
        let mut calls = 0;
        let result = components.finish_into(&mut outputs, control, |_, _, _, _, _| {
            calls += 1;
            Ok([0.0; 2])
        });
        assert!(result.is_err(), "invalid={invalid}");
        assert_eq!(calls, 0);
        assert_eq!(first, [[23.0, -19.0]; 4]);
        assert_eq!(second, [[11.0, -7.0]; 3]);
        assert_eq!(left, [31.0; 4]);
        assert_eq!(right, [37.0; 3]);
        assert_eq!(valid, [[41.0; 2]; 4]);
        assert_eq!(suppressed, before_ranges);
    }
}

fn final_gain_components(control: WorkControl<'_>) -> Components<'static> {
    let mut components = Components::new(AudioSample(0), 1, None, &[]);
    let good = components
        .route((0, Contribution::Original), vec![], 0..1, None, control)
        .unwrap();
    let bad = components
        .route(
            (1, Contribution::Sound(Default::default())),
            vec![],
            0..1,
            None,
            control,
        )
        .unwrap();
    components.allocate();
    components
        .add(good, 0, ([0.125, -0.375], [0.0; 2]), 0, control)
        .unwrap();
    components
        .add(bad, 0, ([0.5; 2], [0.0; 2]), i32::MAX, control)
        .unwrap();
    components
}

#[test]
fn unrequested_final_gain_failure_stays_unconsumed_but_aliases_consume_it() {
    let cancelled = AtomicBool::new(false);
    for required_bad in [false, true] {
        let work = RefCell::new(ReadWork::default());
        let control = work_control(&cancelled, &work);
        let components = final_gain_components(control);
        let mut a = [[0.0; 2]; 1];
        let mut b = [[0.0; 2]; 1];
        let mut outputs = [
            Output {
                selection: selection(Some(0), None),
                samples: OutputSamples::Interleaved(&mut a),
                suppressed: None,
            },
            Output {
                selection: selection(Some(17), required_bad.then_some(1)),
                samples: OutputSamples::Interleaved(&mut b),
                suppressed: None,
            },
        ];
        let result = components.finish_into(&mut outputs, control, |_, _, _, _, _| {
            panic!("memoryless fixture")
        });
        if required_bad {
            assert!(matches!(
                result,
                Err(StageAudioError::Preparation(
                    PreparationError::InvalidSamples
                ))
            ));
        } else {
            result.unwrap();
            assert_eq!(a, [[0.125, -0.375]]);
            assert_eq!(b, [[0.0; 2]]);
        }
    }
}

#[test]
fn multi_output_callback_cancellation_and_exact_work_failures_propagate() {
    let plan = plan(false, false, true);
    let cancelled = AtomicBool::new(false);
    let render = |allowance: Option<usize>, cancel: bool, fail_callback: bool| {
        let work = RefCell::new(ReadWork::default());
        let control = work_control(&cancelled, &work);
        let components = make_components(&plan, false, false, false, control);
        let construction = work.borrow().plan_work;
        if let Some(allowance) = allowance {
            work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - allowance;
        }
        cancelled.store(cancel, std::sync::atomic::Ordering::Relaxed);
        let mut a = [[0.0; 2]; 4];
        let mut b = [[0.0; 2]; 4];
        let mut outputs = [
            Output {
                selection: selection(Some(0), None),
                samples: OutputSamples::Interleaved(&mut a),
                suppressed: None,
            },
            Output {
                selection: selection(Some(1), None),
                samples: OutputSamples::Interleaved(&mut b),
                suppressed: None,
            },
        ];
        let mut calls = Vec::new();
        let result = components.finish_into(
            &mut outputs,
            control,
            |clock, temporal, sample, permissions, voice| {
                if fail_callback {
                    return Err(StageAudioError::Unsupported("output callback fixture"));
                }
                callback(&mut calls, clock, temporal, sample, permissions, voice)
            },
        );
        cancelled.store(false, std::sync::atomic::Ordering::Relaxed);
        let spent = work.borrow().plan_work;
        (
            result,
            if allowance.is_none() {
                spent - construction
            } else {
                spent
            },
        )
    };
    assert!(matches!(
        render(None, false, true).0,
        Err(StageAudioError::Unsupported("output callback fixture"))
    ));
    assert!(matches!(
        render(None, true, false).0,
        Err(StageAudioError::Preparation(PreparationError::Cancelled))
    ));
    let (result, needed) = render(None, false, false);
    result.unwrap();
    assert!(needed > 0);
    let (result, spent) = render(Some(needed), false, false);
    result.unwrap();
    assert_eq!(spent, MAX_PLAN_WORK_PER_READ);
    assert!(matches!(
        render(Some(needed - 1), false, false).0,
        Err(StageAudioError::Limit("plan work per read"))
    ));
}
