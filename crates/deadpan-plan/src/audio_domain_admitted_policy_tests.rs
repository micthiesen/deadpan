use super::*;
use crate::AudioSoundContextAdmission;
use capture_baseline::assert_processing_with_paired_core_work;
use std::{alloc::Layout, cell::RefCell, rc::Rc};

#[derive(Default)]
struct State {
    live: usize,
    work: usize,
    reservations: usize,
}
struct Admission(Rc<RefCell<State>>);
struct Lease {
    state: Rc<RefCell<State>>,
    bytes: usize,
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.state.borrow_mut().live -= self.bytes;
    }
}
impl Admission {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(State::default())))
    }
}
impl AudioSoundContextAdmission for Admission {
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        Rc::as_ptr(&self.0) as usize
    }
    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        self.0.borrow_mut().work += units;
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        let mut state = self.0.borrow_mut();
        state.reservations += 1;
        state.live += layout.size();
        Ok(Lease {
            state: Rc::clone(&self.0),
            bytes: layout.size(),
        })
    }
    fn merge(&mut self, into: &mut Lease, from: &mut Lease) {
        assert!(Rc::ptr_eq(&into.state, &self.0));
        assert!(Rc::ptr_eq(&from.state, &self.0));
        into.bytes += from.bytes;
        from.bytes = 0;
    }
}
fn counters(admission: &Admission) -> (usize, usize, usize) {
    let state = admission.0.borrow();
    (state.live, state.work, state.reservations)
}

#[test]
fn admitted_policy_keeps_ordinary_exact_work_success_and_empty_refusal() {
    let mut checked = 0;
    let mut refusals = 0;
    visit(|domain, case| {
        let samples = window(&case["request"]);
        let mut admission = Admission::new();
        let first = domain
            .processing_for_policy_admitted(samples.clone(), Default::default(), &mut admission)
            .unwrap();
        let before = counters(&admission);
        let needed = expected_policy(case)["work"].as_u64().unwrap() as usize;
        let exact = AudioQueryLimits {
            maximum_work: needed,
            ..Default::default()
        };
        let ordinary = domain
            .processing_for_policy(samples.clone(), Default::default())
            .unwrap()
            .finish_policy(domain, samples.clone(), exact);
        let admitted = first.finish_policy(domain, samples, exact);
        match (ordinary, admitted) {
            (Ok(ordinary), Ok(admitted)) => {
                let (_, expected) = ordinary.into_queries();
                let (query, actual) = admitted.into_queries();
                assert_eq!(policy(&actual), policy(&expected));
                assert_eq!(policy(&actual), expected_policy(case));
                assert_eq!(counters(&admission), before);
                drop(query);
            }
            (Err(expected), Err(actual)) => {
                assert_eq!(error(&actual), error(&expected));
                refusals += 1;
            }
            _ => panic!("exact fresh policy behavior changed: {}", case["name"]),
        }
        assert_eq!(counters(&admission), (0, before.1, before.2));
        checked += 1;
    });
    assert_eq!(checked, 26);
    assert!(
        refusals > 0,
        "empty ordinary policy needs a positive query allowance after enter"
    );
}

#[test]
fn admitted_first_policy_witness_preserves_reuse_fallback_and_the_live_carrier() {
    let mut eligible = 0;
    let mut fallback = 0;
    visit(|domain, case| {
        let samples = window(&case["request"]);
        let mut admission = Admission::new();
        let (first, core_events) = capture_baseline::observe_case(
            case["name"].as_str().unwrap(),
            "admitted_processing_query",
            || {
                domain.processing_for_policy_admitted(
                    samples.clone(),
                    Default::default(),
                    &mut admission,
                )
            },
        );
        let first = first.unwrap();
        let query = first.query();
        let observed = processing(domain, query.query());
        let old = &case["first_processing_true"]["query"];
        assert_eq!(
            query.canonical_units(),
            old["work"].as_u64().unwrap() as usize
        );
        assert_eq!(observed["work"].as_u64().unwrap() as usize, query.work());
        assert_processing_with_paired_core_work(
            &observed,
            query.canonical_units(),
            old,
            &core_events,
            case["name"].as_str().unwrap(),
        );
        assert_eq!(
            query.work(),
            query.canonical_units() + query.additional_units()
        );
        assert!(query.was_forwarded());
        let before = counters(&admission);
        assert!(before.0 > 0);
        assert_eq!(before.1, query.work());
        let span_buffer = query.query().spans.as_ptr();
        let first_work = query.work();
        let reuse = first.stops.permits_binding_reuse();
        assert_eq!(
            reuse,
            case["first_tape_eligibility"]["content_only_eligible"] == true
        );
        eligible += usize::from(reuse);
        fallback += usize::from(!reuse);
        let expected = expected_policy(case);
        let needed = expected["work"].as_u64().unwrap() as usize;
        let limits = if removed_second(case).is_some() {
            AudioQueryLimits {
                maximum_work: needed,
                ..Default::default()
            }
        } else {
            AudioQueryLimits::default()
        };
        let (returned, actual) = first
            .finish_policy(domain, samples, limits)
            .unwrap()
            .into_queries();
        assert_eq!(policy(&actual), expected);
        assert_eq!(returned.query().spans.as_ptr(), span_buffer);
        assert_eq!(returned.work(), first_work);
        assert_eq!(
            counters(&admission),
            before,
            "finish must not rebill the first query"
        );
        drop(returned);
        assert_eq!(admission.0.borrow().live, 0);
        assert_eq!(admission.0.borrow().work, before.1);
    });
    assert_eq!(eligible, 22);
    assert_eq!(fallback, 4);
}

