//! Direct owner-input admission retains the OLD private clock membership.
use super::*;
use crate::AudioSoundContextAdmission;
use crate::plan::audio_owners::tests::legacy_snapshot_v1::{
    capture_baseline,
    direct_input_baseline::{error_snapshot, input_snapshot, visit_cases},
    wide_baseline::Parent as QueryParent,
};
use serde_json::Value;
use std::{alloc::Layout, cell::RefCell, rc::Rc};

#[derive(Default)]
struct PolicyState {
    live: usize,
    peak: usize,
    cap: usize,
    reservations: usize,
    forwarded: usize,
    work_calls: usize,
    refuse_reservation: Option<usize>,
    refuse_work_call: Option<usize>,
    cancel_after: Option<usize>,
    rotate_reservation: Option<usize>,
    rotate_work_call: Option<usize>,
}
struct Policy<const M: bool>(Rc<RefCell<PolicyState>>);
struct Lease {
    state: Rc<RefCell<PolicyState>>,
    bytes: usize,
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.state.borrow_mut().live -= self.bytes;
    }
}
impl<const M: bool> Policy<M> {
    fn new(cap: usize) -> Self {
        Self(Rc::new(RefCell::new(PolicyState {
            cap,
            ..Default::default()
        })))
    }
    fn rotate(&mut self, cap: usize) {
        self.0 = Rc::new(RefCell::new(PolicyState {
            cap,
            ..Default::default()
        }));
    }
}
impl<const M: bool> AudioSoundContextAdmission for Policy<M> {
    const METERED: bool = M;
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        Rc::as_ptr(&self.0) as usize
    }
    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        let rotate = {
            let mut state = self.0.borrow_mut();
            state.work_calls += 1;
            if state.refuse_work_call == Some(state.work_calls) {
                return Err(PlanError::AudioQueryLimit("test direct work refusal"));
            }
            if state
                .cancel_after
                .is_some_and(|limit| state.forwarded >= limit)
            {
                return Err(PlanError::AudioQueryLimit("test direct cancellation"));
            }
            state.forwarded += units;
            (state.rotate_work_call == Some(state.work_calls)).then_some(state.cap)
        };
        if let Some(cap) = rotate {
            self.rotate(cap);
        }
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        let (lease, rotate) = {
            let mut state = self.0.borrow_mut();
            state.reservations += 1;
            if state.refuse_reservation == Some(state.reservations) {
                return Err(PlanError::AudioQueryLimit(
                    "test direct reservation refusal",
                ));
            }
            let next = state.live.checked_add(layout.size()).unwrap();
            if next > state.cap {
                return Err(PlanError::AudioQueryLimit("test direct allocation refusal"));
            }
            state.live = next;
            state.peak = state.peak.max(next);
            let lease = Lease {
                state: Rc::clone(&self.0),
                bytes: layout.size(),
            };
            let rotate =
                (state.rotate_reservation == Some(state.reservations)).then_some(state.cap);
            (lease, rotate)
        };
        if let Some(cap) = rotate {
            self.rotate(cap);
        }
        Ok(lease)
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        assert!(Rc::ptr_eq(&target.state, &self.0));
        assert!(Rc::ptr_eq(&source.state, &self.0));
        target.bytes += source.bytes;
        source.bytes = 0;
    }
}

