//! Processing-only policy/cache lifecycle using the existing checked Graph fixture.
use super::*;

#[test]
fn processing_only_lease_remains_live_inside_policy_population_and_numeric_read() {
    for domain_read in [false, true] {
        let (plan, mut provider) = fixture();
        let context = flattened_context(&plan, domain_read);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        let range = AudioSample(0)..AudioSample(64);
        let request = original_request::with_legacy_flattened_queries(|| match &context {
            Context::Root => renderer.prepare_original_root(&plan, range.start, 64, control, true),
            Context::Domain(domain) => {
                renderer.prepare_original_domain(domain, range.start, 64, control, true)
            }
            Context::Retained(_) => unreachable!(),
        })
        .unwrap();
        let live = work.borrow().sound_context_frames;
        assert!(live > 0);
        let owners = match &context {
            Context::Root => plan.audio_gain_owners(range.clone(), Default::default()),
            Context::Domain(domain) => domain.gain_owners(range.clone(), Default::default()),
            Context::Retained(_) => unreachable!(),
        }
        .unwrap();
        let mut mix = SharedMix::new(range.start, 64, None, None, true, None, &[]);
        let routes = owners
            .spans()
            .iter()
            .map(|span| {
                assert_eq!(span.support(), AudioOwnerSupport::Active);
                Some(
                    mix.route(
                        span.owners(),
                        span.samples(),
                        (0, Contribution::Original),
                        control,
                    )
                    .unwrap(),
                )
            })
            .collect::<Vec<_>>();
        mix.begin_prediction().unwrap();
        let reserved = mix.resident_frames().unwrap() + 64 * 3;
        renderer.make_room(reserved, false, control).unwrap();
        renderer.active_frames += reserved;
        mix.allocate_policy();
        let observed = std::cell::Cell::new(0_usize);
        policy_prediction::populate(
            &mut mix,
            policy_prediction::Inputs {
                original: Some(policy_prediction::Original {
                    policy: policy_prediction::OriginalPolicy::Queries(request.queries()),
                    owners: &owners,
                    routes: &routes,
                    id: 0,
                }),
                roots: &[],
                root_range: range.clone(),
                root_ids: &[],
                root_routes: &[],
                beats: &[],
                beat_routes: &[],
            },
            |_| {
                assert_eq!(
                    work.borrow().sound_context_frames,
                    live,
                    "actual Original selection callback sees the isolated processing lease"
                );
                observed.set(observed.get() + 1);
                true
            },
            control,
        )
        .unwrap();
        assert!(observed.get() > 0);
        assert_eq!(work.borrow().sound_context_frames, live);
        assert_eq!(provider.calls, 0);
        assert_eq!(work.borrow().source_checks, 0);
        mix.allocate();
        let raw = renderer
            .read_prepared_original(&mut provider, request, control, 0)
            .unwrap();
        assert!(provider.calls > 0);
        assert_eq!(
            work.borrow().sound_context_frames,
            0,
            "query charge ends while the actual returned PCM remains live"
        );
        for (span, route) in owners.spans().iter().zip(&routes) {
            for at in span.samples().start.0..span.samples().end.0 {
                let permitted = !raw
                    .suppressed
                    .iter()
                    .chain(&raw.exhausted)
                    .any(|r| r.contains(&AudioSample(at)));
                mix.add(
                    route.as_ref().unwrap(),
                    at as usize,
                    raw.samples[at as usize],
                    0,
                    permitted.then_some(0),
                    control,
                )
                .unwrap();
            }
        }
        mix.verify_prediction(control).unwrap();
        let _finished = mix
            .finish(control, |_, _, _, _, _| panic!("no temporal stages"))
            .unwrap();
        renderer.active_frames -= reserved;
        assert_eq!(renderer.active_frames, 0);
        assert_eq!(work.borrow().sound_context_frames, 0);
    }
}

#[test]
fn processing_only_prepared_late_cache_hit_releases_unused_lease_and_preserves_cached_arc() {
    for domain_read in [false, true] {
        let (plan, mut provider) = fixture();
        let context = flattened_context(&plan, domain_read);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        let mut graph = graph(&plan);
        let range = AudioSample(0)..AudioSample(64);
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
        assert_eq!(work.borrow().sound_context_frames, 0);
        let original_inputs::Read::Cached(first) = first else {
            panic!("real miss must publish cached input");
        };
        let request = original_request::with_legacy_flattened_queries(|| match &context {
            Context::Root => renderer.prepare_original_root(&plan, range.start, 64, control, true),
            Context::Domain(domain) => {
                renderer.prepare_original_domain(domain, range.start, 64, control, true)
            }
            Context::Retained(_) => unreachable!(),
        })
        .unwrap();
        assert!(work.borrow().sound_context_frames > 0);
        let spent = work.borrow().plan_work;
        let calls = provider.calls;
        let published = renderer.next_original_input;
        let hit = renderer
            .shared_original_input_prepared(
                &mut provider,
                &mut graph,
                &context,
                (range, request),
                control,
                0,
            )
            .unwrap();
        let original_inputs::Read::Cached(hit) = hit else {
            panic!("prepared late lookup must reuse cache");
        };
        assert!(Arc::ptr_eq(&first, &hit));
        assert!(
            provider.calls > calls,
            "late hit still re-admits live source"
        );
        assert!(work.borrow().plan_work > spent);
        assert_eq!(renderer.next_original_input, published);
        assert_eq!(
            work.borrow().sound_context_frames,
            0,
            "unused prepared processing query lease dies on the late-hit return"
        );
        drop(first);
        drop(hit);
        graph.original_inputs.clear(&mut renderer);
        assert_eq!(renderer.active_frames, 0);
    }
}

