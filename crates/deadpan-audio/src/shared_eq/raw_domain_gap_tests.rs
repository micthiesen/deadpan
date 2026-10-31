//! Authentic Original cache factories, with raw metadata receipts kept separate.
use super::*;
use crate::stages::raw_domain_query_trace;

fn raw() -> Value {
    serde_json::to_value(raw_domain_query_trace::snapshot()).unwrap()
}
fn reset() {
    reset_receipts();
    raw_domain_query_trace::reset();
}
fn valid(value: &Value) {
    for flag in ["invalid", "overflow", "nested"] {
        assert_eq!(value[flag], false);
    }
    assert_eq!(value["active_depth"], 0);
    assert_eq!(value["live_results"], 0);
    assert_eq!(value["result_creations"], value["result_drops"]);
    for lane in ["plan", "cache"] {
        let v = &value[lane];
        let n = |f: &str| v[f].as_u64().unwrap();
        assert_eq!(
            n("constructor_calls"),
            n("completed_queries") + n("ordinary_calls") + n("failed_queries")
        );
        assert_eq!(n("canonical_units"), 0);
        assert_eq!(n("admitted_calls"), n("completed_queries"));
        assert_eq!(
            n("accepted_work"),
            n("additional_units") + n("partial_work") + n("host_eviction_work")
        );
    }
}

#[test]
fn real_retained_original_cache_hit_rechecks_wav_without_any_fresh_raw_constructor() {
    for cache_lane in [false, true] {
        let (plan, mut provider) = retained_fixture();
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        let input = owner_input(&plan);
        control.spend_plan_work(input.work()).unwrap();
        let caller = {
            let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
            authored_eq::Context::finish_paid_owner_input_admitted(
                &input,
                input.owner(),
                AudioSample(512),
                control,
                &mut admission,
            )
        }
        .unwrap();
        let context = Context::Retained(caller.retained().unwrap().clone());
        let caller_frames = work.borrow().sound_context_frames;
        assert!(caller_frames > 0);
        let mut graph = graph(&plan);
        let range = AudioSample(-1536)..AudioSample(-1408);
        reset();
        let scope = cache_lane.then(|| control.cache_key_scope());
        let first = renderer
            .shared_original_input(
                &mut provider,
                &mut graph,
                &context,
                range.clone(),
                control,
                0,
            )
            .unwrap();
        drop(scope);
        let Read::Cached(first) = first else {
            panic!("real retained miss must populate the cache")
        };
        let cold = raw();
        valid(&cold);
        assert!(cold["result_creations"].as_u64().unwrap() > 0);
        assert_eq!(work.borrow().sound_context_frames, caller_frames);
        let calls = provider.calls;
        let source_checks = work.borrow().source_checks;
        let published = renderer.next_original_input;
        let before = [work.borrow().plan_work, work.borrow().cache_plan_work];
        reset();
        let scope = cache_lane.then(|| control.cache_key_scope());
        let hit = renderer
            .shared_original_input(&mut provider, &mut graph, &context, range, control, 0)
            .unwrap();
        drop(scope);
        let Read::Cached(hit) = hit else {
            panic!("the exact actual key must hit")
        };
        assert!(Arc::ptr_eq(&first, &hit));
        assert_eq!(first.samples, hit.samples);
        assert_eq!(first.dependencies, hit.dependencies);
        assert_eq!(first.suppressed, hit.suppressed);
        assert_eq!(first.exhausted, hit.exhausted);
        assert_eq!(first.relative_depth, hit.relative_depth);
        assert!(provider.calls > calls);
        assert!(work.borrow().source_checks > source_checks);
        assert_eq!(renderer.next_original_input, published);
        assert_eq!(work.borrow().sound_context_frames, caller_frames);
        let trace = raw();
        valid(&trace);
        assert_eq!(trace["result_creations"], 0);
        for lane in ["plan", "cache"] {
            assert_eq!(trace[lane]["constructor_calls"], 0);
            assert_eq!(trace[lane]["accepted_work"], 0);
        }
        let after = [work.borrow().plan_work, work.borrow().cache_plan_work];
        let selected = usize::from(cache_lane);
        assert!(after[selected] > before[selected]);
        assert_eq!(after[1 - selected], before[1 - selected]);
        drop(first);
        drop(hit);
        graph.original_inputs.clear(&mut renderer);
        drop(context);
        drop(caller);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(renderer.cached_resident_frames(), 0);
        assert_eq!(renderer.active_frames, 0);
        assert_eq!(renderer.active_disk_bytes, 0);
        raw_domain_query_trace::disable();
    }
}

