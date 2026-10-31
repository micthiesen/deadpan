//! Default admitted first-query coverage, separate from legacy policy-savings tests.
use super::*;

#[test]
fn admitted_bound_domain_keeps_canonical_receipts_policy_and_actual_wav_output() {
    for case in [BindingCase::Source, BindingCase::Unrelated] {
        let (plan, mut provider) = bound_fixture(case);
        let (_, mut old_provider) = bound_fixture(case);
        let input = root_input(&plan);
        let domain = input.context();
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let old_work = RefCell::new(ReadWork::default());
        let mut reader = StageAudio::new(Arc::clone(&plan));
        let mut old_reader = StageAudio::new(Arc::clone(&plan));
        let (request, trace) = with_domain_policy_trace(false, || {
            reader
                .prepare_original_domain(
                    domain,
                    AudioSample(0),
                    64,
                    work_control(&cancelled, &work),
                    true,
                )
                .unwrap()
        });
        let (old, old_trace) = with_legacy_processing_queries(|| {
            with_domain_policy_trace(false, || {
                old_reader
                    .prepare_original_domain(
                        domain,
                        AudioSample(0),
                        64,
                        work_control(&cancelled, &old_work),
                        true,
                    )
                    .unwrap()
            })
        });
        let (canonical, additional) = processing_work(&request);
        assert!(additional > 0);
        let current = request.queries();
        let previous = old.queries();
        assert_eq!(canonical, previous.processing.work);
        assert_eq!(current.processing.work, canonical + additional);
        assert_eq!(
            current.processing.project_id,
            previous.processing.project_id
        );
        assert_eq!(
            current.processing.revision_id,
            previous.processing.revision_id
        );
        assert_eq!(current.processing.samples, previous.processing.samples);
        assert_eq!(current.processing.lookup, previous.processing.lookup);
        assert_eq!(
            current.processing.spans.len(),
            previous.processing.spans.len()
        );
        // Full private Bound history is covered by Plan's paired constructor
        // proof; this real WAV boundary checks the receipt split and PCM.
        for (current, previous) in current
            .processing
            .spans
            .iter()
            .zip(&previous.processing.spans)
        {
            match (&current.content, &previous.content) {
                (AudioSignalContent::Bound(_), AudioSignalContent::Bound(_)) => {
                    assert_eq!(current.definition, previous.definition);
                    assert_eq!(current.samples, previous.samples);
                    assert_eq!(current.allocated_samples, previous.allocated_samples);
                    assert_eq!(current.project_extent, previous.project_extent);
                    assert_eq!(current.instance, previous.instance);
                    assert_eq!(current.gap_after, previous.gap_after);
                    assert_eq!(current.transform, previous.transform);
                    assert_eq!(current.grid, previous.grid);
                    assert_eq!(current.sampling, previous.sampling);
                    assert_eq!(current.retimes, previous.retimes);
                }
                _ => assert_eq!(current, previous),
            }
        }
        assert_eq!(current.flattened, previous.flattened);
        assert_eq!(current.policy.suppressed, previous.policy.suppressed);
        assert_eq!(current.policy.contents, previous.policy.contents);
        assert_eq!(current.policy.lookup, previous.policy.lookup);
        assert_eq!(current.policy.work, previous.policy.work);
        assert_eq!(current.fades, previous.fades);
        assert_eq!(trace.len(), 1);
        assert_eq!(old_trace.len(), 1);
        assert_eq!(trace[0].processing_work, canonical + additional);
        assert_eq!(old_trace[0].processing_work, canonical);
        assert_eq!(trace[0].policy_work, old_trace[0].policy_work);
        assert_eq!(
            work.borrow().plan_work,
            old_work.borrow().plan_work + additional
        );
        assert!(work.borrow().sound_context_frames > old_work.borrow().sound_context_frames);
        assert!(old_work.borrow().sound_context_frames > 0);
        assert_eq!(provider.calls, 0);
        assert_eq!(old_provider.calls, 0);
        let block = reader
            .read_prepared_original(&mut provider, request, work_control(&cancelled, &work), 0)
            .unwrap();
        let old_block = old_reader
            .read_prepared_original(
                &mut old_provider,
                old,
                work_control(&cancelled, &old_work),
                0,
            )
            .unwrap();
        same_blocks(&block, &old_block);
        assert!(provider.calls > 0);
        assert_eq!(provider.calls, old_provider.calls);
        assert_eq!(work.borrow().source_checks, old_work.borrow().source_checks);
        assert_eq!(
            work.borrow().plan_work,
            old_work.borrow().plan_work + additional + 1
        );
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(old_work.borrow().sound_context_frames, 0);
    }
}