#[test]
fn processing_only_prepared_foreign_late_hit_refuses_before_cache_or_source_io() {
    for domain_read in [false, true] {
        let (plan, mut provider) = fixture();
        let context = flattened_context(&plan, domain_read);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let foreign_work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        let mut graph = graph(&plan);
        let range = AudioSample(0)..AudioSample(64);
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
        let request = original_request::with_legacy_flattened_queries(|| match &context {
            Context::Root => renderer.prepare_original_root(&plan, range.start, 64, control, true),
            Context::Domain(domain) => {
                renderer.prepare_original_domain(domain, range.start, 64, control, true)
            }
            Context::Retained(_) => unreachable!(),
        })
        .unwrap();
        assert!(work.borrow().sound_context_frames > 0);
        let foreign = super::control(&cancelled, &foreign_work);
        let calls = provider.calls;
        let spent = work.borrow().plan_work;
        let published = renderer.next_original_input;
        assert!(matches!(
            renderer.shared_original_input_prepared(
                &mut provider,
                &mut graph,
                &context,
                (range, request),
                foreign,
                0
            ),
            Err(StageAudioError::Plan(
                PlanError::AudioMetadataIssuerMismatch
            ))
        ));
        assert_eq!(provider.calls, calls);
        assert_eq!(renderer.next_original_input, published);
        assert_eq!(work.borrow().plan_work, spent);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(foreign_work.borrow().sound_context_frames, 0);
        assert_eq!(foreign_work.borrow().source_checks, 0);
        assert_eq!(
            foreign_work.borrow().plan_work,
            2,
            "sealed request validation precedes the real processing issuer comparison"
        );
        assert!(foreign_work.borrow().preflight_assets.is_empty());
        drop(first);
        graph.original_inputs.clear(&mut renderer);
        assert_eq!(renderer.active_frames, 0);
    }
}

#[test]
fn warm_probe_constructs_neither_flattened_nor_processing_query() {
    use cached_prediction_trace::Phase;
    for domain_read in [false, true] {
        let (plan, mut provider) = fixture();
        let context = flattened_context(&plan, domain_read);
        let contexts = Contexts {
            original: context.clone(),
            sounds: Sounds::Root,
        };
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let frames = 16;
        let reservation =
            u64::from(frames) * 3 + original_inputs::Cache::resident_frames().unwrap();
        renderer.make_room(reservation, false, control).unwrap();
        renderer.active_frames += reservation;
        let mut graph = graph(&plan);
        let program = Program {
            context: &contexts,
            stop: None,
            allowed: None,
            output_voice: None,
            parent: usize::MAX,
            depth: 0,
        };
        cached_prediction_trace::reset();
        original_request::flattened_work_trace::reset();
        let first = renderer
            .read_shared_program(
                &mut provider,
                &mut graph,
                program,
                AudioSample(0),
                frames,
                control,
            )
            .unwrap();
        let cold = cached_prediction_trace::snapshot();
        let cold_flat =
            serde_json::to_value(original_request::flattened_work_trace::snapshot()).unwrap();
        let cold_processing =
            serde_json::to_value(original_request::flattened_work_trace::processing_snapshot())
                .unwrap();
        for trace in [&cold_flat, &cold_processing] {
            assert!(
                trace["plan"]["constructor_calls"].as_u64().unwrap()
                    + trace["cache"]["constructor_calls"].as_u64().unwrap()
                    > 0
            );
            assert_eq!(trace["invalid"], false);
            assert_eq!(trace["overflow"], false);
        }
        assert!(cold.phase(Phase::Queries).plan_work > 0);
        assert!(cold.phase(Phase::OriginalRead).plan_work > 0);
        assert_eq!(work.borrow().sound_context_frames, 0);
        let calls = provider.calls;
        let published = renderer.next_original_input;
        let second = renderer
            .read_shared_program(
                &mut provider,
                &mut graph,
                program,
                AudioSample(0),
                frames,
                control,
            )
            .unwrap();
        let warm = cached_prediction_trace::snapshot();
        assert_eq!(
            serde_json::to_value(original_request::flattened_work_trace::snapshot()).unwrap(),
            cold_flat
        );
        assert_eq!(
            serde_json::to_value(original_request::flattened_work_trace::processing_snapshot())
                .unwrap(),
            cold_processing
        );
        assert_eq!(
            warm.phase(Phase::Queries).plan_work - cold.phase(Phase::Queries).plan_work,
            0
        );
        assert_eq!(
            warm.phase(Phase::Queries).cache_plan_work - cold.phase(Phase::Queries).cache_plan_work,
            0
        );
        assert_eq!(
            warm.calls[0] - cold.calls[0],
            0,
            "no query scope is entered on early warm hit"
        );
        assert_eq!(
            warm.phase(Phase::OriginalRead).plan_work - cold.phase(Phase::OriginalRead).plan_work,
            0
        );
        assert!(warm.phase(Phase::Probe).plan_work > cold.phase(Phase::Probe).plan_work);
        assert!(warm.calls[3] > cold.calls[3]);
        assert_eq!(warm.active_depth, 0);
        assert_eq!(warm.overflow, 0);
        assert!(
            provider.calls > calls,
            "warm Probe preserves fresh source admission"
        );
        assert_eq!(renderer.next_original_input, published);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(wide(&first), wide(&second));
        renderer.release_shared_graph(graph, reservation);
        assert!(renderer.original_inputs.is_empty());
        assert_eq!(renderer.active_frames, 0);
        assert_eq!(work.borrow().sound_context_frames, 0);
    }
}

#[path = "processing_eviction_trace_tests.rs"]
mod eviction_trace;
