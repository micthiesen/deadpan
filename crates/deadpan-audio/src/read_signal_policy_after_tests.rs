//! Actual per-action N native receipts; observation storage is not an admission.
use super::*;
use crate::stages::read_signal_policy_native_trace as native;
use native::{Fault, UsePoint};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    All,
    PlanFaults,
    CacheFaults,
    Residency,
}
struct Collection {
    mode: Mode,
    rows: Vec<Value>,
    provider: Option<Vec<Value>>,
    unscoped_provider: Vec<Value>,
    next: Fault,
}
thread_local! {
    static COLLECTION: RefCell<Option<Collection>> = const { RefCell::new(None) };
}
struct CollectionGuard;
impl Drop for CollectionGuard {
    fn drop(&mut self) {
        COLLECTION.with(|v| {
            v.borrow_mut().take();
        });
    }
}
fn collection(mode: Mode) -> CollectionGuard {
    original_request::flattened_work_trace::reset();
    COLLECTION.with(|v| {
        assert!(v.borrow().is_none());
        *v.borrow_mut() = Some(Collection {
            mode,
            rows: Vec::new(),
            provider: None,
            unscoped_provider: Vec::new(),
            next: Fault::None,
        });
    });
    CollectionGuard
}
pub(super) fn active() -> bool {
    COLLECTION.with(|v| v.borrow().is_some())
}
pub(super) struct Action {
    native: Option<native::CaptureGuard>,
    keys: Option<before_trace::CaptureGuard>,
    name: String,
    entry: &'static str,
}
pub(super) fn begin_action(name: &str, entry: &'static str) -> Option<Action> {
    let fault = COLLECTION.with(|v| {
        let mut v = v.borrow_mut();
        v.as_mut().map(|v| {
            assert!(v.provider.is_none());
            v.provider = Some(Vec::new());
            std::mem::replace(&mut v.next, Fault::None)
        })
    })?;
    let keys = before_trace::begin_keys();
    let native = native::begin(fault);
    Some(Action {
        native: Some(native),
        keys: Some(keys),
        name: name.to_owned(),
        entry,
    })
}
pub(super) fn provider_position() -> Option<Value> {
    if !active() {
        return None;
    }
    let value = json!({"observer":"N-native-AFTER-and-N-keys-only",
        "native":native::position(),"keys":before_trace::position(),
        "scope":"actual provider callback entry, not decoder I/O"});
    COLLECTION.with(|v| {
        let mut v = v.borrow_mut();
        let v = v.as_mut().unwrap();
        if let Some(rows) = v.provider.as_mut() {
            rows.push(json!({"ordinal":rows.len()+1,"position":value}));
        } else {
            v.unscoped_provider.push(value.clone());
        }
    });
    Some(value)
}
fn keys_end(action: &mut Action) -> Value {
    let guard = action.keys.take().expect("one keys take");
    let value = before_trace::take_keys();
    drop(guard);
    key_proof::valid_keys(&value);
    value
}
fn finish_action(mut action: Action, keys: Option<Value>) -> Value {
    let keys = keys.unwrap_or_else(|| keys_end(&mut action));
    let value = native::take();
    drop(action.native.take());
    valid(&value);
    COLLECTION.with(|v| {
        let mut v = v.borrow_mut();
        let v = v.as_mut().unwrap();
        let provider = v.provider.take().unwrap();
        let ordinal = v.rows.len() + 1;
        v.rows.push(
            json!({"ordinal":ordinal,"action":action.name,"entry":action.entry,
            "native":value,"keys":keys,"provider_entries":provider}),
        );
        json!({"schema":"N-AFTER-action-receipt-reference-v1","ordinal":ordinal,
            "existing_mask_ops":point_suppression::ops::trace::snapshot(),
            "existing_public_owner_work":point_suppression::work_trace::snapshot(),
            "counter_scope":"actual cumulative group counters; not N-only work attribution"})
    })
}
pub(super) fn end_action(action: Action) -> Value {
    finish_action(action, None)
}
fn receipts() -> Vec<Value> {
    COLLECTION.with(|v| {
        let mut v = v.borrow_mut();
        let v = v.as_mut().unwrap();
        assert!(v.provider.is_none());
        assert!(
            v.unscoped_provider.is_empty(),
            "unexpected callbacks outside actual action"
        );
        std::mem::take(&mut v.rows)
    })
}
fn next_fault(value: Fault) {
    COLLECTION.with(|v| {
        let mut v = v.borrow_mut();
        let v = v.as_mut().unwrap();
        assert!(v.provider.is_none());
        v.next = value;
    });
}
fn mask_snapshot(output: &Result<PublicOutput, StageAudioError>) -> Value {
    match output {
        Ok(PublicOutput::Definition(v)) => native::mask_snapshot(&v.suppressed),
        Ok(PublicOutput::Tape(v)) => native::mask_snapshot(&v.suppressed),
        Ok(PublicOutput::Routed(v)) => native::mask_snapshot(&v.suppressed),
        Err(_) => Value::Null,
    }
}
fn invoke_public(
    renderer: &mut StageAudio,
    source: &mut WitnessProvider<'_>,
    target: PublicTarget<'_, '_>,
    request: (i64, u32),
    cancelled: &AtomicBool,
) -> Result<PublicOutput, StageAudioError> {
    match target {
        PublicTarget::Definition(v) => renderer
            .read_definition(
                source,
                v,
                SignalSample(request.0),
                request.1,
                TIMEOUT,
                cancelled,
            )
            .map(PublicOutput::Definition),
        PublicTarget::Tape(v) => renderer
            .read_tape(
                source,
                v,
                SignalSample(request.0),
                request.1,
                TIMEOUT,
                cancelled,
            )
            .map(PublicOutput::Tape),
        PublicTarget::Routed(v) => renderer
            .read_routed_signal(
                source,
                v,
                SignalSample(request.0),
                request.1,
                TIMEOUT,
                cancelled,
            )
            .map(PublicOutput::Routed),
    }
}
pub(super) fn held_public(
    plan: &Arc<RenderPlan>,
    target: PublicTarget<'_, '_>,
    inner: NativeProvider,
    request: (i64, u32),
    name: &str,
) -> Value {
    let _ops = mask_observations();
    let (mut renderer, monitor) = super::renderer(plan, StageLimits::default(), None);
    let mut source = provider(inner, None, false);
    let before = input_value(target, plan);
    let controller_before = controller(&renderer);
    let monitor_before = super::monitor(&monitor);
    let event_count = source.events.len();
    let call_count = source.inner.calls;
    let mut capture = begin_action(name, "dedicated_single_read_lifetime").unwrap();
    let result = invoke_public(
        &mut renderer,
        &mut source,
        target,
        request,
        &AtomicBool::new(false),
    );
    let keys = keys_end(&mut capture);
    let input_after = input_value(target, plan);
    assert_eq!(before, input_after);
    let while_controller = mask_snapshot(&result);
    let mut row = json!({"action":name,"entry":"actual public method; dedicated single-read lifetime capture",
        "start":request.0,"frames":request.1,"input_before":before,"input_after":input_after,
        "controller_before":controller_before,"controller_result_live":controller(&renderer),
        "monitor_before":monitor_before,"monitor_after":super::monitor(&monitor),
        "provider":provider_value(&source,event_count,call_count),"result":output_value(&result)});
    drop(source);
    let after_provider = output_value(&result);
    let before_controller_drop = controller(&renderer);
    drop(renderer);
    let after_controller = output_value(&result);
    let mask_after_controller = mask_snapshot(&result);
    drop(result);
    let receipt = finish_action(capture, Some(keys));
    row["trace"] = receipt;
    json!({"action":row,"result_after_provider_drop":after_provider,
        "controller_before_drop":before_controller_drop,"result_after_controller_drop":after_controller,
        "mask_with_controller":while_controller,"mask_after_controller_drop":mask_after_controller,
        "retirement":"real account Drop rows in this one capture; no retained observer or post-drop dereference"})
}
pub(super) fn private_action(
    plan: &Arc<RenderPlan>,
    target: PrivateTarget<'_, '_>,
    inner: NativeProvider,
    name: &str,
    limits: StageLimits,
    options: PrivateOptions,
) -> Value {
    let _ops = mask_observations();
    let budget = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(options.cancelled);
    let (mut renderer, monitor) = super::renderer(plan, limits, None);
    let mut source = provider(inner, Some(&budget), options.refuse_provider);
    let control = WorkControl {
        work: &budget,
        cancelled: &cancelled,
        deadline: if options.expired {
            Instant::now() - Duration::from_secs(1)
        } else {
            Instant::now() + TIMEOUT
        },
    };
    let (first, first_row) =
        private_once(&mut renderer, &mut source, target, control, &monitor, name);
    let second = if options.repeat && first.is_ok() {
        Some(private_once(
            &mut renderer,
            &mut source,
            target,
            control,
            &monitor,
            "same_ReadWork_memo",
        ))
    } else {
        None
    };
    let same = match (&first, &second) {
        (Ok(a), Some((Ok(b), _))) => Some(a.same(b)),
        _ => None,
    };
    let second_value = match &second {
        Some((_, row)) => json!({"attempted":true,"action":row}),
        None => {
            json!({"attempted":false,"reason":if options.repeat {"first actual preparation failed"}else{"single-action manifest entry"}})
        }
    };
    let final_work = work(&budget);
    drop(source);
    drop(budget);
    let after_work = first.as_ref().ok().map(Prepared::value);
    let before_controller_drop = controller(&renderer);
    drop(renderer);
    let after_controller = first.as_ref().ok().map(Prepared::value);
    drop(second);
    drop(first);
    json!({"setup":"Ok","first":first_row,"second":second_value,"same_actual_prepared_arc":same,
        "final_work":final_work,"prepared_after_ReadWork_drop":after_work,
        "controller_before_drop":before_controller_drop,"prepared_after_controller_drop":after_controller,
        "account_after_controller_drop":"unexposed; no account handle retained",
        "memo_scope":"same actual control and cumulative accepted counters, no reset",
        "cancelled_at_end":cancelled.load(Ordering::Relaxed)})
}
fn n(v: &Value) -> u64 {
    v.as_u64().unwrap()
}
fn array<'a>(v: &'a Value, key: &str) -> &'a Vec<Value> {
    v[key].as_array().unwrap()
}
fn native_rows(v: &Value) -> &Vec<Value> {
    array(v, "native_rows")
}
fn valid(v: &Value) {
    assert_eq!(v["schema"], "read-signal-policy-native-v1");
    assert_eq!(v["invalid"], false);
    assert_eq!(v["overflow"], false);
    assert_eq!(v["dropped"], 0);
    for key in [
        "active_at_take",
        "pending_handoffs_at_take",
        "unreconciled_transfers",
    ] {
        assert_eq!(v[key], json!([]), "{key}");
    }
    assert!(
        array(v, "request_rows").len() <= 1,
        "one actual request per action capture"
    );
    for scope in array(v, "scopes") {
        assert_eq!(scope["request"], 1);
        assert_eq!(scope["factory_active"], false);
        assert!(!scope["exit"].is_null());
        assert_eq!(
            scope["entry"]["ledger"], scope["exit"]["ledger"],
            "actual scope cleanup"
        );
    }
    for row in native_rows(v) {
        assert_eq!(row["request"], 1);
        let before = &row["before"];
        let after = &row["after"];
        match row["kind"].as_str().unwrap() {
            "SpendResult" => {
                let lane = n(&before["lane"]) as usize;
                for i in 0..2 {
                    let expected = if i == lane && row["error"].is_null() {
                        n(&row["amount"])
                    } else {
                        0
                    };
                    assert_eq!(n(&after["work"][i]) - n(&before["work"][i]), expected);
                }
            }
            "ReserveResult" => {
                if row["error"].is_null() {
                    let frames = n(&row["layout"][0]).div_ceil(8);
                    assert_eq!(row["frames"], frames);
                    assert_eq!(n(&after["ledger"]) - n(&before["ledger"]), frames);
                } else {
                    assert_eq!(before["ledger"], after["ledger"]);
                }
            }
            "MakeRoomBefore" | "MakeRoomAfter" => {
                let frames = n(&row["layout"][0]).div_ceil(8);
                assert_eq!(row["frames"], frames);
                assert_eq!(row["resident"]["pending_additional"], frames);
                if row["kind"] == "MakeRoomAfter" {
                    assert_eq!(row["host_crosscheck"], true);
                    assert_eq!(before["ops"]["enabled"], true);
                    assert_eq!(after["ops"]["enabled"], true);
                    assert_eq!(before["ops"]["overflow"], false);
                    assert_eq!(after["ops"]["overflow"], false);
                    assert!(!before["ops"]["reset_epoch"].is_null());
                    assert_eq!(before["ops"]["reset_epoch"], after["ops"]["reset_epoch"]);
                    assert!(!before["host_epoch"].is_null());
                    assert_eq!(before["host_epoch"], after["host_epoch"]);
                    for lane in 0..2 {
                        let eviction =
                            n(&after["host_units"][lane]) - n(&before["host_units"][lane]);
                        let owned = n(&after["ops"]["lanes"][lane][7])
                            - n(&before["ops"]["lanes"][lane][7]);
                        assert_eq!(
                            n(&after["work"][lane]) - n(&before["work"][lane]),
                            eviction + owned
                        );
                        assert_eq!(row["host_delta"][lane], eviction + owned);
                        for phase in 0..7 {
                            assert_eq!(
                                before["ops"]["lanes"][lane][phase],
                                after["ops"]["lanes"][lane][phase]
                            );
                        }
                    }
                }
            }
            "ReleaseAfter" => assert_eq!(
                n(&before["ledger"]) - n(&after["ledger"]),
                n(&row["frames"])
            ),
            "MergeAfter" => {
                assert_eq!(before["ledger"], after["ledger"]);
                assert_eq!(
                    n(&row["merge_frames"][0]) + n(&row["merge_frames"][1]),
                    n(&row["merge_frames"][2])
                );
                assert_eq!(row["merge_frames"][3], 0);
            }
            "SpendAttempt" | "ReserveAttempt" | "ReleaseBefore" | "MergeBefore" => {}
            other => panic!("unknown actual native row {other}"),
        }
    }
    for factory in array(v, "factory_receipts") {
        assert_eq!(factory["factory_equation"], true);
        let ticket = &factory["ticket"];
        for lane in 0..2 {
            let lane_name = if lane == 0 { "Plan" } else { "Cache" };
            let spent: u64 = native_rows(v)
                .iter()
                .filter(|r| {
                    r["ticket"] == *ticket
                        && r["eligible"] == true
                        && r["lane"] == lane_name
                        && r["kind"] == "SpendResult"
                        && r["error"].is_null()
                })
                .map(|r| n(&r["amount"]))
                .sum();
            let host: u64 = native_rows(v)
                .iter()
                .filter(|r| {
                    r["ticket"] == *ticket && r["eligible"] == true && r["kind"] == "MakeRoomAfter"
                })
                .map(|r| n(&r["host_delta"][lane]))
                .sum();
            assert_eq!(factory["accepted_spend"][lane], spent);
            assert_eq!(factory["accepted_host"][lane], host);
            assert_eq!(
                n(&factory["exit"]["work"][lane]) - n(&factory["entry"]["work"][lane]),
                spent + host
            );
        }
        let result = &factory["result"];
        if result["status"] == "Ok" && result["mode"] == "Admitted" {
            assert_eq!(result["forwarded"], true);
            assert_eq!(factory["forwarding_equation"], true);
            let total = n(&result["canonical"]) + n(&result["additional"]);
            assert_eq!(result["total"], total);
            assert_eq!(result["view"]["work"], total);
            assert_eq!(
                n(&factory["accepted_spend"][0]) + n(&factory["accepted_spend"][1]),
                total
            );
            let uses = array(v, "use_rows")
                .iter()
                .filter(|r| r["ticket"] == *ticket && r["value"].get("view").is_some())
                .collect::<Vec<_>>();
            assert!(!uses.is_empty());
            assert_eq!(
                uses[0]["boundary"]["work"], factory["exit"]["work"],
                "no old manual debit after forwarded factory"
            );
            for row in uses {
                assert_eq!(row["value"]["view"], result["view"]);
                if n(&result["view"]["suppressed_len"]) + n(&result["view"]["contents_len"]) > 0 {
                    assert_eq!(row["value"]["same_local_view_identity"], true);
                } else {
                    assert!(row["value"]["same_local_view_identity"].is_null());
                }
            }
        } else if result["status"] == "Err" {
            assert_eq!(result["query_local_work"], "unexposed failed query");
            for field in ["canonical", "additional", "total"] {
                assert!(result.get(field).is_none());
            }
        }
    }
    for h in array(v, "handoff_receipts") {
        if h["success"] != true {
            continue;
        }
        assert_eq!(h["result"]["status"], "Ok");
        assert_eq!(h["capacity_equal"], true);
        let cap = n(&h["prepare_capacity"]);
        let mask = &h["result"]["mask"];
        assert_eq!(mask["mask_capacity"], cap);
        for lane in 0..2 {
            let ops = (0..8)
                .map(|phase| {
                    n(&h["exit"]["ops"]["lanes"][lane][phase])
                        - n(&h["entry"]["ops"]["lanes"][lane][phase])
                })
                .sum::<u64>();
            let eviction = n(&h["exit"]["host_units"][lane]) - n(&h["entry"]["host_units"][lane]);
            assert_eq!(
                n(&h["exit"]["work"][lane]) - n(&h["entry"]["work"][lane]),
                ops + eviction,
                "handoff work is actual native-mask/host work, separate from factory canonical"
            );
        }
        if cap == 0 {
            assert!(h["lease_frames"].is_null());
            assert!(h["pre_exclusive"].is_null());
            assert_eq!(mask["mask_len"], 0);
            assert!(mask["account_live_frames"].is_null());
            assert!(mask["charge_frames"].is_null());
        } else {
            assert_eq!(h["same_pointer_len"], true);
            assert_eq!(h["source_identity"], true);
            assert_eq!(mask["kind"], "Owned");
            assert!(n(&h["pre_exclusive"]["ledger"]) >= n(&h["lease_frames"]));
            for lane in 0..2 {
                for phase in [1, 2, 3, 4, 5] {
                    assert_eq!(
                        h["entry"]["ops"]["lanes"][lane][phase],
                        h["pre_exclusive"]["ops"]["lanes"][lane][phase],
                        "no new scan/map/zero/mark/vector-copy work before exact custody"
                    );
                }
            }
            let frames = std::alloc::Layout::array::<Range<SignalSample>>(cap as usize)
                .unwrap()
                .size()
                .div_ceil(8) as u64;
            assert_eq!(h["vector_frames"], frames);
            assert_eq!(h["lease_frames"], frames);
            let a = n(&h["pre_exclusive_ordinal"]);
            let b = n(&h["finish_ordinal"]);
            let inside = native_rows(v)
                .iter()
                .filter(|r| {
                    r["request"] == h["request"] && n(&r["ordinal"]) > a && n(&r["ordinal"]) < b
                })
                .collect::<Vec<_>>();
            assert!(inside.iter().all(|r| matches!(
                r["kind"].as_str(),
                Some("ReleaseBefore" | "ReleaseAfter" | "MergeBefore" | "MergeAfter")
            )));
            let released: u64 = inside
                .iter()
                .filter(|r| r["kind"] == "ReleaseAfter")
                .map(|r| n(&r["frames"]))
                .sum();
            assert_eq!(
                n(&h["pre_exclusive"]["ledger"]) - frames - released,
                n(&h["exit"]["ledger"])
            );
            assert_eq!(
                n(&mask["account_live_frames"]),
                n(&h["account_before"]["live_frames"]) + frames
            );
        }
    }
    for r in array(v, "resident_rows") {
        assert_eq!(r["request"], 1);
        let total = [
            "cached",
            "active",
            "projected",
            "sound_context",
            "owned_suppression",
            "additional",
        ]
        .iter()
        .map(|k| n(&r[*k]))
        .sum::<u64>();
        assert_eq!(r["sum_with_pending"], total);
    }
}
fn action_receipt<'a>(action: &Value, rows: &'a [Value]) -> &'a Value {
    let id = n(&action["trace"]["ordinal"]);
    let row = rows.iter().find(|r| r["ordinal"] == id).unwrap();
    assert_eq!(row["action"], action["action"]);
    assert_eq!(
        row["provider_entries"].as_array().unwrap().len() as u64,
        n(&action["provider"]["events_after"]) - n(&action["provider"]["events_before"])
    );
    row
}
fn visit_actions<'a>(v: &'a Value, p: String, out: &mut Vec<(String, &'a Value)>) {
    if v["action"].is_string()
        && v["entry"].is_string()
        && v.get("result").is_some()
        && v.get("input_before").is_some()
    {
        out.push((p.clone(), v));
    }
    match v {
        Value::Object(v) => {
            for (k, c) in v {
                visit_actions(
                    c,
                    format!("{p}/{}", k.replace('~', "~0").replace('/', "~1")),
                    out,
                );
            }
        }
        Value::Array(v) => {
            for (i, c) in v.iter().enumerate() {
                visit_actions(c, format!("{p}/{i}"), out);
            }
        }
        _ => {}
    }
}
fn call_map(
    ordinary: &Value,
    keys: &Value,
    current: &Value,
    allow_prefix: bool,
) -> std::collections::BTreeMap<u64, u64> {
    let old = array(ordinary, "calls");
    let key = array(keys, "calls");
    let now = array(current, "calls");
    if allow_prefix {
        assert!(key.len() <= old.len());
    } else {
        assert_eq!(old.len(), key.len());
    }
    assert_eq!(key.len(), now.len());
    let mut result = std::collections::BTreeMap::new();
    for (i, (k, native)) in key.iter().zip(now).enumerate() {
        let o = &old[i];
        for f in [
            "ordinal",
            "caller",
            "parent",
            "parent_selected_ticket",
            "start",
            "frames",
            "depth",
            "input",
        ] {
            assert_eq!(o[f], k[f], "ordinary/current key caller {f}");
        }
        for f in [
            "ordinal",
            "caller",
            "parent",
            "parent_selected_ticket",
            "start",
            "frames",
            "depth",
        ] {
            assert_eq!(k[f], native[f], "keys/native caller {f}");
        }
        if k["input"] == "Mix" {
            assert_eq!(o["producer_entered"], false);
            assert!(
                array(current, "scopes")
                    .iter()
                    .all(|scope| scope["ticket"] != native["ordinal"])
            );
        }
        assert!(
            result
                .insert(n(&o["ordinal"]), n(&native["ordinal"]))
                .is_none()
        );
    }
    result
}
fn compare_policies(ordinary: &Value, receipt: &Value, allow_prefix: bool) {
    let native = &receipt["native"];
    let map = call_map(&ordinary["trace"], &receipt["keys"], native, allow_prefix);
    let policies = array(&ordinary["trace"], "rows")
        .iter()
        .filter(|r| r["phase"] == "policy_query" && r["value"]["status"] == "Ok")
        .collect::<Vec<_>>();
    let current = array(native, "factory_receipts")
        .iter()
        .filter(|r| r["result"]["status"] == "Ok")
        .collect::<Vec<_>>();
    if allow_prefix {
        assert!(current.len() <= policies.len());
    } else {
        assert_eq!(policies.len(), current.len());
    }
    for (old, new) in policies.iter().zip(current) {
        assert_eq!(map.get(&n(&old["ticket"])), Some(&n(&new["ticket"])));
        let scope = &native["scopes"][n(&new["scope"]) as usize - 1];
        let call = &ordinary["trace"]["calls"][n(&old["ticket"]) as usize - 1];
        assert_eq!(scope["tag"], call["input"]);
        assert_eq!(scope["start"], call["start"]);
        assert_eq!(
            scope["end"].as_i64().unwrap() - scope["start"].as_i64().unwrap(),
            n(&call["frames"]) as i64
        );
        assert_eq!(scope["depth"], call["depth"]);
        let a = &old["value"]["policy"];
        let b = &new["result"];
        assert_eq!(b["mode"], "Admitted");
        for key in ["suppressed", "contents", "lookup"] {
            assert_eq!(a[key], b["view"][key]);
        }
        assert_eq!(a["work"], b["canonical"]);
    }
}
fn capture_all() -> Value {
    let guard = collection(Mode::All);
    let fixture = match super::super::family() {
        Ok(v) => v,
        Err(e) => setup_error("actual_registered_fixture", e.as_ref()),
    };
    let rows = receipts();
    drop(guard);
    json!({"schema":"read-signal-policy-AFTER-fixture-v1","fixture":fixture,"receipts":rows})
}
fn frozen() -> Value {
    serde_json::from_slice(include_bytes!(
        "../tests/fixtures/read-signal-policy-baseline-v1/corpus.json"
    ))
    .unwrap()
}
fn paired() -> Value {
    let ordinary = crate::stages::read_signal_policy_request::with_legacy_read_signal_policy(
        super::capture_body,
    );
    let legacy_proof = key_replay::assert_relation(&frozen(), &ordinary);
    let current = capture_all();
    let mut before = Vec::new();
    visit_actions(&ordinary["actual"], String::new(), &mut before);
    let mut after = Vec::new();
    visit_actions(&current["fixture"], String::new(), &mut after);
    assert_eq!(before.len(), 28);
    assert_eq!(before.len(), after.len());
    assert_eq!(array(&current, "receipts").len(), 28);
    let mut joined = Vec::new();
    let mut lanes = [false; 2];
    for ((p, a), (q, b)) in before.iter().zip(after) {
        assert_eq!(p, &q);
        assert_eq!(a["action"], b["action"]);
        assert_eq!(a["input_before"], b["input_before"]);
        assert_eq!(b["input_before"], b["input_after"]);
        for field in ["start", "frames", "depth"] {
            assert_eq!(a.get(field), b.get(field));
        }
        let receipt = action_receipt(b, array(&current, "receipts"));
        let quota = p == "/controls/literal_old2048/measured";
        if !quota {
            assert_eq!(
                a["result"], b["result"],
                "default output/capacity/error priority {p}"
            );
            compare_policies(a, receipt, false);
            key_proof::assert_keys_match(&a["trace"], &receipt["keys"]);
        } else {
            if b["result"]["status"] == "Ok" {
                assert_eq!(a["result"], b["result"]);
            } else {
                assert_eq!(b["result"]["error"]["kind"], "Limit");
                assert_eq!(
                    b["result"]["error"]["typed_payload"]["reason"],
                    "resident PCM or stage cache"
                );
            }
            compare_policies(a, receipt, true);
        }
        for row in native_rows(&receipt["native"]) {
            if row["kind"] == "SpendResult"
                && row["eligible"] == true
                && row["error"].is_null()
                && n(&row["amount"]) > 0
            {
                lanes[if row["lane"] == "Plan" { 0 } else { 1 }] = true;
            }
        }
        joined.push(
            json!({"path":p,"receipt":receipt["ordinal"],"ordinary":a["result"]["status"],
            "current":b["result"]["status"],"literal_old2048_not_an_equivalence_cap":quota}),
        );
    }
    assert_eq!(
        lanes,
        [true, true],
        "positive actual selected Plan and Cache work"
    );
    assert_eq!(
        ordinary["actual"]["projection_before"],
        current["fixture"]["projection_before"]
    );
    assert_eq!(
        current["fixture"]["projection_before"],
        current["fixture"]["projection_after"]
    );
    let refs = key_proof::current_references(&current["fixture"], array(&current, "receipts"));
    key_proof::mutations_rejected(array(&current, "receipts"));
    let overlap = nested_lifetime(array(&current, "receipts"));
    let life = dedicated_lifetime(&current["fixture"], array(&current, "receipts"));
    let warm = warm_shell(array(&current, "receipts"));
    json!({"ordinary":ordinary,"legacy_key_relation":legacy_proof,"current":current,"joined":joined,
        "current_key_references":refs,"nested_lifetime":overlap,"dedicated_lifetime":life,"warm_disposal":warm})
}
fn focused_public(
    plan: &Arc<RenderPlan>,
    definition: &AudioDefinition<'_>,
    inner: NativeProvider,
    f: Fault,
    name: &str,
) -> Value {
    let _ops = mask_observations();
    let (mut renderer, monitor) = super::renderer(plan, StageLimits::default(), None);
    let mut source = provider(inner, None, false);
    next_fault(f);
    let action = public_once(
        &mut renderer,
        &mut source,
        PublicTarget::Definition(definition),
        (0, 256),
        name,
        &monitor,
        &AtomicBool::new(false),
    );
    let mut rows = receipts();
    assert_eq!(rows.len(), 1);
    json!({"action":action,"receipt":rows.remove(0),"controller_after":controller(&renderer)})
}
fn focused_cache(
    plan: &Arc<RenderPlan>,
    definition: &AudioDefinition<'_>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
    f: Fault,
) -> SetupResult<Value> {
    let _ops = mask_observations();
    let directory = tempfile::tempdir()?;
    let cache = PcmCache::at(directory.path())?;
    let (mut first, monitor) = super::renderer(plan, StageLimits::default(), Some(cache.clone()));
    let mut source = provider(reopen()?, None, false);
    let warmup = public_once(
        &mut first,
        &mut source,
        PublicTarget::Definition(definition),
        (0, 256),
        "focused_cache_population",
        &monitor,
        &AtomicBool::new(false),
    );
    assert_eq!(warmup["result"]["status"], "Ok");
    let mut warm_receipts = receipts();
    assert_eq!(warm_receipts.len(), 1);
    drop(source);
    drop(first);
    let (mut renderer, monitor) =
        super::renderer(plan, StageLimits::default(), Some(cache.clone()));
    let mut source = provider(reopen()?, None, false);
    next_fault(f);
    let action = public_once(
        &mut renderer,
        &mut source,
        PublicTarget::Definition(definition),
        (0, 256),
        "focused_cache_fresh_renderer",
        &monitor,
        &AtomicBool::new(false),
    );
    let mut rows = receipts();
    assert_eq!(rows.len(), 1);
    Ok(
        json!({"warmup":warmup,"warmup_receipt":warm_receipts.remove(0),"action":action,
        "receipt":rows.remove(0),"cache_after":entries(&cache),"controller_after":controller(&renderer)}),
    )
}
fn accepted(v: &Value, before: Option<u64>) -> Vec<Value> {
    native_rows(v).iter().filter(|r|before.is_none_or(|n|self::n(&r["ordinal"])<n) &&
        matches!(r["kind"].as_str(),Some("SpendResult"|"ReserveResult")) && r["error"].is_null()).map(|r|
        json!({"kind":r["kind"],"attempt":r["attempt"],"eligible":r["eligible"],"ticket":r["ticket"],
            "cause":r["cause"],"request":r["request"],"lane":r["lane"],"amount":r["amount"],
            "layout":r["layout"],"frames":r["frames"],"work_before":r["before"]["work"],
            "work_after":r["after"]["work"],"ledger_before":r["before"]["ledger"],"ledger_after":r["after"]["ledger"]})).collect()
}
fn refusal(baseline: &Value, actual: &Value, target: &Value) {
    let b = &baseline["receipt"]["native"];
    let a = &actual["receipt"]["native"];
    assert_eq!(actual["action"]["result"]["status"], "Error");
    let injected = native_rows(a)
        .iter()
        .filter(|r| r["actual_injected_refusal"] == true)
        .collect::<Vec<_>>();
    assert_eq!(injected.len(), 1);
    let row = injected[0];
    for key in [
        "ticket", "attempt", "lane", "amount", "layout", "eligible", "cause",
    ] {
        assert_eq!(row[key], target[key]);
    }
    assert_eq!(
        row["kind"],
        if target["kind"] == "SpendAttempt" {
            "SpendResult"
        } else {
            "ReserveResult"
        }
    );
    assert_eq!(row["action"], "Refuse");
    assert_eq!(row["error"]["kind"], "Limit");
    assert_eq!(
        row["error"]["reason"],
        if target["kind"] == "SpendAttempt" {
            "test-only read signal policy spend refusal"
        } else {
            "test-only read signal policy reserve refusal"
        }
    );
    assert_eq!(accepted(b, Some(n(&target["ordinal"]))), accepted(a, None));
    assert!(
        native_rows(a)
            .iter()
            .filter(|r| n(&r["ordinal"]) > n(&row["ordinal"]))
            .all(
                |r| !matches!(r["kind"].as_str(), Some("SpendResult" | "ReserveResult"))
                    || !r["error"].is_null()
            )
    );
}
fn sweep(mut run: impl FnMut(Fault) -> SetupResult<Value>, lane: &str) -> SetupResult<Value> {
    let baseline = run(Fault::None)?;
    assert_eq!(baseline["action"]["result"]["status"], "Ok");
    let b = &baseline["receipt"]["native"];
    let selected = array(b, "factory_receipts")
        .iter()
        .find(|r| {
            r["result"]["status"] == "Ok"
                && n(&r["accepted_spend"][if lane == "Plan" { 0 } else { 1 }]) > 0
                && ["SpendAttempt", "ReserveAttempt"].iter().all(|kind| {
                    native_rows(b).iter().any(|x| {
                        x["ticket"] == r["ticket"]
                            && x["eligible"] == true
                            && x["lane"] == lane
                            && x["kind"] == *kind
                    })
                })
        })
        .expect("real factory with both callbacks in required actual lane");
    let targets = native_rows(b)
        .iter()
        .filter(|r| {
            r["ticket"] == selected["ticket"]
                && r["eligible"] == true
                && r["lane"] == lane
                && matches!(r["kind"].as_str(), Some("SpendAttempt" | "ReserveAttempt"))
        })
        .cloned()
        .collect::<Vec<_>>();
    assert!(!targets.is_empty());
    let mut failures = Vec::new();
    for target in &targets {
        let f = if target["kind"] == "SpendAttempt" {
            Fault::RefuseSpend(n(&target["attempt"]) as usize)
        } else {
            Fault::RefuseReserve(n(&target["attempt"]) as usize)
        };
        let actual = run(f)?;
        refusal(&baseline, &actual, target);
        failures.push(json!({"target":target,"actual":actual}));
    }
    let mut cancels = Vec::new();
    for kind in ["SpendAttempt", "ReserveAttempt"] {
        let target = targets.iter().find(|r| r["kind"] == kind).unwrap();
        let f = if kind == "SpendAttempt" {
            Fault::CancelBeforeSpend(n(&target["attempt"]) as usize)
        } else {
            Fault::CancelBeforeReserve(n(&target["attempt"]) as usize)
        };
        let actual = run(f)?;
        assert_cancelled(&actual["action"]);
        assert!(
            actual["receipt"]["native"]["native_rows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["ticket"] == target["ticket"]
                    && r["attempt"] == target["attempt"]
                    && r["action"] == "Cancel"
                    && r["error"]["cancelled"] == true)
        );
        assert_eq!(
            accepted(b, Some(n(&target["ordinal"]))),
            accepted(&actual["receipt"]["native"], None)
        );
        cancels.push(json!({"target":target,"actual":actual}));
    }
    Ok(
        json!({"baseline":baseline,"selected_factory":selected,"targets":targets,"refusals":failures,"callback_cancellations":cancels,
        "scope":"every actual callback of this one measured factory; global per-kind ordinals, actual lane; no full-fixture sweep"}),
    )
}
fn factory_interval<'a>(v: &'a Value, ticket: &Value) -> (&'a Value, &'a Value) {
    let uses = array(v, "use_rows");
    let begin = uses
        .iter()
        .find(|r| r["ticket"] == *ticket && r["phase"] == "factory_begin")
        .unwrap();
    let end = uses
        .iter()
        .find(|r| r["ticket"] == *ticket && r["phase"] == "factory_result")
        .unwrap();
    (begin, end)
}
fn retained_factory_frames(v: &Value, factory: &Value) -> u64 {
    let (a, b) = factory_interval(v, &factory["ticket"]);
    let rows = native_rows(v)
        .iter()
        .filter(|r| n(&r["ordinal"]) > n(&a["ordinal"]) && n(&r["ordinal"]) < n(&b["ordinal"]));
    let mut credit = 0u64;
    let mut debit = 0u64;
    for r in rows {
        if r["kind"] == "ReserveResult" && r["error"].is_null() {
            assert_eq!(r["eligible"], true);
            assert_eq!(r["ticket"], factory["ticket"]);
            credit += n(&r["frames"]);
        }
        if r["kind"] == "ReleaseAfter" {
            debit += n(&r["frames"]);
        }
    }
    let retained = credit - debit;
    assert_eq!(
        n(&factory["exit"]["ledger"]) - n(&factory["entry"]["ledger"]),
        retained
    );
    retained
}
fn overlap_in(
    v: &Value,
    require_child_allocation: bool,
    expected_child: Option<&Value>,
) -> Option<Value> {
    for child in array(v, "factory_receipts") {
        if expected_child.is_some_and(|ticket| child["ticket"] != *ticket) {
            continue;
        }
        let ticket = n(&child["ticket"]);
        let call = &v["calls"][ticket as usize - 1];
        let Some(parent) = call["parent_selected_ticket"].as_u64() else {
            continue;
        };
        let Some(parent_factory) = array(v, "factory_receipts")
            .iter()
            .find(|r| r["ticket"] == parent && r["result"]["status"] == "Ok")
        else {
            continue;
        };
        let retained = retained_factory_frames(v, parent_factory);
        if retained == 0 {
            continue;
        }
        let scope = &v["scopes"][n(&child["scope"]) as usize - 1];
        let (begin, end) = factory_interval(v, &child["ticket"]);
        if n(&scope["entry"]["ledger"]) < n(&parent_factory["entry"]["ledger"]) + retained {
            continue;
        }
        let Some(before) = array(v, "use_rows").iter().rev().find(|r| {
            r["ticket"] == parent
                && n(&r["ordinal"]) < n(&begin["ordinal"])
                && r["value"].get("view").is_some()
        }) else {
            continue;
        };
        let after = array(v, "use_rows").iter().find(|r| {
            r["ticket"] == parent
                && n(&r["ordinal"]) > n(&end["ordinal"])
                && r["value"].get("view").is_some()
        });
        let allocated = native_rows(v)
            .iter()
            .filter(|r| {
                r["ticket"] == child["ticket"]
                    && r["eligible"] == true
                    && r["kind"] == "ReserveResult"
                    && r["error"].is_null()
                    && n(&r["frames"]) > 0
            })
            .collect::<Vec<_>>();
        let peak = allocated
            .iter()
            .map(|r| n(&r["after"]["ledger"]))
            .max()
            .unwrap_or(n(&scope["entry"]["ledger"]));
        if require_child_allocation
            && (allocated.is_empty() || peak <= n(&scope["entry"]["ledger"]))
        {
            continue;
        }
        if child["result"]["status"] == "Ok" {
            let Some(after) = after else {
                continue;
            };
            assert_eq!(before["value"]["view"], after["value"]["view"]);
            assert_eq!(before["value"]["same_local_view_identity"], true);
            assert_eq!(after["value"]["same_local_view_identity"], true);
        }
        return Some(
            json!({"parent_ticket":parent,"child_ticket":ticket,"parent_factory":parent_factory,
            "parent_actual_factory_retained_frames":retained,"parent_view_before":before,"parent_view_after":after,
            "child_scope":scope,"child_peak_from_positive_native_reserves":peak,"child_positive_reserves":allocated,
            "scope":"actual factory reserve/release net plus retained private query and matching borrowed identity around child; no invented per-lease IDs"}),
        );
    }
    None
}
fn nested_lifetime(rows: &[Value]) -> Value {
    rows.iter()
        .find_map(|r| {
            overlap_in(&r["native"], true, None).map(|v| json!({"receipt":r["ordinal"],"proof":v}))
        })
        .expect("real parent policy held through a positively allocated N child")
}
fn dedicated_lifetime(fixture: &Value, rows: &[Value]) -> Value {
    let actual = &fixture["families"]["definition_group"]["Hold_side_result_lifetime"];
    let receipt = action_receipt(&actual["action"], rows);
    assert_eq!(actual["action"]["result"]["status"], "Ok");
    assert_eq!(
        actual["action"]["result"],
        actual["result_after_provider_drop"]
    );
    assert_eq!(
        actual["action"]["result"],
        actual["result_after_controller_drop"]
    );
    let first = &actual["mask_with_controller"];
    let last = &actual["mask_after_controller_drop"];
    assert_eq!(first["kind"], "Owned");
    assert!(n(&first["mask_len"]) > 0);
    for key in [
        "kind",
        "mask_len",
        "mask_capacity",
        "charge_frames",
        "account_id",
    ] {
        assert_eq!(first[key], last[key]);
    }
    assert_eq!(first["account_live_frames"], last["account_live_frames"]);
    let id = &first["account_id"];
    let events = array(&receipt["native"], "account_rows")
        .iter()
        .filter(|r| r["account_id"] == *id)
        .collect::<Vec<_>>();
    let release = events.iter().find(|r| r["kind"] == "ChargeDrop").unwrap();
    assert_eq!(release["frames"], first["charge_frames"]);
    assert_eq!(
        n(&release["before"]) - n(&release["after"]),
        n(&release["frames"])
    );
    let retire = events
        .iter()
        .find(|r| r["kind"] == "LastHandleDrop")
        .unwrap();
    assert_eq!(retire["after"], 0);
    assert!(n(&retire["ordinal"]) > n(&release["ordinal"]));
    assert!(events.iter().all(|r| r["work"].is_null()));
    let scopes = array(&receipt["native"], "scopes");
    assert!(!scopes.is_empty());
    assert_eq!(scopes[0]["entry"]["ledger"], 0);
    assert_eq!(scopes[0]["exit"]["ledger"], 0);
    json!({"receipt":receipt["ordinal"],"mask_with_controller":first,"mask_after_controller":last,
        "actual_charge_release":release,"actual_final_handle_retirement":retire,
        "scope":"request ledger restored while returned mask owns native charge; then no-observer final retirement after single read"})
}
fn warm_shell(rows: &[Value]) -> Value {
    for receipt in rows {
        let v = &receipt["native"];
        for h in array(v, "handoff_receipts") {
            let Some(ticket) = h["ticket"].as_u64() else {
                continue;
            };
            if v["calls"][ticket as usize - 1]["caller"] != "StreamedStageInput"
                || h["success"] != true
                || h["result"]["mask"]["kind"] != "Owned"
                || n(&h["result"]["mask"]["mask_len"]) == 0
            {
                continue;
            }
            let mask = &h["result"]["mask"];
            if let Some(drop) = array(v, "account_rows").iter().find(|r| {
                r["kind"] == "ChargeDrop"
                    && r["account_id"] == mask["account_id"]
                    && n(&r["ordinal"]) > n(&h["finish_ordinal"])
                    && r["frames"] == mask["charge_frames"]
                    && r["after"] == 3
            }) {
                return json!({"receipt":receipt["ordinal"],"actual_streamed_handoff":h,"actual_payload_drop":drop,
                    "scope":"real positive streamed input mask payload disposed while controller retains measured shell3; no seeded count"});
            }
        }
    }
    panic!("real streamed positive mask disposal retaining actual shell3 required");
}
fn assert_cancelled(action: &Value) {
    assert_eq!(action["result"]["status"], "Error");
    assert_eq!(action["cancelled_at_end"], true);
    assert_eq!(action["result"]["error"]["kind"], "Preparation");
    assert_eq!(
        action["result"]["error"]["typed_payload"]["variant"],
        "Cancelled"
    );
}
fn live_cancellation(
    plan: &Arc<RenderPlan>,
    definition: &AudioDefinition<'_>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
) -> SetupResult<Value> {
    let baseline = focused_public(
        plan,
        definition,
        reopen()?,
        Fault::None,
        "nested_cancel_baseline",
    );
    assert_eq!(baseline["action"]["result"]["status"], "Ok");
    let b = &baseline["receipt"]["native"];
    let preflight = focused_public(
        plan,
        definition,
        reopen()?,
        Fault::CancelAtUse(UsePoint::ContentBefore(0)),
        "live_content_preflight_cancel",
    );
    assert_cancelled(&preflight["action"]);
    assert_eq!(
        preflight["action"]["provider"]["events_after"],
        preflight["action"]["provider"]["events_before"]
    );
    assert!(
        array(&preflight["receipt"]["native"], "factory_receipts")
            .iter()
            .any(|f| f["result"]["status"] == "Ok"
                && retained_factory_frames(&preflight["receipt"]["native"], f) > 0)
    );
    let parent = focused_public(
        plan,
        definition,
        reopen()?,
        Fault::CancelAtUse(UsePoint::SpanBefore(0)),
        "parent_live_cancel",
    );
    assert_cancelled(&parent["action"]);
    let use_row = array(&parent["receipt"]["native"], "use_rows")
        .iter()
        .find(|r| r["phase"] == "use_point" && r["value"]["action"] == "set_real_cancellation")
        .unwrap();
    let factory = array(&parent["receipt"]["native"], "factory_receipts")
        .iter()
        .find(|r| r["ticket"] == use_row["ticket"])
        .unwrap();
    assert!(retained_factory_frames(&parent["receipt"]["native"], factory) > 0);
    assert!(n(&use_row["boundary"]["ledger"]) > n(&factory["entry"]["ledger"]));
    let target = native_rows(b)
        .iter()
        .find(|r| {
            matches!(r["kind"].as_str(), Some("SpendAttempt" | "ReserveAttempt"))
                && r["eligible"] == true
                && {
                    let ticket = n(&r["ticket"]);
                    let call = &b["calls"][ticket as usize - 1];
                    call["parent_selected_ticket"].is_number()
                        && native_rows(b).iter().any(|x| {
                            x["ticket"] == r["ticket"]
                                && x["kind"] == "ReserveResult"
                                && x["error"].is_null()
                                && n(&x["frames"]) > 0
                                && n(&x["ordinal"]) < n(&r["ordinal"])
                        })
                }
        })
        .expect("actual nested callback with earlier positive child allocation")
        .clone();
    let f = if target["kind"] == "SpendAttempt" {
        Fault::CancelBeforeSpend(n(&target["attempt"]) as usize)
    } else {
        Fault::CancelBeforeReserve(n(&target["attempt"]) as usize)
    };
    let child = focused_public(plan, definition, reopen()?, f, "child_live_cancel");
    assert_cancelled(&child["action"]);
    let actual = native_rows(&child["receipt"]["native"])
        .iter()
        .find(|r| {
            r["action"] == "Cancel"
                && r["eligible"] == true
                && r["attempt"] == target["attempt"]
                && r["ticket"] == target["ticket"]
                && r["error"]["cancelled"] == true
        })
        .unwrap();
    let scope = array(&child["receipt"]["native"], "scopes")
        .iter()
        .find(|s| s["ticket"] == actual["ticket"])
        .unwrap();
    assert!(n(&actual["before"]["ledger"]) > n(&scope["entry"]["ledger"]));
    assert_eq!(
        accepted(b, Some(n(&target["ordinal"]))),
        accepted(&child["receipt"]["native"], None)
    );
    let child_overlap = overlap_in(&child["receipt"]["native"], false, Some(&target["ticket"]))
        .expect("this cancelled child with retained parent guards");
    assert_eq!(child_overlap["child_ticket"], target["ticket"]);
    assert_eq!(
        child_overlap["parent_ticket"],
        child["receipt"]["native"]["calls"][n(&target["ticket"]) as usize - 1]["parent_selected_ticket"]
    );
    Ok(
        json!({"baseline":baseline,"live_content_preflight_cancel":preflight,"parent":parent,"parent_cancel_use":use_row,
        "child":child,"child_target":target,"child_overlap":child_overlap}),
    )
}
fn provider_failure(
    plan: &Arc<RenderPlan>,
    definition: &AudioDefinition<'_>,
    inner: NativeProvider,
) -> Value {
    let _ops = mask_observations();
    let (mut renderer, monitor) = super::renderer(plan, StageLimits::default(), None);
    let mut source = provider(inner, None, true);
    let action = public_once(
        &mut renderer,
        &mut source,
        PublicTarget::Definition(definition),
        (0, 256),
        "late_real_provider_refusal",
        &monitor,
        &AtomicBool::new(false),
    );
    assert_eq!(action["result"]["status"], "Error");
    assert_eq!(
        action["result"]["error"]["typed_payload"]["variant"],
        "SourceUnavailable"
    );
    let mut rows = receipts();
    assert_eq!(rows.len(), 1);
    let receipt = rows.remove(0);
    let event = receipt["provider_entries"]
        .as_array()
        .unwrap()
        .first()
        .expect("actual provider callback");
    let ticket = &event["position"]["native"]["selected_ticket"];
    assert!(ticket.is_number());
    let factory = array(&receipt["native"], "factory_receipts")
        .iter()
        .find(|r| r["ticket"] == *ticket && r["result"]["status"] == "Ok")
        .unwrap();
    assert!(retained_factory_frames(&receipt["native"], factory) > 0);
    let stack = event["position"]["native"]["active_stack"]
        .as_array()
        .unwrap();
    assert!(
        stack
            .iter()
            .any(|f| f["ticket"] == *ticket && f["kind"] == "scope")
    );
    let parent_cleanup = if let Some(parent) =
        receipt["native"]["calls"][n(ticket) as usize - 1]["parent_selected_ticket"].as_u64()
    {
        let p = array(&receipt["native"], "factory_receipts")
            .iter()
            .find(|r| r["ticket"] == parent && r["result"]["status"] == "Ok")
            .unwrap();
        let retained = retained_factory_frames(&receipt["native"], p);
        assert!(retained > 0);
        let after_callback = native_rows(&receipt["native"])
            .iter()
            .filter(|r| {
                r["ticket"] == parent
                    && r["kind"] == "ReleaseAfter"
                    && n(&r["ordinal"]) > n(&event["position"]["native"]["event"])
            })
            .collect::<Vec<_>>();
        assert!(after_callback.iter().map(|r| n(&r["frames"])).sum::<u64>() >= retained);
        Some(
            json!({"parent_factory":p,"retained":retained,"actual_releases_after_child_callback":after_callback}),
        )
    } else {
        None
    };
    json!({"action":action,"receipt":receipt,"factory_live_at_callback":factory,
        "parent_cleanup_after_child_PCM_failure":parent_cleanup,
        "scope":"real qualified source; explicit provider refusal after actual selected factory success; callback entry, not decoder I/O"})
}
fn handoff_cancellation(
    plan: &Arc<RenderPlan>,
    definition: &AudioDefinition<'_>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
) -> SetupResult<Value> {
    let mut cases = Vec::new();
    for point in [
        UsePoint::BeforeHandoff,
        UsePoint::PrepareInitialCheck,
        UsePoint::PrepareRecheck,
    ] {
        let _ops = mask_observations();
        let (mut renderer, monitor) = super::renderer(plan, StageLimits::default(), None);
        let mut source = provider(reopen()?, None, false);
        next_fault(Fault::CancelAtUse(point));
        let action = public_once(
            &mut renderer,
            &mut source,
            PublicTarget::Definition(definition),
            (512, 128),
            "positive_final_mask_cancel",
            &monitor,
            &AtomicBool::new(false),
        );
        assert_cancelled(&action);
        let mut rows = receipts();
        assert_eq!(rows.len(), 1);
        let receipt = rows.remove(0);
        let native = &receipt["native"];
        let cancelled = array(native, "use_rows")
            .iter()
            .find(|r| r["phase"] == "use_point" && r["value"]["action"] == "set_real_cancellation")
            .unwrap();
        let factory = array(native, "factory_receipts")
            .iter()
            .find(|f| f["ticket"] == cancelled["ticket"] && f["result"]["status"] == "Ok")
            .unwrap();
        assert!(retained_factory_frames(native, factory) > 0);
        let handoff = array(native, "handoff_receipts")
            .iter()
            .find(|h| h["ticket"] == cancelled["ticket"])
            .unwrap();
        assert_eq!(handoff["success"], false);
        assert!(handoff["pre_exclusive"].is_null());
        assert!(handoff["vector_frames"].is_null());
        let box_cleanup = if point == UsePoint::PrepareRecheck {
            let drop = array(native, "account_rows")
                .iter()
                .find(|r| {
                    r["handoff"] == handoff["ordinal"]
                        && r["kind"] == "ChargeDrop"
                        && n(&r["frames"]) > 0
                })
                .unwrap();
            assert_eq!(n(&drop["before"]) - n(&drop["after"]), n(&drop["frames"]));
            assert!(
                n(&drop["after"]) > 0,
                "controller shell remains after prepared destination cleanup"
            );
            Some(drop)
        } else {
            None
        };
        cases.push(json!({"point":point,"action":action,"receipt":receipt,
            "live_source_factory":factory,"cancel_use":cancelled,
            "failed_handoff":handoff,"actual_prepared_box_cleanup":box_cleanup,
            "controller_after":controller(&renderer)}));
    }
    Ok(
        json!({"cases":cases,"scope":"real existing checks; before exclusive preflight; no inserted check or post-exclusive hook"}),
    )
}
fn cap_trial(
    plan: &Arc<RenderPlan>,
    definition: &AudioDefinition<'_>,
    inner: NativeProvider,
    cap: u32,
    matched: bool,
    held: bool,
) -> Value {
    assert!(cap > 0 && cap <= StageLimits::default().maximum_resident_frames);
    let _ops = mask_observations();
    let (mut renderer, monitor) = super::renderer(plan, StageLimits::default(), None);
    let mut source = provider(inner, None, false);
    let cancel = AtomicBool::new(false);
    let target = PublicTarget::Definition(definition);
    let warmup = public_once(
        &mut renderer,
        &mut source,
        target,
        (0, 256),
        "resident_real_warmup",
        &monitor,
        &cancel,
    );
    assert_eq!(warmup["result"]["status"], "Ok");
    let warmup_receipts = receipts();
    let mut holder = None;
    let mut setup = Value::Null;
    if matched {
        let (actual, row) = call_public(
            &mut renderer,
            &mut source,
            target,
            (512, 128),
            "resident_actual_holder",
            &monitor,
            &cancel,
        );
        assert_eq!(row["result"]["status"], "Ok");
        let scalar = mask_snapshot(&actual);
        assert_eq!(scalar["kind"], "Owned");
        assert!(n(&scalar["mask_len"]) > 0);
        let rows = receipts();
        setup = json!({"action":row,"receipts":rows,"mask_live":scalar,"controller_before_choice":controller(&renderer)});
        if held {
            holder = Some(actual);
        } else {
            drop(actual);
        }
    }
    let before_choice = controller(&renderer);
    renderer.limits.maximum_resident_frames = cap;
    let measured = public_once(
        &mut renderer,
        &mut source,
        target,
        (512, 128),
        "resident_measured",
        &monitor,
        &cancel,
    );
    let measured_receipts = receipts();
    let success = measured["result"]["status"] == "Ok";
    if !success {
        assert_eq!(measured["result"]["error"]["kind"], "Limit");
        assert_eq!(
            measured["result"]["error"]["typed_payload"]["reason"],
            "resident PCM or stage cache"
        );
    } else {
        assert!(
            !array(&measured_receipts[0]["native"], "factory_receipts").is_empty(),
            "measured success must run selected N"
        );
    }
    let holder_before_drop = holder.as_ref().map(mask_snapshot);
    let controller_before_drop = controller(&renderer);
    drop(holder);
    let controller_after_drop = controller(&renderer);
    if let Some(mask) = &holder_before_drop {
        assert_eq!(
            n(&controller_before_drop["owned_account"])
                - n(&controller_after_drop["owned_account"]),
            n(&mask["charge_frames"])
        );
    }
    let retry = if held && !success {
        let actual = public_once(
            &mut renderer,
            &mut source,
            target,
            (512, 128),
            "resident_drop_retry",
            &monitor,
            &cancel,
        );
        let rows = receipts();
        Some(json!({"action":actual,"receipts":rows,"controller_after":controller(&renderer)}))
    } else {
        None
    };
    assert_eq!(renderer.limits.maximum_resident_frames, cap);
    json!({"ceiling":cap,"matched_positive_setup":matched,"held":held,"warmup":warmup,"warmup_receipts":warmup_receipts,
        "actual_positive_setup":setup,"controller_after_choice":before_choice,"measured":measured,"measured_receipts":measured_receipts,
        "success":success,"holder_before_drop":holder_before_drop,"controller_before_holder_drop":controller_before_drop,
        "controller_after_holder_drop":controller_after_drop,"retry":retry,
        "same_controller_same_cap":true,"fresh_ReadWork_per_public_call":true,
        "scope":"actual N returned holder only; capture-local IDs not joined across requests; no observer/Arc telemetry"})
}
fn minimum(mut run: impl FnMut(u32) -> SetupResult<Value>) -> SetupResult<(u32, Vec<Value>)> {
    let mut lo = 1;
    let mut hi = StageLimits::default().maximum_resident_frames;
    let mut rows = Vec::new();
    let top = run(hi)?;
    assert_eq!(top["success"], true);
    rows.push(top);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let row = run(mid)?;
        if row["success"] == true {
            hi = mid;
        } else {
            lo = mid + 1;
        }
        rows.push(row);
    }
    let exact = run(lo)?;
    assert_eq!(exact["success"], true);
    rows.push(exact);
    if lo > 1 {
        let below = run(lo - 1)?;
        assert_eq!(below["success"], false);
        rows.push(below);
    }
    for a in &rows {
        for b in &rows {
            if n(&a["ceiling"]) < n(&b["ceiling"]) && a["success"] == true {
                assert_eq!(b["success"], true, "observed fresh-state monotonic witness");
            }
        }
    }
    Ok((lo, rows))
}
fn matched_prechoice(row: &Value) -> Value {
    let setup = &row["actual_positive_setup"];
    assert!(setup.is_object());
    let action = &setup["action"];
    let warmup = &row["warmup"];
    json!({"input_before":action["input_before"],"input_after":action["input_after"],
        "result":action["result"],"provider":action["provider"],
        "controller_before":action["controller_before"],"controller_result_live":action["controller_result_live"],
        "controller_before_choice":setup["controller_before_choice"],"mask_live":setup["mask_live"],
        "warmup_input_before":warmup["input_before"],"warmup_input_after":warmup["input_after"],
        "warmup_result":warmup["result"],"warmup_provider":warmup["provider"],
        "warmup_controller_before":warmup["controller_before"],"warmup_controller_result_live":warmup["controller_result_live"]})
}
fn assert_matched_prechoice(rows: &[&Value]) -> Value {
    let first = matched_prechoice(rows.first().unwrap());
    for row in rows {
        assert_eq!(
            first,
            matched_prechoice(row),
            "actual state before hold/drop choice"
        );
    }
    json!({"states":rows.len(),"actual_prechoice_state":first,
        "relation":"actual input, complete output/provider/controller/cache/resident and borrowed mask values equal before hold/drop choice; measured legal cap set afterward",
        "raw_receipts":"retained separately, including differing telemetry reset epochs and cumulative host counters"})
}
fn residency(
    plan: &Arc<RenderPlan>,
    definition: &AudioDefinition<'_>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
) -> SetupResult<Value> {
    let (plain, plain_rows) =
        minimum(|cap| Ok(cap_trial(plan, definition, reopen()?, cap, false, false)))?;
    let initial_held = cap_trial(plan, definition, reopen()?, plain, true, true);
    let (dropped, dropped_rows) =
        minimum(|cap| Ok(cap_trial(plan, definition, reopen()?, cap, true, false)))?;
    let (held, held_rows) =
        minimum(|cap| Ok(cap_trial(plan, definition, reopen()?, cap, true, true)))?;
    let matched = dropped_rows.iter().chain(&held_rows).collect::<Vec<_>>();
    let matched_prechoice = assert_matched_prechoice(&matched);
    let separating = if held > dropped {
        let row = cap_trial(plan, definition, reopen()?, held - 1, true, true);
        assert_eq!(row["success"], false);
        assert_eq!(row["retry"]["action"]["result"]["status"], "Ok");
        Some(row)
    } else {
        None
    };
    let mut natural_constructor = Vec::new();
    for row in plain_rows.iter().chain(&dropped_rows).chain(&held_rows) {
        for receipt in row["measured_receipts"].as_array().unwrap() {
            assert_eq!(receipt["native"]["fault"], "None");
            for r in native_rows(&receipt["native"]) {
                if r["kind"] == "MakeRoomAfter"
                    && r["eligible"] == true
                    && r["error"]["kind"] == "Limit"
                {
                    natural_constructor.push(
                        json!({"ceiling":row["ceiling"],"row":r,"receipt":receipt["ordinal"]}),
                    );
                }
            }
        }
    }
    Ok(
        json!({"plain_minimum":plain,"plain_trials":plain_rows,"initial_held_trial_retained":initial_held,
        "matched_dropped_minimum":dropped,"matched_dropped_trials":dropped_rows,
        "matched_held_minimum":held,"matched_held_trials":held_rows,"matched_prechoice":matched_prechoice,"separating_held_drop_retry":separating,
        "natural_constructor_refusals":natural_constructor,
        "gap":if separating.is_some() {Value::Null}else{json!("no actual separating boundary found in fixed matched states")},
        "peak_scope":"resident predicate operands/actual request ledger and native account are separate from physical memory"}),
    )
}
pub(super) fn fixture_visit(
    plan: &Arc<RenderPlan>,
    base: &Arc<AudioStageProjection<'_>>,
    asset: &deadpan_core::AssetRecord,
    wire: &Value,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
    provenance: impl FnOnce() -> SetupResult<Value>,
) -> SetupResult<Option<Value>> {
    let mode = COLLECTION.with(|v| v.borrow().as_ref().map(|v| v.mode));
    let Some(mode) = mode else {
        return Ok(None);
    };
    let provenance = provenance()?;
    if mode == Mode::All {
        return Ok(Some(fixture_cases(
            plan, base, asset, wire, reopen, provenance,
        )?));
    }
    let (plain, document) = plain_nested_plan(asset, wire)?;
    let source = plain.audio_definition(AudioDefinitionSelector::Node { node: id("a") })?;
    let group = plain.audio_definition(AudioDefinitionSelector::Node { node: id("group") })?;
    let nested = plain.audio_definition(AudioDefinitionSelector::Node { node: id("outer") })?;
    let result = match mode {
        Mode::PlanFaults => {
            let faults = sweep(
                |f| {
                    Ok(focused_public(
                        &plain,
                        &source,
                        reopen()?,
                        f,
                        "focused_Plan",
                    ))
                },
                "Plan",
            )?;
            let cancellation = live_cancellation(&plain, &nested, reopen)?;
            let provider = provider_failure(&plain, &source, reopen()?);
            let child_provider = provider_failure(&plain, &nested, reopen()?);
            assert!(!child_provider["parent_cleanup_after_child_PCM_failure"].is_null());
            let handoff = handoff_cancellation(&plain, &group, reopen)?;
            json!({"faults":faults,"live_cancellation":cancellation,"late_provider_failure":provider,
                "actual_child_PCM_provider_failure":child_provider,"positive_final_mask_cancellation":handoff})
        }
        Mode::CacheFaults => sweep(|f| focused_cache(&plain, &nested, reopen, f), "Cache")?,
        Mode::Residency => residency(&plain, &group, reopen)?,
        Mode::All => unreachable!(),
    };
    Ok(Some(
        json!({"provenance":provenance,"actual_checked_document":document,"result":result}),
    ))
}
fn focused(mode: Mode) -> Value {
    let guard = collection(mode);
    let body = match super::super::family() {
        Ok(v) => v,
        Err(e) => setup_error("actual_focused_registered_fixture", e.as_ref()),
    };
    assert!(body["provenance"]["receipt_id"].is_string(), "{body}");
    assert!(body["result"].is_object(), "{body}");
    assert!(
        receipts().is_empty(),
        "focused cases must retain their receipts in their actual case"
    );
    drop(guard);
    body
}
fn all_body() -> Value {
    let pair = paired();
    let plan = focused(Mode::PlanFaults);
    let cache = focused(Mode::CacheFaults);
    let residency = focused(Mode::Residency);
    json!({"schema":"read-signal-policy-native-AFTER-qualification-v1","paired":pair,
        "Plan_callbacks_and_live_cancel":plan,"Cache_callbacks":cache,"resident_and_held_output":residency,
        "scope":{"selected":"N read_signal policy factory and retained use/custody only",
            "ordinary_before42":"physical immutable body and key-aware relation, no regeneration",
            "capacities":"actual before source growth vs current prepare/returned observations; no normalization",
            "failed_query_work":"local canonical/additional split unexposed",
            "explicit_gaps":["no fabricated positive-capacity empty","no initial mask-work refusal fault namespace",
                "no whole-preparation/graph/PCM/dependency admission","no product Gate claim"]}})
}
#[test]
fn read_signal_policy_current_paired_values_work_and_real_lifetimes() {
    paired();
}
#[test]
fn read_signal_policy_real_plan_callbacks_parent_child_cancel_and_provider_cleanup() {
    focused(Mode::PlanFaults);
}
#[test]
fn read_signal_policy_real_cache_callback_refusals() {
    focused(Mode::CacheFaults);
}
#[test]
fn read_signal_policy_public_resident_boundary_and_actual_holder_recovery() {
    let value = focused(Mode::Residency);
    assert!(
        !value["result"]["natural_constructor_refusals"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!value["result"]["separating_held_drop_retry"].is_null());
}
#[test]
#[ignore = "manual fresh full actual N AFTER qualification; no expected body"]
fn capture_read_signal_policy_after_v1() {
    let output = std::env::var_os("DEADPAN_READ_SIGNAL_POLICY_AFTER_OUTPUT").unwrap();
    let output = Path::new(&output);
    assert!(output.is_absolute() && output.starts_with("/tmp"));
    let variant = std::env::var("DEADPAN_READ_SIGNAL_POLICY_AFTER_VARIANT").unwrap();
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_READ_SIGNAL_POLICY_AFTER_SOURCE_SHA256").unwrap();
    assert!(source.len() == 64 && source.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(
        std::env::var("DEADPAN_READ_SIGNAL_POLICY_AFTER_PUBLISHED_CHECKPOINT").unwrap(),
        CHECKPOINT_N
    );
    let mut body = all_body();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    body["published_checkpoint"] = json!(CHECKPOINT_N);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(&body).unwrap())
        .unwrap();
    file.write_all(b"\n").unwrap();
    file.flush().unwrap();
    file.sync_all().unwrap();
}
#[path = "read_signal_policy_after_key_replay.rs"]
mod key_proof;
