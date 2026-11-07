//! Actual O admission observations; capture storage is outside the request ledger.
use super::*;
use crate::stages::{
    read_signal_policy_native_trace as n_native, stage_block_policy_native_trace as native,
};
use native::{Fault, UsePoint};
use std::collections::{BTreeMap, BTreeSet};

#[path = "stage_block_policy_evidence_journal.rs"]
mod journal;

#[path = "saved_key_output_policy_before_tests.rs"]
mod saved_key_output_policy;

fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}
fn validate_journal_case(raw: &Value) {
    fn visit(value: &Value, count: &mut usize) {
        match value {
            Value::Object(map) => {
                if map.contains_key("native")
                    && map.contains_key("ambient_N")
                    && map.contains_key("keys")
                {
                    valid(&value["native"], &value["ambient_N"]);
                    super::key_replay::valid_keys(&value["keys"]);
                    *count += 1;
                }
                for child in map.values() {
                    visit(child, count);
                }
            }
            Value::Array(values) => {
                for child in values {
                    visit(child, count);
                }
            }
            _ => {}
        }
    }
    let mut count = 0;
    visit(raw, &mut count);
    assert!(
        count > 0,
        "journal production cases require complete actual O/N/key receipts"
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    All,
    Plan,
    Cache,
    Residency,
}
struct Collection {
    mode: Mode,
    next: Fault,
    rows: Vec<Value>,
    provider: Option<Vec<Value>>,
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
        let mut v = v.borrow_mut();
        assert!(v.is_none());
        *v = Some(Collection {
            mode,
            next: Fault::None,
            rows: Vec::new(),
            provider: None,
        });
    });
    CollectionGuard
}
pub(super) fn active() -> bool {
    COLLECTION.with(|v| v.borrow().is_some())
}
pub(in super::super) struct Action {
    name: String,
    entry: &'static str,
    o: native::CaptureGuard,
    n: n_native::CaptureGuard,
    keys: saved_key_output_policy::KeysCapture,
}
pub(super) fn begin_action(name: &str, entry: &'static str) -> Option<Action> {
    let fault = COLLECTION.with(|v| {
        v.borrow_mut().as_mut().map(|v| {
            assert!(v.provider.is_none());
            v.provider = Some(Vec::new());
            std::mem::replace(&mut v.next, Fault::None)
        })
    })?;
    saved_key_output_policy::action_begin(name, entry);
    Some(Action {
        name: name.into(),
        entry,
        keys: saved_key_output_policy::begin_keys(),
        n: n_native::begin(n_native::Fault::None),
        o: native::begin(fault),
    })
}
pub(super) fn provider_position() -> Option<Value> {
    if !active() {
        return None;
    }
    let value = json!({"observer":"O-native-and-current-N-native-and-N-KeysOnly",
        "o":native::position(),"n":n_native::position(),"keys":before_trace::position(),
        "scope":"actual callback entry; no O provider-use or decoder-I/O inference"});
    COLLECTION.with(|v| {
        let mut v = v.borrow_mut();
        let v = v.as_mut().unwrap();
        let rows = v
            .provider
            .as_mut()
            .expect("provider callback inside one actual action");
        rows.push(json!({"ordinal":rows.len()+1,"position":value}));
    });
    Some(value)
}
pub(super) fn end_action(action: Action) -> Value {
    let o = native::take();
    let n = n_native::take();
    let keys = saved_key_output_policy::take_keys(&action.keys);
    drop(action.o);
    drop(action.n);
    drop(action.keys);
    valid(&o, &n);
    saved_key_output_policy::valid_action_keys(&keys);
    COLLECTION.with(|v| {
        let mut v = v.borrow_mut();
        let v = v.as_mut().unwrap();
        let ordinal = v.rows.len() + 1;
        let provider = v.provider.take().unwrap();
        saved_key_output_policy::action_end(ordinal);
        v.rows.push(
            json!({"ordinal":ordinal,"action":action.name,"entry":action.entry,
            "native":o,"ambient_N":n,"keys":keys,"provider_entries":provider}),
        );
        json!({"schema":"O-AFTER-action-reference-v1","ordinal":ordinal,
            "existing_mask_ops":point_suppression::ops::trace::snapshot(),
            "existing_public_owner_work":point_suppression::work_trace::snapshot(),
            "counter_scope":"actual cumulative group counters, not O-only work"})
    })
}
fn receipts() -> Vec<Value> {
    COLLECTION.with(|v| {
        let mut v = v.borrow_mut();
        let v = v.as_mut().unwrap();
        assert!(v.provider.is_none());
        std::mem::take(&mut v.rows)
    })
}
fn next_fault(fault: Fault) {
    COLLECTION.with(|v| {
        let mut v = v.borrow_mut();
        let v = v.as_mut().unwrap();
        assert!(v.provider.is_none());
        v.next = fault;
    });
}
pub(super) fn private_action(
    plan: &Arc<RenderPlan>,
    target: PrivateTarget<'_, '_>,
    inner: NativeProvider,
    name: &str,
    limits: StageLimits,
    options: PrivateOptions,
) -> Value {
    // This existing raw lifetime helper uses common action dispatch and retains no account observer.
    super::super::after::private_action(plan, target, inner, name, limits, options)
}
fn fields(v: &Value, expected: &[&str]) {
    assert_eq!(
        v.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        expected.iter().copied().collect::<BTreeSet<_>>()
    );
}
fn n(v: &Value) -> u64 {
    v.as_u64().unwrap()
}
fn id(v: &Value) -> Option<u64> {
    if v.is_null() { None } else { Some(n(v)) }
}
fn array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v[key].as_array().unwrap()
}
fn pair(v: &Value) -> [u64; 2] {
    let values = v.as_array().unwrap();
    assert_eq!(values.len(), 2);
    [n(&values[0]), n(&values[1])]
}
fn boundary(v: &Value) {
    fields(
        v,
        &[
            "work",
            "ledger",
            "lane",
            "ops",
            "host_units",
            "host_overflow",
            "host_epoch",
        ],
    );
    pair(&v["work"]);
    n(&v["ledger"]);
    assert!(n(&v["lane"]) < 2);
    pair(&v["host_units"]);
    assert_eq!(v["host_overflow"], false);
    assert!(id(&v["host_epoch"]).is_some_and(|epoch| epoch > 0));
    let ops = &v["ops"];
    fields(ops, &["enabled", "reset_epoch", "lanes", "overflow"]);
    assert_eq!(ops["enabled"], true);
    assert_eq!(ops["overflow"], false);
    assert!(id(&ops["reset_epoch"]).is_some_and(|epoch| epoch > 0));
    assert_eq!(array(ops, "lanes").len(), 2);
    for lane in array(ops, "lanes") {
        assert_eq!(lane.as_array().unwrap().len(), 8);
        for value in lane.as_array().unwrap() {
            n(value);
        }
    }
}
fn same_epochs(a: &Value, b: &Value) {
    boundary(a);
    boundary(b);
    assert_eq!(a["host_epoch"], b["host_epoch"]);
    assert_eq!(a["ops"]["reset_epoch"], b["ops"]["reset_epoch"]);
}
fn resident(v: &Value) {
    fields(
        v,
        &[
            "cached",
            "active",
            "projected",
            "sound_context",
            "owned_suppression",
            "pending_additional",
            "sum_before_pending",
            "limit",
        ],
    );
    let sum = [
        "cached",
        "active",
        "projected",
        "sound_context",
        "owned_suppression",
    ]
    .iter()
    .try_fold(0u64, |sum, k| sum.checked_add(n(&v[*k])))
    .unwrap();
    assert_eq!(v["sum_before_pending"], sum);
    n(&v["pending_additional"]);
    assert!(n(&v["limit"]) > 0);
}
fn native_error(v: &Value) {
    fields(v, &["kind", "reason", "cancelled"]);
    error_kind(&v["kind"]);
    assert!(v["reason"].is_null() || v["reason"].is_string());
    assert_eq!(
        v["reason"].is_string(),
        matches!(v["kind"].as_str(), Some("Limit" | "Unsupported"))
    );
    v["cancelled"].as_bool().unwrap();
}
fn result(v: &Value) {
    match v["status"].as_str().unwrap() {
        "Ok" => {
            fields(v, &["status", "success_payload"]);
            assert_eq!(v["success_payload"], "unexposed");
        }
        "Err" => {
            fields(v, &["status", "error"]);
            typed(&v["error"]);
        }
        _ => panic!("unknown native result"),
    }
}
fn view(v: &Value) {
    fields(
        v,
        &[
            "suppressed",
            "contents",
            "lookup",
            "work",
            "suppressed_len",
            "contents_len",
            "capacity",
        ],
    );
    assert_eq!(n(&v["suppressed_len"]), array(v, "suppressed").len() as u64);
    assert_eq!(n(&v["contents_len"]), array(v, "contents").len() as u64);
    n(&v["work"]);
    fields(
        &v["lookup"],
        &[
            "iteration_run_comparisons",
            "sequence_comparisons",
            "visited_nodes",
        ],
    );
    for value in v["lookup"].as_object().unwrap().values() {
        n(value);
    }
    for range in array(v, "suppressed") {
        fields(range, &["start", "end"]);
        assert!(range["start"].as_i64().unwrap() <= range["end"].as_i64().unwrap());
    }
    for content in array(v, "contents") {
        match content["type"].as_str().unwrap() {
            "silence" => {
                fields(content, &["type", "reason"]);
                assert!(content["reason"].is_string());
            }
            "source" => {
                fields(content, &["type", "duration", "source", "start", "support"]);
                assert!(content["source"].is_object());
            }
            _ => panic!("unexpected content in this actual frozen O fixture"),
        }
    }
    assert_eq!(
        v["capacity"],
        "unexposed borrowed slices; never inferred from length"
    );
}
fn pcm(v: &Value, len: u64) {
    let values = v.as_array().unwrap();
    assert_eq!(values.len() as u64, len);
    for value in values {
        pair(value);
        for bit in value.as_array().unwrap() {
            u32::try_from(n(bit)).unwrap();
        }
    }
}
fn callback_key(row: &Value) -> (String, u64, u64) {
    (
        row["phase"]
            .as_str()
            .unwrap()
            .split('_')
            .next()
            .unwrap()
            .into(),
        id(&row["ticket"]).unwrap_or(0),
        n(&row["value"]["attempt"]),
    )
}
fn call_rows<'a>(v: &'a Value, ticket: &Value, phase: &str) -> Vec<&'a Value> {
    array(v, "rows")
        .iter()
        .filter(|r| r["ticket"] == *ticket && r["phase"] == phase)
        .collect()
}
fn one<'a>(v: &'a Value, ticket: &Value, phase: &str) -> &'a Value {
    let rows = call_rows(v, ticket, phase);
    assert_eq!(rows.len(), 1, "{phase}");
    rows[0]
}
fn retained_bytes(v: &Value) -> usize {
    array(v, "calls")
        .iter()
        .chain(array(v, "rows"))
        .map(|r| serde_json::to_vec(r).unwrap().len())
        .sum()
}
fn valid(v: &Value, ambient: &Value) {
    fields(
        v,
        &[
            "schema",
            "fault",
            "attempts",
            "invalid",
            "overflow",
            "dropped",
            "active_at_take",
            "calls",
            "rows",
            "retained_payload_bytes",
            "request_rows",
            "bounds",
            "storage",
            "ownership",
            "ledger_scope",
            "request_identity_scope",
        ],
    );
    assert_eq!(v["schema"], "stage-block-policy-native-v1");
    assert_eq!(v["invalid"], false);
    assert_eq!(v["overflow"], false);
    assert_eq!(v["dropped"], 0);
    assert_eq!(v["active_at_take"], json!([]));
    assert_eq!(n(&v["retained_payload_bytes"]), retained_bytes(v) as u64);
    assert_eq!(
        v["bounds"],
        json!({"rows":32768,"calls":4096,"active_frames":128,
        "requests":4096,"serialized_payload_bytes":67108864})
    );
    assert!(array(v, "rows").len() <= 32768 && array(v, "calls").len() <= 4096);
    assert!(retained_bytes(v) <= 67108864 && serde_json::to_vec(v).unwrap().len() <= 67108864);
    assert_eq!(
        v["storage"],
        "postconstruction retained calls/rows accounting plus final JSON check; not physical RSS; temporary JSON outside ledger"
    );
    assert_eq!(
        v["ownership"],
        "discard-only complete query owner; no O native mask/Box/Account/handoff"
    );
    assert_eq!(
        v["ledger_scope"],
        "exact callback-local reserve/release/merge equations and O method entry/exit restoration; ambient gaps or JointTransfer are not inferred as frees or balanced residuals"
    );
    assert_eq!(
        v["request_identity_scope"],
        "one capture must not reuse a dead Work address; no numeric key dereference or inferred retirement"
    );
    fault_shape(&v["fault"]);
    let requests = array(v, "request_rows");
    assert!(requests.len() <= 1, "one actual live Work per action");
    for (i, r) in requests.iter().enumerate() {
        fields(r, &["ordinal", "identity"]);
        assert_eq!(r["ordinal"], i + 1);
        assert_eq!(
            r["identity"],
            "private exact live-Work reference equality; numeric key unexposed and never dereferenced"
        );
    }
    let calls = array(v, "calls");
    for (i, c) in calls.iter().enumerate() {
        fields(
            c,
            &[
                "ordinal",
                "request",
                "parent",
                "input",
                "start",
                "sample_len",
                "entry",
                "method_entry",
                "method_exit",
                "exit",
                "factory_entry",
                "factory_active",
                "factory_done",
                "accepted_spend",
                "accepted_host",
            ],
        );
        assert_eq!(c["ordinal"], i + 1);
        assert_eq!(c["request"], 1);
        if let Some(parent) = id(&c["parent"]) {
            assert!(parent > 0 && parent < (i + 1) as u64);
        }
        assert!(matches!(
            c["input"].as_str(),
            Some("Signal" | "PreserveTape")
        ));
        c["start"].as_i64().unwrap();
        n(&c["sample_len"]);
        for key in ["entry", "method_entry", "method_exit", "exit"] {
            boundary(&c[key]);
            same_epochs(&c["entry"], &c[key]);
        }
        assert_eq!(
            c["method_entry"]["ledger"], c["method_exit"]["ledger"],
            "complete O owner cleanup"
        );
        assert_eq!(c["factory_active"], false);
        c["factory_done"].as_bool().unwrap();
        pair(&c["accepted_spend"]);
        pair(&c["accepted_host"]);
        if !c["factory_entry"].is_null() {
            boundary(&c["factory_entry"]);
        }
    }
    let mut frames = BTreeMap::<u64, Value>::new();
    let mut closed = BTreeSet::new();
    let mut previous = Vec::<u64>::new();
    let mut max_frame = 0;
    let mut entered = BTreeSet::new();
    let mut attempts = [0u64; 2];
    let mut pending = BTreeMap::<(String, u64, u64), Value>::new();
    let mut factories = BTreeSet::new();
    let mut rooms = BTreeMap::new();
    let mut completed_rooms = BTreeSet::new();
    let mut mutations = BTreeMap::new();
    for (i, row) in array(v, "rows").iter().enumerate() {
        fields(
            row,
            &["ordinal", "phase", "ticket", "request", "position", "value"],
        );
        assert_eq!(row["ordinal"], i + 1);
        assert_eq!(row["request"], 1);
        let ticket = id(&row["ticket"]);
        let position = &row["position"];
        fields(position, &["active_stack", "last_ordinal"]);
        assert_eq!(position["last_ordinal"], i + 1);
        let mut stack = Vec::new();
        assert!(array(position, "active_stack").len() <= 128);
        let mut births = 0;
        for frame in array(position, "active_stack") {
            fields(frame, &["id", "ticket", "kind"]);
            let frame_id = n(&frame["id"]);
            let t = n(&frame["ticket"]);
            assert!(t > 0 && t <= calls.len() as u64);
            assert!(stack.last().is_none_or(|prior| *prior < frame_id));
            assert!(!closed.contains(&frame_id));
            assert!(matches!(frame["kind"].as_str(), Some("caller" | "method")));
            if let Some(old) = frames.get(&frame_id) {
                assert_eq!(old, frame);
            } else {
                births += 1;
                assert_eq!(births, 1);
                assert_eq!(frame_id, max_frame + 1);
                max_frame = frame_id;
                assert_eq!(
                    frame["kind"],
                    if row["phase"] == "caller_enter" {
                        "caller"
                    } else {
                        assert_eq!(row["phase"], "method_enter");
                        "method"
                    }
                );
                frames.insert(frame_id, frame.clone());
            }
            stack.push(frame_id);
        }
        let shared = previous
            .iter()
            .zip(&stack)
            .take_while(|(a, b)| a == b)
            .count();
        for frame in &previous[shared..] {
            closed.insert(*frame);
        }
        previous = stack;
        if matches!(row["phase"].as_str(), Some("caller_enter" | "method_enter")) {
            assert_eq!(births, 1);
        } else {
            assert_eq!(births, 0);
        }
        assert_eq!(
            ticket,
            array(position, "active_stack")
                .last()
                .map(|f| n(&f["ticket"])),
            "every row binds its actual nearest live O frame"
        );
        let phase = row["phase"].as_str().unwrap();
        let value = &row["value"];
        if phase == "factory_begin" {
            assert!(factories.insert(ticket.unwrap()));
        }
        if phase == "factory_result" {
            assert!(factories.remove(&ticket.unwrap()));
        }
        if matches!(phase, "method_enter" | "method_exit_after_owner_cleanup") {
            let top = array(position, "active_stack").last().unwrap();
            assert_eq!(top["kind"], "method");
            let stack = array(position, "active_stack");
            assert!(stack.len() >= 2);
            let caller = &stack[stack.len() - 2];
            assert_eq!(caller["kind"], "caller");
            assert_eq!(caller["ticket"], top["ticket"]);
        }
        if matches!(
            phase,
            "writer_push_before" | "writer_push_result" | "writer_push_after" | "caller_exit"
        ) {
            let top = array(position, "active_stack").last().unwrap();
            assert_eq!(top["kind"], "caller");
        }
        if phase == "caller_enter" {
            let t = ticket.unwrap();
            assert!(entered.insert(t));
            assert_eq!(t, entered.len() as u64);
            let stack = array(position, "active_stack");
            assert_eq!(stack.last().unwrap()["kind"], "caller");
            let parent = stack.iter().rev().nth(1).map(|f| n(&f["ticket"]));
            assert_eq!(id(&calls[t as usize - 1]["parent"]), parent);
        } else if let Some(t) = ticket {
            assert!(entered.contains(&t));
        }
        match phase {
            "spend_attempt" | "reserve_attempt" => {
                fields(
                    value,
                    &[
                        "attempt",
                        "eligible",
                        "cause",
                        "amount",
                        "layout",
                        "before",
                        "action",
                        "n_position",
                    ],
                );
                boundary(&value["before"]);
                let eligible = value["eligible"].as_bool().unwrap();
                let expected = array(position, "active_stack")
                    .last()
                    .is_some_and(|f| f["kind"] == "method" && factories.contains(&n(&f["ticket"])));
                assert_eq!(eligible, expected, "actual selected factory interval");
                assert_eq!(
                    value["cause"],
                    if eligible {
                        "SelectedFactory"
                    } else {
                        "Ambient"
                    }
                );
                let lane = usize::from(phase == "reserve_attempt");
                if eligible {
                    attempts[lane] += 1;
                    assert_eq!(value["attempt"], attempts[lane]);
                } else {
                    assert_eq!(value["attempt"], 0);
                    assert_eq!(value["action"], "None");
                }
                n(&value["amount"]);
                assert!(matches!(
                    value["action"].as_str(),
                    Some("None" | "Cancel" | "Refuse")
                ));
                if lane == 1 {
                    let layout = pair(&value["layout"]);
                    assert!(layout[1] > 0 && layout[1].is_power_of_two());
                } else {
                    assert!(value["layout"].is_null());
                }
                assert!(pending.insert(callback_key(row), value.clone()).is_none());
            }
            "spend_result" | "reserve_result" => {
                fields(
                    value,
                    &[
                        "attempt",
                        "eligible",
                        "cause",
                        "amount",
                        "layout",
                        "frames",
                        "before",
                        "after",
                        "error",
                        "action",
                        "actual_injected_refusal",
                        "equation",
                    ],
                );
                let before = pending
                    .remove(&callback_key(row))
                    .expect("actual callback attempt");
                if phase == "reserve_result" {
                    let room = completed_rooms.remove(&callback_key(row));
                    if value["error"].is_null() {
                        assert!(room, "successful reserve reached actual make_room");
                    }
                }
                for key in [
                    "attempt", "eligible", "cause", "amount", "layout", "before", "action",
                ] {
                    assert_eq!(before[key], value[key]);
                }
                same_epochs(&value["before"], &value["after"]);
                assert_eq!(value["equation"], true);
                if !value["error"].is_null() {
                    native_error(&value["error"]);
                }
                let injected = value["error"]["kind"] == "Limit"
                    && matches!(
                        value["error"]["reason"].as_str(),
                        Some(
                            "test-only stage block policy spend refusal"
                                | "test-only stage block policy reserve refusal"
                        )
                    );
                assert_eq!(value["actual_injected_refusal"], injected);
                if phase == "spend_result" {
                    assert!(value["frames"].is_null());
                    for lane in 0..2 {
                        let expected =
                            if value["error"].is_null() && n(&value["before"]["lane"]) == lane {
                                n(&value["amount"])
                            } else {
                                0
                            };
                        assert_eq!(
                            n(&value["after"]["work"][lane as usize])
                                .checked_sub(n(&value["before"]["work"][lane as usize])),
                            Some(expected)
                        );
                    }
                } else if value["error"].is_null() {
                    let frames = n(&value["layout"][0]).div_ceil(8);
                    assert_eq!(value["frames"], frames);
                    assert_eq!(
                        n(&value["after"]["ledger"]).checked_sub(n(&value["before"]["ledger"])),
                        Some(frames)
                    );
                } else {
                    assert_eq!(value["before"]["ledger"], value["after"]["ledger"]);
                }
            }
            "make_room_before" | "make_room_after" => {
                let before = phase == "make_room_before";
                fields(
                    value,
                    if before {
                        &[
                            "attempt", "eligible", "before", "layout", "frames", "resident",
                        ]
                    } else {
                        &[
                            "attempt",
                            "eligible",
                            "before",
                            "after",
                            "layout",
                            "frames",
                            "resident",
                            "host_work_delta",
                            "host_native_crosscheck",
                            "error",
                        ]
                    },
                );
                boundary(&value["before"]);
                resident(&value["resident"]);
                value["eligible"].as_bool().unwrap();
                n(&value["attempt"]);
                let layout = pair(&value["layout"]);
                assert!(layout[1] > 0 && layout[1].is_power_of_two());
                let key = (
                    "reserve".to_owned(),
                    ticket.unwrap_or(0),
                    n(&value["attempt"]),
                );
                let attempt = pending.get(&key).expect("make_room inside actual reserve");
                for field in ["attempt", "eligible", "before", "layout"] {
                    assert_eq!(value[field], attempt[field]);
                }
                if before {
                    assert!(rooms.insert(key.clone(), value.clone()).is_none());
                } else {
                    let start = rooms
                        .remove(&key)
                        .expect("matching actual make_room before");
                    for field in ["attempt", "eligible", "before", "layout", "frames"] {
                        assert_eq!(start[field], value[field]);
                    }
                    assert!(completed_rooms.insert(key));
                }
                assert_eq!(value["frames"], n(&value["layout"][0]).div_ceil(8));
                assert_eq!(value["frames"], value["resident"]["pending_additional"]);
                if !before {
                    same_epochs(&value["before"], &value["after"]);
                    assert_eq!(value["host_native_crosscheck"], true);
                    for lane in 0..2 {
                        let host = n(&value["after"]["host_units"][lane])
                            - n(&value["before"]["host_units"][lane]);
                        let native = n(&value["after"]["ops"]["lanes"][lane][7])
                            - n(&value["before"]["ops"]["lanes"][lane][7]);
                        assert_eq!(value["host_work_delta"][lane], host + native);
                        assert_eq!(
                            n(&value["after"]["work"][lane]) - n(&value["before"]["work"][lane]),
                            host + native
                        );
                        for phase in 0..7 {
                            assert_eq!(
                                value["before"]["ops"]["lanes"][lane][phase],
                                value["after"]["ops"]["lanes"][lane][phase]
                            );
                        }
                    }
                }
            }
            "release_before" | "merge_before" => {
                fields(value, &["before", "frames", "cause", "scope"]);
                boundary(&value["before"]);
                pair(&value["frames"]);
                assert_eq!(value["cause"], "ActualLeaseMutation");
                let key = (phase.split('_').next().unwrap().to_owned(), ticket);
                assert!(mutations.insert(key, value.clone()).is_none());
            }
            "release_after" | "merge_after" => {
                fields(
                    value,
                    &[
                        "before",
                        "after",
                        "frames_before",
                        "frames_after",
                        "equation",
                        "cause",
                    ],
                );
                same_epochs(&value["before"], &value["after"]);
                assert_eq!(value["equation"], true);
                let a = pair(&value["frames_before"]);
                let b = pair(&value["frames_after"]);
                let key = (phase.split('_').next().unwrap().to_owned(), ticket);
                let start = mutations
                    .remove(&key)
                    .expect("matching actual lease mutation before");
                assert_eq!(start["before"], value["before"]);
                assert_eq!(start["frames"], value["frames_before"]);
                if phase == "release_after" {
                    assert_eq!(
                        n(&value["before"]["ledger"]).checked_sub(a[0]),
                        Some(n(&value["after"]["ledger"]))
                    );
                } else {
                    assert_eq!(value["before"]["ledger"], value["after"]["ledger"]);
                    assert_eq!(a[0] + a[1], b[0]);
                    assert_eq!(b[1], 0);
                }
            }
            _ => {
                fields(value, &["boundary", "n_position", "data"]);
                boundary(&value["boundary"]);
                logical_data(
                    phase,
                    &value["data"],
                    ticket.map(|t| &calls[t as usize - 1]),
                );
            }
        }
    }
    assert!(
        pending.is_empty()
            && factories.is_empty()
            && rooms.is_empty()
            && completed_rooms.is_empty()
            && mutations.is_empty()
    );
    assert_eq!(entered.len(), calls.len());
    assert_eq!(v["attempts"], json!(attempts));
    for c in calls {
        validate_call(v, c);
    }
    ambient_n(ambient);
    for row in array(v, "rows") {
        let pos = row["value"].get("n_position");
        if let Some(pos) = pos.filter(|v| !v.is_null()) {
            if let Some(t) = id(&pos["selected_ticket"]) {
                assert!(t > 0 && t <= array(ambient, "calls").len() as u64);
            }
            for frame in array(pos, "active_stack") {
                if let Some(t) = id(&frame["ticket"]) {
                    assert!(t > 0 && t <= array(ambient, "calls").len() as u64);
                }
            }
        }
    }
}
fn typed(v: &Value) {
    fields(
        v,
        &[
            "kind",
            "message",
            "detail",
            "cancelled",
            "typed_payload",
            "error_schema_scope",
        ],
    );
    error_kind(&v["kind"]);
    assert_eq!(
        v["error_schema_scope"],
        "exact outer kind and selected direct payloads; original Debug/display supplemental, no complete recursive typed claim"
    );
    for key in ["message", "detail"] {
        assert!(v[key].is_string());
    }
    v["cancelled"].as_bool().unwrap();
    assert!(v["typed_payload"].is_object());
}
fn logical_data(phase: &str, data: &Value, call: Option<&Value>) {
    match phase {
        "caller_enter" => {
            fields(data, &["resident", "origin"]);
            resident(&data["resident"]);
            assert_eq!(
                data["origin"],
                "BuildStageBlock only; after DSP read/check and before writer push"
            );
        }
        "caller_exit" | "method_enter" | "factory_begin" | "method_exit_after_owner_cleanup" => {
            fields(data, &[])
        }
        "range_result" => {
            if data["status"] == "Ok" {
                fields(data, &["status", "range"]);
                fields(&data["range"], &["start", "end"]);
                let c = call.unwrap();
                assert_eq!(data["range"]["start"], c["start"]);
                assert_eq!(
                    data["range"]["end"]
                        .as_i64()
                        .unwrap()
                        .checked_sub(c["start"].as_i64().unwrap()),
                    Some(n(&c["sample_len"]) as i64)
                );
            } else {
                fields(data, &["status", "error"]);
                assert_eq!(data["status"], "Err");
                typed(&data["error"]);
            }
        }
        "query_limits_result" => {
            if data["status"] == "Ok" {
                fields(data, &["status", "maximum_work", "maximum_spans"]);
                n(&data["maximum_work"]);
                n(&data["maximum_spans"]);
            } else {
                fields(data, &["status", "error"]);
                assert_eq!(data["status"], "Err");
                typed(&data["error"]);
            }
        }
        "factory_result" => {
            fields(data, &["result", "equations"]);
            let r = &data["result"];
            let eq = &data["equations"];
            fields(
                eq,
                &[
                    "entry",
                    "exit",
                    "accepted_spend",
                    "accepted_host",
                    "work_delta",
                    "factory_equation",
                    "forwarding_equation",
                ],
            );
            same_epochs(&eq["entry"], &eq["exit"]);
            pair(&eq["accepted_spend"]);
            pair(&eq["accepted_host"]);
            pair(&eq["work_delta"]);
            assert_eq!(eq["factory_equation"], true);
            match r["status"].as_str().unwrap() {
                "Ok" => {
                    fields(
                        r,
                        &[
                            "status",
                            "mode",
                            "view",
                            "canonical",
                            "additional",
                            "total",
                            "forwarded",
                        ],
                    );
                    assert_eq!(r["mode"], "Admitted");
                    view(&r["view"]);
                    assert_eq!(r["forwarded"], true);
                    assert_eq!(
                        n(&r["canonical"]).checked_add(n(&r["additional"])),
                        Some(n(&r["total"]))
                    );
                    assert_eq!(r["total"], r["view"]["work"]);
                    assert_eq!(eq["forwarding_equation"], true);
                }
                "Err" => {
                    fields(r, &["status", "error", "query_local_work"]);
                    typed(&r["error"]);
                    assert_eq!(r["query_local_work"], "unexposed failed query");
                    assert!(eq["forwarding_equation"].is_null());
                }
                _ => panic!("unknown O factory result"),
            }
        }
        "owner_borrow_begin" | "owner_borrow_end" | "content_before" | "content_after"
        | "final_check_before" | "final_check_after" | "apply_before" | "apply_after" => {
            fields(data, &["same_owner_and_borrowed_slices", "view"]);
            assert_eq!(data["same_owner_and_borrowed_slices"], true);
            view(&data["view"]);
        }
        "owner_drop_before" | "owner_drop_after" => {
            fields(data, &["scope"]);
            assert_eq!(
                data["scope"],
                "actual boundary around complete policy owner disposal; no native mask handoff"
            );
        }
        "use_point" => {
            fields(data, &["point", "set_real_cancellation"]);
            data["set_real_cancellation"].as_bool().unwrap();
            use_point(&data["point"]);
        }
        "content_index" => {
            fields(data, &["index"]);
            n(&data["index"]);
        }
        "apply_pcm_before" | "apply_pcm_after" => {
            fields(data, &["pcm_bits"]);
            pcm(&data["pcm_bits"], n(&call.unwrap()["sample_len"]));
        }
        "writer_push_before" | "writer_push_after" => {
            fields(
                data,
                &["writer_len", "block_len", "block_capacity", "pcm_bits"],
            );
            n(&data["writer_len"]);
            assert_eq!(data["block_len"], call.unwrap()["sample_len"]);
            assert!(n(&data["block_capacity"]) >= n(&data["block_len"]));
            pcm(&data["pcm_bits"], n(&data["block_len"]));
        }
        "manual_debit_before" => {
            fields(data, &["amount"]);
            n(&data["amount"]);
        }
        "manual_debit_result"
        | "content_result"
        | "final_check_result"
        | "apply_result"
        | "method_result"
        | "writer_push_result" => result(data),
        _ => panic!("unknown O logical phase {phase}"),
    }
}
fn use_point(value: &Value) -> UsePoint {
    match value.as_str() {
        Some("BeforeLimits") => UsePoint::BeforeLimits,
        Some("FinalCheck") => UsePoint::FinalCheck,
        _ => {
            fields(value, &["ContentBefore"]);
            UsePoint::ContentBefore(usize::try_from(n(&value["ContentBefore"])).unwrap())
        }
    }
}
fn validate_call(v: &Value, c: &Value) {
    let t = &c["ordinal"];
    let enter = one(v, t, "caller_enter");
    let method = one(v, t, "method_enter");
    let exit = one(v, t, "method_exit_after_owner_cleanup");
    let caller_exit = one(v, t, "caller_exit");
    assert_eq!(enter["value"]["boundary"], c["entry"]);
    assert_eq!(method["value"]["boundary"], c["method_entry"]);
    assert_eq!(exit["value"]["boundary"], c["method_exit"]);
    assert_eq!(caller_exit["value"]["boundary"], c["exit"]);
    assert!(
        n(&enter["ordinal"]) < n(&method["ordinal"])
            && n(&method["ordinal"]) < n(&exit["ordinal"])
            && n(&exit["ordinal"]) < n(&caller_exit["ordinal"])
    );
    let outcome = one(v, t, "method_result");
    assert!(n(&outcome["ordinal"]) < n(&exit["ordinal"]));
    let factories = call_rows(v, t, "factory_result");
    assert_eq!(factories.len(), usize::from(c["factory_done"] == true));
    if let Some(row) = factories.first() {
        let begin = one(v, t, "factory_begin");
        assert!(n(&begin["ordinal"]) < n(&row["ordinal"]));
        let eq = &row["value"]["data"]["equations"];
        assert_eq!(eq["entry"], c["factory_entry"]);
        let mut spend = [0u64; 2];
        let mut host = [0u64; 2];
        for r in array(v, "rows")
            .iter()
            .filter(|r| r["ticket"] == *t && r["value"]["eligible"] == true)
        {
            let x = &r["value"];
            if r["phase"] == "spend_result" && x["error"].is_null() {
                spend[n(&x["before"]["lane"]) as usize] += n(&x["amount"]);
            }
            if r["phase"] == "make_room_after" {
                for lane in 0..2 {
                    host[lane] += n(&x["host_work_delta"][lane]);
                }
            }
        }
        assert_eq!(c["accepted_spend"], json!(spend));
        assert_eq!(c["accepted_host"], json!(host));
        assert_eq!(eq["accepted_spend"], json!(spend));
        assert_eq!(eq["accepted_host"], json!(host));
        for lane in 0..2 {
            assert_eq!(
                n(&eq["exit"]["work"][lane]).checked_sub(n(&eq["entry"]["work"][lane])),
                Some(spend[lane] + host[lane])
            );
            assert_eq!(eq["work_delta"][lane], spend[lane] + host[lane]);
        }
        let result = &row["value"]["data"]["result"];
        if result["status"] == "Ok" {
            assert_eq!(spend[0] + spend[1], n(&result["total"]));
            assert!(call_rows(v, t, "manual_debit_before").is_empty());
            assert!(
                call_rows(v, t, "manual_debit_result").is_empty(),
                "no legacy debit after forwarding"
            );
            let owner_begin = one(v, t, "owner_borrow_begin");
            let owner_end = one(v, t, "owner_borrow_end");
            assert!(
                n(&row["ordinal"]) < n(&owner_begin["ordinal"])
                    && n(&owner_begin["ordinal"]) < n(&owner_end["ordinal"])
            );
            assert!(
                n(&owner_end["ordinal"]) < n(&outcome["ordinal"]),
                "borrow ends before method result on success or error"
            );
            assert_eq!(owner_begin["value"]["boundary"]["work"], eq["exit"]["work"]);
            for use_row in array(v, "rows")
                .iter()
                .filter(|r| r["ticket"] == *t && r["value"]["data"].get("view").is_some())
            {
                assert_eq!(use_row["value"]["data"]["view"], result["view"]);
                assert_eq!(
                    use_row["value"]["data"]["same_owner_and_borrowed_slices"],
                    true
                );
                assert!(
                    n(&use_row["ordinal"]) >= n(&owner_begin["ordinal"])
                        && n(&use_row["ordinal"]) <= n(&owner_end["ordinal"])
                );
            }
            let contents = call_rows(v, t, "content_index");
            for (index, r) in contents.iter().enumerate() {
                assert_eq!(r["value"]["data"]["index"], index);
            }
            assert!(contents.len() as u64 <= n(&result["view"]["contents_len"]));
            assert_eq!(call_rows(v, t, "content_before").len(), contents.len());
            assert_eq!(call_rows(v, t, "content_result").len(), contents.len());
            assert_eq!(call_rows(v, t, "content_after").len(), contents.len());
            let before_content = call_rows(v, t, "content_before");
            let results = call_rows(v, t, "content_result");
            let after_content = call_rows(v, t, "content_after");
            for index in 0..contents.len() {
                assert!(
                    n(&before_content[index]["ordinal"]) < n(&contents[index]["ordinal"])
                        && n(&contents[index]["ordinal"]) < n(&results[index]["ordinal"])
                        && n(&results[index]["ordinal"]) < n(&after_content[index]["ordinal"])
                );
                if index + 1 < contents.len() {
                    assert!(
                        n(&after_content[index]["ordinal"])
                            < n(&before_content[index + 1]["ordinal"])
                    );
                }
                if results[index]["value"]["data"]["status"] == "Err" {
                    assert_eq!(index + 1, contents.len());
                    assert_eq!(outcome["value"]["data"]["status"], "Err");
                }
            }
            let checks = call_rows(v, t, "final_check_result");
            if let Some(check) = checks.first() {
                assert_eq!(checks.len(), 1);
                let before = one(v, t, "final_check_before");
                let after = one(v, t, "final_check_after");
                assert!(
                    n(&before["ordinal"]) < n(&check["ordinal"])
                        && n(&check["ordinal"]) < n(&after["ordinal"])
                        && n(&after["ordinal"]) < n(&owner_end["ordinal"])
                );
            }
            if outcome["value"]["data"]["status"] == "Ok" {
                assert_eq!(contents.len() as u64, n(&result["view"]["contents_len"]));
                let before = one(v, t, "owner_drop_before");
                let after = one(v, t, "owner_drop_after");
                assert!(
                    n(&owner_end["ordinal"]) < n(&before["ordinal"])
                        && n(&before["ordinal"]) < n(&after["ordinal"])
                );
                assert!(n(&after["ordinal"]) < n(&outcome["ordinal"]));
                assert_eq!(
                    after["value"]["boundary"]["ledger"],
                    c["method_entry"]["ledger"]
                );
                let check = one(v, t, "final_check_result");
                let apply = one(v, t, "apply_result");
                assert_eq!(check["value"]["data"]["status"], "Ok");
                assert_eq!(apply["value"]["data"]["status"], "Ok");
                assert!(
                    n(&check["ordinal"]) < n(&apply["ordinal"])
                        && n(&apply["ordinal"]) < n(&owner_end["ordinal"])
                );
            }
        } else {
            assert!(call_rows(v, t, "owner_borrow_begin").is_empty());
        }
    } else {
        assert!(c["factory_entry"].is_null());
        assert_eq!(c["accepted_spend"], json!([0, 0]));
        assert_eq!(c["accepted_host"], json!([0, 0]));
    }
    let writer = call_rows(v, t, "writer_push_before");
    if outcome["value"]["data"]["status"] == "Ok" {
        assert_eq!(writer.len(), 1);
        let before = writer[0];
        let after = one(v, t, "writer_push_after");
        let result = one(v, t, "writer_push_result");
        assert!(
            n(&exit["ordinal"]) < n(&before["ordinal"])
                && n(&before["ordinal"]) < n(&result["ordinal"])
                && n(&result["ordinal"]) < n(&after["ordinal"])
                && n(&after["ordinal"]) < n(&caller_exit["ordinal"])
        );
        assert_eq!(
            before["value"]["boundary"]["ledger"],
            c["method_entry"]["ledger"]
        );
        assert_eq!(
            before["value"]["data"]["pcm_bits"],
            one(v, t, "apply_pcm_after")["value"]["data"]["pcm_bits"]
        );
        assert_eq!(
            before["value"]["data"]["pcm_bits"],
            after["value"]["data"]["pcm_bits"]
        );
        if result["value"]["data"]["status"] == "Ok" {
            assert_eq!(
                n(&after["value"]["data"]["writer_len"])
                    .checked_sub(n(&before["value"]["data"]["writer_len"])),
                Some(n(&c["sample_len"]))
            );
        }
    } else {
        assert!(writer.is_empty());
        assert!(call_rows(v, t, "writer_push_result").is_empty());
    }
}
fn ambient_n(v: &Value) {
    fields(
        v,
        &[
            "schema",
            "fault",
            "attempts",
            "invalid",
            "dropped",
            "overflow",
            "active_at_take",
            "pending_handoffs_at_take",
            "unreconciled_transfers",
            "calls",
            "scopes",
            "factory_receipts",
            "use_rows",
            "native_rows",
            "handoff_receipts",
            "account_rows",
            "resident_rows",
            "request_rows",
        ],
    );
    assert_eq!(v["schema"], "read-signal-policy-native-v1");
    assert_eq!(v["fault"], "None");
    assert_eq!(v["invalid"], false);
    assert_eq!(v["overflow"], false);
    assert_eq!(v["dropped"], 0);
    for k in [
        "active_at_take",
        "pending_handoffs_at_take",
        "unreconciled_transfers",
    ] {
        assert_eq!(v[k], json!([]));
    }
    assert!(array(v, "request_rows").len() <= 1);
    for scope in array(v, "scopes") {
        assert_eq!(scope["factory_active"], false);
        assert_eq!(scope["entry"]["ledger"], scope["exit"]["ledger"]);
    }
}
fn receipt<'a>(action: &Value, rows: &'a [Value]) -> &'a Value {
    assert_eq!(action["trace"]["schema"], "O-AFTER-action-reference-v1");
    let row = rows
        .iter()
        .find(|r| r["ordinal"] == action["trace"]["ordinal"])
        .unwrap();
    assert_eq!(row["action"], action["action"]);
    assert_eq!(
        array(row, "provider_entries").len() as u64,
        n(&action["provider"]["events_after"]) - n(&action["provider"]["events_before"])
    );
    row
}
fn visit_actions<'a>(v: &'a Value, p: String, out: &mut Vec<(String, &'a Value)>) {
    if v["action"].is_string() && v.get("result").is_some() && v.get("input_before").is_some() {
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
fn logical_provider(v: &Value) -> Value {
    let mut events = Vec::new();
    for e in array(v, "events") {
        events.push(json!({"project":e["project"],"revision":e["revision"],"asset":e["asset"],
            "cancelled":e["cancelled"],"will_refuse":e["will_refuse"],"prepared_index":e["prepared_index"],
            "prepared_matrix":e["prepared_matrix"],"prepared_provenance":e["prepared_provenance"]}));
    }
    json!({"events_before":v["events_before"],"events_after":v["events_after"],
        "source_calls_before":v["source_calls_before"],"source_calls_after":v["source_calls_after"],"events":events})
}
fn same_keys(a: &Value, b: &Value) {
    let left = array(a, "key_transcripts");
    let right = array(b, "key_transcripts");
    assert_eq!(left.len(), right.len());
    for (a, b) in left.iter().zip(right) {
        for key in [
            "ordinal",
            "parent",
            "updates",
            "actual_finish",
            "actual_scan_result",
            "exited",
            "termination",
        ] {
            assert_eq!(a[key], b[key], "literal ordered current key stream {key}");
        }
    }
}
fn compare_policy(old: &Value, now: &Value) {
    let obs = &old["trace"]["observer"];
    let calls = array(obs, "calls")
        .iter()
        .filter(|c| c["selected"] == true)
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), array(now, "calls").len());
    for (a, b) in calls.into_iter().zip(array(now, "calls")) {
        assert_eq!(a["context"]["output"]["kind"], b["input"]);
        assert_eq!(a["context"]["block_start"], b["start"]);
        assert_eq!(a["context"]["block_len"], b["sample_len"]);
        let old_policy = one(obs, &a["ordinal"], "policy_query_result");
        let new_policy = one(now, &b["ordinal"], "factory_result");
        let a = &old_policy["value"];
        let b = &new_policy["value"]["data"]["result"];
        assert_eq!(a["status"], b["status"]);
        if a["status"] == "Ok" {
            for key in ["suppressed", "contents", "lookup"] {
                assert_eq!(a["policy"][key], b["view"][key]);
            }
            assert_eq!(a["policy"]["work"], b["canonical"]);
            assert_eq!(
                one(obs, &old_policy["ticket"], "helper_final_pcm")["value"]["pcm_bits"],
                one(now, &new_policy["ticket"], "apply_pcm_after")["value"]["data"]["pcm_bits"]
            );
        } else {
            assert_eq!(a["error"], b["error"]);
        }
    }
}
fn parent_n_overlap(r: &Value) -> Option<Value> {
    let o = &r["native"];
    let ntrace = &r["ambient_N"];
    for c in array(o, "calls") {
        let entry = one(o, &c["ordinal"], "method_enter");
        let exit = one(o, &c["ordinal"], "method_exit_after_owner_cleanup");
        let start = &entry["value"]["n_position"];
        let end = &exit["value"]["n_position"];
        let Some(parent) = id(&start["selected_ticket"]) else {
            continue;
        };
        let Some(factory) = array(ntrace, "factory_receipts")
            .iter()
            .find(|f| f["ticket"] == parent && f["result"]["status"] == "Ok")
        else {
            continue;
        };
        let Some(before) = array(ntrace, "use_rows").iter().rev().find(|row| {
            row["ticket"] == parent
                && n(&row["ordinal"]) <= n(&start["event"])
                && row["value"].get("view").is_some()
        }) else {
            continue;
        };
        let Some(after) = array(ntrace, "use_rows").iter().find(|row| {
            row["ticket"] == parent
                && n(&row["ordinal"]) >= n(&end["event"])
                && row["value"].get("view").is_some()
        }) else {
            continue;
        };
        let begin = array(ntrace, "use_rows")
            .iter()
            .find(|row| row["ticket"] == parent && row["phase"] == "factory_begin")
            .unwrap();
        let finish = array(ntrace, "use_rows")
            .iter()
            .find(|row| row["ticket"] == parent && row["phase"] == "factory_result")
            .unwrap();
        let mut retained = 0u64;
        for row in array(ntrace, "native_rows").iter().filter(|row| {
            n(&row["ordinal"]) > n(&begin["ordinal"]) && n(&row["ordinal"]) < n(&finish["ordinal"])
        }) {
            if row["kind"] == "ReserveResult" && row["error"].is_null() {
                retained += n(&row["frames"]);
            }
            if row["kind"] == "ReleaseAfter" {
                retained = retained.checked_sub(n(&row["frames"])).unwrap();
            }
        }
        if retained == 0 {
            continue;
        }
        assert_eq!(
            n(&factory["exit"]["ledger"]).checked_sub(n(&factory["entry"]["ledger"])),
            Some(retained)
        );
        assert!(n(&c["method_entry"]["ledger"]) >= n(&factory["entry"]["ledger"]) + retained);
        let positive = call_rows(o, &c["ordinal"], "reserve_result")
            .into_iter()
            .filter(|row| {
                row["value"]["eligible"] == true
                    && row["value"]["error"].is_null()
                    && n(&row["value"]["frames"]) > 0
            })
            .collect::<Vec<_>>();
        if positive.is_empty() {
            continue;
        }
        assert_eq!(before["value"]["view"], after["value"]["view"]);
        assert_eq!(before["value"]["same_local_view_identity"], true);
        assert_eq!(after["value"]["same_local_view_identity"], true);
        return Some(
            json!({"O_call":c,"N_parent_ticket":parent,"N_factory":factory,"N_retained_factory_frames":retained,
            "N_view_before":before,"N_view_after":after,"O_positive_reserves":positive,
            "scope":"actual N parent query retained around O allocation and cleanup; no O recursion or post-Work identity"}),
        );
    }
    None
}

