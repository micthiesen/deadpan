use super::*;
use deadpan_core::{ProjectDocument, RepeatInstance};
use deadpan_plan::AudioSoundContextAdmission;
use std::alloc::Layout;
use std::cell::Cell;
use std::rc::Rc;

const SNAPSHOT: &str =
    include_str!("../../../deadpan-plan/tests/fixtures/owner-query-baseline-v1/snapshot.json");

#[derive(Default)]
struct Ledger {
    live_bytes: Cell<usize>,
}

struct Lease {
    ledger: Rc<Ledger>,
    bytes: usize,
    active: bool,
}

impl Drop for Lease {
    fn drop(&mut self) {
        if self.active {
            self.ledger
                .live_bytes
                .set(self.ledger.live_bytes.get() - self.bytes);
        }
    }
}

struct Policy<const M: bool> {
    issuer: u8,
    ledger: Rc<Ledger>,
    spend_calls: usize,
    spent: usize,
    fail_spend: Option<usize>,
    reserve_calls: usize,
    layouts: Vec<Layout>,
    max_bytes: usize,
    fail_reserve: bool,
    rotate_after_spend: Option<usize>,
    rotate_after_reserve: bool,
    merges: usize,
}

impl<const M: bool> Policy<M> {
    fn new(max_bytes: usize) -> Self {
        Self {
            issuer: 7,
            ledger: Rc::new(Ledger::default()),
            spend_calls: 0,
            spent: 0,
            fail_spend: None,
            reserve_calls: 0,
            layouts: Vec::new(),
            max_bytes,
            fail_reserve: false,
            rotate_after_spend: None,
            rotate_after_reserve: false,
            merges: 0,
        }
    }
}

impl<const M: bool> AudioSoundContextAdmission for Policy<M> {
    const METERED: bool = M;
    type Error = StageAudioError;
    type Issuer = u8;
    type Lease = Lease;

    fn issuer(&self) -> u8 {
        self.issuer
    }

    fn spend(&mut self, work: usize) -> Result<(), StageAudioError> {
        self.spend_calls += 1;
        if self.fail_spend == Some(self.spend_calls) {
            return Err(StageAudioError::Limit("caller admission work sentinel"));
        }
        self.spent += work;
        if self.rotate_after_spend == Some(self.spend_calls) {
            self.issuer += 1;
        }
        Ok(())
    }

    fn reserve(&mut self, layout: Layout) -> Result<Lease, StageAudioError> {
        self.reserve_calls += 1;
        self.layouts.push(layout);
        if self.fail_reserve || layout.size() > self.max_bytes {
            return Err(StageAudioError::Limit("caller admission reserve sentinel"));
        }
        self.ledger
            .live_bytes
            .set(self.ledger.live_bytes.get() + layout.size());
        let lease = Lease {
            ledger: Rc::clone(&self.ledger),
            bytes: layout.size(),
            active: true,
        };
        if self.rotate_after_reserve {
            self.issuer += 1;
        }
        Ok(lease)
    }

    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        self.merges += 1;
        assert!(Rc::ptr_eq(&target.ledger, &source.ledger));
        target.bytes += source.bytes;
        source.active = false;
    }
}

fn repeat_plan() -> RenderPlan {
    let snapshot: serde_json::Value = serde_json::from_str(SNAPSHOT).unwrap();
    let document = snapshot["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|fixture| fixture["fixture"] == "stable_repeat_plays_default_gap_and_overrides")
        .unwrap()["document"]
        .clone();
    let document = ProjectDocument::from_json(&document.to_string()).unwrap();
    RenderPlan::compile(&document).unwrap()
}

fn repeat_owner(plan: &RenderPlan) -> AudioOwnerClock<'_> {
    let query = plan
        .audio_gain_owners(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    query.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node.as_str() == "voice")
        .filter(|owner| !owner.instance().repeats.is_empty())
        .unwrap()
        .clone()
}

fn empty_repeat_owner(plan: &RenderPlan) -> AudioOwnerClock<'_> {
    let query = plan
        .audio_gain_owners(AudioSample(128)..AudioSample(129), Default::default())
        .unwrap();
    query.spans()[0].owners()[0].clone()
}

fn repeat_bytes(owner: &AudioOwnerClock<'_>) -> usize {
    Layout::array::<RepeatInstance>(owner.instance().repeats.len())
        .unwrap()
        .size()
}

