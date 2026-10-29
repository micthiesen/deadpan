//! Capability, first-match, and finite-resource failures of the new START path.
use super::*;
use crate::plan::audio_owners::tests::legacy_snapshot_v1::wide_baseline;
use deadpan_core::ProjectDocument;
use serde_json::Value;
use std::{alloc::Layout, cell::RefCell, rc::Rc};

#[derive(Default)]
struct State {
    live: usize,
    peak: usize,
    cap: usize,
    work: usize,
    work_calls: usize,
    reserve_calls: usize,
    refuse_work: Option<usize>,
    refuse_reserve: Option<usize>,
    rotate_work: Option<usize>,
    rotate_reserve: Option<usize>,
    cancel_after: Option<usize>,
}
struct Policy<const M: bool> {
    state: Rc<RefCell<State>>,
    issuer: usize,
}
struct Lease {
    state: Rc<RefCell<State>>,
    bytes: usize,
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.state.borrow_mut().live -= self.bytes;
    }
}
impl<const M: bool> Policy<M> {
    fn new(cap: usize) -> Self {
        Self {
            state: Rc::new(RefCell::new(State {
                cap,
                ..Default::default()
            })),
            issuer: 1,
        }
    }
}
impl<const M: bool> AudioSoundContextAdmission for Policy<M> {
    const METERED: bool = M;
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.issuer
    }
    fn spend(&mut self, n: usize) -> Result<(), PlanError> {
        let mut state = self.state.borrow_mut();
        state.work_calls += 1;
        if state.refuse_work == Some(state.work_calls)
            || state.cancel_after.is_some_and(|limit| state.work >= limit)
        {
            return Err(PlanError::AudioQueryLimit("test span-start cancellation"));
        }
        state.work += n;
        if state.rotate_work == Some(state.work_calls) {
            self.issuer += 1;
        }
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        let mut state = self.state.borrow_mut();
        state.reserve_calls += 1;
        if state.refuse_reserve == Some(state.reserve_calls)
            || state.live + layout.size() > state.cap
        {
            return Err(PlanError::AudioQueryLimit(
                "test span-start allocation refusal",
            ));
        }
        state.live += layout.size();
        state.peak = state.peak.max(state.live);
        if state.rotate_reserve == Some(state.reserve_calls) {
            self.issuer += 1;
        }
        Ok(Lease {
            state: Rc::clone(&self.state),
            bytes: layout.size(),
        })
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        assert!(Rc::ptr_eq(&target.state, &self.state) && Rc::ptr_eq(&source.state, &self.state));
        target.bytes += source.bytes;
        source.bytes = 0;
    }
}
fn frozen() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json"
    ))
    .unwrap()
}
fn plan() -> RenderPlan {
    let fixture = frozen();
    let document =
        ProjectDocument::from_json(&fixture["documents"][0]["document"].to_string()).unwrap();
    RenderPlan::compile(&document).unwrap()
}
fn candidates<'p, const M: bool>(
    plan: &'p RenderPlan,
    policy: &mut Policy<M>,
) -> AdmittedAudioOwnerSpanStartCandidates<'p, 'p, usize, Lease> {
    plan.audio_owner_span_start_candidates_admitted(
        AudioSample(0)..AudioSample(512),
        Default::default(),
        policy,
    )
    .unwrap()
}
fn request<'a, 'p>(
    query: &'a AdmittedAudioOwnerSpanStartCandidates<'_, 'p, usize, Lease>,
) -> AudioOwnerSpanStartRequest<'a, 'p> {
    AudioOwnerSpanStartRequest {
        span_index: 0,
        requested: query.query().spans()[0].owners().last().unwrap(),
        at: AudioSample(0),
    }
}
fn failed<T>(value: Result<T, PlanError>) -> PlanError {
    match value {
        Ok(_) => panic!("expected checked capability/resource failure"),
        Err(error) => error,
    }
}

