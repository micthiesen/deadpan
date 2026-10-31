use super::*;

const ADDITIONAL: u64 = 37;

fn seed<'plan, 'work>(
    renderer: &mut StageAudio,
    provider: &mut Provider,
    graph: &mut Graph<'plan, 'work>,
    range: Range<AudioSample>,
    control: WorkControl<'work>,
) -> u64 {
    let read = renderer
        .shared_original_input(provider, graph, &Context::Root, range.clone(), control, 0)
        .unwrap();
    assert!(matches!(read, Read::Cached(_)));
    let key = graph
        .original_inputs
        .entries
        .iter()
        .flatten()
        .find(|entry| entry.samples == range && entry.depth == 0)
        .unwrap()
        .key;
    drop(read);
    assert_eq!(Arc::strong_count(&renderer.original_inputs[&key].block), 1);
    key
}

fn probe<'plan, 'work>(
    renderer: &StageAudio,
    provider: &mut impl AudioSourceProvider,
    graph: &mut Graph<'plan, 'work>,
    additional: u64,
    control: WorkControl<'work>,
) -> Result<Option<Arc<ReadBlock>>, StageAudioError> {
    renderer.try_cached_original_for_prediction(
        provider,
        graph,
        &Context::Root,
        (AudioSample(0)..AudioSample(64), additional),
        control,
        0,
    )
}

fn declined_without_side_effects<'plan, 'work>(
    renderer: &StageAudio,
    provider: &mut Provider,
    graph: &mut Graph<'plan, 'work>,
    additional: u64,
    control: WorkControl<'work>,
) {
    let before = cache_snapshot(&graph.original_inputs);
    let calls = provider.calls;
    let payloads = renderer.original_inputs.len();
    let required = renderer.cache.len();
    let counts = renderer
        .original_inputs
        .values()
        .map(|cached| Arc::strong_count(&cached.block))
        .collect::<Vec<_>>();
    let before_work = control.work.borrow().plan_work;
    assert!(
        probe(renderer, provider, graph, additional, control)
            .unwrap()
            .is_none()
    );
    assert_eq!(cache_snapshot(&graph.original_inputs), before);
    assert_eq!(
        provider.calls, calls,
        "declined candidate must not perform source I/O"
    );
    assert_eq!(renderer.original_inputs.len(), payloads);
    assert_eq!(
        renderer.cache.len(),
        required,
        "probe cannot evict required stages"
    );
    assert_eq!(
        renderer
            .original_inputs
            .values()
            .map(|cached| Arc::strong_count(&cached.block))
            .collect::<Vec<_>>(),
        counts,
        "no candidate pin may escape a decline"
    );
    assert!(
        control.work.borrow().plan_work > before_work,
        "actual probe operations are billed"
    );
}

#[test]
fn early_probe_pins_the_exact_cached_block_and_readmits_its_real_source() {
    let (plan, mut provider) = fixture();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
    let key = seed(
        &mut renderer,
        &mut provider,
        &mut graph,
        AudioSample(0)..AudioSample(64),
        control,
    );
    seed(
        &mut renderer,
        &mut provider,
        &mut graph,
        AudioSample(1)..AudioSample(65),
        control,
    );
    let slot = graph
        .original_inputs
        .entries
        .iter()
        .position(|entry| entry.as_ref().is_some_and(|entry| entry.key == key))
        .unwrap();
    assert_eq!(graph.original_inputs.least_recent, Some(slot));
    let expected = Arc::as_ptr(&renderer.original_inputs[&key].block);
    let calls = provider.calls;
    let before_work = work.borrow().plan_work;
    let block = probe(&renderer, &mut provider, &mut graph, ADDITIONAL, control)
        .unwrap()
        .unwrap();
    assert_eq!(Arc::as_ptr(&block), expected);
    assert_eq!(Arc::strong_count(&block), 2);
    assert_eq!(graph.original_inputs.most_recent, Some(slot));
    assert_eq!(block.start, AudioSample(0));
    assert_eq!(block.samples.len(), 64);
    assert!(provider.calls > calls);
    assert!(work.borrow().plan_work > before_work);
    drop(block);
    assert_eq!(Arc::strong_count(&renderer.original_inputs[&key].block), 1);
    graph.original_inputs.clear(&mut renderer);
    assert_eq!(renderer.cached_resident_frames(), 0);
}