#[test]
fn unmetered_mode_keeps_the_legacy_key_and_does_not_call_admission() {
    let plan = crate::stages::authored_eq::eq_admission_fixture::fixture();
    let owner = empty_repeat_owner(&plan);
    let mut policy = Policy::<false>::new(usize::MAX);

    let stop = AdmittedStop::new(&owner, 13, &mut policy).unwrap();
    assert_eq!(stop.owner, OwnerKey::new(&owner));
    assert_eq!(stop.before, 13);
    assert_eq!(stop.issuer(), 7);
    assert!(stop.lease().is_none());
    assert_eq!(policy.spend_calls, 0);
    assert_eq!(policy.reserve_calls, 0);
    assert_eq!(policy.merges, 0);
}

#[test]
fn metered_mode_copies_exact_repeat_array_under_one_exact_live_lease() {
    let plan = repeat_plan();
    let owner = repeat_owner(&plan);
    let expected_bytes = repeat_bytes(&owner);
    assert!(expected_bytes > 0);
    let mut policy = Policy::<true>::new(expected_bytes);

    let stop = AdmittedStop::new(&owner, 4, &mut policy).unwrap();
    assert_eq!(stop.owner, OwnerKey::new(&owner));
    assert_eq!(stop.before, 4);
    assert_eq!(stop.issuer(), 7);
    assert_eq!(stop.owner.instance.repeats, owner.instance().repeats);
    assert_eq!(
        policy.layouts,
        vec![Layout::array::<RepeatInstance>(owner.instance().repeats.len()).unwrap()]
    );
    assert_eq!(policy.ledger.live_bytes.get(), expected_bytes);
    assert_eq!(stop.lease().unwrap().bytes, expected_bytes);
    assert_eq!(policy.spent, owner.instance().repeats.len() + 4);
    assert_eq!(policy.merges, 0);

    drop(stop);
    assert_eq!(policy.ledger.live_bytes.get(), 0);
    assert_eq!(
        policy.spent,
        owner.instance().repeats.len() + 4,
        "accepted work is not refunded when the key drops"
    );
}

#[test]
fn metered_empty_repeat_path_needs_no_array_lease() {
    let plan = crate::stages::authored_eq::eq_admission_fixture::fixture();
    let owner = empty_repeat_owner(&plan);
    assert!(owner.instance().repeats.is_empty());
    let mut policy = Policy::<true>::new(0);

    let stop = AdmittedStop::new(&owner, 2, &mut policy).unwrap();
    assert_eq!(stop.owner, OwnerKey::new(&owner));
    assert_eq!(stop.before, 2);
    assert!(stop.lease().is_none());
    assert_eq!(policy.spend_calls, 4);
    assert_eq!(policy.spent, 4);
    assert_eq!(policy.reserve_calls, 0);
}

#[test]
fn one_byte_short_repeat_array_reservation_is_refused_without_a_live_lease() {
    let plan = repeat_plan();
    let owner = repeat_owner(&plan);
    let expected_bytes = repeat_bytes(&owner);
    let mut policy = Policy::<true>::new(expected_bytes - 1);

    assert!(matches!(
        AdmittedStop::new(&owner, 0, &mut policy),
        Err(StageAudioError::Limit("caller admission reserve sentinel"))
    ));
    assert_eq!(policy.layouts[0].size(), expected_bytes);
    assert_eq!(policy.reserve_calls, 1);
    assert_eq!(policy.ledger.live_bytes.get(), 0);
    assert_eq!(policy.spent, 2);
}

#[test]
fn each_reached_work_spend_can_refuse_and_releases_prior_array_charge() {
    let plan = repeat_plan();
    let owner = repeat_owner(&plan);
    let reached_spends = owner.instance().repeats.len() + 4;
    for failure_at in 1..=reached_spends {
        let mut policy = Policy::<true>::new(usize::MAX);
        policy.fail_spend = Some(failure_at);

        assert!(matches!(
            AdmittedStop::new(&owner, 9, &mut policy),
            Err(StageAudioError::Limit("caller admission work sentinel"))
        ));
        assert_eq!(policy.spend_calls, failure_at);
        assert_eq!(policy.spent, failure_at - 1);
        assert_eq!(
            policy.ledger.live_bytes.get(),
            0,
            "failure at spend {failure_at}"
        );
        assert_eq!(policy.merges, 0);
    }
}

