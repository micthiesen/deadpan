//! Actual WAV consumers of the admitted Structural Point content carrier.
use super::*;
use crate::stages::{point_content_query_trace, point_content_request};
use deadpan_plan::{
    AudioDefinitionSelector, AudioPointDomain, AudioRootPlacement, ReferenceSample,
};

fn point_trace() -> Value {
    serde_json::to_value(point_content_query_trace::snapshot()).unwrap()
}
fn reset_point() {
    original_request::flattened_work_trace::reset();
    point_content_query_trace::reset();
}
fn units(value: &Value, field: &str) -> u64 {
    value[field].as_u64().unwrap()
}
fn valid_point(value: &Value) {
    for field in ["invalid", "nested", "overflow"] {
        assert_eq!(value[field], false, "{value}");
    }
    assert_eq!(value["active_depth"], 0);
    assert_eq!(value["live_results"], 0);
    assert_eq!(value["result_creations"], value["result_drops"]);
    for lane in ["plan", "cache"] {
        let lane = &value[lane];
        assert_eq!(
            units(lane, "constructor_calls"),
            units(lane, "ordinary_calls")
                + units(lane, "completed_queries")
                + units(lane, "failed_queries")
        );
        assert_eq!(lane["completed_queries"], lane["admitted_calls"]);
        assert_eq!(
            units(lane, "accepted_work"),
            units(lane, "canonical_units")
                + units(lane, "additional_units")
                + units(lane, "partial_work")
                + units(lane, "host_eviction_work")
        );
    }
}
struct PointWatch<'work> {
    inner: ObservedProvider<'work>,
    events: Vec<Value>,
    disable_at_source: bool,
}
impl AudioSourceProvider for PointWatch<'_> {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        self.events.push(json!({
            "point_content":point_trace(),
            "work":work_snapshot(self.inner.work),
        }));
        if self.disable_at_source {
            point_content_query_trace::disable();
        }
        self.inner.source(project, revision, asset, cancelled)
    }
}
fn point_plan(stage: bool) -> Arc<RenderPlan> {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut nodes = vec![("a", source(rate, 0, 8192, 8192))];
    let root = if stage {
        nodes.push(("stage", preserve("a", 8192, 4096)));
        "stage"
    } else {
        "a"
    };
    let (document, _) = document(rate, &[root], nodes);
    Arc::new(RenderPlan::compile(&document).unwrap())
}
fn point_domain(plan: &RenderPlan, stage: bool) -> AudioPointDomain<'_> {
    let definition = plan
        .audio_definition(AudioDefinitionSelector::Node {
            node: id(if stage { "stage" } else { "a" }),
        })
        .unwrap();
    definition
        .in_point_clock(
            AudioRootPlacement::new(
                ExactRatio::ZERO,
                ExactRatio::ONE,
                ExactRatio::ZERO..definition.exact_duration(),
            )
            .unwrap(),
            ExactRatio::ZERO,
        )
        .unwrap()
}
fn with_point<R>(
    stage: bool,
    fault: Fault,
    action: impl FnOnce(
        &mut StageAudio,
        &AudioPointDomain<'_>,
        WorkControl<'_>,
        &mut PointWatch<'_>,
    ) -> R,
) -> R {
    let plan = point_plan(stage);
    let domain = point_domain(&plan, stage);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut provider = PointWatch {
        inner: ObservedProvider {
            prepared: cases::prepared(cases::Wav::Stereo48000),
            calls: 0,
            events: Vec::new(),
            work: &work,
            fault,
        },
        events: Vec::new(),
        disable_at_source: false,
    };
    let result = action(&mut renderer, &domain, control, &mut provider);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(renderer.active_frames, 0);
    assert_eq!(renderer.active_disk_bytes, 0);
    point_content_query_trace::disable();
    result
}
fn point_output(result: &Result<SignalBlock, StageAudioError>) -> Value {
    match result {
        Ok(block) => json!({
            "status":"Ok",
            "f32_bits":block.samples.iter().map(|sample|sample.map(f32::to_bits)).collect::<Vec<_>>(),
            "dependencies":dependencies(&block.dependencies),
            "relative_depth":block.relative_depth,
            "suppressed":block.suppressed.iter().map(|range|[range.start.0,range.end.0]).collect::<Vec<_>>(),
        }),
        Err(error) => json!({"status":"Error","error":super::error(error)}),
    }
}
fn point_read(
    renderer: &mut StageAudio,
    domain: &AudioPointDomain<'_>,
    control: WorkControl<'_>,
    provider: &mut PointWatch<'_>,
) -> Result<SignalBlock, StageAudioError> {
    renderer.read_point_domain_controlled(
        provider,
        domain,
        ReferenceSample(domain.reference_samples().start.0 + 16),
        16,
        control,
        0,
    )
}
fn observed_point(cache: bool, fault: Fault, legacy: bool) -> Value {
    with_point(false, fault, |renderer, domain, control, provider| {
        observe_point(renderer, domain, control, provider, cache, legacy)
    })
}
fn observe_point(
    renderer: &mut StageAudio,
    domain: &AudioPointDomain<'_>,
    control: WorkControl<'_>,
    provider: &mut PointWatch<'_>,
    cache: bool,
    legacy: bool,
) -> Value {
    reset_point();
    let scope = cache.then(|| control.cache_key_scope());
    let before = work_snapshot(control.work);
    let controller_before = controller_snapshot(renderer);
    let result = if legacy {
        point_content_request::with_legacy_point_content(|| {
            point_read(renderer, domain, control, provider)
        })
    } else {
        point_read(renderer, domain, control, provider)
    };
    let output = point_output(&result);
    let trace = point_trace();
    valid_point(&trace);
    let after_return = work_snapshot(control.work);
    assert_eq!(after_return["sound_context_frames"], 0);
    let controller_after_return = controller_snapshot(renderer);
    drop(result);
    let after_drop = work_snapshot(control.work);
    let controller_after_drop = controller_snapshot(renderer);
    drop(scope);
    json!({
        "output":output,"point_content":trace,"events":provider.events,
        "provider_calls":provider.inner.calls,"provider_events":provider.inner.events,
        "work_before":before,"work_after_return":after_return,"work_after_drop":after_drop,
        "work_after_scope":work_snapshot(control.work),
        "controller_before":controller_before,"controller_after_return":controller_after_return,
        "controller_after_drop":controller_after_drop,
        "global_host_eviction":original_request::flattened_work_trace::host_eviction_units().0,
    })
}

