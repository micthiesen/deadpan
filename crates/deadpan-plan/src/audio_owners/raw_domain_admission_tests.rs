//! Public raw-query acceptance over the independently frozen 77 real operands.
use super::*;

#[path = "raw_domain_point_borrow_tests.rs"]
mod point_borrow_tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OwnState {
    live: usize,
    peak: usize,
    reservations: usize,
    accepted: usize,
    work_callbacks: usize,
}
impl OwnState {
    fn read(value: &Rc<RefCell<PolicyState>>) -> Self {
        let value = value.borrow();
        Self {
            live: value.live,
            peak: value.peak,
            reservations: value.reservations,
            accepted: value.forwarded,
            work_callbacks: value.work_calls,
        }
    }
    fn frozen(value: &Value) -> Self {
        Self {
            live: count(&value["live"]),
            peak: count(&value["peak"]),
            reservations: count(&value["reservations"]),
            accepted: count(&value["accepted_work"]),
            work_callbacks: count(&value["work_callbacks"]),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    Spend(usize),
    Reserve {
        size: usize,
        align: usize,
    },
    Merge {
        target_before: usize,
        source_before: usize,
        target_after: usize,
        source_after: usize,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Event {
    operation: Operation,
    accepted: bool,
    before: OwnState,
    after: OwnState,
}
impl Event {
    fn frozen(value: &Value) -> Self {
        let operation = match value["event"].as_str().unwrap() {
            "spend" => Operation::Spend(count(&value["units"])),
            "reserve" => {
                assert_eq!(value["lease"]["same_issuer"], true);
                assert_eq!(value["lease"]["bytes"], value["layout"]["size"]);
                Operation::Reserve {
                    size: count(&value["layout"]["size"]),
                    align: count(&value["layout"]["align"]),
                }
            }
            "merge" => {
                assert_eq!(value["target_same_issuer"], true);
                assert_eq!(value["source_same_issuer"], true);
                Operation::Merge {
                    target_before: count(&value["target_bytes_before"]),
                    source_before: count(&value["source_bytes_before"]),
                    target_after: count(&value["target_bytes_after"]),
                    source_after: count(&value["source_bytes_after"]),
                }
            }
            other => panic!("unrecognized frozen callback {other}"),
        };
        Self {
            operation,
            accepted: value["accepted"].as_bool().unwrap_or(true),
            before: OwnState::frozen(&value["before"]),
            after: OwnState::frozen(&value["after"]),
        }
    }
}
fn count(value: &Value) -> usize {
    usize::try_from(value.as_u64().unwrap()).unwrap()
}

#[derive(Debug, Clone, Copy)]
enum Fault {
    None,
    WorkLimit(usize),
    ResidentLimit(usize),
    RefuseSpend(usize),
    RefuseReserve(usize),
    CancelSpend(usize),
    RotateSpend(usize),
    RotateReserve(usize),
    RotateMerge(usize),
}
struct Recording<const M: bool> {
    policy: Policy<M>,
    caller: Option<(Rc<RefCell<PolicyState>>, Value)>,
    issuers: Vec<Rc<RefCell<PolicyState>>>,
    events: Vec<Event>,
    fault: Fault,
    spend_ordinal: usize,
    merge_ordinal: usize,
    issuer_reads: Cell<usize>,
}
impl<const M: bool> Recording<M> {
    fn new(caller: Option<&Rc<RefCell<PolicyState>>>, fault: Fault) -> Self {
        let policy = Policy::<M>::new(match fault {
            Fault::ResidentLimit(cap) => cap,
            _ => usize::MAX,
        });
        match fault {
            Fault::RefuseSpend(at) => policy.0.borrow_mut().refuse_work_call = Some(at),
            Fault::RefuseReserve(at) => policy.0.borrow_mut().refuse_reservation = Some(at),
            Fault::RotateSpend(at) => policy.0.borrow_mut().rotate_work_call = Some(at),
            Fault::RotateReserve(at) => policy.0.borrow_mut().rotate_reservation = Some(at),
            _ => {}
        }
        Self {
            caller: caller.map(|value| (Rc::clone(value), state(value))),
            issuers: vec![Rc::clone(&policy.0)],
            policy,
            events: Vec::new(),
            fault,
            spend_ordinal: 0,
            merge_ordinal: 0,
            issuer_reads: Cell::new(0),
        }
    }
    fn check_caller(&self) {
        if let Some((caller, before)) = &self.caller {
            assert_eq!(&state(caller), before);
        }
    }
    fn remember(&mut self) {
        self.check_caller();
        if !Rc::ptr_eq(self.issuers.last().unwrap(), &self.policy.0) {
            self.issuers.push(Rc::clone(&self.policy.0));
        }
    }
    fn accepted(&self) -> usize {
        self.issuers
            .iter()
            .map(|state| state.borrow().forwarded)
            .sum()
    }
    fn assert_released(&self) {
        self.check_caller();
        for state in &self.issuers {
            assert_eq!(state.borrow().live, 0);
        }
        assert_eq!(
            self.accepted(),
            self.events
                .iter()
                .filter_map(|event| match event.operation {
                    Operation::Spend(units) if event.accepted => Some(units),
                    _ => None,
                })
                .sum::<usize>(),
            "accepted work survives each failure and issuer rotation"
        );
    }
}
impl<const M: bool> AudioSoundContextAdmission for Recording<M> {
    const METERED: bool = M;
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.check_caller();
        self.issuer_reads.set(self.issuer_reads.get() + 1);
        self.policy.issuer()
    }
    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        self.check_caller();
        let old = Rc::clone(&self.policy.0);
        let before = OwnState::read(&old);
        self.spend_ordinal += 1;
        let result = match self.fault {
            Fault::WorkLimit(limit) if before.accepted.checked_add(units).unwrap() > limit => {
                Err(PlanError::AudioQueryLimit("test raw accepted-work cap"))
            }
            Fault::CancelSpend(at) if at == self.spend_ordinal => {
                Err(PlanError::AudioQueryLimit("test raw callback cancellation"))
            }
            _ => self.policy.spend(units),
        };
        self.events.push(Event {
            operation: Operation::Spend(units),
            accepted: result.is_ok(),
            before,
            after: OwnState::read(&old),
        });
        self.remember();
        result
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        self.check_caller();
        let old = Rc::clone(&self.policy.0);
        let before = OwnState::read(&old);
        let result = self.policy.reserve(layout);
        if let Ok(lease) = &result {
            assert!(Rc::ptr_eq(&lease.state, &old));
            assert_eq!(lease.bytes, layout.size());
        }
        self.events.push(Event {
            operation: Operation::Reserve {
                size: layout.size(),
                align: layout.align(),
            },
            accepted: result.is_ok(),
            before,
            after: OwnState::read(&old),
        });
        self.remember();
        result
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        self.check_caller();
        let old = Rc::clone(&self.policy.0);
        let before = OwnState::read(&old);
        assert!(Rc::ptr_eq(&target.state, &old));
        assert!(Rc::ptr_eq(&source.state, &old));
        let target_before = target.bytes;
        let source_before = source.bytes;
        self.policy.merge(target, source);
        self.events.push(Event {
            operation: Operation::Merge {
                target_before,
                source_before,
                target_after: target.bytes,
                source_after: source.bytes,
            },
            accepted: true,
            before,
            after: OwnState::read(&old),
        });
        self.merge_ordinal += 1;
        if matches!(self.fault, Fault::RotateMerge(at) if at == self.merge_ordinal) {
            self.policy.rotate(usize::MAX);
        }
        self.remember();
    }
}

fn check_bound(plan: &RenderPlan, bound: &AudioBound<'_>, row: &Value) {
    assert_eq!(bound.capture_baseline_snapshot(plan), row["bound_before"]);
    assert_eq!(row["bound_before"], row["bound_after"]);
    assert_eq!(bound.work(), count(&row["literal_bound_work"]));
    assert_eq!(
        row["literal_resolved_work"],
        row["bound_before"]["resolved"]["work"]
    );
    assert_eq!(
        row["literal_lattice_work"],
        row["bound_before"]["resolved"]["lattice"]["work"]
    );
    assert_eq!(row["ordinary"]["local_allowance"], 65_536);
    let helper = &row["existing_internal_metered_helper"];
    assert_eq!(helper["local_before"]["remaining"], 65_536);
    assert_eq!(
        count(&helper["local_after"]["remaining"]) + count(&helper["additional"]),
        65_536
    );
    assert_eq!(helper["canonical"], 0);
}
fn assert_value<const M: bool>(
    plan: &RenderPlan,
    result: &crate::plan::AdmittedAudioBoundDomain<'_, usize, Lease>,
    row: &Value,
    recorder: &Recording<M>,
) {
    assert_eq!(
        raw_value(plan, result.domain()),
        row["ordinary"]["result"]["raw_domain"]
    );
    assert_eq!(result.canonical_units(), 0);
    assert_eq!(
        result.additional_units(),
        if M {
            count(&row["existing_internal_metered_helper"]["additional"])
        } else {
            0
        }
    );
    assert_eq!(result.total_units(), result.additional_units());
    assert_eq!(recorder.accepted(), result.total_units());
    assert_eq!(result.was_forwarded(), M);
    recorder.check_caller();
}
fn frozen_events(row: &Value) -> Vec<Event> {
    row["existing_internal_metered_helper"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(Event::frozen)
        .collect()
}
fn public_events(old: &[Event], actual: &[Event]) -> usize {
    assert!(actual.len() >= old.len());
    assert_eq!(
        &actual[..old.len()],
        old,
        "literal OLD helper prefix including requested Layout and issuer ledger"
    );
    let tail = &actual[old.len()..];
    match tail {
        [] => 0,
        [event] => {
            assert_eq!(
                event.operation,
                Operation::Spend(0),
                "only the explicit public seal may extend the frozen helper trace"
            );
            assert!(event.accepted);
            assert_eq!(event.before.accepted, old.last().unwrap().after.accepted);
            assert_eq!(
                event.after,
                OwnState {
                    work_callbacks: event.before.work_callbacks + 1,
                    ..event.before
                }
            );
            1
        }
        _ => panic!("unexpected extra public callbacks: {tail:?}"),
    }
}

fn baseline(
    plan: &RenderPlan,
    bound: &AudioBound<'_>,
    row: &Value,
    caller: Option<&Rc<RefCell<PolicyState>>>,
) -> (Vec<Event>, usize) {
    let mut recorder = Recording::<true>::new(caller, Fault::None);
    let value = bound.raw_domain_query(&mut recorder).unwrap();
    assert_value(plan, &value, row, &recorder);
    let helper = &row["existing_internal_metered_helper"];
    let seals = public_events(&frozen_events(row), &recorder.events);
    let helper_live = OwnState::frozen(&helper["ledger_while_result_live"]);
    if seals == 1 {
        assert_eq!(recorder.events.last().unwrap().before, helper_live);
    }
    assert_eq!(
        OwnState::read(&recorder.policy.0),
        OwnState {
            work_callbacks: helper_live.work_callbacks + seals,
            ..helper_live
        }
    );
    // A seal callback is visible, including its zero units, rather than rebilled.
    assert_eq!(
        recorder.policy.0.borrow().forwarded,
        count(&helper["accepted_work"])
    );
    drop(value);
    recorder.assert_released();
    assert_eq!(
        recorder.policy.0.borrow().peak,
        count(&helper["ledger_after_result_drop"]["peak"])
    );
    (recorder.events, seals)
}

fn false_true(
    plan: &RenderPlan,
    bound: &AudioBound<'_>,
    row: &Value,
    caller: Option<&Rc<RefCell<PolicyState>>>,
) -> usize {
    let mut recorder = Recording::<false>::new(caller, Fault::ResidentLimit(0));
    recorder.policy.0.borrow_mut().refuse_work_call = Some(1);
    recorder.policy.0.borrow_mut().refuse_reservation = Some(1);
    recorder.fault = Fault::WorkLimit(0);
    let before = state(&recorder.policy.0);
    let result = bound.raw_domain_query(&mut recorder);
    match result {
        Ok(value) => {
            assert_eq!(row["ordinary"]["result"]["result"], "Ok");
            assert_value(plan, &value, row, &recorder);
            assert_eq!(state(&recorder.policy.0), before);
            drop(value);
        }
        Err(error) => {
            assert_eq!(row["ordinary"]["result"]["result"], "Error");
            assert_eq!(
                snapshot::error_json(&error),
                row["ordinary"]["result"]["error"]
            );
        }
    }
    assert!(recorder.events.is_empty());
    assert_eq!(recorder.issuer_reads.get(), 0);
    assert_eq!(state(&recorder.policy.0), before);
    recorder.assert_released();
    baseline(plan, bound, row, caller).1
}

fn rejected_event(old: Event, fault: Fault) -> Event {
    let mut expected = Event {
        accepted: false,
        after: old.before,
        ..old
    };
    match fault {
        Fault::RefuseSpend(_) => expected.after.work_callbacks += 1,
        Fault::RefuseReserve(_) | Fault::ResidentLimit(_) => expected.after.reservations += 1,
        Fault::WorkLimit(_) | Fault::CancelSpend(_) => {}
        _ => panic!("not a rejecting fault"),
    }
    expected
}
fn reject(
    bound: &AudioBound<'_>,
    caller: Option<&Rc<RefCell<PolicyState>>>,
    expected: &[Event],
    index: usize,
    fault: Fault,
    label: &'static str,
) {
    let mut recorder = Recording::<true>::new(caller, fault);
    let result = bound.raw_domain_query(&mut recorder);
    assert!(
        matches!(&result, Err(PlanError::AudioQueryLimit(actual)) if *actual == label),
        "{fault:?}"
    );
    drop(result);
    assert_eq!(
        recorder.events.len(),
        index + 1,
        "rejected callback must be last: {fault:?}"
    );
    assert_eq!(&recorder.events[..index], &expected[..index]);
    assert_eq!(
        recorder.events[index],
        rejected_event(expected[index], fault)
    );
    recorder.assert_released();
    assert_eq!(recorder.issuers.len(), 1);
    assert_eq!(recorder.accepted(), expected[index].before.accepted);
}
fn budgets(
    plan: &RenderPlan,
    bound: &AudioBound<'_>,
    row: &Value,
    caller: Option<&Rc<RefCell<PolicyState>>>,
) -> usize {
    let (expected, seals) = baseline(plan, bound, row, caller);
    let helper = &row["existing_internal_metered_helper"];
    let work = count(&helper["accepted_work"]);
    let peak = count(&helper["ledger_after_result_drop"]["peak"]);
    assert!((11..=30).contains(&work));
    for fault in [Fault::WorkLimit(work), Fault::ResidentLimit(peak)] {
        let mut recorder = Recording::<true>::new(caller, fault);
        let value = bound.raw_domain_query(&mut recorder).unwrap();
        assert_value(plan, &value, row, &recorder);
        assert_eq!(recorder.events, expected);
        drop(value);
        recorder.assert_released();
    }
    let work_index = expected
        .iter()
        .position(|event| {
            matches!(event.operation,
        Operation::Spend(units) if event.before.accepted + units > work - 1)
        })
        .unwrap();
    reject(
        bound,
        caller,
        &expected,
        work_index,
        Fault::WorkLimit(work - 1),
        "test raw accepted-work cap",
    );
    let peak_index = expected
        .iter()
        .position(|event| {
            matches!(event.operation,
        Operation::Reserve { .. } if event.after.live > peak - 1)
        })
        .unwrap();
    reject(
        bound,
        caller,
        &expected,
        peak_index,
        Fault::ResidentLimit(peak - 1),
        "test wide allocation refusal",
    );
    seals
}

fn failures(
    plan: &RenderPlan,
    bound: &AudioBound<'_>,
    row: &Value,
    caller: Option<&Rc<RefCell<PolicyState>>>,
) -> (usize, usize) {
    let (expected, seals) = baseline(plan, bound, row, caller);
    let mut spends = 0;
    let mut reserves = 0;
    let mut merges = 0;
    let mut attempts = 0;
    for (index, event) in expected.iter().enumerate() {
        let rotation = match event.operation {
            Operation::Spend(_) => {
                spends += 1;
                reject(
                    bound,
                    caller,
                    &expected,
                    index,
                    Fault::RefuseSpend(spends),
                    "test wide work refusal",
                );
                reject(
                    bound,
                    caller,
                    &expected,
                    index,
                    Fault::CancelSpend(spends),
                    "test raw callback cancellation",
                );
                attempts += 2;
                Fault::RotateSpend(spends)
            }
            Operation::Reserve { .. } => {
                reserves += 1;
                reject(
                    bound,
                    caller,
                    &expected,
                    index,
                    Fault::RefuseReserve(reserves),
                    "test wide reservation refusal",
                );
                attempts += 1;
                Fault::RotateReserve(reserves)
            }
            Operation::Merge { .. } => {
                merges += 1;
                Fault::RotateMerge(merges)
            }
        };
        let mut recorder = Recording::<true>::new(caller, rotation);
        let result = bound.raw_domain_query(&mut recorder);
        assert!(
            matches!(&result, Err(PlanError::AudioMetadataIssuerMismatch)),
            "{rotation:?}"
        );
        drop(result);
        assert_eq!(
            recorder.events,
            expected[..=index],
            "accepted rotation callback must be last"
        );
        assert_eq!(recorder.issuers.len(), 2);
        assert!(!Rc::ptr_eq(&recorder.issuers[0], &recorder.issuers[1]));
        assert_eq!(recorder.issuers[0].borrow().forwarded, event.after.accepted);
        assert_eq!(recorder.issuers[1].borrow().forwarded, 0);
        assert_eq!(recorder.issuers[1].borrow().reservations, 0);
        recorder.assert_released();
        attempts += 1;
    }
    (attempts, seals)
}

#[derive(Clone, Copy)]
enum Mode {
    Values,
    Budgets,
    Failures,
    Phase,
    BorrowedPointSignal,
}
struct Run {
    mode: Mode,
    rows: Vec<Value>,
    next: usize,
    roots: usize,
    points: usize,
    attempts: usize,
    seal_callbacks: usize,
    borrowed_points: usize,
}
thread_local! { static RUN: RefCell<Option<Run>> = const { RefCell::new(None) }; }
struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        RUN.with(|run| *run.borrow_mut() = None);
    }
}
fn borrowed_point_signal(
    plan: &RenderPlan,
    bound: &AudioBound<'_>,
    row: &Value,
    caller: Option<&Rc<RefCell<PolicyState>>>,
) -> (usize, usize) {
    let mut recorder = Recording::<true>::new(caller, Fault::None);
    let result = bound.raw_domain_query(&mut recorder).unwrap();
    assert_value(plan, &result, row, &recorder);
    let is_point = point_borrow_tests::assert_matches_frozen(plan, result.domain(), row);
    let seals = public_events(&frozen_events(row), &recorder.events);
    drop(result);
    recorder.assert_released();
    (usize::from(is_point), seals)
}

fn phase_query(
    plan: &RenderPlan,
    bound: &AudioBound<'_>,
    row: &Value,
    caller: Option<&Rc<RefCell<PolicyState>>>,
) -> usize {
    let mut recorder = Recording::<true>::new(caller, Fault::None);
    let live = Rc::clone(&recorder.policy.0);
    let result = crate::plan::audio_bound::phase_trace::with_live(
        move || live.borrow().live,
        || bound.raw_domain_query(&mut recorder),
    );
    match result {
        Ok(value) => {
            assert_eq!(row["ordinary"]["result"]["result"], "Ok");
            assert_value(plan, &value, row, &recorder);
            drop(value);
        }
        Err(error) => {
            assert_eq!(row["ordinary"]["result"]["result"], "Error");
            assert_eq!(
                snapshot::error_json(&error),
                row["ordinary"]["result"]["error"]
            );
        }
    }
    let seals = public_events(&frozen_events(row), &recorder.events);
    recorder.assert_released();
    seals
}
pub(super) fn observe(
    plan: &RenderPlan,
    bound: &AudioBound<'_>,
    caller: Option<&Rc<RefCell<PolicyState>>>,
) {
    let Some((mode, row)) = RUN.with(|run| {
        let mut run = run.borrow_mut();
        let run = run.as_mut()?;
        let row = run
            .rows
            .get(run.next)
            .expect("unexpected extra actual Bound")
            .clone();
        run.next += 1;
        Some((run.mode, row))
    }) else {
        return;
    };
    check_bound(plan, bound, &row);
    assert_eq!(caller_state(caller), row["ordinary"]["caller_before"]);
    let (attempts, seals, borrowed_points) = match mode {
        Mode::Values => (0, false_true(plan, bound, &row, caller), 0),
        Mode::Budgets => (4, budgets(plan, bound, &row, caller), 0),
        Mode::Failures => {
            let (attempts, seals) = failures(plan, bound, &row, caller);
            (attempts, seals, 0)
        }
        Mode::Phase => (0, phase_query(plan, bound, &row, caller), 0),
        Mode::BorrowedPointSignal => {
            let (borrowed_points, seals) = borrowed_point_signal(plan, bound, &row, caller);
            (0, seals, borrowed_points)
        }
    };
    check_bound(plan, bound, &row);
    assert_eq!(
        caller_state(caller),
        row["ordinary"]["caller_after_result_drop"]
    );
    RUN.with(|run| {
        let mut run = run.borrow_mut();
        let run = run.as_mut().unwrap();
        match row["ordinary"]["result"]["raw_domain"]["tag"]
            .as_str()
            .unwrap()
        {
            "RootRoundEven" => run.roots += 1,
            "PointCeil" => run.points += 1,
            other => panic!("unwitnessed actual raw variant {other}"),
        }
        run.attempts += attempts;
        run.seal_callbacks += seals;
        run.borrowed_points += borrowed_points;
    });
}
fn run(mode: Mode) {
    let old: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/raw-domain-query-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let mut rows: Vec<Value> = old["retained_some_constructions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value["construction"].clone())
        .collect();
    for case in old["direct_processing"]["cases"].as_array().unwrap() {
        for lane in ["processing", "root_signal"] {
            if let Some(values) =
                case[lane]["bound_constructions_while_actual_query_live"].as_array()
            {
                rows.extend(values.iter().map(|value| value["construction"].clone()));
            }
        }
    }
    assert_eq!(rows.len(), 77);
    let frozen_failure_attempts = rows
        .iter()
        .map(|row| {
            frozen_events(row)
                .iter()
                .map(|event| match event.operation {
                    Operation::Spend(_) => 3,
                    Operation::Reserve { .. } => 2,
                    Operation::Merge { .. } => 1,
                })
                .sum::<usize>()
        })
        .sum::<usize>();
    RUN.with(|run| {
        assert!(run.borrow().is_none());
        *run.borrow_mut() = Some(Run {
            mode,
            rows,
            next: 0,
            roots: 0,
            points: 0,
            attempts: 0,
            seal_callbacks: 0,
            borrowed_points: 0,
        });
    });
    let _reset = Reset;
    assert_eq!(
        matrix_raw(),
        old,
        "all original input/caller/cached values and setup None/rejected cases remain frozen"
    );
    let run = RUN.with(|run| run.borrow_mut().take().unwrap());
    assert_eq!((run.next, run.roots, run.points), (77, 59, 18));
    assert!(
        matches!(run.seal_callbacks, 0 | 77),
        "uniform explicit public seal tail"
    );
    match mode {
        Mode::Values | Mode::Phase => assert_eq!(run.attempts, 0),
        Mode::Budgets => assert_eq!(run.attempts, 77 * 4),
        Mode::Failures => assert_eq!(
            run.attempts,
            frozen_failure_attempts + run.seal_callbacks * 3
        ),
        Mode::BorrowedPointSignal => {
            assert_eq!(run.attempts, 0);
            assert_eq!(run.borrowed_points, 18);
        }
    }
}
#[test]
fn public_false_and_true_match_all_77_frozen_operands_without_rebilling() {
    run(Mode::Values);
}
#[test]
fn borrowed_point_signal_matches_all_18_frozen_point_payloads() {
    run(Mode::BorrowedPointSignal);
}

#[test]
fn public_exact_and_one_short_work_and_requested_peak_for_all_77() {
    run(Mode::Budgets);
}
#[test]
fn public_every_callback_refusal_cancellation_and_issuer_rotation_for_all_77() {
    run(Mode::Failures);
}

#[test]
fn phase_heap_lower_bounds_are_covered_by_actual_public_policy_live_bytes_for_all_77() {
    crate::plan::audio_bound::phase_trace::start();
    run(Mode::Phase);
    let summary = crate::plan::audio_bound::phase_trace::finish();
    let counts = summary.phases.map(|row| row.calls);
    assert_eq!(
        counts[0], 77,
        "one actual temporary definition per public query"
    );
    assert_eq!(
        counts[1], 59,
        "one actual Root initial allocation per public query"
    );
    assert_eq!(
        counts[2], 59,
        "one actual Root selector replacement per public query"
    );
    assert_eq!(
        counts[3], 59,
        "one actual Root repeat replacement per public query"
    );
    assert!(
        counts[4] > 0,
        "actual Root constraint replacements are exercised"
    );
    assert_eq!(
        counts[5], 59,
        "one actual Root instance replacement per public query"
    );
    assert_eq!(
        counts[6], 18,
        "one actual Point initial allocation per public query"
    );
    assert_eq!(
        counts[7], 18,
        "one actual Point evaluation replacement per public query"
    );
    for (index, count) in summary.phases.iter().enumerate() {
        assert!(
            count.max_heap_lower_bound > 0,
            "phase {index} has measured owned heap"
        );
    }
    assert_eq!(
        summary.phases[2].selector_pairs, 0,
        "all frozen Root evaluation selectors are None"
    );
    assert_eq!(summary.phases[2].nonempty_new_selectors, 0);
    assert_eq!(
        summary.phases[2].nonempty_new_selectors + summary.phases[2].empty_new_selectors,
        counts[2],
        "Root Some and None replacement options are counted from actual values"
    );
    assert_eq!(summary.phases[0].logical_definition_ids, 77);
    assert!(summary.phases[6].selector_pairs == 18);
    assert!(summary.phases[7].selector_pairs > 0);
    assert_eq!(
        summary.phases[7].selector_pairs,
        summary.phases[7].nonempty_new_selectors
    );
    assert_eq!(
        summary.phases[7].nonempty_new_selectors + summary.phases[7].empty_new_selectors,
        counts[7],
        "Point Some and None evaluation options are counted from actual values"
    );
}
