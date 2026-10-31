//! Real ReadWork/WAV boundaries; no alternate lease accounting policy.
use super::*;

fn processing_trace() -> serde_json::Value {
    serde_json::to_value(flattened_work_trace::processing_snapshot()).unwrap()
}
fn processing_plan_trace() -> serde_json::Value {
    let trace = processing_trace();
    for flag in ["nested", "invalid", "overflow"] {
        assert_eq!(trace[flag], false, "{trace}");
    }
    assert_eq!(trace["active_depth"], 0);
    trace["plan"].clone()
}
struct ProcessingWatchingProvider<'work> {
    inner: Provider,
    work: &'work RefCell<ReadWork>,
    live: u64,
    observed: usize,
    refuse: bool,
}
impl AudioSourceProvider for ProcessingWatchingProvider<'_> {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        assert!(self.live > 0);
        assert_eq!(
            self.work.borrow().sound_context_frames,
            self.live,
            "processing-only preparation retains its actual lease at WAV admission"
        );
        self.observed += 1;
        if self.refuse {
            return Err(PreparationError::SourceUnavailable(
                "revoked with processing lease live".into(),
            ));
        }
        self.inner.source(project, revision, asset, cancelled)
    }
}

#[test]
fn processing_only_carrier_bills_once_and_lives_through_wav_io_unused_drop_and_error() {
    for domain_read in [false, true] {
        for outcome in 0..3 {
            let (plan, provider) = fixture();
            let domain = plan
                .audio_domain_at(AudioSample(0), Default::default())
                .unwrap();
            let mut reader = StageAudio::new(Arc::clone(&plan));
            let cancelled = AtomicBool::new(false);
            let work = RefCell::new(ReadWork::default());
            let control = work_control(&cancelled, &work);
            flattened_work_trace::reset();
            let request = with_legacy_flattened_queries(|| {
                if domain_read {
                    reader.prepare_original_domain(&domain, AudioSample(0), 64, control, true)
                } else {
                    reader.prepare_original_root(&plan, AudioSample(0), 64, control, true)
                }
            })
            .unwrap();
            assert!(matches!(request.queries.flattened, Flattened::Legacy(_)));
            let Processing::Admitted { query, issuer } = &request.queries.processing else {
                panic!("default processing must be admitted");
            };
            assert!(*issuer == sound_context_admission::issuer(control));
            let canonical = query.canonical_units();
            let additional = query.additional_units();
            let total = query.work();
            assert!(additional > 0 && query.was_forwarded());
            assert_eq!(total, canonical + additional);
            let borrowed = request.queries();
            assert!(borrowed.processing.spans.iter().all(|span| matches!(
                span.content,
                AudioSignalContent::Leaf(AudioContent::Source { .. })
            )));
            let expected = if domain_read {
                domain
                    .processing(AudioSample(0)..AudioSample(64), Default::default())
                    .unwrap()
            } else {
                plan.audio_processing(AudioSample(0)..AudioSample(64), Default::default())
                    .unwrap()
            };
            assert_eq!(canonical, expected.work);
            assert_eq!(borrowed.processing.spans, expected.spans);
            let spent = work.borrow().plan_work;
            assert_eq!(
                spent,
                borrowed.flattened.work
                    + total
                    + borrowed.policy.work
                    + borrowed.fades.map_or(0, |fades| fades.work)
            );
            let trace = processing_plan_trace();
            assert_eq!(trace["constructor_calls"], 1);
            assert_eq!(trace["completed_queries"], 1);
            assert_eq!(trace["failed_queries"], 0);
            assert_eq!(trace["accepted_work"], total);
            assert_eq!(trace["canonical_units"], canonical);
            assert_eq!(trace["additional_units"], additional);
            assert_eq!(trace["issuer_work"], 0);
            let live = work.borrow().sound_context_frames;
            assert!(live > 0 && provider.calls == 0);
            assert!(work.borrow().preflight_assets.is_empty());
            if outcome == 0 {
                drop(request);
                assert_eq!(work.borrow().sound_context_frames, 0);
                assert_eq!(work.borrow().plan_work, spent);
                continue;
            }
            let mut provider = ProcessingWatchingProvider {
                inner: provider,
                work: &work,
                live,
                observed: 0,
                refuse: outcome == 2,
            };
            let result = reader.read_prepared_original(&mut provider, request, control, 0);
            if outcome == 1 {
                let block = result.unwrap();
                assert_eq!(block.samples.len(), 64);
                assert!(provider.inner.calls > 0);
                assert_eq!(
                    work.borrow().sound_context_frames,
                    0,
                    "returned WAV PCM owns no processing metadata"
                );
                drop(block);
            } else {
                assert!(matches!(
                    result,
                    Err(StageAudioError::Preparation(
                        PreparationError::SourceUnavailable(_)
                    ))
                ));
                assert_eq!(provider.inner.calls, 0);
            }
            if outcome == 1 {
                assert_eq!(provider.observed, provider.inner.calls);
                assert!(provider.observed > 0);
            } else {
                assert_eq!(provider.observed, 1);
            }
            assert_eq!(work.borrow().sound_context_frames, 0);
            assert_eq!(
                work.borrow().plan_work,
                spent + 1,
                "only processing issuer comparison is new in this explicit legacy-flat mode"
            );
            assert_eq!(processing_plan_trace()["issuer_work"], 1);
            assert_eq!(reader.active_frames, 0);
        }
    }
}