#[test]
fn early_probe_respects_exact_future_reservation_with_actual_live_owner_metadata() {
    let (plan, mut provider) = fixture();
    let cancelled = AtomicBool::new(false);
    let measurement_work = RefCell::new(ReadWork::default());
    let measurement_control = control(&cancelled, &measurement_work);
    let mut measurement = StageAudio::new(Arc::clone(&plan));
    let mut measurement_graph = graph(&plan);
    seed(
        &mut measurement,
        &mut provider,
        &mut measurement_graph,
        AudioSample(0)..AudioSample(64),
        measurement_control,
    );
    seed(
        &mut measurement,
        &mut provider,
        &mut measurement_graph,
        AudioSample(1)..AudioSample(65),
        measurement_control,
    );
    let cached = measurement.cached_resident_frames();
    assert_eq!(measurement.cached_stage_count(), 0);
    let owners = {
        let mut admission =
            sound_context_admission::Admission::new(&mut measurement, measurement_control);
        Context::Root
            .owners_admitted(
                &plan,
                AudioSample(0)..AudioSample(64),
                measurement_control,
                &mut admission,
            )
            .unwrap()
    };
    let metadata = measurement_work.borrow().sound_context_frames;
    assert!(metadata > 0);
    assert_eq!(owners.query().samples(), AudioSample(0)..AudioSample(64));
    drop(owners);
    assert_eq!(measurement_work.borrow().sound_context_frames, 0);
    measurement_graph.original_inputs.clear(&mut measurement);
    let full = cached
        .checked_add(metadata)
        .unwrap()
        .checked_add(ADDITIONAL)
        .unwrap();

    // Preserve a separate cold-path assertion: at the final retained-state
    // ceiling, constructing the real query before the second source read can
    // force the optional PCM cache to shed an entry for read-time headroom.
    let mut cold_renderer = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_resident_frames: u32::try_from(full).unwrap(),
            ..StageLimits::default()
        },
    )
    .unwrap();
    let cold_work = RefCell::new(ReadWork::default());
    let cold_control = control(&cancelled, &cold_work);
    let mut cold_graph = graph(&plan);
    let cold_owners = {
        let mut admission =
            sound_context_admission::Admission::new(&mut cold_renderer, cold_control);
        Context::Root
            .owners_admitted(
                &plan,
                AudioSample(0)..AudioSample(64),
                cold_control,
                &mut admission,
            )
            .unwrap()
    };
    seed(
        &mut cold_renderer,
        &mut provider,
        &mut cold_graph,
        AudioSample(0)..AudioSample(64),
        cold_control,
    );
    seed(
        &mut cold_renderer,
        &mut provider,
        &mut cold_graph,
        AudioSample(1)..AudioSample(65),
        cold_control,
    );
    assert_eq!(cold_work.borrow().sound_context_frames, metadata);
    assert!(cold_renderer.cached_resident_frames() < cached);
    drop(cold_owners);
    cold_graph.original_inputs.clear(&mut cold_renderer);
    assert_eq!(cold_renderer.cached_resident_frames(), 0);

    for (ceiling, hit) in [(full, true), (full - 1, false)] {
        // Warm a valid two-entry cache and live owner query before imposing
        // the probe budget. Cold processing-query construction has a larger
        // temporary peak and can evict optional entries under this ceiling.
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        let mut graph = graph(&plan);
        // Admit the real owner query before seeding optional payloads. Its
        // construction peak can legitimately evict an older cache entry.
        let owners = {
            let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
            Context::Root
                .owners_admitted(
                    &plan,
                    AudioSample(0)..AudioSample(64),
                    control,
                    &mut admission,
                )
                .unwrap()
        };
        assert_eq!(work.borrow().sound_context_frames, metadata);
        let key = seed(
            &mut renderer,
            &mut provider,
            &mut graph,
            AudioSample(0)..AudioSample(64),
            control,
        );
        seed(
            &mut renderer,
            &mut provider,
            &mut graph,
            AudioSample(1)..AudioSample(65),
            control,
        );
        assert_eq!(renderer.cached_resident_frames(), cached);
        renderer.limits.maximum_resident_frames = u32::try_from(ceiling).unwrap();
        if hit {
            let calls = provider.calls;
            let expected = Arc::as_ptr(&renderer.original_inputs[&key].block);
            let block = probe(&renderer, &mut provider, &mut graph, ADDITIONAL, control)
                .unwrap()
                .unwrap();
            assert_eq!(Arc::as_ptr(&block), expected);
            assert!(provider.calls > calls);
            drop(block);
        } else {
            declined_without_side_effects(
                &renderer,
                &mut provider,
                &mut graph,
                ADDITIONAL,
                control,
            );
        }
        assert_eq!(work.borrow().sound_context_frames, metadata);
        drop(owners);
        assert_eq!(work.borrow().sound_context_frames, 0);
        graph.original_inputs.clear(&mut renderer);
        assert_eq!(renderer.cached_resident_frames(), 0);
    }
}

