//! New admission paths compare every field to the independently frozen OLD
//! wide queries. The work field is checked separately, never normalized.
use super::tests::legacy_snapshot_v1::wide_baseline::{Parent, query_snapshot, visit_cases};
use super::*;
use crate::AudioSoundContextAdmission;
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
                return Err(PlanError::AudioQueryLimit("test wide work refusal"));
            }
            if state
                .cancel_after
                .is_some_and(|limit| state.forwarded >= limit)
            {
                return Err(PlanError::AudioQueryLimit("test wide cancellation"));
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
                return Err(PlanError::AudioQueryLimit("test wide reservation refusal"));
            }
            let next = state.live.checked_add(layout.size()).unwrap();
            if next > state.cap {
                return Err(PlanError::AudioQueryLimit("test wide allocation refusal"));
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

fn snapshot() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json"
    ))
    .unwrap()
}
fn range(case: &Value) -> Range<AudioSample> {
    let value = &case["request"]["window"];
    AudioSample(value["start"].as_i64().unwrap())..AudioSample(value["end"].as_i64().unwrap())
}
fn admitted<'plan, const M: bool>(
    parent: Parent<'_, 'plan>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    policy: &mut Policy<M>,
) -> Result<AdmittedAudioOwnerQuery<'plan, usize, Lease>, PlanError> {
    match parent {
        Parent::Root(plan) => plan.audio_gain_owners_admitted(samples, limits, policy),
        Parent::Domain(domain) => domain.gain_owners_admitted(samples, limits, policy),
    }
}
fn normal<'plan>(
    parent: Parent<'_, 'plan>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
) -> Result<AudioOwnerQuery<'plan>, PlanError> {
    match parent {
        Parent::Root(plan) => plan.audio_gain_owners(samples, limits),
        Parent::Domain(domain) => domain.gain_owners(samples, limits),
    }
}
fn assert_frozen<const M: bool>(parent: Parent<'_, '_>, case: &Value, policy: &mut Policy<M>) {
    let value = admitted(parent, range(case), Default::default(), policy).unwrap();
    let old = &case["query"];
    let old_work = usize::try_from(old["work"].as_u64().unwrap()).unwrap();
    let observed = query_snapshot(value.query(), parent.plan());
    assert_eq!(
        observed.as_object().unwrap().len(),
        old.as_object().unwrap().len()
    );
    for (key, expected) in old.as_object().unwrap() {
        if key == "work" {
            assert_eq!(observed[key].as_u64().unwrap(), value.work() as u64);
        } else {
            assert_eq!(&observed[key], expected, "{} field {key}", case["name"]);
        }
    }
    assert_eq!(value.canonical_units(), old_work);
    assert_eq!(
        value.canonical_units() + value.additional_units(),
        value.work()
    );
    assert_eq!(value.was_forwarded(), M);
    assert_eq!(
        policy.0.borrow().forwarded,
        if M { value.work() } else { 0 }
    );
    if M {
        assert!(value.additional_units() >= policy.0.borrow().reservations);
        assert!(policy.0.borrow().live > 0);
    } else {
        assert_eq!(value.additional_units(), 0);
        assert_eq!(policy.0.borrow().reservations, 0);
        assert_eq!(policy.0.borrow().work_calls, 0);
        assert_eq!(observed, *old);
    }
    drop(value);
    assert_eq!(policy.0.borrow().live, 0);
}

#[test]
fn wide_gain_queries_preserve_all_frozen_fields_and_canonical_work_in_both_modes() {
    let frozen = snapshot();
    let mut count = 0;
    visit_cases(&frozen, |parent, case| {
        let old = normal(parent, range(case), Default::default()).unwrap();
        assert_eq!(query_snapshot(&old, parent.plan()), case["query"]);
        drop(old);
        assert_frozen(parent, case, &mut Policy::<false>::new(0));
        assert_frozen(parent, case, &mut Policy::<true>::new(usize::MAX));
        count += 1;
    });
    assert_eq!(count, 14);
}