#[test]
fn reserve_refusal_has_no_lease_and_preserves_previously_accepted_work() {
    let plan = repeat_plan();
    let owner = repeat_owner(&plan);
    let mut policy = Policy::<true>::new(usize::MAX);
    policy.fail_reserve = true;

    assert!(matches!(
        AdmittedStop::new(&owner, 0, &mut policy),
        Err(StageAudioError::Limit("caller admission reserve sentinel"))
    ));
    assert_eq!(policy.spend_calls, 2);
    assert_eq!(policy.spent, 2);
    assert_eq!(policy.reserve_calls, 1);
    assert_eq!(policy.ledger.live_bytes.get(), 0);
}

#[test]
fn issuer_rotation_after_work_or_reservation_refuses_and_releases_the_lease() {
    let plan = repeat_plan();
    let owner = repeat_owner(&plan);

    let mut after_work = Policy::<true>::new(usize::MAX);
    after_work.rotate_after_spend = Some(1);
    assert!(matches!(
        AdmittedStop::new(&owner, 0, &mut after_work),
        Err(StageAudioError::Plan(
            PlanError::AudioMetadataIssuerMismatch
        ))
    ));
    assert_eq!(after_work.spent, 1);
    assert_eq!(after_work.reserve_calls, 0);
    assert_eq!(after_work.ledger.live_bytes.get(), 0);

    let mut after_reserve = Policy::<true>::new(usize::MAX);
    after_reserve.rotate_after_reserve = true;
    assert!(matches!(
        AdmittedStop::new(&owner, 0, &mut after_reserve),
        Err(StageAudioError::Plan(
            PlanError::AudioMetadataIssuerMismatch
        ))
    ));
    assert_eq!(after_reserve.spent, 2);
    assert_eq!(after_reserve.reserve_calls, 1);
    assert_eq!(after_reserve.ledger.live_bytes.get(), 0);
}

#[test]
fn unmetered_mode_never_attempts_the_synthetic_repeat_array_reservation() {
    let plan = repeat_plan();
    let owner = repeat_owner(&plan);
    let mut policy = Policy::<false>::new(0);
    policy.fail_reserve = true;
    policy.fail_spend = Some(1);

    let stop = AdmittedStop::new(&owner, 5, &mut policy).unwrap();
    assert_eq!(stop.owner, OwnerKey::new(&owner));
    assert_eq!(stop.before, 5);
    assert!(stop.lease().is_none());
    assert_eq!(policy.spend_calls, 0);
    assert_eq!(policy.reserve_calls, 0);
    assert_eq!(policy.merges, 0);
}

fn boundary_for(owner: &AudioOwnerClock<'_>, before: usize) -> Boundary {
    Boundary {
        owner: OwnerKey::new(owner),
        before,
    }
}

fn boundary_fields(boundary: &Boundary) -> serde_json::Value {
    serde_json::json!({
        "owner": boundary.owner.snapshot_fields(),
        "before": boundary.before,
    })
}

#[test]
fn borrowed_boundary_snapshot_preserves_exact_fields_without_unmetered_admission() {
    let plan = repeat_plan();
    let owner = repeat_owner(&plan);
    let boundary = boundary_for(&owner, 17);
    let expected = boundary_fields(&boundary);
    let mut policy = Policy::<false>::new(0);
    policy.fail_spend = Some(1);
    policy.fail_reserve = true;

    let snapshot = AdmittedStop::from_boundary(&boundary, &mut policy).unwrap();
    assert_eq!(boundary_fields(&snapshot), expected);
    assert_eq!(snapshot.issuer(), 7);
    assert!(snapshot.lease().is_none());
    assert_eq!(policy.spend_calls, 0);
    assert_eq!(policy.reserve_calls, 0);
    assert_eq!(policy.merges, 0);
    assert_eq!(policy.ledger.live_bytes.get(), 0);
}

#[test]
fn borrowed_boundary_snapshot_reserves_exact_repeat_bytes_before_copy() {
    let plan = repeat_plan();
    let owner = repeat_owner(&plan);
    let boundary = boundary_for(&owner, 23);
    let expected_bytes = repeat_bytes(&owner);
    assert!(expected_bytes > 0);
    let mut policy = Policy::<true>::new(expected_bytes);

    let snapshot = AdmittedStop::from_boundary(&boundary, &mut policy).unwrap();
    assert_eq!(boundary_fields(&snapshot), boundary_fields(&boundary));
    assert_eq!(
        snapshot.owner.instance.repeats,
        boundary.owner.instance.repeats
    );
    assert_eq!(snapshot.before, boundary.before);
    assert_eq!(snapshot.lease().unwrap().bytes, expected_bytes);
    assert_eq!(
        policy.layouts,
        vec![Layout::array::<RepeatInstance>(boundary.owner.instance.repeats.len()).unwrap()]
    );
    assert_eq!(policy.spent, boundary.owner.instance.repeats.len() + 5);
    assert_eq!(policy.merges, 0);
    assert_eq!(policy.ledger.live_bytes.get(), expected_bytes);

    drop(snapshot);
    assert_eq!(policy.ledger.live_bytes.get(), 0);
    assert_eq!(policy.spent, boundary.owner.instance.repeats.len() + 5);
}