#[test]
fn processing_foreign_readwork_is_rejected_with_legacy_flattened_before_source_or_query_io() {
    for domain_read in [false, true] {
        let (plan, mut provider) = fixture();
        let domain = plan
            .audio_domain_at(AudioSample(0), Default::default())
            .unwrap();
        let mut reader = StageAudio::new(Arc::clone(&plan));
        let cancelled = AtomicBool::new(false);
        let original = RefCell::new(ReadWork::default());
        let foreign = RefCell::new(ReadWork::default());
        let control = work_control(&cancelled, &original);
        let request = with_legacy_flattened_queries(|| {
            if domain_read {
                reader.prepare_original_domain(&domain, AudioSample(0), 64, control, true)
            } else {
                reader.prepare_original_root(&plan, AudioSample(0), 64, control, true)
            }
        })
        .unwrap();
        assert!(matches!(request.queries.flattened, Flattened::Legacy(_)));
        assert!(matches!(
            request.queries.processing,
            Processing::Admitted { .. }
        ));
        assert!(original.borrow().sound_context_frames > 0);
        let spent = original.borrow().plan_work;
        assert!(matches!(
            reader.read_prepared_original(
                &mut provider,
                request,
                work_control(&cancelled, &foreign),
                0
            ),
            Err(StageAudioError::Plan(
                PlanError::AudioMetadataIssuerMismatch
            ))
        ));
        assert_eq!(provider.calls, 0);
        assert_eq!(original.borrow().plan_work, spent);
        assert_eq!(foreign.borrow().plan_work, 1);
        for ledger in [&original, &foreign] {
            assert_eq!(ledger.borrow().sound_context_frames, 0);
            assert!(ledger.borrow().preflight_assets.is_empty());
            assert_eq!(ledger.borrow().source_checks, 0);
            assert_eq!(ledger.borrow().prepared_stages, 0);
        }
    }
}