#[test]
fn point_content_carrier_survives_actual_wav_success_failure_and_cancel_in_both_lanes() {
    for cache in [false, true] {
        for fault in [Fault::None, Fault::ProviderFailure, Fault::ProviderCancel] {
            let old = observed_point(cache, fault, true);
            let new = observed_point(cache, fault, false);
            assert_eq!(new["output"], old["output"]);
            assert_eq!(new["provider_calls"], old["provider_calls"]);
            assert!(new["provider_calls"].as_u64().unwrap() > 0);
            assert_eq!(new["controller_after_drop"], old["controller_after_drop"]);
            let lane = if cache { "cache" } else { "plan" };
            let other = if cache { "plan" } else { "cache" };
            let trace = &new["point_content"];
            assert_eq!(trace[lane]["completed_queries"], 1);
            assert_eq!(trace[lane]["partial_work"], 0);
            assert_eq!(trace[lane]["host_eviction_work"], 0);
            assert_eq!(trace[other]["accepted_work"], 0);
            assert!(units(&trace[lane], "additional_units") > 0);
            assert_eq!(
                trace[lane]["canonical_units"],
                old["point_content"][lane]["canonical_units"]
            );
            assert_eq!(trace["result_creations"], 1);
            assert_eq!(trace["result_drops"], 1);
            assert!(units(trace, "preflight_callbacks") > 0);
            if matches!(fault, Fault::None) {
                assert!(units(trace, "numerical_callbacks") > 0);
                assert!(units(trace, "span_callbacks") > 0);
                assert_eq!(new["output"]["status"], "Ok");
            } else if matches!(fault, Fault::ProviderCancel) {
                assert_eq!(new["output"]["error"]["is_cancelled"], true);
            } else {
                assert_eq!(
                    new["output"]["error"]["debug"],
                    "Preparation(SourceUnavailable(\"retained OLD provider sentinel\"))"
                );
            }
            for (lane, counter) in [("plan", "plan_work"), ("cache", "cache_plan_work")] {
                let new_delta =
                    units(&new["work_after_drop"], counter) - units(&new["work_before"], counter);
                let old_delta =
                    units(&old["work_after_drop"], counter) - units(&old["work_before"], counter);
                assert_eq!(
                    new_delta - old_delta,
                    units(&trace[lane], "additional_units"),
                    "the completed constructor's independent fee remains accepted after later source outcomes"
                );
            }
            assert_eq!(new["global_host_eviction"], json!([0, 0]));
            let new_events = new["events"].as_array().unwrap();
            let old_events = old["events"].as_array().unwrap();
            assert_eq!(new_events.len(), old_events.len());
            for (new_event, old_event) in new_events.iter().zip(old_events) {
                assert!(units(&new_event["point_content"], "live_results") > 0);
                assert_eq!(old_event["point_content"]["live_results"], 0);
                assert!(units(&new_event["work"], "sound_context_frames") > 0);
                assert_eq!(old_event["work"]["sound_context_frames"], 0);
            }
        }
    }
}

