//! Actual conditional fade carriers through real ReadWork and WAV admission.
use super::*;

fn fade_trace() -> serde_json::Value {
    let trace = serde_json::to_value(flattened_work_trace::fade_snapshot()).unwrap();
    for flag in ["nested", "invalid", "overflow"] {
        assert_eq!(trace[flag], false, "{trace}");
    }
    assert_eq!(trace["active_depth"], 0);
    trace
}

fn fade_plan_trace() -> serde_json::Value {
    fade_trace()["plan"].clone()
}

fn fade_units(request: &PreparedOriginalRequest<'_, '_, '_>) -> (usize, usize) {
    let Some(Fade::Admitted { query, .. }) = &request.queries.fades else {
        panic!("enabled Original fades must own their admitted carrier");
    };
    assert!(query.was_forwarded());
    assert_eq!(
        query.work(),
        query.canonical_units() + query.additional_units()
    );
    assert_eq!(request.queries().fades.unwrap().work, query.work());
    (query.canonical_units(), query.additional_units())
}

fn prepare<'context, 'plan, 'work>(
    reader: &mut StageAudio,
    plan: &'plan RenderPlan,
    domain: Option<&'context AudioDomain<'plan>>,
    start: AudioSample,
    control: WorkControl<'work>,
    edge_fades: bool,
) -> Result<PreparedOriginalRequest<'context, 'plan, 'work>, StageAudioError>
where
    'plan: 'context,
{
    if let Some(domain) = domain {
        reader.prepare_original_domain(domain, start, 64, control, edge_fades)
    } else {
        reader.prepare_original_root(plan, start, 64, control, edge_fades)
    }
}

fn assert_three_carriers(request: &PreparedOriginalRequest<'_, '_, '_>) {
    assert!(matches!(
        &request.queries.flattened,
        Flattened::Admitted { .. }
    ));
    assert!(matches!(
        &request.queries.processing,
        Processing::Admitted { .. }
    ));
    assert!(matches!(&request.queries.policy, Policy::Admitted { .. }));
}

fn compare_content(actual: RootReadQueriesRef<'_, '_>, old: RootReadQueriesRef<'_, '_>) {
    same_queries(
        RootReadQueriesRef {
            flattened: actual.flattened,
            processing: actual.processing,
            policy: actual.policy,
            fades: None,
        },
        RootReadQueriesRef {
            flattened: old.flattened,
            processing: old.processing,
            policy: old.policy,
            fades: None,
        },
    );
    match (actual.fades, old.fades) {
        (Some(actual), Some(old)) => {
            assert_eq!(actual.spans, old.spans);
            assert_eq!(actual.lookup, old.lookup);
        }
        (None, None) => {}
        _ => panic!("fade presence changed"),
    }
}

fn lane(snapshot: flattened_work_trace::Snapshot) -> serde_json::Value {
    let value = serde_json::to_value(snapshot).unwrap();
    for flag in ["nested", "invalid", "overflow"] {
        assert_eq!(value[flag], false, "{value}");
    }
    assert_eq!(value["active_depth"], 0);
    value["plan"].clone()
}

fn assert_issuer_receipts(fades: bool) {
    for snapshot in [
        flattened_work_trace::snapshot(),
        flattened_work_trace::processing_snapshot(),
        flattened_work_trace::policy_snapshot(),
    ] {
        let trace = lane(snapshot);
        assert_eq!(trace["issuer_checks"], 1);
        assert_eq!(trace["issuer_work"], 1);
    }
    let fade = fade_plan_trace();
    assert_eq!(fade["issuer_checks"], usize::from(fades));
    assert_eq!(fade["issuer_work"], usize::from(fades));
}

