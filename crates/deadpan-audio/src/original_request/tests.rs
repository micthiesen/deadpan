use super::*;
use crate::stages::controlled_reads::{Provider, fixture_document_at_rate};

#[path = "policy_reuse_tests.rs"]
mod policy_reuse;

fn fixture() -> (Arc<RenderPlan>, Provider) {
    let (document, provider) = fixture_document_at_rate(FrameRate::new(48_000, 1).unwrap());
    (Arc::new(RenderPlan::compile(&document).unwrap()), provider)
}

fn work_control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        work,
        deadline: Instant::now() + Duration::from_secs(30),
    }
}

/// The previous construction sequence, kept as a focused work/content oracle.
fn previous_queries<'plan>(
    plan: &'plan RenderPlan,
    domain: Option<&AudioDomain<'plan>>,
    start: AudioSample,
    frames: u32,
    control: WorkControl<'_>,
    edge_fades: bool,
) -> Result<RootReadQueries<'plan>, StageAudioError> {
    control.check()?;
    let end = AudioSample(start.0 + i64::from(frames));
    let flattened = match domain {
        Some(domain) => domain.audio(start..end, control.query_limits()?)?,
        None => plan.audio(start..end, control.query_limits()?)?,
    };
    control.spend_plan_work(flattened.work)?;
    let processing = match domain {
        Some(domain) => domain.processing(start..end, control.query_limits()?)?,
        None => plan.audio_processing(start..end, control.query_limits()?)?,
    };
    control.spend_plan_work(processing.work)?;
    let policy = match domain {
        Some(domain) => domain.policy(start..end, control.query_limits()?)?,
        None => plan.audio_policy(start..end, control.query_limits()?)?,
    };
    control.spend_plan_work(policy.work)?;
    let fades = if edge_fades {
        let query = match domain {
            Some(domain) => domain.fades(start..end, control.query_limits()?)?,
            None => plan.audio_fades(start..end, control.query_limits()?)?,
        };
        control.spend_plan_work(query.work)?;
        Some(query)
    } else {
        None
    };
    Ok(RootReadQueries {
        flattened,
        processing,
        policy,
        fades,
    })
}

fn same_queries(actual: RootReadQueriesRef<'_, '_>, expected: &RootReadQueries<'_>) {
    assert_eq!(actual.flattened, &expected.flattened);
    assert_eq!(actual.processing, &expected.processing);
    assert_eq!(actual.policy.suppressed, expected.policy.suppressed);
    assert_eq!(actual.policy.contents, expected.policy.contents);
    assert_eq!(actual.policy.lookup, expected.policy.lookup);
    assert_eq!(actual.policy.work, expected.policy.work);
    assert_eq!(actual.fades, expected.fades.as_ref());
}

fn same_blocks(actual: &ReadBlock, expected: &ReadBlock) {
    assert_eq!(actual.start, expected.start);
    assert_eq!(
        actual
            .samples
            .iter()
            .map(|frame| frame.map(f32::to_bits))
            .collect::<Vec<_>>(),
        expected
            .samples
            .iter()
            .map(|frame| frame.map(f32::to_bits))
            .collect::<Vec<_>>()
    );
    assert_eq!(actual.suppressed, expected.suppressed);
    assert_eq!(actual.exhausted, expected.exhausted);
    assert_eq!(actual.dependencies, expected.dependencies);
    assert_eq!(actual.relative_depth, expected.relative_depth);
}