#[test]
fn point_content_resident_refusal_cleans_partial_metadata_before_any_provider_io() {
    for cache in [false, true] {
        with_point(false, Fault::None, |renderer, domain, control, provider| {
            renderer.limits.maximum_resident_frames = 1;
            reset_point();
            let scope = cache.then(|| control.cache_key_scope());
            let before = work_snapshot(control.work);
            let result = point_read(renderer, domain, control, provider);
            assert!(matches!(
                result,
                Err(StageAudioError::Limit("resident PCM or stage cache"))
            ));
            let trace = point_trace();
            valid_point(&trace);
            let lane = if cache { "cache" } else { "plan" };
            let other = if cache { "plan" } else { "cache" };
            assert_eq!(trace[lane]["constructor_calls"], 1);
            assert_eq!(trace[lane]["failed_queries"], 1);
            assert_eq!(trace[lane]["completed_queries"], 0);
            assert_eq!(trace[lane]["canonical_units"], 0);
            assert_eq!(trace[lane]["additional_units"], 0);
            assert_eq!(trace[lane]["accepted_work"], trace[lane]["partial_work"]);
            assert_eq!(trace[lane]["host_eviction_work"], 0);
            assert_eq!(trace[other]["accepted_work"], 0);
            assert_eq!(trace["result_creations"], 0);
            assert_eq!(provider.inner.calls, 0);
            let counter = if cache {
                "cache_plan_work"
            } else {
                "plan_work"
            };
            assert_eq!(
                units(&work_snapshot(control.work), counter) - units(&before, counter),
                units(&trace[lane], "partial_work")
            );
            assert_eq!(control.work.borrow().sound_context_frames, 0);
            drop(scope);
        });
    }
}

