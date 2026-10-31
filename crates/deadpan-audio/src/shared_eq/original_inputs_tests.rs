use super::*;
use crate::stages::controlled_reads::{Provider, fixture_document_at_rate};
use deadpan_core::FrameRate;

#[path = "cached_probe_tests.rs"]
mod cached_probe;
#[path = "prepared_input_tests.rs"]
mod prepared;

fn fixture() -> (Arc<RenderPlan>, Provider) {
    let (document, provider) = fixture_document_at_rate(FrameRate::new(48_000, 1).unwrap());
    (Arc::new(RenderPlan::compile(&document).unwrap()), provider)
}

fn graph<'plan, 'work>(plan: &'plan RenderPlan) -> Graph<'plan, 'work> {
    Graph {
        plan,
        nodes: Vec::new(),
        resident: 0,
        node_capacity_frames: 0,
        wet_filters: std::array::from_fn(|_| None),
        event_tiles: BTreeMap::new(),
        event_nodes: Default::default(),
        event_filters: BTreeMap::new(),
        voices: BTreeMap::new(),
        permission_families: BTreeMap::new(),
        sends: None,
        skip: Vec::new(),
        original_only: false,
        original_inputs: Cache::default(),
    }
}

fn control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work,
    }
}

#[test]
fn exact_original_input_reuse_rechecks_sources_and_preserves_pcm_and_masks() {
    let (plan, mut provider) = fixture();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
    let first = renderer
        .shared_original_input(
            &mut provider,
            &mut graph,
            &Context::Root,
            AudioSample(0)..AudioSample(64),
            control,
            0,
        )
        .unwrap();
    let first_work = work.borrow().plan_work;
    let calls = provider.calls;
    let second = renderer
        .shared_original_input(
            &mut provider,
            &mut graph,
            &Context::Root,
            AudioSample(0)..AudioSample(64),
            control,
            0,
        )
        .unwrap();
    assert!(provider.calls > calls, "hit must re-admit its source");
    assert!(work.borrow().plan_work - first_work < first_work);
    let (Read::Cached(first), Read::Cached(second)) = (&first, &second) else {
        panic!("fixture must exercise an actual cache hit")
    };
    assert!(Arc::ptr_eq(first, second));
    assert_eq!(first.start, second.start);
    assert_eq!(first.suppressed, second.suppressed);
    assert_eq!(first.exhausted, second.exhausted);
    assert_eq!(first.dependencies, second.dependencies);
    assert_eq!(first.relative_depth, second.relative_depth);
    let reference = renderer
        .read_controlled(&mut provider, AudioSample(0), 64, control, true, 0)
        .unwrap();
    assert_eq!(first.samples, reference.samples);
    assert_eq!(first.suppressed, reference.suppressed);
    assert_eq!(first.exhausted, reference.exhausted);
    assert_eq!(first.dependencies, reference.dependencies);
    graph.original_inputs.clear(&mut renderer);
    assert!(renderer.original_inputs.is_empty());
}

