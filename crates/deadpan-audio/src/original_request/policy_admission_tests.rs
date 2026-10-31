//! Real Original policy leases, issuer checks, partial work and WAV failures.
use super::*;

fn policy_snapshot() -> serde_json::Value {
    serde_json::to_value(flattened_work_trace::policy_snapshot()).unwrap()
}

fn policy_plan_trace() -> serde_json::Value {
    let trace = policy_snapshot();
    for flag in ["nested", "invalid", "overflow"] {
        assert_eq!(trace[flag], false, "{trace}");
    }
    assert_eq!(trace["active_depth"], 0);
    trace["plan"].clone()
}

fn policy_units(request: &PreparedOriginalRequest<'_, '_, '_>) -> (usize, usize) {
    let Policy::Admitted { query, .. } = &request.queries.policy else {
        panic!("default Original policy must be admitted");
    };
    assert!(query.was_forwarded());
    let canonical = query.canonical_units();
    let additional = query.additional_units();
    assert_eq!(query.work(), canonical.checked_add(additional).unwrap());
    assert_eq!(request.queries().policy.work, query.work());
    (canonical, additional)
}

fn prepare<'context, 'plan, 'work>(
    reader: &mut StageAudio,
    plan: &'plan RenderPlan,
    domain: Option<&'context AudioDomain<'plan>>,
    control: WorkControl<'work>,
    edge_fades: bool,
) -> Result<PreparedOriginalRequest<'context, 'plan, 'work>, StageAudioError>
where
    'plan: 'context,
{
    if let Some(domain) = domain {
        reader.prepare_original_domain(domain, AudioSample(0), 64, control, edge_fades)
    } else {
        reader.prepare_original_root(plan, AudioSample(0), 64, control, edge_fades)
    }
}

#[test]
fn policy_is_admitted_once_and_kept_live_through_the_real_source_failure() {
    struct RefuseWithAllMetadataLive<'work> {
        _inner: Provider,
        work: &'work RefCell<ReadWork>,
        expected_live: u64,
        observed: usize,
    }
    impl AudioSourceProvider for RefuseWithAllMetadataLive<'_> {
        fn source(
            &mut self,
            _project: &ProjectId,
            _revision: &RevisionId,
            _asset: &AssetId,
            _cancelled: &AtomicBool,
        ) -> Result<&crate::PreparedSource, PreparationError> {
            assert_eq!(self.work.borrow().sound_context_frames, self.expected_live);
            self.observed += 1;
            Err(PreparationError::SourceUnavailable(
                "refused with all three Original metadata leases live".into(),
            ))
        }
    }

    for domain_read in [false, true] {
        let (plan, provider) = fixture();
        let domain = plan
            .audio_domain_at(AudioSample(0), Default::default())
            .unwrap();
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = work_control(&cancelled, &work);
        let mut reader = StageAudio::new(Arc::clone(&plan));
        flattened_work_trace::reset();
        let request = prepare(
            &mut reader,
            &plan,
            domain_read.then_some(&domain),
            control,
            false,
        )
        .unwrap();
        let (policy_canonical, policy_additional) = policy_units(&request);
        let live = work.borrow().sound_context_frames;
        let prepared_work = work.borrow().plan_work;
        assert!(live > 0 && policy_additional > 0);
        let policy_total = policy_canonical + policy_additional;
        assert_eq!(request.queries().policy.work, policy_total);
        assert_eq!(policy_plan_trace()["constructor_calls"], 1);
        assert_eq!(policy_plan_trace()["completed_queries"], 1);
        assert_eq!(policy_plan_trace()["failed_queries"], 0);
        assert_eq!(policy_plan_trace()["accepted_work"], policy_total);
        assert_eq!(policy_plan_trace()["canonical_units"], policy_canonical);
        assert_eq!(policy_plan_trace()["additional_units"], policy_additional);
        assert_eq!(policy_plan_trace()["issuer_work"], 0);
        assert_eq!(provider.calls, 0);

        let mut provider = RefuseWithAllMetadataLive {
            _inner: provider,
            work: &work,
            expected_live: live,
            observed: 0,
        };
        assert!(matches!(
            reader.read_prepared_original(&mut provider, request, control, 0),
            Err(StageAudioError::Preparation(
                PreparationError::SourceUnavailable(_)
            ))
        ));
        assert_eq!(provider.observed, 1);
        assert_eq!(provider._inner.calls, 0);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(work.borrow().plan_work, prepared_work + 3);
        assert_eq!(policy_plan_trace()["issuer_work"], 1);
        assert_eq!(policy_plan_trace()["issuer_checks"], 1);
        assert_eq!(reader.active_frames, 0);
    }
}

