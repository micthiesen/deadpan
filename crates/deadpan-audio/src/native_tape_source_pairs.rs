//! Complete active observations paired with the independent literal source replay.
use super::*;
use crate::stages::routed::projected_tape_source_trace::{self as trace, Event, Phase};
use deadpan_plan::SignalPolicyView;

const MAX_EVENTS: usize = 512;
const FROZEN: &[u8] =
    include_bytes!("../tests/fixtures/native-tape-source-baseline-v1/corpus.json");

#[derive(Default)]
struct Batch {
    attempted: usize,
    events: Vec<Value>,
    queries: usize,
    current: Option<QueryIdentity>,
    request: Option<usize>,
}
struct QueryIdentity {
    ordinal: usize,
    pointer: usize,
    len: usize,
    work: [usize; 2],
    before_handoff: bool,
}
thread_local! {
    static BATCH: RefCell<Option<Batch>> = const { RefCell::new(None) };
}
fn accepted(control: WorkControl<'_>) -> [usize; 2] {
    let work = control.work.borrow();
    [work.plan_work, work.cache_plan_work]
}
fn view(value: SignalPolicyView<'_>) -> Value {
    json!({"suppressed":value.suppressed.iter().map(|r|[r.start.0,r.end.0]).collect::<Vec<_>>(),
        "suppressed_len":value.suppressed.len(),"contents":value.contents,"contents_len":value.contents.len(),
        "lookup":value.lookup,"work":value.work,
        "capacities":"slice view exposes no capacity; true final capacity is captured at PrepareMask"})
}
fn record(event: Event<'_, '_>, renderer: &StageAudio, control: WorkControl<'_>) {
    BATCH.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(batch) = slot.as_mut() else { return };
        batch.attempted += 1;
        assert!(batch.attempted <= MAX_EVENTS, "no J event truncation is accepted");
        let request = control.work as *const RefCell<ReadWork> as usize;
        if let Some(prior) = batch.request {
            assert_eq!(prior, request, "one receipts batch must identify one actual ReadWork");
        } else {
            batch.request = Some(request);
        }
        let actual_work = accepted(control);
        let detail = match event {
            Event::Query(query) => {
                let borrowed = query.view();
                assert_eq!(query.work(), query.canonical_units().checked_add(query.additional_units()).unwrap());
                assert_eq!(borrowed.work,query.work());
                assert!(query.was_forwarded());
                batch.queries += 1;
                batch.current = Some(QueryIdentity {
                    ordinal:batch.queries,pointer:borrowed.suppressed.as_ptr() as usize,
                    len:borrowed.suppressed.len(),work:actual_work,before_handoff:false,
                });
                json!({"event":"Query","query_ordinal":batch.queries,"view":view(borrowed),
                    "canonical":query.canonical_units(),"additional":query.additional_units(),
                    "total":query.work(),"was_forwarded":query.was_forwarded(),
                    "equation":"total == canonical + additional == borrowed view work"})
            }
            Event::Use {phase,view:borrowed} => {
                let current=batch.current.as_mut().expect("real Query event precedes native use");
                assert_eq!(current.pointer,borrowed.suppressed.as_ptr() as usize);
                assert_eq!(current.len,borrowed.suppressed.len());
                assert_eq!(current.work,actual_work,"preflight/check/zeroing must not debit the forwarded query twice");
                let phase=match phase {
                    Phase::ContentPreflight {completed}=>{
                        assert!(completed <= borrowed.contents.len());
                        json!({"kind":"ContentPreflight","completed":completed})
                    }
                    Phase::ControlChecked=>json!({"kind":"ControlChecked"}),
                    Phase::BeforeHandoff=>{
                        current.before_handoff=true;
                        json!({"kind":"BeforeHandoff"})
                    }
                };
                json!({"event":"Use","query_ordinal":current.ordinal,"phase":phase,"view":view(borrowed),
                    "same_actual_source_pointer_and_len":true,"accepted_work_unchanged_since_Query":true})
            }
            Event::PrepareMask {capacity,lease_frames} => {
                let current=batch.current.take().expect("actual source view precedes J handoff");
                assert!(current.before_handoff);
                assert!(capacity >= current.len);
                assert_eq!(capacity==0,lease_frames.is_none());
                json!({"event":"PrepareMask","query_ordinal":current.ordinal,
                    "actual_capacity":capacity,"actual_source_len":current.len,"actual_lease_frames":lease_frames,
                    "source_view_was_live_before_handoff":true,
                    "scope":"entry to actual native prepare; success/commit is not implied by this event"})
            }
        };
        batch.events.push(json!({"detail":detail,"accepted_work":actual_work,
            "sound_context_frames":control.work.borrow().sound_context_frames,
            "account_live_frames":renderer.owned_suppression.live_frames(),
            "active_frames":renderer.active_frames,
            "projected_resident_frames":control.work.borrow().projected_resident_frames,
            "projected_disk_bytes":control.work.borrow().projected_disk_bytes}));
    });
}
pub(super) fn take_case_trace() -> Option<Value> {
    BATCH.with(|slot| {
        let mut slot = slot.borrow_mut();
        let batch = slot.as_mut()?;
        let batch = std::mem::take(batch);
        Some(
            json!({"schema":"native-tape-source-J-events-v1","attempted":batch.attempted,
            "dropped":0,"events":batch.events,"completed_Query_events":batch.queries,
            "query_without_PrepareMask":batch.current.map(|v|v.ordinal),
            "source_addresses":"compared in-process only; no pointer value serialized",
            "error_constructor_work":"unexposed; never inferred from remaining work"}),
        )
    })
}
struct Recording;
impl Recording {
    fn begin() -> Self {
        BATCH.with(|slot| {
            assert!(slot.borrow().is_none());
            *slot.borrow_mut() = Some(Batch::default());
        });
        trace::set_observer(Some(Box::new(record)));
        Self
    }
}
impl Drop for Recording {
    fn drop(&mut self) {
        trace::set_observer(None);
        BATCH.with(|slot| *slot.borrow_mut() = None);
    }
}
fn paired_result(path: &str, old: &Value, current: &Value, comparisons: &mut Vec<Value>) {
    let old_status = old["status"].as_str().unwrap();
    let current_status = current["status"].as_str().unwrap();
    if old_status == "Ok" && current_status == "Ok" {
        assert_eq!(
            old.as_object().unwrap().keys().collect::<Vec<_>>(),
            current.as_object().unwrap().keys().collect::<Vec<_>>(),
            "result fields at {path}"
        );
        for (key, value) in old.as_object().unwrap() {
            match key.as_str() {
                "sample_storage" => {}
                "suppressed" => {
                    for key in [
                        "values",
                        "len",
                        "first",
                        "last",
                        "reverse",
                        "iter_size_hint",
                    ] {
                        assert_eq!(
                            value[key], current["suppressed"][key],
                            "mask values at {path}/{key}"
                        );
                    }
                }
                _ => assert_eq!(
                    value, &current[key],
                    "complete successful payload field at {path}/{key}"
                ),
            }
        }
    }
    comparisons.push(
        json!({"path":path,"legacy_status":old_status,"active_status":current_status,
        "payload_compared_exactly":old_status=="Ok" && current_status=="Ok",
        "literal_results_equal":old==current,"legacy":old,"active":current}),
    );
}
fn visit_results(path: &str, old: &Value, active_root: &Value, comparisons: &mut Vec<Value>) {
    match old {
        Value::Object(fields) => {
            for (key, value) in fields {
                let path = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                if value.get("status").is_some_and(Value::is_string) {
                    let current = active_root
                        .pointer(&path)
                        .expect("corresponding actual result exists");
                    assert!(
                        current.get("status").is_some_and(Value::is_string),
                        "missing actual result at {path}"
                    );
                    paired_result(&path, value, current, comparisons);
                } else {
                    visit_results(&path, value, active_root, comparisons);
                }
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                visit_results(&format!("{path}/{index}"), value, active_root, comparisons);
            }
        }
        _ => {}
    }
}
fn validate_lifetimes(value: &Value) {
    match value {
        Value::Object(fields) => {
            if let Some(result) = fields.get("result") {
                for field in [
                    "result_after_provider_controller_drop",
                    "result_after_work_drop",
                    "result_after_controller_drop",
                ] {
                    if let Some(held) = fields.get(field) {
                        assert_eq!(result, held, "actual held result changed at {field}");
                    }
                }
            }
            for value in fields.values() {
                validate_lifetimes(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                validate_lifetimes(value)
            }
        }
        _ => {}
    }
}
fn paired_body() -> Value {
    let frozen: Value = serde_json::from_slice(FROZEN).unwrap();
    let legacy = capture_cases();
    assert_eq!(
        legacy, frozen,
        "separate J source legacy remains the entire exact before38 body"
    );
    let _recording = Recording::begin();
    let mut active = capture_cases_legacy();
    let tail = take_case_trace().expect("active capture enabled");
    active["schema"] = json!("native-tape-source-admitted-v1");
    active["scope"]["current"] = json!(
        "I admitted Tape source query; exact native final Vec custody; G public route destination remains active"
    );
    active["scope"]["source_policy"] = json!(
        "actual query inventory/final Vec admitted; graph operands/other ordinary preparation policies remain outside this unit"
    );
    active["scope"]["gaps"] = json!([
        "ordinary preparation preflights outside the selected routed source remain ordinary",
        "no constructor-local canonical/additional fields exposed on failed factories",
        "native capacity is observed at prepare entry; that event alone does not establish commit",
        "PCM/dependencies and whole preparation remain outside mask custody"
    ]);
    validate_lifetimes(&active);
    let mut comparisons = Vec::new();
    visit_results("", &legacy, &active, &mut comparisons);
    assert!(
        !comparisons.is_empty(),
        "every actual result must be visited"
    );
    json!({"schema":"native-tape-source-pairs-v1","published_I":CHECKPOINT,
        "ordinary_legacy":legacy,"active":active,"unassociated_active_tail":tail,
        "result_comparisons":comparisons,
        "scope":"complete raw lanes and literal resource/error differences retained; successful PCM/public values compare exactly"})
}
#[test]
fn admitted_tape_native_pairs_preserve_successful_payloads_and_literal_failures() {
    crate::stages::intrinsic_preflight_policy_request::with_legacy_intrinsic_preflight_policy(
        admitted_tape_native_pairs_preserve_successful_payloads_and_literal_failures_historical,
    );
}

fn admitted_tape_native_pairs_preserve_successful_payloads_and_literal_failures_historical() {
    let _ = paired_body();
}
#[test]
#[ignore = "manual fresh complete J native paired capture"]
fn capture_native_tape_source_pairs_v1() {
    crate::stages::intrinsic_preflight_policy_request::with_legacy_intrinsic_preflight_policy(
        capture_native_tape_source_pairs_v1_historical,
    );
}

fn capture_native_tape_source_pairs_v1_historical() {
    let path = std::env::var_os("DEADPAN_NATIVE_TAPE_SOURCE_PAIRS_OUTPUT")
        .expect("fresh absolute /tmp output");
    let path = Path::new(&path);
    assert!(path.is_absolute() && path.starts_with("/tmp"));
    let variant = std::env::var("DEADPAN_NATIVE_TAPE_SOURCE_PAIRS_VARIANT").unwrap();
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_NATIVE_TAPE_SOURCE_PAIRS_SOURCE_SHA256").unwrap();
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    assert_eq!(
        std::env::var("DEADPAN_NATIVE_TAPE_SOURCE_PAIRS_PUBLISHED_CHECKPOINT").unwrap(),
        CHECKPOINT
    );
    let mut body = paired_body();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    body["published_checkpoint"] = json!(CHECKPOINT);
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(&body).unwrap())
        .unwrap();
    file.write_all(b"\n").unwrap();
    file.flush().unwrap();
    file.sync_all().unwrap();
}