#[test]
fn conditional_fades_match_ordinary_values_and_wav_reads_with_actual_four_product_receipts() {
    for domain_read in [false, true] {
        for edge_fades in [false, true] {
            for start in [0, 512, 640] {
                let (plan, mut provider) = fixture();
                let (_, mut old_provider) = fixture();
                let domain = plan
                    .audio_domain_at(AudioSample(start), Default::default())
                    .unwrap();
                let mut reader = StageAudio::new(Arc::clone(&plan));
                let mut old_reader = StageAudio::new(Arc::clone(&plan));
                let cancelled = AtomicBool::new(false);
                let work = RefCell::new(ReadWork::default());
                let old_work = RefCell::new(ReadWork::default());
                let control = work_control(&cancelled, &work);
                let old_control = work_control(&cancelled, &old_work);
                flattened_work_trace::reset();
                let request = prepare(
                    &mut reader,
                    &plan,
                    domain_read.then_some(&domain),
                    AudioSample(start),
                    control,
                    edge_fades,
                )
                .unwrap();
                assert_three_carriers(&request);
                let old = previous_queries(
                    &plan,
                    domain_read.then_some(&domain),
                    AudioSample(start),
                    64,
                    old_control,
                    edge_fades,
                )
                .unwrap();
                compare_content(request.queries(), old.as_borrowed());
                let mut additional = assert_flattened_cost(&request, &old.flattened)
                    + assert_processing_cost(&request, &old.processing)
                    + assert_policy_cost(&request, &old.policy);
                if edge_fades {
                    let (canonical, extra) = fade_units(&request);
                    assert_eq!(canonical, old.fades.as_ref().unwrap().work);
                    assert!(extra > 0);
                    additional += extra;
                    let trace = fade_plan_trace();
                    assert_eq!(trace["constructor_calls"], 1);
                    assert_eq!(trace["completed_queries"], 1);
                    assert_eq!(trace["failed_queries"], 0);
                    assert_eq!(trace["canonical_units"], canonical);
                    assert_eq!(trace["additional_units"], extra);
                    assert_eq!(trace["partial_work"], 0);
                } else {
                    assert!(request.queries.fades.is_none());
                    let trace = fade_plan_trace();
                    for field in [
                        "constructor_calls",
                        "completed_queries",
                        "failed_queries",
                        "accepted_work",
                        "canonical_units",
                        "additional_units",
                        "partial_work",
                        "host_eviction_work",
                        "issuer_checks",
                        "issuer_work",
                    ] {
                        assert_eq!(trace[field], 0, "{field}: {trace}");
                    }
                }
                let trace = fade_trace();
                let evictions = trace["all_host_eviction_work"][0].as_u64().unwrap() as usize;
                assert_eq!(
                    work.borrow().plan_work,
                    old_work.borrow().plan_work + additional + evictions
                );
                assert_eq!(provider.calls, 0);
                assert!(work.borrow().sound_context_frames > 0);
                assert!(work.borrow().preflight_assets.is_empty());
                let actual = reader
                    .read_prepared_original(&mut provider, request, control, 0)
                    .unwrap();
                let old = old_reader
                    .read_queries(
                        &mut old_provider,
                        AudioSample(start),
                        64,
                        old_control,
                        0,
                        old,
                    )
                    .unwrap();
                same_blocks(&actual, &old);
                assert_eq!(provider.calls, old_provider.calls);
                assert_eq!(
                    work.borrow().plan_work,
                    old_work.borrow().plan_work
                        + additional
                        + evictions
                        + 3
                        + usize::from(edge_fades)
                );
                assert_issuer_receipts(edge_fades);
                assert_eq!(work.borrow().sound_context_frames, 0);
                assert_eq!(reader.active_frames, 0);
                assert_eq!(work.borrow().source_checks, old_work.borrow().source_checks);
            }
        }
    }
}

#[test]
fn all_four_metadata_products_stay_live_at_real_source_failure_and_cancellation() {
    struct Watch<'work> {
        work: &'work RefCell<ReadWork>,
        expected_live: u64,
        observed: usize,
        cancel_at_source: bool,
    }
    impl AudioSourceProvider for Watch<'_> {
        fn source(
            &mut self,
            _project: &ProjectId,
            _revision: &RevisionId,
            _asset: &AssetId,
            cancelled: &AtomicBool,
        ) -> Result<&crate::PreparedSource, PreparationError> {
            assert_eq!(self.work.borrow().sound_context_frames, self.expected_live);
            self.observed += 1;
            if self.cancel_at_source {
                cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                Err(PreparationError::Cancelled)
            } else {
                Err(PreparationError::SourceUnavailable(
                    "refused while all four Original products are charged".into(),
                ))
            }
        }
    }

    for domain_read in [false, true] {
        for mode in 0..3 {
            let (plan, provider) = fixture();
            let domain = plan
                .audio_domain_at(AudioSample(0), Default::default())
                .unwrap();
            let mut reader = StageAudio::new(Arc::clone(&plan));
            let cancelled = AtomicBool::new(false);
            let work = RefCell::new(ReadWork::default());
            let control = work_control(&cancelled, &work);
            let request = prepare(
                &mut reader,
                &plan,
                domain_read.then_some(&domain),
                AudioSample(0),
                control,
                true,
            )
            .unwrap();
            assert_three_carriers(&request);
            let (_, additional) = fade_units(&request);
            assert!(additional > 0);
            let live = work.borrow().sound_context_frames;
            let spent = work.borrow().plan_work;
            assert!(live > 0);
            assert_eq!(provider.calls, 0);
            let mut watch = Watch {
                work: &work,
                expected_live: live,
                observed: 0,
                cancel_at_source: mode == 1,
            };
            flattened_work_trace::reset();
            if mode == 2 {
                cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            let error = reader
                .read_prepared_original(&mut watch, request, control, 0)
                .err()
                .expect("source failure or cancellation must reject the real read");
            if mode == 0 {
                assert!(matches!(
                    error,
                    StageAudioError::Preparation(PreparationError::SourceUnavailable(_))
                ));
            } else {
                assert!(error.is_cancelled());
            }
            assert_eq!(watch.observed, usize::from(mode != 2));
            assert_eq!(work.borrow().sound_context_frames, 0);
            assert_eq!(reader.active_frames, 0);
            if mode == 2 {
                assert_eq!(work.borrow().plan_work, spent);
                assert_eq!(fade_plan_trace()["issuer_checks"], 0);
            } else {
                assert_eq!(work.borrow().plan_work, spent + 4);
                assert_issuer_receipts(true);
            }
        }
    }
}

