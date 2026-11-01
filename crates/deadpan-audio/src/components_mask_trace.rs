//! Default-off, bounded receipts for actual single-output Components masks.
//! Raw pointers stay private and are used only for allocation identity checks.
use super::{StageAudioError, WorkControl, point_suppression::AudioSuppressionMask};
use serde_json::{Value, json};
use std::cell::RefCell;

const MAX_ROWS: usize = 1024;
const MAX_EVENTS: usize = 256;

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}
#[derive(Default)]
struct State {
    enabled: bool,
    rows: Vec<Row>,
    stack: Vec<usize>,
    attempted_rows: usize,
    truncated_rows: bool,
}
#[derive(Default)]
struct Row {
    value: Value,
    destination_ptr: usize,
    events: usize,
}
#[derive(Clone, Copy)]
pub(super) struct Receipt(usize);

#[derive(Clone, Copy)]
pub(super) struct Request {
    pub(super) start: i64,
    pub(super) program_depth: usize,
    pub(super) frames: usize,
    pub(super) active_frames: u64,
    pub(super) source_reservation_frames: u64,
    pub(super) account_live_frames: u64,
    pub(super) plan_work_before: usize,
    pub(super) cache_work_before: usize,
}

pub(super) fn enable() {
    STATE.with(|s| {
        *s.borrow_mut() = State {
            enabled: true,
            ..State::default()
        }
    });
}
pub(super) fn disable() {
    STATE.with(|s| s.borrow_mut().enabled = false);
}
pub(super) fn is_enabled() -> bool {
    STATE.with(|s| s.borrow().enabled)
}
pub(super) fn reset_if_enabled() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.enabled {
            *s = State {
                enabled: true,
                ..State::default()
            };
        }
    });
}
pub(super) fn snapshot() -> Value {
    STATE.with(|s| {
        let s = s.borrow();
        json!({
            "schema": "components-inline-marks-v2",
            "enabled": s.enabled,
            "attempted_rows": s.attempted_rows,
            "dropped_rows": s.attempted_rows - s.rows.len(),
            "truncated_rows": s.truncated_rows,
            "unfinished": !s.stack.is_empty(),
            "active_row": s.stack.last().copied(),
            "rows": s.rows.iter().map(|r| r.value.clone()).collect::<Vec<_>>(),
        })
    })
}

pub(super) fn begin_request(request: Request) -> Option<Receipt> {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if !s.enabled {
            return None;
        }
        s.attempted_rows += 1;
        if s.rows.len() >= MAX_ROWS {
            s.truncated_rows = true;
            return None;
        }
        let index = s.rows.len();
        let parent_row_id = s.stack.last().copied();
        s.rows.push(Row {
            value: json!({
                "row_id": index,
                "parent_row_id": parent_row_id,
                "program_depth": request.program_depth,
                "start": request.start,
                "requested_frames": request.frames,
                "active_frames_at_source": request.active_frames,
                "source_reservation_frames": request.source_reservation_frames,
                "account_live_frames_at_source": request.account_live_frames,
                "source_plan_work_before": request.plan_work_before,
                "source_cache_work_before": request.cache_work_before,
                "source_init": null,
                "reservation_phase": null,
                "source_storage": "inline_bitset",
                "source_heap_allocation": false,
                "source_heap_charge_frames": 0,
                "source_frames": request.frames,
                "inline_word_count": null,
                "inline_bytes": null,
                "inline_words_initial": null,
                "inline_words_latest": null,
                "raw_mark_count": null,
                "marks": {"attempt_count": 0, "written_count": 0, "first_attempt": null, "last_attempt": null, "first_written": null, "last_written": null, "first_accepted_work_after": null, "last_accepted_work_after": null},
                "callback_count": 0,
                "driver_kinds": [],
                "events": [],
                "projection_started": false,
                "inline_source_scope_closed": false,
                "destination_reservation": null,
                "destination_storage": null,
                "result": null
            }),
            ..Row::default()
        });
        s.stack.push(index);
        Some(Receipt(index))
    })
}

