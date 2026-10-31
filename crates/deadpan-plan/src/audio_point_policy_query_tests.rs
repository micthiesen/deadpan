//! Plan protocol tests; this backend is not the native account/Box implementation.
use super::*;
use crate::{AudioDefinitionSelector, AudioRootPlacement};
use deadpan_core::{NodeId, ProjectDocument};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug)]
enum Event {
    Spend {
        units: usize,
        accepted: bool,
    },
    Reserve {
        id: usize,
        bytes: usize,
        align: usize,
        accepted: bool,
    },
    Merge {
        target: usize,
        source: usize,
    },
    Prepare,
    Drop {
        id: usize,
        bytes: usize,
    },
}
#[derive(Clone, Copy, Debug)]
struct CommitObservation {
    pointer: usize,
    len: usize,
    capacity: usize,
    prepared_capacity: usize,
    source_id: Option<usize>,
    source_bytes: usize,
    live_at_prepare: usize,
    live_at_commit: usize,
    events_at_commit: usize,
}
#[derive(Default)]
struct Ledger {
    live: usize,
    accepted_work: usize,
    spends: usize,
    reservations: usize,
    next: usize,
    refuse_spend: Option<usize>,
    refuse_reserve: Option<usize>,
    refuse_prepare: bool,
    prepare_calls: usize,
    commit_calls: usize,
    commit: Option<CommitObservation>,
    events: Vec<Event>,
}
struct TestLease {
    ledger: Rc<RefCell<Ledger>>,
    id: usize,
    bytes: usize,
}
impl Drop for TestLease {
    fn drop(&mut self) {
        if self.bytes != 0 {
            let mut state = self.ledger.borrow_mut();
            state.live -= self.bytes;
            state.events.push(Event::Drop {
                id: self.id,
                bytes: self.bytes,
            });
        }
    }
}
struct Backend<const M: bool>(Rc<RefCell<Ledger>>);
impl<const M: bool> Backend<M> {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(Ledger::default())))
    }
    fn released(&self) {
        let state = self.0.borrow();
        assert_eq!(state.live, 0);
        let accepted: usize = state
            .events
            .iter()
            .map(|event| match event {
                Event::Spend {
                    units,
                    accepted: true,
                } => *units,
                _ => 0,
            })
            .sum();
        assert_eq!(
            state.accepted_work, accepted,
            "accepted work is never refunded"
        );
        let reserved: usize = state
            .events
            .iter()
            .map(|event| match event {
                Event::Reserve {
                    bytes,
                    accepted: true,
                    ..
                } => *bytes,
                _ => 0,
            })
            .sum();
        let mut dropped_ids = std::collections::BTreeSet::new();
        let dropped: usize = state
            .events
            .iter()
            .map(|event| match event {
                Event::Drop { id, bytes } => {
                    assert!(
                        dropped_ids.insert(*id),
                        "one final positive drop per lease identity"
                    );
                    *bytes
                }
                _ => 0,
            })
            .sum();
        assert_eq!(
            reserved, dropped,
            "all actually accepted lease bytes released"
        );
    }
}
impl<const M: bool> AudioSoundContextAdmission for Backend<M> {
    const METERED: bool = M;
    type Error = PlanError;
    type Issuer = usize;
    type Lease = TestLease;
    fn issuer(&self) -> usize {
        Rc::as_ptr(&self.0) as usize
    }
    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        assert!(M, "ordinary policy must not spend");
        let mut state = self.0.borrow_mut();
        state.spends += 1;
        let accepted = state.refuse_spend != Some(state.spends);
        state.events.push(Event::Spend { units, accepted });
        if !accepted {
            return Err(PlanError::AudioQueryLimit("Point policy test work refusal"));
        }
        state.accepted_work += units;
        Ok(())
    }
    fn reserve(&mut self, layout: std::alloc::Layout) -> Result<TestLease, PlanError> {
        assert!(M, "ordinary policy must not reserve");
        let mut state = self.0.borrow_mut();
        state.reservations += 1;
        state.next += 1;
        let id = state.next;
        let accepted = state.refuse_reserve != Some(state.reservations);
        state.events.push(Event::Reserve {
            id,
            bytes: layout.size(),
            align: layout.align(),
            accepted,
        });
        if !accepted {
            return Err(PlanError::AudioQueryLimit(
                "Point policy test reserve refusal",
            ));
        }
        state.live += layout.size();
        Ok(TestLease {
            ledger: Rc::clone(&self.0),
            id,
            bytes: layout.size(),
        })
    }
    fn merge(&mut self, target: &mut TestLease, source: &mut TestLease) {
        assert!(M, "ordinary policy must not merge");
        assert!(Rc::ptr_eq(&target.ledger, &self.0));
        assert!(Rc::ptr_eq(&source.ledger, &self.0));
        self.0.borrow_mut().events.push(Event::Merge {
            target: target.id,
            source: source.id,
        });
        target.bytes += source.bytes;
        source.bytes = 0;
    }
}
struct Prepared {
    capacity: usize,
    source_id: Option<usize>,
    source_bytes: usize,
    live: usize,
}
struct ProtocolMask {
    ranges: Vec<Range<SignalSample>>,
    lease: Option<TestLease>,
}
impl<const M: bool> AudioSignalPolicyMaskHandoff for Backend<M> {
    type PreparedMask = Prepared;
    type Mask = ProtocolMask;
    fn prepare_signal_mask(
        &mut self,
        source: SignalPolicyMaskRef<'_, usize, TestLease>,
    ) -> Result<Prepared, PlanError> {
        assert_eq!(source.issuer(), self.issuer());
        let bytes = std::alloc::Layout::array::<Range<SignalSample>>(source.capacity())
            .unwrap()
            .size();
        assert_eq!(source.lease().map(|lease| lease.bytes).unwrap_or(0), bytes);
        if let Some(lease) = source.lease() {
            assert!(Rc::ptr_eq(&lease.ledger, &self.0));
        }
        let mut state = self
            .0
            .try_borrow_mut()
            .map_err(|_| PlanError::InvalidPlan("test ledger borrowed"))?;
        state.prepare_calls += 1;
        if state.refuse_prepare {
            return Err(PlanError::AudioQueryLimit(
                "Point policy test prepare refusal",
            ));
        }
        state.events.push(Event::Prepare);
        Ok(Prepared {
            capacity: source.capacity(),
            source_id: source.lease().map(|lease| lease.id),
            source_bytes: bytes,
            live: state.live,
        })
    }
    fn commit_signal_mask(
        &mut self,
        prepared: Prepared,
        ranges: Vec<Range<SignalSample>>,
        source: Option<&mut TestLease>,
    ) -> ProtocolMask {
        // This protocol model transfers an existing lease within this recording
        // ledger. It does not model native account credit or Storage Box fees.
        let mut state = self.0.borrow_mut();
        state.commit_calls += 1;
        state.commit = Some(CommitObservation {
            pointer: ranges.as_ptr() as usize,
            len: ranges.len(),
            capacity: ranges.capacity(),
            prepared_capacity: prepared.capacity,
            source_id: prepared.source_id,
            source_bytes: prepared.source_bytes,
            live_at_prepare: prepared.live,
            live_at_commit: state.live,
            events_at_commit: state.events.len(),
        });
        drop(state);
        let lease = source.map(|source| {
            let destination = TestLease {
                ledger: Rc::clone(&source.ledger),
                id: source.id,
                bytes: source.bytes,
            };
            source.bytes = 0;
            destination
        });
        ProtocolMask { ranges, lease }
    }
}