#[test]
fn point_content_plan_work_refusal_keeps_only_the_actual_accepted_prefix_before_wav_io() {
    let success = observed_point(false, Fault::None, false);
    let total = units(&success["point_content"]["plan"], "accepted_work");
    assert!(total > 1);
    with_point(false, Fault::None, |renderer, domain, control, provider| {
        let remaining = usize::try_from(total - 1).unwrap();
        control.work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - remaining;
        reset_point();
        let before = work_snapshot(control.work);
        let result = point_read(renderer, domain, control, provider);
        assert!(matches!(
            result,
            Err(StageAudioError::Plan(PlanError::AudioQueryLimit(
                "structural work"
            )))
        ));
        let trace = point_trace();
        valid_point(&trace);
        assert_eq!(trace["plan"]["constructor_calls"], 1);
        assert_eq!(trace["plan"]["failed_queries"], 1);
        assert_eq!(trace["plan"]["completed_queries"], 0);
        assert_eq!(trace["plan"]["canonical_units"], 0);
        assert_eq!(trace["plan"]["additional_units"], 0);
        assert_eq!(
            trace["plan"]["accepted_work"],
            trace["plan"]["partial_work"]
        );
        assert!(units(&trace["plan"], "partial_work") <= remaining as u64);
        assert_eq!(
            units(&work_snapshot(control.work), "plan_work") - units(&before, "plan_work"),
            units(&trace["plan"], "partial_work")
        );
        assert_eq!(trace["cache"]["accepted_work"], 0);
        assert_eq!(trace["result_creations"], 0);
        assert_eq!(provider.inner.calls, 0);
        assert_eq!(control.work.borrow().sound_context_frames, 0);
    });
}

#[test]
fn point_content_cache_lane_early_work_refusal_precedes_constructor_and_wav_io() {
    with_point(false, Fault::None, |renderer, domain, control, provider| {
        control.work.borrow_mut().cache_plan_work =
            MAX_PLAN_WORK_PER_READ - query_limits().maximum_work + 1;
        reset_point();
        let scope = control.cache_key_scope();
        let before = work_snapshot(control.work);
        assert!(matches!(
            point_read(renderer, domain, control, provider),
            Err(StageAudioError::Limit(CACHE_KEY_WORK_LIMIT))
        ));
        assert_eq!(work_snapshot(control.work), before);
        let trace = point_trace();
        valid_point(&trace);
        assert_eq!(trace["cache"]["constructor_calls"], 0);
        assert_eq!(provider.inner.calls, 0);
        drop(scope);
    });
}

#[test]
fn point_content_result_drop_survives_trace_disable_at_actual_wav_callback() {
    with_point(false, Fault::None, |renderer, domain, control, provider| {
        reset_point();
        provider.disable_at_source = true;
        let block = point_read(renderer, domain, control, provider).unwrap();
        assert!(provider.inner.calls > 0);
        assert!(
            provider
                .events
                .iter()
                .all(|event| units(&event["point_content"], "live_results") > 0)
        );
        let trace = point_trace();
        valid_point(&trace);
        assert_eq!(trace["result_creations"], 1);
        assert_eq!(trace["result_drops"], 1);
        assert_eq!(control.work.borrow().sound_context_frames, 0);
        drop(block);
        reset_point();
        valid_point(&point_trace());
    });
}

#[test]
fn point_content_stays_live_during_real_preserve_cache_hit_validation_in_both_lanes() {
    for cache in [false, true] {
        with_point(true, Fault::None, |renderer, domain, control, provider| {
            let cold = point_read(renderer, domain, control, provider).unwrap();
            assert!(!renderer.cache.is_empty());
            let prepared = control.work.borrow().prepared_stages;
            let cache_before = controller_snapshot(renderer)["cache"].clone();
            provider.events.clear();
            provider.inner.events.clear();
            provider.inner.calls = 0;
            reset_point();
            let scope = cache.then(|| control.cache_key_scope());
            let warm = point_read(renderer, domain, control, provider).unwrap();
            assert_eq!(point_output(&Ok(warm)), point_output(&Ok(cold)));
            assert_eq!(control.work.borrow().prepared_stages, prepared);
            assert_eq!(controller_snapshot(renderer)["cache"], cache_before);
            assert!(
                provider.inner.calls > 0,
                "cached Preserve still validates actual WAV dependencies"
            );
            assert!(
                provider
                    .events
                    .iter()
                    .all(|event| units(&event["point_content"], "live_results") > 0)
            );
            let trace = point_trace();
            valid_point(&trace);
            let lane = if cache { "cache" } else { "plan" };
            assert_eq!(trace[lane]["completed_queries"], 1);
            assert!(units(&trace[lane], "additional_units") > 0);
            assert!(units(&trace, "numerical_callbacks") > 0);
            assert_eq!(control.work.borrow().sound_context_frames, 0);
            drop(scope);
        });
    }
}

