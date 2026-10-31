//! Default-off bounded receipts for actual non-separate SharedMix producers.
//! Pointer integers are retained only for in-process identity checks, never serialized.
use super::{
    AudioSample, WorkControl,
    original_request::flattened_work_trace,
    point_suppression::{
        AudioSuppressionMask,
        ops::{MaskWorkPhase, trace as native},
    },
    shared_mix::Permissions,
};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};

const MAX_ROWS: usize = 128;
const MAX_EVENTS_PER_ROW: usize = 64;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static STATE: RefCell<State> = RefCell::new(State::default());
}

#[derive(Default)]
struct State {
    rows: Vec<Row>,
    current: Option<usize>,
    truncated_rows: bool,
    truncated_events: bool,
}

#[derive(Default)]
struct Row {
    json: Value,
    source_ptr: usize,
    source_len: usize,
    source_capacity: usize,
    destination_ptr: usize,
    destination_capacity: usize,
    event_count: usize,
}

#[derive(Clone, Copy)]
pub(super) struct PendingSource {
    start: AudioSample,
    pointer: usize,
    len: usize,
    capacity: usize,
}

#[derive(Clone, Copy)]
pub(super) struct Receipt(usize);

pub(super) fn pending_source(start: AudioSample, permissions: &Vec<Permissions>) -> PendingSource {
    PendingSource {
        start,
        pointer: permissions.as_ptr() as usize,
        len: permissions.len(),
        capacity: permissions.capacity(),
    }
}

pub(super) fn enable() {
    STATE.with(|state| *state.borrow_mut() = State::default());
    ENABLED.with(|enabled| enabled.set(true));
}

pub(super) fn disable() {
    ENABLED.with(|enabled| enabled.set(false));
}

pub(super) fn is_enabled() -> bool {
    active()
}

pub(super) fn reset_if_enabled() {
    if active() {
        STATE.with(|state| *state.borrow_mut() = State::default());
    }
}

pub(super) fn begin(
    source: PendingSource,
    source_reservation_frames: u64,
    active_frames: u64,
    account_live_frames: u64,
    control: WorkControl<'_>,
) -> Option<Receipt> {
    if !active() {
        return None;
    }
    let work = {
        let work = control.work.borrow();
        [work.plan_work, work.cache_plan_work]
    };
    let host = flattened_work_trace::host_eviction_boundary();
    let native_boundary = native::work_boundary();
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.rows.len() == MAX_ROWS {
            state.truncated_rows = true;
            state.current = None;
            return None;
        }
        let index = state.rows.len();
        state.rows.push(Row {
            json: json!({
                "start": source.start.0,
                "source_len": source.len,
                "source_capacity": source.capacity,
                "source_element_bytes": std::mem::size_of::<Permissions>(),
                "source_reservation_frames": source_reservation_frames,
                "active_frames_at_entry": active_frames,
                "account_live_frames_at_entry": account_live_frames,
                "plan_work_at_entry": work[0],
                "cache_plan_work_at_entry": work[1],
                "native_work_before": serde_json::to_value(native_boundary).unwrap(),
                "host_boundary_before": host,
                "events": [],
                "work_by_phase": {},
                "source_disposed": false,
                "source_identity_preserved": null,
                "result": null,
                "events_truncated": false
            }),
            source_ptr: source.pointer,
            source_len: source.len,
            source_capacity: source.capacity,
            event_count: 0,
            ..Row::default()
        });
        state.current = Some(index);
        Some(Receipt(index))
    })
}

pub(super) fn reserve_before(
    frames: u64,
    active_frames: u64,
    account_live_frames: u64,
    control: WorkControl<'_>,
) {
    if !recording() {
        return;
    }
    let work = {
        let work = control.work.borrow();
        [work.plan_work, work.cache_plan_work]
    };
    event(
        "reserve_before",
        json!({
            "requested_frames": frames,
            "active_frames": active_frames,
            "account_live_frames": account_live_frames,
            "plan_work": work[0],
            "cache_plan_work": work[1]
        }),
    );
}

pub(super) fn reserve_after(
    frames: u64,
    accepted: bool,
    active_frames: u64,
    account_live_frames: u64,
    control: WorkControl<'_>,
) {
    if !recording() {
        return;
    }
    let work = {
        let work = control.work.borrow();
        [work.plan_work, work.cache_plan_work]
    };
    if active() {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if let Some(index) = state.current {
                state.rows[index].json["destination_reservation"] = json!({
                    "frames": frames,
                    "accepted": accepted
                });
            }
        });
    }
    event(
        "reserve_after",
        json!({
            "requested_frames": frames,
            "accepted": accepted,
            "active_frames": active_frames,
            "account_live_frames": account_live_frames,
            "plan_work": work[0],
            "cache_plan_work": work[1]
        }),
    );
}

