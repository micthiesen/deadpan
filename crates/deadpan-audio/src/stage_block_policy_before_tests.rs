//! Ordinary O post-DSP observations with N, M, L and J current.
use super::*;
use crate::stages::stage_block_policy_before_trace as o_trace;

const CHECKPOINT_O: &str = "52ac739bb447262223ba6f0e50563de3d4faab04";
std::thread_local! {
    static FOCUSED_O: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

struct Focus(bool);
impl Drop for Focus {
    fn drop(&mut self) {
        FOCUSED_O.with(|value| value.set(self.0));
    }
}
fn focus() -> Focus {
    Focus(FOCUSED_O.with(|value| value.replace(true)))
}
pub(super) fn active() -> bool {
    after::active() || FOCUSED_O.with(std::cell::Cell::get)
}
pub(super) fn provider_position() -> Option<Value> {
    if after::active() {
        return after::provider_position();
    }
    let observer = o_trace::position();
    let keys = before_trace::position();
    if observer.is_none() && keys.is_none() {
        return None;
    }
    Some(json!({"observer":observer,"keys":keys,
        "scope":"actual callback entry position; not decoder I/O"}))
}

pub(super) enum Action {
    Ordinary(OrdinaryAction),
    After(after::Action),
}
pub(super) struct OrdinaryAction {
    name: String,
    entry: &'static str,
    observer: o_trace::CaptureGuard,
    keys: before_trace::CaptureGuard,
}
pub(super) fn begin_action(name: &str, entry: &'static str) -> Option<Action> {
    if let Some(action) = after::begin_action(name, entry) {
        return Some(Action::After(action));
    }
    if !active() {
        return None;
    }
    Some(Action::Ordinary(OrdinaryAction {
        name: name.into(),
        entry,
        observer: o_trace::begin(),
        keys: before_trace::begin_keys(),
    }))
}
fn observer_fields(value: &Value, expected: &[&str]) {
    let actual = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        actual,
        expected
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
    );
}
fn observer_uint(value: &Value) -> u64 {
    value.as_u64().unwrap()
}
fn observer_optional_id(value: &Value) -> Option<u64> {
    if value.is_null() {
        None
    } else {
        Some(observer_uint(value))
    }
}
fn observer_caller(value: &Value) -> &str {
    let caller = value.as_str().unwrap();
    assert!(matches!(
        caller,
        "BuildStageBlock" | "SavedKeyOutputBits" | "RoutedLegacy"
    ));
    caller
}
fn observer_result(value: &Value) {
    match value["status"].as_str().unwrap() {
        "Ok" => observer_fields(value, &["status"]),
        "Err" => {
            observer_fields(value, &["status", "error"]);
            assert!(value["error"].is_object());
        }
        _ => panic!("unknown O helper result status"),
    }
}
fn observer_payload_bytes(value: &Value) -> usize {
    value["calls"]
        .as_array()
        .unwrap()
        .iter()
        .chain(value["rows"].as_array().unwrap())
        .try_fold(0usize, |sum, row| {
            sum.checked_add(serde_json::to_vec(row).unwrap().len())
        })
        .unwrap()
}
#[derive(Default)]
struct ObserverFrames {
    seen: BTreeMap<u64, Value>,
    closed: std::collections::BTreeSet<u64>,
    previous: Vec<u64>,
    highest: u64,
}
impl ObserverFrames {
    fn position(
        &mut self,
        value: &Value,
        ordinal: u64,
        calls: &[Value],
        available_calls: u64,
        row_phase: &str,
    ) {
        observer_fields(
            value,
            &[
                "schema",
                "last_attempted_ordinal",
                "selected_ticket",
                "active_stack",
            ],
        );
        assert_eq!(value["schema"], "stage-block-policy-before-position-v1");
        assert_eq!(observer_uint(&value["last_attempted_ordinal"]), ordinal);
        let selected = observer_optional_id(&value["selected_ticket"]);
        let stack = value["active_stack"].as_array().unwrap();
        assert!(stack.len() <= 128);
        if matches!(row_phase, "context_enter" | "caller_enter") {
            let top = stack.last().unwrap();
            assert!(!self.seen.contains_key(&observer_uint(&top["frame"])));
            assert_eq!(top["kind"] == "caller", row_phase == "caller_enter");
        }
        let mut ids = Vec::new();
        let mut previous_id = 0;
        for frame in stack {
            observer_fields(
                frame,
                &["frame", "ticket", "kind", "phase", "index", "caller"],
            );
            let id = observer_uint(&frame["frame"]);
            assert!(id > previous_id);
            previous_id = id;
            ids.push(id);
            let ticket = observer_optional_id(&frame["ticket"]);
            let index = observer_optional_id(&frame["index"]);
            let caller = if frame["caller"].is_null() {
                None
            } else {
                Some(observer_caller(&frame["caller"]))
            };
            if let Some(ticket) = ticket {
                assert!(ticket > 0 && ticket <= available_calls);
                assert_eq!(
                    caller,
                    Some(observer_caller(&calls[ticket as usize - 1]["caller"]))
                );
            } else {
                assert!(caller.is_none());
            }
            let kind = frame["kind"].as_str().unwrap();
            let phase = frame["phase"].as_str().unwrap();
            match kind {
                "caller" => {
                    assert!(ticket.is_some());
                    assert_eq!(phase, "caller");
                    assert!(index.is_none());
                }
                "ordinary_query" => {
                    assert!(ticket.is_some());
                    assert_eq!(phase, "ordinary_query");
                    assert!(index.is_none());
                }
                "operation" => {
                    assert!(ticket.is_some());
                    assert!(matches!(
                        phase,
                        "query_limits"
                            | "policy_query"
                            | "manual_debit"
                            | "content_preflight"
                            | "control_check"
                            | "apply_suppression"
                            | "merged_suppression"
                            | "writer_push"
                    ));
                    assert_eq!(index.is_some(), phase == "content_preflight");
                }
                "build_stage"
                | "scan_saved_stage_key"
                | "reserve_stream"
                | "recorder_schema_context" => {
                    assert_eq!(phase, kind);
                    assert!(ticket.is_none() && index.is_none());
                }
                _ => panic!("unknown O frame kind"),
            }
            if let Some(previous) = self.seen.get(&id) {
                assert_eq!(previous, frame);
            }
        }
        let shared = self
            .previous
            .iter()
            .zip(&ids)
            .take_while(|(a, b)| a == b)
            .count();
        self.closed.extend(self.previous[shared..].iter().copied());
        for (id, frame) in ids[shared..].iter().zip(&stack[shared..]) {
            assert!(!self.closed.contains(id));
            if self.seen.is_empty() {
                assert_eq!(*id, 1);
            }
            assert!(*id > self.highest);
            match frame["kind"].as_str().unwrap() {
                "caller" => assert_eq!(row_phase, "caller_enter"),
                "ordinary_query" => assert_eq!(row_phase, "policy_query_result"),
                "operation" => {
                    let expected = match frame["phase"].as_str().unwrap() {
                        "query_limits" => "query_limits_result",
                        "policy_query" => "policy_query_result",
                        "manual_debit" => "manual_debit_result",
                        "content_preflight" => "content_preflight_result",
                        "control_check" => "control_check_result",
                        "apply_suppression" => "apply_suppression_result",
                        "merged_suppression" => "merged_return_vec",
                        "writer_push" => "writer_push_result",
                        _ => unreachable!(),
                    };
                    assert_eq!(row_phase, expected);
                }
                _ => assert_eq!(row_phase, "context_enter"),
            }
            assert!(self.seen.insert(*id, frame.clone()).is_none());
            self.highest = *id;
        }
        let expected = stack
            .iter()
            .rev()
            .find(|frame| frame["kind"] == "caller")
            .and_then(|frame| {
                if frame["caller"] == "BuildStageBlock" {
                    observer_optional_id(&frame["ticket"])
                } else {
                    None
                }
            });
        assert_eq!(selected, expected);
        self.previous = ids;
    }
}
fn observer_call(call: &Value, ordinal: u64) {
    observer_fields(
        call,
        &[
            "ordinal",
            "parent",
            "caller",
            "selected",
            "context",
            "helper_result",
        ],
    );
    assert_eq!(observer_uint(&call["ordinal"]), ordinal);
    let caller = observer_caller(&call["caller"]);
    assert_eq!(
        call["selected"].as_bool(),
        Some(caller == "BuildStageBlock")
    );
    assert!(call["context"].is_object());
    observer_result(&call["helper_result"]);
    if let Some(parent) = observer_optional_id(&call["parent"]) {
        assert!(parent > 0 && parent < ordinal);
    }
}
fn observer_row_caller(position: &Value, ticket: u64, calls: &[Value]) {
    let stack = position["active_stack"].as_array().unwrap();
    let index = stack
        .iter()
        .rposition(|frame| frame["kind"] == "caller")
        .expect("nonambient O row requires an active caller");
    let caller = &stack[index];
    assert_eq!(observer_optional_id(&caller["ticket"]), Some(ticket));
    let call = &calls[ticket as usize - 1];
    assert_eq!(caller["caller"], call["caller"]);
    let parent = stack[..index]
        .iter()
        .rev()
        .find_map(|frame| observer_optional_id(&frame["ticket"]));
    assert_eq!(observer_optional_id(&call["parent"]), parent);
}
fn validate_observer(value: &Value) -> usize {
    observer_fields(
        value,
        &[
            "schema",
            "attempted",
            "dropped",
            "invalid",
            "overflow",
            "selected_calls",
            "active_at_take",
            "calls",
            "rows",
            "payload_bytes",
            "bounds",
            "observer_storage",
            "ordinary_ownership",
            "layout_scope",
            "query_capacity_scope",
        ],
    );
    assert_eq!(value["schema"], "stage-block-policy-before-v1");
    assert_eq!(observer_uint(&value["dropped"]), 0);
    assert_eq!(value["invalid"].as_bool(), Some(false));
    assert_eq!(value["overflow"].as_bool(), Some(false));
    assert!(value["active_at_take"].as_array().unwrap().is_empty());
    assert_eq!(
        value["observer_storage"],
        "post-construction serialized retained calls/rows accounting and final document check; not physical RSS; snapshots/JSON temporaries outside request ledger"
    );
    assert_eq!(
        value["ordinary_ownership"],
        "unadmitted source Vec and inventory; no O source lease, Admission callback, output account or handoff"
    );
    assert_eq!(
        value["layout_scope"],
        "capacity-derived Layout observations after actual construction, not allocator requests"
    );
    assert_eq!(
        value["query_capacity_scope"],
        "ordinary outer Vec capacities only; nested heaps unexposed"
    );
    observer_fields(
        &value["bounds"],
        &["rows", "calls", "active_frames", "serialized_payload_bytes"],
    );
    for (field, expected) in [
        ("rows", 32768),
        ("calls", 4096),
        ("active_frames", 128),
        ("serialized_payload_bytes", 64 * 1024 * 1024),
    ] {
        assert_eq!(observer_uint(&value["bounds"][field]), expected);
    }
    let calls = value["calls"].as_array().unwrap();
    let rows = value["rows"].as_array().unwrap();
    assert!(calls.len() <= 4096 && rows.len() <= 32768);
    assert_eq!(observer_uint(&value["attempted"]), rows.len() as u64);
    let selected = calls
        .iter()
        .filter(|call| call["selected"].as_bool() == Some(true))
        .count();
    assert_eq!(observer_uint(&value["selected_calls"]), selected as u64);
    for (index, call) in calls.iter().enumerate() {
        observer_call(call, (index + 1) as u64);
    }
    let mut frames = ObserverFrames::default();
    let mut entered = std::collections::BTreeSet::new();
    let mut completed = std::collections::BTreeSet::new();
    for (index, row) in rows.iter().enumerate() {
        observer_fields(
            row,
            &[
                "ordinal",
                "ticket",
                "phase",
                "position",
                "value",
                "snapshot",
                "n_before_position",
                "n_native_position",
            ],
        );
        let ordinal = (index + 1) as u64;
        assert_eq!(observer_uint(&row["ordinal"]), ordinal);
        let ticket = observer_optional_id(&row["ticket"]);
        let phase = row["phase"].as_str().unwrap();
        let ambient = matches!(
            phase,
            "context_enter"
                | "stream_reservation_before"
                | "stream_reservation_result"
                | "stream_reservation_accepted"
                | "stream_release_before"
                | "stream_release_after"
        );
        assert!(
            ambient
                || matches!(
                    phase,
                    "caller_enter"
                        | "returned_vec_discarded_before_push"
                        | "writer_push_result"
                        | "writer_after_push"
                        | "helper_enter"
                        | "range_result"
                        | "query_limits_result"
                        | "policy_query_result"
                        | "manual_debit_before"
                        | "manual_debit_result"
                        | "content_preflight_before"
                        | "content_preflight_result"
                        | "content_preflight_accepted"
                        | "control_check_before"
                        | "control_check_result"
                        | "apply_suppression_before"
                        | "apply_suppression_result"
                        | "apply_suppression_pcm_after"
                        | "merge_before"
                        | "merged_return_vec"
                        | "helper_result"
                        | "helper_final_pcm"
                        | "returned_vec_discarded_before_output_bits"
                        | "returned_vec_retained_by_routed_legacy"
                )
        );
        if ambient {
            assert!(ticket.is_none());
        } else {
            let ticket = ticket.unwrap();
            assert!(ticket > 0 && ticket <= calls.len() as u64);
            if phase == "caller_enter" {
                assert_eq!(ticket, entered.len() as u64 + 1);
                assert!(entered.insert(ticket));
            } else {
                assert!(entered.contains(&ticket));
            }
        }
        frames.position(
            &row["position"],
            ordinal,
            calls,
            entered.len() as u64,
            phase,
        );
        if !ambient {
            observer_row_caller(&row["position"], ticket.unwrap(), calls);
        }
        if phase == "caller_enter" {
            let ticket = ticket.unwrap();
            let stack = row["position"]["active_stack"].as_array().unwrap();
            let caller = stack.last().unwrap();
            assert_eq!(caller["kind"], "caller");
            assert_eq!(observer_optional_id(&caller["ticket"]), Some(ticket));
            let parent = stack[..stack.len() - 1]
                .iter()
                .rev()
                .find_map(|frame| observer_optional_id(&frame["ticket"]));
            assert_eq!(
                observer_optional_id(&calls[ticket as usize - 1]["parent"]),
                parent
            );
        }
        if phase == "helper_result" {
            let ticket = ticket.unwrap();
            assert!(completed.insert(ticket));
            assert_eq!(row["value"], calls[ticket as usize - 1]["helper_result"]);
        }
        assert!(row["value"].is_object());
        observer_fields(&row["snapshot"], &["work", "controller", "progress"]);
        if matches!(phase, "stream_release_before" | "stream_release_after") {
            assert!(row["snapshot"]["work"].is_null());
        } else {
            assert!(row["snapshot"]["work"].is_object());
        }
        assert!(row["snapshot"]["controller"].is_object());
        assert!(row["snapshot"]["progress"].is_null() || row["snapshot"]["progress"].is_object());
        for field in ["n_before_position", "n_native_position"] {
            assert!(row[field].is_null() || row[field].is_object());
        }
    }
    assert_eq!(entered.len(), calls.len());
    assert_eq!(completed, entered);
    let payload = observer_payload_bytes(value);
    assert_eq!(observer_uint(&value["payload_bytes"]), payload as u64);
    assert!(payload <= 64 * 1024 * 1024);
    assert!(serde_json::to_vec(value).unwrap().len() <= 64 * 1024 * 1024);
    selected
}
pub(super) fn end_action(action: Action) -> Value {
    let action = match action {
        Action::Ordinary(action) => action,
        Action::After(action) => return after::end_action(action),
    };
    let keys = before_trace::take_keys();
    let observer = o_trace::take();
    drop(action.keys);
    drop(action.observer);
    let selected = validate_observer(&observer);
    key_replay::valid_keys(&keys);
    json!({"schema":"stage-block-policy-before-action-v1","action":action.name,
        "entry":action.entry,"observer":observer,"keys":keys,
        "selected_calls":selected,
        "zero_selected_calls":selected==0,
        "existing_mask_ops":point_suppression::ops::trace::snapshot(),
        "existing_public_owner_work":point_suppression::work_trace::snapshot(),
        "supplemental_counter_scope":"actual existing cumulative group counters; no O admission-local attribution"})
}