#[test]
fn point_content_constructor_records_real_original_cache_eviction_separately_in_both_lanes() {
    for cache in [false, true] {
        with_point(false, Fault::None, |renderer, domain, control, provider| {
            shared_eq::with_cached_original_for_point_test(
                renderer,
                provider,
                control,
                |renderer, provider| {
                    assert!(!renderer.original_inputs.is_empty());
                    let caller = control.work.borrow().sound_context_frames;
                    let cached_inputs = renderer.original_inputs.len();
                    let cached_frames = renderer.cached_resident_frames();
                    assert!(cached_frames > 0);
                    renderer.limits.maximum_resident_frames =
                        u32::try_from(cached_frames + caller).unwrap();
                    provider.events.clear();
                    reset_point();
                    let scope = cache.then(|| control.cache_key_scope());
                    let result = point_read(renderer, domain, control, provider).unwrap();
                    let trace = point_trace();
                    valid_point(&trace);
                    let (global, overflow) =
                        original_request::flattened_work_trace::host_eviction_units();
                    assert!(!overflow);
                    let index = usize::from(cache);
                    assert!(global[index] > 0);
                    assert_eq!(global[1 - index], 0);
                    assert_eq!(
                        trace[if cache { "cache" } else { "plan" }]["host_eviction_work"],
                        global[index]
                    );
                    assert!(renderer.original_inputs.len() < cached_inputs);
                    assert_eq!(trace["result_creations"], 1);
                    assert_eq!(trace["result_drops"], 1);
                    assert_eq!(control.work.borrow().sound_context_frames, caller);
                    drop(result);
                    drop(scope);
                },
            );
        });
    }
}

#[test]
fn point_content_early_foreign_depth_range_cancel_and_deadline_priorities_precede_construction() {
    with_point(false, Fault::None, |renderer, domain, control, provider| {
        let mut foreign = StageAudio::new(point_plan(false));
        let start = domain.reference_samples().start;
        for (use_foreign, depth, frames, expected) in [
            (true, usize::MAX, 0, "foreign"),
            (false, usize::MAX, 0, "depth"),
            (false, 0, 0, "range"),
            (false, 0, MAX_OUTPUT_FRAMES + 1, "range"),
        ] {
            reset_point();
            let before = work_snapshot(control.work);
            let target = if use_foreign {
                &mut foreign
            } else {
                &mut *renderer
            };
            let result = target
                .read_point_domain_controlled(provider, domain, start, frames, control, depth);
            match expected {
                "foreign" => assert!(matches!(result, Err(StageAudioError::ForeignDomain))),
                "depth" => assert!(matches!(
                    result,
                    Err(StageAudioError::Limit("nested stage depth"))
                )),
                "range" => assert!(matches!(result, Err(StageAudioError::Range))),
                _ => unreachable!(),
            }
            assert_eq!(work_snapshot(control.work), before);
            valid_point(&point_trace());
            assert_eq!(point_trace()["plan"]["constructor_calls"], 0);
            assert_eq!(provider.inner.calls, 0);
        }
        reset_point();
        let before = work_snapshot(control.work);
        control.cancelled.store(true, Ordering::Relaxed);
        assert!(matches!(
            foreign.read_point_domain_controlled(provider, domain, start, 0, control, usize::MAX,),
            Err(StageAudioError::Preparation(PreparationError::Cancelled))
        ));
        control.cancelled.store(false, Ordering::Relaxed);
        let expired = WorkControl {
            deadline: Instant::now() - Duration::from_secs(1),
            ..control
        };
        assert!(matches!(
            foreign.read_point_domain_controlled(provider, domain, start, 0, expired, usize::MAX,),
            Err(StageAudioError::Timeout)
        ));
        assert_eq!(work_snapshot(control.work), before);
        valid_point(&point_trace());
        assert_eq!(point_trace()["plan"]["constructor_calls"], 0);
        assert_eq!(provider.inner.calls, 0);
    });
}

