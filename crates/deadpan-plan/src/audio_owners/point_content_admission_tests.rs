//! Public Point content acceptance driven by the unchanged frozen OLD matrices.
use super::*;
use std::cell::Cell;

fn error_json(value: &PlanError) -> Value {
    match value {
        PlanError::AudioMetadataIssuerMismatch => {
            json!({"variant":"AudioMetadataIssuerMismatch"})
        }
        other => snapshot::error_json(other),
    }
}

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static REQUEST_COUNT: Cell<usize> = const { Cell::new(0) };
}
struct Session;
impl Session {
    fn new() -> Self {
        ACTIVE.with(|active| assert!(!active.replace(true)));
        REQUEST_COUNT.with(|count| count.set(0));
        Self
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.set(false));
    }
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    None,
    Cap(usize),
    RefuseSpend(usize),
    RefuseReserve(usize),
    CancelSpend(usize),
    RotateSpend(usize),
    RotateReserve(usize),
    RotateMerge(usize),
}
struct Metered {
    inner: super::Recording,
    issuers: Vec<Rc<RefCell<PolicyState>>>,
    fault: Fault,
    spends: usize,
    merges: usize,
}
impl Metered {
    fn new(fault: Fault) -> Self {
        let policy = Policy::<true>::new(match fault {
            Fault::Cap(cap) => cap,
            _ => usize::MAX,
        });
        match fault {
            Fault::RefuseSpend(at) => policy.0.borrow_mut().refuse_work_call = Some(at),
            Fault::RefuseReserve(at) => policy.0.borrow_mut().refuse_reservation = Some(at),
            Fault::RotateSpend(at) => policy.0.borrow_mut().rotate_work_call = Some(at),
            Fault::RotateReserve(at) => policy.0.borrow_mut().rotate_reservation = Some(at),
            _ => {}
        }
        let issuers = vec![Rc::clone(&policy.0)];
        Self {
            inner: super::Recording {
                policy,
                events: Vec::new(),
            },
            issuers,
            fault,
            spends: 0,
            merges: 0,
        }
    }
    fn remember(&mut self) {
        if !Rc::ptr_eq(self.issuers.last().unwrap(), &self.inner.policy.0) {
            self.issuers.push(Rc::clone(&self.inner.policy.0));
        }
    }
    fn ledger(&self) -> Value {
        Value::Array(self.issuers.iter().map(state).collect())
    }
    fn released(&self) {
        for issuer in &self.issuers {
            assert_eq!(issuer.borrow().live, 0, "all real query leases released");
        }
        let accepted: usize = self.issuers.iter().map(|v| v.borrow().forwarded).sum();
        let events: usize = self
            .inner
            .events
            .iter()
            .filter(|v| v["event"] == "spend" && v["accepted"] == true)
            .map(|v| v["units"].as_u64().unwrap() as usize)
            .sum();
        assert_eq!(accepted, events, "no refund or duplicate forwarding");
    }
}
impl AudioSoundContextAdmission for Metered {
    const METERED: bool = true;
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.inner.issuer()
    }
    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        self.spends += 1;
        let result = if matches!(self.fault, Fault::CancelSpend(at) if at == self.spends) {
            let before = state(&self.inner.policy.0);
            let error = PlanError::AudioQueryLimit("test Point callback cancellation");
            self.inner
                .events
                .push(json!({"event":"spend","units":units,"accepted":false,
                "before":before,"after":state(&self.inner.policy.0),
                "error":error_json(&error)}));
            Err(error)
        } else {
            self.inner.spend(units)
        };
        self.remember();
        result
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        let result = self.inner.reserve(layout);
        self.remember();
        result
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        self.inner.merge(target, source);
        self.merges += 1;
        if matches!(self.fault, Fault::RotateMerge(at) if at == self.merges) {
            self.inner.policy.rotate(usize::MAX);
        }
        self.remember();
    }
}
struct False;
impl AudioSoundContextAdmission for False {
    const METERED: bool = false;
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        panic!("false query inspected issuer")
    }
    fn spend(&mut self, _: usize) -> Result<(), PlanError> {
        panic!("false query spent work")
    }
    fn reserve(&mut self, _: Layout) -> Result<Lease, PlanError> {
        panic!("false query reserved")
    }
    fn merge(&mut self, _: &mut Lease, _: &mut Lease) {
        panic!("false query merged")
    }
}
#[derive(Debug, PartialEq)]
struct Observation {
    result: Value,
    receipt: Option<(usize, usize, usize, bool)>,
    events: Vec<Value>,
    live: Value,
    after_drop: Value,
}
fn helper(
    plan: &RenderPlan,
    point: &crate::AudioPointDomain<'_>,
    request: &Request,
    fault: Fault,
) -> Observation {
    let mut policy = Metered::new(fault);
    let mut local = Budget {
        remaining: request.limits.maximum_work,
        lookup: Default::default(),
    };
    let (value, canonical, additional) = {
        let mut budget = QueryBudget::new(&mut local, &mut policy);
        let value = admission::outer::<_, Metered>(point.signal_ref().structural_child_query(
            request.samples.clone(),
            request.limits,
            AudioBoundaryRule::PointCeil,
            (true, true),
            &mut budget,
        ));
        (value, budget.canonical_used, budget.additional_used)
    };
    let (result, receipt) = match &value {
        Ok(value) => {
            let total = canonical.checked_add(additional).unwrap();
            assert_eq!(value.query.work, total);
            assert_eq!(request.limits.maximum_work - local.remaining, total);
            (
                json!({"result":"Ok","query":query_snapshot(plan,&value.query)}),
                Some((canonical, additional, total, true)),
            )
        }
        Err(error) => (json!({"result":"Error","error":error_json(error)}), None),
    };
    let live = policy.ledger();
    drop(value);
    policy.released();
    Observation {
        result,
        receipt,
        events: policy.inner.events.clone(),
        live,
        after_drop: policy.ledger(),
    }
}
fn public(
    plan: &RenderPlan,
    point: &crate::AudioPointDomain<'_>,
    request: &Request,
    fault: Fault,
) -> Observation {
    let mut policy = Metered::new(fault);
    let value = point.content_query_admitted(request.samples.clone(), request.limits, &mut policy);
    let (result, receipt) = match &value {
        Ok(value) => {
            assert!(value.was_forwarded());
            assert_eq!(value.total_units(), value.query().work);
            assert_eq!(
                value
                    .canonical_units()
                    .checked_add(value.additional_units())
                    .unwrap(),
                value.total_units()
            );
            (
                json!({"result":"Ok","query":query_snapshot(plan,value.query())}),
                Some((
                    value.canonical_units(),
                    value.additional_units(),
                    value.total_units(),
                    value.was_forwarded(),
                )),
            )
        }
        Err(error) => (json!({"result":"Error","error":error_json(error)}), None),
    };
    let live = policy.ledger();
    drop(value);
    policy.released();
    Observation {
        result,
        receipt,
        events: policy.inner.events.clone(),
        live,
        after_drop: policy.ledger(),
    }
}
fn pair(
    plan: &RenderPlan,
    point: &crate::AudioPointDomain<'_>,
    request: &Request,
    fault: Fault,
) -> Observation {
    let before = point.probe_baseline_snapshot(plan);
    let expected = helper(plan, point, request, fault);
    let actual = public(plan, point, request, fault);
    assert_eq!(actual, expected, "request={} fault={fault:?}", request.name);
    assert_eq!(point.probe_baseline_snapshot(plan), before);
    expected
}
fn false_query(plan: &RenderPlan, point: &crate::AudioPointDomain<'_>, request: &Request) {
    let ordinary = point
        .signal_ref()
        .query(request.samples.clone(), request.limits);
    let actual = point.content_query_admitted(request.samples.clone(), request.limits, &mut False);
    match (&ordinary, &actual) {
        (Ok(expected), Ok(actual)) => {
            assert_eq!(
                query_snapshot(plan, actual.query()),
                query_snapshot(plan, expected)
            );
            assert_eq!(actual.canonical_units(), expected.work);
            assert_eq!(actual.additional_units(), 0);
            assert_eq!(actual.total_units(), expected.work);
            assert!(!actual.was_forwarded());
        }
        (Err(expected), Err(actual)) => {
            assert_eq!(error_json(actual), error_json(expected))
        }
        _ => panic!("false result differs from ordinary for {}", request.name),
    }
}
fn shared_raw_query(
    plan: &RenderPlan,
    expected_point: &crate::AudioPointDomain<'_>,
    bound: &AudioBound<'_>,
    request: &Request,
    isolated: &Observation,
) {
    let mut policy = recording();
    let raw = bound.raw_domain_query(&mut policy).unwrap();
    let raw_body = raw_snapshot(plan, &raw);
    let AudioBoundDomain::Point(point) = raw.domain() else {
        panic!("existing Point changed kind")
    };
    assert_eq!(
        point.probe_baseline_snapshot(plan),
        expected_point.probe_baseline_snapshot(plan)
    );
    let before = state(&policy.policy.0);
    let raw_live = before["live"].as_u64().unwrap();
    assert!(raw_live > 0, "real raw carrier must remain resident");
    let event_start = policy.events.len();
    let value = point.content_query_admitted(request.samples.clone(), request.limits, &mut policy);
    let body = match &value {
        Ok(value) => {
            assert_eq!(
                Some((
                    value.canonical_units(),
                    value.additional_units(),
                    value.total_units(),
                    value.was_forwarded()
                )),
                isolated.receipt
            );
            json!({"result":"Ok","query":query_snapshot(plan,value.query())})
        }
        Err(error) => json!({"result":"Error","error":error_json(error)}),
    };
    assert_eq!(body, isolated.result);
    let live = state(&policy.policy.0);
    let query_live: u64 = isolated
        .live
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["live"].as_u64().unwrap())
        .sum();
    assert_eq!(live["live"].as_u64().unwrap(), raw_live + query_live);
    let query_work: u64 = isolated
        .after_drop
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["accepted_work"].as_u64().unwrap())
        .sum();
    assert_eq!(
        live["accepted_work"].as_u64().unwrap() - before["accepted_work"].as_u64().unwrap(),
        query_work
    );
    let shape = |event: &Value| {
        let mut event = event.clone();
        event.as_object_mut().unwrap().remove("before");
        event.as_object_mut().unwrap().remove("after");
        event
    };
    assert_eq!(
        policy.events[event_start..]
            .iter()
            .map(shape)
            .collect::<Vec<_>>(),
        isolated.events.iter().map(shape).collect::<Vec<_>>()
    );
    drop(value);
    assert_eq!(state(&policy.policy.0)["live"], before["live"]);
    assert_eq!(raw_snapshot(plan, &raw), raw_body);
    drop(raw);
    assert_eq!(state(&policy.policy.0)["live"], 0);
}

