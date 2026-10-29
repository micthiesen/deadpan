//! Independent comparisons with the immutable OLD START and point-cost runs.
use super::wide_baseline::Parent;
use super::*;
use crate::{AudioOwnerSpanStartRequest, AudioSoundContextAdmission};
use std::{alloc::Layout, cell::RefCell, rc::Rc};

const MEMORY_CAP: usize = 1_048_576;

#[derive(Default)]
struct Ledger {
    live: usize,
    peak: usize,
    reserved: usize,
    released: usize,
    forwarded: usize,
    work_calls: usize,
    reservations: usize,
}
struct Policy<const M: bool>(Rc<RefCell<Ledger>>);
struct Lease {
    ledger: Rc<RefCell<Ledger>>,
    bytes: usize,
}
impl Drop for Lease {
    fn drop(&mut self) {
        let mut ledger = self.ledger.borrow_mut();
        ledger.live -= self.bytes;
        ledger.released += self.bytes;
    }
}
impl<const M: bool> Policy<M> {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(Ledger::default())))
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
        let mut ledger = self.0.borrow_mut();
        let next = ledger.forwarded.checked_add(units).unwrap();
        if next > AudioQueryLimits::default().maximum_work {
            return Err(PlanError::AudioQueryLimit("START differential global work"));
        }
        ledger.forwarded = next;
        ledger.work_calls += 1;
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        let mut ledger = self.0.borrow_mut();
        let next = ledger.live.checked_add(layout.size()).unwrap();
        if next > MEMORY_CAP {
            return Err(PlanError::AudioQueryLimit(
                "START differential finite memory",
            ));
        }
        ledger.live = next;
        ledger.peak = ledger.peak.max(next);
        ledger.reserved += layout.size();
        ledger.reservations += 1;
        Ok(Lease {
            ledger: Rc::clone(&self.0),
            bytes: layout.size(),
        })
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        assert!(Rc::ptr_eq(&target.ledger, &self.0));
        assert!(Rc::ptr_eq(&source.ledger, &self.0));
        target.bytes += source.bytes;
        source.bytes = 0;
    }
}

fn baseline() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/span-start-input-baseline-v1/snapshot.json"
    ))
    .unwrap()
}
fn point_costs() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/span-start-input-baseline-v1/metered-point-costs.json"
    ))
    .unwrap()
}
fn find<'a>(values: &'a Value, name: &str) -> &'a Value {
    values
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .expect("immutable OLD case")
}
fn units(value: &Value) -> usize {
    usize::try_from(value.as_u64().unwrap()).unwrap()
}

fn assert_input_semantics(actual: &AudioOwnerInput<'_>, old: &Value) {
    let actual = input(actual);
    assert_eq!(
        actual.as_object().unwrap().len(),
        old.as_object().unwrap().len()
    );
    for (field, value) in old.as_object().unwrap() {
        if field != "total_work" {
            assert_eq!(&actual[field], value, "full Input field {field}");
        }
    }
}

fn assert_events_with_removed_point_budget(actual: &[Value], old: &[Value], point_work: usize) {
    assert_eq!(actual.len(), old.len());
    for (actual, old) in actual.iter().zip(old) {
        // The fresh capture budget has exactly Qpoint more available. No
        // discarded-span, selected-slot, Core or affine field is normalized.
        let mut expected = old["observation"].clone();
        if expected["kind"] == "actual_audio_walk_owner_input" {
            for field in ["budget_before", "budget_remaining"] {
                expected[field] = json!(units(&expected[field]) + point_work);
            }
        }
        assert_eq!(actual["observation"], expected, "full actual capture event");
    }
}

