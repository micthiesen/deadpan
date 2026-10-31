//! Actual WAV callbacks and local raw constructor failures, with real input owners.
use super::*;
use crate::stages::raw_domain_query_trace;

fn raw() -> Value {
    serde_json::to_value(raw_domain_query_trace::snapshot()).unwrap()
}
fn reset_raw() {
    original_request::flattened_work_trace::reset();
    retained_request::trace::reset();
    raw_domain_query_trace::reset();
}
fn n(value: &Value, field: &str) -> u64 {
    value[field].as_u64().unwrap()
}
fn valid(value: &Value) {
    for flag in ["invalid", "overflow", "nested"] {
        assert_eq!(value[flag], false);
    }
    assert_eq!(value["active_depth"], 0);
    for lane in ["plan", "cache"] {
        let v = &value[lane];
        assert_eq!(
            n(v, "constructor_calls"),
            n(v, "completed_queries") + n(v, "ordinary_calls") + n(v, "failed_queries")
        );
        assert_eq!(n(v, "admitted_calls"), n(v, "completed_queries"));
        assert_eq!(n(v, "canonical_units"), 0);
        assert_eq!(
            n(v, "accepted_work"),
            n(v, "additional_units") + n(v, "partial_work") + n(v, "host_eviction_work")
        );
    }
    assert_eq!(n(value, "live_results"), 0);
    assert_eq!(n(value, "result_creations"), n(value, "result_drops"));
}
struct Watch<'a> {
    inner: ObservedProvider<'a>,
    raw_events: Vec<Value>,
    disable_trace_at_source: bool,
}
impl AudioSourceProvider for Watch<'_> {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        self.raw_events.push(json!({"raw":raw(),"sound_context_frames":self.inner.work.borrow().sound_context_frames}));
        if self.disable_trace_at_source {
            raw_domain_query_trace::disable();
        }
        self.inner.source(project, revision, asset, cancelled)
    }
}
fn with_retained<R>(
    fixture: &cases::Fixture,
    shared: bool,
    fault: Fault,
    action: impl FnOnce(
        &mut StageAudio,
        &AudioOwnerRetainedInput<'_>,
        WorkControl<'_>,
        &mut Watch<'_>,
    ) -> R,
) -> R {
    let plan = Arc::new(RenderPlan::compile(&fixture.current).unwrap());
    let owner = owner_at(&plan, fixture.owner, AudioSample(fixture.observed_at));
    let input = plan
        .audio_owner_input(&owner, AudioSample(fixture.observed_at), Default::default())
        .unwrap();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut provider = Watch {
        inner: ObservedProvider {
            prepared: cases::prepared(fixture.wav),
            calls: 0,
            events: Vec::new(),
            work: &work,
            fault,
        },
        raw_events: Vec::new(),
        disable_trace_at_source: false,
    };
    let result = if shared {
        control.spend_plan_work(input.work()).unwrap();
        let context = {
            let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
            authored_eq::Context::finish_paid_owner_input_admitted(
                &input,
                &owner,
                AudioSample(fixture.observed_at),
                control,
                &mut admission,
            )
        }
        .unwrap();
        assert!(work.borrow().sound_context_frames > 0);
        action(
            &mut renderer,
            context.retained().unwrap(),
            control,
            &mut provider,
        )
    } else {
        let probe = input
            .retained_probe_at(AudioSample(fixture.observed_at), Default::default())
            .unwrap();
        action(
            &mut renderer,
            probe.input().unwrap(),
            control,
            &mut provider,
        )
    };
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(renderer.active_frames, 0);
    assert_eq!(renderer.active_disk_bytes, 0);
    raw_domain_query_trace::disable();
    result
}
fn observed(
    fixture: &cases::Fixture,
    shared: bool,
    authored: bool,
    fault: Fault,
    legacy: bool,
) -> Value {
    with_retained(
        fixture,
        shared,
        fault,
        |renderer, retained, control, provider| {
            let (start, frames) = fixture.windows[1];
            let caller = control.work.borrow().sound_context_frames;
            let literal = retained.work();
            reset_raw();
            let work_before = work_snapshot(control.work);
            let mut run = || {
                renderer.read_retained_controlled(
                    provider,
                    retained,
                    AudioSample(start),
                    frames,
                    control,
                    (0, authored),
                )
            };
            let result = if legacy {
                raw_domain_request::with_legacy_raw_domain(run)
            } else {
                run()
            };
            let output = match &result {
                Ok(block) => json!({"status":"Ok","block":read_block(block)}),
                Err(error) => json!({"status":"Error","error":super::error(error)}),
            };
            let trace = raw();
            valid(&trace);
            assert_eq!(
                control.work.borrow().sound_context_frames,
                caller,
                "raw/fade/owner guards end before returned PCM drops"
            );
            assert_eq!(retained.work(), literal);
            assert!(provider.inner.calls > 0);
            assert_eq!(provider.raw_events.len(), provider.inner.calls);
            drop(result);
            assert_eq!(control.work.borrow().sound_context_frames, caller);
            json!({"output":output,"raw":trace,"events":provider.raw_events,"caller_frames":caller,
                "work_before":work_before,"work_after":work_snapshot(control.work)})
        },
    )
}
#[test]
fn actual_root_and_fractional_point_raw_carriers_survive_nested_wav_success_error_and_cancel() {
    let fixtures = cases::fixtures();
    for (index, fixture) in fixtures[..2].iter().enumerate() {
        for shared in [false, true] {
            for authored in [false, true] {
                for fault in [Fault::None, Fault::ProviderFailure, Fault::ProviderCancel] {
                    let old = observed(fixture, shared, authored, fault, true);
                    let new = observed(fixture, shared, authored, fault, false);
                    assert_eq!(
                        new["output"], old["output"],
                        "complete actual raw legacy output/typed source outcome"
                    );
                    for (lane, counter) in [("plan", "plan_work"), ("cache", "cache_plan_work")] {
                        assert_eq!(new["work_before"][counter], old["work_before"][counter]);
                        assert_eq!(new["raw"][lane]["partial_work"], 0);
                        assert_eq!(new["raw"][lane]["host_eviction_work"], 0);
                        let new_work =
                            n(&new["work_after"], counter) - n(&new["work_before"], counter);
                        let old_work =
                            n(&old["work_after"], counter) - n(&old["work_before"], counter);
                        assert_eq!(
                            i128::from(new_work) - i128::from(old_work),
                            i128::from(n(&new["raw"][lane], "additional_units")),
                            "successful raw constructor fees remain accepted once through later source failure/cancellation"
                        );
                    }
                    let events = new["events"].as_array().unwrap();
                    let old_events = old["events"].as_array().unwrap();
                    assert_eq!(events.len(), old_events.len());
                    assert!(n(&new["raw"], "result_creations") > 0);
                    assert!(
                        n(
                            &new["raw"],
                            if index == 0 {
                                "nested_root_callbacks"
                            } else {
                                "nested_point_callbacks"
                            }
                        ) > 0
                    );
                    for (new, old) in events.iter().zip(old_events) {
                        assert!(n(&new["raw"], "live_results") > 0);
                        assert_eq!(old["raw"]["live_results"], 0);
                        assert!(
                            n(new, "sound_context_frames") > n(old, "sound_context_frames"),
                            "actual raw lease is additional to the same caller/fade/owner metadata"
                        );
                    }
                    let output = &new["output"];
                    match fault {
                        Fault::None => assert_eq!(output["status"], "Ok"),
                        Fault::ProviderFailure => assert_eq!(
                            output["error"]["debug"],
                            "Preparation(SourceUnavailable(\"retained OLD provider sentinel\"))"
                        ),
                        Fault::ProviderCancel => {
                            assert_eq!(output["error"]["debug"], "Preparation(Cancelled)");
                            assert_eq!(output["error"]["is_cancelled"], true);
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    }
}

#[test]
fn raw_local_work_refusal_retains_exact_accepted_prefix_before_provider_in_both_lanes() {
    let fixtures = cases::fixtures();
    for fixture in &fixtures[..2] {
        for cache in [false, true] {
            let fees = with_retained(
                fixture,
                true,
                Fault::None,
                |renderer, retained, control, provider| {
                    reset_raw();
                    let scope = cache.then(|| control.cache_key_scope());
                    let at = retained
                        .reference_at(AudioSample(fixture.windows[1].0))
                        .unwrap();
                    let result = renderer
                        .read_bound_at(retained.bound(), provider, at, (16, 0), control)
                        .unwrap();
                    let trace = raw();
                    valid(&trace);
                    let lane = if cache { "cache" } else { "plan" };
                    assert_eq!(trace[lane]["completed_queries"], 1);
                    assert_eq!(trace[lane]["partial_work"], 0);
                    let fees = n(&trace[lane], "additional_units") as usize;
                    drop(result);
                    drop(scope);
                    fees
                },
            );
            assert!(fees > 1);
            for remaining in [0, fees - 1] {
                with_retained(
                    fixture,
                    true,
                    Fault::None,
                    |renderer, retained, control, provider| {
                        let caller = control.work.borrow().sound_context_frames;
                        let prior = MAX_PLAN_WORK_PER_READ - remaining;
                        {
                            let mut work = control.work.borrow_mut();
                            let value = if cache {
                                &mut work.cache_plan_work
                            } else {
                                &mut work.plan_work
                            };
                            assert!(*value <= prior);
                            *value = prior;
                        }
                        let before = [
                            control.work.borrow().plan_work,
                            control.work.borrow().cache_plan_work,
                        ];
                        reset_raw();
                        let scope = cache.then(|| control.cache_key_scope());
                        let at = retained
                            .reference_at(AudioSample(fixture.windows[1].0))
                            .unwrap();
                        let result = renderer.read_bound_at(
                            retained.bound(),
                            provider,
                            at,
                            (16, 0),
                            control,
                        );
                        let label = if cache {
                            CACHE_KEY_WORK_LIMIT
                        } else {
                            "plan work per read"
                        };
                        assert!(
                            matches!(&result,Err(StageAudioError::Limit(actual)) if *actual==label)
                        );
                        drop(result);
                        drop(scope);
                        let trace = raw();
                        valid(&trace);
                        let lane = if cache { "cache" } else { "plan" };
                        let other = if cache { "plan" } else { "cache" };
                        assert_eq!(trace[lane]["constructor_calls"], 1);
                        assert_eq!(trace[lane]["failed_queries"], 1);
                        assert_eq!(trace[lane]["completed_queries"], 0);
                        assert_eq!(trace[lane]["additional_units"], 0);
                        assert_eq!(trace[lane]["partial_work"], remaining);
                        assert_eq!(trace[lane]["accepted_work"], remaining);
                        assert_eq!(trace[lane]["host_eviction_work"], 0);
                        assert_eq!(trace[other]["accepted_work"], 0);
                        assert_eq!(trace["result_creations"], 0);
                        assert_eq!(provider.inner.calls, 0);
                        let after = [
                            control.work.borrow().plan_work,
                            control.work.borrow().cache_plan_work,
                        ];
                        let selected = usize::from(cache);
                        assert_eq!(after[selected], before[selected] + remaining);
                        assert_eq!(after[1 - selected], before[1 - selected]);
                        assert_eq!(control.work.borrow().sound_context_frames, caller);
                    },
                );
            }
        }
    }
}

#[test]
fn actual_result_drop_is_tracked_when_source_disables_the_trace_while_it_is_live() {
    let fixtures = cases::fixtures();
    with_retained(
        &fixtures[0],
        true,
        Fault::None,
        |renderer, retained, control, provider| {
            let caller = control.work.borrow().sound_context_frames;
            provider.disable_trace_at_source = true;
            reset_raw();
            let at = retained
                .reference_at(AudioSample(fixtures[0].windows[1].0))
                .unwrap();
            let result = renderer
                .read_bound_at(retained.bound(), provider, at, (16, 0), control)
                .unwrap();
            assert!(provider.inner.calls > 0);
            assert!(
                provider
                    .raw_events
                    .iter()
                    .all(|event| n(&event["raw"], "live_results") > 0)
            );
            let trace = raw();
            valid(&trace);
            assert_eq!(trace["result_creations"], 1);
            assert_eq!(trace["result_drops"], 1);
            assert_eq!(control.work.borrow().sound_context_frames, caller);
            drop(result);
            raw_domain_query_trace::reset();
            valid(&raw());
        },
    );
}

#[test]
fn raw_bound_foreign_depth_range_and_cancel_priorities_precede_construction_and_io() {
    let fixtures = cases::fixtures();
    let fixture = &fixtures[0];
    with_retained(
        fixture,
        true,
        Fault::None,
        |renderer, retained, control, provider| {
            let bound = retained.bound();
            let at = retained
                .reference_at(AudioSample(fixture.windows[1].0))
                .unwrap();
            let foreign = Arc::new(RenderPlan::compile(&fixture.current).unwrap());
            let mut foreign = StageAudio::new(foreign);
            for (foreign_plan, depth, frames, expected) in [
                (false, renderer.limits.maximum_depth + 1, 0, "depth"),
                (true, foreign.limits.maximum_depth + 1, 0, "depth"),
                (true, 0, 0, "foreign"),
                (false, 0, 0, "range"),
                (false, 0, MAX_OUTPUT_FRAMES + 1, "range"),
            ] {
                reset_raw();
                let before = work_snapshot(control.work);
                let target = if foreign_plan {
                    &mut foreign
                } else {
                    &mut *renderer
                };
                let result = target.read_bound_at(bound, provider, at, (frames, depth), control);
                match expected {
                    "depth" => assert!(matches!(
                        result,
                        Err(StageAudioError::Limit("nested stage depth"))
                    )),
                    "foreign" => assert!(matches!(result, Err(StageAudioError::ForeignDomain))),
                    "range" => assert!(matches!(result, Err(StageAudioError::Range))),
                    _ => unreachable!(),
                }
                assert_eq!(target.active_frames, 0);
                assert_eq!(target.active_disk_bytes, 0);
                assert_eq!(work_snapshot(control.work), before);
                let trace = raw();
                valid(&trace);
                for lane in ["plan", "cache"] {
                    assert_eq!(trace[lane]["constructor_calls"], 0);
                }
                assert_eq!(provider.inner.calls, 0);
                assert_eq!(foreign.active_frames, 0);
                assert_eq!(foreign.active_disk_bytes, 0);
            }
            reset_raw();
            let before = work_snapshot(control.work);
            assert!(matches!(
                renderer.read_bound(
                    bound,
                    provider,
                    BoundRead {
                        offset: -1,
                        frames: 0,
                        depth: usize::MAX
                    },
                    control
                ),
                Err(StageAudioError::Range)
            ));
            assert_eq!(work_snapshot(control.work), before);
            control.cancelled.store(true, Ordering::Relaxed);
            let result = foreign.read_bound_at(bound, provider, at, (0, usize::MAX), control);
            assert!(matches!(
                result,
                Err(StageAudioError::Preparation(PreparationError::Cancelled))
            ));
            control.cancelled.store(false, Ordering::Relaxed);
            let expired = WorkControl {
                deadline: Instant::now() - Duration::from_secs(1),
                ..control
            };
            assert!(matches!(
                foreign.read_bound_at(bound, provider, at, (0, usize::MAX), expired),
                Err(StageAudioError::Timeout)
            ));
            assert_eq!(work_snapshot(control.work), before);
            let trace = raw();
            valid(&trace);
            for lane in ["plan", "cache"] {
                assert_eq!(trace[lane]["constructor_calls"], 0);
            }
            assert_eq!(provider.inner.calls, 0);
        },
    );
}

#[test]
fn genuine_frozen_silent_hold_bound_bypasses_raw_constructor_and_provider() {
    let frozen: Value = serde_json::from_str(include_str!(
        "../../../deadpan-plan/tests/fixtures/retained-fade-query-supplement-v1/snapshot.json"
    ))
    .unwrap();
    let case = frozen["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| {
            case["fixture"]["name"] == "retained_supplement/captured_marked_repeat_fragment"
        })
        .unwrap();
    let document = ProjectDocument::from_json(&case["fixture"]["document"].to_string()).unwrap();
    let plan = Arc::new(RenderPlan::compile(&document).unwrap());
    let owner = owner_at(&plan, "voice", AudioSample(6400));
    let input = plan
        .audio_owner_input(&owner, AudioSample(6400), Default::default())
        .unwrap();
    let probe = input
        .retained_probe_at(AudioSample(6400), Default::default())
        .unwrap();
    let retained = probe.input().unwrap();
    assert!(retained.bound().is_silent_hold());
    let mut renderer = StageAudio::new(plan.clone());
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut provider = ObservedProvider {
        prepared: cases::prepared(cases::Wav::Stereo48000),
        calls: 0,
        events: Vec::new(),
        work: &work,
        fault: Fault::None,
    };
    reset_raw();
    let before = work_snapshot(&work);
    let result = renderer
        .read_bound_at(
            retained.bound(),
            &mut provider,
            ExactRatio::integer(i64::MAX),
            (16, 0),
            control,
        )
        .unwrap();
    assert_eq!(result.samples, vec![[0.0; 2]; 16]);
    assert_eq!(result.suppressed, vec![SignalSample(0)..SignalSample(16)]);
    assert!(result.dependencies.is_empty());
    assert_eq!(result.relative_depth, 0);
    assert_eq!(work_snapshot(&work), before);
    let trace = raw();
    valid(&trace);
    for lane in ["plan", "cache"] {
        assert_eq!(trace[lane]["constructor_calls"], 0);
    }
    assert_eq!(trace["result_creations"], 0);
    assert_eq!(provider.calls, 0);
    raw_domain_query_trace::disable();
}