#[test]
fn policy_partial_failure_keeps_actual_debits_and_releases_all_query_leases() {
    for domain_read in [false, true] {
        let (plan, _) = fixture();
        let domain = plan
            .audio_domain_at(AudioSample(0), Default::default())
            .unwrap();
        let cancelled = AtomicBool::new(false);
        let mut reader = StageAudio::new(Arc::clone(&plan));
        let measured = RefCell::new(ReadWork::default());
        let reference = prepare(
            &mut reader,
            &plan,
            domain_read.then_some(&domain),
            work_control(&cancelled, &measured),
            false,
        )
        .unwrap();
        let (policy_canonical, policy_additional) = policy_units(&reference);
        let policy_total = policy_canonical + policy_additional;
        let needed = measured.borrow().plan_work;
        let policy_start = needed - policy_total;
        drop(reference);
        assert_eq!(measured.borrow().sound_context_frames, 0);

        let work = RefCell::new(ReadWork {
            plan_work: MAX_PLAN_WORK_PER_READ - (needed - 1),
            ..Default::default()
        });
        let before = work.borrow().plan_work;
        flattened_work_trace::reset();
        let error = prepare(
            &mut reader,
            &plan,
            domain_read.then_some(&domain),
            work_control(&cancelled, &work),
            false,
        )
        .err()
        .expect("the actual policy constructor cannot complete one work unit short");
        assert!(matches!(
            error,
            StageAudioError::Limit("plan work per read")
                | StageAudioError::Plan(PlanError::AudioQueryLimit(_))
        ));
        let accepted_after_failure = work.borrow().plan_work - before;
        assert!(accepted_after_failure > policy_start);
        assert!(accepted_after_failure < needed);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(policy_plan_trace()["constructor_calls"], 1);
        assert_eq!(policy_plan_trace()["completed_queries"], 0);
        assert_eq!(policy_plan_trace()["failed_queries"], 1);
        assert!(policy_plan_trace()["partial_work"].as_u64().unwrap() > 0);
        assert!(policy_plan_trace()["partial_work"].as_u64().unwrap() < policy_total as u64);
        assert_eq!(
            policy_plan_trace()["accepted_work"].as_u64().unwrap(),
            (work.borrow().plan_work - before - policy_start) as u64
        );
        let policy_lane = policy_plan_trace();
        assert_eq!(
            policy_lane["accepted_work"].as_u64().unwrap()
                - policy_lane["host_eviction_work"].as_u64().unwrap(),
            policy_lane["partial_work"].as_u64().unwrap()
        );
        assert_eq!(policy_plan_trace()["issuer_work"], 0);
    }
}

#[test]
fn only_the_policy_carrier_can_have_a_foreign_readwork_issuer() {
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
            owner_control,
            false,
        )
        .unwrap();
        let mut foreign = prepare(
            &mut reader,
            &plan,
            domain_read.then_some(&domain),
            foreign_control,
            false,
        )
        .unwrap();
        let Flattened::Admitted { issuer: flat, .. } = &owner.queries.flattened else {
            panic!("flat carrier must be admitted");
        };
        let Processing::Admitted {
            issuer: processing, ..
        } = &owner.queries.processing
        else {
            panic!("processing carrier must be admitted");
        };
        let Policy::Admitted { issuer: policy, .. } = &foreign.queries.policy else {
            panic!("foreign policy carrier must be admitted");
        };
        assert!(*flat == sound_context_admission::issuer(owner_control));
        assert!(*processing == sound_context_admission::issuer(owner_control));
        assert!(*policy == sound_context_admission::issuer(foreign_control));
        std::mem::swap(&mut owner.queries.policy, &mut foreign.queries.policy);
        drop(foreign);
        let owner_before = owner_work.borrow().plan_work;
        let foreign_before = foreign_work.borrow().plan_work;
        assert!(owner_work.borrow().sound_context_frames > 0);
        assert!(foreign_work.borrow().sound_context_frames > 0);

        flattened_work_trace::reset();
        assert!(matches!(
            reader.read_prepared_original(&mut provider, owner, owner_control, 0),
            Err(StageAudioError::Plan(
                PlanError::AudioMetadataIssuerMismatch
            ))
        ));
        assert_eq!(provider.calls, 0);
        assert_eq!(owner_work.borrow().plan_work, owner_before + 3);
        assert_eq!(foreign_work.borrow().plan_work, foreign_before);
        assert_eq!(owner_work.borrow().sound_context_frames, 0);
        assert_eq!(foreign_work.borrow().sound_context_frames, 0);
        for (product, snapshot) in [
            ("flattened", flattened_work_trace::snapshot()),
            ("processing", flattened_work_trace::processing_snapshot()),
            ("policy", flattened_work_trace::policy_snapshot()),
        ] {
            let lane = serde_json::to_value(snapshot).unwrap()["plan"].clone();
            assert_eq!(lane["issuer_checks"], 1, "{product}: {lane}");
            assert_eq!(lane["issuer_work"], 1, "{product}: {lane}");
        }
    }
}

#[test]
fn unused_policy_carrier_releases_its_real_readwork_charge_without_refund() {
    for domain_read in [false, true] {
        let (plan, _) = fixture();
        let domain = plan
            .audio_domain_at(AudioSample(0), Default::default())
            .unwrap();
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let mut reader = StageAudio::new(Arc::clone(&plan));
        let request = prepare(
            &mut reader,
            &plan,
            domain_read.then_some(&domain),
            work_control(&cancelled, &work),
            false,
        )
        .unwrap();
        let spent = work.borrow().plan_work;
        let live = work.borrow().sound_context_frames;
        assert!(spent > 0 && live > 0);
        assert!(matches!(&request.queries.policy, Policy::Admitted { .. }));
        drop(request);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(work.borrow().plan_work, spent);
        assert_eq!(reader.active_frames, 0);
    }
}