pub(super) fn observe_request(
    plan: &RenderPlan,
    point: &crate::AudioPointDomain<'_>,
    request: &Request,
    bound: Option<&AudioBound<'_>>,
) {
    if !ACTIVE.with(Cell::get) {
        return;
    }
    REQUEST_COUNT.with(|count| count.set(count.get() + 1));
    false_query(plan, point, request);
    let baseline = pair(plan, point, request, Fault::None);
    if let Some(bound) = bound {
        shared_raw_query(plan, point, bound, request, &baseline);
    }
    if request.name != "full" || baseline.receipt.is_none() {
        return;
    }
    let total = baseline.receipt.unwrap().2;
    let mut exact = request.clone();
    exact.limits.maximum_work = total;
    assert!(pair(plan, point, &exact, Fault::None).receipt.is_some());
    if total > 0 {
        exact.limits.maximum_work = total - 1;
        assert!(pair(plan, point, &exact, Fault::None).receipt.is_none());
    }
    let peak = baseline
        .live
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["peak"].as_u64().unwrap() as usize)
        .max()
        .unwrap();
    assert!(
        pair(plan, point, request, Fault::Cap(peak))
            .receipt
            .is_some()
    );
    if peak > 0 {
        assert!(
            pair(plan, point, request, Fault::Cap(peak - 1))
                .receipt
                .is_none()
        );
    }
    let (mut spend, mut reserve, mut merge) = (0, 0, 0);
    for (index, event) in baseline.events.iter().enumerate() {
        let faults: Vec<Fault> = match event["event"].as_str().unwrap() {
            "spend" => {
                spend += 1;
                vec![
                    Fault::RefuseSpend(spend),
                    Fault::CancelSpend(spend),
                    Fault::RotateSpend(spend),
                ]
            }
            "reserve" => {
                reserve += 1;
                vec![Fault::RefuseReserve(reserve), Fault::RotateReserve(reserve)]
            }
            "merge" => {
                merge += 1;
                vec![Fault::RotateMerge(merge)]
            }
            other => panic!("unknown helper event {other}"),
        };
        for fault in faults {
            let failed = pair(plan, point, request, fault);
            assert!(failed.receipt.is_none(), "fault not exercised: {fault:?}");
            assert_eq!(
                failed.events.len(),
                index + 1,
                "no callbacks after rejection/rotation"
            );
            for (actual, expected) in failed.events[..index].iter().zip(&baseline.events[..index]) {
                let shape = |event: &Value| {
                    let mut event = event.clone();
                    event.as_object_mut().unwrap().remove("before");
                    event.as_object_mut().unwrap().remove("after");
                    event
                };
                assert_eq!(
                    shape(actual),
                    shape(expected),
                    "accepted helper callback prefix"
                );
            }
        }
    }
}

#[test]
fn public_point_content_matches_complete_initial_old_matrix() {
    let _session = Session::new();
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/point-content-query-baseline-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(matrix_point_content(), expected);
    assert_eq!(REQUEST_COUNT.with(Cell::get), 306);
}
#[test]
fn public_point_content_matches_complete_public_supplement() {
    let _session = Session::new();
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/point-content-query-public-supplement-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(supplement::matrix_supplement(), expected);
    assert_eq!(REQUEST_COUNT.with(Cell::get), 53);
}
