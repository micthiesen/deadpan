//! Current-O ordinary saved-key output-bit evidence. Capture storage is outside request admission.
use super::super as o_fixture;
use super::super::super as n_fixture;
use super::*;
use crate::stages::saved_key_output_policy_before_trace as p_trace;

const CHECKPOINT_P: &str = "dfc87a7e0cf18a7af0267bb10dfa8ffaedb3e756";
const STORAGE: &str = "post-construction serialized retained calls/rows/windows accounting and final document check; not physical RSS; snapshots/JSON temporaries and inner O capture outside request ledger";
const P_SCOPE: &str =
    "ordinary saved-key output bits; no P selected Admission, source owner, fault or policy query";
const DISCARDED: &str =
    "ordinary returned merged Vec was discarded by the existing statement; no admitted owner";

struct Collection {
    actions: Vec<Value>,
    pending: Option<Value>,
    current_key_group: Option<usize>,
    key_groups: Vec<Value>,
}
thread_local! {
    static P_COLLECTION: RefCell<Option<Collection>> = const { RefCell::new(None) };
}
struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        P_COLLECTION.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}
fn collect() -> Guard {
    P_COLLECTION.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(slot.is_none());
        *slot = Some(Collection {
            actions: Vec::new(),
            pending: None,
            current_key_group: None,
            key_groups: Vec::new(),
        });
    });
    Guard
}
pub(super) fn active() -> bool {
    P_COLLECTION.with(|slot| slot.borrow().is_some())
}
fn location() -> Value {
    p_trace::position().unwrap()
}
pub(super) fn action_begin(name: &str, entry: &'static str) {
    if !active() {
        return;
    }
    let location = location();
    P_COLLECTION.with(|slot| {
        let mut slot = slot.borrow_mut();
        let collection = slot.as_mut().unwrap();
        assert!(collection.pending.is_none());
        collection.pending = Some(json!({"action":name,"entry":entry,"begin":location}));
    });
}
pub(super) fn action_end(ordinal: usize) {
    if !active() {
        return;
    }
    let location = location();
    P_COLLECTION.with(|slot| {
        let mut slot = slot.borrow_mut();
        let collection = slot.as_mut().unwrap();
        assert_eq!(ordinal, collection.actions.len() + 1);
        let mut value = collection.pending.take().unwrap();
        value["ordinal"] = json!(ordinal);
        value["end"] = location;
        collection.actions.push(value);
    });
}
fn action_ranges() -> Vec<Value> {
    P_COLLECTION.with(|slot| {
        let mut slot = slot.borrow_mut();
        let collection = slot.as_mut().unwrap();
        assert!(collection.pending.is_none());
        std::mem::take(&mut collection.actions)
    })
}

pub(super) enum KeysCapture {
    Owned(before_trace::CaptureGuard),
    Group { ordinal: usize, begin: Value },
}
pub(super) fn begin_keys() -> KeysCapture {
    let group = P_COLLECTION.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|collection| collection.current_key_group)
    });
    match group {
        Some(ordinal) => KeysCapture::Group {
            ordinal,
            begin: before_trace::position().unwrap(),
        },
        None => KeysCapture::Owned(before_trace::begin_keys()),
    }
}
pub(super) fn take_keys(capture: &KeysCapture) -> Value {
    match capture {
        KeysCapture::Owned(_guard) => before_trace::take_keys(),
        KeysCapture::Group { ordinal, begin } => {
            json!({"schema":"P-before-KeysOnly-group-reference-v1","group":ordinal,
                "begin":begin,"end":before_trace::position().unwrap()})
        }
    }
}
pub(super) fn valid_action_keys(keys: &Value) {
    if keys["schema"] == "P-before-KeysOnly-group-reference-v1" {
        exact(keys, &["schema", "group", "begin", "end"]);
        let current = P_COLLECTION.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|collection| collection.current_key_group)
        });
        assert_eq!(current.map(|n| n as u64), Some(uint(&keys["group"])));
    } else {
        o_fixture::key_replay::valid_keys(keys);
    }
}
fn key_group<T>(name: &str, run: impl FnOnce() -> T) -> T {
    let entry = p_trace::position().unwrap();
    let ordinal = P_COLLECTION.with(|slot| {
        let mut slot = slot.borrow_mut();
        let collection = slot.as_mut().unwrap();
        assert!(collection.current_key_group.is_none());
        let ordinal = collection.key_groups.len() + 1;
        collection.current_key_group = Some(ordinal);
        ordinal
    });
    let guard = before_trace::begin_keys();
    let actual = run();
    let keys = before_trace::take_keys();
    drop(guard);
    o_fixture::key_replay::valid_keys(&keys);
    let exit = p_trace::position().unwrap();
    P_COLLECTION.with(|slot| {
        let mut slot = slot.borrow_mut();
        let collection = slot.as_mut().unwrap();
        assert_eq!(collection.current_key_group.take(), Some(ordinal));
        collection
            .key_groups
            .push(json!({"schema":"P-before-KeysOnly-group-v1",
            "ordinal":ordinal,"name":name,"entry":entry,"exit":exit,"keys":keys}));
    });
    actual
}
fn key_groups() -> Vec<Value> {
    P_COLLECTION.with(|slot| {
        let mut slot = slot.borrow_mut();
        let collection = slot.as_mut().unwrap();
        assert!(collection.current_key_group.is_none());
        std::mem::take(&mut collection.key_groups)
    })
}
fn request_keys<'a>(current: &'a Value, receipt: &'a Value) -> &'a Value {
    let keys = &receipt["keys"];
    if keys["schema"] == "P-before-KeysOnly-group-reference-v1" {
        exact(keys, &["schema", "group", "begin", "end"]);
        let ordinal = uint(&keys["group"]);
        assert!(ordinal > 0);
        let group = &values(&current["fixture"], "key_groups")[ordinal as usize - 1];
        assert_eq!(group["ordinal"], ordinal);
        &group["keys"]
    } else {
        assert_eq!(keys["schema"], "read-signal-policy-keys-v1");
        keys
    }
}
fn keys_boundary(keys: &Value, position: &Value) {
    exact(
        position,
        &[
            "schema",
            "last_attempted_ordinal",
            "selected_ticket",
            "active_stack",
            "active_key_streams",
        ],
    );
    assert_eq!(position["schema"], "read-signal-policy-keys-position-v1");
    let ordinal = uint(&position["last_attempted_ordinal"]);
    if ordinal == 0 {
        assert!(position["selected_ticket"].is_null());
        assert!(values(position, "active_stack").is_empty());
        assert!(values(position, "active_key_streams").is_empty());
    } else {
        let actual = &values(keys, "rows")[ordinal as usize - 1]["position"];
        for field in [
            "last_attempted_ordinal",
            "selected_ticket",
            "active_stack",
            "active_key_streams",
        ] {
            assert_eq!(position[field], actual[field]);
        }
    }
}
fn group_references(current: &Value, ranges: &[Value]) {
    let receipts = values(current, "receipts");
    assert_eq!(receipts.len(), ranges.len());
    let groups = values(&current["fixture"], "key_groups");
    assert_eq!(groups.len(), 2);
    let mut used = BTreeSet::new();
    for (index, group) in groups.iter().enumerate() {
        exact(
            group,
            &["schema", "ordinal", "name", "entry", "exit", "keys"],
        );
        assert_eq!(group["schema"], "P-before-KeysOnly-group-v1");
        let ordinal = (index + 1) as u64;
        assert_eq!(uint(&group["ordinal"]), ordinal);
        assert_eq!(
            group["name"],
            if index == 0 {
                "P_public_persistent"
            } else {
                "P_private_persistent_projected"
            }
        );
        o_fixture::key_replay::valid_keys(&group["keys"]);
        let (begin, begin_row, begin_window, active, active_window) = p_position(&group["entry"]);
        let (end, end_row, end_window, closed, closed_window) = p_position(&group["exit"]);
        assert!(
            active.is_empty()
                && closed.is_empty()
                && active_window.is_none()
                && closed_window.is_none()
        );
        assert_eq!(
            end - begin,
            values(&group["keys"], "key_transcripts").len() as u64
        );
        let mut row_frontier = 0;
        let mut scan_frontier = begin;
        let mut p_row_frontier = begin_row;
        let mut window_frontier = begin_window;
        for (request, (receipt, range)) in receipts.iter().zip(ranges).enumerate() {
            let reference = &receipt["keys"];
            if reference["schema"] != "P-before-KeysOnly-group-reference-v1"
                || reference["group"] != ordinal
            {
                continue;
            }
            assert!(used.insert(request));
            assert!(
                if index == 0 {
                    matches!(
                        receipt["action"].as_str(),
                        Some(
                            "P_persistent_public_empty_directory"
                                | "P_persistent_public_warm_fresh_ReadWork"
                        )
                    )
                } else {
                    matches!(
                        receipt["action"].as_str(),
                        Some(
                            "P_persistent_projected_first"
                                | "P_persistent_projected_same_ReadWork_memo"
                        )
                    )
                },
                "only the first renderer's cold/warm or first/memo requests share keys"
            );
            exact(reference, &["schema", "group", "begin", "end"]);
            let (a, ar, aw, ac, a_live) = p_position(&range["begin"]);
            let (b, br, bw, bc, b_live) = p_position(&range["end"]);
            assert!(ac.is_empty() && bc.is_empty() && a_live.is_none() && b_live.is_none());
            assert_eq!(a, scan_frontier);
            assert_eq!(ar, p_row_frontier);
            assert_eq!(aw, window_frontier);
            let start = &reference["begin"];
            let finish = &reference["end"];
            for position in [start, finish] {
                keys_boundary(&group["keys"], position);
                exact(
                    position,
                    &[
                        "schema",
                        "last_attempted_ordinal",
                        "selected_ticket",
                        "active_stack",
                        "active_key_streams",
                    ],
                );
                assert_eq!(position["schema"], "read-signal-policy-keys-position-v1");
                assert!(position["selected_ticket"].is_null());
                assert!(values(position, "active_stack").is_empty());
                assert!(values(position, "active_key_streams").is_empty());
            }
            assert_eq!(uint(&start["last_attempted_ordinal"]), row_frontier);
            let next = uint(&finish["last_attempted_ordinal"]);
            assert!(next >= row_frontier && next <= values(&group["keys"], "rows").len() as u64);
            let streams = &values(&group["keys"], "key_transcripts")
                [(a - begin) as usize..(b - begin) as usize];
            for stream in streams {
                let at = uint(&stream["entry_position"]["last_attempted_ordinal"]);
                assert!(at > row_frontier && at <= next);
            }
            row_frontier = next;
            scan_frontier = b;
            p_row_frontier = br;
            window_frontier = bw;
        }
        assert_eq!(row_frontier, values(&group["keys"], "rows").len() as u64);
        assert_eq!(scan_frontier, end);
        assert_eq!(p_row_frontier, end_row);
        assert_eq!(window_frontier, end_window);
    }
    for (index, receipt) in receipts.iter().enumerate() {
        if matches!(
            receipt["action"].as_str(),
            Some(
                "P_persistent_public_fresh_renderer_same_directory"
                    | "P_persistent_projected_fresh_renderer_fresh_ReadWork"
            )
        ) {
            assert_eq!(receipt["keys"]["schema"], "read-signal-policy-keys-v1");
        }
        assert_eq!(
            used.contains(&index),
            receipt["keys"]["schema"] == "P-before-KeysOnly-group-reference-v1"
        );
    }
}