fn frozen() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/direct-owner-input-baseline-v1/snapshot.json"
    ))
    .unwrap()
}
fn at(case: &Value) -> AudioSample {
    AudioSample(case["request"]["at"].as_i64().unwrap())
}
fn limits(case: &Value) -> AudioQueryLimits {
    let value = &case["request"]["limits"];
    AudioQueryLimits {
        maximum_spans: value["maximum_spans"].as_u64().unwrap() as usize,
        maximum_work: value["maximum_work"].as_u64().unwrap() as usize,
    }
}
fn admitted<'plan, const M: bool>(
    parent: QueryParent<'_, 'plan>,
    owner: &AudioOwnerClock<'_>,
    at: AudioSample,
    limits: AudioQueryLimits,
    admission: &mut Policy<M>,
) -> Result<AdmittedAudioOwnerInputCapture<'plan, usize, Lease>, PlanError> {
    match parent {
        QueryParent::Root(plan) => plan.audio_owner_input_admitted(owner, at, limits, admission),
        QueryParent::Domain(domain) => domain.owner_input_admitted(owner, at, limits, admission),
    }
}
fn assert_input_fields<const M: bool>(
    value: &AdmittedAudioOwnerInputCapture<'_, usize, Lease>,
    expected: &Value,
    policy: &Policy<M>,
) {
    let input = value
        .input()
        .expect("direct capture always selects an actual member");
    let actual = input_snapshot(input);
    let old = &expected["input"];
    assert_eq!(
        actual.as_object().unwrap().len(),
        old.as_object().unwrap().len()
    );
    for (key, old) in old.as_object().unwrap() {
        if key == "total_work" {
            assert_eq!(actual[key].as_u64().unwrap(), value.work() as u64);
        } else {
            assert_eq!(&actual[key], old, "direct input field {key}");
        }
    }
    assert_eq!(
        value.query_canonical_units() + value.capture_canonical_units(),
        expected["canonical_total_work"].as_u64().unwrap() as usize
    );
    assert_eq!(
        value.query_canonical_units() + value.query_additional_units(),
        value.query_work()
    );
    assert_eq!(
        value.capture_canonical_units() + value.capture_additional_units(),
        value.capture_work()
    );
    assert_eq!(input.work(), value.work());
    assert_eq!(value.was_forwarded(), M);
    assert_eq!(
        policy.0.borrow().forwarded,
        if M { value.work() } else { 0 }
    );
    if M {
        assert!(value.capture_additional_units() > 0);
        assert!(policy.0.borrow().live > 0);
    } else {
        assert_eq!(value.query_additional_units(), 0);
        assert_eq!(value.capture_additional_units(), 0);
        assert_eq!(actual, *old);
        assert_eq!(policy.0.borrow().work_calls, 0);
        assert_eq!(policy.0.borrow().reservations, 0);
    }
}

#[test]
fn direct_false_mode_matches_every_frozen_input_error_and_actual_capture_event() {
    let snapshot = frozen();
    let mut count = 0;
    visit_cases(&snapshot, |parent, owner, case| {
        let name = case["name"].as_str().unwrap();
        let mut policy = Policy::<false>::new(0);
        let (result, events) =
            capture_baseline::observe_case(name, "old_direct_public_input", || {
                admitted(parent, owner, at(case), limits(case), &mut policy)
            });
        match result {
            Ok(value) => {
                assert_eq!(case["output"]["result"], "Ok");
                assert_input_fields(&value, &case["output"], &policy);
                drop(value);
            }
            Err(error) => assert_eq!(error_snapshot(&error), case["output"]["error"], "{name}"),
        }
        assert_eq!(
            events.as_slice(),
            case["actual_capture_events"].as_array().unwrap().as_slice(),
            "actual OLD discarded span/seed: {name}"
        );
        assert_eq!(policy.0.borrow().live, 0);
        assert_eq!(policy.0.borrow().forwarded, 0);
        count += 1;
    });
    assert_eq!(count, snapshot["cases"].as_array().unwrap().len());
    assert!(count > 14);
}