fn private_action(
    plan: &Arc<RenderPlan>,
    target: PrivateTarget<'_, '_>,
    inner: NativeProvider,
    name: &str,
    limits: StageLimits,
    options: PrivateOptions,
) -> Value {
    if after::active() {
        after::private_action(plan, target, inner, name, limits, options)
    } else {
        super::private_action(plan, target, inner, name, limits, options)
    }
}

fn setup_result(name: &str, value: SetupResult<Value>) -> Value {
    match value {
        Ok(value) => value,
        Err(error) => setup_error(name, error.as_ref()),
    }
}
fn with_provider(
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
    action: impl FnOnce(NativeProvider) -> Value,
) -> Value {
    match reopen() {
        Ok(inner) => action(inner),
        Err(error) => setup_error("actual_qualified_native_reopen", error.as_ref()),
    }
}
fn public_control(
    plan: &Arc<RenderPlan>,
    target: PublicTarget<'_, '_>,
    inner: NativeProvider,
    request: (i64, u32),
    name: &str,
    cancelled: bool,
) -> Value {
    let _ops = mask_observations();
    let (mut renderer, monitor) = super::renderer(plan, StageLimits::default(), None);
    let mut source = super::provider(inner, None, false);
    public_once(
        &mut renderer,
        &mut source,
        target,
        request,
        name,
        &monitor,
        &AtomicBool::new(cancelled),
    )
}
fn not_run(reason: &str) -> Value {
    json!({"attempted":false,"reason":reason})
}
fn plain_cases(
    plan: &Arc<RenderPlan>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
    families: &mut BTreeMap<&'static str, Value>,
    controls: &mut BTreeMap<&'static str, Value>,
) {
    match plan.audio_definition(AudioDefinitionSelector::Node { node: id("outer") }) {
        Ok(definition) => {
            let target = PublicTarget::Definition(&definition);
            families.insert(
                "nested_public_Signal",
                setup_result(
                    "actual_nested_public_pair",
                    public_pair(plan, target, reopen, (0, 256), "O_nested_public_Signal"),
                ),
            );
            families.insert(
                "persistent_nested",
                setup_result("actual_persistent_nested", persistent(plan, target, reopen)),
            );
            for (name, request, cancelled) in [
                ("cancelled_public", (0, 256), true),
                ("zero_public_frames", (0, 0), false),
            ] {
                controls.insert(
                    name,
                    with_provider(reopen, |inner| {
                        public_control(plan, target, inner, request, name, cancelled)
                    }),
                );
            }
            match extract_stage(definition.signal()) {
                Ok(stage) => {
                    families.insert(
                        "streamed_Signal",
                        with_provider(reopen, |inner| {
                            private_action(
                                plan,
                                PrivateTarget::Stage(&stage),
                                inner,
                                "O_streamed_Signal",
                                StageLimits::default(),
                                PrivateOptions {
                                    repeat: true,
                                    ..Default::default()
                                },
                            )
                        }),
                    );
                    controls.insert(
                        "nested_depth1",
                        with_provider(reopen, |inner| {
                            private_action(
                                plan,
                                PrivateTarget::Stage(&stage),
                                inner,
                                "O_nested_depth1",
                                StageLimits {
                                    maximum_depth: 1,
                                    ..Default::default()
                                },
                                PrivateOptions::default(),
                            )
                        }),
                    );
                }
                Err(error) => {
                    families.insert("streamed_Signal", setup_plan_error(error));
                    controls.insert(
                        "nested_depth1",
                        not_run("checked query exposed no plain Stage"),
                    );
                }
            }
        }
        Err(error) => {
            families.insert("nested_public_Signal", setup_plan_error(error));
            for name in ["streamed_Signal", "persistent_nested"] {
                families.insert(name, not_run("outer definition construction rejected"));
            }
            for name in ["cancelled_public", "zero_public_frames", "nested_depth1"] {
                controls.insert(name, not_run("outer definition construction rejected"));
            }
        }
    }
    match routed_definition_source(plan) {
        Ok(domain) => {
            families.insert(
                "complete_Source_route_control",
                setup_result(
                    "actual_complete_Source_route",
                    public_pair(
                        plan,
                        PublicTarget::Routed(&domain),
                        reopen,
                        (0, 256),
                        "O_complete_Source_route_control",
                    ),
                ),
            );
        }
        Err(error) => {
            families.insert("complete_Source_route_control", setup_plan_error(error));
        }
    }
}
fn fixture_cases(
    plan: &Arc<RenderPlan>,
    base: &Arc<AudioStageProjection<'_>>,
    asset: &deadpan_core::AssetRecord,
    wire: &Value,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
    provenance: Value,
) -> Value {
    let projection_before = trace::projection_value(base, plan);
    let mut families = BTreeMap::<&'static str, Value>::new();
    let mut controls = BTreeMap::<&'static str, Value>::new();
    let plain = match plain_nested_plan(asset, wire) {
        Ok((plain, document)) => {
            plain_cases(&plain, reopen, &mut families, &mut controls);
            json!({"setup":"Ok","checked_document":document})
        }
        Err(error) => {
            for name in [
                "nested_public_Signal",
                "streamed_Signal",
                "persistent_nested",
                "complete_Source_route_control",
            ] {
                families.insert(name, not_run("checked plain nested document rejected"));
            }
            for name in ["cancelled_public", "zero_public_frames", "nested_depth1"] {
                controls.insert(name, not_run("checked plain nested document rejected"));
            }
            setup_error("checked_plain_nested_document", error.as_ref())
        }
    };
    families.insert(
        "projected_PreserveTape",
        with_provider(reopen, |inner| {
            private_action(
                plan,
                PrivateTarget::Projected(base),
                inner,
                "O_projected_PreserveTape",
                StageLimits::default(),
                PrivateOptions {
                    repeat: true,
                    ..Default::default()
                },
            )
        }),
    );
    match signed_tape(plan, base.stage().input_signal()) {
        Ok(tape) => {
            families.insert(
                "signed_fractional_Tape_attempt",
                with_provider(reopen, |inner| {
                    public_control(
                        plan,
                        PublicTarget::Tape(&tape),
                        inner,
                        (0, 8),
                        "O_signed_fractional_Tape_attempt",
                        false,
                    )
                }),
            );
        }
        Err(error) => {
            families.insert("signed_fractional_Tape_attempt", setup_plan_error(error));
        }
    }
    let bound_document = match super::bound_document(asset) {
        Ok((bound, document)) => {
            let action = match bound_tape(&bound) {
                Ok(tape) => with_provider(reopen, |inner| {
                    public_control(
                        &bound,
                        PublicTarget::Tape(&tape),
                        inner,
                        (0, 32),
                        "O_genuine_Bound_attempt",
                        false,
                    )
                }),
                Err(error) => setup_plan_error(error),
            };
            families.insert("genuine_Bound_attempt", action);
            json!({"setup":"Ok","checked_public_binding_document":document})
        }
        Err(error) => {
            families.insert(
                "genuine_Bound_attempt",
                not_run("checked Bound document rejected"),
            );
            setup_error("checked_public_Bound_document", error.as_ref())
        }
    };
    for (name, options, limits) in [
        (
            "expired_private",
            PrivateOptions {
                expired: true,
                ..Default::default()
            },
            StageLimits::default(),
        ),
        (
            "input_limit639",
            PrivateOptions::default(),
            StageLimits {
                maximum_input_frames: 639,
                ..Default::default()
            },
        ),
        (
            "qualified_provider_refusal",
            PrivateOptions {
                refuse_provider: true,
                ..Default::default()
            },
            StageLimits::default(),
        ),
    ] {
        controls.insert(
            name,
            with_provider(reopen, |inner| {
                private_action(
                    plan,
                    PrivateTarget::Projected(base),
                    inner,
                    name,
                    limits,
                    options,
                )
            }),
        );
    }
    controls.insert(
        "negative_public_Tape",
        match base.input_tape() {
            Some(tape) => with_provider(reopen, |inner| {
                public_control(
                    plan,
                    PublicTarget::Tape(tape),
                    inner,
                    (-1, 1),
                    "O_negative_public_Tape",
                    false,
                )
            }),
            None => not_run("actual registered projection input is not Tape"),
        },
    );
    let projection_after = trace::projection_value(base, plan);
    assert_eq!(projection_before, projection_after);
    assert_eq!(families.len(), 7);
    assert_eq!(controls.len(), 7);
    json!({"provenance":provenance,"projection_before":projection_before,
        "projection_after":projection_after,"plain_document_setup":plain,
        "bound_document_setup":bound_document,"families":families,"controls":controls,
        "declared_normal_action_upper_bound":12,"declared_control_action_upper_bound":7,
        "actual_calls":"counted from action receipts; constructor failures and private first-failure skips remain literal",
        "residency":"ordinary legal defaults and actual controller/work snapshots only; no O lease or seeded budget",
        "faults":"real early cancellation/deadline/depth/input/provider/range controls; no O Admission callback namespace"})
}
pub(super) fn fixture_visit(
    plan: &Arc<RenderPlan>,
    base: &Arc<AudioStageProjection<'_>>,
    asset: &deadpan_core::AssetRecord,
    wire: &Value,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
    provenance: impl FnOnce() -> SetupResult<Value>,
) -> SetupResult<Option<Value>> {
    if after::active() {
        return after::fixture_visit(plan, base, asset, wire, reopen, provenance);
    }
    if !active() {
        return Ok(None);
    }
    Ok(Some(fixture_cases(
        plan,
        base,
        asset,
        wire,
        reopen,
        provenance()?,
    )))
}
fn observed_counts(value: &Value) -> Value {
    fn visit(
        value: &Value,
        actions: &mut u64,
        calls: &mut u64,
        selected: &mut u64,
        zero: &mut u64,
    ) {
        if value["schema"] == "stage-block-policy-before-action-v1" {
            *actions += 1;
            let rows = value["observer"]["calls"].as_array().unwrap();
            *calls += rows.len() as u64;
            let count = rows.iter().filter(|row| row["selected"] == true).count() as u64;
            *selected += count;
            *zero += u64::from(count == 0);
            return;
        }
        match value {
            Value::Object(rows) => {
                for value in rows.values() {
                    visit(value, actions, calls, selected, zero);
                }
            }
            Value::Array(rows) => {
                for value in rows {
                    visit(value, actions, calls, selected, zero);
                }
            }
            _ => {}
        }
    }
    let (mut actions, mut calls, mut selected, mut zero) = (0, 0, 0, 0);
    visit(value, &mut actions, &mut calls, &mut selected, &mut zero);
    json!({"actual_actions":actions,"all_helper_calls":calls,"selected_BuildStageBlock_calls":selected,
        "unselected_helper_calls":calls-selected,"zero_selected_actions":zero,
        "scope":"actual completed action receipts; no positive mode/lane/reachability inferred from a family name"})
}
fn capture_body() -> Value {
    crate::stages::stage_block_policy_request::with_legacy_stage_block_policy(
        capture_body_historical,
    )
}
fn capture_body_historical() -> Value {
    let _focus = focus();
    let actual = match super::super::family() {
        Ok(value) => value,
        Err(error) => setup_error("actual_registered_WAV_fixture", error.as_ref()),
    };
    let counts = observed_counts(&actual);
    let keys = key_replay::validate_body(&actual);
    json!({"schema":"stage-block-policy-ordinary-before-v1","actual":actual,
        "observed_counts":counts,"current_key_proof":keys,
        "qualification_status":"actual body unadjudicated; independent reachability and freeze required",
        "scope":{"selected":"BuildStageBlock after native DSP read/check and before writer push",
            "unselected":["SavedKeyOutputBits","RoutedLegacy"],
            "current_prior_units":["N","M","L","J"],
            "normal_action_upper_bound":12,"control_action_upper_bound":7,
            "mode_and_lane_obligations":"actual Signal and PreserveTape, Plan and nested child Cache required before qualification",
            "ordinary_local_split":"failed construction canonical/additional and admission-local split unexposed",
            "capacities":"actual source policy and returned merged Vec separately; capacity-derived Layout, not allocator request",
            "no_claims":["O Admission/source lease/custody","positive Tape output","Bound or Source reach by label",
                "late provider callback from content preflight","whole preparation/graph/PCM/dependency admission","product Gate"],
            "observation_storage":"unadmitted test JSON/readback storage outside request ledger",
            "checkpoint":CHECKPOINT_O}})
}