pub(super) fn source_initialized(
    receipt: Receipt,
    result: &Result<Option<super::shared_components::ComponentMarks>, StageAudioError>,
    plan_after: usize,
    cache_after: usize,
) {
    if !is_enabled() {
        return;
    }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(row) = s.rows.get_mut(receipt.0) else {
            return;
        };
        match result {
            Ok(Some(marks)) => {
                row.value["inline_word_count"] = json!(marks.test_words().len());
                row.value["inline_bytes"] = json!(std::mem::size_of_val(marks.test_words()));
                row.value["inline_words_initial"] = json!(marks.test_words());
                row.value["source_init"] = json!({
                    "success": true,
                    "plan_work_after": plan_after,
                    "cache_work_after": cache_after
                });
            }
            Ok(None) => {
                row.value["source_init"] = json!({
                    "success": false,
                    "error": "missing-source-buffer",
                    "plan_work_after": plan_after,
                    "cache_work_after": cache_after
                });
            }
            Err(error) => {
                row.value["source_init"] = json!({
                    "success": false,
                    "error": format!("{error:?}"),
                    "plan_work_after": plan_after,
                    "cache_work_after": cache_after
                });
            }
        }
    });
}

pub(super) fn source_init_failure_complete(
    receipt: Receipt,
    error: &StageAudioError,
    active_frames: u64,
    account_live_frames: u64,
    control: WorkControl<'_>,
) {
    set_failure_result(receipt, error, active_frames, account_live_frames, control);
    pop(receipt);
}
pub(super) fn driver(kind: &'static str) {
    mutate_current(|row| {
        row.value["driver_kinds"]
            .as_array_mut()
            .expect("driver list initialized")
            .push(json!(kind));
    });
}
pub(super) fn callback() {
    mutate_current(|row| {
        row.value["callback_count"] = json!(row.value["callback_count"].as_u64().unwrap_or(0) + 1)
    });
}
pub(super) fn mark_attempt(index: usize, sample: i64, control: WorkControl<'_>) {
    if !has_current_row() {
        return;
    }
    let work = control.work.borrow();
    let before = json!({
        "index": index,
        "sample": sample,
        "plan_work_before": work.plan_work,
        "cache_plan_work_before": work.cache_plan_work
    });
    mutate_current(|row| {
        let marks = &mut row.value["marks"];
        let count = marks["attempt_count"].as_u64().unwrap_or(0);
        marks["attempt_count"] = json!(count + 1);
        if marks["first_attempt"].is_null() {
            marks["first_attempt"] = before.clone();
        }
        marks["last_attempt"] = before.clone();
    });
}
pub(super) fn mark_written(index: usize, sample: i64, control: WorkControl<'_>) {
    if !has_current_row() {
        return;
    }
    let work = control.work.borrow();
    let after = json!({"plan_work": work.plan_work, "cache_plan_work": work.cache_plan_work});
    mutate_current(|row| {
        let marks = &mut row.value["marks"];
        let count = marks["written_count"].as_u64().unwrap_or(0);
        marks["written_count"] = json!(count + 1);
        let value = json!([index, sample]);
        if marks["first_written"].is_null() {
            marks["first_written"] = value.clone();
        }
        marks["last_written"] = value;
        if marks["first_accepted_work_after"].is_null() {
            marks["first_accepted_work_after"] = after.clone();
        }
        marks["last_accepted_work_after"] = after;
    });
}
pub(super) fn mark_refused(index: usize, sample: i64, error: &StageAudioError) {
    if !has_current_row() {
        return;
    }
    event(
        "mark_refused",
        json!({"index": index, "sample": sample, "error": format!("{error:?}")}),
    );
}
pub(super) fn projection_begin() {
    mutate_current(|row| {
        row.value["projection_started"] = json!(true);
        row.value["reservation_phase"] = json!("destination");
    });
}
pub(super) fn projection_end() {
    mutate_current(|row| row.value["reservation_phase"] = Value::Null);
}
pub(super) fn reserve_before(frames: u64, active: u64, live: u64) {
    if let Some(phase) = reservation_phase() {
        event(
            &format!("{phase}_reserve_before"),
            json!({
                "frames": frames, "active_frames": active, "account_live_frames": live
            }),
        );
    }
}
pub(super) fn reserve_after(frames: u64, accepted: bool, active: u64, live: u64) {
    let Some(phase) = reservation_phase() else {
        return;
    };
    let reservation = json!({
        "frames": frames, "accepted": accepted, "active_frames": active, "account_live_frames": live
    });
    mutate_current(|row| {
        if phase == "destination" {
            row.value["destination_reservation"] = reservation.clone();
        } else {
            row.value["source_reservation"] = reservation.clone();
        }
    });
    event(
        &format!("{phase}_reserve_after"),
        json!({
            "frames": frames, "accepted": accepted, "active_frames": active, "account_live_frames": live
        }),
    );
}
pub(super) fn destination_storage(ptr: usize, capacity: usize) {
    if !projecting() {
        return;
    }
    mutate_current(|row| {
        row.destination_ptr = ptr;
        row.value["destination_storage"] = json!({"capacity": capacity});
    });
}
pub(super) fn inline_source_snapshot(marks: &super::shared_components::ComponentMarks) {
    mutate_current(|row| {
        row.value["inline_words_latest"] = json!(marks.test_words());
    });
}
pub(super) fn counted_marks(count: usize) {
    mutate_current(|row| row.value["raw_mark_count"] = json!(count));
}
pub(super) fn inline_source_closed() {
    mutate_current(|row| row.value["inline_source_scope_closed"] = json!(true));
}
pub(super) fn source_account_after(live: u64) {
    mutate_current(|row| row.value["account_live_frames_at_completion"] = json!(live));
}
pub(super) fn complete(
    receipt: Receipt,
    result: &Result<AudioSuppressionMask, StageAudioError>,
    active_frames: u64,
    account_live_frames: u64,
    control: WorkControl<'_>,
) {
    if !is_enabled() {
        return;
    }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(row) = s.rows.get_mut(receipt.0) else {
            return;
        };
        let (ok, error, len, cap, owned, same) = match result {
            Ok(mask) => (
                true,
                None,
                mask.len(),
                mask.capacity(),
                mask.is_owned(),
                row.destination_ptr != 0
                    && row.destination_ptr == mask.as_slice().as_ptr() as usize,
            ),
            Err(error) => (false, Some(format!("{error:?}")), 0, 0, false, false),
        };
        row.value["result"] = json!({
            "success": ok, "error": error, "mask_len": len, "mask_capacity": cap,
            "owned": owned, "destination_identity_preserved": same,
            "active_frames": active_frames, "account_live_frames": account_live_frames,
            "plan_work_after": control.work.borrow().plan_work,
            "cache_plan_work_after": control.work.borrow().cache_plan_work
        });
    });
    pop(receipt);
}

