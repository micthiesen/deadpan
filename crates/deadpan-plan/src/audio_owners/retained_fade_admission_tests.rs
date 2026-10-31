//! Replays the actual OLD borrowed handles while their construction guards live.
use super::*;
use std::cell::Cell;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Check {
    Off,
    False,
    True,
    Limits,
    Failures,
}
thread_local! {
    static CHECK: Cell<Check> = const { Cell::new(Check::Off) };
    static COUNTS: RefCell<(usize, usize)> = const { RefCell::new((0, 0)) };
}
struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        CHECK.set(Check::Off);
    }
}
fn run(check: Check) -> (usize, usize) {
    CHECK.with(|active| assert_eq!(active.replace(check) as u8, Check::Off as u8));
    let _reset = Reset;
    COUNTS.with(|counts| *counts.borrow_mut() = (0, 0));
    let old: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/retained-fade-query-baseline-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(matrix(), old);
    let supplement: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/retained-fade-query-supplement-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(super::supplement::supplement_matrix(), supplement);
    assert_eq!(
        super::overlap_tests::diagnostic::ordinary_case(),
        super::overlap_tests::diagnostic::frozen_case()
    );
    COUNTS.with(|counts| *counts.borrow())
}
fn range(row: &Value) -> Range<AudioSample> {
    let v = &row["request"]["samples"];
    AudioSample(v["start"].as_i64().unwrap())..AudioSample(v["end"].as_i64().unwrap())
}
fn limits(row: &Value) -> AudioQueryLimits {
    let v = &row["request"]["limits"];
    AudioQueryLimits {
        maximum_work: v["maximum_work"].as_u64().unwrap().try_into().unwrap(),
        maximum_spans: v["maximum_spans"].as_u64().unwrap().try_into().unwrap(),
    }
}
fn assert_true(
    value: &crate::AdmittedAudioFadeQuery<usize, Lease>,
    old: &Value,
    policy: &Policy<true>,
) {
    let actual = fade_query_baseline::query_snapshot(value.query());
    assert_eq!(
        actual.as_object().unwrap().len(),
        old.as_object().unwrap().len()
    );
    for (key, expected) in old.as_object().unwrap() {
        if key != "work" {
            assert_eq!(&actual[key], expected, "{key}");
        }
    }
    assert_eq!(
        value.canonical_units(),
        old["work"].as_u64().unwrap() as usize
    );
    assert_eq!(
        value.work(),
        value.canonical_units() + value.additional_units()
    );
    assert_eq!(actual["work"].as_u64().unwrap() as usize, value.work());
    assert_eq!(value.work(), policy.0.borrow().forwarded);
    assert!(value.was_forwarded());
}
fn old_success(rows: &[Value]) -> &Value {
    rows.iter()
        .find(|row| {
            row["name"] == "current_delivery/default"
                && row["output"]["result"] == "Ok"
                && !row["output"]["query"]["spans"]
                    .as_array()
                    .unwrap()
                    .is_empty()
        })
        .expect("actual nonempty retained delivery witness")
}
pub(super) fn observe(
    input: &AudioOwnerInput<'_>,
    retained: &AudioOwnerRetainedInput<'_>,
    rows: &[Value],
    caller: Option<&Rc<RefCell<PolicyState>>>,
) {
    let check = CHECK.get();
    if check == Check::Off {
        return;
    }
    COUNTS.with(|counts| counts.borrow_mut().0 += 1);
    let before = retained.capture_baseline_snapshot(input.context().plan);
    let caller_before = caller_snapshot(caller);
    match check {
        Check::Off => unreachable!(),
        Check::False => {
            for row in rows {
                let mut policy = Policy::<false>::new(0);
                match retained.fades_admitted(range(row), limits(row), &mut policy) {
                    Ok(value) => {
                        assert_eq!(row["output"]["result"], "Ok", "{}", row["name"]);
                        assert_eq!(
                            fade_query_baseline::query_snapshot(value.query()),
                            row["output"]["query"]
                        );
                        assert_eq!(
                            (value.work(), value.canonical_units()),
                            (value.query().work, value.query().work)
                        );
                        assert_eq!(value.additional_units(), 0);
                        assert!(!value.was_forwarded());
                        assert_eq!(caller_snapshot(caller), caller_before);
                        drop(value);
                    }
                    Err(error) => assert_eq!(
                        fade_query_baseline::result_snapshot(&Err(error)),
                        row["output"]
                    ),
                }
                let state = policy.0.borrow();
                assert_eq!(
                    (
                        state.live,
                        state.reservations,
                        state.work_calls,
                        state.forwarded
                    ),
                    (0, 0, 0, 0)
                );
                assert_eq!(caller_snapshot(caller), caller_before);
                COUNTS.with(|counts| counts.borrow_mut().1 += 1);
            }
        }
        Check::True => {
            for row in rows.iter().filter(|row| row["output"]["result"] == "Ok") {
                let mut policy = Policy::<true>::new(usize::MAX);
                let mut requested = limits(row);
                requested.maximum_work = AudioQueryLimits::default().maximum_work;
                let value = retained
                    .fades_admitted(range(row), requested, &mut policy)
                    .unwrap();
                assert_true(&value, &row["output"]["query"], &policy);
                if range(row).is_empty() {
                    assert_eq!((value.work(), value.additional_units()), (0, 0));
                    assert_eq!(
                        (policy.0.borrow().reservations, policy.0.borrow().live),
                        (0, 0)
                    );
                } else {
                    assert!(value.additional_units() > 0);
                    assert!(policy.0.borrow().live > 0);
                }
                assert_eq!(caller_snapshot(caller), caller_before);
                drop(value);
                assert_eq!(policy.0.borrow().live, 0);
                assert_eq!(caller_snapshot(caller), caller_before);
                COUNTS.with(|counts| counts.borrow_mut().1 += 1);
            }
        }
        Check::Limits | Check::Failures => {
            let row = old_success(rows);
            let requested = range(row);
            let mut policy = Policy::<true>::new(usize::MAX);
            let value = retained
                .fades_admitted(requested.clone(), Default::default(), &mut policy)
                .unwrap();
            assert_true(&value, &row["output"]["query"], &policy);
            let (work, peak, reservations, callbacks) = {
                let state = policy.0.borrow();
                (
                    value.work(),
                    state.peak,
                    state.reservations,
                    state.work_calls,
                )
            };
            assert!(work > 1 && peak > 0 && reservations > 0 && callbacks > 0);
            drop(value);
            assert_eq!(policy.0.borrow().live, 0);
            if check == Check::Limits {
                for (work_limit, cap, success) in [
                    (work, peak, true),
                    (work - 1, usize::MAX, false),
                    (AudioQueryLimits::default().maximum_work, peak - 1, false),
                ] {
                    let mut policy = Policy::<true>::new(cap);
                    let result = retained.fades_admitted(
                        requested.clone(),
                        AudioQueryLimits {
                            maximum_work: work_limit,
                            ..Default::default()
                        },
                        &mut policy,
                    );
                    assert_eq!(result.is_ok(), success);
                    if let Ok(value) = &result {
                        assert_true(value, &row["output"]["query"], &policy);
                        assert_eq!(policy.0.borrow().peak, peak);
                    }
                    drop(result);
                    assert_eq!(policy.0.borrow().live, 0);
                    assert_eq!(caller_snapshot(caller), caller_before);
                    COUNTS.with(|counts| counts.borrow_mut().1 += 1);
                }
            } else {
                let attempts = failure_audit::check(retained, row, caller);
                COUNTS.with(|counts| counts.borrow_mut().1 += attempts);
            }
        }
    }
    assert_eq!(caller_snapshot(caller), caller_before);
    assert_eq!(
        retained.capture_baseline_snapshot(input.context().plan),
        before
    );
}
#[test]
fn false_replays_all_frozen_retained_corpora_on_actual_borrowed_handles() {
    assert_eq!(run(Check::False), (54, 4_048));
}
#[test]
fn true_preserves_all_old_successes_and_literal_nested_work() {
    assert_eq!(run(Check::True), (54, 2_489));
}
#[test]
fn exact_work_and_peak_then_one_short_replay_every_actual_some_handle() {
    assert_eq!(run(Check::Limits), (54, 162));
}
#[test]
fn every_work_and_reservation_refusal_releases_the_query_and_preserves_the_caller() {
    let (handles, attempts) = run(Check::Failures);
    assert_eq!(handles, 54);
    assert!(attempts > 1_000);
}

fn caller_snapshot(caller: Option<&Rc<RefCell<PolicyState>>>) -> Option<[usize; 6]> {
    caller.map(|state| {
        let state = state.borrow();
        if state.forwarded > 0 {
            assert!(state.live > 0, "actual metered caller retains its leases");
        }
        [
            state.live,
            state.peak,
            state.cap,
            state.reservations,
            state.forwarded,
            state.work_calls,
        ]
    })
}
#[path = "retained_fade_failure_audit.rs"]
mod failure_audit;