#[test]
fn rejects_foreign_root_equivalent_domain_interior_and_inactive_without_new_work() {
    let other = plan();
    let plan = plan();
    let mut policy = Policy::<true>::new(1_048_576);
    let query = candidates(&plan, &mut policy);
    let paid = policy.state.borrow().work;
    assert!(matches!(
        failed(query.capture_root(&other, request(&query), Default::default(), &mut policy)),
        PlanError::InvalidPlan("foreign span-start capture parent")
    ));
    let mut interior = request(&query);
    interior.at = AudioSample(1);
    assert!(matches!(
        failed(query.capture_root(&plan, interior, Default::default(), &mut policy)),
        PlanError::InvalidPlan("input observation is not span start")
    ));
    let mut outside = request(&query);
    outside.span_index = usize::MAX;
    assert!(matches!(
        failed(query.capture_root(&plan, outside, Default::default(), &mut policy)),
        PlanError::InvalidPlan("span-start index out of range")
    ));
    assert_eq!(policy.state.borrow().work, paid);
    let first = plan
        .audio_owner_input(
            request(&query).requested,
            AudioSample(0),
            Default::default(),
        )
        .unwrap();
    let equivalent = plan
        .audio_owner_input(
            request(&query).requested,
            AudioSample(0),
            Default::default(),
        )
        .unwrap();
    let domain = first.context();
    let domain_query = domain
        .owner_span_start_candidates_admitted(
            domain.root_samples(),
            Default::default(),
            &mut policy,
        )
        .unwrap();
    let before = policy.state.borrow().work;
    assert!(matches!(
        failed(domain_query.capture_domain(
            equivalent.context(),
            request(&domain_query),
            Default::default(),
            &mut policy
        )),
        PlanError::InvalidPlan("foreign span-start capture parent")
    ));
    assert_eq!(policy.state.borrow().work, before);
    drop(domain_query);
    drop(query);
    assert_eq!(policy.state.borrow().live, 0);

    let mut checked = 0;
    wide_baseline::visit_cases(&frozen(), |parent, case| {
        let window = &case["request"]["window"];
        let samples = AudioSample(window["start"].as_i64().unwrap())
            ..AudioSample(window["end"].as_i64().unwrap());
        let mut policy = Policy::<true>::new(1_048_576);
        let query = match parent {
            wide_baseline::Parent::Root(plan) => plan
                .audio_owner_span_start_candidates_admitted(
                    samples,
                    Default::default(),
                    &mut policy,
                )
                .unwrap(),
            wide_baseline::Parent::Domain(domain) => domain
                .owner_span_start_candidates_admitted(samples, Default::default(), &mut policy)
                .unwrap(),
        };
        let witness = query
            .query()
            .spans()
            .iter()
            .flat_map(|span| span.owners())
            .next()
            .unwrap();
        for (index, span) in query
            .query()
            .spans()
            .iter()
            .enumerate()
            .filter(|(_, span)| span.support() == AudioOwnerSupport::Inactive)
        {
            let before = policy.state.borrow().work;
            let request = AudioOwnerSpanStartRequest {
                span_index: index,
                requested: witness,
                at: span.samples().start,
            };
            let result = match parent {
                wide_baseline::Parent::Root(plan) => {
                    query.capture_root(plan, request, Default::default(), &mut policy)
                }
                wide_baseline::Parent::Domain(domain) => {
                    query.capture_domain(domain, request, Default::default(), &mut policy)
                }
            };
            assert!(matches!(
                failed(result),
                PlanError::InvalidPlan("inactive span-start input")
            ));
            assert_eq!(policy.state.borrow().work, before);
            checked += 1;
        }
        drop(query);
        assert_eq!(policy.state.borrow().live, 0);
    });
    assert!(checked > 0);
}