pub(super) fn complete_failure(
    receipt: Receipt,
    error: &StageAudioError,
    active_frames: u64,
    account_live_frames: u64,
    control: WorkControl<'_>,
) {
    if !is_enabled() {
        return;
    }
    set_failure_result(receipt, error, active_frames, account_live_frames, control);
    pop(receipt);
}

fn set_failure_result(
    receipt: Receipt,
    error: &StageAudioError,
    active_frames: u64,
    account_live_frames: u64,
    control: WorkControl<'_>,
) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(row) = s.rows.get_mut(receipt.0) else {
            return;
        };
        row.value["result"] = json!({
            "success": false,
            "error": format!("{error:?}"),
            "active_frames": active_frames,
            "account_live_frames": account_live_frames,
            "plan_work_after": control.work.borrow().plan_work,
            "cache_plan_work_after": control.work.borrow().cache_plan_work,
        });
    });
}

fn pop(receipt: Receipt) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.stack.last() == Some(&receipt.0) {
            s.stack.pop();
        }
    });
}
fn has_current_row() -> bool {
    STATE.with(|s| {
        let s = s.borrow();
        s.enabled && s.stack.last().is_some_and(|i| *i < s.rows.len())
    })
}
fn projecting() -> bool {
    STATE.with(|s| {
        let s = s.borrow();
        s.stack
            .last()
            .is_some_and(|i| s.enabled && s.rows[*i].value["projection_started"] == true)
    })
}
fn reservation_phase() -> Option<&'static str> {
    STATE.with(|s| {
        let s = s.borrow();
        let index = *s.stack.last()?;
        if !s.enabled {
            return None;
        }
        match s.rows[index].value["reservation_phase"].as_str()? {
            "source" => Some("source"),
            "destination" => Some("destination"),
            _ => None,
        }
    })
}
fn mutate_current(f: impl FnOnce(&mut Row)) {
    if !is_enabled() {
        return;
    }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if let Some(index) = s.stack.last().copied() {
            f(&mut s.rows[index]);
        }
    });
}
fn event(kind: &str, value: Value) {
    mutate_current(|row| {
        let events = row.value["events"]
            .as_array_mut()
            .expect("events initialized");
        if row.events >= MAX_EVENTS {
            row.value["events_truncated"] = json!(true);
        } else {
            row.events += 1;
            events.push(json!({"kind": kind, "value": value}));
        }
    });
}