#[test]
fn wide_gain_peak_exact_and_one_short_hold_and_release_the_actual_result() {
    visit_cases(&snapshot(), |parent, case| {
        let mut measure = Policy::<true>::new(usize::MAX);
        let value = admitted(parent, range(case), Default::default(), &mut measure).unwrap();
        let peak = measure.0.borrow().peak;
        let work = value.work();
        assert!(peak > 0);
        assert!(measure.0.borrow().live > 0);
        drop(value);
        assert_eq!(measure.0.borrow().live, 0);

        let mut exact = Policy::<true>::new(peak);
        let value = admitted(parent, range(case), Default::default(), &mut exact).unwrap();
        assert_eq!(exact.0.borrow().peak, peak);
        assert_eq!(value.work(), work);
        let accounting = Rc::clone(&exact.0);
        drop(exact);
        assert!(
            accounting.borrow().live > 0,
            "guard outlives temporary admission"
        );
        drop(value);
        assert_eq!(accounting.borrow().live, 0);
        assert_eq!(
            accounting.borrow().forwarded,
            work,
            "work is never refunded"
        );

        let mut short = Policy::<true>::new(peak - 1);
        assert!(matches!(
            admitted(parent, range(case), Default::default(), &mut short),
            Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
        ));
        assert_eq!(short.0.borrow().live, 0);
        assert!(short.0.borrow().forwarded > 0);
    });
}

#[test]
fn wide_gain_live_result_remains_charged_during_a_second_query() {
    visit_cases(&snapshot(), |parent, case| {
        if case["name"] != "repeat_root_all_five_spans" {
            return;
        }
        let mut measure = Policy::<true>::new(usize::MAX);
        let first = admitted(parent, range(case), Default::default(), &mut measure).unwrap();
        let retained = measure.0.borrow().live;
        let solo_peak = measure.0.borrow().peak;
        assert!(retained > 0);
        drop(first);

        let mut exact = Policy::<true>::new(retained + solo_peak);
        let first = admitted(parent, range(case), Default::default(), &mut exact).unwrap();
        let second = admitted(parent, range(case), Default::default(), &mut exact).unwrap();
        assert_eq!(exact.0.borrow().peak, retained + solo_peak);
        drop(second);
        assert_eq!(exact.0.borrow().live, retained);
        drop(first);
        assert_eq!(exact.0.borrow().live, 0);

        let mut short = Policy::<true>::new(retained + solo_peak - 1);
        let first = admitted(parent, range(case), Default::default(), &mut short).unwrap();
        assert!(matches!(
            admitted(parent, range(case), Default::default(), &mut short),
            Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
        ));
        assert_eq!(short.0.borrow().live, retained);
        assert_eq!(
            query_snapshot(first.query(), parent.plan())["spans"],
            case["query"]["spans"]
        );
        drop(first);
        assert_eq!(short.0.borrow().live, 0);
    });
}

#[test]
fn wide_gain_refuses_every_actual_reservation_and_work_callback_without_leaks_or_refunds() {
    visit_cases(&snapshot(), |parent, case| {
        let mut baseline = Policy::<true>::new(usize::MAX);
        let value = admitted(parent, range(case), Default::default(), &mut baseline).unwrap();
        let reservations = baseline.0.borrow().reservations;
        let work_calls = baseline.0.borrow().work_calls;
        drop(value);
        assert_eq!(baseline.0.borrow().live, 0);
        for index in 1..=reservations {
            let mut refusing = Policy::<true>::new(usize::MAX);
            refusing.0.borrow_mut().refuse_reservation = Some(index);
            assert!(
                matches!(
                    admitted(parent, range(case), Default::default(), &mut refusing),
                    Err(PlanError::AudioQueryLimit("test wide reservation refusal"))
                ),
                "{} reservation {index}",
                case["name"]
            );
            assert_eq!(refusing.0.borrow().reservations, index);
            assert_eq!(refusing.0.borrow().live, 0);
            assert!(refusing.0.borrow().forwarded > 0);
        }
        for index in 1..=work_calls {
            let mut refusing = Policy::<true>::new(usize::MAX);
            refusing.0.borrow_mut().refuse_work_call = Some(index);
            assert!(
                matches!(
                    admitted(parent, range(case), Default::default(), &mut refusing),
                    Err(PlanError::AudioQueryLimit("test wide work refusal"))
                ),
                "{} work call {index}",
                case["name"]
            );
            assert_eq!(refusing.0.borrow().work_calls, index);
            assert_eq!(refusing.0.borrow().live, 0);
            assert_eq!(refusing.0.borrow().forwarded == 0, index == 1);
        }
    });
}