fn persistent_public(
    plan: &Arc<RenderPlan>,
    target: PublicTarget<'_, '_>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
) -> SetupResult<Value> {
    let _ops = mask_observations();
    let (
        _directory,
        cache,
        before,
        input_before,
        cold,
        after_cold,
        warm,
        after_warm,
        cancelled,
        _first_monitor,
    ) = key_group("P_public_persistent", || -> SetupResult<_> {
        let directory = tempfile::tempdir()?;
        let cache = PcmCache::at(directory.path())?;
        let before = entries(&cache);
        let input_before = n_fixture::input_value(target, plan);
        let (mut first, monitor) =
            n_fixture::renderer(plan, StageLimits::default(), Some(cache.clone()));
        let mut source = provider(reopen()?, None, false);
        let cancelled = AtomicBool::new(false);
        let cold = public_once(
            &mut first,
            &mut source,
            target,
            (0, 256),
            "P_persistent_public_empty_directory",
            &monitor,
            &cancelled,
        );
        let after_cold = entries(&cache);
        let warm = public_once(
            &mut first,
            &mut source,
            target,
            (0, 256),
            "P_persistent_public_warm_fresh_ReadWork",
            &monitor,
            &cancelled,
        );
        let after_warm = entries(&cache);
        drop(source);
        drop(first);
        Ok((
            directory,
            cache,
            before,
            input_before,
            cold,
            after_cold,
            warm,
            after_warm,
            cancelled,
            monitor,
        ))
    })?;
    let (mut fresh, monitor) =
        n_fixture::renderer(plan, StageLimits::default(), Some(cache.clone()));
    let mut source = provider(reopen()?, None, false);
    let restored = public_once(
        &mut fresh,
        &mut source,
        target,
        (0, 256),
        "P_persistent_public_fresh_renderer_same_directory",
        &monitor,
        &cancelled,
    );
    drop(source);
    drop(fresh);
    let input_after = n_fixture::input_value(target, plan);
    assert_eq!(input_before, input_after);
    Ok(
        json!({"setup":"Ok","input_before":input_before,"input_after":input_after,
        "entries_before":before,"cold":cold,"entries_after_cold":after_cold,
        "warm":warm,"entries_after_warm":after_warm,"fresh_renderer":restored,
        "entries_after_fresh":entries(&cache),
        "scope":"real public configuration, unchanged default caps; actual scans and restore/memo effects are separately observed"}),
    )
}
fn persistent_projected(
    plan: &Arc<RenderPlan>,
    base: &Arc<AudioStageProjection<'_>>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
) -> SetupResult<Value> {
    let _ops = mask_observations();
    let (
        _directory,
        cache,
        entries_before,
        cancelled,
        first_row,
        second_row,
        same,
        final_work,
        after_work,
        controller_before_drop,
        after_controller,
        entries_after_first_renderer,
        _first_monitor,
    ) = key_group("P_private_persistent_projected", || -> SetupResult<_> {
        let directory = tempfile::tempdir()?;
        let cache = PcmCache::at(directory.path())?;
        let entries_before = entries(&cache);
        let budget = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let (mut first_renderer, monitor) =
            n_fixture::renderer(plan, StageLimits::default(), Some(cache.clone()));
        let mut source = provider(reopen()?, Some(&budget), false);
        let control = WorkControl {
            work: &budget,
            cancelled: &cancelled,
            deadline: Instant::now() + TIMEOUT,
        };
        let (first, first_row) = private_once(
            &mut first_renderer,
            &mut source,
            PrivateTarget::Projected(base),
            control,
            &monitor,
            "P_persistent_projected_first",
        );
        let second = if first.is_ok() {
            Some(private_once(
                &mut first_renderer,
                &mut source,
                PrivateTarget::Projected(base),
                control,
                &monitor,
                "P_persistent_projected_same_ReadWork_memo",
            ))
        } else {
            None
        };
        let same = match (&first, &second) {
            (Ok(a), Some((Ok(b), _))) => Some(a.same(b)),
            _ => None,
        };
        let second_row = match &second {
            Some((_, row)) => json!({"attempted":true,"action":row}),
            None => json!({"attempted":false,"reason":"first actual preparation failed"}),
        };
        let final_work = work(&budget);
        drop(source);
        drop(budget);
        let after_work = first.as_ref().ok().map(Prepared::value);
        let controller_before_drop = controller(&first_renderer);
        drop(first_renderer);
        let after_controller = first.as_ref().ok().map(Prepared::value);
        drop(second);
        drop(first);
        let entries_after_first_renderer = entries(&cache);
        Ok((
            directory,
            cache,
            entries_before,
            cancelled,
            first_row,
            second_row,
            same,
            final_work,
            after_work,
            controller_before_drop,
            after_controller,
            entries_after_first_renderer,
            monitor,
        ))
    })?;
    let budget = RefCell::new(ReadWork::default());
    let (mut fresh, monitor) =
        n_fixture::renderer(plan, StageLimits::default(), Some(cache.clone()));
    let mut source = provider(reopen()?, Some(&budget), false);
    let control = WorkControl {
        work: &budget,
        cancelled: &cancelled,
        deadline: Instant::now() + TIMEOUT,
    };
    let (restored, restored_row) = private_once(
        &mut fresh,
        &mut source,
        PrivateTarget::Projected(base),
        control,
        &monitor,
        "P_persistent_projected_fresh_renderer_fresh_ReadWork",
    );
    let fresh_work = work(&budget);
    drop(source);
    drop(budget);
    let fresh_after_work = restored.as_ref().ok().map(Prepared::value);
    let fresh_controller_before_drop = controller(&fresh);
    drop(fresh);
    let fresh_after_controller = restored.as_ref().ok().map(Prepared::value);
    drop(restored);
    Ok(
        json!({"setup":"Ok","entries_before":entries_before,"first":first_row,
        "second":second_row,"same_actual_prepared_arc":same,"final_work":final_work,
        "prepared_after_ReadWork_drop":after_work,"controller_before_drop":controller_before_drop,
        "prepared_after_controller_drop":after_controller,
        "entries_after_first_renderer":entries_after_first_renderer,
        "fresh_renderer":restored_row,"fresh_work":fresh_work,
        "fresh_prepared_after_ReadWork_drop":fresh_after_work,
        "fresh_controller_before_drop":fresh_controller_before_drop,
        "fresh_prepared_after_controller_drop":fresh_after_controller,
        "entries_after_fresh":entries(&cache),
        "account_after_controller_drop":"unexposed; no account handle retained",
        "scope":"actual checked private Projected constructor, Some(real cache), unchanged default caps; no synthetic producer, source, fee or residency claim"}),
    )
}
pub(super) fn augment(
    current: Value,
    plan: &Arc<RenderPlan>,
    base: &Arc<AudioStageProjection<'_>>,
    asset: &deadpan_core::AssetRecord,
    wire: &Value,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
) -> Value {
    let public = match plain_nested_plan(asset, wire) {
        Ok((plain, document)) => match plain.audio_definition(AudioDefinitionSelector::Node {
            node: o_fixture::id("outer"),
        }) {
            Ok(definition) => json!({"checked_document":document,"actual":
                o_fixture::setup_result("P_real_public_persistent",
                    persistent_public(&plain,PublicTarget::Definition(&definition),reopen))}),
            Err(error) => key_group("P_public_persistent", || setup_plan_error(error)),
        },
        Err(error) => key_group("P_public_persistent", || {
            o_fixture::setup_error("P_checked_plain_nested_document", error.as_ref())
        }),
    };
    let projected = o_fixture::setup_result(
        "P_real_persistent_projected",
        persistent_projected(plan, base, reopen),
    );
    json!({"schema":"saved-key-output-policy-current-O-fixture-v2",
        "current_O_fixture":current,"additional_public_persistent":public,
        "additional_private_persistent_projected":projected,"key_groups":key_groups(),
        "scope":"current O/N/M/L/J; original nineteen actions plus at most six actual additional actions; setup failures and conditional memo omissions remain literal"})
}

