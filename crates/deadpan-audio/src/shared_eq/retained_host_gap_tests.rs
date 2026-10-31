//! Real cache factories and the frozen retained document, without synthetic payloads.
use super::*;
use crate::stages::retained_request;
use deadpan_core::{NodeId, ProjectDocument};
use serde_json::{Value, json};

fn retained_fixture() -> (Arc<RenderPlan>, Provider) {
    let (_, provider) = fixture();
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/retained-host-read-baseline-v1/snapshot.json"),
    )
    .unwrap();
    let frozen: Value = serde_json::from_slice(&bytes).unwrap();
    let case = &frozen["cases"][0];
    assert_eq!(case["name"], "root-resumed-signed-history");
    let document = ProjectDocument::from_json(&case["current_document"].to_string()).unwrap();
    (Arc::new(RenderPlan::compile(&document).unwrap()), provider)
}
fn owner_input(plan: &RenderPlan) -> deadpan_plan::AudioOwnerInput<'_> {
    let owners = plan
        .audio_gain_owners(AudioSample(512)..AudioSample(513), Default::default())
        .unwrap();
    let owner = owners.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node == NodeId::new("a").unwrap())
        .unwrap();
    plan.audio_owner_input(owner, AudioSample(512), Default::default())
        .unwrap()
}
fn reset_receipts() {
    original_request::flattened_work_trace::reset();
    retained_request::trace::reset();
}
fn receipts() -> Value {
    json!({
        "fade": retained_request::trace::fade_snapshot(),
        "owners": retained_request::trace::owners_snapshot(),
    })
}
fn assert_valid(receipts: &Value) {
    for product in ["fade", "owners"] {
        assert_eq!(receipts[product]["invalid"], false);
        assert_eq!(receipts[product]["overflow"], false);
        assert_eq!(receipts[product]["active_depth"], 0);
    }
}

#[test]
fn foreign_retained_plan_wins_over_depth_and_range_before_constructor_or_source_io() {
    let (plan, mut provider) = retained_fixture();
    let input = owner_input(&plan);
    let probe = input
        .retained_probe_at(AudioSample(512), Default::default())
        .unwrap();
    let retained = probe.input().unwrap();
    let document = {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/retained-host-read-baseline-v1/snapshot.json"),
        )
        .unwrap();
        let frozen: Value = serde_json::from_slice(&bytes).unwrap();
        ProjectDocument::from_json(&frozen["cases"][0]["current_document"].to_string()).unwrap()
    };
    let foreign = Arc::new(RenderPlan::compile(&document).unwrap());
    assert!(!retained.belongs_to(&foreign));
    let mut renderer = StageAudio::new(foreign);
    for (start, frames, depth) in [
        (AudioSample(-1536), 128, 0),
        (AudioSample(i64::MAX), 0, renderer.limits.maximum_depth + 1),
    ] {
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        reset_receipts();
        assert!(matches!(
            renderer.read_retained_controlled(
                &mut provider,
                retained,
                start,
                frames,
                control,
                (depth, true),
            ),
            Err(StageAudioError::ForeignDomain)
        ));
        assert_eq!(provider.calls, 0);
        let receipts = receipts();
        assert_valid(&receipts);
        for product in ["fade", "owners"] {
            assert_eq!(receipts[product]["plan"]["constructor_calls"], 0);
            assert_eq!(receipts[product]["cache"]["constructor_calls"], 0);
        }
        let work = work.borrow();
        assert_eq!(work.plan_work, 0);
        assert_eq!(work.cache_plan_work, 0);
        assert_eq!(work.source_checks, 0);
        assert_eq!(work.sound_context_frames, 0);
        assert!(work.observed.is_empty());
        assert!(work.preflight_assets.is_empty());
        assert_eq!(renderer.active_frames, 0);
        assert_eq!(renderer.active_disk_bytes, 0);
    }
}