fn visit(mut test: impl FnMut(&crate::AudioPointDomain<'_>, &Value)) {
    let frozen: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/point-content-query-public-supplement-v1/snapshot.json"
    ))
    .unwrap();
    let mut count = 0;
    for case in frozen["cases"].as_array().unwrap() {
        if case["setup"]["status"] != "PointConstructed" {
            continue;
        }
        let document = ProjectDocument::from_json(&case["document_before"].to_string()).unwrap();
        let document_before = serde_json::to_value(&document).unwrap();
        let plan = RenderPlan::compile(&document).unwrap();
        let definition = plan
            .audio_definition(AudioDefinitionSelector::Node {
                node: NodeId::new(case["selected_node"].as_str().unwrap()).unwrap(),
            })
            .unwrap();
        assert_eq!(
            serde_json::to_value(definition.selector()).unwrap(),
            case["point_request"]["selector"]
        );
        let placement: AudioRootPlacement =
            serde_json::from_value(case["point_request"]["placement"].clone()).unwrap();
        let origin: deadpan_core::ExactRatio =
            serde_json::from_value(case["point_request"]["grid_origin"].clone()).unwrap();
        let point = definition.in_point_clock(placement, origin).unwrap();
        assert_eq!(
            point.probe_baseline_snapshot(&plan),
            case["setup"]["input_before"]
        );
        test(&point, case);
        assert_eq!(
            point.probe_baseline_snapshot(&plan),
            case["setup"]["input_before"]
        );
        assert_eq!(serde_json::to_value(&document).unwrap(), document_before);
        count += 1;
    }
    assert_eq!(
        count, 3,
        "all three checked immutable public Point fixtures"
    );
    visit_positive_points(&mut test);
}
fn visit_positive_points(test: &mut impl FnMut(&crate::AudioPointDomain<'_>, &Value)) {
    let frozen: Value = serde_json::from_str(include_str!(
        "../../deadpan-audio/tests/fixtures/point-policy-final-mask-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let cases = frozen["internal_and_transfer_paths"]["cases"]
        .as_array()
        .unwrap();
    for name in [
        "private-point-silent-hold",
        "private-point-source-padding",
        "private-point-genuine-preserve-bound",
    ] {
        let case = cases
            .iter()
            .find(|case| case["name"] == name)
            .expect("immutable pre-C checked positive Point fixture");
        assert_eq!(case["setup"]["status"], "Ready", "{name}");
        assert_eq!(
            case["internal_request"]["reader"],
            "read_point_domain_controlled"
        );
        let document = ProjectDocument::from_json(&case["document_before"].to_string()).unwrap();
        let document_before = serde_json::to_value(&document).unwrap();
        assert_eq!(document_before, case["document_before"], "{name}");
        let plan = RenderPlan::compile(&document).unwrap();
        let expected_point = &case["setup"]["point"];
        let definition = plan
            .audio_definition(AudioDefinitionSelector::Node {
                node: NodeId::new(expected_point["definition"]["node"].as_str().unwrap()).unwrap(),
            })
            .unwrap();
        let placement: AudioRootPlacement =
            serde_json::from_value(expected_point["placement"].clone()).unwrap();
        let origin: deadpan_core::ExactRatio =
            serde_json::from_value(expected_point["reference_grid"]["frame_origin"].clone())
                .unwrap();
        let point = definition.in_point_clock(placement, origin).unwrap();
        let point_before = point.probe_baseline_snapshot(&plan);
        assert_eq!(
            serde_json::to_value(point.definition()).unwrap(),
            expected_point["definition"],
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(point.root()).unwrap(),
            expected_point["root"],
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(point.placement()).unwrap(),
            expected_point["placement"],
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(point.reference_grid()).unwrap(),
            expected_point["reference_grid"],
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(point.reference_sampling()).unwrap(),
            expected_point["reference_sampling"],
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(point.reference_samples()).unwrap(),
            expected_point["reference_samples"],
            "{name}"
        );
        let request = &case["internal_request"];
        let start = crate::ReferenceSample(request["start"].as_i64().unwrap());
        let end = crate::ReferenceSample(
            start
                .0
                .checked_add(request["frames"].as_i64().unwrap())
                .unwrap(),
        );
        let range =
            point.signal_at_reference(start).unwrap()..point.signal_at_reference(end).unwrap();
        let ordinary = point
            .signal_ref()
            .policy(range, Default::default())
            .unwrap();
        let actual = ordinary
            .suppressed
            .iter()
            .map(|range| [range.start.0, range.end.0])
            .collect::<Vec<_>>();
        let expected = &case["result_while_local_work_alive"]["suppressed"];
        assert_eq!(
            case["result_while_local_work_alive"]["status"], "Ok",
            "{name}"
        );
        assert!(
            !ordinary.suppressed.is_empty(),
            "actual positive policy required: {name}"
        );
        assert!(expected["len"].as_u64().unwrap() > 0, "{name}");
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            expected["values"],
            "{name}"
        );
        assert!(
            !point
                .signal_ref()
                .policy(full(&point), Default::default())
                .unwrap()
                .suppressed
                .is_empty(),
            "positive full factory required: {name}"
        );
        test(&point, case);
        assert_eq!(point.probe_baseline_snapshot(&plan), point_before, "{name}");
        assert_eq!(
            serde_json::to_value(&document).unwrap(),
            document_before,
            "{name}"
        );
    }
}
fn full(point: &crate::AudioPointDomain<'_>) -> Range<SignalSample> {
    let support = point.reference_samples();
    point.signal_at_reference(support.start).unwrap()
        ..point.signal_at_reference(support.end).unwrap()
}
fn policy_value(view: SignalPolicyView<'_>) -> Value {
    json!({"suppressed":view.suppressed,"contents":view.contents,"lookup":view.lookup})
}
fn ordinary_value(query: &AudioPolicyQuery<SignalSample>) -> Value {
    json!({"suppressed":query.suppressed,"contents":query.contents,"lookup":query.lookup})
}

#[test]
fn checked_frozen_point_geometry_and_actual_policy_values_match_both_modes() {
    let mut positive = 0;
    let mut empty = 0;
    visit(|point, _| {
        let range = full(point);
        for range in [range.clone(), range.start..range.start] {
            let expected = point
                .signal_ref()
                .policy(range.clone(), Default::default())
                .unwrap();
            let mut metered = Backend::<true>::new();
            let actual = point
                .policy_admitted(range.clone(), Default::default(), &mut metered)
                .unwrap();
            assert_eq!(policy_value(actual.view()), ordinary_value(&expected));
            assert_eq!(actual.canonical_units(), expected.work);
            assert_eq!(
                actual.work(),
                actual.canonical_units() + actual.additional_units()
            );
            assert_eq!(metered.0.borrow().accepted_work, actual.work());
            assert!(actual.was_forwarded());
            if actual.view().suppressed.is_empty() {
                empty += 1
            } else {
                positive += 1
            }
            drop(actual);
            metered.released();
            let mut unmetered = Backend::<false>::new();
            let actual = point
                .policy_admitted(range, Default::default(), &mut unmetered)
                .unwrap();
            assert_eq!(policy_value(actual.view()), ordinary_value(&expected));
            assert_eq!(actual.work(), expected.work);
            assert_eq!(actual.additional_units(), 0);
            assert!(!actual.was_forwarded());
            assert!(unmetered.0.borrow().events.is_empty());
            drop(actual);
            unmetered.released();
        }
    });
    assert!(positive > 0 && empty > 0);
}

#[test]
fn exact_final_allocation_and_lease_transfer_while_inventory_remains_live() {
    let mut positive = 0;
    let mut empty = 0;
    visit(|point, _| {
        let range = full(point);
        for range in [range.clone(), range.start..range.start] {
            let mut backend = Backend::<true>::new();
            let query = point
                .policy_admitted(range, Default::default(), &mut backend)
                .unwrap();
            let expected = policy_value(query.view());
            let pointer = query.view().suppressed.as_ptr() as usize;
            let len = query.view().suppressed.len();
            let result = query.into_suppression(&mut backend).unwrap();
            assert_eq!(
                serde_json::to_value(&result.ranges).unwrap(),
                expected["suppressed"]
            );
            let observation = backend.0.borrow().commit.unwrap();
            assert_eq!(observation.pointer, pointer);
            assert_eq!(observation.len, len);
            assert_eq!(observation.capacity, result.ranges.capacity());
            assert_eq!(observation.capacity, observation.prepared_capacity);
            assert_eq!(result.ranges.as_ptr() as usize, pointer);
            assert_eq!(observation.live_at_commit, observation.live_at_prepare);
            assert_eq!(backend.0.borrow().live, observation.source_bytes);
            assert!(matches!(
                backend.0.borrow().events[observation.events_at_commit - 1],
                Event::Prepare
            ));
            if let Some(id) = observation.source_id {
                positive += 1;
                assert_eq!(result.lease.as_ref().unwrap().id, id);
                assert!(
                    observation.live_at_prepare > observation.source_bytes,
                    "inventory still charged"
                );
                let state = backend.0.borrow();
                assert!(state.events.iter().any(|event| matches!(event,
                    Event::Reserve {id:issued,bytes,align,accepted:true}
                        if *issued==id&&*bytes==observation.source_bytes
                            &&*align==std::mem::align_of::<Range<SignalSample>>()
                )));
                assert!(
                    !state.events.iter().any(|event| matches!(event,
                        Event::Merge {target,source} if *target==id||*source==id
                    )),
                    "exclusive final lease must never be merged"
                );
            } else {
                empty += 1;
                assert_eq!(observation.capacity, 0);
                assert_eq!(observation.source_bytes, 0);
                assert!(result.lease.is_none());
            }
            drop(result);
            backend.released();
        }
    });
    assert!(positive > 0 && empty > 0);
}

#[test]
fn prepare_refusal_releases_complete_source_and_inventory_without_commit() {
    visit(|point, _| {
        let mut backend = Backend::<true>::new();
        let query = point
            .policy_admitted(full(point), Default::default(), &mut backend)
            .unwrap();
        assert!(backend.0.borrow().live > 0);
        let accepted = backend.0.borrow().accepted_work;
        backend.0.borrow_mut().refuse_prepare = true;
        assert!(matches!(
            query.into_suppression(&mut backend),
            Err(PlanError::AudioQueryLimit(
                "Point policy test prepare refusal"
            ))
        ));
        assert_eq!(backend.0.borrow().prepare_calls, 1);
        assert_eq!(backend.0.borrow().commit_calls, 0);
        assert_eq!(backend.0.borrow().accepted_work, accepted);
        backend.released();
    });
}

#[test]
fn issuer_and_unmetered_handoff_reject_before_prepare() {
    visit(|point, _| {
        let mut source = Backend::<true>::new();
        let query = point
            .policy_admitted(full(point), Default::default(), &mut source)
            .unwrap();
        let mut other = Backend::<true>::new();
        assert!(matches!(
            query.into_suppression(&mut other),
            Err(PlanError::AudioMetadataIssuerMismatch)
        ));
        assert_eq!(other.0.borrow().prepare_calls, 0);
        assert_eq!(other.0.borrow().commit_calls, 0);
        source.released();
        other.released();
        let mut ordinary = Backend::<false>::new();
        let query = point
            .policy_admitted(full(point), Default::default(), &mut ordinary)
            .unwrap();
        assert!(matches!(
            query.into_suppression(&mut ordinary),
            Err(PlanError::InvalidPlan(
                "unmetered Point policy mask handoff"
            ))
        ));
        assert_eq!(ordinary.0.borrow().prepare_calls, 0);
        assert_eq!(ordinary.0.borrow().commit_calls, 0);
        ordinary.released();
    });
}

#[test]
fn every_actual_work_and_reservation_refusal_prefix_releases_without_refunds() {
    visit(|point, _| {
        let mut measured = Backend::<true>::new();
        let query = point
            .policy_admitted(full(point), Default::default(), &mut measured)
            .unwrap();
        let spends = measured.0.borrow().spends;
        let reservations = measured.0.borrow().reservations;
        drop(query);
        measured.released();
        assert!(spends > 0 && reservations > 0);
        for at in 1..=spends {
            let mut backend = Backend::<true>::new();
            backend.0.borrow_mut().refuse_spend = Some(at);
            let result = point.policy_admitted(full(point), Default::default(), &mut backend);
            assert!(result.is_err(), "actual spend prefix {at}");
            drop(result);
            assert_eq!(backend.0.borrow().spends, at);
            backend.released();
        }
        for at in 1..=reservations {
            let mut backend = Backend::<true>::new();
            backend.0.borrow_mut().refuse_reserve = Some(at);
            let result = point.policy_admitted(full(point), Default::default(), &mut backend);
            assert!(result.is_err(), "actual reservation prefix {at}");
            drop(result);
            assert_eq!(backend.0.borrow().reservations, at);
            backend.released();
        }
    });
}

#[test]
fn same_sized_real_vector_guard_cannot_alias_the_final_mask_lease() {
    let mut covered = 0;
    visit(|point, _| {
        let mut backend = Backend::<true>::new();
        let query = point
            .policy_admitted(full(point), Default::default(), &mut backend)
            .unwrap();
        let capacity = query.mask.ranges.capacity();
        if capacity == 0 {
            drop(query);
            backend.released();
            return;
        }
        covered += 1;
        let layout = std::alloc::Layout::array::<Range<SignalSample>>(capacity).unwrap();
        let lease = backend.reserve(layout).unwrap();
        let unrelated_id = lease.id;
        let unrelated = ProtocolMask {
            ranges: Vec::with_capacity(capacity),
            lease: Some(lease),
        };
        assert_eq!(unrelated.ranges.capacity(), capacity);
        let result = query.into_suppression(&mut backend).unwrap();
        let final_id = backend.0.borrow().commit.unwrap().source_id.unwrap();
        assert_ne!(final_id, unrelated_id);
        assert_eq!(result.lease.as_ref().unwrap().bytes, layout.size());
        assert_eq!(unrelated.lease.as_ref().unwrap().bytes, layout.size());
        assert_eq!(backend.0.borrow().live, 2 * layout.size());
        assert!(
            !backend.0.borrow().events.iter().any(|event| matches!(event,
                Event::Merge {target,source} if *target==final_id||*source==final_id
            ))
        );
        drop(result);
        assert_eq!(backend.0.borrow().live, layout.size());
        drop(unrelated);
        backend.released();
    });
    assert!(covered > 0);
}

#[test]
fn borrowed_protocol_prepare_refusal_keeps_source_until_shared_borrow_ends() {
    visit(|point, _| {
        let mut backend = Backend::<true>::new();
        let query = point
            .policy_admitted(full(point), Default::default(), &mut backend)
            .unwrap();
        let ledger = Rc::clone(&backend.0);
        let shared = ledger.borrow();
        let source = SignalPolicyMaskRef {
            issuer: query.mask.memory.issuer,
            capacity: query.mask.ranges.capacity(),
            lease: query.mask.memory.lease.as_ref(),
        };
        assert!(matches!(
            backend.prepare_signal_mask(source),
            Err(PlanError::InvalidPlan("test ledger borrowed"))
        ));
        assert_eq!(shared.commit_calls, 0);
        assert_eq!(query.view().work, query.work());
        // Consuming into_suppression under this conflicting Ref is outside the
        // caller contract: dropping its failed query would also need RefMut.
        drop(shared);
        drop(query);
        backend.released();
    });
}