fn exact(value: &Value, keys: &[&str]) {
    super::fields(value, keys);
}
fn uint(value: &Value) -> u64 {
    value.as_u64().unwrap()
}
fn optional(value: &Value) -> Option<u64> {
    if value.is_null() {
        None
    } else {
        let id = uint(value);
        assert!(id > 0);
        Some(id)
    }
}
fn values<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key].as_array().unwrap()
}
fn p_position(value: &Value) -> (u64, u64, u64, Vec<u64>, Option<u64>) {
    exact(
        value,
        &[
            "schema",
            "last_scan",
            "last_row",
            "last_window",
            "active_scans",
            "active_window",
        ],
    );
    assert_eq!(
        value["schema"],
        "saved-key-output-policy-before-position-v2"
    );
    let last = uint(&value["last_scan"]);
    let stack = values(value, "active_scans")
        .iter()
        .map(|v| {
            let id = uint(v);
            assert!(id > 0 && id <= last);
            id
        })
        .collect::<Vec<_>>();
    assert!(stack.len() <= 128);
    assert!(stack.windows(2).all(|p| p[0] < p[1]));
    let last_window = uint(&value["last_window"]);
    let window = optional(&value["active_window"]);
    if let Some(window) = window {
        assert!(window <= last_window);
    }
    (last, uint(&value["last_row"]), last_window, stack, window)
}
fn clock(value: &Value) -> u64 {
    assert_eq!(value["schema"], "stage-block-policy-before-position-v1");
    uint(&value["last_attempted_ordinal"])
}
fn payload(value: &Value) -> usize {
    values(value, "calls")
        .iter()
        .chain(values(value, "rows"))
        .chain(values(value, "windows"))
        .try_fold(0usize, |sum, row| {
            sum.checked_add(serde_json::to_vec(row).unwrap().len())
        })
        .unwrap()
}
fn call_shape(value: &Value, ordinal: u64) {
    exact(value, &["ordinal", "parent", "context"]);
    assert_eq!(uint(&value["ordinal"]), ordinal);
    if let Some(parent) = optional(&value["parent"]) {
        assert!(parent < ordinal);
    }
    let context = &value["context"];
    exact(context, &["depth", "recipe", "input", "output"]);
    assert!(uint(&context["depth"]) > 0);
    exact(
        &context["recipe"],
        &[
            "input_frames",
            "output_frames",
            "rate",
            "pitch_semitones",
            "engine",
        ],
    );
    uint(&context["recipe"]["input_frames"]);
    uint(&context["recipe"]["output_frames"]);
    exact(&context["recipe"]["rate"], &["numerator", "denominator"]);
    assert!(context["recipe"]["rate"]["numerator"].is_number());
    assert!(context["recipe"]["rate"]["denominator"].is_number());
    assert!(context["recipe"]["pitch_semitones"].is_number());
    assert!(context["recipe"]["engine"].is_string());
    for key in ["input", "output"] {
        assert!(context[key].is_object());
        assert!(matches!(
            context[key]["kind"].as_str(),
            Some("Signal" | "Tape" | "PreserveTape" | "Mix")
        ));
        assert_eq!(context[key]["belongs_to_plan"].as_bool(), Some(true));
    }
}
fn scan_result(value: &Value) {
    match value["status"].as_str().unwrap() {
        "Ok" => {
            let saved = value["saved"].as_bool().unwrap();
            if saved {
                exact(
                    value,
                    &[
                        "status",
                        "saved",
                        "key",
                        "dependency_count",
                        "relative_depth",
                    ],
                );
                assert_eq!(values(value, "key").len(), 32);
                for b in values(value, "key") {
                    assert!(uint(b) <= 255);
                }
                uint(&value["dependency_count"]);
                uint(&value["relative_depth"]);
            } else {
                exact(value, &["status", "saved"]);
            }
        }
        "Err" => {
            exact(value, &["status", "error"]);
            super::typed(&value["error"]);
        }
        _ => panic!("unknown actual scan status"),
    }
}
fn work_transition(before: &Value, after: &Value, accepted: u64) {
    let fields = [
        "prepared_stages",
        "prepared_frames",
        "source_checks",
        "observed",
        "preflight_assets",
        "plan_work",
        "cache_plan_work",
        "cache_key_scopes",
        "billing_lane",
        "projected_len",
        "projected_capacity",
        "projected_active_len",
        "projected_preflight_len",
        "intrinsic_preflight_len",
        "projected_resident_frames",
        "sound_context_frames",
        "projected_disk_bytes",
        "cancelled",
    ];
    let a = &before["work"];
    let b = &after["work"];
    exact(a, &fields);
    exact(b, &fields);
    assert!(uint(&a["cache_key_scopes"]) > 0);
    assert_eq!(a["billing_lane"], "Cache");
    assert_eq!(b["cache_key_scopes"], a["cache_key_scopes"]);
    assert_eq!(b["billing_lane"], "Cache");
    assert_eq!(
        uint(&b["cache_plan_work"]),
        uint(&a["cache_plan_work"]).checked_add(accepted).unwrap()
    );
    assert!(uint(&b["cache_plan_work"]) <= MAX_PLAN_WORK_PER_READ as u64);
    for field in fields {
        // Cancellation is an external AtomicBool; check() can observe it between these rows.
        if !matches!(field, "cache_plan_work" | "cancelled") {
            assert_eq!(
                b[field], a[field],
                "only the accepted Cache fee may change Work: {field}"
            );
        }
    }
    a["cancelled"].as_bool().unwrap();
    b["cancelled"].as_bool().unwrap();
    assert_eq!(after["controller"], before["controller"]);
    assert_eq!(after["progress"], before["progress"]);
}
fn control_error(error: &Value) {
    super::typed(error);
    if error["cancelled"] == true {
        super::assert_cancelled_error(error);
    } else {
        assert_eq!(error["kind"], "Timeout");
        assert_eq!(error["typed_payload"], json!({"shape":"unit"}));
    }
}
fn ordinary_work(ordinary: &Value) {
    let rows = values(ordinary, "rows");
    let one = |phase: &str| {
        let found = rows
            .iter()
            .filter(|r| r["phase"] == phase)
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 1, "one reached {phase}");
        found[0]
    };
    let count = |phase: &str| rows.iter().filter(|r| r["phase"] == phase).count();
    let helper = one("helper_enter");
    let range = one("range_result");
    let result = one("helper_result");
    assert_eq!(rows[0]["phase"], "caller_enter");
    assert_eq!(rows[1], *helper);
    assert_eq!(rows[2], *range);
    assert_eq!(values(ordinary, "calls").len(), 1);
    assert_eq!(ordinary["calls"][0]["caller"], "SavedKeyOutputBits");
    let no_debit = || {
        assert_eq!(count("manual_debit_before"), 0);
        assert_eq!(count("manual_debit_result"), 0);
    };
    let failed_tail = |failure: &Value, expected_error: &Value| {
        let index = rows.iter().position(|r| std::ptr::eq(r, failure)).unwrap();
        assert_eq!(rows.len(), index + 3);
        assert_eq!(rows[index + 1]["phase"], "helper_result");
        assert_eq!(rows[index + 2]["phase"], "helper_final_pcm");
        assert_eq!(result["value"]["status"], "Err");
        assert_eq!(result["value"]["error"], *expected_error);
        super::typed(&result["value"]["error"]);
        work_transition(&failure["snapshot"], &result["snapshot"], 0);
    };
    work_transition(&helper["snapshot"], &range["snapshot"], 0);
    if range["value"]["status"] == "Err" {
        assert_eq!(count("query_limits_result"), 0);
        assert_eq!(count("policy_query_result"), 0);
        no_debit();
        failed_tail(range, &range["value"]["error"]);
        return;
    }
    assert_eq!(range["value"]["status"], "Ok");
    let limits = one("query_limits_result");
    assert_eq!(rows[3], *limits);
    work_transition(&range["snapshot"], &limits["snapshot"], 0);
    let spent = uint(&limits["snapshot"]["work"]["cache_plan_work"]);
    let remaining = (MAX_PLAN_WORK_PER_READ as u64).checked_sub(spent);
    let default_limits = crate::stages::query_limits();
    if limits["value"]["status"] == "Err" {
        assert_eq!(count("policy_query_result"), 0);
        no_debit();
        let error = &limits["value"]["error"];
        if error["kind"] == "Limit" {
            super::typed(error);
            assert_eq!(
                error["typed_payload"],
                json!({"reason":CACHE_KEY_WORK_LIMIT})
            );
            assert!(remaining.is_none_or(|n| n < default_limits.maximum_work as u64));
        } else {
            control_error(error);
        }
        failed_tail(limits, error);
        return;
    }
    assert_eq!(limits["value"]["status"], "Ok");
    assert!(remaining.unwrap() >= default_limits.maximum_work as u64);
    assert_eq!(
        uint(&limits["value"]["maximum_work"]),
        default_limits.maximum_work as u64
    );
    assert_eq!(
        uint(&limits["value"]["maximum_spans"]),
        default_limits.maximum_spans as u64
    );
    let query = one("policy_query_result");
    assert_eq!(rows[4], *query);
    work_transition(&limits["snapshot"], &query["snapshot"], 0);
    if query["value"]["status"] == "Err" {
        no_debit();
        assert_eq!(
            query["value"]["local_work"],
            "not exposed on construction failure"
        );
        let error = &result["value"]["error"];
        super::typed(error);
        assert_eq!(error["kind"], "Plan");
        assert_eq!(error["typed_payload"], query["value"]["error"]);
        failed_tail(query, error);
        return;
    }
    assert_eq!(query["value"]["status"], "Ok");
    let before = one("manual_debit_before");
    let debit = one("manual_debit_result");
    assert_eq!(rows[5], *before);
    assert_eq!(rows[6], *debit);
    assert_eq!(before["value"]["policy"], query["value"]["policy"]);
    let amount = uint(&before["value"]["amount"]);
    assert_eq!(
        amount,
        uint(&query["value"]["policy"]["work"]),
        "manual Cache debit is this exact ordinary policy's work"
    );
    assert!(amount <= uint(&limits["value"]["maximum_work"]));
    work_transition(&query["snapshot"], &before["snapshot"], 0);
    let accepted = debit["value"]["status"] == "Ok";
    work_transition(
        &before["snapshot"],
        &debit["snapshot"],
        if accepted { amount } else { 0 },
    );
    if accepted {
        exact(&debit["value"], &["status"]);
    } else {
        assert_eq!(debit["value"]["status"], "Err");
        exact(&debit["value"], &["status", "error"]);
        let error = &debit["value"]["error"];
        if error["kind"] == "Limit" {
            super::typed(error);
            assert_eq!(
                error["typed_payload"],
                json!({"reason":CACHE_KEY_WORK_LIMIT})
            );
            assert!(
                uint(&before["snapshot"]["work"]["cache_plan_work"])
                    .checked_add(amount)
                    .is_none_or(|n| n > MAX_PLAN_WORK_PER_READ as u64)
            );
        } else {
            control_error(error);
        }
        failed_tail(debit, error);
    }
}