fn fault_shape(v: &Value) {
    if v == "None" {
        return;
    }
    let entries = v.as_object().unwrap();
    assert_eq!(entries.len(), 1);
    let (kind, value) = entries.iter().next().unwrap();
    match kind.as_str() {
        "RefuseSpend" | "RefuseReserve" | "CancelBeforeSpend" | "CancelBeforeReserve" => {
            assert!(n(value) > 0)
        }
        "CancelAtUse" => {
            use_point(value);
        }
        "CancelAtCallUse" => {
            let pair = value.as_array().unwrap();
            assert_eq!(pair.len(), 2);
            assert!(n(&pair[0]) > 0);
            use_point(&pair[1]);
        }
        _ => panic!("unknown O fault"),
    }
}
fn capture_all() -> Value {
    let guard = collection(Mode::All);
    let fixture = super::super::super::family().expect("actual registered WAV fixture");
    let rows = receipts();
    drop(guard);
    let mut body = object([
        ("schema", json!("stage-block-policy-after-fixture-v1")),
        ("fixture", fixture),
        ("receipts", Value::Array(rows)),
    ]);
    body["current_key_proof"] = super::key_replay::validate_body(&body);
    body
}
fn paired() -> Value {
    let ordinary = super::capture_body();
    let frozen: Value = serde_json::from_slice(include_bytes!(
        "../tests/fixtures/stage-block-policy-baseline-v1/corpus.json"
    ))
    .unwrap();
    let legacy_relation = super::key_replay::assert_relation(&frozen, &ordinary);
    let current = capture_all();
    let mut before = Vec::new();
    let mut after = Vec::new();
    visit_actions(&ordinary["actual"], String::new(), &mut before);
    visit_actions(&current["fixture"], String::new(), &mut after);
    assert_eq!(before.len(), after.len());
    assert_eq!(before.len(), array(&current, "receipts").len());
    let mut joined = Vec::new();
    let mut modes = BTreeSet::new();
    let mut lanes = [false; 2];
    let mut zero_suppressed = false;
    for ((p, a), (q, b)) in before.iter().zip(after) {
        assert_eq!(p, &q);
        for key in [
            "action",
            "entry",
            "input_before",
            "input_after",
            "result",
            "monitor_before",
            "monitor_after",
        ] {
            assert_eq!(a[key], b[key], "complete paired field {p}/{key}");
        }
        for key in ["start", "frames", "depth"] {
            assert_eq!(a.get(key), b.get(key));
        }
        assert_eq!(b["input_before"], b["input_after"]);
        assert_eq!(
            logical_provider(&a["provider"]),
            logical_provider(&b["provider"])
        );
        let row = receipt(b, array(&current, "receipts"));
        compare_policy(a, &row["native"]);
        same_keys(&a["trace"]["keys"], &row["keys"]);
        for call in array(&row["native"], "calls") {
            modes.insert(call["input"].as_str().unwrap().to_owned());
            if let Some(factory) =
                call_rows(&row["native"], &call["ordinal"], "factory_result").first()
            {
                let result = &factory["value"]["data"]["result"];
                if result["status"] == "Ok" && result["view"]["suppressed_len"] == 0 {
                    assert!(n(&result["canonical"]) > 0);
                    zero_suppressed = true;
                }
            }
        }
        for callback in array(&row["native"], "rows").iter().filter(|r| {
            r["phase"] == "spend_result"
                && r["value"]["eligible"] == true
                && r["value"]["error"].is_null()
                && n(&r["value"]["amount"]) > 0
        }) {
            lanes[n(&callback["value"]["before"]["lane"]) as usize] = true;
        }
        joined.push(json!({"path":p,"receipt":row["ordinal"],"selected_calls":array(&row["native"],"calls").len(),
            "result":b["result"]["status"]}));
    }
    assert_eq!(
        modes,
        BTreeSet::from(["Signal".to_owned(), "PreserveTape".to_owned()])
    );
    assert_eq!(lanes, [true, true]);
    assert!(zero_suppressed);
    for path in [
        "/families/nested_public_Signal/warm",
        "/families/persistent_nested/fresh_renderer",
        "/families/streamed_Signal/second/action",
        "/families/projected_PreserveTape/second/action",
        "/families/signed_fractional_Tape_attempt",
        "/families/complete_Source_route_control/cold",
        "/families/complete_Source_route_control/warm",
    ] {
        let action = current["fixture"].pointer(path).unwrap();
        assert!(
            array(
                &receipt(action, array(&current, "receipts"))["native"],
                "calls"
            )
            .is_empty(),
            "actual bypass {path}"
        );
    }
    let bound = &current["fixture"]["families"]["genuine_Bound_attempt"];
    let bound_calls = array(
        &receipt(bound, array(&current, "receipts"))["native"],
        "calls",
    );
    assert_eq!(bound_calls.len(), 1);
    assert_eq!(bound_calls[0]["input"], "Signal");
    for family in ["streamed_Signal", "projected_PreserveTape"] {
        for field in [
            "same_actual_prepared_arc",
            "prepared_after_ReadWork_drop",
            "prepared_after_controller_drop",
        ] {
            assert_eq!(
                ordinary["actual"]["families"][family][field],
                current["fixture"]["families"][family][field]
            );
        }
    }
    assert_eq!(
        ordinary["actual"]["provenance"],
        current["fixture"]["provenance"]
    );
    assert_eq!(
        current["fixture"]["projection_before"],
        current["fixture"]["projection_after"]
    );
    let overlap = array(&current, "receipts")
        .iter()
        .find_map(parent_n_overlap)
        .expect("real positive N parent owner remains through positive O allocation and cleanup");
    mutations(array(&current, "receipts"));
    json!({"ordinary":ordinary,"legacy_key_relation":legacy_relation,"current":current,"joined":joined,
        "actual_parent_N_overlap":overlap,"positive_modes":modes,"positive_lanes":lanes,
        "zero_suppression_inventory_has_positive_canonical_work":zero_suppressed,
        "fully_empty_policy_inventory":"not established by frozen fixture; no fabricated witness"})
}
fn focused_read(
    plan: &Arc<RenderPlan>,
    definition: &AudioDefinition<'_>,
    inner: NativeProvider,
    cache: Option<PcmCache>,
    fault: Fault,
    name: &str,
) -> Value {
    let _ops = mask_observations();
    let (mut renderer, monitor) =
        super::super::renderer(plan, StageLimits::default(), cache.clone());
    let mut source = provider(inner, None, false);
    next_fault(fault);
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
    let row = rows.remove(0);
    json!({"action":action,"receipt":row,"controller_after":controller(&renderer),
        "cache_after":cache.as_ref().map(entries)})
}
fn accepted(v: &Value, before: Option<u64>) -> Vec<Value> {
    array(v, "rows")
        .iter()
        .filter(|r| {
            before.is_none_or(|at| n(&r["ordinal"]) < at)
                && matches!(r["phase"].as_str(), Some("spend_result" | "reserve_result"))
                && r["value"]["error"].is_null()
        })
        .map(|r| {
            let x = &r["value"];
            json!({"phase":r["phase"],"ticket":r["ticket"],"request":r["request"],
            "attempt":x["attempt"],"eligible":x["eligible"],"cause":x["cause"],"amount":x["amount"],
            "layout":x["layout"],"frames":x["frames"],"lane":x["before"]["lane"],
            "work_before":x["before"]["work"],"work_after":x["after"]["work"],
            "ledger_before":x["before"]["ledger"],"ledger_after":x["after"]["ledger"]})
        })
        .collect()
}
fn assert_cancelled_error(error: &Value) {
    assert_eq!(error["kind"], "Preparation");
    assert_eq!(error["cancelled"], true);
    assert_eq!(
        error["typed_payload"],
        json!({"variant":"Cancelled","shape":"unit"})
    );
}
fn callback_refusal(baseline: &Value, actual: &Value, target: &Value, cancel: bool) {
    let before = &baseline["receipt"]["native"];
    let now = &actual["receipt"]["native"];
    let phase = if target["phase"] == "spend_attempt" {
        "spend_result"
    } else {
        "reserve_result"
    };
    let matches = array(now, "rows")
        .iter()
        .filter(|r| {
            r["phase"] == phase
                && r["value"]["eligible"] == true
                && r["value"]["attempt"] == target["value"]["attempt"]
        })
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1);
    let row = matches[0];
    assert_eq!(row["ticket"], target["ticket"]);
    assert_eq!(
        row["value"]["action"],
        if cancel { "Cancel" } else { "Refuse" }
    );
    for key in ["attempt", "eligible", "cause", "amount", "layout", "before"] {
        if key == "before" {
            for f in ["work", "ledger", "lane"] {
                assert_eq!(row["value"][key][f], target["value"][key][f]);
            }
        } else {
            assert_eq!(row["value"][key], target["value"][key]);
        }
    }
    assert_eq!(actual["action"]["result"]["status"], "Error");
    if cancel {
        assert_eq!(actual["action"]["cancelled_at_end"], true);
        assert_cancelled_error(&actual["action"]["result"]["error"]);
        assert_eq!(
            row["value"]["error"],
            json!({"kind":"Preparation","reason":null,"cancelled":true})
        );
        assert_eq!(row["value"]["actual_injected_refusal"], false);
    } else {
        let reason = if phase == "spend_result" {
            "test-only stage block policy spend refusal"
        } else {
            "test-only stage block policy reserve refusal"
        };
        assert_eq!(row["value"]["actual_injected_refusal"], true);
        assert_eq!(row["value"]["error"]["kind"], "Limit");
        assert_eq!(row["value"]["error"]["reason"], reason);
        let outer = &actual["action"]["result"]["error"];
        assert_eq!(outer["kind"], "Limit");
        assert_eq!(outer["cancelled"], false);
        assert_eq!(outer["typed_payload"], json!({"reason":reason}));
    }
    assert_eq!(
        accepted(before, Some(n(&target["ordinal"]))),
        accepted(now, None)
    );
    assert!(
        array(now, "rows")
            .iter()
            .filter(|r| n(&r["ordinal"]) > n(&row["ordinal"]))
            .all(
                |r| !matches!(r["phase"].as_str(), Some("spend_result" | "reserve_result"))
                    || !r["value"]["error"].is_null()
            )
    );
}