#[test]
fn synthetic_duplicate_private_matches_choose_first_and_false_scan_costs_zero() {
    let plan = plan();
    let mut policy = Policy::<true>::new(1_048_576);
    let query = candidates(&plan, &mut policy);
    // Explicit synthetic vector, not a claimed normal-document oracle. Both
    // clocks are actual identical observations; pointer proves first selection.
    let owners = vec![
        request(&query).requested.clone(),
        request(&query).requested.clone(),
    ];
    for metered in [true, false] {
        let mut local = Budget {
            remaining: 100,
            lookup: LookupStats::default(),
        };
        if metered {
            let mut budget = admission::Budget::new(&mut local, &mut policy);
            let found = first_match(&owners, &owners[1], AudioSample(0), &mut budget).unwrap();
            assert!(std::ptr::eq(found, &owners[0]));
            assert_eq!(budget.additional_used, 2);
        } else {
            let mut unmetered = Policy::<false>::new(0);
            let mut budget = admission::Budget::new(&mut local, &mut unmetered);
            let found = first_match(&owners, &owners[1], AudioSample(0), &mut budget).unwrap();
            assert!(std::ptr::eq(found, &owners[0]));
            assert_eq!(budget.additional_used, 0);
            assert_eq!(budget.remaining, 100);
        }
    }
}

fn exact_work<const M: bool>() {
    let plan = plan();
    let mut policy = Policy::<M>::new(1_048_576);
    let query = candidates(&plan, &mut policy);
    let wide_live = policy.state.borrow().live;
    let capture = query
        .capture_root(&plan, request(&query), Default::default(), &mut policy)
        .unwrap();
    let work = capture.work();
    assert_eq!(capture.canonical_units() + capture.additional_units(), work);
    assert_eq!(capture.additional_units() == 0, !M);
    assert!(std::ptr::eq(
        capture.shared_query_work(),
        query.shared_query_work()
    ));
    drop(capture);
    assert_eq!(policy.state.borrow().live, wide_live);
    let paid = policy.state.borrow().work;
    let exact = AudioQueryLimits {
        maximum_work: work,
        ..Default::default()
    };
    let capture = query
        .capture_root(&plan, request(&query), exact, &mut policy)
        .unwrap();
    assert_eq!(capture.work(), work);
    drop(capture);
    assert_eq!(policy.state.borrow().work - paid, if M { work } else { 0 });
    let short = AudioQueryLimits {
        maximum_work: work - 1,
        ..Default::default()
    };
    assert!(matches!(
        failed(query.capture_root(&plan, request(&query), short, &mut policy)),
        PlanError::AudioQueryLimit(_)
    ));
    assert_eq!(policy.state.borrow().live, wide_live);
    drop(query);
    assert_eq!(policy.state.borrow().live, 0);
}
#[test]
fn fresh_capture_exact_and_one_short_work_never_rebill_or_subtract_shared_query() {
    exact_work::<false>();
    exact_work::<true>();
}

#[derive(Clone, Copy)]
enum Interrupt {
    Work,
    Reserve,
    RotateWork,
    RotateReserve,
}
fn set_interrupt(state: &mut State, kind: Interrupt, at: usize) {
    match kind {
        Interrupt::Work => state.refuse_work = Some(at),
        Interrupt::Reserve => state.refuse_reserve = Some(at),
        Interrupt::RotateWork => state.rotate_work = Some(at),
        Interrupt::RotateReserve => state.rotate_reserve = Some(at),
    }
}
fn is_work(kind: Interrupt) -> bool {
    matches!(kind, Interrupt::Work | Interrupt::RotateWork)
}

