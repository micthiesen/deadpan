//! Independent fresh raw reconstruction while the genuine input carrier lives.
use super::*;
use crate::plan::audio_owners::admission::{self, Budget as QueryBudget};
use crate::{AudioBound, AudioSignalContent};
use std::cell::Cell;

const CHECKPOINT: &str = "87ab6192e765a34dae9595139e10bd9010d6315e";
const ALLOWANCE: usize = 65_536;

thread_local! {
    static COLLECT: RefCell<Option<Vec<Value>>> = const { RefCell::new(None) };
    static ORIGIN: Cell<Option<&'static str>> = const { Cell::new(None) };
    static FIXTURE: RefCell<Option<Value>> = const { RefCell::new(None) };
}

pub(super) struct FixtureScope(Option<Option<Value>>);
impl FixtureScope {
    pub(super) fn new(descriptor: &Value) -> Self {
        let active = COLLECT.with(|slot| slot.borrow().is_some());
        Self(active.then(|| FIXTURE.with(|slot| slot.replace(Some(descriptor.clone())))))
    }
}
impl Drop for FixtureScope {
    fn drop(&mut self) {
        if let Some(previous) = self.0.take() {
            FIXTURE.with(|slot| *slot.borrow_mut() = previous);
        }
    }
}

pub(super) struct Scope(Option<Option<&'static str>>);
impl Scope {
    pub(super) fn new(origin: &'static str) -> Self {
        let active = COLLECT.with(|slot| slot.borrow().is_some());
        Self(active.then(|| ORIGIN.replace(Some(origin))))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(previous) = self.0 {
            ORIGIN.set(previous);
        }
    }
}
struct Session;
impl Session {
    fn new() -> Self {
        COLLECT.with(|slot| {
            assert!(slot.borrow().is_none());
            *slot.borrow_mut() = Some(Vec::new());
        });
        assert_eq!(ORIGIN.get(), None);
        Self
    }
    fn finish(self) -> Vec<Value> {
        COLLECT.with(|slot| slot.borrow_mut().take().unwrap())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        COLLECT.with(|slot| *slot.borrow_mut() = None);
        ORIGIN.set(None);
        FIXTURE.with(|slot| *slot.borrow_mut() = None);
    }
}

fn state(value: &Rc<RefCell<PolicyState>>) -> Value {
    let value = value.borrow();
    let PolicyState {
        live,
        peak,
        cap,
        reservations,
        forwarded,
        work_calls,
        refuse_reservation,
        refuse_work_call,
        cancel_after,
        rotate_reservation,
        rotate_work_call,
    } = &*value;
    json!({"live":live,"peak":peak,"cap":cap,"reservations":reservations,
        "accepted_work":forwarded,"work_callbacks":work_calls,
        "refuse_reservation":refuse_reservation,"refuse_work_call":refuse_work_call,
        "cancel_after":cancel_after,"rotate_reservation":rotate_reservation,
        "rotate_work_call":rotate_work_call})
}
fn caller_state(value: Option<&Rc<RefCell<PolicyState>>>) -> Value {
    value.map_or_else(|| json!({"kind":"ordinary_no_admission_ledger"}), state)
}

// Forward the existing owning policy; these observations do not model residency.
struct Recording {
    policy: Policy<true>,
    events: Vec<Value>,
}
impl AudioSoundContextAdmission for Recording {
    const METERED: bool = true;
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.policy.issuer()
    }
    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        let before = state(&self.policy.0);
        let result = self.policy.spend(units);
        self.events.push(json!({"event":"spend","units":units,
            "accepted":result.is_ok(),"before":before,"after":state(&self.policy.0),
            "error":result.as_ref().err().map(snapshot::error_json)}));
        result
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        let before = state(&self.policy.0);
        let result = self.policy.reserve(layout);
        self.events.push(
            json!({"event":"reserve","layout":{"size":layout.size(),"align":layout.align()},
            "accepted":result.is_ok(),"before":before,"after":state(&self.policy.0),
            "lease":result.as_ref().ok().map(|lease| json!({"bytes":lease.bytes,
                "same_issuer":Rc::ptr_eq(&lease.state,&self.policy.0)})),
            "error":result.as_ref().err().map(snapshot::error_json)}),
        );
        result
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        let before = state(&self.policy.0);
        let target_before = target.bytes;
        let source_before = source.bytes;
        let same_target = Rc::ptr_eq(&target.state, &self.policy.0);
        let same_source = Rc::ptr_eq(&source.state, &self.policy.0);
        self.policy.merge(target, source);
        self.events.push(
            json!({"event":"merge","before":before,"after":state(&self.policy.0),
            "target_bytes_before":target_before,"source_bytes_before":source_before,
            "target_bytes_after":target.bytes,"source_bytes_after":source.bytes,
            "target_same_issuer":same_target,"source_same_issuer":same_source}),
        );
    }
}