fn shape(value: &Value) {
    exact(
        value,
        &[
            "schema",
            "attempted",
            "dropped",
            "invalid",
            "overflow",
            "active_at_take",
            "calls",
            "rows",
            "windows",
            "active_window_at_take",
            "payload_bytes",
            "bounds",
            "scope",
            "observer_storage",
        ],
    );
    assert_eq!(value["schema"], "saved-key-output-policy-before-v2");
    assert_eq!(value["invalid"].as_bool(), Some(false));
    assert_eq!(value["overflow"].as_bool(), Some(false));
    assert_eq!(uint(&value["dropped"]), 0);
    assert!(values(value, "active_at_take").is_empty());
    assert!(value["active_window_at_take"].is_null());
    assert_eq!(value["scope"], P_SCOPE);
    assert_eq!(value["observer_storage"], STORAGE);
    exact(
        &value["bounds"],
        &[
            "rows",
            "scans",
            "windows",
            "active_scans",
            "serialized_payload_bytes",
        ],
    );
    for (key, expected) in [
        ("rows", 32768),
        ("scans", 4096),
        ("windows", 4096),
        ("active_scans", 128),
        ("serialized_payload_bytes", 67108864),
    ] {
        assert_eq!(uint(&value["bounds"][key]), expected);
    }
    assert!(
        values(value, "calls").len() <= 4096
            && values(value, "rows").len() <= 32768
            && values(value, "windows").len() <= 4096
    );
    assert_eq!(
        uint(&value["attempted"]),
        values(value, "rows").len() as u64
    );
    for (index, call) in values(value, "calls").iter().enumerate() {
        call_shape(call, (index + 1) as u64);
    }
    for (index, row) in values(value, "rows").iter().enumerate() {
        exact(
            row,
            &[
                "ordinal",
                "scan",
                "window",
                "phase",
                "position",
                "value",
                "snapshot",
                "o_position",
                "o_native_position",
                "n_before_position",
                "n_native_position",
            ],
        );
        assert_eq!(uint(&row["ordinal"]), (index + 1) as u64);
        let scan = optional(&row["scan"]);
        let window = optional(&row["window"]);
        if let Some(window) = window {
            assert!(window <= values(value, "windows").len() as u64);
        }
        let phase = row["phase"].as_str().unwrap();
        assert!(matches!(
            phase,
            "saved_key_disabled"
                | "cache_scope_before"
                | "scan_enter"
                | "output_loop_enter"
                | "output_check_before"
                | "output_check_after"
                | "ordinary_helper_before"
                | "ordinary_result_discarded_before_bits"
                | "bit_hash_before"
                | "bit_hash_after"
                | "mask_consumed_cursor_before"
                | "cursor_after"
                | "output_loop_exit"
                | "key_final_check_before"
                | "key_final_check_after"
                | "key_finish"
                | "scan_raw_result"
                | "cache_scope_after"
                | "scan_mapped_result"
        ));
        let thin = matches!(phase, "bit_hash_before" | "bit_hash_after");
        if thin {
            assert!(scan.is_some());
            for key in [
                "position",
                "snapshot",
                "o_position",
                "o_native_position",
                "n_before_position",
                "n_native_position",
            ] {
                assert!(row[key].is_null());
            }
            exact(&row["value"], &["cursor", "sample_bits", "byte"]);
            uint(&row["value"]["cursor"]);
            let bits = values(&row["value"], "sample_bits");
            assert_eq!(bits.len(), 2);
            for b in bits {
                assert!(uint(b) <= u64::from(u32::MAX));
            }
            assert_eq!(
                uint(&row["value"]["byte"]),
                u64::from(f32::from_bits(uint(&bits[0]) as u32) == 0.0)
            );
        } else {
            let (_, last_row, last_window, stack, active_window) = p_position(&row["position"]);
            assert_eq!(last_row, (index + 1) as u64);
            let opened = values(value, "windows")
                .iter()
                .filter(|w| uint(&w["row_before"]) < last_row)
                .count();
            assert_eq!(last_window, opened as u64);
            assert_eq!(stack.last().copied(), scan);
            assert_eq!(active_window, window);
            exact(&row["snapshot"], &["work", "controller", "progress"]);
            assert!(row["snapshot"]["work"].is_object());
            assert!(row["snapshot"]["controller"].is_object());
            assert!(
                row["snapshot"]["progress"].is_null() || row["snapshot"]["progress"].is_object()
            );
            for key in [
                "o_position",
                "o_native_position",
                "n_before_position",
                "n_native_position",
            ] {
                assert!(row[key].is_null() || row[key].is_object());
            }
            assert_eq!(row["o_position"].is_null(), window.is_none());
            if window.is_some() {
                clock(&row["o_position"]);
            }
            match phase {
                "saved_key_disabled"
                | "cache_scope_before"
                | "scan_enter"
                | "key_final_check_before"
                | "cache_scope_after" => exact(&row["value"], &[]),
                "output_loop_enter" => {
                    exact(
                        &row["value"],
                        &["input_cursor", "output_frames", "zero_output"],
                    );
                    uint(&row["value"]["input_cursor"]);
                    let frames = uint(&row["value"]["output_frames"]);
                    assert_eq!(row["value"]["zero_output"].as_bool(), Some(frames == 0));
                }
                "output_check_before" => {
                    exact(&row["value"], &["cursor"]);
                    uint(&row["value"]["cursor"]);
                }
                "output_check_after" => {
                    exact(&row["value"], &["cursor", "status"]);
                    uint(&row["value"]["cursor"]);
                    assert_eq!(row["value"]["status"], "Ok");
                }
                "ordinary_helper_before" | "mask_consumed_cursor_before" | "cursor_after" => {
                    exact(&row["value"], &["cursor", "count"]);
                    uint(&row["value"]["cursor"]);
                    assert!(uint(&row["value"]["count"]) > 0);
                }
                "ordinary_result_discarded_before_bits" => {
                    exact(&row["value"], &["cursor", "count", "ownership"]);
                    uint(&row["value"]["cursor"]);
                    assert!(uint(&row["value"]["count"]) > 0);
                    assert_eq!(row["value"]["ownership"], DISCARDED);
                }
                "output_loop_exit" => {
                    exact(&row["value"], &["cursor", "output_frames"]);
                    assert_eq!(row["value"]["cursor"], row["value"]["output_frames"]);
                    uint(&row["value"]["cursor"]);
                }
                "key_final_check_after" => {
                    exact(&row["value"], &["status"]);
                    assert_eq!(row["value"]["status"], "Ok");
                }
                "key_finish" => {
                    exact(&row["value"], &["key"]);
                    assert_eq!(values(&row["value"], "key").len(), 32);
                    for byte in values(&row["value"], "key") {
                        assert!(uint(byte) <= 255);
                    }
                }
                "scan_raw_result" | "scan_mapped_result" => scan_result(&row["value"]),
                _ => unreachable!(),
            }
        }
    }
    let mut previous_end = 0u64;
    for (index, window) in values(value, "windows").iter().enumerate() {
        exact(
            window,
            &["ordinal", "scan", "row_before", "row_after", "observer"],
        );
        assert_eq!(uint(&window["ordinal"]), (index + 1) as u64);
        let scan = optional(&window["scan"]).unwrap();
        assert!(scan <= values(value, "calls").len() as u64);
        let begin = uint(&window["row_before"]);
        let end = uint(&window["row_after"]);
        assert!(begin >= previous_end && end > begin && end <= values(value, "rows").len() as u64);
        previous_end = end;
        assert_eq!(o_fixture::validate_observer(&window["observer"]), 0);
        ordinary_work(&window["observer"]);
        let calls = values(&window["observer"], "calls");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["caller"], "SavedKeyOutputBits");
        assert!(calls[0]["parent"].is_null());
        for row in &values(value, "rows")[begin as usize..end as usize] {
            assert_eq!(row["window"], window["ordinal"]);
            assert_eq!(row["scan"], window["scan"]);
        }
    }
    for row in values(value, "rows") {
        if let Some(window) = optional(&row["window"]) {
            let window = &values(value, "windows")[window as usize - 1];
            let ordinal = uint(&row["ordinal"]);
            assert!(ordinal > uint(&window["row_before"]) && ordinal <= uint(&window["row_after"]));
            assert_eq!(row["scan"], window["scan"]);
        }
    }
    assert_eq!(uint(&value["payload_bytes"]), payload(value) as u64);
    assert!(payload(value) <= 67108864);
    assert!(serde_json::to_vec(value).unwrap().len() <= 67108864);
}

