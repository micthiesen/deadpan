//! Actual M per-call receipts; no capture data is an admission authority.
use super::*;
use crate::stages::stage_validation_policy_native_trace as native;
use native::{Fault, UsePhase};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Paired,
    PlanFaults,
    CacheFaults,
    Residency,
}
struct Collection {
    mode: Mode,
    family: String,
    rows: Vec<Value>,
    active: Option<Vec<Value>>,
    next: Fault,
}
thread_local! {
    static COLLECT:RefCell<Option<Collection>>=const{RefCell::new(None)};
}
struct CollectionGuard;
impl Drop for CollectionGuard {
    fn drop(&mut self) {
        COLLECT.with(|v| {
            v.borrow_mut().take();
        });
    }
}
fn collection(mode: Mode) -> CollectionGuard {
    COLLECT.with(|v| {
        assert!(v.borrow().is_none());
        *v.borrow_mut() = Some(Collection {
            mode,
            family: String::new(),
            rows: Vec::new(),
            active: None,
            next: Fault::None,
        });
    });
    CollectionGuard
}
pub(super) struct Family(Option<String>);
impl Drop for Family {
    fn drop(&mut self) {
        if let Some(old) = self.0.take() {
            COLLECT.with(|v| v.borrow_mut().as_mut().unwrap().family = old);
        }
    }
}
pub(super) fn family_context(name: &str) -> Family {
    Family(COLLECT.with(|v| {
        v.borrow_mut()
            .as_mut()
            .map(|s| std::mem::replace(&mut s.family, name.to_owned()))
    }))
}
pub(super) struct Action {
    guard: Option<native::CaptureGuard>,
    family: Option<String>,
    action: String,
    entry: &'static str,
}
pub(super) fn begin_action(name: &str, entry: &'static str) -> Action {
    let selection = COLLECT.with(|v| {
        let mut v = v.borrow_mut();
        v.as_mut().map(|s| {
            assert!(s.active.is_none());
            s.active = Some(Vec::new());
            (s.family.clone(), std::mem::take(&mut s.next))
        })
    });
    let Some((family, fault)) = selection else {
        return Action {
            guard: None,
            family: None,
            action: String::new(),
            entry,
        };
    };
    original_request::flattened_work_trace::reset();
    Action {
        guard: Some(native::begin(fault)),
        family: Some(family),
        action: name.to_owned(),
        entry,
    }
}
pub(super) fn provider_entry_sidehook() {
    COLLECT.with(|v| {
        if let Some(s) = v.borrow_mut().as_mut()
            && let Some(rows) = s.active.as_mut()
        {
            rows.push(
                json!({"ordinal":rows.len()+1,"native_position":native::position(),
                "scope":"actual provider callback entry, not decoder I/O"}),
            );
        }
    });
}
pub(super) fn end_action(mut token: Action) {
    if let Some(guard) = token.guard.take() {
        let value = native::take();
        drop(guard);
        valid(&value);
        COLLECT.with(|slot|{
            let mut slot=slot.borrow_mut();let s=slot.as_mut().unwrap();
            let provider=s.active.take().unwrap();
            s.rows.push(json!({"ordinal":s.rows.len()+1,"family":token.family,
                "action":token.action,"entry":token.entry,"native":value,"provider_entries":provider}));
        });
    }
}
fn take_rows() -> Vec<Value> {
    COLLECT.with(|v| {
        let mut v = v.borrow_mut();
        let s = v.as_mut().unwrap();
        assert!(s.active.is_none());
        std::mem::take(&mut s.rows)
    })
}
fn fault(value: Fault) {
    COLLECT.with(|v| {
        let mut v = v.borrow_mut();
        let s = v.as_mut().unwrap();
        assert!(s.active.is_none());
        s.next = value;
    });
}
fn n(v: &Value) -> u64 {
    v.as_u64().unwrap()
}
fn native_rows(v: &Value) -> &Vec<Value> {
    v["native_rows"].as_array().unwrap()
}
fn logical(v: &Value) -> &Vec<Value> {
    v["logical_rows"].as_array().unwrap()
}
fn scopes(v: &Value) -> &Vec<Value> {
    v["scopes"].as_array().unwrap()
}
fn admitted_net(v: &Value, scope: &Value, position: u64) -> i128 {
    native_rows(v)
        .iter()
        .filter(|r| r["scope"] == *scope && n(&r["ordinal"]) <= position)
        .map(|r| {
            if r["kind"] == "ReserveResult" && r["error"].is_null() {
                i128::from(n(&r["frames"]))
            } else if r["kind"] == "ReleaseAfter" {
                -i128::from(n(&r["frames"]))
            } else {
                0
            }
        })
        .sum()
}
fn valid(v: &Value) {
    assert_eq!(v["schema"], "stage-validation-native-ledger-v1");
    for key in ["invalid", "overflow", "foreign_request"] {
        assert_eq!(v[key], false, "{key}: {v}");
    }
    assert_eq!(v["dropped"], 0);
    assert_eq!(v["active_scopes"], 0);
    for scope in scopes(v) {
        assert_eq!(scope["request"], 1);
        assert!(
            scope["parent"].is_null(),
            "M owners end before later child preparation"
        );
        assert_eq!(scope["exit"]["ledger"], scope["entry"]["ledger"]);
        assert_eq!(admitted_net(v, &scope["ordinal"], u64::MAX), 0);
    }
    for row in native_rows(v) {
        assert_eq!(row["request"], 1);
        if row["kind"] == "MakeRoomAfter" {
            assert_eq!(row["host_crosscheck"], true);
        }
        if matches!(
            row["kind"].as_str(),
            Some("MakeRoomBefore" | "MakeRoomAfter")
        ) {
            let r = &row["resident"];
            let sum = [
                "cached",
                "active",
                "projected",
                "sound_context",
                "owned_suppression",
            ]
            .iter()
            .map(|k| n(&r[*k]))
            .sum::<u64>();
            assert_eq!(r["sum_before_pending"], sum);
            if row["kind"] == "MakeRoomAfter" && row["error"].is_null() {
                assert!(sum + n(&r["pending_additional"]) <= n(&r["limit"]));
            }
        }
        if matches!(
            row["kind"].as_str(),
            Some("MakeRoomBefore" | "MakeRoomAfter")
        ) || (row["kind"] == "ReserveResult" && row["error"].is_null())
        {
            assert_eq!(n(&row["frames"]), n(&row["layout"][0]).div_ceil(8));
            if row["kind"] == "MakeRoomBefore" {
                assert_eq!(row["resident"]["pending_additional"], row["frames"]);
            }
        }
        if row["kind"] == "ReserveResult" && row["error"].is_null() {
            assert_eq!(
                n(&row["after"]["ledger"]) - n(&row["before"]["ledger"]),
                n(&row["frames"])
            );
        }
        if row["kind"] == "ReleaseAfter" {
            assert_eq!(
                n(&row["before"]["ledger"]) - n(&row["after"]["ledger"]),
                n(&row["frames"])
            );
        }
    }
    for row in logical(v) {
        assert_eq!(row["request"], 1);
        let scope = scopes(v)
            .iter()
            .find(|s| s["ordinal"] == row["scope"])
            .unwrap();
        let net = admitted_net(v, &row["scope"], n(&row["native_position"]));
        assert!(n(&scope["peak"]) >= n(&row["boundary"]["ledger"]));
        assert_eq!(
            i128::from(n(&row["boundary"]["ledger"])),
            i128::from(n(&scope["entry"]["ledger"])) + net
        );
        if row["phase"] == "factory_success" {
            let a = &row["value"];
            assert_eq!(a["forwarded"], true);
            assert_eq!(a["actual_success_equations_exact"], true);
            assert_eq!(n(&a["canonical"]) + n(&a["additional"]), n(&a["total"]));
            assert_eq!(a["total"], a["view"]["work"]);
            let t = &a["native_totals"];
            assert_eq!(t["exact"], true);
            assert_eq!(
                n(&t["accepted_spend"][0]) + n(&t["accepted_spend"][1]),
                n(&a["total"])
            );
            for lane in 0..2 {
                assert_eq!(
                    n(&t["observed"]["work"][lane]) - n(&t["entry"]["work"][lane]),
                    n(&t["accepted_spend"][lane]) + n(&t["accepted_host"][lane])
                );
            }
        }
        if row["phase"] == "factory_error" {
            assert_eq!(row["value"]["native_accepted_prefix"]["exact"], true);
            assert_eq!(
                row["value"]["canonical_additional_split"],
                "unexposed on failed factory"
            );
        }
        if row["phase"] == "use" {
            assert_eq!(row["value"]["same_local_view_identity"], true);
            let factory = logical(v)
                .iter()
                .find(|a| {
                    a["scope"] == row["scope"]
                        && matches!(
                            a["phase"].as_str(),
                            Some("factory_success" | "factory_ordinary")
                        )
                })
                .unwrap();
            assert_eq!(row["value"]["view"], factory["value"]["view"]);
            if native_rows(v).iter().any(|r| {
                r["scope"] == row["scope"]
                    && r["kind"] == "ReserveResult"
                    && r["error"].is_null()
                    && n(&r["frames"]) > 0
                    && n(&r["ordinal"]) <= n(&row["native_position"])
            }) {
                assert!(net > 0, "actual funded query stays live through its use");
            }
        }
        if row["phase"] == "mark" && row["value"]["phase"] == "OwnerDropped" {
            assert_eq!(row["boundary"]["ledger"], scope["entry"]["ledger"]);
            let owner = logical(v)
                .iter()
                .find(|r| {
                    r["scope"] == row["scope"]
                        && matches!(
                            r["phase"].as_str(),
                            Some("factory_success" | "factory_ordinary")
                        )
                })
                .unwrap();
            let uses = logical(v)
                .iter()
                .filter(|r| r["scope"] == row["scope"] && r["phase"] == "use")
                .map(|r| &r["value"]["phase"])
                .collect::<Vec<_>>();
            for phase in [
                "PolicyReady",
                "ProgressBefore",
                "ProgressAfter",
                "IterationComplete",
            ] {
                assert!(
                    uses.contains(&&json!(phase)),
                    "completed monitored owner missing {phase}"
                );
            }
            for i in 0..owner["value"]["view"]["contents"].as_array().unwrap().len() {
                assert!(uses.contains(&&json!({"ContentBefore":i})));
                assert!(uses.contains(&&json!({"ContentAccepted":i})));
            }
            assert!(logical(v).iter().any(|r| r["scope"] == row["scope"]
                && r["phase"] == "mark"
                && r["value"]["phase"] == "OwnerDropBefore"));
        }
    }
}
fn frozen() -> Value {
    serde_json::from_slice(include_bytes!(
        "../tests/fixtures/stage-validation-policy-baseline-v1/corpus.json"
    ))
    .unwrap()
}
fn capture(legacy: bool) -> (Value, Vec<Value>) {
    let _collection = collection(Mode::Paired);
    let body = if legacy {
        crate::stages::stage_validation_policy_request::with_legacy_stage_validation_policy(
            capture_body,
        )
    } else {
        capture_body()
    };
    let rows = take_rows();
    (body, rows)
}
fn visit_actions<'a>(value: &'a Value, p: String, out: &mut Vec<(String, &'a Value)>) {
    if value.get("trace").is_some() && value.get("action").is_some() {
        out.push((p, value));
        return;
    }
    match value {
        Value::Object(v) => {
            for (k, v) in v {
                visit_actions(v, format!("{p}/{k}"), out)
            }
        }
        Value::Array(v) => {
            for (i, v) in v.iter().enumerate() {
                visit_actions(v, format!("{p}/{i}"), out)
            }
        }
        _ => {}
    }
}
fn family_for(p: &str, a: &Value) -> String {
    if p.contains("/structural/") {
        "registered_structural_projected".into()
    } else if p.contains("/qualified_source_input/") {
        "qualified_source_input".into()
    } else if p.contains("/plain_nested/public/") {
        "plain_nonowned_nested_stage".into()
    } else if p.contains("/plain_nested/private/") {
        "plain_stage_first_cached".into()
    } else if p.contains("/projected_private/") {
        "projected_first_memo".into()
    } else if p.contains("/controls/") {
        a["action"].as_str().unwrap().to_owned()
    } else {
        String::new()
    }
}
fn match_receipt<'a>(p: &str, a: &Value, rows: &'a [Value]) -> &'a Value {
    let expected_family = family_for(p, a);
    let matches = rows
        .iter()
        .filter(|r| r["action"] == a["action"] && r["family"] == expected_family)
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "receipt match {p} / {} / {expected_family}",
        a["action"]
    );
    matches[0]
}
fn policy_rows(action: &Value) -> Vec<&Value> {
    action["trace"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["phase"] == "policy_constructed")
        .collect()
}
fn chunk_context(row: &Value) -> &Value {
    &row["position"]["active_stack"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|f| f["kind"] == "chunk")
        .unwrap()["context"]
}
fn caller_id(position: &Value) -> &Value {
    &position["active_stack"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|f| {
            matches!(
                f["kind"].as_str(),
                Some("caller_current_stage" | "caller_projected")
            )
        })
        .unwrap()["ordinal"]
}
fn stream_lifetime(action: &Value, receipt: &Value) {
    let v = &receipt["native"];
    let rows = action["trace"]["rows"].as_array().unwrap();
    for scope in scopes(v) {
        let enter = logical(v)
            .iter()
            .find(|r| r["scope"] == scope["ordinal"] && r["phase"] == "scope_enter")
            .unwrap();
        let position = &enter["value"]["before_trace_position"];
        let caller = caller_id(position);
        if scope["entry"]["lane"] == 1 {
            assert!(
                !position["active_key_streams"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            assert!(
                position["active_stack"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|f| f["kind"] == "build_stage")
                    .count()
                    >= 2
            );
        }
        let chunk = position["active_stack"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|f| f["kind"] == "chunk")
            .unwrap();
        assert_eq!(scope["provider"], chunk["context"]["output"]);
        assert_eq!(scope["start"], chunk["context"]["range"]["start"]);
        assert_eq!(scope["end"], chunk["context"]["range"]["end"]);
        let reserved = rows
            .iter()
            .find(|r| r["phase"] == "stream_reserved" && caller_id(&r["position"]) == caller)
            .unwrap();
        let release_before = rows
            .iter()
            .find(|r| r["phase"] == "stream_release_before" && caller_id(&r["position"]) == caller)
            .unwrap();
        let release_after = rows
            .iter()
            .find(|r| r["phase"] == "stream_release_after" && caller_id(&r["position"]) == caller)
            .unwrap();
        assert!(n(&reserved["ordinal"]) < n(&position["last_attempted_ordinal"]));
        assert!(n(&release_before["ordinal"]) > n(&position["last_attempted_ordinal"]));
        assert_eq!(
            n(&release_before["controller"]["active_frames"])
                - n(&release_after["controller"]["active_frames"]),
            n(&reserved["value"]["resident_frames"])
        );
        assert_eq!(
            reserved["value"]["resident_frames"],
            release_before["value"]["resident_frames"]
        );
        assert_eq!(
            reserved["value"]["disk_bytes"],
            release_before["value"]["disk_bytes"]
        );
        for policy in rows
            .iter()
            .filter(|r| r["phase"] == "policy_constructed" && caller_id(&r["position"]) == caller)
        {
            assert!(
                n(&policy["controller"]["active_frames"])
                    >= n(&reserved["value"]["resident_frames"])
            );
        }
    }
}
fn caller_build_context(position: &Value) -> Value {
    let stack = position["active_stack"].as_array().unwrap();
    let nearest = |kind: &str| stack.iter().rev().find(|r| r["kind"] == kind).unwrap();
    let build = nearest("build_stage");
    let caller = stack
        .iter()
        .rev()
        .find(|r| {
            matches!(
                r["kind"].as_str(),
                Some("caller_current_stage" | "caller_projected")
            )
        })
        .unwrap();
    let validation = nearest("validation");
    let chunk = nearest("chunk");
    assert_eq!(build["parent"], caller["ordinal"]);
    assert_eq!(validation["parent"], build["ordinal"]);
    assert_eq!(chunk["parent"], validation["ordinal"]);
    let frame = |value: &Value| {
        json!({"ordinal":value["ordinal"],"parent":value["parent"],
        "kind":value["kind"],"context":value["context"]})
    };
    json!({"caller":frame(caller),"build":frame(build),"validation":frame(validation),"chunk":frame(chunk),
        "excluded":"attempted row ordinal and changing observation phase are not scope identity"})
}
fn compare_policies(old: &Value, receipt: &Value) {
    let policies = policy_rows(old);
    let v = &receipt["native"];
    let factories = logical(v)
        .iter()
        .filter(|r| r["phase"] == "factory_success")
        .collect::<Vec<_>>();
    assert_eq!(policies.len(), factories.len());
    for (old, new) in policies.iter().zip(factories) {
        let scope = scopes(v)
            .iter()
            .find(|s| s["ordinal"] == new["scope"])
            .unwrap();
        let context = chunk_context(old);
        let entered = logical(v)
            .iter()
            .find(|r| r["scope"] == new["scope"] && r["phase"] == "scope_enter")
            .unwrap();
        assert_eq!(
            caller_build_context(&old["position"]),
            caller_build_context(&entered["value"]["before_trace_position"]),
            "join before-trace scope IDs/parents/full contexts; exclude only changing row position and phase"
        );
        assert_eq!(scope["provider"], context["output"]);
        assert_eq!(scope["role"], "Output");
        assert_eq!(scope["start"], context["range"]["start"]);
        assert_eq!(scope["end"], context["range"]["end"]);
        assert_eq!(scope["depth"], context["depth"]);
        let old = &old["value"];
        let new = &new["value"];
        for k in ["suppressed", "contents", "lookup"] {
            assert_eq!(old[k], new["view"][k]);
        }
        assert_eq!(old["work"], new["canonical"]);
    }
}
fn paired() -> Value {
    let (ordinary, ordinary_native) = capture(true);
    let replay = key_replay::assert_relation(&frozen(), &ordinary);
    let (current, current_native) = capture(false);
    let mut old = Vec::new();
    visit_actions(&ordinary, String::new(), &mut old);
    let mut now = Vec::new();
    visit_actions(&current, String::new(), &mut now);
    assert_eq!(old.len(), now.len());
    assert_eq!(old.len(), current_native.len());
    let mut joined = Vec::new();
    let mut lanes = [false; 2];
    for ((p, a), (q, b)) in old.iter().zip(&now) {
        assert_eq!(p, q);
        let receipt = match_receipt(q, b, &current_native);
        let bounds = &b["provider"];
        assert_eq!(
            receipt["provider_entries"].as_array().unwrap().len() as u64,
            n(&bounds["events_after"]) - n(&bounds["events_before"])
        );
        let quota = p.contains("/old_L2048/measured");
        if a["result"]["status"] == "Ok" && !quota {
            assert_eq!(b["result"]["status"], "Ok", "default regression at {p}");
            assert_eq!(
                a["result"], b["result"],
                "complete output/capacity mismatch {p}"
            );
            compare_policies(a, receipt);
            for scope in scopes(&receipt["native"]) {
                assert!(
                    logical(&receipt["native"])
                        .iter()
                        .any(|r| r["scope"] == scope["ordinal"]
                            && r["phase"] == "mark"
                            && r["value"]["phase"] == "OwnerDropped")
                );
            }
        } else if !quota {
            assert_eq!(
                a["result"], b["result"],
                "complete control error priority changed {p}"
            );
            compare_policies(a, receipt);
        } else if b["result"]["status"] == "Ok" {
            assert_eq!(
                a["result"], b["result"],
                "old2048 may not turn an old failure into unqualified success"
            );
            compare_policies(a, receipt);
        } else {
            assert_eq!(b["result"]["error"]["kind"], "Limit");
            assert_eq!(
                b["result"]["error"]["typed_payload"]["reason"],
                "resident PCM or stage cache"
            );
        }
        for r in native_rows(&receipt["native"]) {
            if r["kind"] == "SpendResult" && r["error"].is_null() && n(&r["amount"]) > 0 {
                lanes[n(&r["before"]["lane"]) as usize] = true;
            }
        }
        joined.push(json!({"path":p,"receipt_ordinal":receipt["ordinal"],"ordinary_status":a["result"]["status"],
            "current_status":b["result"]["status"],"ordinary_policy_count":policy_rows(a).len(),
            "current_successful_factories":logical(&receipt["native"]).iter().filter(|r|r["phase"]=="factory_success").count(),
            "quota_observation_not_equivalence":quota}));
    }
    assert_eq!(
        lanes,
        [true, true],
        "actual Plan and inherited Cache native work required"
    );
    assert_eq!(
        ordinary["actual"]["qualified_source_input"]["projection_before"],
        current["actual"]["qualified_source_input"]["projection_before"]
    );
    assert_eq!(
        current["actual"]["qualified_source_input"]["projection_before"],
        current["actual"]["qualified_source_input"]["projection_after"]
    );
    assert_eq!(
        ordinary["actual"]["projection_before"],
        current["actual"]["projection_before"]
    );
    assert_eq!(
        current["actual"]["projection_before"],
        current["actual"]["projection_after"]
    );
    assert_eq!(
        current["actual"]["plain_nested"]["definition_before"],
        current["actual"]["plain_nested"]["definition_after"]
    );
    json!({"ordinary":ordinary,"ordinary_native":ordinary_native,"current":current,
        "current_native":current_native,"before41_key_relation":replay,"action_joins":joined,
        "scope":"default output parity; literal separate quotas; M per-chunk owners do not persist into later PCM"})
}
fn accepted(v: &Value, before: Option<u64>) -> Vec<Value> {
    native_rows(v).iter().filter(|r|before.is_none_or(|cut|n(&r["ordinal"])<cut))
        .filter(|r|matches!(r["kind"].as_str(),Some("SpendResult"|"ReserveResult"))&&r["error"].is_null())
        .map(|r|json!({"kind":r["kind"],"scope":r["scope"],"attempt":r["attempt"],"amount":r["amount"],
            "layout":r["layout"],"frames":r["frames"],"before":r["before"]["work"],"after":r["after"]["work"]})).collect()
}
fn receipt_only() -> Value {
    let mut rows = take_rows();
    assert_eq!(rows.len(), 1);
    rows.remove(0)
}
fn plan_attempt(
    plan: &Arc<RenderPlan>,
    base: &Arc<AudioStageProjection<'_>>,
    inner: NativeProvider,
    f: Fault,
) -> Value {
    let _family = family_context("focused_plan");
    let budget = RefCell::new(ReadWork::default());
    let cancel = AtomicBool::new(false);
    let mut source = provider(inner, Some(&budget), false);
    let (mut renderer, monitor) = new_renderer(plan, StageLimits::default(), None);
    let control = WorkControl {
        work: &budget,
        cancelled: &cancel,
        deadline: Instant::now() + TIMEOUT,
    };
    let before = trace::projection_value(base, plan);
    fault(f);
    let (result, action) = private_once(
        &mut renderer,
        &mut source,
        PrepareTarget::Projected(base),
        control,
        &monitor,
        "focused_plan",
    );
    let receipt = receipt_only();
    stream_lifetime(&action, &receipt);
    let after = trace::projection_value(base, plan);
    assert_eq!(before, after);
    let final_work = work(&budget);
    drop(result);
    drop(source);
    drop(renderer);
    assert_eq!(final_work["sound_context_frames"], 0);
    json!({"action":action,"receipt":receipt,"work_after":final_work,"cancelled":cancel.load(Ordering::Relaxed),
        "input_before":before,"input_after":after})
}
fn cache_attempt(
    plan: &Arc<RenderPlan>,
    definition: &deadpan_plan::AudioDefinition<'_>,
    inner: NativeProvider,
    f: Fault,
) -> Value {
    let _family = family_context("focused_cache");
    let directory = tempfile::tempdir().unwrap();
    let cache = PcmCache::at(directory.path()).unwrap();
    let (mut renderer, monitor) = new_renderer(plan, StageLimits::default(), Some(cache.clone()));
    let mut source = provider(inner, None, false);
    let before = definition_value(definition, plan);
    fault(f);
    let action = public_once(
        &mut renderer,
        &mut source,
        Target::Definition(definition),
        (0, 256),
        "focused_cache",
        &monitor,
        &AtomicBool::new(false),
    );
    let receipt = receipt_only();
    stream_lifetime(&action, &receipt);
    let after = definition_value(definition, plan);
    assert_eq!(before, after);
    json!({"action":action,"receipt":receipt,"input_before":before,"input_after":after,
        "cache_after":entries(&cache),"controller_after":controller(&renderer)})
}
fn refusal(baseline: &Value, replay: &Value, target: &Value) {
    assert_eq!(replay["action"]["result"]["status"], "Error");
    let b = &baseline["receipt"]["native"];
    let v = &replay["receipt"]["native"];
    for scope in scopes(v) {
        let old_scope = scopes(b)
            .iter()
            .find(|s| s["ordinal"] == scope["ordinal"])
            .unwrap();
        for key in [
            "provider", "role", "start", "end", "depth", "parent", "request",
        ] {
            assert_eq!(
                scope[key], old_scope[key],
                "actual failure scope identity {key}"
            );
        }
        let entered = logical(v)
            .iter()
            .find(|r| r["scope"] == scope["ordinal"] && r["phase"] == "scope_enter")
            .unwrap();
        let old_entered = logical(b)
            .iter()
            .find(|r| r["scope"] == scope["ordinal"] && r["phase"] == "scope_enter")
            .unwrap();
        assert_eq!(
            caller_build_context(&entered["value"]["before_trace_position"]),
            caller_build_context(&old_entered["value"]["before_trace_position"])
        );
    }
    let injected = native_rows(v)
        .iter()
        .filter(|r| r["actual_injected_refusal"] == true)
        .collect::<Vec<_>>();
    assert_eq!(injected.len(), 1);
    let r = injected[0];
    assert_eq!(r["attempt"], target["attempt"]);
    assert_eq!(r["scope"], target["scope"]);
    assert_eq!(
        r["kind"],
        if target["kind"] == "SpendAttempt" {
            "SpendResult"
        } else {
            "ReserveResult"
        }
    );
    assert_eq!(r["error"]["kind"], "Limit");
    assert_eq!(
        r["error"]["reason"],
        if target["kind"] == "SpendAttempt" {
            "test-only stage validation policy spend refusal"
        } else {
            "test-only stage validation policy reserve refusal"
        }
    );
    assert_eq!(accepted(b, Some(n(&target["ordinal"]))), accepted(v, None));
    assert!(
        native_rows(v)
            .iter()
            .filter(|x| n(&x["ordinal"]) > n(&r["ordinal"]))
            .all(
                |x| !matches!(x["kind"].as_str(), Some("SpendResult" | "ReserveResult"))
                    || !x["error"].is_null()
            )
    );
}
fn sweep(mut run: impl FnMut(Fault) -> Value, lane: u64, tag: &str) -> Value {
    let baseline = run(Fault::None);
    assert_eq!(baseline["action"]["result"]["status"], "Ok");
    let v = &baseline["receipt"]["native"];
    let factory = logical(v)
        .iter()
        .find(|r| {
            r["phase"] == "factory_success" && {
                let s = scopes(v)
                    .iter()
                    .find(|s| s["ordinal"] == r["scope"])
                    .unwrap();
                s["provider"] == tag && n(&s["entry"]["lane"]) == lane
            }
        })
        .expect("actual requested provider/lane factory");
    let scope = factory["scope"].clone();
    let targets = native_rows(v)
        .iter()
        .filter(|r| {
            r["scope"] == scope
                && matches!(r["kind"].as_str(), Some("SpendAttempt" | "ReserveAttempt"))
        })
        .cloned()
        .collect::<Vec<_>>();
    assert!(!targets.is_empty());
    assert!(targets.iter().any(|r| r["kind"] == "ReserveAttempt"));
    let mut refusals = Vec::new();
    for t in &targets {
        let f = if t["kind"] == "SpendAttempt" {
            Fault::RefuseSpend(n(&t["attempt"]) as usize)
        } else {
            Fault::RefuseReserve(n(&t["attempt"]) as usize)
        };
        let actual = run(f);
        refusal(&baseline, &actual, t);
        refusals.push(json!({"target":t,"actual":actual}));
    }
    let mut cancellations = Vec::new();
    for (attempt_kind, result_kind) in [
        ("SpendAttempt", "SpendResult"),
        ("ReserveAttempt", "ReserveResult"),
    ] {
        let t = targets.iter().find(|r| r["kind"] == attempt_kind).unwrap();
        let fault = if attempt_kind == "SpendAttempt" {
            Fault::CancelBeforeSpend(n(&t["attempt"]) as usize)
        } else {
            Fault::CancelBeforeReserve(n(&t["attempt"]) as usize)
        };
        let cancelled = run(fault);
        assert_eq!(cancelled["action"]["result"]["status"], "Error");
        assert_eq!(cancelled["action"]["result"]["error"]["cancelled"], true);
        let rows = native_rows(&cancelled["receipt"]["native"]);
        let hit = rows
            .iter()
            .find(|r| r["kind"] == result_kind && r["action"] == "Cancel")
            .unwrap();
        assert_eq!(hit["attempt"], t["attempt"]);
        assert_eq!(hit["scope"], t["scope"]);
        assert_eq!(hit["error"]["cancelled"], true);
        assert_eq!(hit["actual_injected_refusal"], false);
        assert_eq!(
            accepted(v, Some(n(&t["ordinal"]))),
            accepted(&cancelled["receipt"]["native"], None)
        );
        cancellations.push(json!({"target":t,"actual":cancelled}));
    }
    let mut uses = Vec::new();
    if lane == 0 {
        for phase in [UsePhase::ContentBefore(0), UsePhase::ProgressAfter] {
            let actual = run(Fault::CancelAtUseSeam(phase));
            assert_eq!(actual["action"]["result"]["status"], "Error");
            assert_eq!(actual["action"]["result"]["error"]["cancelled"], true);
            let v = &actual["receipt"]["native"];
            let row = logical(v)
                .iter()
                .find(|r| r["phase"] == "use" && r["value"]["action"] == "set_real_cancellation")
                .unwrap();
            let s = scopes(v)
                .iter()
                .find(|s| s["ordinal"] == row["scope"])
                .unwrap();
            assert_eq!(row["scope"], scope);
            assert!(n(&row["boundary"]["ledger"]) > n(&s["entry"]["ledger"]));
            assert_eq!(
                accepted(v, Some(n(&row["native_position"]) + 1)),
                accepted(v, None)
            );
            uses.push(actual);
        }
    }
    json!({"baseline":baseline,"selected_scope":scope,"targets":targets,"refusals":refusals,
        "actual_callback_cancels":cancellations,"actual_owner_live_use_cancels":uses,
        "scope":"only observed callbacks of one actual query; accepted prefixes/cleanup, no failed canonical split"})
}
fn held_setup(
    renderer: &mut StageAudio,
    source: &mut WitnessProvider<'_>,
    base: &Arc<AudioStageProjection<'_>>,
    monitor: &crate::PreparationMonitor,
) -> (RoutedSignalBlock, Value, Value) {
    let route = SoundRoute::identity(ratio(i128::from(base.output_frames()), 1)).unwrap();
    let grid = AudioSampleGrid::new(
        ExactRatio::ZERO,
        ExactRatio::ONE,
        AudioBoundaryRule::PointCeil,
    )
    .unwrap();
    let count = route.nodes().len();
    let route = AudioSoundRoute::<SignalSample>::new(route, vec![grid; count]).unwrap();
    let domain = AudioRoutedSignal::projected(Arc::clone(base), route).unwrap();
    let before = controller(renderer);
    let event_count = source.events.len();
    let call_count = source.inner.calls;
    let capture = begin_trace();
    let native = begin_action("held_setup", "public");
    let result = renderer.read_routed_signal(
        source,
        &domain,
        SignalSample(512),
        256,
        TIMEOUT,
        &AtomicBool::new(false),
    );
    end_action(native);
    let observed = end_trace(capture);
    let receipt = receipt_only();
    let value = json!({"before":before,"result":public_result(&result),"trace":observed,
        "controller_result_live":controller(renderer),"monitor_after":crate_monitor(monitor),
        "provider":provider_value(source,event_count,call_count)});
    stream_lifetime(&value, &receipt);
    let output = result.unwrap();
    assert!(!output.suppressed.is_empty() && output.suppressed.is_owned());
    (output, value, receipt)
}
fn cap_attempt(
    plan: &Arc<RenderPlan>,
    base: &Arc<AudioStageProjection<'_>>,
    inner: NativeProvider,
    cap: u32,
    matched: bool,
    held: bool,
) -> Value {
    assert!(cap > 0 && cap <= StageLimits::default().maximum_resident_frames);
    let (mut renderer, monitor) = new_renderer(plan, StageLimits::default(), None);
    let mut source = provider(inner, None, false);
    let warmup = public_once(
        &mut renderer,
        &mut source,
        Target::Projection(base),
        (0, 256),
        "cap_warmup",
        &monitor,
        &AtomicBool::new(false),
    );
    assert_eq!(warmup["result"]["status"], "Ok");
    let warmup_receipt = receipt_only();
    let mut owner = None;
    let mut positive_setup = Value::Null;
    if matched {
        let (result, value, receipt) = held_setup(&mut renderer, &mut source, base, &monitor);
        positive_setup = json!({"actual":value,"receipt":receipt,
            "controller_before_holder_choice":controller(&renderer)});
        if held {
            owner = Some(result);
        }
    }
    let after_choice = controller(&renderer);
    renderer.limits.maximum_resident_frames = cap;
    let measured = public_once(
        &mut renderer,
        &mut source,
        Target::Projection(base),
        (512, 256),
        "cap_measured",
        &monitor,
        &AtomicBool::new(false),
    );
    let receipt = receipt_only();
    stream_lifetime(&measured, &receipt);
    let success = measured["result"]["status"] == "Ok";
    if !success {
        assert_eq!(measured["result"]["error"]["kind"], "Limit");
        assert_eq!(
            measured["result"]["error"]["typed_payload"]["reason"],
            "resident PCM or stage cache"
        );
    } else {
        assert!(
            !scopes(&receipt["native"]).is_empty(),
            "measured success must actually run M"
        );
    }
    let observer = renderer.owned_suppression.observer();
    let before_drop = observer.as_ref().map(|v| v.live_frames());
    drop(owner);
    let after_drop = observer.as_ref().map(|v| v.live_frames());
    let retry = if held && !success {
        let actual = public_once(
            &mut renderer,
            &mut source,
            Target::Projection(base),
            (512, 256),
            "cap_drop_retry",
            &monitor,
            &AtomicBool::new(false),
        );
        let receipt = receipt_only();
        stream_lifetime(&actual, &receipt);
        Some(json!({"actual":actual,"receipt":receipt}))
    } else {
        None
    };
    assert_eq!(renderer.limits.maximum_resident_frames, cap);
    json!({"ceiling":cap,"matched_positive_setup":matched,"held":held,"warmup":warmup,"warmup_receipt":warmup_receipt,
        "positive_setup":positive_setup,"controller_after_holder_choice":after_choice,
        "measured":measured,"receipt":receipt,"success":success,
        "account_before_holder_drop":before_drop,"account_after_holder_drop":after_drop,
        "retry_same_controller_same_cap":retry,"fresh_ReadWork_per_public_call":true,
        "holder_scope":"actual prior J opaque output only; no M query owner escapes"})
}
fn minimum(mut run: impl FnMut(u32) -> Value) -> (u32, Vec<Value>) {
    let mut rows = Vec::new();
    let mut lo = 1;
    let mut hi = StageLimits::default().maximum_resident_frames;
    let top = run(hi);
    assert_eq!(top["success"], true);
    rows.push(top);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let actual = run(mid);
        let ok = actual["success"] == true;
        rows.push(actual);
        if ok { hi = mid } else { lo = mid + 1 }
    }
    let exact = run(lo);
    assert_eq!(exact["success"], true);
    rows.push(exact);
    assert!(lo > 1);
    let below = run(lo - 1);
    assert_eq!(below["success"], false);
    rows.push(below);
    (lo, rows)
}
fn residency(
    plan: &Arc<RenderPlan>,
    base: &Arc<AudioStageProjection<'_>>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
) -> SetupResult<Value> {
    let (min, rows) = minimum(|cap| cap_attempt(plan, base, reopen().unwrap(), cap, false, false));
    let (drop_min, dropped) =
        minimum(|cap| cap_attempt(plan, base, reopen().unwrap(), cap, true, false));
    let (held_min, held) =
        minimum(|cap| cap_attempt(plan, base, reopen().unwrap(), cap, true, true));
    assert!(
        held_min > drop_min,
        "actual matched holder must expose a legal separating cap"
    );
    let proof = cap_attempt(plan, base, reopen()?, held_min - 1, true, true);
    assert_eq!(proof["success"], false);
    assert_eq!(
        proof["retry_same_controller_same_cap"]["actual"]["result"]["status"],
        "Ok"
    );
    Ok(
        json!({"minimum":min,"attempts":rows,"matched_drop_minimum":drop_min,"matched_drop_attempts":dropped,
        "matched_held_minimum":held_min,"matched_held_attempts":held,"unchanged_cap_recovery":proof,
        "scope":"separate measured resident state, never changes old2048 or equivalence caps"}),
    )
}
pub(super) fn fixture_visit(
    plan: &Arc<RenderPlan>,
    base: &Arc<AudioStageProjection<'_>>,
    asset: &deadpan_core::AssetRecord,
    wire: &Value,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
    provenance: &Value,
) -> SetupResult<Option<Value>> {
    let mode = COLLECT.with(|v| v.borrow().as_ref().map(|s| s.mode));
    let value = match mode {
        None | Some(Mode::Paired) => return Ok(None),
        Some(Mode::PlanFaults) => sweep(
            |f| plan_attempt(plan, base, reopen().unwrap(), f),
            0,
            "PreserveTape",
        ),
        Some(Mode::CacheFaults) => {
            let (plain, document) = plain_nested_plan(asset, wire)?;
            let definition =
                plain.audio_definition(deadpan_plan::AudioDefinitionSelector::Node {
                    node: id("outer"),
                })?;
            let values = sweep(
                |f| cache_attempt(&plain, &definition, reopen().unwrap(), f),
                1,
                "Signal",
            );
            json!({"document":document,"actual":values})
        }
        Some(Mode::Residency) => residency(plan, base, reopen)?,
    };
    Ok(Some(json!({"provenance":provenance,"actual":value})))
}
fn focused(mode: Mode) -> Value {
    let _guard = collection(mode);
    let _focus = focus();
    family().unwrap()
}
#[test]
fn stage_validation_actual_paired_work_and_content_progress_lifetime() {
    crate::stages::read_signal_policy_request::with_legacy_read_signal_policy(
        stage_validation_actual_paired_work_and_content_progress_lifetime_historical,
    );
}
fn stage_validation_actual_paired_work_and_content_progress_lifetime_historical() {
    paired();
}
#[test]
fn stage_validation_plan_every_callback_refusal_and_live_cancel() {
    crate::stages::read_signal_policy_request::with_legacy_read_signal_policy(
        stage_validation_plan_every_callback_refusal_and_live_cancel_historical,
    );
}
fn stage_validation_plan_every_callback_refusal_and_live_cancel_historical() {
    focused(Mode::PlanFaults);
}
#[test]
fn stage_validation_cache_every_callback_refusal_and_live_cancel() {
    crate::stages::read_signal_policy_request::with_legacy_read_signal_policy(
        stage_validation_cache_every_callback_refusal_and_live_cancel_historical,
    );
}
fn stage_validation_cache_every_callback_refusal_and_live_cancel_historical() {
    focused(Mode::CacheFaults);
}
#[test]
fn stage_validation_public_resident_boundary_and_real_prior_output_recovery() {
    crate::stages::read_signal_policy_request::with_legacy_read_signal_policy(
        stage_validation_public_resident_boundary_and_real_prior_output_recovery_historical,
    );
}
fn stage_validation_public_resident_boundary_and_real_prior_output_recovery_historical() {
    focused(Mode::Residency);
}
fn capture_after_body() -> Value {
    json!({"schema":"stage-validation-policy-after-v1","paired":paired(),
        "plan_faults":focused(Mode::PlanFaults),"cache_faults":focused(Mode::CacheFaults),
        "residency":focused(Mode::Residency),
        "scope":"initial complete stage output validation only; real J holder is separate; no normalized before body"})
}
#[test]
#[ignore = "manual fresh actual after capture; no expected after fixture"]
fn capture_stage_validation_policy_after_v1() {
    crate::stages::read_signal_policy_request::with_legacy_read_signal_policy(
        capture_stage_validation_policy_after_v1_historical,
    );
}
fn capture_stage_validation_policy_after_v1_historical() {
    let output = std::env::var_os("DEADPAN_STAGE_VALIDATION_AFTER_OUTPUT").unwrap();
    let output = Path::new(&output);
    assert!(output.is_absolute() && output.starts_with("/tmp"));
    let variant = std::env::var("DEADPAN_STAGE_VALIDATION_AFTER_VARIANT").unwrap();
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_STAGE_VALIDATION_AFTER_SOURCE_SHA256").unwrap();
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    assert_eq!(
        std::env::var("DEADPAN_STAGE_VALIDATION_AFTER_PUBLISHED_CHECKPOINT").unwrap(),
        CHECKPOINT
    );
    let mut body = capture_after_body();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    body["published_checkpoint"] = json!(CHECKPOINT);
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