fn callback_batches(trace: &Value, targets: &[Value]) -> (Vec<Vec<usize>>, Value) {
    let mut jobs = Vec::new();
    let mut batches = Vec::new();
    let mut factories = Vec::new();
    for call in array(trace, "calls") {
        let indices = targets
            .iter()
            .enumerate()
            .filter(|(_, target)| target["ticket"] == call["ordinal"])
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let first_batch = jobs.len();
        for (local_batch, chunk) in indices.chunks(256).enumerate() {
            assert!(!chunk.is_empty() && chunk.len() <= 256);
            jobs.push(chunk.to_vec());
            batches.push(json!({"schema":"stage-block-policy-after-callback-batch-v1",
                "ordinal":batches.len()+1,"call":call["ordinal"],"factory_batch":local_batch+1,
                "ceiling":256,"target_indices":chunk.iter().map(|&index|index+1).collect::<Vec<_>>(),
                "fault_case_indices":chunk.iter().flat_map(|&index|[index*2+1,index*2+2]).collect::<Vec<_>>()}));
        }
        factories.push(json!({"schema":"stage-block-policy-after-callback-factory-inventory-v1",
            "call":call["ordinal"],"actual_targets":indices.len(),"actual_batches":jobs.len()-first_batch}));
    }
    let indices = jobs.iter().flatten().copied().collect::<Vec<_>>();
    assert_eq!(
        indices,
        (0..targets.len()).collect::<Vec<_>>(),
        "factory batches cover the complete original callback order and multiplicity"
    );
    let covered = indices
        .iter()
        .map(|index| targets[*index].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        covered.as_slice(),
        targets,
        "factory batches preserve every original callback field and global fault ordinal"
    );
    let manifest = json!({"schema":"stage-block-policy-after-callback-coverage-v1",
        "ceiling_per_batch":256,"actual_targets":targets.len(),"actual_batches":jobs.len(),
        "actual_calls":array(trace,"calls").len(),"factories":factories,"batches":batches,
        "inventory_order_and_multiplicity_exact":true,
        "scope":"metadata batches only; each original global spend/reserve ordinal reruns the unchanged complete cold public action; no aggregate JSON memory bound"});
    (jobs, manifest)
}