#[test]
fn actual_raw_constructor_evicts_real_unpinned_wav_cache_in_each_accounting_lane() {
    for cache_lane in [false, true] {
        let (plan, mut provider) = retained_fixture();
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        let input = owner_input(&plan);
        control.spend_plan_work(input.work()).unwrap();
        let caller = {
            let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
            authored_eq::Context::finish_paid_owner_input_admitted(
                &input,
                input.owner(),
                AudioSample(512),
                control,
                &mut admission,
            )
        }
        .unwrap();
        let retained = caller.retained().unwrap();
        let caller_frames = work.borrow().sound_context_frames;
        assert!(caller_frames > 0);
        let mut graph = graph(&plan);
        for index in 0..16 {
            let start = 512 + index * 256;
            let cached = renderer
                .shared_original_input(
                    &mut provider,
                    &mut graph,
                    &Context::Root,
                    AudioSample(start)..AudioSample(start + 256),
                    control,
                    0,
                )
                .unwrap();
            assert!(matches!(cached, Read::Cached(_)));
            drop(cached);
        }
        let count = renderer.original_inputs.len();
        assert_eq!(count, 16);
        let cached_frames = renderer.cached_resident_frames();
        assert!(cached_frames > 0);
        assert_eq!(work.borrow().sound_context_frames, caller_frames);
        let limit = cached_frames.checked_add(caller_frames).unwrap();
        assert!(limit < u64::from(renderer.limits.maximum_resident_frames));
        renderer.limits.maximum_resident_frames = u32::try_from(limit).unwrap();
        reset();
        let scope = cache_lane.then(|| control.cache_key_scope());
        let at = retained.reference_at(AudioSample(-1536)).unwrap();
        let result = renderer.read_bound_at(retained.bound(), &mut provider, at, (16, 0), control);
        let block = result.expect("the real cache can evict enough unpinned payload for the read");
        assert_eq!(block.samples.len(), 16);
        assert!(!block.dependencies.is_empty());
        assert!(block.samples.iter().any(|sample| *sample != [0.0; 2]));
        drop(block);
        drop(scope);
        let trace = raw();
        valid(&trace);
        let lane = if cache_lane { "cache" } else { "plan" };
        let other = if cache_lane { "plan" } else { "cache" };
        let receipt = &trace[lane];
        assert_eq!(receipt["completed_queries"], 1);
        assert_eq!(receipt["failed_queries"], 0);
        assert_eq!(receipt["partial_work"], 0);
        let raw_evicted = receipt["host_eviction_work"].as_u64().unwrap();
        assert!(
            raw_evicted > 0,
            "real native raw reservation must evict an actual WAV cache payload"
        );
        let global = receipts()["fade"]["all_host_eviction_work"][usize::from(cache_lane)]
            .as_u64()
            .unwrap();
        let removed = count - renderer.original_inputs.len();
        assert_eq!(global, removed as u64);
        assert!(
            global >= raw_evicted,
            "later numerical work may evict more; raw records its own real subset"
        );
        assert_eq!(
            receipt["accepted_work"].as_u64().unwrap(),
            receipt["additional_units"].as_u64().unwrap() + raw_evicted
        );
        assert_eq!(trace[other]["accepted_work"], 0);
        assert_eq!(work.borrow().sound_context_frames, caller_frames);
        graph.original_inputs.clear(&mut renderer);
        drop(caller);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(renderer.cached_resident_frames(), 0);
        assert_eq!(renderer.active_frames, 0);
        assert_eq!(renderer.active_disk_bytes, 0);
        raw_domain_query_trace::disable();
    }
}