#[test]
fn early_probe_checks_active_projected_and_disk_ledger_terms_without_eviction() {
    let (plan, mut provider) = fixture();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
    seed(
        &mut renderer,
        &mut provider,
        &mut graph,
        AudioSample(0)..AudioSample(64),
        control,
    );
    renderer.limits.maximum_resident_frames =
        u32::try_from(renderer.cached_resident_frames() + ADDITIONAL).unwrap();

    // Explicit private-ledger contract cases, not physical allocation claims.
    renderer.active_frames = 1;
    declined_without_side_effects(&renderer, &mut provider, &mut graph, ADDITIONAL, control);
    renderer.active_frames = 0;
    work.borrow_mut().projected_resident_frames = 1;
    declined_without_side_effects(&renderer, &mut provider, &mut graph, ADDITIONAL, control);
    work.borrow_mut().projected_resident_frames = 0;

    renderer.limits.maximum_disk_bytes = 1;
    renderer.active_disk_bytes = 2;
    declined_without_side_effects(&renderer, &mut provider, &mut graph, ADDITIONAL, control);
    renderer.active_disk_bytes = 0;
    work.borrow_mut().projected_disk_bytes = 2;
    declined_without_side_effects(&renderer, &mut provider, &mut graph, ADDITIONAL, control);
    work.borrow_mut().projected_disk_bytes = 0;
    drop(
        probe(&renderer, &mut provider, &mut graph, ADDITIONAL, control)
            .unwrap()
            .unwrap(),
    );
    graph.original_inputs.clear(&mut renderer);
}