#[test]
fn wide_gain_uses_exact_supplied_local_work_and_span_limits_in_both_modes() {
    visit_cases(&snapshot(), |parent, case| {
        let mut measure = Policy::<true>::new(usize::MAX);
        let value = admitted(parent, range(case), Default::default(), &mut measure).unwrap();
        let total = value.work();
        let canonical = value.canonical_units();
        let spans = value.query().spans().len();
        drop(value);
        for (metered, work) in [(false, canonical), (true, total)] {
            let exact = AudioQueryLimits {
                maximum_spans: spans,
                maximum_work: work,
            };
            let short = AudioQueryLimits {
                maximum_work: work - 1,
                ..exact
            };
            if metered {
                let mut policy = Policy::<true>::new(usize::MAX);
                let value = admitted(parent, range(case), exact, &mut policy).unwrap();
                assert_eq!(value.work(), work);
                drop(value);
                assert!(matches!(
                    admitted(parent, range(case), short, &mut policy),
                    Err(PlanError::AudioQueryLimit(_))
                ));
                assert_eq!(policy.0.borrow().live, 0);
            } else {
                let mut policy = Policy::<false>::new(0);
                let value = admitted(parent, range(case), exact, &mut policy).unwrap();
                assert_eq!(query_snapshot(value.query(), parent.plan()), case["query"]);
                drop(value);
                assert!(matches!(
                    admitted(parent, range(case), short, &mut policy),
                    Err(PlanError::AudioQueryLimit(_))
                ));
                assert_eq!(policy.0.borrow().work_calls, 0);
                assert_eq!(policy.0.borrow().reservations, 0);
            }
        }
        if spans > 1 {
            let short = AudioQueryLimits {
                maximum_spans: spans - 1,
                ..Default::default()
            };
            let mut policy = Policy::<true>::new(usize::MAX);
            assert!(matches!(
                admitted(parent, range(case), short, &mut policy),
                Err(PlanError::AudioQueryLimit("owner span count"))
            ));
            assert_eq!(policy.0.borrow().live, 0);
            let mut policy = Policy::<false>::new(0);
            assert!(matches!(
                admitted(parent, range(case), short, &mut policy),
                Err(PlanError::AudioQueryLimit("owner span count"))
            ));
            assert_eq!(policy.0.borrow().work_calls, 0);
        }
    });
}

#[test]
fn wide_gain_cancellation_and_wrong_issuer_release_in_progress_allocations() {
    visit_cases(&snapshot(), |parent, case| {
        let mut cancelled = Policy::<true>::new(usize::MAX);
        cancelled.0.borrow_mut().cancel_after = Some(1);
        assert!(matches!(
            admitted(parent, range(case), Default::default(), &mut cancelled),
            Err(PlanError::AudioQueryLimit("test wide cancellation"))
        ));
        assert!(cancelled.0.borrow().forwarded > 0);
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
                admitted(parent, range(case), Default::default(), &mut rotating),
                Err(PlanError::AudioMetadataIssuerMismatch)
            ));
            assert_eq!(old.borrow().live, 0);
            assert_eq!(rotating.0.borrow().live, 0);
            assert!(old.borrow().forwarded > 0);
        }
    });
}