#[test]
fn borrowed_boundary_snapshot_empty_repeats_need_no_array_reservation() {
    let plan = crate::stages::authored_eq::eq_admission_fixture::fixture();
    let owner = empty_repeat_owner(&plan);
    let boundary = boundary_for(&owner, 11);
    assert!(boundary.owner.instance.repeats.is_empty());
    let mut policy = Policy::<true>::new(0);

    let snapshot = AdmittedStop::from_boundary(&boundary, &mut policy).unwrap();
    assert_eq!(boundary_fields(&snapshot), boundary_fields(&boundary));
    assert!(snapshot.lease().is_none());
    assert_eq!(policy.reserve_calls, 0);
    assert_eq!(policy.spent, 5);
    assert_eq!(policy.ledger.live_bytes.get(), 0);
}

#[test]
fn borrowed_boundary_snapshot_one_byte_short_refuses_without_retaining_charge() {
    let plan = repeat_plan();
    let owner = repeat_owner(&plan);
    let boundary = boundary_for(&owner, 0);
    let expected_bytes = repeat_bytes(&owner);
    let mut policy = Policy::<true>::new(expected_bytes - 1);

    assert!(matches!(
        AdmittedStop::from_boundary(&boundary, &mut policy),
        Err(StageAudioError::Limit("caller admission reserve sentinel"))
    ));
    assert_eq!(policy.layouts[0].size(), expected_bytes);
    assert_eq!(policy.spend_calls, 3);
    assert_eq!(policy.spent, 3);
    assert_eq!(policy.ledger.live_bytes.get(), 0);
}

#[test]
fn borrowed_boundary_snapshot_each_reached_work_spend_cleans_up() {
    let plan = repeat_plan();
    let owner = repeat_owner(&plan);
    let boundary = boundary_for(&owner, 4);
    let reached_spends = boundary.owner.instance.repeats.len() + 5;

    for failure_at in 1..=reached_spends {
        let mut policy = Policy::<true>::new(usize::MAX);
        policy.fail_spend = Some(failure_at);
        assert!(matches!(
            AdmittedStop::from_boundary(&boundary, &mut policy),
            Err(StageAudioError::Limit("caller admission work sentinel"))
        ));
        assert_eq!(policy.spend_calls, failure_at);
        assert_eq!(policy.spent, failure_at - 1);
        assert_eq!(policy.ledger.live_bytes.get(), 0, "spend {failure_at}");
        assert_eq!(policy.merges, 0);
    }
}

#[test]
fn borrowed_boundary_snapshot_reserve_and_issuer_failures_leave_no_lease() {
    let plan = repeat_plan();
    let owner = repeat_owner(&plan);
    let boundary = boundary_for(&owner, 2);

    let mut reserve_failure = Policy::<true>::new(usize::MAX);
    reserve_failure.fail_reserve = true;
    assert!(matches!(
        AdmittedStop::from_boundary(&boundary, &mut reserve_failure),
        Err(StageAudioError::Limit("caller admission reserve sentinel"))
    ));
    assert_eq!(reserve_failure.spent, 3);
    assert_eq!(reserve_failure.ledger.live_bytes.get(), 0);

    let mut work_rotation = Policy::<true>::new(usize::MAX);
    work_rotation.rotate_after_spend = Some(1);
    assert!(matches!(
        AdmittedStop::from_boundary(&boundary, &mut work_rotation),
        Err(StageAudioError::Plan(
            PlanError::AudioMetadataIssuerMismatch
        ))
    ));
    assert_eq!(work_rotation.spent, 1);
    assert_eq!(work_rotation.ledger.live_bytes.get(), 0);

    let mut reserve_rotation = Policy::<true>::new(usize::MAX);
    reserve_rotation.rotate_after_reserve = true;
    assert!(matches!(
        AdmittedStop::from_boundary(&boundary, &mut reserve_rotation),
        Err(StageAudioError::Plan(
            PlanError::AudioMetadataIssuerMismatch
        ))
    ));
    assert_eq!(reserve_rotation.spent, 3);
    assert_eq!(reserve_rotation.ledger.live_bytes.get(), 0);
}