#[test]
fn a_foreign_fade_carrier_is_rejected_after_the_three_valid_issuers_before_wav_io() {
    for domain_read in [false, true] {
        let (plan, mut provider) = fixture();
        let domain = plan
            .audio_domain_at(AudioSample(0), Default::default())
            .unwrap();
        let cancelled = AtomicBool::new(false);
        let owner_work = RefCell::new(ReadWork::default());
        let foreign_work = RefCell::new(ReadWork::default());
        let owner_control = work_control(&cancelled, &owner_work);
        let foreign_control = work_control(&cancelled, &foreign_work);
        let mut reader = StageAudio::new(Arc::clone(&plan));
        let mut owner = prepare(
            &mut reader,
            &plan,
            domain_read.then_some(&domain),
            AudioSample(0),
            owner_control,
            true,
        )
        .unwrap();
        let mut foreign = prepare(
            &mut reader,
            &plan,
            domain_read.then_some(&domain),
            AudioSample(0),
            foreign_control,
            true,
        )
        .unwrap();
        let Flattened::Admitted { issuer: flat, .. } = &owner.queries.flattened else {
            panic!("flattened carrier must be admitted");
        };
        let Processing::Admitted {
            issuer: processing, ..
        } = &owner.queries.processing
        else {
            panic!("processing carrier must be admitted");
        };
        let Policy::Admitted { issuer: policy, .. } = &owner.queries.policy else {
            panic!("policy carrier must be admitted");
        };
        let Some(Fade::Admitted { issuer: fade, .. }) = &foreign.queries.fades else {
            panic!("foreign fade carrier must be admitted");
        };
        for issuer in [*flat, *processing, *policy] {
            assert!(issuer == sound_context_admission::issuer(owner_control));
        }
        assert!(*fade == sound_context_admission::issuer(foreign_control));
        std::mem::swap(&mut owner.queries.fades, &mut foreign.queries.fades);
        drop(foreign);
        assert!(owner_work.borrow().sound_context_frames > 0);
        assert!(foreign_work.borrow().sound_context_frames > 0);
        let owner_spent = owner_work.borrow().plan_work;
        let foreign_spent = foreign_work.borrow().plan_work;
        flattened_work_trace::reset();
        assert!(matches!(
            reader.read_prepared_original(&mut provider, owner, owner_control, 0),
            Err(StageAudioError::Plan(
                PlanError::AudioMetadataIssuerMismatch
            ))
        ));
        assert_eq!(provider.calls, 0);
        assert_eq!(owner_work.borrow().plan_work, owner_spent + 4);
        assert_eq!(foreign_work.borrow().plan_work, foreign_spent);
        assert_eq!(owner_work.borrow().sound_context_frames, 0);
        assert_eq!(foreign_work.borrow().sound_context_frames, 0);
        assert_issuer_receipts(true);
    }
}