fn live_case(trace: &Value, target: &Value, actual: &Value) -> Value {
    assert_eq!(actual["action"]["cancelled_at_end"], true);
    let now = &actual["receipt"]["native"];
    let hits = array(now, "rows")
        .iter()
        .filter(|r| {
            r["phase"] == "use_point" && r["value"]["data"]["set_real_cancellation"] == true
        })
        .collect::<Vec<_>>();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["ticket"], target["ticket"]);
    assert_eq!(
        hits[0]["value"]["data"]["point"],
        target["value"]["data"]["point"]
    );
    assert_eq!(
        accepted(trace, Some(n(&target["ordinal"]))),
        accepted(now, None)
    );
    let checks = array(now, "rows")
        .iter()
        .filter(|r| {
            n(&r["ordinal"]) > n(&hits[0]["ordinal"])
                && matches!(
                    r["phase"].as_str(),
                    Some("query_limits_result" | "content_result" | "final_check_result")
                )
                && r["value"]["data"]["status"] == "Err"
        })
        .collect::<Vec<_>>();
    if actual["action"]["result"]["status"] == "Error" {
        assert_cancelled_error(&actual["action"]["result"]["error"]);
        assert!(!checks.is_empty());
        assert_cancelled_error(&checks[0]["value"]["data"]["error"]);
    } else {
        assert_eq!(actual["action"]["result"]["status"], "Ok");
        assert!(
            checks.is_empty(),
            "success only when no existing check observed cancellation"
        );
    }
    checks
        .first()
        .map(|value| (**value).clone())
        .unwrap_or(Value::Null)
}