#[test]
fn original_input_optional_cache_respects_live_owner_query_metadata_budget() {
    let (plan, mut provider) = fixture();
    let cancelled = AtomicBool::new(false);
    let samples = AudioSample(0)..AudioSample(64);

    // Measure the actual optional payload charge and keep its PCM as the
    // ample-budget reference. The fixture reads a real WAV-backed Source.
    let mut measurement_renderer = StageAudio::new(Arc::clone(&plan));
    let measurement_work = RefCell::new(ReadWork::default());
    let measurement_control = control(&cancelled, &measurement_work);
    let mut measurement_graph = graph(&plan);
    assert_eq!(measurement_renderer.cached_resident_frames(), 0);
    let reference = measurement_renderer
        .shared_original_input(
            &mut provider,
            &mut measurement_graph,
            &Context::Root,
            samples.clone(),
            measurement_control,
            0,
        )
        .unwrap();
    assert!(matches!(reference, Read::Cached(_)));
    assert_eq!(measurement_graph.original_inputs.len, 1);
    assert_eq!(measurement_renderer.original_inputs.len(), 1);
    assert_eq!(measurement_renderer.cached_stage_count(), 0);
    let pcm_resident = measurement_renderer
        .original_inputs
        .values()
        .next()
        .unwrap()
        .resident;
    assert!(pcm_resident > 0);
    assert_eq!(measurement_renderer.cached_resident_frames(), pcm_resident);
    assert_eq!(measurement_renderer.active_frames, 0);
    assert_eq!(measurement_work.borrow().projected_resident_frames, 0);
    assert_eq!(measurement_work.borrow().sound_context_frames, 0);
    measurement_graph
        .original_inputs
        .clear(&mut measurement_renderer);
    assert_eq!(measurement_renderer.cached_resident_frames(), 0);

    // The adapter's renderer borrow ends here, while the real admitted query
    // keeps its lease alive until the explicit drop below.
    let owners = {
        let mut admission =
            sound_context_admission::Admission::new(&mut measurement_renderer, measurement_control);
        Context::Root
            .owners_admitted(&plan, samples.clone(), measurement_control, &mut admission)
            .unwrap()
    };
    assert_eq!(owners.query().samples(), samples);
    assert!(!owners.query().spans().is_empty());
    let metadata_resident = measurement_work.borrow().sound_context_frames;
    assert!(metadata_resident > 0);
    assert_eq!(measurement_renderer.cached_resident_frames(), 0);
    assert_eq!(measurement_renderer.active_frames, 0);
    drop(owners);
    assert_eq!(measurement_work.borrow().sound_context_frames, 0);

    let total = pcm_resident.checked_add(metadata_resident).unwrap();
    // Cold reading constructs processing metadata while the separate owner
    // lease is live. The publication ceiling does not cover that earlier peak.
    let mut cold_renderer = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_resident_frames: u32::try_from(total).unwrap(),
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
        Context::Root.owners_admitted(&plan, samples.clone(), cold_control, &mut admission)
    }
    .unwrap();
    assert_eq!(cold_work.borrow().sound_context_frames, metadata_resident);
    let cold_result = cold_renderer.shared_original_input(
        &mut provider,
        &mut cold_graph,
        &Context::Root,
        samples.clone(),
        cold_control,
        0,
    );
    assert!(matches!(
        cold_result,
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    assert_eq!(cold_work.borrow().sound_context_frames, metadata_resident);
    assert_eq!(cold_renderer.cached_resident_frames(), 0);
    assert_eq!(cold_renderer.cached_stage_count(), 0);
    drop(cold_owners);
    assert_eq!(cold_work.borrow().sound_context_frames, 0);

    // Hold a real owner lease while measuring optional publication. Establish
    // both allocations at ample capacity, then apply the exact/one-short
    // retained-state ceiling before the cache decision.
    for (ceiling, should_cache) in [(total, true), (total.checked_sub(1).unwrap(), false)] {
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        let mut graph = graph(&plan);
        let owners = {
            let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
            Context::Root
                .owners_admitted(&plan, samples.clone(), control, &mut admission)
                .unwrap()
        };
        assert_eq!(owners.query().samples(), samples);
        assert!(!owners.query().spans().is_empty());
        assert_eq!(work.borrow().sound_context_frames, metadata_resident);
        assert_eq!(renderer.cached_resident_frames(), 0);
        assert_eq!(renderer.active_frames, 0);
        assert_eq!(work.borrow().projected_resident_frames, 0);
        let calls = provider.calls;
        let source_checks = work.borrow().source_checks;
        let raw = renderer
            .read_controlled(&mut provider, samples.start, 64, control, true, 0)
            .unwrap();
        assert!(
            provider.calls > calls,
            "completed read must admit its real source"
        );
        assert!(work.borrow().source_checks > source_checks);
        let calls = provider.calls;
        let source_checks = work.borrow().source_checks;
        renderer.limits.maximum_resident_frames = u32::try_from(ceiling).unwrap();
        let lookup = LookupKey::new(&Context::Root, &samples, 0);
        let raw = renderer
            .cache_original_input(
                &mut graph,
                &Context::Root,
                (samples.clone(), 0, lookup),
                raw,
                control,
            )
            .unwrap();
        assert_eq!(provider.calls, calls);
        assert_eq!(work.borrow().source_checks, source_checks);
        assert_eq!(raw.start, reference.start);
        assert_eq!(raw.samples, reference.samples);
        assert_eq!(raw.suppressed, reference.suppressed);
        assert_eq!(raw.exhausted, reference.exhausted);
        assert_eq!(raw.dependencies, reference.dependencies);
        assert_eq!(raw.relative_depth, reference.relative_depth);
        assert_eq!(work.borrow().sound_context_frames, metadata_resident);
        let cached = renderer.cached_resident_frames();
        let active = renderer.active_frames;
        let projected = work.borrow().projected_resident_frames;
        let aggregate = cached
            .checked_add(active)
            .and_then(|frames| frames.checked_add(projected))
            .and_then(|frames| frames.checked_add(metadata_resident))
            .unwrap();
        assert!(
            aggregate <= ceiling,
            "completed Original read exceeded residency: cached={cached}, active={active}, \
             projected={projected}, live_metadata={metadata_resident}, \
             candidate_pcm={pcm_resident}, aggregate={aggregate}, cap={ceiling}"
        );
        assert_eq!(matches!(raw, Read::Cached(_)), should_cache);
        assert_eq!(matches!(raw, Read::Fresh(_)), !should_cache);
        assert_eq!(graph.original_inputs.len, usize::from(should_cache));
        assert_eq!(renderer.original_inputs.len(), usize::from(should_cache));
        assert_eq!(cached, if should_cache { pcm_resident } else { 0 });
        assert_eq!(active, 0);
        assert_eq!(projected, 0);
        drop(raw);
        drop(owners);
        assert_eq!(work.borrow().sound_context_frames, 0);
        graph.original_inputs.clear(&mut renderer);
        assert_eq!(graph.original_inputs.len, 0);
        assert!(renderer.original_inputs.is_empty());
        assert_eq!(renderer.cached_resident_frames(), 0);
        assert_eq!(renderer.active_frames, 0);
        assert_eq!(work.borrow().projected_resident_frames, 0);
    }
}

#[test]
fn original_input_cache_shares_pressure_with_required_allocations_and_respects_pins() {
    let (plan, mut provider) = fixture();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
    let raw = renderer
        .shared_original_input(
            &mut provider,
            &mut graph,
            &Context::Root,
            AudioSample(0)..AudioSample(64),
            control,
            0,
        )
        .unwrap();
    assert!(renderer.cached_resident_frames() > 64);
    let ceiling = u64::from(renderer.limits.maximum_resident_frames);
    assert!(matches!(
        renderer.make_room(ceiling, false, control),
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    assert_eq!(renderer.original_inputs.len(), 1);
    drop(raw);
    renderer.make_room(ceiling, false, control).unwrap();
    assert!(renderer.original_inputs.is_empty());
    assert_eq!(renderer.cached_resident_frames(), 0);
    // The stale key cannot provide old PCM after generic pressure eviction.
    let raw = renderer
        .shared_original_input(
            &mut provider,
            &mut graph,
            &Context::Root,
            AudioSample(0)..AudioSample(64),
            control,
            0,
        )
        .unwrap();
    assert_eq!(graph.original_inputs.len, 1);
    assert_eq!(renderer.original_inputs.len(), 1);
    drop(raw);
    graph.original_inputs.clear(&mut renderer);
    assert_eq!(renderer.cached_resident_frames(), 0);
    assert_eq!(renderer.active_frames, 0);
}

#[test]
fn original_input_hit_refuses_unavailable_source_and_cancellation() {
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
    let mut unavailable = Unavailable(0);
    assert!(
        renderer
            .shared_original_input(
                &mut unavailable,
                &mut graph,
                &Context::Root,
                AudioSample(0)..AudioSample(64),
                control,
                0,
            )
            .is_err()
    );
    assert_eq!(unavailable.0, 1);
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    let calls = provider.calls;
    assert!(
        renderer
            .shared_original_input(
                &mut provider,
                &mut graph,
                &Context::Root,
                AudioSample(0)..AudioSample(64),
                control,
                0,
            )
            .is_err()
    );
    assert_eq!(provider.calls, calls);
    graph.original_inputs.clear(&mut renderer);
    assert_eq!(renderer.cached_resident_frames(), 0);
}

#[test]
fn original_input_identity_keeps_ranges_depths_and_controllers_separate() {
    let (plan, mut provider) = fixture();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
    for (start, depth) in [(0, 0), (1, 0), (0, 1)] {
        drop(
            renderer
                .shared_original_input(
                    &mut provider,
                    &mut graph,
                    &Context::Root,
                    AudioSample(start)..AudioSample(start + 64),
                    control,
                    depth,
                )
                .unwrap(),
        );
    }
    assert_eq!(renderer.original_inputs.len(), 3);
    assert_eq!(graph.original_inputs.len, 3);
    let mut other = StageAudio::new(Arc::clone(&plan));
    let calls = provider.calls;
    assert!(matches!(
        other.shared_original_input(
            &mut provider,
            &mut graph,
            &Context::Root,
            AudioSample(0)..AudioSample(64),
            control,
            0,
        ),
        Err(StageAudioError::ForeignDomain)
    ));
    assert_eq!(provider.calls, calls);
    assert!(other.original_inputs.is_empty());
    graph.original_inputs.clear(&mut renderer);
    assert!(renderer.original_inputs.is_empty());
}

#[test]
fn original_input_hit_refuses_changed_real_source_provenance() {
    use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
    use deadpan_media::source_index::SourceContentIdentity;
    use sha2::{Digest, Sha256};
    use std::io::Cursor;

    let (plan, mut provider) = fixture();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
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
    let mut bytes = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav"),
    )
    .unwrap();
    let mut at = 12;
    loop {
        let length = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        if &bytes[at..at + 4] == b"data" {
            assert!(length > 0);
            bytes[at + 8] ^= 1;
            break;
        }
        at += 8 + length + length % 2;
    }
    let session = AudioSession::open_verified(
        &mut Cursor::new(&bytes),
        SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64).unwrap(),
        0,
        AudioSessionLimits::default(),
        &cancelled,
    )
    .unwrap();
    let index = session.index().clone();
    let changed = crate::PreparedSource::with_layout(
        session,
        &index,
        provider.prepared.matrix_layout(),
        &cancelled,
    )
    .unwrap();
    assert_ne!(changed.provenance(), provider.prepared.provenance());
    provider.prepared = changed;
    assert!(matches!(
        renderer.shared_original_input(
            &mut provider,
            &mut graph,
            &Context::Root,
            AudioSample(0)..AudioSample(64),
            control,
            0,
        ),
        Err(StageAudioError::Preparation(
            PreparationError::IndexMismatch
        ))
    ));
    graph.original_inputs.clear(&mut renderer);
    assert!(renderer.original_inputs.is_empty());
    assert_eq!(renderer.active_frames, 0);
}

#[test]
fn original_input_domain_reuse_requires_the_same_retained_allocation() {
    let (plan, mut provider) = fixture();
    let owners = plan
        .audio_gain_owners(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let owner = owners.spans()[0].owners().last().unwrap();
    let input = plan
        .audio_owner_input(owner, AudioSample(0), Default::default())
        .unwrap();
    let domain = Arc::new(input.context().clone());
    let contexts = [
        Context::Domain(Arc::clone(&domain)),
        Context::Domain(Arc::clone(&domain)),
        Context::Domain(Arc::new((*domain).clone())),
    ];
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
    let mut reads = Vec::new();
    for context in &contexts {
        reads.push(
            renderer
                .shared_original_input(
                    &mut provider,
                    &mut graph,
                    context,
                    AudioSample(0)..AudioSample(64),
                    control,
                    0,
                )
                .unwrap(),
        );
    }
    let cached = |read: &Read| match read {
        Read::Cached(block) => Arc::as_ptr(block),
        Read::Fresh(_) => panic!("fixture must retain its input"),
    };
    assert_eq!(cached(&reads[0]), cached(&reads[1]));
    assert_ne!(cached(&reads[0]), cached(&reads[2]));
    assert_eq!(reads[0].samples, reads[2].samples);
    assert_eq!(renderer.original_inputs.len(), 2);
    drop(reads);
    graph.original_inputs.clear(&mut renderer);
    assert_eq!(renderer.cached_resident_frames(), 0);
}

#[test]
fn indexed_cache_keeps_exact_collision_checks_and_lru_removal_order() {
    let (plan, mut provider) = fixture();
    let owners = plan
        .audio_gain_owners(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let owner = owners.spans()[0].owners().last().unwrap();
    let input = plan
        .audio_owner_input(owner, AudioSample(0), Default::default())
        .unwrap();
    let domain = Arc::new(input.context().clone());
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut cache = Cache::default();

    // Force an accelerator-key collision. A hit still has to validate the
    // retained full Context before it can return a cached block.
    let forced = LookupKey::new(&Context::Root, &(AudioSample(0)..AudioSample(64)), 0);
    cache
        .insert(
            Entry {
                context: Context::Domain(Arc::clone(&domain)),
                samples: AudioSample(0)..AudioSample(64),
                depth: 0,
                key: 1,
                lookup: forced,
                hash_previous: None,
                hash_next: None,
                previous: None,
                next: None,
            },
            control,
        )
        .unwrap();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut corrupted = graph(&plan);
    corrupted.original_inputs = cache;
    let source_calls = provider.calls;
    assert!(matches!(
        renderer.shared_original_input(
            &mut provider,
            &mut corrupted,
            &Context::Root,
            AudioSample(0)..AudioSample(64),
            control,
            0,
        ),
        Err(StageAudioError::ForeignDomain)
    ));
    assert_eq!(provider.calls, source_calls);
    assert!(renderer.original_inputs.is_empty());
    assert_eq!(renderer.cached_resident_frames(), 0);
    assert_eq!(renderer.active_frames, 0);
    corrupted.original_inputs.clear(&mut renderer);
    assert_eq!(corrupted.original_inputs.len, 0);
    let mut cache = Cache::default();

    let mut second_slot = None;
    for (index, start) in [(0, 0), (1, 64), (2, 128)] {
        let samples = AudioSample(start)..AudioSample(start + 64);
        let context = Context::Root;
        let slot = cache
            .insert(
                Entry {
                    context: context.clone(),
                    lookup: LookupKey::new(&context, &samples, 0),
                    samples,
                    depth: 0,
                    key: index as u64 + 10,
                    hash_previous: None,
                    hash_next: None,
                    previous: None,
                    next: None,
                },
                control,
            )
            .unwrap();
        if index == 1 {
            second_slot = Some(slot);
        }
    }
    let hit = cache
        .find(
            LookupKey::new(&Context::Root, &(AudioSample(0)..AudioSample(64)), 0),
            control,
        )
        .unwrap()
        .unwrap();
    cache.touch(hit);
    assert_eq!(cache.least_recent, second_slot);
    assert_eq!(cache.remove(second_slot.unwrap(), control).unwrap().key, 11);
    assert_eq!(cache.least_recent, Some(2));
    assert_eq!(cache.most_recent, Some(0));
    assert_eq!(cache.len, 2);
    let replacement_samples = AudioSample(192)..AudioSample(256);
    let replacement_context = Context::Root;
    let reused = cache
        .insert(
            Entry {
                context: replacement_context.clone(),
                lookup: LookupKey::new(&replacement_context, &replacement_samples, 0),
                samples: replacement_samples,
                depth: 0,
                key: 99,
                hash_previous: None,
                hash_next: None,
                previous: None,
                next: None,
            },
            control,
        )
        .unwrap();
    assert_eq!(Some(reused), second_slot);
    assert_eq!(cache.most_recent, Some(reused));
    assert_eq!(cache.len, 3);
    assert!(
        Cache::resident_frames().unwrap() >= std::mem::size_of::<Cache<'_>>().div_ceil(8) as u64
    );
}

#[test]
fn fixed_lookup_storage_is_reserved_before_audio_graph_admission() {
    let (plan, mut provider) = fixture();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    renderer.limits.maximum_resident_frames = 256;
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let before = provider.calls;
    let result = renderer.read_shared_input(
        &mut provider,
        AudioSample(0),
        64,
        control(&cancelled, &work),
        InputSelection::default(),
    );
    assert!(matches!(
        result,
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    assert_eq!(provider.calls, before);
    assert_eq!(renderer.active_frames, 0);
    assert_eq!(renderer.cached_resident_frames(), 0);
}

fn indexed_entry(key: LookupKey, id: u64) -> Entry<'static> {
    Entry {
        context: Context::Root,
        samples: AudioSample(key.start)..AudioSample(key.end),
        depth: key.depth,
        key: id,
        lookup: key,
        hash_previous: None,
        hash_next: None,
        previous: None,
        next: None,
    }
}

fn colliding_keys(count: usize) -> Vec<LookupKey> {
    let first = LookupKey::new(&Context::Root, &(AudioSample(0)..AudioSample(1)), 0);
    let result: Vec<_> = (0..1_000_000)
        .map(|start| {
            LookupKey::new(
                &Context::Root,
                &(AudioSample(start)..AudioSample(start + 1)),
                0,
            )
        })
        .filter(|key| key.bucket() == first.bucket())
        .take(count)
        .collect();
    assert_eq!(result.len(), count);
    result
}

fn cache_snapshot(cache: &Cache<'_>) -> String {
    let entries: Vec<_> = cache
        .entries
        .iter()
        .map(|entry| {
            entry.as_ref().map(|entry| {
                (
                    entry.key,
                    entry.lookup,
                    entry.hash_previous,
                    entry.hash_next,
                    entry.previous,
                    entry.next,
                )
            })
        })
        .collect();
    format!(
        "{:?}",
        (
            cache.index,
            cache.free_slots,
            cache.free_len,
            cache.len,
            cache.most_recent,
            cache.least_recent,
            entries
        )
    )
}

fn assert_index_consistent(cache: &Cache<'_>, control: WorkControl<'_>) {
    let mut hashed = BTreeSet::new();
    for bucket in 0..CAPACITY {
        let mut previous = None;
        let mut candidate = cache.index[bucket];
        while let Some(slot) = candidate {
            assert!(hashed.insert(slot), "duplicate slot or hash cycle");
            let entry = cache.entries[slot].as_ref().unwrap();
            assert_eq!(entry.lookup.bucket(), bucket);
            assert_eq!(entry.hash_previous, previous);
            assert_eq!(cache.find(entry.lookup, control).unwrap(), Some(slot));
            previous = Some(slot);
            candidate = entry.hash_next;
        }
    }
    let mut lru = BTreeSet::new();
    let mut previous = None;
    let mut candidate = cache.most_recent;
    while let Some(slot) = candidate {
        assert!(lru.insert(slot), "LRU cycle");
        let entry = cache.entries[slot].as_ref().unwrap();
        assert_eq!(entry.previous, previous);
        previous = Some(slot);
        candidate = entry.next;
    }
    assert_eq!(cache.least_recent, previous);
    assert_eq!(hashed, lru);
    assert_eq!(hashed.len(), cache.len);
    assert_eq!(cache.len + cache.free_len, CAPACITY);
    let free: BTreeSet<_> = cache.free_slots[..cache.free_len].iter().copied().collect();
    assert_eq!(free.len(), cache.free_len);
    assert!(free.is_disjoint(&hashed));
    assert_eq!(free.len() + hashed.len(), CAPACITY);
}

#[test]
fn collision_bucket_unlinks_head_middle_tail_and_reuses_slots_without_lru_changes() {
    let keys = colliding_keys(CAPACITY + 1);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut cache = Cache::default();
    let mut slots = Vec::new();
    for (id, key) in keys[..CAPACITY].iter().enumerate() {
        slots.push(
            cache
                .insert(indexed_entry(*key, id as u64), control)
                .unwrap(),
        );
    }
    assert_index_consistent(&cache, control);
    let before = work.borrow().plan_work;
    assert_eq!(cache.find(keys[CAPACITY], control).unwrap(), None);
    assert_eq!(work.borrow().plan_work - before, HASH_WORK + CAPACITY);
    let buckets = cache.index;
    cache.touch(slots[0]);
    cache.touch(slots[CAPACITY / 2]);
    assert_eq!(cache.index, buckets);
    assert_index_consistent(&cache, control);

    // Bucket insertion is at the head; LRU touches above cannot alter that.
    for id in [CAPACITY - 1, CAPACITY / 2, 0] {
        let before = work.borrow().plan_work;
        assert_eq!(cache.remove(slots[id], control).unwrap().key, id as u64);
        assert_eq!(work.borrow().plan_work - before, HASH_WORK + LINK_WORK);
        assert_eq!(cache.find(keys[id], control).unwrap(), None);
        assert_index_consistent(&cache, control);
    }
    for id in [0, CAPACITY / 2, CAPACITY - 1] {
        assert_eq!(
            cache
                .insert(indexed_entry(keys[id], id as u64), control)
                .unwrap(),
            slots[id]
        );
        assert_index_consistent(&cache, control);
    }
    // Repeated full-table churn must neither lose a key nor accumulate tombstones.
    for _ in 0..4 {
        for (id, slot) in slots.iter().copied().enumerate() {
            assert_eq!(cache.remove(slot, control).unwrap().key, id as u64);
            assert_eq!(
                cache
                    .insert(indexed_entry(keys[id], id as u64), control)
                    .unwrap(),
                slot
            );
        }
        assert_index_consistent(&cache, control);
    }
}

#[test]
fn index_operations_precharge_exact_work_and_leave_state_unchanged_on_refusal() {
    let keys = colliding_keys(3);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut cache = Cache::default();
    let first = cache.insert(indexed_entry(keys[0], 0), control).unwrap();
    let second = cache.insert(indexed_entry(keys[1], 1), control).unwrap();
    let before = cache_snapshot(&cache);
    let amount = HASH_WORK + LINK_WORK;
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - amount + 1;
    assert!(matches!(
        cache.remove(first, control),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(cache_snapshot(&cache), before);
    assert!(matches!(
        cache.insert(indexed_entry(keys[2], 2), control),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(cache_snapshot(&cache), before);
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - amount;
    assert_eq!(cache.remove(first, control).unwrap().key, 0);
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - amount;
    assert_eq!(
        cache.insert(indexed_entry(keys[0], 0), control).unwrap(),
        first
    );
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    work.borrow_mut().plan_work = 0;
    assert_index_consistent(&cache, control);
    let before = cache_snapshot(&cache);
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(cache.find(keys[0], control).is_err());
    assert!(cache.remove(second, control).is_err());
    assert!(cache.insert(indexed_entry(keys[2], 2), control).is_err());
    assert_eq!(cache_snapshot(&cache), before);
}

#[test]
fn intrusive_index_storage_does_not_increase_the_admitted_fixed_footprint() {
    let old = std::mem::size_of::<[Option<(LookupKey, usize)>; CAPACITY]>();
    let new = std::mem::size_of::<[Option<usize>; CAPACITY]>()
        + CAPACITY * 2 * std::mem::size_of::<Option<usize>>();
    assert!(
        new <= old,
        "bucket heads plus both intrusive links must fit old index"
    );
    assert!(Cache::resident_frames().unwrap() * 8 >= std::mem::size_of::<Cache<'_>>() as u64);
}