#[test]
fn prepared_root_and_domain_match_previous_query_content_work_and_actual_reads() {
    for domain_read in [false, true] {
        for edge_fades in [false, true] {
            for start in [0, 512, 640] {
                let (plan, mut provider) = fixture();
                let (_, mut expected_provider) = fixture();
                let domain = plan
                    .audio_domain_at(AudioSample(start), Default::default())
                    .unwrap();
                let mut reader = StageAudio::new(Arc::clone(&plan));
                let mut expected_reader = StageAudio::new(Arc::clone(&plan));
                let cancelled = AtomicBool::new(false);
                let work = RefCell::new(ReadWork::default());
                let expected_work = RefCell::new(ReadWork::default());
                let control = work_control(&cancelled, &work);
                let expected_control = work_control(&cancelled, &expected_work);
                let request = if domain_read {
                    reader.prepare_original_domain(
                        &domain,
                        AudioSample(start),
                        64,
                        control,
                        edge_fades,
                    )
                } else {
                    reader.prepare_original_root(&plan, AudioSample(start), 64, control, edge_fades)
                }
                .unwrap();
                assert_eq!(provider.calls, 0, "planning has no provider access");
                assert!(work.borrow().preflight_assets.is_empty());
                let expected = previous_queries(
                    &plan,
                    domain_read.then_some(&domain),
                    AudioSample(start),
                    64,
                    expected_control,
                    edge_fades,
                )
                .unwrap();
                same_queries(request.queries(), &expected);
                assert_eq!(work.borrow().plan_work, expected_work.borrow().plan_work);
                let actual = reader
                    .read_prepared_original(&mut provider, request, control, 0)
                    .unwrap();
                let expected = expected_reader
                    .read_queries(
                        &mut expected_provider,
                        AudioSample(start),
                        64,
                        expected_control,
                        0,
                        expected,
                    )
                    .unwrap();
                same_blocks(&actual, &expected);
                assert_eq!(provider.calls, expected_provider.calls);
                assert_eq!(work.borrow().plan_work, expected_work.borrow().plan_work);
                assert_eq!(
                    work.borrow().source_checks,
                    expected_work.borrow().source_checks
                );
            }
        }
    }
}

