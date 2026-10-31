//! Real ReadWork leases, rather than a second test accounting policy.
use super::*;

struct LeaseWatchingProvider<'work> {
    inner: Provider,
    work: &'work RefCell<ReadWork>,
    expected_minimum: u64,
    observed: usize,
    refuse: bool,
}
impl AudioSourceProvider for LeaseWatchingProvider<'_> {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        assert!(self.expected_minimum > 0);
        assert!(
            self.work.borrow().sound_context_frames >= self.expected_minimum,
            "actual numerical source admission must retain the flattened query lease"
        );
        self.observed += 1;
        if self.refuse {
            return Err(PreparationError::SourceUnavailable(
                "refused with flattened lease live".into(),
            ));
        }
        self.inner.source(project, revision, asset, cancelled)
    }
}

#[test]
fn flattened_request_lease_is_live_during_real_source_io_and_dies_before_returned_pcm() {
    for domain_read in [false, true] {
        for edge_fades in [false, true] {
            let (plan, provider) = fixture();
            let domain = plan
                .audio_domain_at(AudioSample(0), Default::default())
                .unwrap();
            let mut reader = StageAudio::new(Arc::clone(&plan));
            let cancelled = AtomicBool::new(false);
            let work = RefCell::new(ReadWork::default());
            let control = work_control(&cancelled, &work);
            let request = if domain_read {
                reader.prepare_original_domain(&domain, AudioSample(0), 64, control, edge_fades)
            } else {
                reader.prepare_original_root(&plan, AudioSample(0), 64, control, edge_fades)
            }
            .unwrap();
            let live = work.borrow().sound_context_frames;
            assert!(live > 0);
            match &request.queries.flattened {
                Flattened::Admitted { query, issuer } => {
                    assert!(*issuer == sound_context_admission::issuer(control));
                    assert!(query.was_forwarded());
                    assert!(query.additional_units() > 0);
                    assert_eq!(
                        query.work(),
                        query.canonical_units() + query.additional_units()
                    );
                    assert!(work.borrow().plan_work >= query.work());
                }
                Flattened::Legacy(_) => panic!("production preparation used legacy flattening"),
            }
            assert!(work.borrow().preflight_assets.is_empty());
            assert_eq!(provider.calls, 0);
            let mut provider = LeaseWatchingProvider {
                inner: provider,
                work: &work,
                expected_minimum: live,
                observed: 0,
                refuse: false,
            };
            let block = reader
                .read_prepared_original(&mut provider, request, control, 0)
                .unwrap();
            assert!(provider.observed > 0);
            assert_eq!(block.samples.len(), 64);
            assert_eq!(
                work.borrow().sound_context_frames,
                0,
                "returned PCM does not retain flattened metadata"
            );
            assert_eq!(reader.active_frames, 0);
            drop(block);
            assert_eq!(work.borrow().sound_context_frames, 0);
        }
    }
}