#[test]
fn wide_gain_range_limits_and_seed_admission_preserve_legacy_error_order() {
    visit_cases(&snapshot(), |parent, case| {
        let bounds = match parent {
            Parent::Root(plan) => AudioSample(0)..plan.audio_duration().unwrap(),
            Parent::Domain(domain) => domain.samples.clone(),
        };
        let invalid = AudioQueryLimits {
            maximum_spans: 0,
            ..Default::default()
        };
        let before = AudioSample(bounds.start.0 - 1)..bounds.start;
        for metered in [false, true] {
            if metered {
                let mut policy = Policy::<true>::new(usize::MAX);
                assert!(matches!(
                    admitted(parent, before.clone(), invalid, &mut policy),
                    Err(PlanError::AudioRangeOutOfRange)
                ));
                assert_eq!(policy.0.borrow().work_calls, 0);
                assert_eq!(policy.0.borrow().reservations, 0);
            } else {
                let mut policy = Policy::<false>::new(0);
                assert!(matches!(
                    admitted(parent, before.clone(), invalid, &mut policy),
                    Err(PlanError::AudioRangeOutOfRange)
                ));
                assert_eq!(policy.0.borrow().work_calls, 0);
            }
        }
        let reversed = bounds.end..bounds.start;
        let mut policy = Policy::<false>::new(0);
        assert!(matches!(
            admitted(parent, reversed.clone(), invalid, &mut policy),
            Err(PlanError::InvalidAudioLimits)
        ));
        assert!(matches!(
            admitted(parent, reversed, Default::default(), &mut policy),
            Err(PlanError::AudioRangeOutOfRange)
        ));
        assert_eq!(policy.0.borrow().work_calls, 0);
        assert_eq!(policy.0.borrow().reservations, 0);

        if case["name"] == "repeat_actual_default_gap_domain_full" {
            let Parent::Domain(domain) = parent else {
                panic!("actual Domain case")
            };
            let mut ample = Policy::<true>::new(usize::MAX);
            assert!(matches!(
                admitted(parent, range(case), invalid, &mut ample),
                Err(PlanError::InvalidAudioLimits)
            ));
            assert!(
                ample.0.borrow().reservations > 0,
                "seed copied before inherited validation"
            );
            assert_eq!(ample.0.borrow().live, 0);
            let mut short = Policy::<true>::new(0);
            assert!(matches!(
                admitted(parent, range(case), invalid, &mut short),
                Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
            ));
            assert_eq!(short.0.borrow().live, 0);

            let mut local = Budget {
                remaining: AudioQueryLimits::default().maximum_work,
                lookup: LookupStats::default(),
            };
            let mut measure = Policy::<true>::new(usize::MAX);
            let mut budget = admission::Budget::new(&mut local, &mut measure);
            let seed = query_admission::State::from_seed(&domain.seed, &mut budget).unwrap();
            let seed_work = budget.additional_used;
            assert_eq!(budget.canonical_used, 0);
            assert!(seed_work > 0);
            drop(seed);
            assert_eq!(measure.0.borrow().live, 0);
            // A zero remainder after seed ownership is valid for an empty
            // query; revalidating a reduced maximum_work would reject it.
            let mut exact = Policy::<true>::new(usize::MAX);
            let value = admitted(
                parent,
                domain.samples.start..domain.samples.start,
                AudioQueryLimits {
                    maximum_work: seed_work,
                    ..Default::default()
                },
                &mut exact,
            )
            .unwrap();
            assert!(value.query().spans().is_empty());
            assert_eq!(value.work(), seed_work);
            assert_eq!(exact.0.borrow().forwarded, seed_work);
            drop(value);
            assert_eq!(exact.0.borrow().live, 0);
        }
    });
}

#[test]
fn wide_gain_root_duration_error_precedes_inherited_limits_and_reversed_range() {
    let frozen = snapshot();
    let document =
        deadpan_core::ProjectDocument::from_json(&frozen["documents"][0]["document"].to_string())
            .unwrap();
    let mut plan = RenderPlan::compile(&document).unwrap();
    // Synthetic private metadata only for the pre-query overflow branch. The
    // immutable fixture and every measured case above remain unchanged.
    plan.metadata.duration = deadpan_core::FrameDuration::new(i64::MAX).unwrap();
    plan.metadata.presentation_basis.frame_rate = deadpan_core::FrameRate::new(1, 1).unwrap();
    let invalid = AudioQueryLimits {
        maximum_spans: 0,
        ..Default::default()
    };
    for metered in [false, true] {
        if metered {
            let mut policy = Policy::<true>::new(usize::MAX);
            assert!(matches!(
                plan.audio_gain_owners_admitted(
                    AudioSample(1)..AudioSample(0),
                    invalid,
                    &mut policy
                ),
                Err(PlanError::Time(TimeError::Overflow))
            ));
            assert_eq!(policy.0.borrow().work_calls, 0);
            assert_eq!(policy.0.borrow().reservations, 0);
        } else {
            let mut policy = Policy::<false>::new(0);
            assert!(matches!(
                plan.audio_gain_owners_admitted(
                    AudioSample(1)..AudioSample(0),
                    invalid,
                    &mut policy
                ),
                Err(PlanError::Time(TimeError::Overflow))
            ));
            assert_eq!(policy.0.borrow().work_calls, 0);
            assert_eq!(policy.0.borrow().reservations, 0);
        }
    }
    assert!(matches!(
        plan.audio_gain_owners(AudioSample(1)..AudioSample(0), invalid),
        Err(PlanError::Time(TimeError::Overflow))
    ));
    let mut policy = Policy::<true>::new(usize::MAX);
    assert!(matches!(
        plan.audio_gain_owners_admitted(AudioSample(-1)..AudioSample(0), invalid, &mut policy),
        Err(PlanError::AudioRangeOutOfRange)
    ));
    assert_eq!(policy.0.borrow().work_calls, 0);
}

#[path = "flattened_query_admission_tests.rs"]
mod flattened;

#[path = "processing_query_admission_tests.rs"]
mod processing_admission;

#[path = "policy_child_admission_tests.rs"]
mod policy_children;

#[path = "policy_query_admission_tests.rs"]
mod policy_admission;

#[path = "policy_point_clone_tests.rs"]
mod policy_point;
