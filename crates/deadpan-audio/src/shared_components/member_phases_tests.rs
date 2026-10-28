use super::*;
use deadpan_core::*;
use std::sync::atomic::Ordering;

type Call = (Temporal, i64, usize, [bool; 2]);

fn readable(calls: &[Call]) -> Vec<(String, i64, usize, [bool; 2])> {
    calls
        .iter()
        .map(|(kind, at, voice, mask)| {
            let kind = match kind {
                Temporal::Equalizer(stage) => format!("eq:{stage}"),
                Temporal::Wet(effect) => format!("wet:{effect:?}"),
            };
            (kind, *at, *voice, *mask)
        })
        .collect()
}

fn work_control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work,
    }
}

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

/// Two samples, one effect owner, distinct route voices and both wet effects.
/// Unity-valued authored envelopes select dynamic execution without changing
/// the successful PCM oracle. The test does not open media or assert admission.
fn plan(dynamic: bool) -> RenderPlan {
    let root = ProjectDocument::new(
        ProjectId::new("member-phases").unwrap(),
        RevisionId::new("revision").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let envelopes = if dynamic {
        let range = GainRange::new(ExactRatio::ZERO, ExactRatio::integer(2)).unwrap();
        vec![
            GainEnvelope::new(
                GainClock::OwnerOutput,
                range,
                GainDb::UNITY,
                vec![GainSegment::new(range.end(), GainDb::UNITY, GainCurve::Step).unwrap()],
            )
            .unwrap(),
        ]
    } else {
        vec![]
    };
    let sends = TailSends::default()
        .with_level(TailEffect::Reverb, Some(GainDb::UNITY))
        .with_level(TailEffect::Delay, Some(GainDb::UNITY));
    let mut root_node = BeatNode::sequence("Root", vec![id("leaf")]);
    root_node.audio_treatments = AudioTreatments::with_equalizer_stages(
        vec![
            AudioTreatmentStage::ClipGain,
            AudioTreatmentStage::Equalizer,
        ],
        Some(ClipGain::new(GainDb::UNITY, false, envelopes, vec![]).unwrap()),
        None,
        None,
        Some(
            EqualizerRecipe::new(
                [GainDb::new(3000).unwrap(), GainDb::UNITY, GainDb::UNITY],
                None,
                None,
            )
            .unwrap(),
        ),
    )
    .unwrap()
    .with_wet_sends(sends);
    let leaf = BeatNode::hold(
        "Leaf",
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(2).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    );
    let mut wire = serde_json::to_value(root).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), root_node),
        (id("leaf"), leaf),
    ]))
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    RenderPlan::compile(&document).unwrap()
}

fn components<'plan>(
    plan: &'plan RenderPlan,
    gap: bool,
    skip: bool,
    stop: bool,
    value: [f64; 2],
    control: WorkControl<'_>,
) -> Components<'plan> {
    let query = plan
        .audio_gain_owners(AudioSample(0)..AudioSample(2), Default::default())
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
    let root = layers
        .iter()
        .find(|(clock, _)| clock.instance().node == id("root"))
        .unwrap();
    let stop = stop.then(|| OwnerKey::new(&root.0));
    let skip = if skip {
        vec![authored_gain::identity(&root.0)]
    } else {
        vec![]
    };
    let mut components = Components::new(AudioSample(0), 2, None, &skip);
    for voice in [5, 2] {
        components
            .route(
                (voice, Contribution::Sound(TailSends::default())),
                layers.clone(),
                0..2,
                stop.as_ref(),
                control,
            )
            .unwrap();
    }
    let _reserved = components.resident_frames().unwrap();
    components.allocate();
    for route in 0..2 {
        for at in 0..2 {
            if gap && route == 0 && at == 1 {
                continue;
            }
            components
                .add(route, at, (value, [0.0; 2]), 0, control)
                .unwrap();
        }
    }
    components
}

fn call(kind: Temporal, at: i64, voice: usize, mask: [bool; 2]) -> Call {
    (kind, at, voice, mask)
}