#[test]
fn direct_true_mode_keeps_exact_private_membership_and_all_input_fields() {
    visit_cases(&frozen(), |parent, owner, case| {
        if matches!(case["case_kind"].as_str().unwrap(), "exact" | "one_short") {
            return;
        }
        let mut policy = Policy::<true>::new(usize::MAX);
        let (result, events) = capture_baseline::observe_case(
            case["name"].as_str().unwrap(),
            "old_direct_public_input",
            || admitted(parent, owner, at(case), limits(case), &mut policy),
        );
        match result {
            Ok(value) => {
                assert_eq!(case["output"]["result"], "Ok");
                assert_input_fields(&value, &case["output"], &policy);
                let expected = case["actual_capture_events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|event| event["observation"]["kind"] == "actual_audio_walk_owner_input")
                    .unwrap();
                let actual = events
                    .iter()
                    .find(|event| event["observation"]["kind"] == "actual_audio_walk_owner_input")
                    .unwrap();
                for field in ["at", "selected_observation", "capture_slot"] {
                    assert_eq!(actual["observation"][field], expected["observation"][field]);
                }
                assert_eq!(
                    value.capture_canonical_units(),
                    expected["observation"]["actual_capture_work_spent"]
                        .as_u64()
                        .unwrap() as usize
                );
                assert_walk_fields(
                    &actual["observation"]["walk"],
                    &expected["observation"]["walk"],
                    "",
                );
                if let Some(old_resolved) = expected["observation"]["walk"]
                    .pointer("/discarded_span/value/content/value/resolved/work")
                {
                    let receipt = events
                        .iter()
                        .find(|event| event["observation"]["kind"] == "actual_capture_core_receipt")
                        .unwrap();
                    assert_eq!(&receipt["observation"]["canonical"], old_resolved);
                    assert_eq!(receipt["observation"]["forwarded"], true);
                    assert_eq!(
                        receipt["observation"]["total"].as_u64().unwrap(),
                        receipt["observation"]["canonical"].as_u64().unwrap()
                            + receipt["observation"]["additional"].as_u64().unwrap()
                    );
                }
                drop(value);
            }
            Err(error) => {
                assert_eq!(
                    error_snapshot(&error),
                    case["output"]["error"],
                    "{}",
                    case["name"]
                );
                if matches!(
                    case["case_kind"].as_str().unwrap(),
                    "foreign_plan_same_document"
                        | "invalid_limits_precede_foreign_owner"
                        | "checked_sample_plus_one_overflow"
                ) {
                    assert_eq!(policy.0.borrow().work_calls, 0);
                    assert_eq!(policy.0.borrow().reservations, 0);
                }
            }
        }
        assert_eq!(policy.0.borrow().live, 0);
    });
}

fn assert_walk_fields(actual: &Value, expected: &Value, path: &str) {
    // These two nested receipts report newly forwarded Core ownership work.
    // Every other actual discarded-span field remains the frozen OLD value.
    if matches!(
        path,
        "/discarded_span/value/content/value/resolved/work"
            | "/discarded_span/value/content/value/resolved/lattice/work"
    ) {
        assert!(actual.as_u64().unwrap() >= expected.as_u64().unwrap());
        return;
    }
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            assert_eq!(actual.len(), expected.len(), "{path}");
            for (key, value) in expected {
                assert_walk_fields(&actual[key], value, &format!("{path}/{key}"));
            }
        }
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len(), "{path}");
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_walk_fields(actual, expected, &format!("{path}/{index}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

#[test]
fn direct_query_source_overlap_has_exact_peak_and_guard_survives_admission_scope() {
    visit_cases(&frozen(), |parent, owner, case| {
        if case["case_kind"] != "actual_checked_member" || case["output"]["result"] != "Ok" {
            return;
        }
        let mut measure = Policy::<true>::new(usize::MAX);
        let value = admitted(parent, owner, at(case), limits(case), &mut measure).unwrap();
        let peak = measure.0.borrow().peak;
        let work = value.work();
        assert!(peak > 0);
        assert!(measure.0.borrow().live > 0);
        drop(value);
        assert_eq!(measure.0.borrow().live, 0);
        let mut exact = Policy::<true>::new(peak);
        let value = admitted(parent, owner, at(case), limits(case), &mut exact).unwrap();
        assert_eq!(exact.0.borrow().peak, peak);
        let ledger = Rc::clone(&exact.0);
        drop(exact);
        assert!(ledger.borrow().live > 0);
        assert!(value.input().is_some());
        drop(value);
        assert_eq!(ledger.borrow().live, 0);
        assert_eq!(ledger.borrow().forwarded, work);
        let mut short = Policy::<true>::new(peak - 1);
        assert!(matches!(
            admitted(parent, owner, at(case), limits(case), &mut short),
            Err(PlanError::AudioQueryLimit("test direct allocation refusal"))
        ));
        assert_eq!(short.0.borrow().live, 0);
        assert!(short.0.borrow().forwarded > 0);
    });
}

#[test]
fn direct_every_reservation_and_work_refusal_cleans_up_without_refunding_work() {
    let mut checked = std::collections::BTreeSet::new();
    visit_cases(&frozen(), |parent, owner, case| {
        if case["case_kind"] != "actual_checked_member"
            || case["output"]["result"] != "Ok"
            || case["setup"]["selected_member_index"].as_u64().unwrap() == 0
        {
            return;
        }
        let parent_name = case["setup"]["parent_case"].as_str().unwrap();
        if !matches!(
            parent_name,
            "repeat_root_all_five_spans"
                | "hidden_actual_retained_reference_domain"
                | "fractional_point_bound_root_full"
        ) || !checked.insert(parent_name.to_owned())
        {
            return;
        }
        let mut measure = Policy::<true>::new(usize::MAX);
        let value = admitted(parent, owner, at(case), limits(case), &mut measure).unwrap();
        let reservations = measure.0.borrow().reservations;
        let calls = measure.0.borrow().work_calls;
        drop(value);
        for index in 1..=reservations {
            let mut refusing = Policy::<true>::new(usize::MAX);
            refusing.0.borrow_mut().refuse_reservation = Some(index);
            assert!(matches!(
                admitted(parent, owner, at(case), limits(case), &mut refusing),
                Err(PlanError::AudioQueryLimit(
                    "test direct reservation refusal"
                ))
            ));
            assert_eq!(refusing.0.borrow().reservations, index);
            assert_eq!(refusing.0.borrow().live, 0);
            assert!(refusing.0.borrow().forwarded > 0);
        }
        for index in 1..=calls {
            let mut refusing = Policy::<true>::new(usize::MAX);
            refusing.0.borrow_mut().refuse_work_call = Some(index);
            assert!(matches!(
                admitted(parent, owner, at(case), limits(case), &mut refusing),
                Err(PlanError::AudioQueryLimit("test direct work refusal"))
            ));
            assert_eq!(refusing.0.borrow().work_calls, index);
            assert_eq!(refusing.0.borrow().live, 0);
            assert_eq!(refusing.0.borrow().forwarded == 0, index == 1);
        }
    });
    assert!(!checked.is_empty());
}

#[test]
fn direct_limits_keep_zero_query_remainder_semantics_and_bound_total_new_work() {
    visit_cases(&frozen(), |parent, owner, case| {
        if case["case_kind"] != "actual_checked_member" || case["output"]["result"] != "Ok" {
            return;
        }
        let mut measure = Policy::<true>::new(usize::MAX);
        let value = admitted(parent, owner, at(case), limits(case), &mut measure).unwrap();
        let total = value.work();
        let query_total = value.query_work();
        let old_q = value.query_canonical_units();
        drop(value);
        let mut exact = Policy::<true>::new(usize::MAX);
        let value = admitted(
            parent,
            owner,
            at(case),
            AudioQueryLimits {
                maximum_work: total,
                ..Default::default()
            },
            &mut exact,
        )
        .unwrap();
        assert_eq!(value.work(), total);
        drop(value);
        for maximum_work in [total - 1, query_total] {
            let mut short = Policy::<true>::new(usize::MAX);
            assert!(matches!(
                admitted(
                    parent,
                    owner,
                    at(case),
                    AudioQueryLimits {
                        maximum_work,
                        ..Default::default()
                    },
                    &mut short
                ),
                Err(PlanError::AudioQueryLimit(_))
            ));
            assert_eq!(short.0.borrow().live, 0);
            assert!(short.0.borrow().forwarded > 0);
        }
        let mut false_policy = Policy::<false>::new(0);
        assert!(matches!(
            admitted(
                parent,
                owner,
                at(case),
                AudioQueryLimits {
                    maximum_work: old_q,
                    ..Default::default()
                },
                &mut false_policy
            ),
            Err(PlanError::AudioQueryLimit("structural work"))
        ));
        assert_eq!(false_policy.0.borrow().work_calls, 0);
    });
}

#[test]
fn direct_missing_match_pays_actual_comparison_work_and_cancel_or_issuer_refusal_is_clean() {
    let mut checked = 0;
    visit_cases(&frozen(), |parent, owner, case| {
        if case["case_kind"] != "absent_instance" {
            return;
        }
        let at = at(case);
        let point = at..AudioSample(at.0 + 1);
        let mut query_policy = Policy::<true>::new(usize::MAX);
        let query = match parent {
            QueryParent::Root(plan) => {
                plan.audio_gain_owners_admitted(point, Default::default(), &mut query_policy)
            }
            QueryParent::Domain(domain) => {
                domain.gain_owners_admitted(point, Default::default(), &mut query_policy)
            }
        }
        .unwrap();
        let query_work = query.work();
        let owners = query.query().spans()[0].owners().len();
        drop(query);
        let mut missing = Policy::<true>::new(usize::MAX);
        assert!(matches!(
            admitted(parent, owner, at, limits(case), &mut missing),
            Err(PlanError::InvalidPlan(
                "audio input owner is not current at sample"
            ))
        ));
        assert_eq!(missing.0.borrow().forwarded, query_work + 1 + owners);
        assert_eq!(missing.0.borrow().live, 0);
        let mut cancelled = Policy::<true>::new(usize::MAX);
        cancelled.0.borrow_mut().cancel_after = Some(query_work);
        assert!(matches!(
            admitted(parent, owner, at, limits(case), &mut cancelled),
            Err(PlanError::AudioQueryLimit("test direct cancellation"))
        ));
        assert_eq!(cancelled.0.borrow().forwarded, query_work);
        assert_eq!(cancelled.0.borrow().live, 0);
        for on_reservation in [false, true] {
            let mut rotating = Policy::<true>::new(usize::MAX);
            let old = Rc::clone(&rotating.0);
            if on_reservation {
                rotating.0.borrow_mut().rotate_reservation = Some(1);
            } else {
                rotating.0.borrow_mut().rotate_work_call = Some(1);
            }
            assert!(matches!(
                admitted(parent, owner, at, limits(case), &mut rotating),
                Err(PlanError::AudioMetadataIssuerMismatch)
            ));
            assert_eq!(old.borrow().live, 0);
            assert_eq!(rotating.0.borrow().live, 0);
        }
        checked += 1;
    });
    assert_eq!(checked, 14);
}