#[test]
fn flattened_unused_and_failed_prepared_requests_release_live_real_ledger_charge() {
    for domain_read in [false, true] {
        for failure in 0..5 {
            let (plan, provider) = fixture();
            let domain = plan
                .audio_domain_at(AudioSample(0), Default::default())
                .unwrap();
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
            let live = work.borrow().sound_context_frames;
            let spent = work.borrow().plan_work;
            assert!(live > 0 && spent > 0);
            if failure == 0 {
                drop(request);
                assert_eq!(work.borrow().sound_context_frames, 0);
                assert_eq!(work.borrow().plan_work, spent);
                assert_eq!(provider.calls, 0);
                continue;
            }
            let mut provider = LeaseWatchingProvider {
                inner: provider,
                work: &work,
                expected_minimum: live,
                observed: 0,
                refuse: failure == 4,
            };
            if failure == 1 {
                cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            if failure == 2 {
                control.deadline = Instant::now() - Duration::from_secs(1);
            }
            let depth = if failure == 3 {
                reader.limits.maximum_depth + 1
            } else {
                0
            };
            let error = match reader.read_prepared_original(&mut provider, request, control, depth)
            {
                Err(error) => error,
                Ok(_) => panic!("expected prepared read refusal"),
            };
            match failure {
                1 => assert!(error.is_cancelled()),
                2 => assert!(matches!(error, StageAudioError::Timeout)),
                3 => assert!(matches!(
                    error,
                    StageAudioError::Limit("nested stage depth")
                )),
                4 => assert!(matches!(
                    error,
                    StageAudioError::Preparation(PreparationError::SourceUnavailable(_))
                )),
                _ => unreachable!(),
            }
            assert_eq!(work.borrow().sound_context_frames, 0);
            assert!(
                work.borrow().plan_work >= spent,
                "successful work is never refunded"
            );
            assert_eq!(reader.active_frames, 0);
            if failure < 4 {
                assert_eq!(provider.observed, 0);
                assert_eq!(provider.inner.calls, 0);
                assert!(work.borrow().preflight_assets.is_empty());
            } else {
                assert_eq!(provider.observed, 1);
            }
        }
    }
}

#[test]
fn flattened_foreign_readwork_is_refused_before_query_read_and_source_admission() {
    for domain_read in [false, true] {
        let (plan, mut provider) = fixture();
        let domain = plan
            .audio_domain_at(AudioSample(0), Default::default())
            .unwrap();
        let mut reader = StageAudio::new(Arc::clone(&plan));
        let cancelled = AtomicBool::new(false);
        let original_work = RefCell::new(ReadWork::default());
        let foreign_work = RefCell::new(ReadWork::default());
        let original = work_control(&cancelled, &original_work);
        let foreign = work_control(&cancelled, &foreign_work);
        let request = if domain_read {
            reader.prepare_original_domain(&domain, AudioSample(0), 64, original, true)
        } else {
            reader.prepare_original_root(&plan, AudioSample(0), 64, original, true)
        }
        .unwrap();
        let spent = original_work.borrow().plan_work;
        assert!(original_work.borrow().sound_context_frames > 0);
        assert!(matches!(
            reader.read_prepared_original(&mut provider, request, foreign, 0),
            Err(StageAudioError::Plan(
                PlanError::AudioMetadataIssuerMismatch
            ))
        ));
        assert_eq!(provider.calls, 0);
        assert_eq!(original_work.borrow().sound_context_frames, 0);
        assert_eq!(foreign_work.borrow().sound_context_frames, 0);
        assert_eq!(original_work.borrow().plan_work, spent);
        assert_eq!(
            foreign_work.borrow().plan_work,
            1,
            "only actual issuer comparison is billed"
        );
        assert!(original_work.borrow().preflight_assets.is_empty());
        assert!(foreign_work.borrow().preflight_assets.is_empty());
        assert_eq!(foreign_work.borrow().source_checks, 0);
        assert_eq!(foreign_work.borrow().prepared_stages, 0);
    }
}

#[test]
fn flattened_live_charge_reduces_actual_required_pcm_reservation_room() {
    for domain_read in [false, true] {
        let (plan, provider) = fixture();
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
        let live = work.borrow().sound_context_frames;
        assert!(live > 0);
        assert_eq!(reader.cached_resident_frames(), 0);
        assert_eq!(reader.active_frames, 0);
        let pcm = 3_u64; // The actual reserve(1,1) formula is two input plus one output.
        reader.limits.maximum_resident_frames = u32::try_from(live + pcm - 1).unwrap();
        assert!(matches!(
            reader.reserve(1, 1, control),
            Err(StageAudioError::Limit("resident PCM or stage cache"))
        ));
        assert_eq!(reader.active_frames, 0);
        assert_eq!(work.borrow().sound_context_frames, live);
        drop(request);
        assert_eq!(work.borrow().sound_context_frames, 0);
        let reserved = reader.reserve(1, 1, control).unwrap();
        assert_eq!(reserved, pcm);
        assert_eq!(reader.active_frames, pcm);
        reader.active_frames -= reserved;
        assert_eq!(provider.calls, 0);
    }
}

#[test]
fn flattened_later_processing_failure_drops_earlier_flattened_lease_without_refunding_work() {
    for domain_read in [false, true] {
        let (plan, provider) = fixture();
        let domain = plan
            .audio_domain_at(AudioSample(0), Default::default())
            .unwrap();
        let cancelled = AtomicBool::new(false);
        let mut reader = StageAudio::new(Arc::clone(&plan));
        let measured = RefCell::new(ReadWork::default());
        let measure_control = work_control(&cancelled, &measured);
        let flat = reader
            .prepare_flattened(
                if domain_read {
                    Origin::Domain(&domain)
                } else {
                    Origin::Root(&plan)
                },
                AudioSample(0)..AudioSample(64),
                measure_control,
            )
            .unwrap();
        let flat_work = measured.borrow().plan_work;
        assert!(flat_work > 0 && measured.borrow().sound_context_frames > 0);
        drop(flat);
        assert_eq!(measured.borrow().sound_context_frames, 0);
        let processing_work = if domain_read {
            domain
                .processing(AudioSample(0)..AudioSample(64), Default::default())
                .unwrap()
                .work
        } else {
            plan.audio_processing(AudioSample(0)..AudioSample(64), Default::default())
                .unwrap()
                .work
        };
        assert!(
            processing_work > 1,
            "fixture must fail inside later processing, not before it"
        );
        let remaining = flat_work + processing_work - 1;
        let initial = MAX_PLAN_WORK_PER_READ - remaining;
        let work = RefCell::new(ReadWork {
            plan_work: initial,
            ..Default::default()
        });
        let control = work_control(&cancelled, &work);
        let result = if domain_read {
            reader.prepare_original_domain(&domain, AudioSample(0), 64, control, true)
        } else {
            reader.prepare_original_root(&plan, AudioSample(0), 64, control, true)
        };
        assert!(matches!(
            result,
            Err(StageAudioError::Plan(PlanError::AudioQueryLimit(_)))
        ));
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(
            work.borrow().plan_work,
            initial + flat_work,
            "failed ordinary later constructor exposes no successful receipt; earlier real work remains spent"
        );
        assert_eq!(provider.calls, 0);
        assert!(work.borrow().preflight_assets.is_empty());
    }
}
