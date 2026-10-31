//! Actual WAV cache pressure, with a receipt at the accepted eviction debit.
use super::*;

#[test]
fn completed_original_constructors_separate_real_eviction_work_in_both_products_and_lanes() {
    for processing_eviction in [false, true] {
        for cache_lane in [false, true] {
            let (plan, mut provider) = fixture();
            let cancelled = AtomicBool::new(false);
            let measured = RefCell::new(ReadWork::default());
            let work = RefCell::new(ReadWork::default());
            let measurement = control(&cancelled, &measured);
            let control = control(&cancelled, &work);
            let mut renderer = StageAudio::new(Arc::clone(&plan));
            let flat = original_request::with_legacy_processing_queries(|| {
                renderer.prepare_original_root(&plan, AudioSample(0), 256, measurement, true)
            })
            .unwrap();
            let flat_frames = measured.borrow().sound_context_frames;
            assert!(flat_frames > 0);
            drop(flat);
            let whole = renderer
                .prepare_original_root(&plan, AudioSample(0), 256, measurement, true)
                .unwrap();
            assert_eq!(whole.queries().processing.spans.len(), 1);
            assert!(matches!(
                whole.queries().processing.spans[0].content,
                AudioSignalContent::Leaf(AudioContent::Source { .. })
            ));
            let retained = measured.borrow().sound_context_frames;
            // The one-span Root conversion simultaneously owns its one-slot
            // SignalSpan input Vec and completed processing output Vec.
            let input = u64::try_from(std::mem::size_of::<deadpan_plan::AudioSignalSpan<'_>>())
                .unwrap()
                .div_ceil(8);
            let required_peak = retained.checked_add(input).unwrap();
            drop(whole);
            assert_eq!(measured.borrow().sound_context_frames, 0);
            assert_eq!(provider.calls, 0);

            let mut graph = graph(&plan);
            let cached = renderer
                .shared_original_input(
                    &mut provider,
                    &mut graph,
                    &Context::Root,
                    AudioSample(0)..AudioSample(256),
                    control,
                    0,
                )
                .unwrap();
            let original_inputs::Read::Cached(cached) = cached else {
                panic!("real cold read must publish the verified WAV cache payload");
            };
            drop(cached); // Leave the controller's single Arc evictable.
            assert_eq!(renderer.original_inputs.len(), 1);
            let cached_frames = renderer.cached_resident_frames();
            assert!(
                cached_frames > required_peak,
                "cached={cached_frames}, peak={required_peak}, flat={flat_frames}, retained={retained}"
            );
            renderer.limits.maximum_resident_frames = u32::try_from(if processing_eviction {
                cached_frames.checked_add(flat_frames).unwrap()
            } else {
                required_peak
            })
            .unwrap();
            let calls = provider.calls;
            let before = {
                let work = work.borrow();
                [work.plan_work, work.cache_plan_work]
            };
            original_request::flattened_work_trace::reset();
            let cache_scope = cache_lane.then(|| control.cache_key_scope());
            let request = renderer
                .prepare_original_root(&plan, AudioSample(0), 256, control, true)
                .unwrap();
            drop(cache_scope);
            assert_eq!(
                provider.calls, calls,
                "constructor pressure never reopens WAV"
            );
            assert!(
                renderer.original_inputs.is_empty(),
                "actual existing cache payload was evicted"
            );
            assert!(work.borrow().sound_context_frames > 0);
            let flat =
                serde_json::to_value(original_request::flattened_work_trace::snapshot()).unwrap();
            let processing =
                serde_json::to_value(original_request::flattened_work_trace::processing_snapshot())
                    .unwrap();
            let lane = if cache_lane { "cache" } else { "plan" };
            let other = if cache_lane { "plan" } else { "cache" };
            let selected = if processing_eviction {
                &processing
            } else {
                &flat
            };
            let not_selected = if processing_eviction {
                &flat
            } else {
                &processing
            };
            for trace in [&flat, &processing] {
                for field in ["invalid", "nested", "overflow"] {
                    assert_eq!(trace[field], false, "{trace}");
                }
                assert_eq!(trace["active_depth"], 0);
                assert_eq!(trace[lane]["constructor_calls"], 1);
                assert_eq!(trace[lane]["completed_queries"], 1);
                assert_eq!(trace[lane]["failed_queries"], 0);
                assert_eq!(trace[lane]["partial_work"], 0);
                assert_eq!(trace[other]["accepted_work"], 0);
                assert_eq!(trace["all_host_eviction_work"][usize::from(cache_lane)], 1);
                assert_eq!(trace["all_host_eviction_work"][usize::from(!cache_lane)], 0);
                let canonical = trace[lane]["canonical_units"].as_u64().unwrap();
                let additional = trace[lane]["additional_units"].as_u64().unwrap();
                let eviction = trace[lane]["host_eviction_work"].as_u64().unwrap();
                assert_eq!(
                    trace[lane]["accepted_work"].as_u64().unwrap(),
                    canonical + additional + eviction
                );
            }
            assert_eq!(selected[lane]["host_eviction_work"], 1);
            assert_eq!(not_selected[lane]["host_eviction_work"], 0);
            let queries = request.queries();
            let constructor_and_policy = queries.flattened.work
                + queries.processing.work
                + queries.policy.work
                + queries.fades.unwrap().work;
            let after = {
                let work = work.borrow();
                [work.plan_work, work.cache_plan_work]
            };
            let index = usize::from(cache_lane);
            assert_eq!(after[index] - before[index], constructor_and_policy + 1);
            assert_eq!(after[1 - index], before[1 - index]);
            drop(request);
            assert_eq!(work.borrow().sound_context_frames, 0);
            graph.original_inputs.clear(&mut renderer);
            assert_eq!(renderer.active_frames, 0);
        }
    }
}