#[test]
fn processing_and_flattened_real_charges_add_and_each_reduces_required_pcm_room() {
    for domain_read in [false, true] {
        let (plan, provider) = fixture();
        let domain = plan
            .audio_domain_at(AudioSample(0), Default::default())
            .unwrap();
        let cancelled = AtomicBool::new(false);
        let flat_work = RefCell::new(ReadWork::default());
        let processing_work = RefCell::new(ReadWork::default());
        let both_work = RefCell::new(ReadWork::default());
        let mut reader = StageAudio::new(Arc::clone(&plan));
        let (flat_frames, processing_frames, request) = {
            let mut prepare = |control| {
                if domain_read {
                    reader.prepare_original_domain(&domain, AudioSample(0), 64, control, true)
                } else {
                    reader.prepare_original_root(&plan, AudioSample(0), 64, control, true)
                }
            };
            let flat =
                with_legacy_processing_queries(|| prepare(work_control(&cancelled, &flat_work)))
                    .unwrap();
            let flat_frames = flat_work.borrow().sound_context_frames;
            assert!(flat_frames > 0);
            drop(flat);
            let processing = with_legacy_flattened_queries(|| {
                prepare(work_control(&cancelled, &processing_work))
            })
            .unwrap();
            let processing_frames = processing_work.borrow().sound_context_frames;
            assert!(processing_frames > 0);
            drop(processing);
            let request = prepare(work_control(&cancelled, &both_work)).unwrap();

            (flat_frames, processing_frames, request)
        };
        let both = both_work.borrow().sound_context_frames;
        assert_eq!(both, flat_frames + processing_frames);
        assert_eq!(flat_work.borrow().sound_context_frames, 0);
        assert_eq!(processing_work.borrow().sound_context_frames, 0);
        let control = work_control(&cancelled, &both_work);
        assert_eq!(reader.cached_resident_frames(), 0);
        let pcm = 3_u64; // reserve(1,1) retains two input frames and one output.
        reader.limits.maximum_resident_frames = u32::try_from(both + pcm - 1).unwrap();
        assert!(matches!(
            reader.reserve(1, 1, control),
            Err(StageAudioError::Limit("resident PCM or stage cache"))
        ));
        assert_eq!(reader.active_frames, 0);
        let PreparedOriginalRequest {
            queries:
                PreparedQueries {
                    flattened,
                    processing,
                    ..
                },
            ..
        } = request;
        drop(processing);
        assert_eq!(both_work.borrow().sound_context_frames, flat_frames);
        let reserved = reader.reserve(1, 1, control).unwrap();
        assert_eq!(reserved, pcm);
        reader.active_frames -= reserved;
        drop(flattened);
        assert_eq!(both_work.borrow().sound_context_frames, 0);
        assert_eq!(provider.calls, 0);
    }
}

#[test]
fn processing_failed_constructor_keeps_real_partial_fees_and_releases_earlier_flattened_lease() {
    for domain_read in [false, true] {
        let (plan, provider) = fixture();
        let domain = plan
            .audio_domain_at(AudioSample(0), Default::default())
            .unwrap();
        let cancelled = AtomicBool::new(false);
        let measured = RefCell::new(ReadWork::default());
        let mut reader = StageAudio::new(Arc::clone(&plan));
        let request = if domain_read {
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
        let flat = request.queries().flattened.work;
        let processing = request.queries().processing.work;
        assert!(flat > 0 && processing > 1);
        drop(request);
        assert_eq!(measured.borrow().sound_context_frames, 0);
        let remaining = flat + processing - 1;
        let initial = MAX_PLAN_WORK_PER_READ - remaining;
        let work = RefCell::new(ReadWork {
            plan_work: initial,
            ..Default::default()
        });
        flattened_work_trace::reset();
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
        assert!(matches!(
            result,
            Err(StageAudioError::Plan(PlanError::AudioQueryLimit(
                "structural work"
            )))
        ));
        let trace = processing_plan_trace();
        assert_eq!(trace["constructor_calls"], 1);
        assert_eq!(trace["completed_queries"], 0);
        assert_eq!(trace["failed_queries"], 1);
        assert_eq!(trace["partial_work"], processing - 1);
        assert_eq!(trace["accepted_work"], processing - 1);
        assert_eq!(work.borrow().plan_work, initial + flat + processing - 1);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert!(work.borrow().preflight_assets.is_empty());
        assert_eq!(provider.calls, 0);
    }
}
