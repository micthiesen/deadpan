//! Default-off observations of the ordinary saved-key output-bit caller.
use super::*;
use serde_json::{Value, json};

const MAX_ROWS: usize = 32768;
const MAX_SCANS: usize = 4096;
const MAX_WINDOWS: usize = 4096;
const MAX_ACTIVE: usize = 128;
const MAX_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

struct Capture {
    epoch: u64,
    attempted: usize,
    dropped: usize,
    invalid: bool,
    overflow: bool,
    calls: Vec<Value>,
    rows: Vec<Value>,
    stack: Vec<usize>,
    windows: Vec<Value>,
    next_window: usize,
    active_window: Option<usize>,
    payload_bytes: usize,
}
thread_local! {
    static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) };
    static EPOCH: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
pub(super) struct CaptureGuard(u64);
impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.as_ref().is_some_and(|c| c.epoch == self.0) {
                *slot = None;
            }
        });
    }
}
pub(super) fn begin() -> CaptureGuard {
    let epoch = EPOCH.with(|value| {
        let next = value
            .get()
            .checked_add(1)
            .expect("P before epoch exhausted");
        value.set(next);
        next
    });
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(slot.is_none(), "saved-key output trace already active");
        *slot = Some(Capture {
            epoch,
            attempted: 0,
            dropped: 0,
            invalid: false,
            overflow: false,
            calls: Vec::new(),
            rows: Vec::new(),
            stack: Vec::new(),
            windows: Vec::new(),
            next_window: 0,
            active_window: None,
            payload_bytes: 0,
        });
    });
    CaptureGuard(epoch)
}
fn enabled() -> bool {
    CAPTURE.with(|slot| slot.borrow().as_ref().is_some_and(|c| !c.invalid))
}
fn position_value(c: &Capture) -> Value {
    json!({"schema":"saved-key-output-policy-before-position-v2",
        "last_scan":c.calls.len(),"last_row":c.attempted,"last_window":c.next_window,
        "active_scans":c.stack,"active_window":c.active_window})
}
pub(super) fn position() -> Option<Value> {
    CAPTURE.with(|slot| slot.borrow().as_ref().map(position_value))
}
fn overflow(c: &mut Capture) {
    c.invalid = true;
    c.overflow = true;
    c.dropped = c.dropped.saturating_add(1);
}
fn account(c: &mut Capture, value: &Value) -> bool {
    let bytes = serde_json::to_vec(value)
        .expect("P before JSON serializes")
        .len();
    let Some(total) = c.payload_bytes.checked_add(bytes) else {
        overflow(c);
        return false;
    };
    if total > MAX_PAYLOAD_BYTES {
        overflow(c);
        return false;
    }
    c.payload_bytes = total;
    true
}
pub(super) struct ScanGuard(Option<(u64, usize)>);
impl Drop for ScanGuard {
    fn drop(&mut self) {
        let Some((epoch, ordinal)) = self.0 else {
            return;
        };
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(c) = slot.as_mut().filter(|c| c.epoch == epoch) else {
                return;
            };
            if c.stack.last() != Some(&ordinal) {
                c.invalid = true;
                return;
            }
            c.stack.pop();
        });
    }
}
pub(super) fn scan(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    context: impl FnOnce() -> Value,
) -> ScanGuard {
    if !enabled() {
        return ScanGuard(None);
    }
    let context = context();
    let guard = ScanGuard(CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let c = slot.as_mut()?;
        if c.calls.len() >= MAX_SCANS || c.stack.len() >= MAX_ACTIVE {
            overflow(c);
            return None;
        }
        let ordinal = c.calls.len() + 1;
        let call = json!({"ordinal":ordinal,"parent":c.stack.last(),"context":context});
        if !account(c, &call) {
            return None;
        }
        c.calls.push(call);
        c.stack.push(ordinal);
        Some((c.epoch, ordinal))
    }));
    mark(renderer, control, "scan_enter", || json!({}));
    guard
}
pub(super) struct WindowGuard {
    capture: Option<(u64, usize, Option<usize>, usize)>,
    ordinary: Option<stage_block_policy_before_trace::CaptureGuard>,
}
impl Drop for WindowGuard {
    fn drop(&mut self) {
        let Some((epoch, ordinal, scan, row_before)) = self.capture else {
            return;
        };
        // The existing O caller is declared later, so its frame is already gone.
        let observer = self
            .ordinary
            .as_ref()
            .map_or(Value::Null, stage_block_policy_before_trace::take_owned);
        drop(self.ordinary.take());
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(c) = slot.as_mut().filter(|c| c.epoch == epoch) else {
                return;
            };
            if c.active_window != Some(ordinal) {
                c.invalid = true;
                return;
            }
            c.active_window = None;
            if c.invalid {
                return;
            }
            if c.windows.len() >= MAX_WINDOWS {
                overflow(c);
                return;
            }
            if observer.is_null()
                || observer["invalid"] != false
                || observer["overflow"] != false
                || observer["dropped"] != 0
            {
                c.invalid = true;
            }
            let value = json!({"ordinal":ordinal,"scan":scan,"row_before":row_before,
                "row_after":c.attempted,"observer":observer});
            if account(c, &value) {
                c.windows.push(value)
            }
        });
    }
}
pub(super) fn window() -> WindowGuard {
    if !enabled() {
        return WindowGuard {
            capture: None,
            ordinary: None,
        };
    }
    // A conflicting ordinary capture is evidence failure; never take its data.
    if stage_block_policy_before_trace::position().is_some() {
        CAPTURE.with(|slot| {
            if let Some(c) = slot.borrow_mut().as_mut() {
                c.invalid = true
            }
        });
        return WindowGuard {
            capture: None,
            ordinary: None,
        };
    }
    let capture = CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let c = slot.as_mut()?;
        if c.active_window.is_some() {
            c.invalid = true;
            return None;
        }
        if c.next_window >= MAX_WINDOWS {
            overflow(c);
            return None;
        }
        c.next_window += 1;
        c.active_window = Some(c.next_window);
        Some((c.epoch, c.next_window, c.stack.last().copied(), c.attempted))
    });
    WindowGuard {
        ordinary: capture.map(|_| stage_block_policy_before_trace::begin()),
        capture,
    }
}
pub(super) fn mark(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    phase: &'static str,
    payload: impl FnOnce() -> Value,
) {
    if !enabled() {
        return;
    }
    let value = payload();
    // The exact byte stream is already retained by N KeysOnly. Per-bit rows
    // carry no renderer snapshot or copied recorder stack.
    let thin = matches!(phase, "bit_hash_before" | "bit_hash_after");
    let (snapshot, o_position, o_native_position, n_before_position, n_native_position) = if thin {
        (Value::Null, None, None, None, None)
    } else {
        (
            stage_block_policy_before_trace::snapshots(renderer, Some(control)),
            stage_block_policy_before_trace::position(),
            stage_block_policy_native_trace::position(),
            read_signal_policy_before_trace::position(),
            read_signal_policy_native_trace::position(),
        )
    };
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        if c.invalid { return }
        let Some(ordinal) = c.attempted.checked_add(1) else {
            overflow(c);
            return;
        };
        c.attempted = ordinal;
        if c.rows.len() >= MAX_ROWS {
            overflow(c);
            return;
        }
        let row = json!({"ordinal":ordinal,"scan":c.stack.last(),"window":c.active_window,"phase":phase,
            "position":if thin { Value::Null } else { position_value(c) },"value":value,"snapshot":snapshot,
            "o_position":o_position,"o_native_position":o_native_position,
            "n_before_position":n_before_position,"n_native_position":n_native_position});
        if account(c, &row) { c.rows.push(row) }
    });
}
pub(super) fn take() -> Value {
    CAPTURE.with(|slot| slot.borrow_mut().take().map_or(Value::Null, |mut c| {
        if !c.stack.is_empty() || c.active_window.is_some() { c.invalid = true }
        let mut value = json!({"schema":"saved-key-output-policy-before-v2",
            "attempted":c.attempted,"dropped":c.dropped,"invalid":c.invalid,"overflow":c.overflow,
            "active_at_take":c.stack,"active_window_at_take":c.active_window,
            "calls":c.calls,"rows":c.rows,"windows":c.windows,"payload_bytes":c.payload_bytes,
            "bounds":{"rows":MAX_ROWS,"scans":MAX_SCANS,"windows":MAX_WINDOWS,"active_scans":MAX_ACTIVE,
                "serialized_payload_bytes":MAX_PAYLOAD_BYTES},
            "scope":"ordinary saved-key output bits; no P selected Admission, source owner, fault or policy query",
            "observer_storage":"post-construction serialized retained calls/rows/windows accounting and final document check; not physical RSS; snapshots/JSON temporaries and inner O capture outside request ledger"});
        if serde_json::to_vec(&value).expect("P before JSON serializes").len() > MAX_PAYLOAD_BYTES {
            value["invalid"] = json!(true);
            value["overflow"] = json!(true);
            value["dropped"] = json!(c.dropped.saturating_add(1));
        }
        value
    }))
}

