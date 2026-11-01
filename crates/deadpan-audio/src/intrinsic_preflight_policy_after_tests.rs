//! L actual native qualification. Raw observations are retained beside derived checks.
use super::*;
use crate::stages::intrinsic_preflight_policy_native_trace as native;
use native::{Fault, UsePhase};
use std::cell::Cell;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Collect,
    SourceFaults,
    NestedFaults,
    Boundaries,
}
struct Collection {
    epoch: u64,
    mode: Mode,
    family: Option<String>,
    actions: Vec<Value>,
}
thread_local! {
    static COLLECTION:RefCell<Option<Collection>>=const {RefCell::new(None)};
    static COLLECTION_EPOCH:Cell<u64>=const {Cell::new(0)};
}
struct CollectionGuard(u64);
impl Drop for CollectionGuard {
    fn drop(&mut self) {
        COLLECTION.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.as_ref().is_some_and(|v| v.epoch == self.0) {
                slot.take();
            }
        });
    }
}
fn collection(mode: Mode) -> CollectionGuard {
    COLLECTION.with(|slot| assert!(slot.borrow().is_none()));
    let epoch = COLLECTION_EPOCH.with(|slot| {
        let n = slot.get().checked_add(1).unwrap();
        slot.set(n);
        n
    });
    COLLECTION.with(|slot| {
        *slot.borrow_mut() = Some(Collection {
            epoch,
            mode,
            family: None,
            actions: Vec::new(),
        })
    });
    CollectionGuard(epoch)
}
pub(super) struct FamilyContext {
    epoch: Option<u64>,
    previous: Option<String>,
}
impl Drop for FamilyContext {
    fn drop(&mut self) {
        COLLECTION.with(|slot| {
            if let Some(s) = slot.borrow_mut().as_mut()
                && Some(s.epoch) == self.epoch
            {
                s.family = self.previous.take();
            }
        });
    }
}
pub(super) fn family_context(name: &str) -> FamilyContext {
    COLLECTION.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else {
            return FamilyContext {
                epoch: None,
                previous: None,
            };
        };
        let previous = s.family.replace(name.to_owned());
        FamilyContext {
            epoch: Some(s.epoch),
            previous,
        }
    })
}
pub(super) struct ActionCapture {
    guard: Option<native::CaptureGuard>,
    family: Option<String>,
    action: Option<String>,
    entry: &'static str,
}
pub(super) fn begin_action(action: &str, entry: &'static str) -> ActionCapture {
    let family = COLLECTION.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|v| v.mode == Mode::Collect)
            .map(|s| s.family.clone())
    });
    let Some(family) = family else {
        return ActionCapture {
            guard: None,
            family: None,
            action: None,
            entry,
        };
    };
    ActionCapture {
        guard: Some(native::begin(Fault::None)),
        family,
        action: Some(action.to_owned()),
        entry,
    }
}
fn valid_capture(value: &Value) {
    assert_eq!(value["invalid"], false, "native capture invalid: {value}");
    assert_eq!(value["overflow"], false);
    assert_eq!(value["dropped"], 0);
    assert_eq!(value["foreign_request"], false);
    assert_eq!(value["active_scopes"], 0);
    for use_row in value["logical_rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["phase"] == "use")
    {
        assert_eq!(use_row["value"]["same_local_view_identity"], true);
        let factory = value["logical_rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| {
                v["scope"] == use_row["scope"]
                    && matches!(
                        v["phase"].as_str(),
                        Some("factory_success" | "factory_ordinary")
                    )
            })
            .unwrap();
        assert_eq!(
            use_row["value"]["view"], factory["value"]["view"],
            "actual complete borrowed view remains immutable"
        );
    }
    for scope in value["scopes"].as_array().unwrap() {
        assert_eq!(scope["request"], 1, "one fresh request per native capture");
        assert!(scope["exit"].is_object());
        assert_eq!(
            scope["exit"]["ledger"], scope["entry"]["ledger"],
            "actual query cleanup"
        );
    }
}
pub(super) fn end_action(mut action: ActionCapture) {
    if action.guard.is_none() {
        return;
    }
    let value = native::take();
    valid_capture(&value);
    drop(action.guard.take());
    COLLECTION.with(|slot| {
        let mut slot = slot.borrow_mut();
        slot.as_mut()
            .unwrap()
            .actions
            .push(json!({"family":action.family,"action":action.action,
            "entry":action.entry,"native":value}));
    });
}
fn collect_body(legacy: bool) -> (Value, Vec<Value>) {
    let _collection = collection(Mode::Collect);
    let body = if legacy {
        crate::stages::intrinsic_preflight_policy_request::with_legacy_intrinsic_preflight_policy(
            super::capture_body,
        )
    } else {
        super::capture_body()
    };
    let actions =
        COLLECTION.with(|slot| std::mem::take(&mut slot.borrow_mut().as_mut().unwrap().actions));
    (body, actions)
}
fn families(body: &Value) -> &Vec<Value> {
    body["actual"]["families"].as_array().unwrap()
}
fn assert_default_success(body: &Value) {
    let actual = families(body);
    assert_eq!(actual.len(), 5);
    let names = actual
        .iter()
        .map(|v| v["family"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        names,
        BTreeSet::from([
            "qualified_structural_source_hold",
            "qualified_source_input",
            "qualified_source_output",
            "qualified_mix_control",
            "plain_nonowned_nested_stage"
        ])
    );
    for family in actual {
        for field in ["public_cold", "public_warm"] {
            assert_eq!(family[field]["result"]["status"], "Ok", "{family}");
        }
        let private = &family["private_prepare"];
        assert_eq!(private["first"]["status"], "Ok");
        assert_eq!(private["second_same_ReadWork"]["result"]["status"], "Ok");
        assert_eq!(private["same_prepared_arc"], true);
    }
    let errors = body["actual"]["failure_attempts"].as_array().unwrap();
    assert_eq!(errors.len(), 6);
    for error in errors {
        assert_eq!(error["first"]["status"], "Error", "{error}");
    }
    let fit = body["actual"]["setup_attempts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["case"] == "qualified_fitbeat_setup_attempt")
        .unwrap();
    assert_eq!(fit["status"], "Error");
}
fn logical_output(value: &Value) -> Value {
    if value["status"] != "Ok" {
        return value.clone();
    }
    if let Some(p) = value.get("prepared") {
        json!({"status":"Ok","frames":p["frames"],"dependencies":p["dependencies"],"relative_depth":p["relative_depth"],
            "storage_kind":p["storage"]["kind"],"f32_bits":p["storage"]["interleaved_f32_bits"]})
    } else {
        json!({"status":"Ok","schema_version":value["schema_version"],"stage":value["stage"],
            "project_id":value["project_id"],"revision_id":value["revision_id"],"allocation":value["allocation"],
            "start":value["start"],"f32_bits":value["f32_bits"],"sample_len":value["sample_len"],
            "suppressed_values":value["suppressed"]["values"]})
    }
}
fn policy_rows(action: &Value) -> Vec<&Value> {
    action["trace"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| {
            matches!(
                v["phase"].as_str(),
                Some("input_policy_constructed" | "output_policy_constructed")
            )
        })
        .collect()
}
fn native_for<'a>(actions: &'a [Value], family: &str, entry: &str) -> &'a Value {
    &actions
        .iter()
        .find(|v| {
            v["family"] == family
                && v["entry"] == entry
                && (entry != "public" || v["action"] == "cold")
        })
        .unwrap()["native"]
}
fn assert_factory_equations(value: &Value) {
    valid_capture(value);
    let rows = value["native_rows"].as_array().unwrap();
    let scopes = value["scopes"].as_array().unwrap();
    for logical in value["logical_rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["phase"] == "factory_success")
    {
        let id = logical["scope"].as_u64().unwrap();
        let scope = scopes.iter().find(|v| v["ordinal"] == id).unwrap();
        let claim = &logical["value"];
        let mut spend = [0_u64; 2];
        let mut host = [0_u64; 2];
        for row in rows
            .iter()
            .filter(|v| v["scope"] == id && v["factory"] == true)
        {
            if row["kind"] == "SpendResult" && row["error"].is_null() {
                let lane = row["before"]["selected_lane"].as_u64().unwrap() as usize;
                spend[lane] += row["amount"].as_u64().unwrap();
            }
            if row["kind"] == "MakeRoomAfter" {
                assert_eq!(row["host_crosscheck"], true);
                for (lane, total) in host.iter_mut().enumerate() {
                    *total += row["host_delta"][lane].as_u64().unwrap();
                }
            }
        }
        let total = claim["total"].as_u64().unwrap();
        assert_eq!(
            claim["canonical"].as_u64().unwrap() + claim["additional"].as_u64().unwrap(),
            total
        );
        assert_eq!(claim["view"]["work"], claim["total"]);
        assert_eq!(claim["forwarded"], true);
        assert_eq!(spend.iter().sum::<u64>(), total);
        assert_eq!(scope["accepted_spend"], json!(spend));
        assert_eq!(scope["accepted_host"], json!(host));
        for lane in 0..2 {
            let before = scope["factory_entry"]["work"][lane].as_u64().unwrap();
            let after = scope["factory_exit"]["work"][lane].as_u64().unwrap();
            assert_eq!(after - before, spend[lane] + host[lane]);
        }
    }
}
fn compare_policies(ordinary: &Value, native_value: &Value) {
    let rows = policy_rows(ordinary);
    let success = native_value["logical_rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["phase"] == "factory_success")
        .collect::<Vec<_>>();
    assert_eq!(
        rows.len(),
        success.len(),
        "every constructed policy must have actual successful factory"
    );
    for (old, new) in rows.into_iter().zip(success) {
        let scope = native_value["scopes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["ordinal"] == new["scope"])
            .unwrap();
        assert_eq!(scope["provider"], old["value"]["mode"]);
        let role = if old["phase"] == "input_policy_constructed" {
            "Input"
        } else {
            "Output"
        };
        assert_eq!(scope["role"], role);
        assert_eq!(scope["start"], old["value"]["range"]["start"]);
        assert_eq!(scope["end"], old["value"]["range"]["end"]);
        let ordinary_scope = ordinary["trace"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["phase"] == "scope_enter" && v["caller"] == old["caller"])
            .unwrap();
        assert_eq!(ordinary_scope["kind"], "intrinsic");
        assert_eq!(scope["depth"], ordinary_scope["depth"]);
        let old = &old["value"]["policy"];
        let new = &new["value"];
        assert_eq!(old["suppressed"], new["view"]["suppressed"]);
        assert_eq!(old["contents"], new["view"]["contents"]);
        assert_eq!(old["lookup"], new["view"]["lookup"]);
        assert_eq!(old["work"], new["canonical"]);
    }
    assert_factory_equations(native_value);
}
fn admitted_net(value: &Value, lo: u64, hi: u64) -> i128 {
    let mut net = 0_i128;
    for row in value["native_rows"].as_array().unwrap().iter().filter(|v| {
        let n = v["ordinal"].as_u64().unwrap();
        n > lo && n <= hi
    }) {
        if row["kind"] == "ReserveResult" && row["error"].is_null() {
            net += i128::from(row["frames"].as_u64().unwrap());
        }
        if row["kind"] == "Release" {
            net -= i128::from(row["frames"].as_u64().unwrap());
        }
    }
    net
}
fn assert_nested_lifetime(value: &Value) {
    let scopes = value["scopes"].as_array().unwrap();
    let rows = value["native_rows"].as_array().unwrap();
    let child = scopes
        .iter()
        .find(|v| v["provider"] == "Signal" && v["role"] == "Input" && !v["parent"].is_null())
        .unwrap();
    let parent = scopes
        .iter()
        .find(|v| v["ordinal"] == child["parent"])
        .unwrap();
    assert!(child["peak"].as_u64().unwrap() > child["entry"]["ledger"].as_u64().unwrap());
    assert!(rows.iter().any(|v| v["scope"] == child["ordinal"]
        && v["kind"] == "ReserveResult"
        && v["error"].is_null()
        && v["frames"].as_u64().unwrap() > 0));
    let lo = parent["native_entry_position"].as_u64().unwrap();
    let hi = child["native_entry_position"].as_u64().unwrap();
    let mut net = 0_i128;
    for row in rows.iter().filter(|v| {
        let n = v["ordinal"].as_u64().unwrap();
        n > lo && n <= hi
    }) {
        if row["kind"] == "ReserveResult" && row["error"].is_null() {
            net += i128::from(row["frames"].as_u64().unwrap());
        }
        if row["kind"] == "Release" {
            net -= i128::from(row["frames"].as_u64().unwrap());
        }
    }
    assert!(
        net > 0,
        "parent net accepted metadata must still be live at child entry"
    );
    let parent_entry = parent["entry"]["ledger"].as_u64().unwrap();
    let child_entry = child["entry"]["ledger"].as_u64().unwrap();
    assert_eq!(i128::from(parent_entry) + net, i128::from(child_entry));
    assert!(child_entry > parent_entry);
    let parent_uses = value["logical_rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["scope"] == parent["ordinal"] && v["phase"] == "use")
        .collect::<Vec<_>>();
    let after = parent_uses
        .iter()
        .find(|v| v["value"]["phase"] == "RecursiveComplete")
        .unwrap();
    assert_eq!(after["value"]["same_local_view_identity"], true);
    let factory = value["logical_rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["scope"] == parent["ordinal"] && v["phase"] == "factory_success")
        .unwrap();
    assert_eq!(after["value"]["view"], factory["value"]["view"]);
    assert_eq!(parent["exit"]["ledger"], parent["entry"]["ledger"]);
}
fn paired() -> Value {
    let (old, old_native) = collect_body(true);
    let expected: Value = serde_json::from_slice(include_bytes!(
        "../tests/fixtures/intrinsic-preflight-policy-baseline-v1/corpus.json"
    ))
    .unwrap();
    assert_eq!(
        old, expected,
        "L-only replay keeps J/G active and matches frozen40"
    );
    let (current, current_native) = collect_body(false);
    assert_default_success(&current);
    assert_eq!(old_native.len(), 26);
    assert_eq!(current_native.len(), 26);
    let mut comparisons = Vec::new();
    for (a, b) in families(&old).iter().zip(families(&current)) {
        assert_eq!(a["family"], b["family"]);
        let name = a["family"].as_str().unwrap();
        assert_eq!(b["projection_before"], b["projection_after"]);
        assert_eq!(a["projection_before"], b["projection_before"]);
        for field in ["public_cold", "public_warm"] {
            assert_eq!(
                logical_output(&a[field]["result"]),
                logical_output(&b[field]["result"])
            );
            let n = &current_native
                .iter()
                .find(|v| {
                    v["family"] == name
                        && v["entry"] == "public"
                        && v["action"] == a[field]["action"]
                })
                .unwrap()["native"];
            assert!(
                !n["scopes"].as_array().unwrap().is_empty(),
                "fresh public work must actually preflight"
            );
            compare_policies(&a[field], n);
        }
        assert_eq!(
            logical_output(&a["private_prepare"]["first"]),
            logical_output(&b["private_prepare"]["first"])
        );
        assert_eq!(
            logical_output(&a["private_prepare"]["second_same_ReadWork"]["result"]),
            logical_output(&b["private_prepare"]["second_same_ReadWork"]["result"])
        );
        for field in [
            "prepared_after_ReadWork_drop",
            "prepared_after_controller_drop",
        ] {
            assert_eq!(
                b["private_prepare"]["first"]["prepared"], b["private_prepare"][field],
                "actual Prepared Arc remains complete after budget/controller disposal"
            );
        }
        let native = native_for(&current_native, name, "private_first");
        let ordinary = json!({"trace":a["private_prepare"]["first_trace"]});
        compare_policies(&ordinary, native);
        let memo = native_for(&current_native, name, "private_memo");
        assert!(
            memo["scopes"].as_array().unwrap().is_empty(),
            "prepared same-request memo bypass is distinct from fresh work"
        );
        comparisons.push(json!({"family":name,"public_logical_equal":true,"private_logical_equal":true,
            "ordinary_result":a,"active_result":b,"capacity_scope":"raw capacities retained; no active view capacity inferred"}));
        if name == "plain_nonowned_nested_stage" {
            assert_nested_lifetime(native);
        }
    }
    for (a, b) in old["actual"]["failure_attempts"]
        .as_array()
        .unwrap()
        .iter()
        .zip(current["actual"]["failure_attempts"].as_array().unwrap())
    {
        assert_eq!(a["action"], b["action"]);
        assert_eq!(a["first"]["error"], b["first"]["error"]);
    }
    json!({"schema":"intrinsic-preflight-native-pairs-v1","ordinary_frozen40":old,"active_raw_driver":current,
        "ordinary_native_per_call":old_native,"active_native_per_call":current_native,"comparisons":comparisons,
        "raw_driver_schema_note":"historical before-driver headers retained verbatim; actual policy ownership rows and this after envelope distinguish active data"})
}
fn private_once(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    inner: NativeProvider,
    fault: Fault,
) -> Value {
    let input_before = trace::projection_value(projection, plan);
    let budget = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = WorkControl {
        work: &budget,
        cancelled: &cancelled,
        deadline: Instant::now() + TIMEOUT,
    };
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let mut provider = WitnessProvider {
        inner,
        budget: Some(&budget),
        events: Vec::new(),
        fail: false,
    };
    let before = controller(&renderer);
    let _observations = begin_observations();
    let capture = native::begin(fault);
    let result = renderer.prepare_projected(projection, &mut provider, control, 1);
    let native_value = native::take();
    drop(capture);
    valid_capture(&native_value);
    let observed = finish_observations();
    let output = match &result {
        Ok(b) => json!({"status":"Ok","prepared":prepared(b)}),
        Err(e) => json!({"status":"Error","error":error(e)}),
    };
    let accepted = work(&budget);
    assert_eq!(accepted["sound_context_frames"], 0);
    let after = controller(&renderer);
    let events = std::mem::take(&mut provider.events);
    let calls = provider.inner.calls;
    let token = cancelled.load(Ordering::Relaxed);
    drop(provider);
    drop(budget);
    let after_work = result.as_ref().ok().map(|b| prepared(b));
    drop(renderer);
    let after_controller = result.as_ref().ok().map(|b| prepared(b));
    drop(result);
    let input_after = trace::projection_value(projection, plan);
    assert_eq!(input_before, input_after);
    json!({"entry":"one fresh private prepare_projected","fault":fault,"input_before":input_before,"input_after":input_after,
        "before":before,"result":output,"accepted_work_after":accepted,"controller_after":after,
        "provider_source_calls":calls,"provider_events":events,"cancelled_after":token,
        "prepared_after_work_drop":after_work,"prepared_after_controller_drop":after_controller,
        "ordinary_observation":observed,"native":native_value})
}
fn successful_factory<'a>(value: &'a Value, provider: &str, nested: bool) -> &'a Value {
    value["native"]["logical_rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|logical| {
            if logical["phase"] != "factory_success" {
                return false;
            }
            let scope = value["native"]["scopes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["ordinal"] == logical["scope"])
                .unwrap();
            scope["provider"] == provider
                && scope["role"] == "Input"
                && (!nested || !scope["parent"].is_null())
                && logical["value"]["view"]["contents"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v["type"] == "source")
        })
        .unwrap()
}
fn accepted_signature(value: &Value, before_ordinal: Option<u64>) -> Vec<Value> {
    value["native_rows"].as_array().unwrap().iter().filter(|v|
        matches!(v["kind"].as_str(),Some("SpendResult"|"ReserveResult"))&&v["error"].is_null()&&
        before_ordinal.is_none_or(|n|v["ordinal"].as_u64().unwrap()<n))
        .map(|v|json!({"kind":v["kind"],"scope":v["scope"],"attempt":v["attempt"],"amount":v["amount"],
            "layout":v["layout"],"frames":v["frames"],"work_before":v["before"]["work"],
            "work_after":v["after"]["work"],"ledger_before":v["before"]["ledger"],"ledger_after":v["after"]["ledger"]}))
        .collect()
}
fn assert_refusal_prefix(baseline: &Value, replay: &Value, target: &Value) {
    assert_eq!(replay["result"]["status"], "Error");
    let rows = replay["native"]["native_rows"].as_array().unwrap();
    let injected = rows
        .iter()
        .filter(|v| v["actual_injected_refusal"] == true)
        .collect::<Vec<_>>();
    assert_eq!(injected.len(), 1, "actual named protocol refusal required");
    let injected = injected[0];
    assert_eq!(injected["attempt"], target["attempt"]);
    assert_eq!(injected["scope"], target["scope"]);
    let expected_kind = if target["kind"] == "SpendAttempt" {
        "SpendResult"
    } else {
        "ReserveResult"
    };
    assert_eq!(injected["kind"], expected_kind);
    assert_eq!(injected["error"]["kind"], "Limit");
    assert_eq!(
        accepted_signature(
            &baseline["native"],
            Some(target["ordinal"].as_u64().unwrap())
        ),
        accepted_signature(&replay["native"], None)
    );
    assert!(
        rows.iter()
            .filter(|v| v["ordinal"].as_u64().unwrap() > injected["ordinal"].as_u64().unwrap())
            .all(
                |v| !matches!(v["kind"].as_str(), Some("SpendResult" | "ReserveResult"))
                    || !v["error"].is_null()
            ),
        "cleanup may release but cannot accept a later callback"
    );
    assert_eq!(replay["accepted_work_after"]["sound_context_frames"], 0);
}
fn direct_priority(
    plan: &Arc<RenderPlan>,
    layout: std::alloc::Layout,
    reserve: bool,
    cancel: bool,
) -> Value {
    use deadpan_plan::AudioSoundContextAdmission;
    let budget = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(cancel);
    let control = WorkControl {
        work: &budget,
        cancelled: &cancelled,
        deadline: if cancel {
            Instant::now() + TIMEOUT
        } else {
            Instant::now() - Duration::from_secs(1)
        },
    };
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let _observations = begin_observations();
    let fault = if reserve {
        Fault::RefuseReserve(1)
    } else {
        Fault::RefuseSpend(1)
    };
    let capture = native::begin(fault);
    let error_value;
    {
        let _scope = native::scope(
            control,
            native::ProviderTag::Tape,
            native::Role::Input,
            SignalSample(0)..SignalSample(256),
            1,
        );
        let result = {
            let mut admission =
                crate::stages::sound_context_admission::Admission::new(&mut renderer, control);
            if reserve {
                admission.reserve(layout).map(drop)
            } else {
                admission.spend(1)
            }
        };
        let actual = result.as_ref().expect_err("real control check must reject");
        if cancel {
            assert!(actual.is_cancelled());
        } else {
            assert!(matches!(actual, StageAudioError::Timeout));
        }
        error_value = error(actual);
    }
    let observed = native::take();
    drop(capture);
    valid_capture(&observed);
    assert!(
        observed["native_rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["actual_injected_refusal"] == false)
    );
    let ordinary = finish_observations();
    json!({"entry":"DIRECT actual Native Admission operation; not public runtime or policy factory",
        "reserve":reserve,"real_cancelled_at_entry":cancel,"expired_deadline":!cancel,
        "layout_from_actual_baseline":[layout.size(),layout.align()],"result_error":error_value,
        "native":observed,"accepted_work":work(&budget),"ordinary_observation":ordinary})
}
fn fault_sweep(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    nested: bool,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
) -> SetupResult<Value> {
    let baseline = private_once(plan, projection, reopen()?, Fault::None);
    assert_eq!(baseline["result"]["status"], "Ok");
    assert_factory_equations(&baseline["native"]);
    if nested {
        assert_nested_lifetime(&baseline["native"]);
    }
    let logical = successful_factory(&baseline, if nested { "Signal" } else { "Tape" }, nested);
    let scope = logical["scope"].as_u64().unwrap();
    let targets = baseline["native"]["native_rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| {
            v["scope"] == scope
                && matches!(v["kind"].as_str(), Some("SpendAttempt" | "ReserveAttempt"))
        })
        .cloned()
        .collect::<Vec<_>>();
    assert!(!targets.is_empty());
    assert!(
        targets.len() <= 256,
        "fail rather than truncate a larger real query"
    );
    assert!(targets.iter().any(|v| v["kind"] == "SpendAttempt"));
    assert!(targets.iter().any(|v| v["kind"] == "ReserveAttempt"));
    let mut replays = Vec::new();
    for target in &targets {
        let ordinal = usize::try_from(target["attempt"].as_u64().unwrap()).unwrap();
        let fault = if target["kind"] == "SpendAttempt" {
            Fault::RefuseSpend(ordinal)
        } else {
            Fault::RefuseReserve(ordinal)
        };
        let replay = private_once(plan, projection, reopen()?, fault);
        assert_refusal_prefix(&baseline, &replay, target);
        replays.push(json!({"target":target,"actual":replay}));
    }
    let parent_cancel = private_once(
        plan,
        projection,
        reopen()?,
        Fault::CancelAtUseSeam(UsePhase::GraphBefore),
    );
    assert_eq!(parent_cancel["cancelled_after"], true);
    assert_eq!(parent_cancel["result"]["status"], "Error");
    assert_eq!(parent_cancel["result"]["error"]["cancelled"], true);
    let parent_seam = parent_cancel["native"]["logical_rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["phase"] == "use" && v["value"]["action"] == "set_real_cancellation")
        .unwrap();
    let parent_scope = parent_cancel["native"]["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["ordinal"] == parent_seam["scope"])
        .unwrap();
    assert_eq!(parent_seam["value"]["phase"], "GraphBefore");
    assert!(
        parent_seam["boundary"]["ledger"].as_u64().unwrap()
            > parent_scope["entry"]["ledger"].as_u64().unwrap()
    );
    if nested {
        let baseline_child = baseline["native"]["scopes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["ordinal"] == scope)
            .unwrap();
        assert_eq!(
            baseline_child["parent"], parent_seam["scope"],
            "actual owner is the baseline child's live parent"
        );
    }
    let baseline_parent_seam = baseline["native"]["logical_rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| {
            v["phase"] == "use"
                && v["scope"] == parent_seam["scope"]
                && v["value"]["phase"] == "GraphBefore"
        })
        .unwrap();
    assert_eq!(
        accepted_signature(
            &baseline["native"],
            Some(baseline_parent_seam["native_position"].as_u64().unwrap() + 1)
        ),
        accepted_signature(&parent_cancel["native"], None),
        "actual accepted callback prefix before real use-seam cancellation"
    );
    let parent_net = admitted_net(
        &parent_cancel["native"],
        parent_scope["native_entry_position"].as_u64().unwrap(),
        parent_seam["native_position"].as_u64().unwrap(),
    );
    assert!(parent_net > 0);
    assert_eq!(
        i128::from(parent_scope["entry"]["ledger"].as_u64().unwrap()) + parent_net,
        i128::from(parent_seam["boundary"]["ledger"].as_u64().unwrap())
    );
    let mut child_cancel = Value::Null;
    if nested {
        let child = baseline["native"]["scopes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["ordinal"] == scope)
            .unwrap();
        let target = targets
            .iter()
            .find(|v| {
                v["before"]["ledger"].as_u64().unwrap() > child["entry"]["ledger"].as_u64().unwrap()
            })
            .expect("a real child callback after accepted child metadata");
        let n = usize::try_from(target["attempt"].as_u64().unwrap()).unwrap();
        let fault = if target["kind"] == "SpendAttempt" {
            Fault::CancelBeforeSpend(n)
        } else {
            Fault::CancelBeforeReserve(n)
        };
        child_cancel = private_once(plan, projection, reopen()?, fault);
        assert_eq!(child_cancel["cancelled_after"], true);
        assert_eq!(child_cancel["result"]["status"], "Error");
        assert_eq!(child_cancel["result"]["error"]["cancelled"], true);
        let actual = child_cancel["native"]["native_rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| {
                v["scope"] == scope
                    && v["attempt"] == target["attempt"]
                    && v["scheduled_action"] == "SetRealCancellation"
            })
            .unwrap();
        assert!(
            actual["before"]["ledger"].as_u64().unwrap()
                > child["entry"]["ledger"].as_u64().unwrap()
        );
        assert_eq!(
            accepted_signature(
                &baseline["native"],
                Some(target["ordinal"].as_u64().unwrap())
            ),
            accepted_signature(&child_cancel["native"], None),
            "actual child-live cancellation accepted prefix"
        );
        let actual_child = child_cancel["native"]["scopes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["ordinal"] == scope)
            .unwrap();
        let net = admitted_net(
            &child_cancel["native"],
            actual_child["native_entry_position"].as_u64().unwrap(),
            actual["ordinal"].as_u64().unwrap(),
        );
        assert!(net > 0, "real accepted child metadata above its own entry");
        assert_eq!(
            i128::from(actual_child["entry"]["ledger"].as_u64().unwrap()) + net,
            i128::from(actual["before"]["ledger"].as_u64().unwrap())
        );
    }
    let layout_row = targets
        .iter()
        .find(|v| v["kind"] == "ReserveAttempt")
        .unwrap();
    let layout = std::alloc::Layout::from_size_align(
        layout_row["layout"][0].as_u64().unwrap() as usize,
        layout_row["layout"][1].as_u64().unwrap() as usize,
    )
    .unwrap();
    let priority = if nested {
        Vec::new()
    } else {
        vec![
            direct_priority(plan, layout, false, true),
            direct_priority(plan, layout, true, true),
            direct_priority(plan, layout, false, false),
            direct_priority(plan, layout, true, false),
        ]
    };
    Ok(
        json!({"baseline":baseline,"selected_successful_query_scope":scope,"actual_targets":targets,
        "refusal_replays":replays,"use_seam_cancellation":parent_cancel,"baseline_parent_context":nested,"child_live_cancellation":child_cancel,
        "direct_operation_priority":priority,"count_scope":"only actual callbacks of one selected real query; global native attempt ordinals",
        "unexposed":"failed local canonical/additional; physical memory and allocation origin"}),
    )
}
fn public_once(
    renderer: &mut StageAudio,
    provider: &mut WitnessProvider<'_>,
    projection: &Arc<AudioStageProjection<'_>>,
    start: i64,
    frames: u32,
) -> (Result<RoutedSignalBlock, StageAudioError>, Value) {
    let route = SoundRoute::identity(ratio(i128::from(projection.output_frames()), 1)).unwrap();
    let grid = AudioSampleGrid::new(
        ExactRatio::ZERO,
        ExactRatio::ONE,
        AudioBoundaryRule::PointCeil,
    )
    .unwrap();
    let count = route.nodes().len();
    let route = AudioSoundRoute::<SignalSample>::new(route, vec![grid; count]).unwrap();
    let domain = AudioRoutedSignal::projected(Arc::clone(projection), route).unwrap();
    let before = controller(renderer);
    let events_before = provider.events.len();
    let calls_before = provider.inner.calls;
    let _observations = begin_observations();
    let capture = native::begin(Fault::None);
    let result = renderer.read_routed_signal(
        provider,
        &domain,
        SignalSample(start),
        frames,
        TIMEOUT,
        &AtomicBool::new(false),
    );
    let native_value = native::take();
    drop(capture);
    valid_capture(&native_value);
    let observed = finish_observations();
    let value = json!({"entry":"one actual public read_routed_signal/fresh internal ReadWork","start":start,"frames":frames,
        "before":before,"result":public_result(&result),"after_result_live":controller(renderer),
        "native":native_value,"ordinary_observation":observed,"provider_event_bounds":[events_before,provider.events.len()],
        "provider_calls_before":calls_before,"provider_calls_after":provider.inner.calls,"provider_events":provider.events,
        "public_internal_work":"already dropped; only stored scalar/borrowed snapshots serialized, no request address dereference"});
    (result, value)
}
fn resident_result(result: &Result<RoutedSignalBlock, StageAudioError>) -> bool {
    match result {
        Ok(_) => true,
        Err(StageAudioError::Limit("resident PCM or stage cache")) => false,
        Err(e) => panic!("unexpected resident-search error: {e:?}"),
    }
}
fn positive_held(value: &RoutedSignalBlock) {
    assert!(
        !value.suppressed.is_empty(),
        "actual512/256 includes a positive SilentHold policy"
    );
    assert!(value.suppressed.is_owned(), "actual native owner required");
}
fn search_attempt(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    inner: NativeProvider,
    cap: u32,
    matched_setup: bool,
    keep_held: bool,
) -> Value {
    assert!(cap > 0 && cap <= StageLimits::default().maximum_resident_frames);
    let input_before = trace::projection_value(projection, plan);
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let mut provider = WitnessProvider {
        inner,
        budget: None,
        events: Vec::new(),
        fail: false,
    };
    let (warm, warm_value) = public_once(&mut renderer, &mut provider, projection, 0, 256);
    assert!(warm.is_ok());
    drop(warm);
    let mut held = None;
    let mut held_value = Value::Null;
    if matched_setup {
        let (result, value) = public_once(&mut renderer, &mut provider, projection, 512, 256);
        positive_held(result.as_ref().unwrap());
        held_value = value;
        held = Some(result);
    }
    let before_choice = controller(&renderer);
    if !keep_held {
        drop(held.take());
    }
    renderer.limits.maximum_resident_frames = cap;
    let before_measured = controller(&renderer);
    let (result, measured) = public_once(&mut renderer, &mut provider, projection, 512, 256);
    let success = resident_result(&result);
    if success {
        assert!(
            !measured["native"]["scopes"].as_array().unwrap().is_empty(),
            "fresh work cannot be prepared memo"
        );
    }
    drop(result);
    drop(held);
    let after_drop = controller(&renderer);
    let input_after = trace::projection_value(projection, plan);
    assert_eq!(input_before, input_after);
    json!({"ceiling":cap,"matched_positive_setup":matched_setup,"holder_kept":keep_held,
        "warmup":warm_value,"positive_setup":held_value,"controller_before_holder_choice":before_choice,
        "controller_before_measured":before_measured,"measured":measured,"success":success,
        "after_result_holder_drop":after_drop,"input_before":input_before,"input_after":input_after,
        "scope":"separate actual public resident boundary; original equivalence caps unchanged"})
}
fn minimum_search(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
    matched_setup: bool,
    keep_held: bool,
) -> SetupResult<(u32, Vec<Value>)> {
    let mut attempts = Vec::new();
    let mut low = 0_u32;
    let mut high = StageLimits::default().maximum_resident_frames;
    let first = search_attempt(plan, projection, reopen()?, high, matched_setup, keep_held);
    assert_eq!(
        first["success"], true,
        "existing default ceiling must still succeed"
    );
    attempts.push(first);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        let actual = search_attempt(
            plan,
            projection,
            reopen()?,
            middle,
            matched_setup,
            keep_held,
        );
        if actual["success"] == true {
            high = middle;
        } else {
            low = middle;
        }
        attempts.push(actual);
        assert!(
            attempts.len() <= 26,
            "bounded existing default ceiling binary search"
        );
    }
    assert!(high > 1, "one-below must be a legal positive ceiling");
    let exact = search_attempt(plan, projection, reopen()?, high, matched_setup, keep_held);
    assert_eq!(exact["success"], true);
    attempts.push(exact);
    let below = search_attempt(
        plan,
        projection,
        reopen()?,
        high - 1,
        matched_setup,
        keep_held,
    );
    assert_eq!(below["success"], false);
    attempts.push(below);
    Ok((high, attempts))
}
fn held_recovery(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    inner: NativeProvider,
    cap: u32,
) -> Value {
    let input_before = trace::projection_value(projection, plan);
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let mut provider = WitnessProvider {
        inner,
        budget: None,
        events: Vec::new(),
        fail: false,
    };
    let (warm, warm_value) = public_once(&mut renderer, &mut provider, projection, 0, 256);
    assert!(warm.is_ok());
    drop(warm);
    let (held, held_setup) = public_once(&mut renderer, &mut provider, projection, 512, 256);
    positive_held(held.as_ref().unwrap());
    let held_before = public_result(&held);
    renderer.limits.maximum_resident_frames = cap;
    let account = renderer.owned_suppression.observer().unwrap();
    let holder_live = account.live_frames();
    let (probe, probe_value) = public_once(&mut renderer, &mut provider, projection, 512, 256);
    let probe_success = resident_result(&probe);
    assert_eq!(
        public_result(&held),
        held_before,
        "real held values remain unchanged"
    );
    assert_eq!(renderer.limits.maximum_resident_frames, cap);
    drop(probe);
    let before_drop = controller(&renderer);
    drop(held);
    let holder_dropped = account.live_frames();
    assert!(
        holder_dropped < holder_live,
        "physical owned mask drop releases its actual charge"
    );
    let after_drop = controller(&renderer);
    let (retry, retry_value) = public_once(&mut renderer, &mut provider, projection, 512, 256);
    let retry_success = resident_result(&retry);
    assert_eq!(renderer.limits.maximum_resident_frames, cap);
    drop(retry);
    let input_after = trace::projection_value(projection, plan);
    assert_eq!(input_before, input_after);
    json!({"ceiling":cap,"same_controller_same_cap":true,"fresh_ReadWork_per_call":true,"warmup":warm_value,
        "positive_held_setup":held_setup,"held_value":held_before,"held_probe":probe_value,
        "held_probe_success":probe_success,"account_holder_live":holder_live,
        "controller_before_holder_drop":before_drop,"controller_after_holder_drop":after_drop,
        "account_after_holder_drop":holder_dropped,"retry_after_drop":retry_value,"retry_success":retry_success,
        "qualified":!probe_success&&retry_success,"input_before":input_before,"input_after":input_after})
}
fn boundaries(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
) -> SetupResult<Value> {
    let (minimum, search) = minimum_search(plan, projection, reopen, false, false)?;
    let first = held_recovery(plan, projection, reopen()?, minimum);
    let mut fallback = Value::Null;
    let qualified = if first["qualified"] == true {
        true
    } else {
        let (drop_min, dropped_search) = minimum_search(plan, projection, reopen, true, false)?;
        let (held_min, held_search) = minimum_search(plan, projection, reopen, true, true)?;
        assert!(
            held_min > drop_min,
            "matched real holder must expose a separating legal ceiling"
        );
        let cap = held_min - 1;
        let dropped_at_cap = search_attempt(plan, projection, reopen()?, cap, true, false);
        assert_eq!(dropped_at_cap["success"], true);
        let actual = held_recovery(plan, projection, reopen()?, cap);
        let ok = actual["qualified"] == true;
        fallback = json!({"reason":"first state did not separate held/drop; actual cache eligibility retained",
            "matched_drop_minimum":drop_min,"matched_held_minimum":held_min,
            "dropped_search":dropped_search,"held_search":held_search,"dropped_at_selected_cap":dropped_at_cap,
            "same_controller_recovery":actual});
        ok
    };
    assert!(
        qualified,
        "bounded matched held/drop attempt must demonstrate real unchanged-cap recovery"
    );
    Ok(
        json!({"scope":"measured end-to-end warmed public resident threshold, distinct from admitted-ledger peak",
        "positive_request":{"start":512,"frames":256},"initial_minimum":minimum,"initial_search":search,
        "first_same_controller_held_trial":first,"matched_state_fallback":fallback,"qualified":qualified}),
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
    let mode = COLLECTION.with(|slot| slot.borrow().as_ref().map(|s| s.mode));
    let Some(mode) = mode.filter(|mode| *mode != Mode::Collect) else {
        return Ok(None);
    };
    let setup = provenance()?;
    let actual = match mode {
        Mode::SourceFaults => {
            let projection = qualified_projection(plan, base, false, false)?;
            fault_sweep(plan, &projection, false, reopen)?
        }
        Mode::NestedFaults => {
            let (nested, document) = plain_nested_plan(asset, wire)?;
            let projection = projection_for_plain(&nested)?;
            json!({"actual_checked_nested_document":document,"proof":fault_sweep(&nested,&projection,true,reopen)?})
        }
        Mode::Boundaries => boundaries(plan, base, reopen)?,
        Mode::Collect => unreachable!(),
    };
    Ok(Some(
        json!({"schema":"intrinsic-preflight-focused-native-v1","actual_persisted_fixture":setup,"actual":actual}),
    ))
}
fn focused(mode: Mode) -> Value {
    let _collection = collection(mode);
    super::family().expect("actual persisted setup and public fixture construction")
}
#[test]
fn intrinsic_preflight_actual_paired_work_and_nested_lifetime() {
    crate::stages::stage_validation_policy_request::with_legacy_stage_validation_policy(
        intrinsic_preflight_actual_paired_work_and_nested_lifetime_historical,
    );
}
fn intrinsic_preflight_actual_paired_work_and_nested_lifetime_historical() {
    paired();
}
#[test]
fn intrinsic_preflight_source_every_native_callback_refusal_and_priority() {
    crate::stages::stage_validation_policy_request::with_legacy_stage_validation_policy(
        intrinsic_preflight_source_every_native_callback_refusal_and_priority_historical,
    );
}
fn intrinsic_preflight_source_every_native_callback_refusal_and_priority_historical() {
    focused(Mode::SourceFaults);
}
#[test]
fn intrinsic_preflight_nested_signal_every_callback_and_live_cancellation() {
    crate::stages::stage_validation_policy_request::with_legacy_stage_validation_policy(
        intrinsic_preflight_nested_signal_every_callback_and_live_cancellation_historical,
    );
}
fn intrinsic_preflight_nested_signal_every_callback_and_live_cancellation_historical() {
    focused(Mode::NestedFaults);
}
#[test]
fn intrinsic_preflight_public_resident_boundary_and_real_held_owner_recovery() {
    crate::stages::stage_validation_policy_request::with_legacy_stage_validation_policy(
        intrinsic_preflight_public_resident_boundary_and_real_held_owner_recovery_historical,
    );
}
fn intrinsic_preflight_public_resident_boundary_and_real_held_owner_recovery_historical() {
    focused(Mode::Boundaries);
}
fn capture_after_body() -> Value {
    json!({"schema":"intrinsic-preflight-native-qualified-after-v1","pairs":paired(),
        "source_faults":focused(Mode::SourceFaults),"nested_signal_faults":focused(Mode::NestedFaults),
        "public_resident_boundary":focused(Mode::Boundaries),
        "scope":{"native":"actual request-ledger callbacks/host work/cleanup; no recording backend",
            "capacities":"ordinary true capacities retained; active policy slice capacities unexposed",
            "failures":"INJECTED protocol refusal distinct from actual cancel/deadline/resident cap",
            "gaps":["private Bound constraints/opaque Plan identities unavailable",
                "no physical memory/RSS, per-lease origin, native decoder timing or product Gate claim"]}})
}
#[test]
#[ignore = "manual fresh actual after capture, no generated expected fixture"]
fn capture_intrinsic_preflight_policy_after_v1() {
    crate::stages::stage_validation_policy_request::with_legacy_stage_validation_policy(
        capture_intrinsic_preflight_policy_after_v1_historical,
    );
}
fn capture_intrinsic_preflight_policy_after_v1_historical() {
    let output = std::env::var_os("DEADPAN_INTRINSIC_PREFLIGHT_AFTER_OUTPUT").unwrap();
    let output = Path::new(&output);
    assert!(output.is_absolute() && output.starts_with("/tmp"));
    let variant = std::env::var("DEADPAN_INTRINSIC_PREFLIGHT_AFTER_VARIANT").unwrap();
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_INTRINSIC_PREFLIGHT_AFTER_SOURCE_SHA256").unwrap();
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    assert_eq!(
        std::env::var("DEADPAN_INTRINSIC_PREFLIGHT_AFTER_PUBLISHED_CHECKPOINT").unwrap(),
        BASE
    );
    let mut body = capture_after_body();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    body["published_checkpoint"] = json!(BASE);
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