fn raw_value(plan: &RenderPlan, value: &AudioBoundDomain<'_>) -> Value {
    match value {
        AudioBoundDomain::Point(point) => json!({"tag":"PointCeil",
            "actual_private_point":point.probe_baseline_snapshot(plan)}),
        AudioBoundDomain::Root(_) | AudioBoundDomain::Empty => {
            snapshot::capture_baseline::bound_domain(plan, value)
        }
    }
}
fn raw_result(plan: &RenderPlan, value: &Result<AudioBoundDomain<'_>, PlanError>) -> Value {
    match value {
        Ok(value) => json!({"result":"Ok","raw_domain":raw_value(plan,value)}),
        Err(error) => json!({"result":"Error","error":snapshot::error_json(error),
            "partial_work":"ordinary raw_domain exposes no accepted partial-work receipt"}),
    }
}
fn reconstruct(
    plan: &RenderPlan,
    bound: &AudioBound<'_>,
    caller: Option<&Rc<RefCell<PolicyState>>>,
) -> Value {
    let bound_before = bound.capture_baseline_snapshot(plan);
    let resolved_work = bound_before["resolved"]["work"].as_u64().unwrap();
    let lattice_work = bound_before["resolved"]["lattice"]["work"]
        .as_u64()
        .unwrap();
    assert_eq!(bound.work() as u64, resolved_work);
    let caller_before = caller_state(caller);
    let ordinary = bound.raw_domain();
    let ordinary_body = raw_result(plan, &ordinary);
    let caller_while_ordinary_live = caller_state(caller);
    assert_eq!(caller_before, caller_while_ordinary_live);
    drop(ordinary);
    let caller_after_ordinary_drop = caller_state(caller);
    assert_eq!(caller_before, caller_after_ordinary_drop);

    let mut policy = Recording {
        policy: Policy::<true>::new(usize::MAX),
        events: Vec::new(),
    };
    let helper_before = state(&policy.policy.0);
    let mut local = Budget {
        remaining: ALLOWANCE,
        lookup: Default::default(),
    };
    let local_before = json!({"remaining":local.remaining,"lookup":local.lookup});
    let (result, canonical, additional, remaining, lookup) = {
        let mut budget = QueryBudget::new(&mut local, &mut policy);
        let result = admission::outer::<_, Recording>(bound.raw_domain_admitted(&mut budget));
        (
            result,
            budget.canonical_used,
            budget.additional_used,
            budget.remaining,
            budget.lookup,
        )
    };
    assert_eq!(canonical, 0);
    assert_eq!(policy.policy.0.borrow().forwarded, additional);
    assert_eq!(ALLOWANCE - remaining, canonical + additional);
    let helper_body = match &result {
        Ok(value) => json!({"result":"Ok","raw_domain":raw_value(plan,&value.value)}),
        Err(error) => json!({"result":"Error","error":snapshot::error_json(error)}),
    };
    assert_eq!(ordinary_body["result"], helper_body["result"]);
    match &result {
        Ok(_) => assert_eq!(ordinary_body["raw_domain"], helper_body["raw_domain"]),
        Err(_) => assert_eq!(ordinary_body["error"], helper_body["error"]),
    }
    let helper_while_result_live = state(&policy.policy.0);
    let caller_while_helper_live = caller_state(caller);
    assert_eq!(caller_before, caller_while_helper_live);
    drop(result);
    let helper_after_drop = state(&policy.policy.0);
    assert_eq!(helper_after_drop["live"], 0);
    let caller_after = caller_state(caller);
    assert_eq!(caller_before, caller_after);
    let bound_after = bound.capture_baseline_snapshot(plan);
    assert_eq!(bound_before, bound_after);
    admission_tests::observe(plan, bound, caller);
    json!({"literal_bound_work":bound.work(),
        "literal_resolved_work":resolved_work,
        "literal_lattice_work":lattice_work,"bound_before":bound_before,
        "ordinary":{"local_allowance":ALLOWANCE,"result":ordinary_body,
            "caller_before":caller_before,"caller_while_result_live":caller_while_ordinary_live,
            "caller_after_result_drop":caller_after_ordinary_drop},
        "existing_internal_metered_helper":{"local_before":local_before,
            "local_after":{"remaining":remaining,"lookup":lookup},
            "canonical":canonical,"additional":additional,"accepted_work":policy.policy.0.borrow().forwarded,
            "result":helper_body,"events":policy.events,"ledger_before":helper_before,
            "ledger_while_result_live":helper_while_result_live,"ledger_after_result_drop":helper_after_drop,
            "caller_while_result_live":caller_while_helper_live,"caller_after_result_drop":caller_after},
        "bound_after":bound_after})
}