pub(super) fn destination_storage(pointer: usize, capacity: usize) {
    if !active() {
        return;
    }
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(index) = state.current else { return };
        let row = &mut state.rows[index];
        row.destination_ptr = pointer;
        row.destination_capacity = capacity;
        row.json["destination_storage"] = json!({"capacity": capacity});
    });
}

pub(super) fn work(phase: MaskWorkPhase, units: usize, accepted: bool) {
    if !active() {
        return;
    }
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(index) = state.current else { return };
        let row = &mut state.rows[index];
        let key = format!("{phase:?}");
        let phase_counts = row.json["work_by_phase"].as_object_mut().unwrap();
        let counts = phase_counts.entry(key).or_insert_with(|| {
            json!({
                "attempted_units": 0_u64,
                "accepted_units": 0_u64,
                "failed_calls": 0_u64
            })
        });
        counts["attempted_units"] =
            json!(counts["attempted_units"].as_u64().unwrap() + units as u64);
        if accepted {
            counts["accepted_units"] =
                json!(counts["accepted_units"].as_u64().unwrap() + units as u64);
        } else {
            counts["failed_calls"] = json!(counts["failed_calls"].as_u64().unwrap() + 1);
        }
    });
}

pub(super) fn source_disposed(receipt: Receipt, pointer: usize, len: usize, capacity: usize) {
    if !active() {
        return;
    }
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(row) = state.rows.get_mut(receipt.0) else {
            return;
        };
        row.json["source_disposed"] = json!(true);
        row.json["source_identity_preserved"] = json!(
            row.source_ptr == pointer && row.source_len == len && row.source_capacity == capacity
        );
        row.json["source_disposed_len"] = json!(len);
        row.json["source_disposed_capacity"] = json!(capacity);
    });
}

pub(super) fn complete(
    receipt: Receipt,
    result: &Result<AudioSuppressionMask, super::StageAudioError>,
    active_frames: u64,
    account_live_frames: u64,
    control: WorkControl<'_>,
) {
    if !active() {
        return;
    }
    let work = {
        let work = control.work.borrow();
        [work.plan_work, work.cache_plan_work]
    };
    let native_boundary = serde_json::to_value(native::work_boundary()).unwrap();
    let host = flattened_work_trace::host_eviction_boundary();
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(index) = state.rows.get_mut(receipt.0) else {
            return;
        };
        let (success, error, len, capacity, identity, owned, distinct) = match result {
            Ok(mask) => {
                let pointer = mask.as_slice().as_ptr() as usize;
                (
                    true,
                    None,
                    mask.len(),
                    mask.capacity(),
                    index.destination_ptr != 0 && index.destination_ptr == pointer,
                    mask.is_owned(),
                    index.source_ptr != 0
                        && index.destination_ptr != 0
                        && index.source_ptr != index.destination_ptr,
                )
            }
            Err(error) => (false, Some(format!("{error:?}")), 0, 0, false, false, false),
        };
        index.json["result"] = json!({
            "success": success,
            "error": error,
            "mask_len": len,
            "mask_capacity": capacity,
            "owned": owned,
            "destination_identity_preserved": identity,
            "source_destination_allocations_distinct": distinct,
            "active_frames": active_frames,
            "account_live_frames": account_live_frames,
            "active_frames_after_disposal": active_frames,
            "account_live_frames_after_disposal": account_live_frames,
            "plan_work_after": work[0],
            "cache_plan_work_after": work[1],
            "native_work_after": native_boundary,
            "host_boundary_after": host
        });
        state.current = None;
    });
}

fn active() -> bool {
    ENABLED.with(Cell::get)
}

fn recording() -> bool {
    active() && STATE.with(|state| state.borrow().current.is_some())
}

fn event(kind: &str, value: Value) {
    if !active() {
        return;
    }
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(index) = state.current else { return };
        if state.rows[index].event_count >= MAX_EVENTS_PER_ROW {
            state.truncated_events = true;
            state.rows[index].json["events_truncated"] = json!(true);
            return;
        }
        let row = &mut state.rows[index];
        row.event_count += 1;
        row.json["events"]
            .as_array_mut()
            .unwrap()
            .push(json!({"kind": kind, "value": value}));
    });
}

pub(super) fn snapshot() -> Value {
    STATE.with(|state| {
        let state = state.borrow();
        json!({
            "enabled": active(),
            "unfinished": state.current.is_some(),
            "active_row": state.current,
            "row_limit": MAX_ROWS,
            "event_limit_per_row": MAX_EVENTS_PER_ROW,
            "truncated_rows": state.truncated_rows,
            "truncated_events": state.truncated_events,
            "rows": state.rows.iter().map(|row| &row.json).collect::<Vec<_>>()
        })
    })
}
