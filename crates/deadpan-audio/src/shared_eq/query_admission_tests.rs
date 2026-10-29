use super::*;
use deadpan_plan::AudioSoundContextAdmission;
use std::alloc::Layout;

struct FirstAllocation<'work> {
    control: WorkControl<'work>,
    forwarded_units: usize,
    reservations: usize,
}
impl AudioSoundContextAdmission for FirstAllocation<'_> {
    type Error = StageAudioError;
    type Issuer = ();
    type Lease = ();
    fn issuer(&self) {}
    fn spend(&mut self, units: usize) -> Result<(), Self::Error> {
        self.control.spend_plan_work(units)?;
        self.forwarded_units += units;
        Ok(())
    }
    fn reserve(&mut self, _: Layout) -> Result<(), Self::Error> {
        self.control.check()?;
        self.reservations += 1;
        Err(StageAudioError::Limit("query allocation sentinel"))
    }
    fn merge(&mut self, _: &mut (), _: &mut ()) {}
}

#[test]
fn actual_numerical_original_path_reaches_query_admission_before_legacy_capture() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let cancelled = AtomicBool::new(false);
    let query_work = RefCell::new(ReadWork::default());
    let query_control = fixture::control(&cancelled, &query_work);
    let mut query_policy = FirstAllocation {
        control: query_control,
        forwarded_units: 0,
        reservations: 0,
    };
    assert!(matches!(
        plan.audio_owner_input_candidates_at_admitted(
            AudioSample(128),
            query_control.query_limits().unwrap(),
            &mut query_policy
        ),
        Err(StageAudioError::Limit("query allocation sentinel"))
    ));
    assert_eq!(query_policy.reservations, 1);
    let actual_work = RefCell::new(ReadWork::default());
    let actual_control = fixture::control(&cancelled, &actual_work);
    let mut actual_policy = FirstAllocation {
        control: actual_control,
        forwarded_units: 0,
        reservations: 0,
    };
    assert!(matches!(
        original_context_admitted(
            &plan,
            &Context::Root,
            &OwnerKey::new(&owner),
            AudioSample(128)..AudioSample(129),
            AudioSample(128),
            actual_control,
            &mut actual_policy
        ),
        Err(StageAudioError::Limit("query allocation sentinel"))
    ));
    assert_eq!(actual_policy.reservations, 1);
    assert_eq!(actual_policy.forwarded_units, query_policy.forwarded_units);
    assert_eq!(
        actual_work.borrow().plan_work,
        query_work.borrow().plan_work
    );
    let frozen: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-query-baseline-v1/work-receipts.json"
    ))
    .unwrap();
    let old_complete = frozen["cases"][0]["expected_success_receipt_total"]
        .as_u64()
        .unwrap() as usize;
    assert_eq!(old_complete, 22);
    assert!(
        actual_work.borrow().plan_work < old_complete,
        "query refusal precedes the frozen complete query/capture/probe"
    );
    assert_eq!(actual_work.borrow().sound_context_frames, 0);
}

#[test]
fn legacy_numerical_query_receipts_still_equal_the_pre_admission_golden() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let input = plan
        .audio_owner_input(&owner, AudioSample(128), Default::default())
        .unwrap();
    let domain = Context::Domain(Arc::new(input.context().clone()));
    let retained = retained_plan();
    let query = retained
        .audio_gain_owners(AudioSample(128)..AudioSample(129), Default::default())
        .unwrap();
    let tone = query.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node.as_str() == "tone")
        .unwrap();
    let actual = serde_json::json!([
        super::legacy_query_receipts::one(&plan, &Context::Root, &owner, "unbound_eq_root"),
        super::legacy_query_receipts::one(&plan, &domain, &owner, "unbound_eq_actual_domain"),
        super::legacy_query_receipts::one(
            &retained,
            &Context::Root,
            tone,
            "accepted_retained_tone"
        )
    ]);
    let frozen: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-query-baseline-v1/work-receipts.json"
    ))
    .unwrap();
    assert_eq!(actual, frozen["cases"]);
}