#[test]
fn prepared_source_read_does_not_requery_with_no_plan_work_remaining() {
    for domain_read in [false, true] {
        let (plan, mut provider) = fixture();
        let domain = plan
            .audio_domain_at(AudioSample(0), Default::default())
            .unwrap();
        let mut reader = StageAudio::new(Arc::clone(&plan));
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = work_control(&cancelled, &work);
        let request = if domain_read {
            reader.prepare_original_domain(&domain, AudioSample(0), 64, control, true)
        } else {
            reader.prepare_original_root(&plan, AudioSample(0), 64, control, true)
        }
        .unwrap();
        assert!(
            request
                .queries()
                .processing
                .spans
                .iter()
                .all(|span| matches!(
                    span.content,
                    AudioSignalContent::Leaf(AudioContent::Source { .. })
                ))
        );
        assert_eq!(provider.calls, 0);
        let preparation_work = work.borrow().plan_work;
        work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ;
        let block = reader
            .read_prepared_original(&mut provider, request, control, 0)
            .unwrap();
        assert_eq!(block.samples.len(), 64);
        assert!(
            provider.calls > 0,
            "prepared metadata never substitutes for live admission"
        );
        assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
        let calls = provider.calls;
        assert!(matches!(
            reader.read_controlled(&mut provider, AudioSample(0), 64, control, true, 0),
            Err(StageAudioError::Limit("plan work per read"))
        ));
        assert_eq!(provider.calls, calls);

        let wrapper_work = RefCell::new(ReadWork {
            plan_work: MAX_PLAN_WORK_PER_READ - preparation_work,
            ..Default::default()
        });
        let wrapper_control = work_control(&cancelled, &wrapper_work);
        let mut wrapper_reader = StageAudio::new(Arc::clone(&plan));
        let wrapper = if domain_read {
            wrapper_reader.read_context_controlled(
                &mut provider,
                &domain,
                AudioSample(0),
                64,
                wrapper_control,
                (0, true),
            )
        } else {
            wrapper_reader.read_controlled(
                &mut provider,
                AudioSample(0),
                64,
                wrapper_control,
                true,
                0,
            )
        }
        .unwrap();
        same_blocks(&wrapper, &block);
        assert_eq!(wrapper_work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    }
}

#[test]
fn prepared_identity_requires_exact_plan_domain_window_and_edge_mode() {
    let (plan, mut provider) = fixture();
    let equal_foreign = Arc::new(plan.as_ref().clone());
    let domain = plan
        .audio_domain_at(AudioSample(0), Default::default())
        .unwrap();
    let cloned_domain = domain.clone();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = work_control(&cancelled, &work);
    let reader = StageAudio::new(Arc::clone(&plan));
    let root = reader
        .prepare_original_root(&plan, AudioSample(0), 64, control, true)
        .unwrap();
    assert!(root.matches_root(&plan, AudioSample(0), 64, true));
    assert!(!root.matches_root(&equal_foreign, AudioSample(0), 64, true));
    assert!(!root.matches_root(&plan, AudioSample(1), 64, true));
    assert!(!root.matches_root(&plan, AudioSample(0), 63, true));
    assert!(!root.matches_root(&plan, AudioSample(0), 64, false));
    assert!(!root.matches_domain(&plan, &domain, AudioSample(0), 64, true));
    let mut foreign_reader = StageAudio::new(Arc::clone(&equal_foreign));
    assert!(matches!(
        foreign_reader.read_prepared_original(&mut provider, root, control, 0),
        Err(StageAudioError::ForeignDomain)
    ));
    let request = reader
        .prepare_original_domain(&domain, AudioSample(0), 64, control, true)
        .unwrap();
    assert!(request.matches_domain(&plan, &domain, AudioSample(0), 64, true));
    assert!(!request.matches_domain(&plan, &cloned_domain, AudioSample(0), 64, true));
    assert!(!request.matches_domain(&equal_foreign, &domain, AudioSample(0), 64, true));
    assert!(!request.matches_domain(&plan, &domain, AudioSample(1), 64, true));
    assert!(!request.matches_domain(&plan, &domain, AudioSample(0), 63, true));
    assert!(!request.matches_domain(&plan, &domain, AudioSample(0), 64, false));
    assert!(!request.matches_root(&plan, AudioSample(0), 64, true));
    assert!(matches!(
        foreign_reader.read_prepared_original(&mut provider, request, control, 0),
        Err(StageAudioError::ForeignDomain)
    ));
    assert_eq!(provider.calls, 0);
}

#[test]
fn preparation_preserves_root_and_domain_range_and_foreign_errors() {
    let (plan, provider) = fixture();
    let equal_foreign = Arc::new(plan.as_ref().clone());
    let domain = plan
        .audio_domain_at(AudioSample(0), Default::default())
        .unwrap();
    let foreign_domain = equal_foreign
        .audio_domain_at(AudioSample(0), Default::default())
        .unwrap();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = work_control(&cancelled, &work);
    let reader = StageAudio::new(Arc::clone(&plan));
    for (start, frames) in [
        (AudioSample(0), 0),
        (AudioSample(0), MAX_OUTPUT_FRAMES + 1),
        (AudioSample(i64::MAX), 1),
    ] {
        assert!(matches!(
            reader.prepare_original_root(&plan, start, frames, control, true),
            Err(StageAudioError::Range)
        ));
        assert!(matches!(
            reader.prepare_original_domain(&domain, start, frames, control, true),
            Err(StageAudioError::Range)
        ));
    }
    assert!(matches!(
        reader.prepare_original_root(&plan, AudioSample(-1), 1, control, true),
        Err(StageAudioError::Range)
    ));
    assert!(matches!(
        reader.prepare_original_domain(&domain, AudioSample(-1), 1, control, true),
        Err(StageAudioError::Plan(PlanError::AudioRangeOutOfRange))
    ));
    assert!(matches!(
        reader.prepare_original_root(&equal_foreign, AudioSample(0), 1, control, true),
        Err(StageAudioError::ForeignDomain)
    ));
    assert!(matches!(
        reader.prepare_original_domain(&foreign_domain, AudioSample(0), 1, control, true),
        Err(StageAudioError::ForeignDomain)
    ));
    assert_eq!(provider.calls, 0);
    assert_eq!(work.borrow().plan_work, 0);
}

#[test]
fn exact_preparation_budget_and_one_short_match_previous_query_sequence() {
    let (plan, provider) = fixture();
    let domain = plan
        .audio_domain_at(AudioSample(0), Default::default())
        .unwrap();
    let reader = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    for domain_read in [false, true] {
        let measured = RefCell::new(ReadWork::default());
        previous_queries(
            &plan,
            domain_read.then_some(&domain),
            AudioSample(0),
            64,
            work_control(&cancelled, &measured),
            true,
        )
        .unwrap();
        let needed = measured.borrow().plan_work;
        assert!(needed > 1);
        for remaining in [needed, needed - 1] {
            let work = RefCell::new(ReadWork {
                plan_work: MAX_PLAN_WORK_PER_READ - remaining,
                ..Default::default()
            });
            let old_work = RefCell::new(ReadWork {
                plan_work: MAX_PLAN_WORK_PER_READ - remaining,
                ..Default::default()
            });
            let control = work_control(&cancelled, &work);
            let result = if domain_read {
                reader.prepare_original_domain(&domain, AudioSample(0), 64, control, true)
            } else {
                reader.prepare_original_root(&plan, AudioSample(0), 64, control, true)
            };
            let old = previous_queries(
                &plan,
                domain_read.then_some(&domain),
                AudioSample(0),
                64,
                work_control(&cancelled, &old_work),
                true,
            );
            assert_eq!(result.is_ok(), remaining == needed);
            assert_eq!(result.is_ok(), old.is_ok());
            assert_eq!(
                result.err().map(|error| error.to_string()),
                old.err().map(|error| error.to_string())
            );
            assert_eq!(work.borrow().plan_work, old_work.borrow().plan_work);
        }
    }
    assert_eq!(provider.calls, 0);
}

#[test]
fn prepared_read_still_checks_cancellation_deadline_and_depth_before_sources() {
    let (plan, mut provider) = fixture();
    let domain = plan
        .audio_domain_at(AudioSample(0), Default::default())
        .unwrap();
    for domain_read in [false, true] {
        for failure in 0..3 {
            let mut reader = StageAudio::new(Arc::clone(&plan));
            let cancelled = AtomicBool::new(false);
            let work = RefCell::new(ReadWork::default());
            let mut control = work_control(&cancelled, &work);
            let request = if domain_read {
                reader.prepare_original_domain(&domain, AudioSample(0), 64, control, true)
            } else {
                reader.prepare_original_root(&plan, AudioSample(0), 64, control, true)
            }
            .unwrap();
            let spent = work.borrow().plan_work;
            let depth = if failure == 2 {
                reader.limits.maximum_depth + 1
            } else {
                0
            };
            if failure == 0 {
                cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            if failure == 1 {
                control.deadline = Instant::now() - Duration::from_secs(1);
            }
            let error = reader
                .read_prepared_original(&mut provider, request, control, depth)
                .err()
                .unwrap();
            match failure {
                0 => assert!(error.is_cancelled()),
                1 => assert!(matches!(error, StageAudioError::Timeout)),
                _ => assert!(matches!(
                    error,
                    StageAudioError::Limit("nested stage depth")
                )),
            }
            assert_eq!(work.borrow().plan_work, spent);
            assert_eq!(provider.calls, 0);
        }
    }
}

#[test]
fn planning_cancellation_and_deadline_keep_priority_over_invalid_range() {
    let (plan, provider) = fixture();
    let domain = plan
        .audio_domain_at(AudioSample(0), Default::default())
        .unwrap();
    let reader = StageAudio::new(Arc::clone(&plan));
    for domain_read in [false, true] {
        for cancelled_now in [false, true] {
            let cancelled = AtomicBool::new(cancelled_now);
            let work = RefCell::new(ReadWork::default());
            let mut control = work_control(&cancelled, &work);
            if !cancelled_now {
                control.deadline = Instant::now() - Duration::from_secs(1);
            }
            let error = if domain_read {
                reader.prepare_original_domain(&domain, AudioSample(i64::MAX), 1, control, true)
            } else {
                reader.prepare_original_root(&plan, AudioSample(i64::MAX), 1, control, true)
            }
            .err()
            .unwrap();
            if cancelled_now {
                assert!(error.is_cancelled());
            } else {
                assert!(matches!(error, StageAudioError::Timeout));
            }
            assert_eq!(work.borrow().plan_work, 0);
        }
    }
    assert_eq!(provider.calls, 0);
}

#[test]
fn prepared_metadata_does_not_waive_fresh_source_admission() {
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
            Err(PreparationError::SourceUnavailable(
                "revoked after preparation".into(),
            ))
        }
    }
    let (plan, provider) = fixture();
    let domain = plan
        .audio_domain_at(AudioSample(0), Default::default())
        .unwrap();
    let cancelled = AtomicBool::new(false);
    for domain_read in [false, true] {
        let work = RefCell::new(ReadWork::default());
        let control = work_control(&cancelled, &work);
        let mut reader = StageAudio::new(Arc::clone(&plan));
        let request = if domain_read {
            reader.prepare_original_domain(&domain, AudioSample(0), 64, control, true)
        } else {
            reader.prepare_original_root(&plan, AudioSample(0), 64, control, true)
        }
        .unwrap();
        assert!(work.borrow().preflight_assets.is_empty());
        let spent = work.borrow().plan_work;
        let mut unavailable = Unavailable(0);
        assert!(matches!(
            reader.read_prepared_original(&mut unavailable, request, control, 0),
            Err(StageAudioError::Preparation(
                PreparationError::SourceUnavailable(_)
            ))
        ));
        assert_eq!(unavailable.0, 1);
        assert!(
            work.borrow()
                .preflight_assets
                .contains(&AssetId::new("media").unwrap())
        );
        assert_eq!(work.borrow().plan_work, spent);
    }
    assert_eq!(provider.calls, 0);
}