struct ScanState<'a> {
    rows: Vec<&'a Value>,
    entry: &'a Value,
    action: usize,
    stream: usize,
}
fn ordinary_row<'a>(p: &'a Value, row: &Value, phase: &str) -> &'a Value {
    let window = optional(&row["window"]).unwrap();
    let ordinary = &values(p, "windows")[window as usize - 1]["observer"];
    let ordinal = clock(&row["o_position"]);
    assert!(ordinal > 0);
    let observed = &values(ordinary, "rows")[ordinal as usize - 1];
    assert_eq!(observed["position"], row["o_position"]);
    assert_eq!(observed["phase"], phase);
    observed
}
fn stack_references(position: &Value, rows: &[Value], frame_key: &str) {
    let stack = values(position, "active_stack");
    assert!(stack.len() <= 128);
    let mut prior = 0;
    for frame in stack {
        let id = uint(&frame[frame_key]);
        assert!(id > prior);
        prior = id;
        assert!(
            rows.iter()
                .flat_map(|row| values(&row["position"], "active_stack"))
                .any(|known| known[frame_key] == id && known == frame),
            "borrowed position names an actual same-action recorder frame"
        );
    }
}
fn links(row: &Value, receipt: &Value, actual_keys: &Value) {
    let o = &row["o_native_position"];
    exact(
        o,
        &["schema", "last_ordinal", "active_stack", "selected_factory"],
    );
    assert_eq!(o["schema"], "stage-block-policy-native-position-v1");
    let ordinal = uint(&o["last_ordinal"]);
    let native = &receipt["native"];
    assert!(ordinal <= values(native, "rows").len() as u64);
    stack_references(o, values(native, "rows"), "id");
    if let Some(ticket) = optional(&o["selected_factory"]) {
        let top = values(o, "active_stack").last().unwrap();
        assert_eq!(top["kind"], "method");
        assert_eq!(uint(&top["ticket"]), ticket);
        let begin = values(native, "rows")
            .iter()
            .find(|r| r["ticket"] == ticket && r["phase"] == "factory_begin")
            .unwrap();
        let end = values(native, "rows")
            .iter()
            .find(|r| r["ticket"] == ticket && r["phase"] == "factory_result")
            .unwrap();
        assert!(uint(&begin["ordinal"]) <= ordinal && ordinal < uint(&end["ordinal"]));
    }
    let n = &row["n_native_position"];
    exact(
        n,
        &[
            "event",
            "selected_ticket",
            "factory_ticket",
            "handoff",
            "active_stack",
        ],
    );
    let ambient = &receipt["ambient_N"];
    let event = uint(&n["event"]);
    let maximum = ["use_rows", "native_rows", "account_rows", "resident_rows"]
        .into_iter()
        .flat_map(|key| values(ambient, key))
        .map(|r| uint(&r["ordinal"]))
        .max()
        .unwrap_or(0);
    assert!(event <= maximum);
    stack_references(n, values(ambient, "use_rows"), "frame");
    let expected = values(n, "active_stack")
        .iter()
        .rev()
        .find(|f| matches!(f["kind"].as_str(), Some("consumer" | "invocation" | "mix")))
        .and_then(|f| {
            if f["kind"] == "mix" {
                None
            } else {
                optional(&f["ticket"])
            }
        });
    assert_eq!(optional(&n["selected_ticket"]), expected);
    if let Some(ticket) = optional(&n["factory_ticket"]) {
        assert!(
            values(ambient, "factory_receipts")
                .iter()
                .any(|v| v["ticket"] == ticket)
        );
        assert!(
            values(n, "active_stack")
                .iter()
                .any(|v| v["ticket"] == ticket && !v["scope"].is_null())
        );
    }
    if let Some(handoff) = optional(&n["handoff"]) {
        assert!(handoff <= values(ambient, "handoff_receipts").len() as u64);
    }
    let keys = &row["n_before_position"];
    let (key_begin, key_end) =
        if receipt["keys"]["schema"] == "P-before-KeysOnly-group-reference-v1" {
            (
                uint(&receipt["keys"]["begin"]["last_attempted_ordinal"]),
                uint(&receipt["keys"]["end"]["last_attempted_ordinal"]),
            )
        } else {
            (0, values(actual_keys, "rows").len() as u64)
        };
    let at = uint(&keys["last_attempted_ordinal"]);
    assert!(at >= key_begin && at <= key_end);
    exact(
        keys,
        &[
            "schema",
            "last_attempted_ordinal",
            "selected_ticket",
            "active_stack",
            "active_key_streams",
        ],
    );
    assert_eq!(keys["schema"], "read-signal-policy-keys-position-v1");
    assert!(uint(&keys["last_attempted_ordinal"]) <= values(actual_keys, "rows").len() as u64);
    stack_references(
        keys,
        &values(actual_keys, "rows")[key_begin as usize..key_end as usize],
        "frame",
    );
    let mut selected = None;
    for frame in values(keys, "active_stack").iter().rev() {
        if frame["kind"] == "invocation_mix" {
            break;
        }
        if matches!(frame["kind"].as_str(), Some("consumer" | "invocation")) {
            selected = optional(&frame["ticket"]);
            break;
        }
    }
    assert_eq!(optional(&keys["selected_ticket"]), selected);
    let mut prior = 0;
    for stream in values(keys, "active_key_streams") {
        let stream = uint(stream);
        assert!(stream > prior && stream <= values(actual_keys, "key_transcripts").len() as u64);
        let actual = &values(actual_keys, "key_transcripts")[stream as usize - 1];
        assert_eq!(
            optional(&actual["parent"]),
            if prior == 0 { None } else { Some(prior) }
        );
        prior = stream;
    }
}