#[test]
fn capture_epoch_does_not_clear_a_later_capture() {
    let old = begin();
    let empty = take();
    assert_eq!(empty["schema"], "saved-key-output-policy-before-v2");
    assert_eq!(empty["attempted"], 0);
    assert_eq!(empty["payload_bytes"], 0);
    assert_eq!(empty["invalid"], false);
    let current = begin();
    drop(old);
    assert!(position().is_some());
    drop(current);
    assert!(position().is_none());
}

#[test]
fn serialized_limit_fails_closed_before_retaining_another_value() {
    let mut c = Capture {
        epoch: 1,
        attempted: 0,
        dropped: 0,
        invalid: false,
        overflow: false,
        calls: Vec::new(),
        rows: Vec::new(),
        stack: Vec::new(),
        windows: Vec::new(),
        next_window: 0,
        active_window: None,
        payload_bytes: MAX_PAYLOAD_BYTES,
    };
    assert!(!account(&mut c, &json!({})));
    assert!(c.invalid && c.overflow);
    assert_eq!(c.dropped, 1);
    assert_eq!(c.payload_bytes, MAX_PAYLOAD_BYTES);
    assert!(c.calls.is_empty() && c.rows.is_empty());
}

#[test]
fn window_is_lazy_when_p_is_off_and_collects_only_its_ordinary_epoch() {
    assert!(position().is_none());
    let disabled = window();
    assert!(stage_block_policy_before_trace::position().is_none());
    drop(disabled);
    let capture = begin();
    let local = window();
    let frame = stage_block_policy_before_trace::phase(None, "empty_metadata_test", None);
    drop(frame);
    drop(local);
    assert!(stage_block_policy_before_trace::position().is_none());
    let value = take();
    assert_eq!(value["invalid"], false);
    assert_eq!(value["windows"].as_array().expect("window table").len(), 1);
    assert_eq!(value["windows"][0]["observer"]["invalid"], false);
    assert_eq!(value["windows"][0]["observer"]["active_at_take"], json!([]));
    assert_eq!(value["windows"][0]["row_before"], 0);
    assert_eq!(value["windows"][0]["row_after"], 0);
    drop(capture);
}

#[test]
fn conflicting_ordinary_capture_is_preserved_and_invalidates_p() {
    let ordinary = stage_block_policy_before_trace::begin();
    let capture = begin();
    let local = window();
    drop(local);
    assert!(stage_block_policy_before_trace::position().is_some());
    let value = take();
    assert_eq!(value["invalid"], true);
    assert_eq!(value["windows"], json!([]));
    drop(capture);
    drop(ordinary);
    assert!(stage_block_policy_before_trace::position().is_none());
}
#[test]
fn stale_ordinary_guard_cannot_take_a_later_capture() {
    let old = stage_block_policy_before_trace::begin();
    assert!(!stage_block_policy_before_trace::take().is_null());
    let current = stage_block_policy_before_trace::begin();
    assert!(stage_block_policy_before_trace::take_owned(&old).is_null());
    drop(old);
    assert!(stage_block_policy_before_trace::position().is_some());
    drop(current);
    assert!(stage_block_policy_before_trace::position().is_none());
}