#[test]
fn failed_fade_construction_keeps_actual_partial_work_and_drops_all_four_products() {
    for domain_read in [false, true] {
        let (plan, provider) = fixture();
        let domain = plan
            .audio_domain_at(AudioSample(0), Default::default())
            .unwrap();
        let cancelled = AtomicBool::new(false);
        let measured = RefCell::new(ReadWork::default());
        let mut reader = StageAudio::new(Arc::clone(&plan));
        let reference = prepare(
            &mut reader,
            &plan,
            domain_read.then_some(&domain),
            AudioSample(0),
            work_control(&cancelled, &measured),
            true,
        )
        .unwrap();
        let (canonical, additional) = fade_units(&reference);
        let fade_total = canonical + additional;
        let needed = measured.borrow().plan_work;
        let fade_start = needed - fade_total;
        assert!(fade_total > 1);
        drop(reference);
        assert_eq!(measured.borrow().sound_context_frames, 0);
        for remaining in [needed, needed - 1] {
            let work = RefCell::new(ReadWork {
                plan_work: MAX_PLAN_WORK_PER_READ - remaining,
                ..Default::default()
            });
            let before = work.borrow().plan_work;
            flattened_work_trace::reset();
            let result = prepare(
                &mut reader,
                &plan,
                domain_read.then_some(&domain),
                AudioSample(0),
                work_control(&cancelled, &work),
                true,
            );
            if remaining == needed {
                let request = result.unwrap();
                assert_eq!(fade_units(&request), (canonical, additional));
                assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
                assert_eq!(fade_plan_trace()["completed_queries"], 1);
                drop(request);
            } else {
                let error = result
                    .err()
                    .expect("one short cannot complete the actual fade constructor");
                assert!(matches!(
                    error,
                    StageAudioError::Limit("plan work per read")
                        | StageAudioError::Plan(PlanError::AudioQueryLimit(_))
                ));
                let accepted = work.borrow().plan_work - before;
                assert!(accepted > fade_start);
                assert!(accepted < needed);
                let trace = fade_plan_trace();
                assert_eq!(trace["constructor_calls"], 1);
                assert_eq!(trace["completed_queries"], 0);
                assert_eq!(trace["failed_queries"], 1);
                assert_eq!(
                    trace["accepted_work"].as_u64().unwrap(),
                    (accepted - fade_start) as u64
                );
                assert!(trace["partial_work"].as_u64().unwrap() > 0);
                assert!(trace["partial_work"].as_u64().unwrap() < fade_total as u64);
                assert_eq!(
                    trace["accepted_work"].as_u64().unwrap()
                        - trace["host_eviction_work"].as_u64().unwrap(),
                    trace["partial_work"].as_u64().unwrap()
                );
                assert_eq!(trace["issuer_work"], 0);
            }
            assert_eq!(work.borrow().sound_context_frames, 0);
            assert_eq!(reader.active_frames, 0);
        }
        assert_eq!(provider.calls, 0);
    }
}

#[test]
fn four_product_consumption_has_an_exact_four_unit_boundary_before_source_io() {
    for domain_read in [false, true] {
        for remaining in [4, 3] {
            let (plan, mut provider) = fixture();
            let domain = plan
                .audio_domain_at(AudioSample(0), Default::default())
                .unwrap();
            let cancelled = AtomicBool::new(false);
            let work = RefCell::new(ReadWork::default());
            let control = work_control(&cancelled, &work);
            let mut reader = StageAudio::new(Arc::clone(&plan));
            let request = prepare(
                &mut reader,
                &plan,
                domain_read.then_some(&domain),
                AudioSample(0),
                control,
                true,
            )
            .unwrap();
            assert_three_carriers(&request);
            fade_units(&request);
            work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - remaining;
            flattened_work_trace::reset();
            let result = reader.read_prepared_original(&mut provider, request, control, 0);
            if remaining == 4 {
                let block = result.unwrap();
                assert_eq!(block.samples.len(), 64);
                assert!(provider.calls > 0);
                assert_issuer_receipts(true);
            } else {
                assert!(matches!(
                    result,
                    Err(StageAudioError::Limit("plan work per read"))
                ));
                assert_eq!(provider.calls, 0);
                assert_eq!(fade_plan_trace()["issuer_checks"], 0);
                assert_eq!(fade_plan_trace()["issuer_work"], 0);
            }
            assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
            assert_eq!(work.borrow().sound_context_frames, 0);
            assert_eq!(reader.active_frames, 0);
        }
    }
}

#[test]
fn fade_charge_outlives_preparation_reduces_real_pcm_room_and_drops_without_refund() {
    for domain_read in [false, true] {
        let (plan, provider) = fixture();
        let domain = plan
            .audio_domain_at(AudioSample(0), Default::default())
            .unwrap();
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = work_control(&cancelled, &work);
        let mut reader = StageAudio::new(Arc::clone(&plan));
        let mut request = prepare(
            &mut reader,
            &plan,
            domain_read.then_some(&domain),
            AudioSample(0),
            control,
            true,
        )
        .unwrap();
        assert_three_carriers(&request);
        fade_units(&request);
        let live = work.borrow().sound_context_frames;
        let spent = work.borrow().plan_work;
        assert!(live > 0);
        let pcm = 3_u64; // reserve(1,1) owns two input frames and one output.
        reader.limits.maximum_resident_frames = u32::try_from(live + pcm - 1).unwrap();
        assert!(matches!(
            reader.reserve(1, 1, control),
            Err(StageAudioError::Limit("resident PCM or stage cache"))
        ));
        assert_eq!(reader.active_frames, 0);
        let fades = request.queries.fades.take().unwrap();
        assert_eq!(work.borrow().sound_context_frames, live);
        drop(fades);
        let first_three = work.borrow().sound_context_frames;
        assert!(first_three > 0 && first_three < live);
        let reserved = reader.reserve(1, 1, control).unwrap();
        assert_eq!(reserved, pcm);
        reader.active_frames -= reserved;
        drop(request);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(work.borrow().plan_work, spent);
        assert_eq!(provider.calls, 0);
        assert_eq!(reader.active_frames, 0);
    }
}
