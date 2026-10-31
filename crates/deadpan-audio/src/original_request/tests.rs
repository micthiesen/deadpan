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

/// Compare every query field except the two top-level constructor work totals.
/// Canonical and additional units are reconciled separately below.
fn same_queries(actual: RootReadQueriesRef<'_, '_>, expected: RootReadQueriesRef<'_, '_>) {
    assert_eq!(actual.flattened.project_id, expected.flattened.project_id);
    assert_eq!(actual.flattened.revision_id, expected.flattened.revision_id);
    assert_eq!(actual.flattened.samples, expected.flattened.samples);
    assert_eq!(actual.flattened.spans, expected.flattened.spans);
    assert_eq!(actual.flattened.lookup, expected.flattened.lookup);
    assert_eq!(actual.processing.project_id, expected.processing.project_id);
    assert_eq!(
        actual.processing.revision_id,
        expected.processing.revision_id
    );
    assert_eq!(actual.processing.samples, expected.processing.samples);
    assert_eq!(actual.processing.spans, expected.processing.spans);
    assert_eq!(actual.processing.lookup, expected.processing.lookup);
    assert_eq!(actual.policy.suppressed, expected.policy.suppressed);
    assert_eq!(actual.policy.contents, expected.policy.contents);
    assert_eq!(actual.policy.lookup, expected.policy.lookup);
    assert_eq!(actual.policy.work, expected.policy.work);
    assert_eq!(actual.fades, expected.fades);
}

fn flattened_work(request: &PreparedOriginalRequest<'_, '_, '_>) -> (usize, usize) {
    let Flattened::Admitted { query, .. } = &request.queries.flattened else {
        panic!("this test must exercise allocation-site admission");
    };
    assert!(query.was_forwarded());
    let canonical = query.canonical_units();
    let additional = query.additional_units();
    assert_eq!(query.work(), canonical.checked_add(additional).unwrap());
    assert_eq!(request.queries().flattened.work, query.work());
    (canonical, additional)
}

fn assert_flattened_cost(
    request: &PreparedOriginalRequest<'_, '_, '_>,
    ordinary: &AudioQuery,
) -> usize {
    let (canonical, additional) = flattened_work(request);
    assert_eq!(canonical, ordinary.work);
    assert!(
        additional > 0,
        "the real fixture must execute admitted work"
    );
    additional
}

fn processing_work(request: &PreparedOriginalRequest<'_, '_, '_>) -> (usize, usize) {
    match &request.queries.processing {
        Processing::Admitted { query, .. } => {
            assert!(query.was_forwarded());
            assert_eq!(
                query.work(),
                query
                    .canonical_units()
                    .checked_add(query.additional_units())
                    .unwrap()
            );
            assert_eq!(request.queries().processing.work, query.work());
            (query.canonical_units(), query.additional_units())
        }
        Processing::Legacy(query) => (query.work, 0),
    }
}

fn assert_processing_cost(
    request: &PreparedOriginalRequest<'_, '_, '_>,
    ordinary: &AudioProcessingQuery<'_>,
) -> usize {
    let (canonical, additional) = processing_work(request);
    assert_eq!(canonical, ordinary.work);
    assert!(
        additional > 0,
        "this default path must admit actual processing metadata"
    );
    additional
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
                same_queries(request.queries(), expected.as_borrowed());
                let additional = assert_flattened_cost(&request, &expected.flattened)
                    + assert_processing_cost(&request, &expected.processing);
                assert_eq!(
                    work.borrow().plan_work,
                    expected_work.borrow().plan_work + additional,
                    "canonical queries plus actual forwarded allocation work, each once"
                );
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
                assert_eq!(
                    work.borrow().plan_work,
                    expected_work.borrow().plan_work + additional + 2,
                    "the consuming read bills both actual issuer comparisons"
                );
                assert_eq!(work.borrow().sound_context_frames, 0);
                assert_eq!(
                    work.borrow().source_checks,
                    expected_work.borrow().source_checks
                );
            }
        }
    }
}