fn prove_scans(p: &Value, current: &Value, ranges: &[Value]) -> Value {
    shape(p);
    let receipts = values(current, "receipts");
    group_references(current, ranges);
    assert_eq!(ranges.len(), receipts.len());
    assert!(ranges.len() <= 25);
    let mut last_scan = 0;
    let mut last_row = 0;
    let mut last_window = 0;
    let mut scan_action = Vec::new();
    let mut stream_index = Vec::new();
    for (index, (range, receipt)) in ranges.iter().zip(receipts).enumerate() {
        exact(range, &["action", "entry", "begin", "end", "ordinal"]);
        assert_eq!(uint(&range["ordinal"]), (index + 1) as u64);
        assert_eq!(range["ordinal"], receipt["ordinal"]);
        assert_eq!(range["action"], receipt["action"]);
        assert_eq!(range["entry"], receipt["entry"]);
        super::valid(&receipt["native"], &receipt["ambient_N"]);
        let keys = request_keys(current, receipt);
        if receipt["keys"]["schema"] == "read-signal-policy-keys-v1" {
            o_fixture::key_replay::valid_keys(keys);
        }
        let (begin, begin_row, begin_window, begin_stack, begin_active) =
            p_position(&range["begin"]);
        let (end, end_row, end_window, end_stack, end_active) = p_position(&range["end"]);
        assert!(begin_stack.is_empty() && end_stack.is_empty());
        assert!(begin_active.is_none() && end_active.is_none());
        assert_eq!(begin, last_scan);
        assert_eq!(begin_row, last_row);
        assert_eq!(begin_window, last_window);
        assert!(end >= begin && end_row >= begin_row && end_window >= begin_window);
        let offset = if receipt["keys"]["schema"] == "P-before-KeysOnly-group-reference-v1" {
            let group = &values(&current["fixture"], "key_groups")
                [uint(&receipt["keys"]["group"]) as usize - 1];
            begin.checked_sub(p_position(&group["entry"]).0).unwrap() as usize
        } else {
            0
        };
        let streams = values(keys, "key_transcripts");
        let count = (end - begin) as usize;
        assert!(offset + count <= streams.len());
        if receipt["keys"]["schema"] == "read-signal-policy-keys-v1" {
            assert_eq!(count, streams.len());
        }
        for stream in offset..offset + count {
            scan_action.push(index);
            stream_index.push(stream);
        }
        last_scan = end;
        last_row = end_row;
        last_window = end_window;
    }
    assert_eq!(last_scan, values(p, "calls").len() as u64);
    assert_eq!(last_row, values(p, "rows").len() as u64);
    assert_eq!(last_window, values(p, "windows").len() as u64);
    for (action, range) in ranges.iter().enumerate() {
        let (_, begin_row, begin_window, _, _) = p_position(&range["begin"]);
        let (_, end_row, end_window, _, _) = p_position(&range["end"]);
        let mut key_clock =
            if receipts[action]["keys"]["schema"] == "P-before-KeysOnly-group-reference-v1" {
                uint(&receipts[action]["keys"]["begin"]["last_attempted_ordinal"])
            } else {
                0
            };
        for row in &values(p, "rows")[begin_row as usize..end_row as usize] {
            if !row["position"].is_null() {
                let at = uint(&row["n_before_position"]["last_attempted_ordinal"]);
                assert!(at >= key_clock);
                key_clock = at;
                links(
                    row,
                    &receipts[action],
                    request_keys(current, &receipts[action]),
                );
            }
            if let Some(scan) = optional(&row["scan"]) {
                assert_eq!(scan_action[scan as usize - 1], action);
            }
            if let Some(window) = optional(&row["window"]) {
                assert!(window > begin_window && window <= end_window);
            }
        }
        for window in &values(p, "windows")[begin_window as usize..end_window as usize] {
            assert!(
                uint(&window["row_before"]) >= begin_row && uint(&window["row_after"]) <= end_row
            );
            assert_eq!(scan_action[uint(&window["scan"]) as usize - 1], action);
        }
    }
    let mut states = BTreeMap::<u64, ScanState<'_>>::new();
    let mut stack = Vec::<u64>::new();
    let mut completed = BTreeSet::new();
    let mut entered = 0u64;
    let mut pending_entry: Option<&Value> = None;
    let mut disabled = 0usize;
    for row in values(p, "rows") {
        let phase = row["phase"].as_str().unwrap();
        let scan = optional(&row["scan"]);
        let thin = matches!(phase, "bit_hash_before" | "bit_hash_after");
        if !thin {
            let (last, _, _, now, _) = p_position(&row["position"]);
            if phase == "scan_enter" {
                entered += 1;
                assert_eq!(last, entered);
                let call = &values(p, "calls")[entered as usize - 1];
                assert_eq!(optional(&call["parent"]), stack.last().copied());
                assert_eq!(now.last(), Some(&entered));
                assert_eq!(&now[..now.len() - 1], stack.as_slice());
                let entry = pending_entry
                    .take()
                    .expect("real cache scope precedes scan");
                assert_eq!(entry["scan"], call["parent"]);
                states.insert(
                    entered,
                    ScanState {
                        rows: Vec::new(),
                        entry,
                        action: scan_action[entered as usize - 1],
                        stream: stream_index[entered as usize - 1],
                    },
                );
                stack = now;
            } else {
                let common = stack.iter().zip(&now).take_while(|(a, b)| a == b).count();
                assert_eq!(common, now.len(), "only scan_enter pushes a scan");
                for closed in &stack[common..] {
                    assert!(completed.contains(closed));
                }
                stack = now;
                assert_eq!(last, entered);
            }
            if phase == "cache_scope_before" {
                assert!(pending_entry.replace(row).is_none());
            }
            if phase == "saved_key_disabled" {
                disabled += 1;
                assert_eq!(row["snapshot"]["controller"]["has_persistent_cache"], false);
            }
        }
        assert_eq!(scan, stack.last().copied());
        if let Some(scan) = scan {
            assert!(scan <= entered);
            if phase != "cache_scope_before" && phase != "saved_key_disabled" {
                states.get_mut(&scan).unwrap().rows.push(row);
            }
            if phase == "scan_mapped_result" {
                assert!(completed.insert(scan));
            }
        } else {
            assert!(matches!(phase, "cache_scope_before" | "saved_key_disabled"));
        }
    }
    assert!(pending_entry.is_none());
    for scan in stack {
        assert!(completed.contains(&scan));
    }
    assert_eq!(entered, values(p, "calls").len() as u64);
    assert_eq!(completed.len(), states.len());
    let mut helpers = BTreeSet::new();
    let mut bit_count = 0;
    let mut finishes = 0;
    let mut prefixes = 0;
    for (scan, state) in states {
        let call = &values(p, "calls")[scan as usize - 1];
        let receipt = &receipts[state.action];
        let actual_keys = request_keys(current, receipt);
        let stream = &values(actual_keys, "key_transcripts")[state.stream];
        assert_eq!(uint(&stream["ordinal"]), (state.stream + 1) as u64);
        let expected_parent = optional(&call["parent"]).map(|id| {
            assert_eq!(scan_action[id as usize - 1], state.action);
            (stream_index[id as usize - 1] + 1) as u64
        });
        assert_eq!(optional(&stream["parent"]), expected_parent);
        let rows = state.rows;
        assert_eq!(rows.first().unwrap()["phase"], "scan_enter");
        let enter = &rows[0]["n_before_position"];
        let progress_ordinal = uint(&enter["last_attempted_ordinal"])
            .checked_add(1)
            .unwrap();
        let progress = &values(actual_keys, "rows")[progress_ordinal as usize - 1];
        assert_eq!(progress["ordinal"], progress_ordinal);
        assert_eq!(progress["phase"], "progress_enter");
        let position = &progress["position"];
        for field in ["selected_ticket", "active_stack", "active_key_streams"] {
            assert_eq!(position[field], enter[field]);
        }
        assert_eq!(stream["entry_position"], *position);

        assert_eq!(rows.last().unwrap()["phase"], "scan_mapped_result");
        let exactly_one = |phase: &str| {
            let hits = rows
                .iter()
                .copied()
                .filter(|r| r["phase"] == phase)
                .collect::<Vec<_>>();
            assert_eq!(hits.len(), 1, "scan {scan} phase {phase}");
            hits[0]
        };
        let raw = exactly_one("scan_raw_result");
        let scope_after = exactly_one("cache_scope_after");
        let mapped = exactly_one("scan_mapped_result");
        let entry = &state.entry["snapshot"]["work"];
        let before = &rows[0]["snapshot"]["work"];
        let after = &scope_after["snapshot"]["work"];
        assert_eq!(
            uint(&before["cache_key_scopes"]),
            uint(&entry["cache_key_scopes"]) + 1
        );
        assert_eq!(before["billing_lane"], "Cache");
        assert_eq!(after["cache_key_scopes"], entry["cache_key_scopes"]);
        assert_eq!(after["billing_lane"], entry["billing_lane"]);
        let actual = &stream["actual_scan_result"];
        assert_eq!(raw["value"]["status"], actual["status"]);
        if actual["status"] == "Ok" {
            let saved = &actual["saved"];
            assert_eq!(raw["value"]["saved"].as_bool(), Some(!saved.is_null()));
            if !saved.is_null() {
                assert_eq!(raw["value"]["key"], saved["key"]);
                assert_eq!(
                    uint(&raw["value"]["dependency_count"]),
                    saved["dependencies"].as_object().unwrap().len() as u64
                );
                assert_eq!(raw["value"]["relative_depth"], saved["relative_depth"]);
            }
        } else {
            for field in ["kind", "message", "detail", "cancelled"] {
                assert_eq!(raw["value"]["error"][field], actual["error"][field]);
            }
        }
        if raw["value"]["status"] == "Err"
            && raw["value"]["error"]["kind"] == "Limit"
            && raw["value"]["error"]["typed_payload"]["reason"] == CACHE_KEY_WORK_LIMIT
        {
            assert_eq!(mapped["value"], json!({"status":"Ok","saved":false}));
        } else {
            assert_eq!(mapped["value"], raw["value"]);
        }
        let raw_index = rows
            .iter()
            .position(|r| r["phase"] == "scan_raw_result")
            .unwrap();
        assert_eq!(rows[raw_index + 1]["phase"], "cache_scope_after");
        assert_eq!(rows[raw_index + 2]["phase"], "scan_mapped_result");
        assert_eq!(raw_index + 3, rows.len());
        let production = &rows[1..raw_index];
        let mut i = 0;
        let mut observed_bits = Vec::new();
        let mut cursor = 0;
        if production
            .first()
            .is_some_and(|r| r["phase"] == "output_loop_enter")
        {
            let begin = production[0];
            assert_eq!(
                begin["value"]["input_cursor"],
                call["context"]["recipe"]["input_frames"]
            );
            let frames = uint(&begin["value"]["output_frames"]);
            assert_eq!(frames, uint(&call["context"]["recipe"]["output_frames"]));
            i = 1;
            while cursor < frames && i < production.len() {
                let check = production[i];
                assert_eq!(check["phase"], "output_check_before");
                assert_eq!(uint(&check["value"]["cursor"]), cursor);
                i += 1;
                if i == production.len() {
                    break;
                }
                assert_eq!(production[i]["phase"], "output_check_after");
                assert_eq!(uint(&production[i]["value"]["cursor"]), cursor);
                i += 1;
                assert!(i < production.len());
                let helper = production[i];
                assert_eq!(helper["phase"], "ordinary_helper_before");
                i += 1;
                let count = (frames - cursor).min(256);
                assert_eq!(uint(&helper["value"]["cursor"]), cursor);
                assert_eq!(uint(&helper["value"]["count"]), count);
                let window = optional(&helper["window"]).unwrap();
                assert!(helpers.insert(window));
                let ordinary = &values(p, "windows")[window as usize - 1]["observer"];
                let o_enter = ordinary_row(p, helper, "caller_enter");
                let ticket = uint(&o_enter["ticket"]);
                let o_call = &values(ordinary, "calls")[ticket as usize - 1];
                assert_eq!(o_call["caller"], "SavedKeyOutputBits");
                assert_eq!(o_call["context"]["depth"], call["context"]["depth"]);
                assert_eq!(o_call["context"]["recipe"], call["context"]["recipe"]);
                assert_eq!(uint(&o_call["context"]["block_start"]), cursor);
                assert_eq!(uint(&o_call["context"]["block_len"]), count);
                let owned = &helper["snapshot"]["controller"]["owned_suppression_live_frames"];
                if i == production.len() {
                    assert_eq!(o_call["helper_result"]["status"], "Err");
                    break;
                }
                let discarded = production[i];
                assert_eq!(discarded["phase"], "ordinary_result_discarded_before_bits");
                i += 1;
                assert_eq!(o_call["helper_result"]["status"], "Ok");
                assert_eq!(discarded["value"]["cursor"], helper["value"]["cursor"]);
                assert_eq!(discarded["value"]["count"], helper["value"]["count"]);
                assert_eq!(
                    &discarded["snapshot"]["controller"]["owned_suppression_live_frames"],
                    owned
                );
                let o_discard =
                    ordinary_row(p, discarded, "returned_vec_discarded_before_output_bits");
                assert_eq!(o_discard["ticket"], o_enter["ticket"]);
                let pcm = values(&o_discard["value"], "pcm_bits");
                assert_eq!(pcm.len(), count as usize);
                let final_pcm = values(ordinary, "rows")
                    .iter()
                    .find(|r| r["ticket"] == ticket && r["phase"] == "helper_final_pcm")
                    .unwrap();
                assert_eq!(final_pcm["value"], o_discard["value"]);
                for sample in pcm {
                    assert!(i + 1 < production.len());
                    let before = production[i];
                    let after = production[i + 1];
                    i += 2;
                    assert_eq!(before["phase"], "bit_hash_before");
                    assert_eq!(after["phase"], "bit_hash_after");
                    assert_eq!(before["value"], after["value"]);
                    assert_eq!(before["value"]["sample_bits"], *sample);
                    assert_eq!(uint(&before["value"]["cursor"]), cursor);
                    observed_bits.push(uint(&before["value"]["byte"]));
                }
                assert!(i < production.len());
                let consumed = production[i];
                i += 1;
                assert_eq!(consumed["phase"], "mask_consumed_cursor_before");
                assert_eq!(consumed["value"], helper["value"]);
                assert_eq!(
                    &consumed["snapshot"]["controller"]["owned_suppression_live_frames"],
                    owned
                );
                assert_eq!(consumed["o_position"], discarded["o_position"]);
                cursor += count;
                assert!(i < production.len());
                let advanced = production[i];
                i += 1;
                assert_eq!(advanced["phase"], "cursor_after");
                assert_eq!(uint(&advanced["value"]["cursor"]), cursor);
                assert_eq!(uint(&advanced["value"]["count"]), count);
                assert_eq!(advanced["o_position"], discarded["o_position"]);
            }
            if i < production.len() {
                assert_eq!(cursor, frames);
                assert_eq!(production[i]["phase"], "output_loop_exit");
                i += 1;
                assert!(i < production.len());
                assert_eq!(production[i]["phase"], "key_final_check_before");
                i += 1;
                if i < production.len() {
                    assert_eq!(production[i]["phase"], "key_final_check_after");
                    i += 1;
                    assert!(i < production.len());
                    assert_eq!(production[i]["phase"], "key_finish");
                    assert_eq!(production[i]["value"]["key"], stream["actual_finish"]);
                    i += 1;
                    finishes += 1;
                }
            }
        }
        assert_eq!(
            i,
            production.len(),
            "complete exact reached output phase sequence"
        );
        let finalized = production
            .iter()
            .filter(|r| r["phase"] == "key_finish")
            .count();
        assert_eq!(finalized, usize::from(!stream["actual_finish"].is_null()));
        if raw["value"]["status"] == "Ok" {
            assert_eq!(raw["value"]["saved"], true);
            assert_eq!(finalized, 1);
            assert_eq!(raw["value"]["key"], stream["actual_finish"]);
        }

        for row in production
            .iter()
            .filter(|r| !r["n_before_position"].is_null())
        {
            let ids = values(&row["n_before_position"], "active_key_streams");
            assert_eq!(ids.last().map(uint), Some((state.stream + 1) as u64));
        }
        let expected = values(stream, "updates")
            .iter()
            .filter(|r| r["site"] == "output_suppression_bit")
            .map(|r| {
                assert_eq!(values(r, "bytes").len(), 1);
                uint(&r["bytes"][0])
            })
            .collect::<Vec<_>>();
        assert_eq!(observed_bits, expected);
        bit_count += observed_bits.len();
        if stream["actual_finish"].is_null() {
            prefixes += 1;
        }
    }
    assert_eq!(helpers.len(), values(p, "windows").len());
    let action_inventory = ranges
        .iter()
        .map(|range| {
            let (begin, _, begin_window, _, _) = p_position(&range["begin"]);
            let (end, _, end_window, _, _) = p_position(&range["end"]);
            json!({"ordinal":range["ordinal"],"action":range["action"],"entry":range["entry"],
            "actual_scans":end-begin,"actual_output_windows":end_window-begin_window,
            "zero_scans":end==begin,"zero_output_windows":end_window==begin_window})
        })
        .collect::<Vec<_>>();
    json!({"action_inventory":action_inventory,"actual_actions":ranges.len(),"actual_scans":entered,"actual_ordinary_output_calls":helpers.len(),
        "actual_bit_pairs":bit_count,"actual_finalized_scans":finishes,"actual_prefix_scans":prefixes,
        "actual_disabled_boundaries":disabled,"zero_ordinary_output_calls":helpers.is_empty(),
        "scope":"complete actual scan/window/PCM/one-byte ordering, current N key replay, O ordinary helper and cache-scope joins; no P admission or global account retirement"})
}

