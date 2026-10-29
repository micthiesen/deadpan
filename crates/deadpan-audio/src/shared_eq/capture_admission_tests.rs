use super::*;
use deadpan_plan::AudioSoundContextAdmission;
use std::alloc::Layout;

struct Counting<'work, const M: bool> {
    control: WorkControl<'work>,
    forwarded: usize,
    reservations: usize,
    fail_on: Option<usize>,
}
impl<'work, const M: bool> Counting<'work, M> {
    fn new(control: WorkControl<'work>) -> Self {
        Self {
            control,
            forwarded: 0,
            reservations: 0,
            fail_on: None,
        }
    }
}
impl<const M: bool> AudioSoundContextAdmission for Counting<'_, M> {
    const METERED: bool = M;
    type Error = StageAudioError;
    type Issuer = ();
    type Lease = ();
    fn issuer(&self) {}
    fn spend(&mut self, units: usize) -> Result<(), Self::Error> {
        if M {
            self.control.spend_plan_work(units)?;
            self.forwarded = self.forwarded.checked_add(units).unwrap();
        }
        Ok(())
    }
    fn reserve(&mut self, _: Layout) -> Result<(), Self::Error> {
        self.control.check()?;
        self.reservations += 1;
        if M && self.fail_on == Some(self.reservations) {
            return Err(StageAudioError::Limit("capture allocation sentinel"));
        }
        Ok(())
    }
    fn merge(&mut self, _: &mut (), _: &mut ()) {}
}

#[test]
fn numerical_original_capture_refusal_matches_the_actual_sealed_capture_prefix() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let key = OwnerKey::new(&owner);
    let cancelled = AtomicBool::new(false);
    let standalone_work = RefCell::new(ReadWork::default());
    let standalone_control = fixture::control(&cancelled, &standalone_work);
    let mut standalone = Counting::<true>::new(standalone_control);
    let candidates = plan
        .audio_owner_input_candidates_at_admitted(
            AudioSample(128),
            standalone_control.query_limits().unwrap(),
            &mut standalone,
        )
        .unwrap();
    let selected = candidates.query().spans()[0]
        .owners()
        .iter()
        .position(|clock| OwnerKey::new(clock) == key)
        .unwrap();
    let first_capture_reservation = standalone.reservations + 1;
    standalone.fail_on = Some(first_capture_reservation);
    assert!(matches!(
        candidates.capture(
            selected,
            standalone_control.query_limits().unwrap(),
            &mut standalone
        ),
        Err(StageAudioError::Limit("capture allocation sentinel"))
    ));
    assert_eq!(standalone.reservations, first_capture_reservation);

    let actual_work = RefCell::new(ReadWork::default());
    let actual_control = fixture::control(&cancelled, &actual_work);
    let mut actual = Counting::<true>::new(actual_control);
    actual.fail_on = Some(first_capture_reservation);
    assert!(matches!(
        original_context_admitted(
            &plan,
            &Context::Root,
            &key,
            AudioSample(128)..AudioSample(129),
            AudioSample(128),
            actual_control,
            &mut actual
        ),
        Err(StageAudioError::Limit("capture allocation sentinel"))
    ));
    assert_eq!(actual.reservations, first_capture_reservation);
    assert_eq!(actual.forwarded, standalone.forwarded);
    assert_eq!(
        actual_work.borrow().plan_work,
        standalone_work.borrow().plan_work
    );
}

fn success_work<const M: bool>() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut policy = Counting::<M>::new(control);
    let resolved = original_context_admitted(
        &plan,
        &Context::Root,
        &OwnerKey::new(&owner),
        AudioSample(128)..AudioSample(129),
        AudioSample(128),
        control,
        &mut policy,
    )
    .unwrap()
    .unwrap();
    assert!(matches!(&resolved.context, Context::Domain(_)));
    let frozen: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-query-baseline-v1/work-receipts.json"
    ))
    .unwrap();
    let old = &frozen["cases"][0];
    assert_eq!(old["case"], "unbound_eq_root");
    assert_eq!(old["retained_result"], "None");
    let probe_work = old["retained_probe_work"].as_u64().unwrap() as usize;
    assert_eq!(probe_work, 9);
    if M {
        // Includes every actual Q/C/final-sharing policy debit. Only the
        // unchanged legacy None probe is billed outside those callbacks.
        assert_eq!(work.borrow().plan_work, policy.forwarded + probe_work);
    } else {
        assert_eq!(policy.forwarded, 0);
        assert_eq!(
            work.borrow().plan_work,
            old["expected_success_receipt_total"].as_u64().unwrap() as usize
        );
    }
    drop(resolved);
}
#[test]
fn numerical_original_capture_work_reaches_controller_once_in_both_modes() {
    success_work::<true>();
    success_work::<false>();
}