#[test]
fn prepared_source_read_needs_only_its_exact_issuer_work_allowance() {
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
        work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - 2;
        let block = reader
            .read_prepared_original(&mut provider, request, control, 0)
            .unwrap();
        assert_eq!(block.samples.len(), 64);
        assert!(
            provider.calls > 0,
            "prepared metadata never substitutes for live admission"
        );
        assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
        assert_eq!(work.borrow().sound_context_frames, 0);
        let calls = provider.calls;
        assert!(matches!(
            reader.read_controlled(&mut provider, AudioSample(0), 64, control, true, 0),
            Err(StageAudioError::Limit("plan work per read"))
        ));
        assert_eq!(provider.calls, calls);

        let wrapper_work = RefCell::new(ReadWork {
            plan_work: MAX_PLAN_WORK_PER_READ - preparation_work - 2,
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
        assert_eq!(wrapper_work.borrow().sound_context_frames, 0);

        let short_work = RefCell::new(ReadWork::default());
        let short_control = work_control(&cancelled, &short_work);
        let request = if domain_read {
            wrapper_reader.prepare_original_domain(&domain, AudioSample(0), 64, short_control, true)
        } else {
            wrapper_reader.prepare_original_root(&plan, AudioSample(0), 64, short_control, true)
        }
        .unwrap();
        short_work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - 1;
        let calls = provider.calls;
        assert!(matches!(
            wrapper_reader.read_prepared_original(&mut provider, request, short_control, 0),
            Err(StageAudioError::Limit("plan work per read"))
        ));
        assert_eq!(provider.calls, calls, "one short rejects before source I/O");
        assert_eq!(short_work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
        assert_eq!(short_work.borrow().sound_context_frames, 0);
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
    let mut reader = StageAudio::new(Arc::clone(&plan));
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
    let mut reader = StageAudio::new(Arc::clone(&plan));
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
fn exact_preparation_budget_and_one_short_bill_actual_forwarded_work() {
    let (plan, provider) = fixture();
    let domain = plan
        .audio_domain_at(AudioSample(0), Default::default())
        .unwrap();
    let mut reader = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    for domain_read in [false, true] {
        let ordinary_work = RefCell::new(ReadWork::default());
        let ordinary = previous_queries(
            &plan,
            domain_read.then_some(&domain),
            AudioSample(0),
            64,
            work_control(&cancelled, &ordinary_work),
            true,
        )
        .unwrap();
        let measured = RefCell::new(ReadWork::default());
        let reference = if domain_read {
            reader.prepare_original_domain(
                &domain,
                AudioSample(0),
                64,
                work_control(&cancelled, &measured),
                true,
            )
        } else {
            reader.prepare_original_root(
                &plan,
                AudioSample(0),
                64,
                work_control(&cancelled, &measured),
                true,
            )
        }
        .unwrap();
        same_queries(reference.queries(), ordinary.as_borrowed());
        let additional = assert_flattened_cost(&reference, &ordinary.flattened)
            + assert_processing_cost(&reference, &ordinary.processing);
        let needed = measured.borrow().plan_work;
        let ordinary_needed = ordinary_work.borrow().plan_work;
        assert_eq!(needed, ordinary_needed + additional);
        let queries = reference.queries();
        let completed = queries.flattened.work + queries.processing.work + queries.policy.work;
        assert!(queries.fades.unwrap().work > 0);
        let mut admitted_error = None;

        for remaining in [needed, needed - 1] {
            let work = RefCell::new(ReadWork {
                plan_work: MAX_PLAN_WORK_PER_READ - remaining,
                ..Default::default()
            });
            let initial = work.borrow().plan_work;
            let result = if domain_read {
                reader.prepare_original_domain(
                    &domain,
                    AudioSample(0),
                    64,
                    work_control(&cancelled, &work),
                    true,
                )
            } else {
                reader.prepare_original_root(
                    &plan,
                    AudioSample(0),
                    64,
                    work_control(&cancelled, &work),
                    true,
                )
            };
            if remaining == needed {
                let request = result.unwrap();
                same_queries(request.queries(), reference.queries());
                assert_eq!(flattened_work(&request), flattened_work(&reference));
                assert_eq!(processing_work(&request), processing_work(&reference));
                assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
                drop(request);
            } else {
                assert!(
                    result.is_err(),
                    "one short cannot finish the actual fade query"
                );
                admitted_error = Some(result.err().unwrap().to_string());
                assert_eq!(
                    work.borrow().plan_work,
                    initial + completed,
                    "failed final query retains every earlier forwarded/completed bill"
                );
            }
            assert_eq!(work.borrow().sound_context_frames, 0);
        }

        // The ordinary oracle retains its own smaller boundary; its work is
        // not used as the admitted constructor's allowance.
        let mut ordinary_error = None;
        for remaining in [ordinary_needed, ordinary_needed - 1] {
            let work = RefCell::new(ReadWork {
                plan_work: MAX_PLAN_WORK_PER_READ - remaining,
                ..Default::default()
            });
            let result = previous_queries(
                &plan,
                domain_read.then_some(&domain),
                AudioSample(0),
                64,
                work_control(&cancelled, &work),
                true,
            );
            assert_eq!(result.is_ok(), remaining == ordinary_needed);
            if remaining != ordinary_needed {
                ordinary_error = Some(result.err().unwrap().to_string());
            }
            let expected = if remaining == ordinary_needed {
                MAX_PLAN_WORK_PER_READ
            } else {
                MAX_PLAN_WORK_PER_READ - remaining
                    + ordinary.flattened.work
                    + ordinary.processing.work
                    + ordinary.policy.work
            };
            assert_eq!(work.borrow().plan_work, expected);
        }
        assert_eq!(admitted_error, ordinary_error);
        drop(reference);
        assert_eq!(measured.borrow().sound_context_frames, 0);
        assert_eq!(
            measured.borrow().plan_work,
            needed,
            "dropping metadata never refunds work"
        );
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
            assert_eq!(
                work.borrow().plan_work,
                spent + 2 * usize::from(failure == 2),
                "only a live control bills both issuer comparisons before depth rejection"
            );
            assert_eq!(work.borrow().sound_context_frames, 0);
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
    let mut reader = StageAudio::new(Arc::clone(&plan));
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
        assert_eq!(
            work.borrow().plan_work,
            spent + 2,
            "fresh source rejection follows both actual issuer comparisons"
        );
        assert_eq!(work.borrow().sound_context_frames, 0);
    }
    assert_eq!(provider.calls, 0);
}

#[path = "flattened_admission_tests.rs"]
mod flattened_admission;

#[path = "processing_admission_tests.rs"]
mod processing_admission;