#[test]
fn every_actual_query_and_capture_callback_refusal_or_issuer_rotation_cleans_up() {
    let plan = plan();
    let mut ample = Policy::<true>::new(1_048_576);
    let query = candidates(&plan, &mut ample);
    let qw = ample.state.borrow().work_calls;
    let qr = ample.state.borrow().reserve_calls;
    let capture = query
        .capture_root(&plan, request(&query), Default::default(), &mut ample)
        .unwrap();
    let cw = ample.state.borrow().work_calls - qw;
    let cr = ample.state.borrow().reserve_calls - qr;
    drop(capture);
    drop(query);
    assert_eq!(ample.state.borrow().live, 0);
    assert!(qw > 0 && qr > 0 && cw > 0 && cr > 0);
    for capture_stage in [false, true] {
        for kind in [
            Interrupt::Work,
            Interrupt::Reserve,
            Interrupt::RotateWork,
            Interrupt::RotateReserve,
        ] {
            let count = if capture_stage {
                if is_work(kind) { cw } else { cr }
            } else if is_work(kind) {
                qw
            } else {
                qr
            };
            for index in 1..=count {
                let mut policy = Policy::<true>::new(1_048_576);
                if capture_stage {
                    let query = candidates(&plan, &mut policy);
                    let live = policy.state.borrow().live;
                    let before = policy.state.borrow().work;
                    let base = if is_work(kind) {
                        policy.state.borrow().work_calls
                    } else {
                        policy.state.borrow().reserve_calls
                    };
                    set_interrupt(&mut policy.state.borrow_mut(), kind, base + index);
                    assert!(
                        query
                            .capture_root(&plan, request(&query), Default::default(), &mut policy)
                            .is_err()
                    );
                    assert_eq!(policy.state.borrow().live, live);
                    assert!(policy.state.borrow().work >= before);
                    drop(query);
                } else {
                    set_interrupt(&mut policy.state.borrow_mut(), kind, index);
                    assert!(
                        plan.audio_owner_span_start_candidates_admitted(
                            AudioSample(0)..AudioSample(512),
                            Default::default(),
                            &mut policy
                        )
                        .is_err()
                    );
                }
                assert_eq!(policy.state.borrow().live, 0);
            }
        }
    }
}

#[test]
fn foreign_issuer_mode_and_mid_capture_cancellation_preserve_paid_query() {
    let plan = plan();
    let mut policy = Policy::<true>::new(1_048_576);
    let query = candidates(&plan, &mut policy);
    let live = policy.state.borrow().live;
    let paid = policy.state.borrow().work;
    let mut foreign = Policy::<true>::new(1_048_576);
    foreign.issuer = 2;
    assert!(matches!(
        failed(query.capture_root(&plan, request(&query), Default::default(), &mut foreign)),
        PlanError::AudioMetadataIssuerMismatch
    ));
    let mut false_mode = Policy::<false> {
        state: Rc::clone(&policy.state),
        issuer: policy.issuer,
    };
    assert!(matches!(
        failed(query.capture_root(&plan, request(&query), Default::default(), &mut false_mode)),
        PlanError::InvalidPlan("span-start accounting mode mismatch")
    ));
    assert_eq!(policy.state.borrow().work, paid);
    policy.state.borrow_mut().cancel_after = Some(paid + 1);
    assert!(matches!(
        failed(query.capture_root(&plan, request(&query), Default::default(), &mut policy)),
        PlanError::AudioQueryLimit(_)
    ));
    assert!(policy.state.borrow().work > paid);
    assert_eq!(policy.state.borrow().live, live);
    drop(query);
    assert_eq!(policy.state.borrow().live, 0);
}

#[test]
fn measured_exact_peak_memory_succeeds_and_one_short_releases_every_lease() {
    let plan = plan();
    let mut ample = Policy::<true>::new(1_048_576);
    let query = candidates(&plan, &mut ample);
    let capture = query
        .capture_root(&plan, request(&query), Default::default(), &mut ample)
        .unwrap();
    let peak = ample.state.borrow().peak;
    assert!(peak > 0);
    drop(capture);
    drop(query);
    for (cap, expected) in [(peak, true), (peak - 1, false)] {
        let mut policy = Policy::<true>::new(cap);
        let actual = match plan.audio_owner_span_start_candidates_admitted(
            AudioSample(0)..AudioSample(512),
            Default::default(),
            &mut policy,
        ) {
            Ok(query) => query
                .capture_root(&plan, request(&query), Default::default(), &mut policy)
                .is_ok(),
            Err(_) => false,
        };
        assert_eq!(actual, expected);
        assert_eq!(policy.state.borrow().live, 0);
    }
}