fn validate_body(body: &Value) {
    exact(
        body,
        &[
            "schema",
            "current_O",
            "observer",
            "action_ranges",
            "observed_counts",
            "qualification_status",
            "scope",
            "current_key_proof",
        ],
    );
    assert_eq!(body["schema"], "saved-key-output-policy-ordinary-before-v2");
    assert_eq!(
        body["qualification_status"],
        "actual current O/N/M/L/J ordinary P observations; no P production selector or admission"
    );
    assert_eq!(
        body["scope"],
        "SavedKeyOutputBits only; complete raw current fixture and ordinary helper values; no Tape/Source construction, whole-preparation/dependency/graph admission or product-gate credit"
    );
    let current = &body["current_O"];
    exact(
        current,
        &["schema", "fixture", "receipts", "current_key_proof"],
    );
    assert_eq!(current["schema"], "stage-block-policy-after-fixture-v1");
    assert_eq!(
        current["current_key_proof"],
        o_fixture::key_replay::validate_body(current)
    );
    exact(
        &current["fixture"],
        &[
            "schema",
            "current_O_fixture",
            "additional_public_persistent",
            "additional_private_persistent_projected",
            "key_groups",
            "scope",
        ],
    );
    assert_eq!(
        current["fixture"]["schema"],
        "saved-key-output-policy-current-O-fixture-v2"
    );
    assert_eq!(
        body["observed_counts"],
        prove_scans(&body["observer"], current, values(body, "action_ranges"))
    );
    assert_eq!(
        body["current_key_proof"],
        o_fixture::key_replay::validate_body(body)
    );
}
fn capture_body() -> Value {
    let guard = collect();
    let p_guard = p_trace::begin();
    let current = super::capture_all();
    let p = p_trace::take();
    drop(p_guard);
    let actions = action_ranges();
    drop(guard);
    let proof = prove_scans(&p, &current, &actions);
    let mut body = json!({"schema":"saved-key-output-policy-ordinary-before-v2",
        "current_O":current,"observer":p,"action_ranges":actions,
        "observed_counts":proof,
        "qualification_status":"actual current O/N/M/L/J ordinary P observations; no P production selector or admission",
        "scope":"SavedKeyOutputBits only; complete raw current fixture and ordinary helper values; no Tape/Source construction, whole-preparation/dependency/graph admission or product-gate credit"});
    body["current_key_proof"] = o_fixture::key_replay::validate_body(&body);
    body
}
fn group_reference_mutations(body: &Value) {
    let current = &body["current_O"];
    let ranges = values(body, "action_ranges");
    group_references(current, ranges);
    let request = values(current, "receipts")
        .iter()
        .position(|r| r["keys"]["schema"] == "P-before-KeysOnly-group-reference-v1")
        .expect("actual persistent KeysOnly group request");
    let mut changed = current.clone();
    let end = &mut changed["receipts"][request]["keys"]["end"]["last_attempted_ordinal"];
    *end = json!(uint(end).checked_add(1).unwrap());
    for group in values(&changed["fixture"], "key_groups") {
        o_fixture::key_replay::valid_keys(&group["keys"]);
    }
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| group_references(
            &changed, ranges
        )))
        .is_err()
    );
    let zero = values(current, "receipts")
        .iter()
        .zip(ranges)
        .position(|(receipt, range)| {
            receipt["keys"]["schema"] == "P-before-KeysOnly-group-reference-v1"
                && range["begin"]["last_scan"] == range["end"]["last_scan"]
        })
        .expect("actual zero-scan warm or memo request in persistent group");
    let mut changed_ranges = ranges.to_vec();
    let frontier = &mut changed_ranges[zero]["end"]["last_scan"];
    *frontier = json!(uint(frontier).checked_add(1).unwrap());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| group_references(
            current,
            &changed_ranges
        )))
        .is_err()
    );
}
fn ordinary_work_mutations(original: &Value) {
    let window = values(original, "windows")
        .iter()
        .position(|window| {
            values(&window["observer"], "rows")
                .iter()
                .any(|row| row["phase"] == "manual_debit_result" && row["value"]["status"] == "Ok")
        })
        .expect("actual accepted ordinary SavedKey window for PBD1 mutations");
    ordinary_work(&original["windows"][window]["observer"]);
    for mutation in ["amount", "cache_counter", "plan_counter"] {
        let mut changed = original.clone();
        let ordinary = &mut changed["windows"][window]["observer"];
        let phase = if mutation == "amount" {
            "manual_debit_before"
        } else {
            "manual_debit_result"
        };
        let row = ordinary["rows"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row["phase"] == phase)
            .unwrap();
        let scalar = match mutation {
            "amount" => &mut row["value"]["amount"],
            "cache_counter" => &mut row["snapshot"]["work"]["cache_plan_work"],
            "plan_counter" => &mut row["snapshot"]["work"]["plan_work"],
            _ => unreachable!(),
        };
        *scalar = json!(uint(scalar).checked_add(1).unwrap());
        ordinary["payload_bytes"] = json!(o_fixture::observer_payload_bytes(ordinary));
        changed["payload_bytes"] = json!(payload(&changed));
        let ordinary = &changed["windows"][window]["observer"];
        assert_eq!(
            uint(&ordinary["payload_bytes"]),
            o_fixture::observer_payload_bytes(ordinary) as u64
        );
        assert_eq!(uint(&changed["payload_bytes"]), payload(&changed) as u64);
        assert_eq!(
            o_fixture::validate_observer(ordinary),
            0,
            "the unchanged structural validator must accept isolated fee mutation {mutation}"
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ordinary_work(ordinary)))
                .is_err(),
            "the P-only work equation must reject isolated fee mutation {mutation}"
        );
    }
}