pub(super) fn observe(
    input: &AudioOwnerInput<'_>,
    retained: &AudioOwnerRetainedInput<'_>,
    caller: Option<&Rc<RefCell<PolicyState>>>,
) {
    if !COLLECT.with(|slot| slot.borrow().is_some()) {
        return;
    }
    let origin = ORIGIN.get().expect("actual owning handle origin scope");
    let plan = input.context().plan;
    let input_before = snapshot::input(input);
    let retained_before = retained.capture_baseline_snapshot(plan);
    let cached_raw_before = raw_value(plan, retained.domain());
    let construction = reconstruct(plan, retained.bound(), caller);
    let input_after = snapshot::input(input);
    let retained_after = retained.capture_baseline_snapshot(plan);
    let cached_raw_after = raw_value(plan, retained.domain());
    assert_eq!(input_before, input_after);
    assert_eq!(retained_before, retained_after);
    assert_eq!(cached_raw_before, cached_raw_after);
    let value = json!({"index":COLLECT.with(|slot|slot.borrow().as_ref().unwrap().len()),
        "fixture":FIXTURE.with(|slot|slot.borrow().as_ref().expect("actual checked fixture scope").clone()),
        "origin":origin,"input_before":input_before,"retained_before":retained_before,
        "cached_raw_before":cached_raw_before,"cached_raw_after":cached_raw_after,
        "literal_retained_work":retained.work(),"construction":construction,
        "input_after":input_after,"retained_after":retained_after});
    COLLECT.with(|slot| slot.borrow_mut().as_mut().unwrap().push(value));
}

fn processing_cases() -> Value {
    let mut cases = Vec::new();
    let mut root_bounds = 0;
    let mut domain_bounds = 0;
    let mut signal_bounds = 0;
    snapshot::processing_query_baseline::visit_cases(|parent, descriptor| {
        if descriptor["fixture_source"]["document"].is_null() {
            cases.push(json!({"fixture":descriptor,"result":"NotMeasured",
                "reason":"Existing storage-only constructor supplies no full document; no substitute invented"}));
            return;
        }
        let plan = parent.plan();
        let samples = AudioSample(descriptor["window"]["start"].as_i64().unwrap())
            ..AudioSample(descriptor["window"]["end"].as_i64().unwrap());
        let query = match parent {
            Parent::Root(plan) => plan.audio_processing(samples.clone(), Default::default()),
            Parent::Domain(domain) => domain.processing(samples.clone(), Default::default()),
        };
        let processing = match &query {
            Ok(query) => {
                let mut bounds = Vec::new();
                for (index, span) in query.spans.iter().enumerate() {
                    if let AudioSignalContent::Bound(bound) = &span.content {
                        match parent {
                            Parent::Root(_) => root_bounds += 1,
                            Parent::Domain(_) => domain_bounds += 1,
                        }
                        bounds.push(
                            json!({"span_index":index,"construction":reconstruct(plan,bound,None)}),
                        );
                    }
                }
                json!({"result":"Ok","query":snapshot::processing_query_baseline::query_snapshot(plan,query),
                    "bound_constructions_while_actual_query_live":bounds})
            }
            Err(error) => json!({"result":"Error","error":snapshot::error_json(error)}),
        };
        // This is the actual public root Signal route, not a nominal retained clock.
        let signal_case = if let Parent::Root(_) = parent {
            let signal = plan.audio_signal();
            let end = signal.sample_count().unwrap();
            let result = signal.query(crate::SignalSample(0)..end, Default::default());
            match &result {
                Ok(query) => {
                    let mut bounds = Vec::new();
                    for (index, span) in query.spans.iter().enumerate() {
                        if let AudioSignalContent::Bound(bound) = &span.content {
                            signal_bounds += 1;
                            bounds.push(json!({"span_index":index,"construction":reconstruct(plan,bound,None)}));
                        }
                    }
                    // Public serialization skips work and some private Bound fields.
                    json!({"result":"Ok","signal":signal.probe_baseline_snapshot(plan),
                        "query":query,"literal_query_work":query.work,
                        "bound_constructions_while_actual_query_live":bounds})
                }
                Err(error) => {
                    json!({"result":"Error","signal":signal.probe_baseline_snapshot(plan),
                    "error":snapshot::error_json(error)})
                }
            }
        } else {
            json!({"result":"NotApplicable","reason":"Root Signal measured once per actual Root parent"})
        };
        cases.push(json!({"fixture":descriptor,"processing_request":{"samples":samples,
            "limits":limits_value(Default::default())},"processing":processing,"root_signal":signal_case}));
    });
    json!({"cases":cases,"actual_bound_counts":{"RootProcessing":root_bounds,
        "DomainProcessing":domain_bounds,"RootSignal":signal_bounds}})
}