#[test]
fn real_cached_wav_payloads_are_evicted_by_retained_fade_admission_with_separate_receipts() {
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
        let retained = caller.retained().expect("real retained sharing path");
        let caller_frames = work.borrow().sound_context_frames;
        assert!(caller_frames > 0);

        // Measure this exact constructor without cache pressure on a separate real ledger.
        let measurement = RefCell::new(ReadWork::default());
        let measured = super::control(&cancelled, &measurement);
        let query = renderer
            .prepare_retained_fades(retained, AudioSample(-1536)..AudioSample(-1408), measured)
            .unwrap();
        let measured_live = measurement.borrow().sound_context_frames;
        assert!(measured_live > 0);
        let expected_work = query.query().work;
        drop(query);
        assert_eq!(measurement.borrow().sound_context_frames, 0);

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
            let Read::Cached(cached) = cached else {
                panic!("actual cold WAV read must populate an evictable cache entry");
            };
            drop(cached);
        }
        let cached_count = renderer.original_inputs.len();
        assert_eq!(cached_count, 16);
        let cached_frames = renderer.cached_resident_frames();
        assert!(cached_frames > measured_live);
        assert_eq!(work.borrow().sound_context_frames, caller_frames);
        let limit = cached_frames.checked_add(caller_frames).unwrap();
        assert!(limit < u64::from(renderer.limits.maximum_resident_frames));
        renderer.limits.maximum_resident_frames = u32::try_from(limit).unwrap();
        let calls = provider.calls;
        let before = [work.borrow().plan_work, work.borrow().cache_plan_work];
        reset_receipts();
        let scope = cache_lane.then(|| control.cache_key_scope());
        let query = renderer
            .prepare_retained_fades(retained, AudioSample(-1536)..AudioSample(-1408), control)
            .unwrap();
        drop(scope);
        assert_eq!(
            provider.calls, calls,
            "metadata pressure does not reopen WAV"
        );
        let evicted = cached_count - renderer.original_inputs.len();
        assert!(
            evicted > 0,
            "real metadata reservation must evict a real unpinned WAV payload"
        );
        assert!(work.borrow().sound_context_frames > caller_frames);
        assert_eq!(query.query().work, expected_work);
        let trace = receipts();
        assert_valid(&trace);
        let lane = if cache_lane { "cache" } else { "plan" };
        let other = if cache_lane { "plan" } else { "cache" };
        let receipt = &trace["fade"][lane];
        assert_eq!(receipt["completed_queries"], 1);
        assert_eq!(receipt["failed_queries"], 0);
        assert_eq!(receipt["partial_work"], 0);
        assert_eq!(receipt["host_eviction_work"], evicted);
        assert_eq!(
            trace["fade"]["all_host_eviction_work"][usize::from(cache_lane)],
            evicted
        );
        let canonical = receipt["canonical_units"].as_u64().unwrap();
        let additional = receipt["additional_units"].as_u64().unwrap();
        assert_eq!(canonical + additional, query.query().work as u64);
        assert_eq!(
            receipt["accepted_work"].as_u64().unwrap(),
            canonical + additional + evicted as u64
        );
        assert_eq!(trace["fade"][other]["accepted_work"], 0);
        let after = [work.borrow().plan_work, work.borrow().cache_plan_work];
        let selected = usize::from(cache_lane);
        assert_eq!(
            after[selected] - before[selected],
            query.query().work + evicted
        );
        assert_eq!(after[1 - selected], before[1 - selected]);
        drop(query);
        assert_eq!(work.borrow().sound_context_frames, caller_frames);
        graph.original_inputs.clear(&mut renderer);
        drop(caller);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(renderer.cached_resident_frames(), 0);
        assert_eq!(renderer.active_frames, 0);
        assert_eq!(renderer.active_disk_bytes, 0);
    }
}

#[test]
fn real_retained_original_cache_hit_rechecks_source_and_bypasses_retained_construction() {
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
    let context = Context::Retained(caller.retained().expect("actual retained sharing").clone());
    let caller_frames = work.borrow().sound_context_frames;
    let mut graph = graph(&plan);
    let range = AudioSample(-1536)..AudioSample(-1408);
    reset_receipts();
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
    let Read::Cached(first) = first else {
        panic!("actual retained miss must publish a real cache payload");
    };
    assert_eq!(receipts()["fade"]["plan"]["completed_queries"], 1);
    assert_eq!(work.borrow().sound_context_frames, caller_frames);
    let calls = provider.calls;
    assert!(calls > 0);
    let published = renderer.next_original_input;
    let spent = work.borrow().plan_work;
    reset_receipts();
    let hit = renderer
        .shared_original_input(&mut provider, &mut graph, &context, range, control, 0)
        .unwrap();
    let Read::Cached(hit) = hit else {
        panic!("exact real retained key must reuse cached PCM");
    };
    assert!(Arc::ptr_eq(&first, &hit));
    assert!(
        provider.calls > calls,
        "real hit still re-admits current source provenance"
    );
    assert!(work.borrow().plan_work > spent);
    assert_eq!(renderer.next_original_input, published);
    assert_eq!(work.borrow().sound_context_frames, caller_frames);
    let trace = receipts();
    assert_valid(&trace);
    for product in ["fade", "owners"] {
        assert_eq!(trace[product]["plan"]["constructor_calls"], 0);
        assert_eq!(trace[product]["cache"]["constructor_calls"], 0);
    }
    drop(first);
    drop(hit);
    graph.original_inputs.clear(&mut renderer);
    drop(context);
    drop(caller);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(renderer.cached_resident_frames(), 0);
    assert_eq!(renderer.active_frames, 0);
    assert_eq!(renderer.active_disk_bytes, 0);
}
