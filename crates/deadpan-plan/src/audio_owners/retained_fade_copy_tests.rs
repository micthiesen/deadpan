//! Independent copy Layouts and actual callback issuer rotations on borrowed inputs.
use super::*;
use crate::plan::audio::{EditorialConstraint, EnvelopeConstraint};
use crate::plan::audio_owner_input::retained_fades::trace::{self, Phase};
use std::cell::Cell;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Check {
    Off,
    Layouts,
    Rotations,
}
thread_local! {
    static CHECK:Cell<Check>=const {Cell::new(Check::Off)};
    static HANDLES:Cell<usize>=const {Cell::new(0)};
    static PHASE:RefCell<Option<PhaseState>>=const {RefCell::new(None)};
}
struct PhaseState {
    ledger: Rc<RefCell<PolicyState>>,
    processing: usize,
    editorial: usize,
    processing_before: Option<usize>,
    box_before: Option<usize>,
    editorial_before: Option<usize>,
    completed: [usize; 3],
}
fn phase(event: Phase) {
    PHASE.with(|slot| {
        let mut active = slot.borrow_mut();
        let state = active.as_mut().expect("actual copy phase observer");
        let live = state.ledger.borrow().live;
        match event {
            Phase::ProcessingCopyBefore => assert!(state.processing_before.replace(live).is_none()),
            Phase::ProcessingCopyAfter => {
                assert_eq!(
                    live - state.processing_before.take().unwrap(),
                    state.processing
                );
                state.completed[0] += 1;
            }
            Phase::BoundBoxBefore => assert!(state.box_before.replace(live).is_none()),
            Phase::BoundBoxAfter => {
                assert_eq!(
                    live - state.box_before.take().unwrap(),
                    Layout::new::<crate::AudioBound<'_>>().size()
                );
                state.completed[1] += 1;
            }
            Phase::EditorialCopyBefore => assert!(state.editorial_before.replace(live).is_none()),
            Phase::EditorialCopyAfter => {
                assert_eq!(
                    live - state.editorial_before.take().unwrap(),
                    state.editorial
                );
                state.completed[2] += 1;
            }
            Phase::OuterPushBefore { .. } | Phase::OuterPushAfter { .. } => {}
        }
    });
}
fn identity(value: &str) -> usize {
    Layout::new::<[usize; 2]>()
        .extend(Layout::array::<u8>(value.len()).unwrap())
        .unwrap()
        .0
        .pad_to_align()
        .size()
}
fn text(value: &Value) -> usize {
    identity(value.as_str().expect("actual identity edge"))
}
fn iteration(value: &Value) -> usize {
    if value.is_null() {
        0
    } else {
        text(&value["allocation"])
    }
}
fn repeats(value: &Value) -> usize {
    let values = value.as_array().unwrap();
    Layout::array::<deadpan_core::RepeatInstance>(values.len())
        .unwrap()
        .size()
        + values
            .iter()
            .map(|v| text(&v["node"]) + iteration(&v["iteration"]))
            .sum::<usize>()
}
fn instance(value: &Value) -> usize {
    text(&value["node"]) + repeats(&value["repeats"])
}
fn definition(value: &Value) -> usize {
    if value.is_null() {
        return 0;
    }
    text(
        value
            .get("node")
            .or_else(|| value.get("repeat"))
            .expect("actual selector identity"),
    )
}
fn constraints(value: &Value) -> usize {
    let values = value.as_array().unwrap();
    Layout::array::<EnvelopeConstraint>(values.len())
        .unwrap()
        .size()
        + values
            .iter()
            .map(|v| iteration(&v["gap_after"]))
            .sum::<usize>()
}
pub(super) fn expected(snapshot: &Value) -> (usize, usize) {
    let p = &snapshot["processing"];
    let b = &p["content"]["value"];
    let l = &b["resolved"]["lattice"];
    let clock = &l["clock"];
    let clock_identity = match clock["type"].as_str().unwrap() {
        "project_root_round_even" => 0,
        "preserve_input_point_ceil" => text(&clock["stage"]),
        "definition_point_ceil" => text(&clock["root"]),
        "gap_definition_point_ceil" => text(&clock["repeat"]),
        value => panic!("unreviewed actual clock {value}"),
    };
    let retimes = p["retimes"].as_array().unwrap();
    let processing = definition(&p["definition"])
        + instance(&p["instance"])
        + iteration(&p["gap_after"])
        + Layout::array::<crate::AudioRetimeStage>(retimes.len())
            .unwrap()
            .size()
        + retimes.iter().map(|v| text(&v["node"])).sum::<usize>()
        + Layout::new::<crate::AudioBound<'_>>().size()
        + definition(&b["definition"])
        + instance(&b["instance"])
        + iteration(&b["gap"]["after"])
        + clock_identity
        + instance(&l["instance"])
        + iteration(&l["gap_after"])
        + constraints(&b["constraints"]);
    let e = &snapshot["editorial"];
    let markers = e["editorial"].as_array().unwrap();
    let editorial = Layout::array::<EditorialConstraint>(markers.len())
        .unwrap()
        .size()
        + markers
            .iter()
            .map(|v| iteration(&v["boundary"]["gap_after"]))
            .sum::<usize>()
        + constraints(&e["constraints"])
        + repeats(&e["repeats"]);
    (processing, editorial)
}
fn request(rows: &[Value]) -> Range<AudioSample> {
    let row = rows
        .iter()
        .find(|v| {
            v["name"] == "current_delivery/default"
                && v["output"]["result"] == "Ok"
                && !v["output"]["query"]["spans"].as_array().unwrap().is_empty()
        })
        .expect("actual nonempty delivery");
    AudioSample(row["request"]["samples"]["start"].as_i64().unwrap())
        ..AudioSample(row["request"]["samples"]["end"].as_i64().unwrap())
}
struct MergeRotation {
    inner: Policy<true>,
    calls: usize,
    rotate: Option<usize>,
}
impl crate::AudioSoundContextAdmission for MergeRotation {
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.inner.issuer()
    }
    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        self.inner.spend(units)
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        self.inner.reserve(layout)
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        self.inner.merge(target, source);
        self.calls += 1;
        if self.rotate == Some(self.calls) {
            self.inner.rotate(usize::MAX)
        }
    }
}
pub(super) fn observe(
    input: &AudioOwnerInput<'_>,
    retained: &AudioOwnerRetainedInput<'_>,
    rows: &[Value],
) {
    let check = CHECK.get();
    if check == Check::Off {
        return;
    }
    HANDLES.set(HANDLES.get() + 1);
    let before = retained.capture_baseline_snapshot(input.context().plan);
    let samples = request(rows);
    if check == Check::Layouts {
        let (processing, editorial) = expected(&before);
        let mut policy = Policy::<true>::new(usize::MAX);
        PHASE.with(|slot| {
            assert!(
                slot.replace(Some(PhaseState {
                    ledger: Rc::clone(&policy.0),
                    processing,
                    editorial,
                    processing_before: None,
                    box_before: None,
                    editorial_before: None,
                    completed: [0; 3],
                }))
                .is_none()
            )
        });
        struct Clear;
        impl Drop for Clear {
            fn drop(&mut self) {
                PHASE.with(|slot| {
                    slot.replace(None);
                });
            }
        }
        let _clear = Clear;
        let query = trace::with_observer(phase, || {
            retained.fades_admitted(samples, Default::default(), &mut policy)
        })
        .unwrap();
        PHASE.with(|slot| {
            let state = slot.borrow();
            let state = state.as_ref().unwrap();
            assert!(state.completed[0] > 0);
            assert_eq!(state.completed, [state.completed[0]; 3]);
            assert!(
                state.processing_before.is_none()
                    && state.box_before.is_none()
                    && state.editorial_before.is_none()
            );
        });
        assert!(policy.0.borrow().live >= processing);
        drop(query);
        assert_eq!(policy.0.borrow().live, 0);
    } else {
        let mut measured = MergeRotation {
            inner: Policy::new(usize::MAX),
            calls: 0,
            rotate: None,
        };
        let query = retained
            .fades_admitted(samples.clone(), Default::default(), &mut measured)
            .unwrap();
        let (work_calls, reservations, work) = {
            let state = measured.inner.0.borrow();
            (state.work_calls, state.reservations, state.forwarded)
        };
        let merges = measured.calls;
        assert!(work_calls > 0 && reservations > 0 && merges > 0);
        drop(query);
        assert_eq!(measured.inner.0.borrow().live, 0);
        for (reservation, count) in [(false, work_calls), (true, reservations)] {
            for index in 1..=count {
                let mut policy = Policy::<true>::new(usize::MAX);
                let old = Rc::clone(&policy.0);
                if reservation {
                    old.borrow_mut().rotate_reservation = Some(index)
                } else {
                    old.borrow_mut().rotate_work_call = Some(index)
                }
                let result =
                    retained.fades_admitted(samples.clone(), Default::default(), &mut policy);
                assert!(
                    matches!(result, Err(PlanError::AudioMetadataIssuerMismatch)),
                    "rotate reserve={reservation} index={index}/{count}"
                );
                drop(result);
                assert_eq!(old.borrow().live, 0);
                assert_eq!(policy.0.borrow().live, 0);
                assert!(old.borrow().forwarded <= work);
            }
        }
        for index in 1..=merges {
            let mut policy = MergeRotation {
                inner: Policy::new(usize::MAX),
                calls: 0,
                rotate: Some(index),
            };
            let old = Rc::clone(&policy.inner.0);
            let result = retained.fades_admitted(samples.clone(), Default::default(), &mut policy);
            assert!(
                matches!(result, Err(PlanError::AudioMetadataIssuerMismatch)),
                "rotate actual merge {index}/{merges}"
            );
            drop(result);
            assert_eq!(policy.calls, index);
            assert_eq!(old.borrow().live, 0);
            assert_eq!(policy.inner.0.borrow().live, 0);
            assert!(old.borrow().forwarded <= work);
        }
        let p = &before["processing"];
        let e = &before["editorial"];
        let prepay = 1
            + retained.bound().work()
            + p["retimes"].as_array().unwrap().len()
            + p["instance"]["repeats"].as_array().unwrap().len()
            + e["editorial"].as_array().unwrap().len()
            + e["constraints"].as_array().unwrap().len()
            + e["repeats"].as_array().unwrap().len();
        for after in [0, prepay] {
            let mut policy = Policy::<true>::new(usize::MAX);
            policy.0.borrow_mut().cancel_after = Some(after);
            let result = retained.fades_admitted(samples.clone(), Default::default(), &mut policy);
            assert!(matches!(
                result,
                Err(PlanError::AudioQueryLimit("test wide cancellation"))
            ));
            drop(result);
            assert_eq!(policy.0.borrow().live, 0);
            assert_eq!(policy.0.borrow().forwarded, after);
        }
    }
    assert_eq!(
        retained.capture_baseline_snapshot(input.context().plan),
        before
    );
}
fn run(check: Check) -> usize {
    assert_eq!(CHECK.replace(check) as u8, Check::Off as u8);
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            CHECK.set(Check::Off);
        }
    }
    let _reset = Reset;
    HANDLES.set(0);
    let original: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/retained-fade-query-baseline-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(matrix(), original);
    let supplement: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/retained-fade-query-supplement-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(super::supplement::supplement_matrix(), supplement);
    assert_eq!(
        super::overlap_tests::diagnostic::ordinary_case(),
        super::overlap_tests::diagnostic::frozen_case()
    );
    HANDLES.get()
}
#[test]
fn independent_copied_payload_and_box_layouts_at_actual_allocation_phases() {
    assert_eq!(run(Check::Layouts), 54);
}
#[test]
fn every_spend_reserve_merge_rotation_and_prepaid_cancellation_preserve_caller() {
    assert_eq!(run(Check::Rotations), 54);
}