#[test]
fn admitted_policy_identity_and_range_refusal_precede_invalid_fresh_limits_and_release() {
    let mut checked = 0;
    visit(|domain, case| {
        let samples = window(&case["request"]);
        for changed_domain in [false, true] {
            let mut admission = Admission::new();
            let first = domain
                .processing_for_policy_admitted(samples.clone(), Default::default(), &mut admission)
                .unwrap();
            let before = counters(&admission);
            assert!(before.0 > 0);
            let cloned = domain.clone();
            let changed_range = if samples.is_empty() {
                AudioSample(samples.start.0.checked_sub(1).unwrap())..samples.end
            } else {
                samples.start..samples.start
            };
            let (caller, requested) = if changed_domain {
                (&cloned, samples.clone())
            } else {
                (domain, changed_range)
            };
            assert!(matches!(
                first.finish_policy(
                    caller,
                    requested,
                    AudioQueryLimits {
                        maximum_work: 0,
                        maximum_spans: 0
                    }
                ),
                Err(PlanError::InvalidPlan(
                    "processing policy identity mismatch"
                ))
            ));
            assert_eq!(counters(&admission), (0, before.1, before.2));
            checked += 1;
        }
    });
    assert_eq!(checked, 52);
}

#[test]
fn admitted_policy_uses_fresh_span_limits_after_a_wider_admitted_first_query() {
    let mut checked = 0;
    visit(|domain, case| {
        let samples = window(&case["request"]);
        let mut admission = Admission::new();
        let first = domain
            .processing_for_policy_admitted(samples.clone(), Default::default(), &mut admission)
            .unwrap();
        if first.query().query().spans.len() <= 1 {
            drop(first);
            assert_eq!(admission.0.borrow().live, 0);
            return;
        }
        let before = counters(&admission);
        assert!(before.0 > 0);
        let fresh = AudioQueryLimits {
            maximum_spans: 1,
            ..Default::default()
        };
        let ordinary = domain
            .processing_for_policy(samples.clone(), Default::default())
            .unwrap()
            .finish_policy(domain, samples.clone(), fresh);
        let admitted = first.finish_policy(domain, samples, fresh);
        match (ordinary, admitted) {
            (Ok(ordinary), Ok(admitted)) => {
                let (_, expected) = ordinary.into_queries();
                let (query, actual) = admitted.into_queries();
                assert_eq!(policy(&actual), policy(&expected));
                assert_eq!(counters(&admission), before);
                drop(query);
            }
            (Err(expected), Err(actual)) => assert_eq!(error(&actual), error(&expected)),
            _ => panic!("admitted fresh span behavior changed: {}", case["name"]),
        }
        assert_eq!(counters(&admission), (0, before.1, before.2));
        checked += 1;
    });
    assert_eq!(checked, 6);
}

#[test]
fn paired_work_comparison_rejects_unwitnessed_lattice_fee_and_canonical_drift() {
    let mut checked = 0;
    visit(|domain, case| {
        if case["name"] != "hidden_resumed_actual_capture_domain" {
            return;
        }
        let mut admission = Admission::new();
        let samples = window(&case["request"]);
        let (first, events) = capture_baseline::observe_case(
            case["name"].as_str().unwrap(),
            "admitted_processing_query",
            || domain.processing_for_policy_admitted(samples, Default::default(), &mut admission),
        );
        let first = first.unwrap();
        let query = first.query();
        let canonical = query.canonical_units();
        let observed = processing(domain, query.query());
        let old = &case["first_processing_true"]["query"];
        let name = case["name"].as_str().unwrap();
        assert_processing_with_paired_core_work(&observed, canonical, old, &events, name);
        let mut bad_observed = observed.clone();
        let span = bad_observed["spans"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|span| span["content"]["tag"] == "Bound")
            .unwrap();
        let lattice_work = &mut span["content"]["value"]["resolved"]["lattice"]["work"];
        *lattice_work = json!(lattice_work.as_u64().unwrap() + 1);
        let mut bad_events = events.clone();
        let recorded = bad_events
            .iter_mut()
            .find(|event| event["observation"]["kind"] == "actual_processing_paired_core_binding")
            .unwrap();
        let lattice_work = &mut recorded["observation"]["admitted"]["lattice"]["work"];
        *lattice_work = json!(lattice_work.as_u64().unwrap() + 1);
        assert!(
            std::panic::catch_unwind(|| {
                assert_processing_with_paired_core_work(
                    &bad_observed,
                    canonical,
                    old,
                    &bad_events,
                    name,
                );
            })
            .is_err(),
            "an invented lattice fee cannot hide behind known outer added work"
        );
        let mut canonical_drift = events.clone();
        let recorded = canonical_drift
            .iter_mut()
            .find(|event| event["observation"]["kind"] == "actual_processing_paired_core_binding")
            .unwrap();
        let ordinary_work = &mut recorded["observation"]["ordinary"]["resolved"]["work"];
        *ordinary_work = json!(ordinary_work.as_u64().unwrap() + 1);
        assert!(
            std::panic::catch_unwind(|| {
                assert_processing_with_paired_core_work(
                    &observed,
                    canonical,
                    old,
                    &canonical_drift,
                    name,
                );
            })
            .is_err(),
            "paired ordinary canonical work must stay literal against the frozen value"
        );
        drop(first);
        assert_eq!(admission.0.borrow().live, 0);
        checked += 1;
    });
    assert_eq!(checked, 1);
}