fn expected(dynamic: bool, gap: bool, stop: bool) -> Vec<Call> {
    use TailEffect::{Delay, Reverb};
    let eq = Temporal::Equalizer(1);
    let rev = Temporal::Wet(Reverb);
    let delay = Temporal::Wet(Delay);
    let both = [true, true];
    let last = if gap { [false, true] } else { both };
    // Literal order, independent of the phase/policy helper implementation.
    if dynamic {
        let mut calls = vec![call(eq, 0, 5, both), call(eq, 0, 2, both)];
        if !stop {
            calls.extend([
                call(rev, 0, 5, both),
                call(delay, 0, 5, both),
                call(rev, 0, 2, both),
                call(delay, 0, 2, both),
            ]);
        }
        if !gap {
            calls.push(call(eq, 1, 5, last));
        }
        calls.push(call(eq, 1, 2, last));
        if !stop {
            if !gap {
                calls.extend([call(rev, 1, 5, last), call(delay, 1, 5, last)]);
            }
            calls.extend([call(rev, 1, 2, last), call(delay, 1, 2, last)]);
        }
        calls
    } else {
        let mut calls = vec![call(eq, 0, 5, both), call(eq, 0, 2, both)];
        if !gap {
            calls.push(call(eq, 1, 5, last));
        }
        calls.push(call(eq, 1, 2, last));
        if !stop {
            calls.extend([call(rev, 0, 5, both), call(delay, 0, 5, both)]);
            if !gap {
                calls.extend([call(rev, 1, 5, last), call(delay, 1, 5, last)]);
            }
            calls.extend([
                call(rev, 0, 2, both),
                call(delay, 0, 2, both),
                call(rev, 1, 2, last),
                call(delay, 1, 2, last),
            ]);
        }
        calls
    }
}

fn read(
    calls: &mut Vec<Call>,
    clock: &AudioOwnerClock<'_>,
    kind: Temporal,
    at: AudioSample,
    mask: Permissions,
    voice: Option<usize>,
) -> Result<[f64; 2], StageAudioError> {
    assert_eq!(clock.instance().node, id("root"));
    calls.push(call(
        kind,
        at.0,
        voice.unwrap(),
        [mask.contains(5), mask.contains(2)],
    ));
    Ok(match kind {
        Temporal::Equalizer(_) => [0.2, -0.3],
        Temporal::Wet(TailEffect::Reverb) => [0.01, -0.02],
        Temporal::Wet(TailEffect::Delay) => [0.04, -0.05],
    })
}

#[test]
fn owner_driver_preserves_literal_block_and_sample_callback_orders_and_pcm() {
    for dynamic in [false, true] {
        for gap in [false, true] {
            for stop in [false, true] {
                let plan = plan(dynamic);
                let cancelled = AtomicBool::new(false);
                let work = RefCell::new(ReadWork::default());
                let control = work_control(&cancelled, &work);
                let components = components(&plan, gap, false, stop, [0.125, -0.25], control);
                let mut calls = Vec::new();
                let output = components
                    .finish(
                        Some(2),
                        Permissions::default(),
                        control,
                        |clock, kind, at, mask, voice| {
                            read(&mut calls, clock, kind, at, mask, voice)
                        },
                    )
                    .unwrap();
                assert_eq!(readable(&calls), readable(&expected(dynamic, gap, stop)));
                let value: [f64; 2] = if stop {
                    [0.2, -0.3]
                } else {
                    [0.2 + 0.01 + 0.04, -0.3 - 0.02 - 0.05]
                };
                assert_eq!(
                    output
                        .samples
                        .iter()
                        .map(|frame| frame.map(f64::to_bits))
                        .collect::<Vec<_>>(),
                    vec![value.map(f64::to_bits); 2],
                );
                assert!(output.suppressed.is_empty());
            }
        }
    }
}

