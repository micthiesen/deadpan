use super::*;

fn prepared_query_receipts() -> [serde_json::Value; 4] {
    use crate::stages::original_request::flattened_work_trace as trace;
    [
        trace::snapshot(),
        trace::processing_snapshot(),
        trace::policy_snapshot(),
        trace::fade_snapshot(),
    ]
    .map(|value| serde_json::to_value(value).unwrap())
}

#[test]
fn prepared_miss_and_hit_reuse_one_query_and_keep_live_source_admission() {
    let (plan, mut provider) = fixture();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
    let request = renderer
        .prepare_original_root(&plan, AudioSample(0), 64, control, true)
        .unwrap();
    let query_metadata = work.borrow().sound_context_frames;
    assert!(
        query_metadata > 0,
        "the actual flattened query retains its charge"
    );
    let query_work = work.borrow().plan_work;
    assert!(query_work > 0);
    let first = renderer
        .shared_original_input_prepared(
            &mut provider,
            &mut graph,
            &Context::Root,
            (AudioSample(0)..AudioSample(64), request),
            control,
            0,
        )
        .unwrap();
    assert_eq!(work.borrow().sound_context_frames, 0);
    let before_legacy = work.borrow().plan_work;
    let before_early_receipts = prepared_query_receipts();
    let legacy = renderer
        .shared_original_input(
            &mut provider,
            &mut graph,
            &Context::Root,
            AudioSample(0)..AudioSample(64),
            control,
            0,
        )
        .unwrap();
    assert_eq!(
        prepared_query_receipts(),
        before_early_receipts,
        "an early warm cache hit builds and compares none of the four query carriers"
    );
    let hit_work = work.borrow().plan_work - before_legacy;
    let before_query = work.borrow().plan_work;
    let request = renderer
        .prepare_original_root(&plan, AudioSample(0), 64, control, true)
        .unwrap();
    assert_eq!(work.borrow().sound_context_frames, query_metadata);
    assert_eq!(
        work.borrow().plan_work - before_query,
        query_work,
        "the attempted cold path bills its real metadata construction even on a hit"
    );
    let before_hit = work.borrow().plan_work;
    let before_hit_receipts = prepared_query_receipts();
    let calls = provider.calls;
    let prepared_hit = renderer
        .shared_original_input_prepared(
            &mut provider,
            &mut graph,
            &Context::Root,
            (AudioSample(0)..AudioSample(64), request),
            control,
            0,
        )
        .unwrap();
    assert_eq!(
        work.borrow().plan_work - before_hit,
        hit_work + 1 + 4,
        "the hit bills sealed request validation and all four actual issuer comparisons once each"
    );
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert!(provider.calls > calls);
    let after_hit_receipts = prepared_query_receipts();
    for (before, after) in before_hit_receipts.iter().zip(&after_hit_receipts) {
        for lane in ["plan", "cache"] {
            let increase = if lane == "plan" { 1 } else { 0 };
            for field in ["issuer_checks", "issuer_work"] {
                assert_eq!(
                    after[lane][field].as_u64().unwrap(),
                    before[lane][field].as_u64().unwrap() + increase
                );
            }
            for field in [
                "constructor_calls",
                "completed_queries",
                "failed_queries",
                "accepted_work",
            ] {
                assert_eq!(
                    after[lane][field], before[lane][field],
                    "late hit retains its supplied carrier without reconstruction: {field}"
                );
            }
        }
    }
    let (Read::Cached(first), Read::Cached(legacy), Read::Cached(prepared_hit)) =
        (&first, &legacy, &prepared_hit)
    else {
        panic!("expected exact cached inputs");
    };
    assert!(Arc::ptr_eq(first, legacy) && Arc::ptr_eq(first, prepared_hit));
    assert_eq!(graph.original_inputs.len, 1);
    graph.original_inputs.clear(&mut renderer);
}

#[test]
fn prepared_cache_input_rejects_mismatched_window_and_revoked_source() {
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
            Err(PreparationError::SourceUnavailable("revoked".into()))
        }
    }
    let (plan, mut provider) = fixture();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
    let request = renderer
        .prepare_original_root(&plan, AudioSample(0), 64, control, true)
        .unwrap();
    let mut unavailable = Unavailable(0);
    assert!(matches!(
        renderer.shared_original_input_prepared(
            &mut unavailable,
            &mut graph,
            &Context::Root,
            (AudioSample(1)..AudioSample(65), request),
            control,
            0
        ),
        Err(StageAudioError::ForeignDomain)
    ));
    assert_eq!(unavailable.0, 0);
    assert_eq!(graph.original_inputs.len, 0);
    renderer
        .shared_original_input(
            &mut provider,
            &mut graph,
            &Context::Root,
            AudioSample(0)..AudioSample(64),
            control,
            0,
        )
        .unwrap();
    let request = renderer
        .prepare_original_root(&plan, AudioSample(0), 64, control, true)
        .unwrap();
    assert!(matches!(
        renderer.shared_original_input_prepared(
            &mut unavailable,
            &mut graph,
            &Context::Root,
            (AudioSample(0)..AudioSample(64), request),
            control,
            0
        ),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
    assert_eq!(
        unavailable.0, 1,
        "a prediction never replaces warm source re-admission"
    );
    graph.original_inputs.clear(&mut renderer);
    assert_eq!(renderer.active_frames, 0);
}