#[test]
fn saved_key_output_policy_current_ordinary_values_keys_and_contexts() {
    let body = capture_body();
    validate_body(&body);
    group_reference_mutations(&body);
    let original = &body["observer"];
    ordinary_work_mutations(original);
    let ranges = body["action_ranges"].as_array().unwrap();
    if let Some(index) = values(original, "rows")
        .iter()
        .position(|r| r["phase"] == "bit_hash_after")
    {
        let mut changed = original.clone();
        let byte = uint(&changed["rows"][index]["value"]["byte"]);
        changed["rows"][index]["value"]["byte"] = json!(1 - byte);
        changed["payload_bytes"] = json!(payload(&changed));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| prove_scans(
                &changed,
                &body["current_O"],
                ranges
            )))
            .is_err()
        );
    }
    if let Some(index) = values(original, "rows")
        .iter()
        .position(|r| r["phase"] == "ordinary_result_discarded_before_bits")
    {
        let mut changed = original.clone();
        changed["rows"][index]["value"]["ownership"] = json!("schema-only malformed discard");
        changed["payload_bytes"] = json!(payload(&changed));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| prove_scans(
                &changed,
                &body["current_O"],
                ranges
            )))
            .is_err()
        );
    }
    if let Some(window) = values(original, "windows").first() {
        let mut changed = original.clone();
        let index = uint(&window["row_before"]) as usize;
        changed["rows"][index]["window"] = Value::Null;
        changed["payload_bytes"] = json!(payload(&changed));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| prove_scans(
                &changed,
                &body["current_O"],
                ranges
            )))
            .is_err()
        );
    }
}
#[test]
fn saved_key_output_policy_closed_empty_schema_and_payload_mutations() {
    let guard = p_trace::begin();
    let empty = p_trace::take();
    drop(guard);
    shape(&empty);
    for key in [
        "attempted",
        "payload_bytes",
        "bounds",
        "calls",
        "rows",
        "windows",
        "scope",
    ] {
        let mut changed = empty.clone();
        changed.as_object_mut().unwrap().remove(key);
        assert!(
            std::panic::catch_unwind(|| shape(&changed)).is_err(),
            "{key}"
        );
    }
    for (key, value) in [
        ("attempted", json!(true)),
        ("payload_bytes", json!(1)),
        ("extra", json!(null)),
        ("active_at_take", json!([1])),
        ("active_window_at_take", json!(1)),
        ("invalid", json!(true)),
    ] {
        let mut changed = empty.clone();
        changed[key] = value;
        assert!(
            std::panic::catch_unwind(|| shape(&changed)).is_err(),
            "{key}"
        );
    }
}
#[test]
fn saved_key_output_policy_scan_call_shape_rejects_metadata_damage_directly() {
    // Schema-only metadata is not a renderer, producer, source, or native witness.
    let call = json!({"ordinal":1,"parent":null,"context":{"depth":1,
        "recipe":{"input_frames":1,"output_frames":1,
            "rate":{"numerator":1,"denominator":1},"pitch_semitones":0,"engine":"schema-only"},
        "input":{"kind":"Signal","belongs_to_plan":true},
        "output":{"kind":"Signal","belongs_to_plan":true}}});
    call_shape(&call, 1);
    for key in ["ordinal", "parent", "context"] {
        let mut changed = call.clone();
        changed.as_object_mut().unwrap().remove(key);
        assert!(
            std::panic::catch_unwind(|| call_shape(&changed, 1)).is_err(),
            "{key}"
        );
    }
    for (key, value) in [
        ("ordinal", json!(true)),
        ("parent", json!("1")),
        ("parent", json!(1)),
        ("extra", json!(null)),
    ] {
        let mut changed = call.clone();
        changed[key] = value;
        assert!(
            std::panic::catch_unwind(|| call_shape(&changed, 1)).is_err(),
            "{key}"
        );
    }
}
#[test]
#[ignore = "manual fresh current-O ordinary P capture; no frozen44 expectation"]
fn capture_saved_key_output_policy_before_v1() {
    let output = std::env::var_os("DEADPAN_SAVED_KEY_OUTPUT_POLICY_BEFORE_OUTPUT").unwrap();
    let output = Path::new(&output);
    assert!(output.is_absolute() && output.starts_with("/tmp"));
    let variant = std::env::var("DEADPAN_SAVED_KEY_OUTPUT_POLICY_BEFORE_VARIANT").unwrap();
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_SAVED_KEY_OUTPUT_POLICY_BEFORE_SOURCE_SHA256").unwrap();
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    assert_eq!(
        std::env::var("DEADPAN_SAVED_KEY_OUTPUT_POLICY_BEFORE_PUBLISHED_CHECKPOINT").unwrap(),
        CHECKPOINT_P
    );
    let mut body = capture_body();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    body["published_checkpoint"] = json!(CHECKPOINT_P);
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
}