#[test]
fn skipped_owner_has_no_dry_or_wet_callbacks() {
    for dynamic in [false, true] {
        let plan = plan(dynamic);
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = work_control(&cancelled, &work);
        let components = components(&plan, false, true, false, [0.125, -0.25], control);
        let output = components
            .finish(Some(2), Permissions::default(), control, |_, _, _, _, _| {
                panic!("skipped owner emitted a temporal callback")
            })
            .unwrap();
        assert_eq!(output.samples, vec![[0.125, -0.25]; 2]);
    }
}

#[test]
fn shared_sum_failure_precedes_equalizer_callbacks() {
    for dynamic in [false, true] {
        let plan = plan(dynamic);
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = work_control(&cancelled, &work);
        let components = components(&plan, false, false, false, [f64::MAX; 2], control);
        let result =
            components.finish(Some(2), Permissions::default(), control, |_, _, _, _, _| {
                panic!("nonfinite shared sum reached a temporal callback")
            });
        assert!(matches!(
            result,
            Err(StageAudioError::Preparation(
                PreparationError::InvalidSamples
            ))
        ));
    }
}

#[test]
fn failed_first_wet_application_stops_before_delay() {
    for dynamic in [false, true] {
        let plan = plan(dynamic);
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = work_control(&cancelled, &work);
        let components = components(&plan, false, false, false, [0.125; 2], control);
        let mut calls = Vec::new();
        let result = components.finish(
            Some(2),
            Permissions::default(),
            control,
            |clock, kind, at, mask, voice| {
                read(&mut calls, clock, kind, at, mask, voice)?;
                Ok([f64::MAX; 2])
            },
        );
        assert!(matches!(
            result,
            Err(StageAudioError::Preparation(
                PreparationError::InvalidSamples
            ))
        ));
        let prefix = if dynamic { 3 } else { 5 };
        assert_eq!(
            readable(&calls),
            readable(&expected(dynamic, false, false)[..prefix])
        );
        assert!(matches!(
            calls.last().unwrap().0,
            Temporal::Wet(TailEffect::Reverb)
        ));
    }
}

#[test]
fn callback_error_and_cancellation_emit_no_later_demand() {
    for dynamic in [false, true] {
        for cancel in [false, true] {
            let plan = plan(dynamic);
            let cancelled = AtomicBool::new(false);
            let work = RefCell::new(ReadWork::default());
            let control = work_control(&cancelled, &work);
            let components = components(&plan, false, false, false, [0.125; 2], control);
            let mut calls = Vec::new();
            let result = components.finish(
                Some(2),
                Permissions::default(),
                control,
                |clock, kind, at, mask, voice| {
                    let value = read(&mut calls, clock, kind, at, mask, voice)?;
                    if cancel {
                        cancelled.store(true, Ordering::Relaxed);
                        control.check()?;
                        Ok(value)
                    } else {
                        Err(StageAudioError::Range)
                    }
                },
            );
            assert!(result.is_err());
            assert_eq!(
                readable(&calls),
                readable(&expected(dynamic, false, false)[..1])
            );
        }
    }
}