fn sweep(mut run: impl FnMut(Fault) -> SetupResult<Value>, lane: usize) -> SetupResult<Value> {
    let baseline = run(Fault::None)?;
    assert_eq!(baseline["action"]["result"]["status"], "Ok");
    let trace = &baseline["receipt"]["native"];
    let targets = array(trace, "rows")
        .iter()
        .filter(|r| {
            r["value"]["eligible"] == true
                && matches!(
                    r["phase"].as_str(),
                    Some("spend_attempt" | "reserve_attempt")
                )
        })
        .cloned()
        .collect::<Vec<_>>();
    assert!(!targets.is_empty(), "actual selected callback inventory");
    let (callback_jobs, mut callback_manifest) = callback_batches(trace, &targets);
    assert!(targets.iter().any(|r| r["phase"] == "spend_attempt"
        && r["value"]["before"]["lane"] == lane
        && n(&r["value"]["amount"]) > 0));
    assert!(targets.iter().any(|r| r["phase"] == "reserve_attempt"
        && r["value"]["before"]["lane"] == lane
        && n(&r["value"]["layout"][0]) > 0));
    let mut failures = Vec::new();
    for batch in &callback_jobs {
        assert!(!batch.is_empty() && batch.len() <= 256);
        for &target_index in batch {
            let target = &targets[target_index];
            let ordinal = usize::try_from(n(&target["value"]["attempt"])).unwrap();
            for cancel in [false, true] {
                let fault = match (target["phase"].as_str().unwrap(), cancel) {
                    ("spend_attempt", false) => Fault::RefuseSpend(ordinal),
                    ("reserve_attempt", false) => Fault::RefuseReserve(ordinal),
                    ("spend_attempt", true) => Fault::CancelBeforeSpend(ordinal),
                    ("reserve_attempt", true) => Fault::CancelBeforeReserve(ordinal),
                    _ => unreachable!(),
                };
                let actual = run(fault)?;
                callback_refusal(&baseline, &actual, target, cancel);
                let raw = object([
                    ("target", target.clone()),
                    ("cancel", json!(cancel)),
                    ("actual", actual),
                ]);
                failures.push(journal::append("callback", raw, |raw| {
                    fields(raw, &["target", "cancel", "actual"]);
                    assert_eq!(&raw["target"], target);
                    assert_eq!(raw["cancel"], cancel);
                    callback_refusal(&baseline, &raw["actual"], target, cancel);
                    validate_journal_case(raw);
                }));
            }
        }
    }
    assert_eq!(failures.len(), targets.len() * 2);
    let executed = failures
        .chunks_exact(2)
        .map(|cases| {
            let target = journal::read_case(&cases[0], |raw| {
                validate_journal_case(raw);
                assert_eq!(raw["cancel"], false);
                raw["target"].clone()
            });
            journal::read_case(&cases[1], |raw| {
                validate_journal_case(raw);
                assert_eq!(raw["cancel"], true);
                assert_eq!(raw["target"], target);
            });
            target
        })
        .collect::<Vec<_>>();
    assert_eq!(
        executed.as_slice(),
        targets.as_slice(),
        "every original target executes both fault variants in original order"
    );
    callback_manifest["executed_fault_cases"] = json!(failures.len());
    callback_manifest["fault_variants_per_target"] = json!(2);
    callback_manifest["execution_order_and_multiplicity_exact"] = json!(true);
    let uses = array(trace, "rows")
        .iter()
        .filter(|r| r["phase"] == "use_point")
        .cloned()
        .collect::<Vec<_>>();
    assert!(!uses.is_empty() && uses.len() <= 256);
    let mut cancellation = Vec::new();
    for target in uses {
        let point = use_point(&target["value"]["data"]["point"]);
        let actual = run(Fault::CancelAtCallUse(
            usize::try_from(n(&target["ticket"])).unwrap(),
            point,
        ))?;
        let first_check = live_case(trace, &target, &actual);
        let raw = object([
            ("target", target.clone()),
            ("actual", actual),
            ("first_observed_cancelled_check", first_check),
        ]);
        cancellation.push(journal::append("live_use", raw, |raw| {
            fields(raw, &["target", "actual", "first_observed_cancelled_check"]);
            assert_eq!(raw["target"], target);
            assert_eq!(
                raw["first_observed_cancelled_check"],
                live_case(trace, &target, &raw["actual"])
            );
            validate_journal_case(raw);
        }));
    }
    Ok(object([
        ("baseline", baseline),
        ("lane", json!(lane)),
        (
            "all_actual_selected_callback_targets",
            Value::Array(targets),
        ),
        ("callback_batch_manifest", callback_manifest),
        (
            "refusals_and_callback_cancellations",
            Value::Array(failures),
        ),
        (
            "all_actual_selected_live_use_cancellations",
            Value::Array(cancellation),
        ),
        (
            "scope",
            json!(
                "every selected baseline Plan/Cache callback and actual live use point; full raw cases in journal; real checks determine cancellation priority and accepted prefix"
            ),
        ),
    ]))
}
fn mask_snapshot(v: &Result<PublicOutput, StageAudioError>) -> Value {
    match v {
        Ok(PublicOutput::Definition(v)) => n_native::mask_snapshot(&v.suppressed),
        Ok(PublicOutput::Tape(v)) => n_native::mask_snapshot(&v.suppressed),
        Ok(PublicOutput::Routed(v)) => n_native::mask_snapshot(&v.suppressed),
        Err(_) => Value::Null,
    }
}
fn cap_trial(
    plan: &Arc<RenderPlan>,
    holder_definition: &AudioDefinition<'_>,
    cold: &AudioDefinition<'_>,
    inner: NativeProvider,
    cap: u32,
    held: bool,
) -> Value {
    assert!((1..=StageLimits::default().maximum_resident_frames).contains(&cap));
    let _ops = mask_observations();
    let (mut renderer, monitor) = super::super::renderer(plan, StageLimits::default(), None);
    let mut source = provider(inner, None, false);
    let cancel = AtomicBool::new(false);
    let (output, holder) = call_public(
        &mut renderer,
        &mut source,
        PublicTarget::Definition(holder_definition),
        (512, 128),
        "O_resident_actual_N_Hold_holder",
        &monitor,
        &cancel,
    );
    let holder_rows = receipts();
    assert_eq!(holder_rows.len(), 1);
    assert_eq!(holder["result"]["status"], "Ok");
    let mask = mask_snapshot(&output);
    assert_eq!(mask["kind"], "Owned");
    assert!(n(&mask["mask_len"]) > 0);
    assert!(
        array(&holder_rows[0]["native"], "calls").is_empty(),
        "actual untransformed Hold group is not O"
    );
    let before_choice = controller(&renderer);
    let holder_output = output_value(&output);
    let output = if held {
        Some(output)
    } else {
        drop(output);
        None
    };
    let after_choice = controller(&renderer);
    renderer.limits.maximum_resident_frames = cap;
    let measured = public_once(
        &mut renderer,
        &mut source,
        PublicTarget::Definition(cold),
        (0, 256),
        "O_resident_distinct_cold_outer",
        &monitor,
        &cancel,
    );
    let measured_rows = receipts();
    assert_eq!(measured_rows.len(), 1);
    let before_drop = controller(&renderer);
    drop(output);
    let after_drop = controller(&renderer);
    if held {
        assert_eq!(
            n(&before_drop["owned_account"]).checked_sub(n(&after_drop["owned_account"])),
            Some(n(&mask["charge_frames"]))
        );
    }
    let success = measured["result"]["status"] == "Ok";
    if !success {
        assert_eq!(measured["result"]["error"]["kind"], "Limit");
        assert_eq!(
            measured["result"]["error"]["typed_payload"]["reason"],
            "resident PCM or stage cache"
        );
    } else {
        assert!(
            !array(&measured_rows[0]["native"], "calls").is_empty(),
            "fresh target must execute actual O"
        );
    }
    let retry = if held && !success {
        let action = public_once(
            &mut renderer,
            &mut source,
            PublicTarget::Definition(cold),
            (0, 256),
            "O_resident_holder_drop_retry",
            &monitor,
            &cancel,
        );
        let rows = receipts();
        assert_eq!(rows.len(), 1);
        Some(json!({"action":action,"receipt":rows[0],"controller_after":controller(&renderer)}))
    } else {
        None
    };
    assert_eq!(renderer.limits.maximum_resident_frames, cap);
    json!({"cap":cap,"held":held,"holder":holder,"holder_receipt":holder_rows[0],
        "actual_N_holder_mask":mask,"holder_complete_output":holder_output,
        "controller_before_choice":before_choice,"controller_after_choice":after_choice,
        "measured":measured,"measured_receipt":measured_rows[0],"success":success,
        "controller_before_holder_drop":before_drop,"controller_after_holder_drop":after_drop,"retry":retry,
        "scope":"fresh renderer; real positive N Hold output; distinct still-cold outer Stage; legal cap after hold/drop choice; no cache deletion, budget seed, refund or repricing"})
}
fn residency(
    plan: &Arc<RenderPlan>,
    holder: &AudioDefinition<'_>,
    cold: &AudioDefinition<'_>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
) -> SetupResult<Value> {
    // Every sampled ceiling is inside this predeclared legal domain; search has at most 28 fresh trials.
    let maximum = StageLimits::default().maximum_resident_frames;
    let mut lo = 1u32;
    let mut hi = maximum;
    let mut dropped = Vec::new();
    let top = cap_trial(plan, holder, cold, reopen()?, maximum, false);
    assert_eq!(top["success"], true);
    dropped.push(top);
    for _ in 0..25 {
        if lo == hi {
            break;
        }
        let mid = lo + (hi - lo) / 2;
        let row = cap_trial(plan, holder, cold, reopen()?, mid, false);
        if row["success"] == true {
            hi = mid;
        } else {
            lo = mid + 1;
        }
        dropped.push(row);
    }
    assert_eq!(lo, hi, "bounded search resolves legal resident frontier");
    let frontier = cap_trial(plan, holder, cold, reopen()?, lo, false);
    assert_eq!(frontier["success"], true);
    dropped.push(frontier);
    if lo > 1 {
        let below = cap_trial(plan, holder, cold, reopen()?, lo - 1, false);
        assert_eq!(below["success"], false);
        dropped.push(below);
    }
    let held = cap_trial(plan, holder, cold, reopen()?, lo, true);
    let control = cap_trial(plan, holder, cold, reopen()?, lo, false);
    for key in [
        "holder_complete_output",
        "actual_N_holder_mask",
        "controller_before_choice",
    ] {
        assert_eq!(held[key], control[key]);
    }
    assert_eq!(
        held["measured"]["input_before"],
        control["measured"]["input_before"]
    );
    let retry = &held["retry"];
    let separated = held["success"] == false
        && control["success"] == true
        && !array(&control["measured_receipt"]["native"], "calls").is_empty()
        && retry["action"]["result"]["status"] == "Ok"
        && !array(&retry["receipt"]["native"], "calls").is_empty();
    let natural = dropped
        .iter()
        .chain([&held, &control])
        .flat_map(|trial| {
            array(&trial["measured_receipt"]["native"], "rows")
                .iter()
                .filter(|row| {
                    row["phase"] == "make_room_after"
                        && row["value"]["eligible"] == true
                        && row["value"]["error"]["kind"] == "Limit"
                })
                .map(|row| json!({"cap":trial["cap"],"held":trial["held"],"row":row}))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut sampled = Vec::new();
    let mut candidates = BTreeMap::<u32, Vec<Value>>::new();
    let mut origin_inventory = Vec::new();
    for row in array(&control["measured_receipt"]["native"], "rows")
        .iter()
        .filter(|row| row["phase"] == "make_room_before" && row["value"]["eligible"] == true)
    {
        let resident = &row["value"]["resident"];
        let threshold = n(&resident["sum_before_pending"])
            .checked_add(n(&resident["pending_additional"]))
            .unwrap();
        if threshold > 1 && threshold <= u64::from(maximum) {
            let cap = u32::try_from(threshold - 1).unwrap();
            let origin = json!({"schema":"stage-block-policy-after-reserve-frontier-origin-v1",
                "source_index":origin_inventory.len()+1,"cap":cap,"threshold":threshold,"row":row});
            origin_inventory.push(origin.clone());
            candidates.entry(cap).or_default().push(origin);
        }
    }
    let candidate_caps = candidates.keys().copied().collect::<Vec<_>>();
    let mut frontier_batches = Vec::new();
    let mut executed_caps = Vec::new();
    for (batch_index, caps) in candidate_caps.chunks(32).enumerate() {
        assert!(!caps.is_empty() && caps.len() <= 32);
        let first_trial = sampled.len() + 1;
        for &cap in caps {
            let trial = cap_trial(plan, holder, cold, reopen()?, cap, false);
            assert_eq!(trial["cap"], cap);
            sampled.push(trial);
            executed_caps.push(cap);
        }
        frontier_batches.push(
            json!({"schema":"stage-block-policy-after-reserve-frontier-batch-v1",
            "ordinal":batch_index+1,"ceiling":32,"caps":caps,
            "trial_indices":(first_trial..=sampled.len()).collect::<Vec<_>>()}),
        );
    }
    assert_eq!(
        executed_caps, candidate_caps,
        "all distinct legal reserve candidates execute once in sorted order"
    );
    let mut covered_origins = candidates.values().flatten().cloned().collect::<Vec<_>>();
    covered_origins.sort_by_key(|origin| n(&origin["source_index"]));
    assert_eq!(
        covered_origins, origin_inventory,
        "candidate aliases retain every original source row in original order and multiplicity"
    );
    let aliases = candidates.iter().map(|(cap, origins)| json!({
        "schema":"stage-block-policy-after-reserve-frontier-aliases-v1","cap":cap,
        "origin_indices":origins.iter().map(|origin|n(&origin["source_index"])).collect::<Vec<_>>()
    })).collect::<Vec<_>>();
    let frontier_manifest = json!({"schema":"stage-block-policy-after-reserve-frontier-coverage-v1",
        "ceiling_per_batch":32,"actual_distinct_candidates":candidate_caps.len(),
        "actual_origin_rows":origin_inventory.len(),"actual_trials":sampled.len(),
        "sorted_caps":candidate_caps,"origins":origin_inventory,"aliases":aliases,
        "batches":frontier_batches,"trial_order_and_multiplicity_exact":true,
        "origin_order_and_multiplicity_exact":true,
        "scope":"metadata batching only; every distinct legal cap executes the unchanged fresh full trial once; complete aliases retained"});
    let mut natural = natural;
    for trial in &sampled {
        for row in array(&trial["measured_receipt"]["native"], "rows")
            .iter()
            .filter(|row| {
                row["phase"] == "make_room_after"
                    && row["value"]["eligible"] == true
                    && row["value"]["error"]["kind"] == "Limit"
            })
        {
            natural.push(json!({"cap":trial["cap"],"held":trial["held"],"row":row}));
        }
    }
    #[path = "stage_block_policy_resident_diagnostic.rs"]
    mod resident_diagnostic;
    resident_diagnostic::write(resident_diagnostic::Snapshot {
        predeclared_legal_cap_domain: [1, maximum],
        maximum_binary_trials: 28,
        dropped_frontier: lo,
        dropped_trials: &dropped,
        held_trial: &held,
        matched_drop_control: &control,
        actual_reserve_threshold_trials: &sampled,
        reserve_frontier_batch_manifest: &frontier_manifest,
        selected_o_natural_resident_refusals: &natural,
        separating_held_n_drop_retry: separated,
    })?;
    assert!(
        separated,
        "actual held N output must separate the complete cold public request and recover after its drop"
    );
    assert!(
        natural.is_empty(),
        "this public fixture refuses below its earlier admission floor, before selected O"
    );
    assert!(
        array(&held["measured_receipt"]["native"], "calls").is_empty(),
        "the matched held public request fails before O; recovery is whole-request evidence"
    );
    Ok(
        json!({"predeclared_legal_cap_domain":[1,maximum],"maximum_binary_trials":28,
        "reserve_frontier_batch_manifest":frontier_manifest,
        "dropped_trials":dropped,"dropped_frontier":lo,
        "held_trial":held,"matched_drop_control":control,"actual_reserve_threshold_trials":sampled,
        "selected_O_natural_resident_refusals":natural,"separating_held_N_drop_retry":separated,
        "cold_retry_proof":"retry has real O calls, so failed target did not become a completed reusable Stage",
        "scope":"public whole-request held N/drop retry plus complete legal frontier samples; no selected O natural refusal in this public fixture; no physical RSS or cache-internal identity claim"}),
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
        let mut fixture = super::fixture_cases(plan, base, asset, wire, reopen, provenance);
        fixture["residency"] =
            json!("actual O native leases and separate ambient current N receipts");
        fixture["faults"] = json!(
            "unchanged seven real early controls; selected AFTER callback faults run separately"
        );
        if saved_key_output_policy::active() {
            fixture = saved_key_output_policy::augment(fixture, plan, base, asset, wire, reopen);
        }
        return Ok(Some(fixture));
    }
    if mode == Mode::Residency {
        #[path = "stage_block_policy_projected_diagnostic.rs"]
        mod projected_diagnostic;
        projected_diagnostic::run_if_requested(plan, base, reopen)?;
    }
    let (plain, document) = plain_nested_plan(asset, wire)?;
    let nested = plain.audio_definition(AudioDefinitionSelector::Node {
        node: super::id("outer"),
    })?;
    let result = match mode {
        Mode::Plan => sweep(
            |fault| {
                Ok(focused_read(
                    &plain,
                    &nested,
                    reopen()?,
                    None,
                    fault,
                    "O_focused_Plan",
                ))
            },
            0,
        )?,
        Mode::Cache => sweep(
            |fault| {
                let directory = tempfile::tempdir()?;
                let cache = PcmCache::at(directory.path())?;
                Ok(focused_read(
                    &plain,
                    &nested,
                    reopen()?,
                    Some(cache),
                    fault,
                    "O_focused_cold_Cache",
                ))
            },
            1,
        )?,
        Mode::Residency => {
            let holder = plain.audio_definition(AudioDefinitionSelector::Node {
                node: super::id("group"),
            })?;
            let public = residency(&plain, &holder, &nested, reopen)?;
            #[path = "stage_block_policy_projected_residency.rs"]
            mod projected_residency;
            let private = projected_residency::run(plan, base, reopen)?;
            object([
                ("public_complete_request_held_N_drop_retry", public),
                ("private_projected_natural_O_refusal", private),
            ])
        }
        Mode::All => unreachable!(),
    };
    Ok(Some(
        json!({"provenance":provenance,"actual_checked_document":document,"result":result}),
    ))
}
fn focused(mode: Mode) -> Value {
    let _journal = journal::temporary_if_absent();
    let start = journal::mark();
    let guard = collection(mode);
    let fixture = super::super::super::family().expect("actual registered WAV focused fixture");
    assert!(
        receipts().is_empty(),
        "focused runners consume every action receipt"
    );
    drop(guard);
    let keys = journal::group_proof(start, &fixture);
    object([
        (
            "schema",
            json!("stage-block-policy-after-focused-journal-v1"),
        ),
        ("fixture", fixture),
        ("current_key_proof", keys),
    ])
}
fn mutations(rows: &[Value]) {
    let row = rows
        .iter()
        .find(|r| !array(&r["native"], "calls").is_empty())
        .unwrap();
    let base = &row["native"];
    let ambient = &row["ambient_N"];
    valid(base, ambient);
    let mut cases = vec![
        (
            "derived_payload",
            "/retained_payload_bytes".to_owned(),
            json!(n(&base["retained_payload_bytes"]) + 1),
        ),
        ("closed_bounds", "/bounds/calls".to_owned(), json!(4097)),
        ("call_request", "/calls/0/request".to_owned(), json!(2)),
        (
            "call_parent_type",
            "/calls/0/parent".to_owned(),
            json!("invalid"),
        ),
        (
            "completed_factory",
            "/calls/0/factory_active".to_owned(),
            json!(true),
        ),
        (
            "call_ledger_cleanup",
            "/calls/0/method_exit/ledger".to_owned(),
            json!(n(&base["calls"][0]["method_exit"]["ledger"]) + 1),
        ),
    ];
    let owner = array(base, "rows")
        .iter()
        .position(|r| r["phase"] == "owner_borrow_begin")
        .unwrap();
    cases.push((
        "actual_owner_identity",
        format!("/rows/{owner}/value/data/same_owner_and_borrowed_slices"),
        json!(false),
    ));
    let reserve = array(base, "rows")
        .iter()
        .position(|r| r["phase"] == "reserve_result" && r["value"]["error"].is_null())
        .unwrap();
    cases.push((
        "layout_lease_equation",
        format!("/rows/{reserve}/value/frames"),
        json!(n(&base["rows"][reserve]["value"]["frames"]) + 1),
    ));
    let factory = array(base, "rows")
        .iter()
        .position(|r| {
            r["phase"] == "factory_result" && r["value"]["data"]["result"]["status"] == "Ok"
        })
        .unwrap();
    cases.push((
        "query_spend_once",
        format!("/rows/{factory}/value/data/result/total"),
        json!(n(&base["rows"][factory]["value"]["data"]["result"]["total"]) + 1),
    ));
    cases.push((
        "actual_frame_clock",
        format!("/rows/{owner}/position/last_ordinal"),
        json!(0),
    ));
    for (name, path, replacement) in cases {
        let mut changed = base.clone();
        assert_ne!(changed.pointer(&path).unwrap(), &replacement);
        *changed.pointer_mut(&path).unwrap() = replacement;
        if name != "derived_payload" {
            changed["retained_payload_bytes"] = json!(retained_bytes(&changed));
        }
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| valid(&changed, ambient)))
                .is_err(),
            "native schema mutation {name}"
        );
    }
    let mut changed = base.clone();
    changed["unexpected"] = json!(true);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| valid(&changed, ambient)))
            .is_err()
    );
    let mut changed = base.clone();
    changed
        .as_object_mut()
        .unwrap()
        .remove("retained_payload_bytes");
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| valid(&changed, ambient)))
            .is_err()
    );
    valid(base, ambient);
    let serial = rows
        .iter()
        .find(|r| array(&r["native"], "calls").len() >= 2)
        .unwrap();
    let original = &serial["native"];
    let position = array(original, "rows")
        .iter()
        .position(|r| r["ticket"] == 2 && r["phase"] == "apply_pcm_after")
        .unwrap();
    let mut changed = original.clone();
    changed["rows"][position]["ticket"] = json!(1);
    changed["retained_payload_bytes"] = json!(retained_bytes(&changed));
    assert_eq!(
        changed["rows"][position]["position"],
        original["rows"][position]["position"]
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| valid(
            &changed,
            &serial["ambient_N"]
        )))
        .is_err(),
        "call2 PCM cannot be attributed to already-entered call1"
    );
}
fn all_body() -> Value {
    let paired = paired();
    let plan = focused(Mode::Plan);
    let cache = focused(Mode::Cache);
    let residency = focused(Mode::Residency);
    object([
        ("schema", json!("stage-block-policy-after-journal-v1")),
        ("paired", paired),
        ("plan", plan),
        ("cache", cache),
        ("residency", residency),
        (
            "scope",
            json!(
                "BuildStageBlock only; current N/M/L/J; original output-bit scan stays ordinary; no native O mask/account/handoff or whole-preparation admission"
            ),
        ),
    ])
}
#[test]
fn stage_block_policy_current_paired_values_and_real_owner_lifetime() {
    paired();
}
#[test]
fn stage_block_policy_every_real_plan_callback_and_live_use_cancellation() {
    focused(Mode::Plan);
}
#[test]
fn stage_block_policy_every_real_cache_callback_and_live_use_cancellation() {
    focused(Mode::Cache);
}
#[test]
fn stage_block_policy_real_resident_frontier_and_held_n_drop_retry() {
    focused(Mode::Residency);
}
#[test]
#[ignore = "manual fresh admitted O capture; no AFTER oracle"]
fn capture_stage_block_policy_after_v1() {
    let output = std::env::var_os("DEADPAN_STAGE_BLOCK_POLICY_AFTER_OUTPUT").unwrap();
    let output = Path::new(&output);
    assert!(output.is_absolute() && output.starts_with("/tmp"));
    let variant = std::env::var("DEADPAN_STAGE_BLOCK_POLICY_AFTER_VARIANT").unwrap();
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_STAGE_BLOCK_POLICY_AFTER_SOURCE_SHA256").unwrap();
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    assert_eq!(
        std::env::var("DEADPAN_STAGE_BLOCK_POLICY_AFTER_PUBLISHED_CHECKPOINT").unwrap(),
        CHECKPOINT_O
    );
    let _journal = journal::persistent(output);
    let mut body = all_body();
    let manifest = journal::manifest(&body);
    body["evidence_journal"] = manifest.clone();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    body["published_checkpoint"] = json!(CHECKPOINT_O);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &body).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
    writer.get_ref().sync_all().unwrap();
    drop(writer);
    journal::publish(output, &manifest);
}

fn error_kind(value: &Value) {
    assert!(matches!(
        value.as_str(),
        Some(
            "Range"
                | "PreparationRange"
                | "ForeignDomain"
                | "ForeignDefinition"
                | "InvalidLimits"
                | "Limit"
                | "Timeout"
                | "Unsupported"
                | "Preparation"
                | "Plan"
                | "Time"
                | "Gain"
                | "Dsp"
                | "Filter"
                | "Wet"
                | "Transfer"
                | "Cache"
        )
    ));
}