// These are recorder-schema fixtures only. No media, helper or policy operation runs.
fn observer_schema_context_capture() -> Value {
    let document = ProjectDocument::new(
        ProjectId::new("O-recorder-schema").unwrap(),
        RevisionId::new("schema-only").unwrap(),
        deadpan_core::PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: deadpan_core::ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            id("root"),
            BeatNode::sequence("Schema-only root", vec![id("silence")]),
        ),
        (
            id("silence"),
            BeatNode::hold(
                "Schema-only silence",
                HoldRecipe {
                    picture_context: None,
                    duration: FrameDuration::new(128).unwrap(),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ),
        ),
    ]))
    .unwrap();
    let checked = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let renderer = StageAudio::new(Arc::new(RenderPlan::compile(&checked).unwrap()));
    let budget = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = WorkControl {
        work: &budget,
        cancelled: &cancelled,
        deadline: Instant::now() + TIMEOUT,
    };
    let guard = o_trace::begin();
    {
        let _outer = o_trace::context_scope(
            &renderer,
            control,
            "recorder_schema_context",
            || json!({"scope":"schema only; no helper/media/native policy witness"}),
        );
        for _ in 0..2 {
            let _inner = o_trace::context_scope(
                &renderer,
                control,
                "recorder_schema_context",
                || json!({"scope":"schema only; nested actual recorder frame"}),
            );
        }
    }
    let value = o_trace::take();
    drop(guard);
    assert_eq!(validate_observer(&value), 0);
    value
}
fn observer_schema_reject(value: Value, refresh_payload: bool, name: &str) {
    let mut value = value;
    if refresh_payload {
        value["payload_bytes"] = json!(observer_payload_bytes(&value));
    }
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| validate_observer(&value)))
            .is_err(),
        "O schema mutation accepted: {name}"
    );
}
#[test]
fn stage_block_policy_before_observer_schema_rejects_damage() {
    let guard = o_trace::begin();
    let empty = o_trace::take();
    drop(guard);
    assert_eq!(validate_observer(&empty), 0);
    let context = observer_schema_context_capture();
    assert_eq!(context["calls"], json!([]));
    assert_eq!(context["rows"].as_array().unwrap().len(), 3);
    for field in empty.as_object().unwrap().keys() {
        let mut changed = empty.clone();
        changed.as_object_mut().unwrap().remove(field);
        observer_schema_reject(changed, false, &format!("missing envelope {field}"));
    }
    let mut changed = empty.clone();
    changed["unexpected"] = Value::Null;
    observer_schema_reject(changed, false, "extra envelope field");
    for field in ["attempted", "dropped", "selected_calls", "payload_bytes"] {
        let mut changed = empty.clone();
        changed[field] = json!("0");
        observer_schema_reject(changed, false, &format!("string counter {field}"));
    }
    for field in ["invalid", "overflow"] {
        let mut changed = empty.clone();
        changed[field] = json!("false");
        observer_schema_reject(changed, false, &format!("string flag {field}"));
    }
    for field in ["calls", "rows", "active_at_take"] {
        let mut changed = empty.clone();
        changed[field] = Value::Null;
        observer_schema_reject(changed, false, &format!("null table {field}"));
    }
    for field in ["attempted", "selected_calls", "payload_bytes"] {
        let mut changed = empty.clone();
        changed[field] = json!(1);
        observer_schema_reject(changed, false, &format!("derived counter {field}"));
    }
    for field in ["rows", "calls", "active_frames", "serialized_payload_bytes"] {
        let mut changed = empty.clone();
        changed["bounds"].as_object_mut().unwrap().remove(field);
        observer_schema_reject(changed, false, &format!("missing bound {field}"));
        let mut changed = empty.clone();
        changed["bounds"][field] = json!("0");
        observer_schema_reject(changed, false, &format!("string bound {field}"));
        let mut changed = empty.clone();
        changed["bounds"][field] = json!(0);
        observer_schema_reject(changed, false, &format!("changed bound {field}"));
    }
    let mut changed = empty.clone();
    changed["bounds"]["unexpected"] = Value::Null;
    observer_schema_reject(changed, false, "extra bound");
    for field in context["rows"][0].as_object().unwrap().keys() {
        let mut changed = context.clone();
        changed["rows"][0].as_object_mut().unwrap().remove(field);
        observer_schema_reject(changed, true, &format!("missing row {field}"));
    }
    let mut changed = context.clone();
    changed["rows"][0]["unexpected"] = Value::Null;
    observer_schema_reject(changed, true, "extra row");
    for field in context["rows"][0]["position"].as_object().unwrap().keys() {
        let mut changed = context.clone();
        changed["rows"][0]["position"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        observer_schema_reject(changed, true, &format!("missing position {field}"));
    }
    let mut changed = context.clone();
    changed["rows"][0]["position"]["unexpected"] = Value::Null;
    observer_schema_reject(changed, true, "extra position");
    for field in context["rows"][0]["position"]["active_stack"][0]
        .as_object()
        .unwrap()
        .keys()
    {
        let mut changed = context.clone();
        changed["rows"][0]["position"]["active_stack"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        observer_schema_reject(changed, true, &format!("missing frame {field}"));
    }
    let mut changed = context.clone();
    changed["rows"][0]["position"]["active_stack"][0]["unexpected"] = Value::Null;
    observer_schema_reject(changed, true, "extra frame");
    for pointer in [
        "/rows/0/ticket",
        "/rows/0/position/selected_ticket",
        "/rows/0/position/active_stack/0/ticket",
        "/rows/0/position/active_stack/0/index",
    ] {
        let mut changed = context.clone();
        *changed.pointer_mut(pointer).unwrap() = json!("1");
        observer_schema_reject(changed, true, &format!("string optional ID {pointer}"));
    }
    for (pointer, replacement) in [
        ("/rows/0/ordinal", json!(2)),
        ("/rows/0/position/last_attempted_ordinal", json!(2)),
        ("/rows/0/ticket", json!(1)),
        ("/rows/0/position/selected_ticket", json!(1)),
        ("/rows/0/position/active_stack/0/frame", json!(0)),
        ("/rows/0/position/active_stack/0/frame", json!(100)),
        ("/rows/0/position/active_stack/0/ticket", json!(1)),
        (
            "/rows/0/position/active_stack/0/caller",
            json!("BuildStageBlock"),
        ),
        ("/rows/0/position/active_stack/0/kind", json!("caller")),
        ("/rows/0/position/active_stack/0/phase", json!("changed")),
        ("/rows/0/position/active_stack/0/index", json!(0)),
        ("/rows/0/value", Value::Null),
        ("/rows/0/snapshot/work", json!("missing work")),
        ("/rows/0/position/active_stack", Value::Null),
        ("/rows/2/position/active_stack/1/frame", json!(2)),
    ] {
        let mut changed = context.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        observer_schema_reject(changed, true, pointer);
    }
    let mut changed = context.clone();
    changed["rows"][0]["position"]["active_stack"]
        .as_array_mut()
        .unwrap()
        .push(context["rows"][0]["position"]["active_stack"][0].clone());
    observer_schema_reject(changed, true, "duplicate active frame");
    let mut changed = context.clone();
    changed["rows"][1]["position"]["active_stack"][0]["phase"] = json!("changed");
    observer_schema_reject(changed, true, "changed existing frame identity");
    let mut changed = context.clone();
    changed["rows"][1]["position"]["active_stack"][0]["kind"] = json!("build_stage");
    changed["rows"][1]["position"]["active_stack"][0]["phase"] = json!("build_stage");
    observer_schema_reject(changed, true, "changed coherent existing frame identity");
    let mut changed = context.clone();
    for row in changed["rows"].as_array_mut().unwrap() {
        for frame in row["position"]["active_stack"].as_array_mut().unwrap() {
            frame["frame"] = json!(observer_uint(&frame["frame"]) + 100);
        }
    }
    observer_schema_reject(changed, true, "shifted entire frame-ID namespace");

    // Direct shape checks cannot be masked by whole-trace completion checks.
    let call = json!({"ordinal":1,"parent":null,"caller":"RoutedLegacy","selected":false,
        "context":{"schema_only_call_shape":true},"helper_result":{"status":"Ok"}});
    observer_call(&call, 1);
    for field in call.as_object().unwrap().keys() {
        let mut damaged = call.clone();
        damaged.as_object_mut().unwrap().remove(field);
        observer_call_shape_reject(&damaged, &format!("missing call {field}"));
    }
    for (field, replacement) in [
        ("parent", json!("1")),
        ("parent", json!(0)),
        ("parent", json!(1)),
        ("ordinal", json!("1")),
        ("ordinal", json!(2)),
        ("caller", json!("UnknownCaller")),
        ("selected", json!("false")),
        ("selected", json!(true)),
        ("context", Value::Null),
        ("helper_result", Value::Null),
    ] {
        let mut damaged = call.clone();
        damaged[field] = replacement;
        observer_call_shape_reject(&damaged, &format!("damaged call {field}"));
    }
    let mut damaged = call;
    damaged["unexpected"] = Value::Null;
    observer_call_shape_reject(&damaged, "extra call field");
    assert!(o_trace::position().is_none());
    assert!(before_trace::position().is_none());
}

fn observer_call_shape_reject(value: &Value, name: &str) {
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer_call(value, 1))).is_err(),
        "O call shape mutation accepted: {name}"
    );
}
// Synthetic protocol metadata tests attribution only; it is never native capture evidence.
fn observer_schema_serial_call_table() -> Value {
    let context = observer_schema_context_capture();
    let guard = o_trace::begin();
    let mut value = o_trace::take();
    drop(guard);
    let mut calls = Vec::new();
    let mut rows = Vec::new();
    for ticket in 1..=2u64 {
        calls.push(
            json!({"ordinal":ticket,"parent":null,"caller":"RoutedLegacy",
            "selected":false,"context":{"schema_only_protocol_fixture":true},
            "helper_result":{"status":"Ok"}}),
        );
        let frame = json!({"frame":ticket,"ticket":ticket,"kind":"caller","phase":"caller",
            "index":null,"caller":"RoutedLegacy"});
        for phase in ["caller_enter", "helper_result", "helper_final_pcm"] {
            let ordinal = rows.len() as u64 + 1;
            rows.push(json!({"ordinal":ordinal,"ticket":ticket,"phase":phase,
                "position":{"schema":"stage-block-policy-before-position-v1",
                    "last_attempted_ordinal":ordinal,"selected_ticket":null,
                    "active_stack":[frame.clone()]},
                "value":if phase=="helper_result" {json!({"status":"Ok"})}
                    else {json!({"schema_only_protocol_fixture":true})},
                "snapshot":context["rows"][0]["snapshot"].clone(),
                "n_before_position":null,"n_native_position":null}));
        }
    }
    value["calls"] = json!(calls);
    value["rows"] = json!(rows);
    value["attempted"] = json!(6);
    value["payload_bytes"] = json!(observer_payload_bytes(&value));
    assert_eq!(validate_observer(&value), 0);
    value
}
#[test]
fn stage_block_policy_before_rows_require_their_active_caller_ticket() {
    let baseline = observer_schema_serial_call_table();
    assert_eq!(validate_observer(&baseline), 0);
    let mut changed = baseline.clone();
    changed["rows"][5]["ticket"] = json!(1);
    changed["payload_bytes"] = json!(observer_payload_bytes(&changed));
    assert_eq!(changed["rows"][5]["ticket"], 1);
    assert_eq!(
        changed["rows"][5]["position"]["active_stack"][0]["ticket"],
        2
    );
    assert_eq!(
        observer_uint(&changed["payload_bytes"]),
        observer_payload_bytes(&changed) as u64
    );
    // Shape, result completion and payload are unchanged; only call2 attribution is wrong.
    observer_schema_reject(changed, false, "call2 PCM row reticketed to entered call1");
    let mut changed = baseline.clone();
    changed["payload_bytes"] = json!(observer_payload_bytes(&changed) + 1);
    observer_schema_reject(
        changed,
        false,
        "otherwise coherent table has wrong payload bytes",
    );
    assert_eq!(validate_observer(&baseline), 0);
}