fn matrix_raw() -> Value {
    let session = Session::new();
    let retained = matrix();
    let supplement = supplement::supplement_matrix();
    let coincident = overlap_tests::diagnostic::ordinary_case();
    let constructions = session.finish();
    assert_eq!(
        constructions.len(),
        54,
        "54 genuine retained Some owning scopes"
    );
    for origin in [
        "ordinary_probe",
        "actual_admitted_probe_unmetered",
        "actual_admitted_probe_metered",
        "actual_admitted_probe_selected_then_shared",
    ] {
        assert!(constructions.iter().any(|value| value["origin"] == origin));
    }
    json!({"schema":"old-raw-domain-query-baseline-v1",
        "measurement":"fresh actual ordinary AudioBound::raw_domain plus separately labelled already-existing internal metered helper",
        "retained_setup_corpora":{"baseline":retained,"owning_supplement":supplement,"coincident":coincident},
        "retained_some_constructions":constructions,"direct_processing":processing_cases(),
        "scope_limits":{"Empty":"No fabricated empty-support Bound; only actual checked results are captured",
            "ordinary_partial_work":"not exposed","resource_events":"existing internal helper, not ordinary allocation admission",
            "recursive_preparation":"not measured","native":"independent host witness required"}})
}

fn strip_envelope(mut value: Value) -> Value {
    for field in [
        "variant",
        "source_sha256",
        "published_retained_fade_checkpoint",
    ] {
        value.as_object_mut().unwrap().remove(field);
    }
    value
}
#[test]
#[ignore = "manual OLD producer only; explicit fresh /tmp output and pinned source/checkpoint required"]
fn capture_old_raw_domain_query_baseline_v1() {
    let target =
        std::env::var("DEADPAN_RAW_DOMAIN_OUTPUT").expect("explicit fresh output required");
    let path = Path::new(&target);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_RAW_DOMAIN_VARIANT").expect("explicit variant required");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_RAW_DOMAIN_SOURCE_SHA256")
        .expect("complete source manifest identity required");
    assert!(source.len() == 64 && source.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let checkpoint = std::env::var("DEADPAN_RAW_DOMAIN_PUBLISHED_CHECKPOINT")
        .expect("published checkpoint required");
    assert_eq!(checkpoint, CHECKPOINT);
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .expect("output must be new");
    let mut value = matrix_raw();
    value["variant"] = json!(variant);
    value["source_sha256"] = json!(source);
    value["published_retained_fade_checkpoint"] = json!(checkpoint);
    serde_json::to_writer_pretty(&mut file, &value).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}

#[test]
#[ignore = "external full-body regression; supply separately captured immutable OLD body"]
fn raw_domain_matches_explicit_external_old_v1() {
    let path = std::env::var("DEADPAN_RAW_DOMAIN_EXPECTED")
        .expect("independent immutable OLD capture required");
    let old: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(matrix_raw(), strip_envelope(old));
}

#[path = "raw_domain_admission_tests.rs"]
mod admission_tests;