#[test]
fn early_probe_counts_real_required_memory_and_file_outputs_before_pinning() {
    for disk in [false, true] {
        let (plan, mut provider) = fixture();
        let mut renderer = StageAudio::with_limits(
            Arc::clone(&plan),
            StageLimits {
                maximum_memory_stage_frames: if disk { 1 } else { 1024 * 1024 },
                ..StageLimits::default()
            },
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        // The real fixture's Preserve starts after Source512 + RoomTone128.
        drop(
            renderer
                .read_controlled(&mut provider, AudioSample(640), 64, control, true, 0)
                .unwrap(),
        );
        assert!(!renderer.cache.is_empty());
        let required_resident = renderer
            .cache
            .iter()
            .map(|entry| entry.block.samples.resident_frames())
            .sum::<u64>();
        let required_disk = renderer
            .cache
            .iter()
            .map(|entry| entry.block.samples.disk_bytes())
            .sum::<u64>();
        assert!(if disk {
            required_disk > 0
        } else {
            required_resident > 0
        });
        let mut graph = graph(&plan);
        seed(
            &mut renderer,
            &mut provider,
            &mut graph,
            AudioSample(0)..AudioSample(64),
            control,
        );
        let current = renderer.cached_resident_frames()
            + renderer.active_frames
            + work.borrow().projected_resident_frames
            + work.borrow().sound_context_frames;
        renderer.limits.maximum_resident_frames = u32::try_from(current + ADDITIONAL).unwrap();
        drop(
            probe(&renderer, &mut provider, &mut graph, ADDITIONAL, control)
                .unwrap()
                .unwrap(),
        );
        if disk {
            renderer.limits.maximum_disk_bytes = required_disk - 1;
        } else {
            renderer.limits.maximum_resident_frames -= 1;
        }
        declined_without_side_effects(&renderer, &mut provider, &mut graph, ADDITIONAL, control);
        graph.original_inputs.clear(&mut renderer);
    }
}

#[test]
fn early_probe_leaves_absent_and_stale_candidates_for_the_late_cold_path() {
    let (plan, mut provider) = fixture();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
    let first = seed(
        &mut renderer,
        &mut provider,
        &mut graph,
        AudioSample(0)..AudioSample(64),
        control,
    );
    seed(
        &mut renderer,
        &mut provider,
        &mut graph,
        AudioSample(1)..AudioSample(65),
        control,
    );
    let before = cache_snapshot(&graph.original_inputs);
    let calls = provider.calls;
    assert!(
        renderer
            .try_cached_original_for_prediction(
                &mut provider,
                &mut graph,
                &Context::Root,
                (AudioSample(2)..AudioSample(66), ADDITIONAL),
                control,
                0,
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(cache_snapshot(&graph.original_inputs), before);
    assert_eq!(provider.calls, calls);
    renderer.remove_original_input(&first);
    declined_without_side_effects(&renderer, &mut provider, &mut graph, ADDITIONAL, control);
    assert_eq!(graph.original_inputs.len, 2);
    drop(
        renderer
            .shared_original_input(
                &mut provider,
                &mut graph,
                &Context::Root,
                AudioSample(0)..AudioSample(64),
                control,
                0,
            )
            .unwrap(),
    );
    assert!(
        !graph
            .original_inputs
            .entries
            .iter()
            .flatten()
            .any(|entry| entry.key == first)
    );
    assert_eq!(graph.original_inputs.len, 2);
    assert!(provider.calls > calls);
    graph.original_inputs.clear(&mut renderer);
}

#[test]
fn early_probe_rejects_source_cancel_depth_and_spoofed_keys_without_leaking_pins() {
    struct Unavailable(usize);
    impl AudioSourceProvider for Unavailable {
        fn source(
            &mut self,
            _: &ProjectId,
            _: &RevisionId,
            _: &AssetId,
            _: &AtomicBool,
        ) -> Result<&crate::PreparedSource, PreparationError> {
            self.0 += 1;
            Err(PreparationError::SourceUnavailable("removed".into()))
        }
    }
    let (plan, mut provider) = fixture();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
    let key = seed(
        &mut renderer,
        &mut provider,
        &mut graph,
        AudioSample(0)..AudioSample(64),
        control,
    );
    let mut unavailable = Unavailable(0);
    assert!(matches!(
        probe(&renderer, &mut unavailable, &mut graph, ADDITIONAL, control),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
    assert_eq!(unavailable.0, 1);
    assert_eq!(Arc::strong_count(&renderer.original_inputs[&key].block), 1);

    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    let calls = provider.calls;
    assert!(matches!(
        probe(&renderer, &mut provider, &mut graph, ADDITIONAL, control),
        Err(error) if error.is_cancelled()
    ));
    assert_eq!(provider.calls, calls);
    assert_eq!(Arc::strong_count(&renderer.original_inputs[&key].block), 1);
    cancelled.store(false, std::sync::atomic::Ordering::Relaxed);

    Arc::get_mut(&mut renderer.original_inputs.get_mut(&key).unwrap().block)
        .unwrap()
        .relative_depth = renderer.limits.maximum_depth + 1;
    assert!(matches!(
        probe(&renderer, &mut provider, &mut graph, ADDITIONAL, control),
        Err(StageAudioError::Limit("nested stage depth"))
    ));
    assert_eq!(provider.calls, calls);
    assert_eq!(Arc::strong_count(&renderer.original_inputs[&key].block), 1);
    Arc::get_mut(&mut renderer.original_inputs.get_mut(&key).unwrap().block)
        .unwrap()
        .relative_depth = 0;

    let slot = graph
        .original_inputs
        .entries
        .iter()
        .position(|entry| entry.as_ref().is_some_and(|entry| entry.key == key))
        .unwrap();
    for depth_spoof in [false, true] {
        let entry = graph.original_inputs.entries[slot].as_mut().unwrap();
        if depth_spoof {
            entry.depth = 1;
        } else {
            entry.samples = AudioSample(1)..AudioSample(65);
        }
        assert!(matches!(
            probe(&renderer, &mut provider, &mut graph, ADDITIONAL, control),
            Err(StageAudioError::ForeignDomain)
        ));
        assert_eq!(provider.calls, calls);
        assert_eq!(Arc::strong_count(&renderer.original_inputs[&key].block), 1);
        let entry = graph.original_inputs.entries[slot].as_mut().unwrap();
        entry.depth = 0;
        entry.samples = AudioSample(0)..AudioSample(64);
    }
    graph.original_inputs.clear(&mut renderer);
}