#[test]
fn stage_block_policy_before_empty_capture_has_zero_selected_calls_and_restores_guards() {
    assert!(o_trace::position().is_none());
    assert!(before_trace::position().is_none());
    let value = {
        let _focus = focus();
        end_action(begin_action("empty_schema", "test_only_no_native_operation").unwrap())
    };
    assert_eq!(value["selected_calls"], 0);
    assert_eq!(value["zero_selected_calls"], true);
    assert!(o_trace::position().is_none());
    assert!(before_trace::position().is_none());
    assert!(!active());
}
#[test]
fn stage_block_policy_before_keys_reject_unknown_fields() {
    let guard = before_trace::begin_keys();
    let mut keys = before_trace::take_keys();
    drop(guard);
    keys["unexpected"] = json!(true);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| key_replay::valid_keys(
            &keys
        )))
        .is_err()
    );
}
#[test]
#[ignore = "manual fresh ordinary O BEFORE capture; no oracle43 expectation"]
fn capture_stage_block_policy_before_v1() {
    let output = std::env::var_os("DEADPAN_STAGE_BLOCK_POLICY_BEFORE_OUTPUT").unwrap();
    let output = Path::new(&output);
    assert!(output.is_absolute() && output.starts_with("/tmp"));
    let variant = std::env::var("DEADPAN_STAGE_BLOCK_POLICY_BEFORE_VARIANT").unwrap();
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_STAGE_BLOCK_POLICY_BEFORE_SOURCE_SHA256").unwrap();
    assert!(source.len() == 64 && source.bytes().all(|value| value.is_ascii_hexdigit()));
    assert_eq!(
        std::env::var("DEADPAN_STAGE_BLOCK_POLICY_BEFORE_PUBLISHED_CHECKPOINT").unwrap(),
        CHECKPOINT_O
    );
    let mut body = capture_body();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    body["published_checkpoint"] = json!(CHECKPOINT_O);
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

#[test]
fn stage_block_policy_frozen_before_key_relation_is_exact() {
    crate::stages::stage_block_policy_request::with_legacy_stage_block_policy(|| {
        let bytes = include_bytes!("../tests/fixtures/stage-block-policy-baseline-v1/corpus.json");
        use sha2::Digest;
        assert_eq!(bytes.len(), 6_796_606);
        assert_eq!(
            sha2::Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "58eedb3de7187e20b3b3a61ba08d75cbc490fa32307985cc1e9facdfa3c924a2"
        );
        let frozen: Value = serde_json::from_slice(bytes).unwrap();
        let current = capture_body();
        key_replay::assert_relation(&frozen, &current);
        key_replay::assert_mutations_rejected(&frozen, &current);
    });
}

#[path = "stage_block_policy_key_replay.rs"]
mod key_replay;

#[path = "stage_block_policy_after_tests.rs"]
mod after;