#[test]
fn owner_driver_exact_remaining_work_succeeds_and_one_less_fails() {
    for dynamic in [false, true] {
        let plan = plan(dynamic);
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = work_control(&cancelled, &work);
        let input = components(&plan, false, false, false, [0.125; 2], control);
        let before = work.borrow().plan_work;
        input
            .finish(Some(2), Permissions::default(), control, |_, _, _, _, _| {
                Ok([0.1; 2])
            })
            .unwrap();
        let needed = work.borrow().plan_work - before;
        assert!(needed > 0);
        for shortage in [0, 1] {
            let work = RefCell::new(ReadWork::default());
            let control = work_control(&cancelled, &work);
            let input = components(&plan, false, false, false, [0.125; 2], control);
            work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - needed + shortage;
            let result = input.finish(Some(2), Permissions::default(), control, |_, _, _, _, _| {
                Ok([0.1; 2])
            });
            if shortage == 0 {
                assert!(result.is_ok());
                assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
            } else {
                assert!(matches!(
                    result,
                    Err(StageAudioError::Limit("plan work per read"))
                ));
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn record_demand(request: visitors::TemporalDemand<'_, '_>) -> Call {
    assert_eq!(request.clock.instance().node, id("root"));
    call(
        request.kind,
        request.sample.0,
        request.voice.unwrap(),
        [
            request.permissions.contains(5),
            request.permissions.contains(2),
        ],
    )
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn authenticated_cold_driver_matches_literal_block_and_sample_orders() {
    for dynamic in [false, true] {
        for gap in [false, true] {
            for skip in [false, true] {
                for stop in [false, true] {
                    // Parent fixture uses actual policy_prediction::populate.
                    // This module never creates completion from numerical bits.
                    super::super::completion::tests::with_completed_fixture(
                        dynamic,
                        gap,
                        skip,
                        stop,
                        |policy, control| {
                            let mut calls = Vec::new();
                            policy
                                .visit_demands(control, |request| {
                                    calls.push(record_demand(request));
                                    Ok(())
                                })
                                .unwrap();
                            let expected = if skip {
                                vec![]
                            } else {
                                expected(dynamic, gap, stop)
                            };
                            assert_eq!(readable(&calls), readable(&expected));
                        },
                    );
                }
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn authenticated_cold_callback_errors_stop_at_exact_prefix() {
    for dynamic in [false, true] {
        super::super::completion::tests::with_completed_fixture(
            dynamic,
            false,
            false,
            false,
            |policy, control| {
                let mut calls = Vec::new();
                let result = policy.visit_demands(control, |request| {
                    calls.push(record_demand(request));
                    Err(StageAudioError::Range)
                });
                assert!(matches!(result, Err(StageAudioError::Range)));
                assert_eq!(
                    readable(&calls),
                    readable(&expected(dynamic, false, false)[..1]),
                );
            },
        );
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn authenticated_cold_cancellation_stops_at_exact_prefix() {
    for dynamic in [false, true] {
        super::super::completion::tests::with_completed_fixture(
            dynamic,
            false,
            false,
            false,
            |policy, control| {
                let mut calls = Vec::new();
                let result = policy.visit_demands(control, |request| {
                    calls.push(record_demand(request));
                    control.cancelled.store(true, Ordering::Relaxed);
                    control.check()?;
                    Ok(())
                });
                assert!(result.is_err());
                assert_eq!(
                    readable(&calls),
                    readable(&expected(dynamic, false, false)[..1]),
                );
            },
        );
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn authenticated_cold_exact_remaining_work_and_partial_failure_are_billed() {
    for dynamic in [false, true] {
        super::super::completion::tests::with_completed_fixture(
            dynamic,
            false,
            false,
            false,
            |policy, control| {
                let before = control.work.borrow().plan_work;
                policy.visit_demands(control, |_| Ok(())).unwrap();
                let needed = control.work.borrow().plan_work - before;
                assert!(needed > 0);
                for shortage in [0, 1] {
                    control.work.borrow_mut().plan_work =
                        MAX_PLAN_WORK_PER_READ - needed + shortage;
                    let mut calls = Vec::new();
                    let result = policy.visit_demands(control, |request| {
                        calls.push(record_demand(request));
                        Ok(())
                    });
                    if shortage == 0 {
                        assert!(result.is_ok());
                        assert_eq!(control.work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
                        assert_eq!(readable(&calls), readable(&expected(dynamic, false, false)));
                    } else {
                        assert!(matches!(
                            result,
                            Err(StageAudioError::Limit("plan work per read"))
                        ));
                        let spent = control.work.borrow().plan_work;
                        assert!(spent > MAX_PLAN_WORK_PER_READ - needed + shortage);
                        assert!(spent <= MAX_PLAN_WORK_PER_READ);
                        let full = expected(dynamic, false, false);
                        assert_eq!(readable(&calls), readable(&full[..calls.len()]));
                    }
                }
            },
        );
    }
}