fn observed_bound_point(cache: bool, legacy: bool) -> Value {
    let fixtures = cases::fixtures();
    let fixture = &fixtures[1];
    let plan = Arc::new(RenderPlan::compile(&fixture.current).unwrap());
    let definition = plan
        .audio_definition(AudioDefinitionSelector::Node { node: id("a") })
        .unwrap();
    let domain = definition
        .in_point_clock(
            AudioRootPlacement::new(
                ExactRatio::ZERO,
                ExactRatio::ONE,
                ExactRatio::ZERO..definition.exact_duration(),
            )
            .unwrap(),
            ExactRatio::ZERO,
        )
        .unwrap();
    // This standalone query proves the public factory produced an actual Bound.
    // It is outside the measured ReadWork and is dropped before the native read.
    let ordinary = domain
        .signal_ref()
        .query(SignalSample(16)..SignalSample(32), query_limits())
        .unwrap();
    assert!(
        ordinary
            .spans
            .iter()
            .any(|span| matches!(&span.content, AudioSignalContent::Bound(_)))
    );
    drop(ordinary);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut provider = PointWatch {
        inner: ObservedProvider {
            prepared: cases::prepared(fixture.wav),
            calls: 0,
            events: Vec::new(),
            work: &work,
            fault: Fault::None,
        },
        events: Vec::new(),
        disable_at_source: false,
    };
    let result = observe_point(
        &mut renderer,
        &domain,
        control,
        &mut provider,
        cache,
        legacy,
    );
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(renderer.active_frames, 0);
    assert_eq!(renderer.active_disk_bytes, 0);
    point_content_query_trace::disable();
    result
}

#[test]
fn point_content_public_bound_keeps_nested_actual_point_carriers_live_through_wav() {
    for cache in [false, true] {
        let old = observed_bound_point(cache, true);
        let new = observed_bound_point(cache, false);
        assert_eq!(new["output"], old["output"]);
        assert_eq!(new["output"]["status"], "Ok");
        assert_eq!(new["provider_calls"], old["provider_calls"]);
        assert!(units(&new, "provider_calls") > 0);
        let trace = &new["point_content"];
        valid_point(trace);
        assert!(units(trace, "peak_live_results") > 1);
        assert!(units(trace, "result_creations") > 1);
        assert_eq!(trace["result_creations"], trace["result_drops"]);
        assert_eq!(
            trace["nested"], false,
            "live results nest after constructor scopes end"
        );
        for event in new["events"].as_array().unwrap() {
            assert!(units(&event["point_content"], "live_results") > 1);
        }
        for (lane, counter) in [("plan", "plan_work"), ("cache", "cache_plan_work")] {
            assert_eq!(
                trace[lane]["canonical_units"],
                old["point_content"][lane]["canonical_units"]
            );
            assert_eq!(trace[lane]["partial_work"], 0);
            assert_eq!(trace[lane]["host_eviction_work"], 0);
            let new_delta =
                units(&new["work_after_drop"], counter) - units(&new["work_before"], counter);
            let old_delta =
                units(&old["work_after_drop"], counter) - units(&old["work_before"], counter);
            assert_eq!(
                new_delta - old_delta,
                units(&trace[lane], "additional_units")
            );
        }
        assert_eq!(new["global_host_eviction"], json!([0, 0]));
    }
}