fn exercise<const M: bool>() {
    let frozen = baseline();
    let costs = point_costs();
    let wide: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let direct258: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/direct-owner-input-baseline-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(frozen["documents"], costs["documents"]);
    assert_eq!(frozen["documents"], wide["documents"]);
    let mut captures = 0;
    let mut active = 0;
    let mut inactive = 0;
    let mut ownerless = 0;
    let mut crosschecks = 0;
    wide_baseline::visit_cases(&wide, |parent, case| {
        let name = case["name"].as_str().unwrap();
        let old_parent = find(&frozen["parents"], name);
        let window = &case["request"]["window"];
        let range = AudioSample(window["start"].as_i64().unwrap())
            ..AudioSample(window["end"].as_i64().unwrap());
        let mut policy = Policy::<M>::new();
        let guarded = match parent {
            Parent::Root(plan) => plan.audio_owner_span_start_candidates_admitted(
                range,
                Default::default(),
                &mut policy,
            ),
            Parent::Domain(domain) => {
                domain.owner_span_start_candidates_admitted(range, Default::default(), &mut policy)
            }
        }
        .unwrap();
        let shared = guarded.shared_query_work();
        assert_eq!(shared.was_forwarded(), M);
        assert_eq!(
            shared.canonical_units(),
            units(&old_parent["setup_wide_gain_query"]["work"])
        );
        assert_eq!(
            shared.work(),
            shared.canonical_units() + shared.additional_units()
        );
        assert_eq!(
            policy.0.borrow().forwarded,
            if M { shared.work() } else { 0 }
        );
        let mut expected_wide = old_parent["setup_wide_gain_query"].clone();
        expected_wide["work"] = json!(shared.work());
        assert_eq!(
            query(guarded.query(), parent.plan(), |sample| sample.0),
            expected_wide
        );
        let query_live = policy.0.borrow().live;
        assert_eq!(query_live > 0, M);
        let mut new_capture_work = 0;
        for (span_index, span) in guarded.query().spans().iter().enumerate() {
            let old_span = &old_parent["span_observations"][span_index];
            let at = span.samples().start;
            let clocks: Vec<_> = span
                .owners()
                .iter()
                .map(|owner| clock(owner, parent.plan(), |sample| sample.0))
                .collect();
            assert_eq!(json!(clocks), old_span["ordered_private_clocks"]);
            assert_eq!(old_span["at"], json!(at.0));
            assert_eq!(
                old_span["samples"],
                sample_range(span.samples(), |sample| sample.0)
            );
            assert_eq!(
                old_span["original_is_silent"],
                json!(span.original_is_silent())
            );
            assert_eq!(
                old_span["support"],
                if span.support() == AudioOwnerSupport::Active {
                    "Active"
                } else {
                    "Inactive"
                }
            );
            assert_eq!(old_span["separate_point_gain_query"]["result"], "Ok");
            let point_span = &old_span["separate_point_gain_query"]["query"]["spans"][0];
            assert_eq!(
                point_span["owners"],
                json!(clocks),
                "exact START vector restriction"
            );
            assert_eq!(point_span["support"], old_span["support"]);
            assert_eq!(
                point_span["original_is_silent"],
                old_span["original_is_silent"]
            );
            // The frozen ownerless witness also has inactive support. These
            // coverage counts are independent properties of the same spans.
            ownerless += usize::from(span.owners().is_empty());
            if span.support() == AudioOwnerSupport::Inactive {
                inactive += 1;
                assert_eq!(old_span["capture_witness"], "non_capture");
                continue;
            }
            if span.owners().is_empty() {
                assert_eq!(old_span["capture_witness"], "non_capture");
                continue;
            }
            active += 1;
            let point_name = format!("{name}/span-{span_index}");
            let point = find(&costs["cases"], &point_name);
            // Observe the removed operation independently. Retained point
            // queries can emit Core receipts before the input capture begins;
            // they must match a complete exact prefix, never a kind filter.
            let mut point_policy = Policy::<M>::new();
            let (point_guard, point_events) =
                capture_baseline::observe_case(&point_name, "span_start_differential", || {
                    match parent {
                        Parent::Root(plan) => plan.audio_owner_input_candidates_at_admitted(
                            at,
                            Default::default(),
                            &mut point_policy,
                        ),
                        Parent::Domain(domain) => domain.owner_input_candidates_at_admitted(
                            at,
                            Default::default(),
                            &mut point_policy,
                        ),
                    }
                });
            let point_guard = point_guard.unwrap();
            assert_eq!(
                point_guard.canonical_units(),
                units(&point["output"]["Q"]["canonical"])
            );
            assert_eq!(
                point_guard.additional_units(),
                if M {
                    units(&point["output"]["Q"]["additional"])
                } else {
                    0
                }
            );
            assert_eq!(
                point_guard.work(),
                point_guard.canonical_units() + point_guard.additional_units()
            );
            assert_eq!(
                point_policy.0.borrow().forwarded,
                if M { point_guard.work() } else { 0 }
            );
            let mut expected_point = point["output"]["query"].clone();
            expected_point["work"] = json!(point_guard.work());
            assert_eq!(
                query(point_guard.query(), parent.plan(), |sample| sample.0),
                expected_point
            );
            drop(point_guard);
            assert_eq!(point_policy.0.borrow().live, 0);
            for (member_index, selected) in span.owners().iter().enumerate() {
                let capture_name = format!("{point_name}/member-{member_index}");
                let old = find(&frozen["cases"], &capture_name);
                let matches: Vec<_> = span
                    .owners()
                    .iter()
                    .enumerate()
                    .filter_map(|(index, candidate)| {
                        candidate.same_at(selected, at).then_some(index)
                    })
                    .collect();
                assert_eq!(
                    json!(matches),
                    old["private_same_at"]["wide_span_matching_member_indices"]
                );
                assert_eq!(
                    json!(matches),
                    old["private_same_at"]["point_first_span_matching_member_indices"]
                );
                assert_eq!(
                    clock(&span.owners()[matches[0]], parent.plan(), |sample| sample.0),
                    old["private_same_at"]["first_point_match"]
                );
                let before = policy.0.borrow().forwarded;
                let (captured, events) = capture_baseline::observe_case(
                    &capture_name,
                    "span_start_differential",
                    || {
                        let request = AudioOwnerSpanStartRequest {
                            span_index,
                            requested: selected,
                            at,
                        };
                        match parent {
                            Parent::Root(plan) => {
                                guarded.capture_root(plan, request, Default::default(), &mut policy)
                            }
                            Parent::Domain(domain) => guarded.capture_domain(
                                domain,
                                request,
                                Default::default(),
                                &mut policy,
                            ),
                        }
                    },
                );
                let captured = captured.unwrap();
                assert!(std::ptr::eq(captured.shared_query_work(), shared));
                assert_eq!(captured.was_forwarded(), M);
                assert_eq!(
                    captured.work(),
                    captured.canonical_units() + captured.additional_units()
                );
                assert_eq!(captured.input().work(), captured.work());
                assert_input_semantics(captured.input(), &old["output"]["input"]);
                assert_eq!(
                    captured.canonical_units()
                        + units(&old_span["separate_point_gain_query"]["query"]["work"]),
                    units(&old["output"]["canonical_total_work"])
                );
                assert_eq!(
                    policy.0.borrow().forwarded - before,
                    if M { captured.work() } else { 0 }
                );

                // The unchanged admitted direct API supplies an independent
                // same-mode C reference. Its actual Q receipt must equal the
                // separately frozen OLD metered point-query run.
                let mut direct_policy = Policy::<M>::new();
                let (direct, direct_events) = capture_baseline::observe_case(
                    &capture_name,
                    "span_start_differential",
                    || match parent {
                        Parent::Root(plan) => plan.audio_owner_input_admitted(
                            selected,
                            at,
                            Default::default(),
                            &mut direct_policy,
                        ),
                        Parent::Domain(domain) => domain.owner_input_admitted(
                            selected,
                            at,
                            Default::default(),
                            &mut direct_policy,
                        ),
                    },
                );
                let direct = direct.unwrap();
                assert_eq!(
                    direct.query_canonical_units(),
                    units(&point["output"]["Q"]["canonical"])
                );
                assert_eq!(
                    direct.query_additional_units(),
                    if M {
                        units(&point["output"]["Q"]["additional"])
                    } else {
                        0
                    }
                );
                assert_eq!(
                    direct.query_work(),
                    direct.query_canonical_units() + direct.query_additional_units()
                );
                assert_eq!(
                    direct.query_work(),
                    units(&point["output"]["Q"][if M { "total" } else { "canonical" }])
                );
                assert_eq!(
                    direct_policy.0.borrow().forwarded,
                    if M { direct.work() } else { 0 }
                );
                assert_eq!(direct.capture_canonical_units(), captured.canonical_units());
                assert_eq!(
                    direct.capture_additional_units(),
                    captured.additional_units()
                );
                assert_eq!(direct.capture_work(), captured.work());
                assert_eq!(direct.work(), direct.query_work() + captured.work());
                assert_input_semantics(direct.input().unwrap(), &old["output"]["input"]);
                assert!(direct_events.len() >= point_events.len());
                for (actual, independently_observed) in direct_events.iter().zip(&point_events) {
                    assert_eq!(actual["phase"], independently_observed["phase"]);
                    assert_eq!(
                        actual["observation"], independently_observed["observation"],
                        "complete removed Qpoint event prefix"
                    );
                }
                assert_events_with_removed_point_budget(
                    &events,
                    &direct_events[point_events.len()..],
                    direct.query_work(),
                );
                if !M {
                    assert_eq!(direct.input().map(input).unwrap(), old["output"]["input"]);
                    assert_events_with_removed_point_budget(
                        &events,
                        old["actual_capture_events"].as_array().unwrap(),
                        direct.query_work(),
                    );
                    assert_eq!(captured.additional_units(), 0);
                }
                if let Some(direct_name) = old["frozen_direct258_case"].as_str() {
                    let literal = find(&direct258["cases"], direct_name);
                    assert_eq!(literal["output"], old["output"]);
                    let observations = |case: &Value| {
                        case["actual_capture_events"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|event| event["observation"].clone())
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(observations(literal), observations(old));
                    crosschecks += 1;
                }
                new_capture_work += captured.work();
                let accepted = policy.0.borrow().forwarded;
                drop(captured);
                assert_eq!(
                    policy.0.borrow().live,
                    query_live,
                    "source input releases while wide query remains"
                );
                assert_eq!(
                    policy.0.borrow().forwarded,
                    accepted,
                    "source drop refunds no work"
                );
                drop(direct);
                assert_eq!(direct_policy.0.borrow().live, 0);
                captures += 1;
            }
        }
        let paid = if M {
            shared.work() + new_capture_work
        } else {
            0
        };
        assert_eq!(
            policy.0.borrow().forwarded,
            paid,
            "Qwide charged once for all captures"
        );
        drop(guarded);
        let ledger = policy.0.borrow();
        assert_eq!(ledger.live, 0);
        assert_eq!(ledger.reserved, ledger.released);
        assert!(ledger.peak <= MEMORY_CAP);
        assert_eq!(ledger.forwarded, paid, "query drop refunds no work");
        if !M {
            assert_eq!(ledger.work_calls, 0);
            assert_eq!(ledger.reservations, 0);
        }
    });
    assert_eq!(captures, 73);
    assert_eq!(active, 27);
    assert_eq!(inactive, 3);
    assert_eq!(ownerless, 1);
    assert_eq!(crosschecks, 51);
}

#[test]
fn span_start_false_matches_all_frozen_inputs_and_capture_events_without_qpoint() {
    exercise::<false>();
}

#[test]
fn span_start_true_preserves_full_capture_and_removes_frozen_metered_qpoint() {
    exercise::<true>();
}
