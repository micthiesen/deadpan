//! Default-off ordinary read_signal witness; tickets identify calls, not storage.
use super::*;
use serde_json::{Value, json};
use std::alloc::Layout;

const MAX_ROWS: usize = 32768;
const MAX_CALLS: usize = 4096;
const MAX_ACTIVE: usize = 128;
const MAX_KEY_STREAMS: usize = 256;
const MAX_KEY_UPDATES: usize = 262144;
const MAX_KEY_BYTES: usize = 16 * 1024 * 1024;
const MAX_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Ticket {
    epoch: u64,
    ordinal: usize,
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Caller {
    Definition,
    PublicTape,
    RoutedSource,
    StreamedStageInput,
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Consumer {
    Preflight,
    Numerical,
}
#[derive(Clone, Copy, Debug)]
pub(super) enum SavedPhase {
    Disabled,
    ScopeEnter,
    RawResult,
    ScopeExit,
    MappedResult,
}
struct KeyStream {
    ordinal: usize,
    parent: Option<usize>,
    entry_position: Value,
    updates: Vec<Value>,
    finish: Option<[u8; 32]>,
    result: Option<Value>,
    exited: bool,
}
struct Frame {
    id: usize,
    ticket: Option<Ticket>,
    kind: &'static str,
    phase: &'static str,
    index: Option<usize>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum CaptureMode {
    Ordinary,
    KeysOnly,
}
struct Capture {
    mode: CaptureMode,
    epoch: u64,
    attempted: usize,
    dropped: usize,
    invalid: bool,
    next_frame: usize,
    stack: Vec<Frame>,
    calls: Vec<Value>,
    rows: Vec<Value>,
    payload_bytes: usize,
    key_streams: Vec<KeyStream>,
    key_stack: Vec<usize>,
    key_updates: usize,
    key_bytes: usize,
    key_failed: bool,
    key_result_pending: Option<usize>,
}
thread_local! {
    static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) };
    static EPOCH: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
pub(super) struct CaptureGuard(u64);
impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CAPTURE.with(|s| {
            let mut s = s.borrow_mut();
            if s.as_ref().is_some_and(|c| c.epoch == self.0) {
                *s = None;
            }
        });
    }
}
pub(super) fn begin() -> CaptureGuard {
    begin_mode(CaptureMode::Ordinary)
}
pub(super) fn begin_keys() -> CaptureGuard {
    begin_mode(CaptureMode::KeysOnly)
}
fn begin_mode(mode: CaptureMode) -> CaptureGuard {
    let epoch = EPOCH.with(|e| {
        let n = e.get().checked_add(1).expect("N trace epoch exhausted");
        e.set(n);
        n
    });
    CAPTURE.with(|s| {
        let mut s = s.borrow_mut();
        assert!(s.is_none(), "read signal trace already active");
        *s = Some(Capture {
            mode,
            epoch,
            attempted: 0,
            dropped: 0,
            invalid: false,
            next_frame: 0,
            stack: Vec::new(),
            calls: Vec::new(),
            rows: Vec::new(),
            payload_bytes: 0,
            key_streams: Vec::new(),
            key_stack: Vec::new(),
            key_updates: 0,
            key_bytes: 0,
            key_failed: false,
            key_result_pending: None,
        });
    });
    CaptureGuard(epoch)
}
fn enabled() -> bool {
    CAPTURE.with(|s| s.borrow().is_some())
}
fn selected(c: &Capture) -> Option<Ticket> {
    for f in c.stack.iter().rev() {
        if f.kind == "invocation_mix" {
            return None;
        }
        if f.kind == "consumer" || f.kind == "invocation" {
            return f.ticket;
        }
    }
    None
}
fn position_value(c: &Capture) -> Value {
    json!({"last_attempted_ordinal":c.attempted,
        "selected_ticket":selected(c).map(|t|t.ordinal),
        "active_stack":c.stack.iter().map(|f|json!({"frame":f.id,
            "ticket":f.ticket.map(|t|t.ordinal),"kind":f.kind,"phase":f.phase,"index":f.index})).collect::<Vec<_>>(),
        "active_key_streams":c.key_stack})
}
pub(super) fn position() -> Option<Value> {
    CAPTURE.with(|s| {
        s.borrow().as_ref().map(|c| {
            let mut value = position_value(c);
            if c.mode == CaptureMode::KeysOnly {
                value["schema"] = json!("read-signal-policy-keys-position-v1");
            }
            value
        })
    })
}
pub(super) fn take() -> Value {
    take_mode(CaptureMode::Ordinary)
}
pub(super) fn take_keys() -> Value {
    take_mode(CaptureMode::KeysOnly)
}
fn take_mode(expected_mode: CaptureMode) -> Value {
    CAPTURE.with(|s|s.borrow_mut().take().map_or(Value::Null, |mut c| {
        if c.mode != expected_mode || !c.stack.is_empty() || !c.key_stack.is_empty() || c.key_result_pending.is_some() { c.invalid=true; }
        let mut value = json!({"schema":"read-signal-policy-before-v1","attempted":c.attempted,"dropped":c.dropped,
            "invalid":c.invalid,"active_at_take":position_value(&c)["active_stack"],"calls":c.calls,"rows":c.rows,
            "payload_bytes":c.payload_bytes,"key_transcript_schema":"stage-pcm-hash-updates-v1",
            "key_updates":c.key_updates,"key_bytes":c.key_bytes,"key_failed":c.key_failed,
            "active_key_streams_at_take":c.key_stack,"key_result_pending_at_take":c.key_result_pending,
            "key_transcripts":c.key_streams.iter().map(|s|json!({"ordinal":s.ordinal,
                "parent":s.parent,"entry_position":s.entry_position,"updates":s.updates,
                "actual_finish":s.finish,"actual_scan_result":s.result,"exited":s.exited,
                "termination":if s.finish.is_some() {"finalized"} else {"prefix_only"}})).collect::<Vec<_>>()});
        if c.mode == CaptureMode::KeysOnly {
            value["schema"]=json!("read-signal-policy-keys-v1");
            if let Some(calls)=value["calls"].as_array_mut() {
                for call in calls {
                    if let Some(fields)=call.as_object_mut() {
                        fields.remove("producer_entered");
                        fields.remove("stop_phase");
                    }
                }
            }
        }
        if serde_json::to_vec(&value).expect("trace JSON serializes").len() > MAX_PAYLOAD_BYTES {
            value["invalid"] = json!(true);
            value["dropped"] = json!(c.dropped + 1);
        }
        value
    }))
}
fn push(
    c: &mut Capture,
    phase: &'static str,
    ticket: Option<Ticket>,
    value: Value,
    snapshot: Value,
) {
    c.attempted += 1;
    if c.rows.len() == MAX_ROWS {
        c.dropped += 1;
        c.invalid = true;
        return;
    }
    if ticket.is_some_and(|t| t.epoch != c.epoch || t.ordinal == 0 || t.ordinal > c.calls.len()) {
        c.invalid = true;
        return;
    }
    let row = json!({"ordinal":c.attempted,"phase":phase,"ticket":ticket.map(|t|t.ordinal),
        "position":position_value(c),"value":value,"snapshot":snapshot});
    let bytes = serde_json::to_vec(&row)
        .expect("trace JSON serializes")
        .len();
    if bytes > MAX_PAYLOAD_BYTES - c.payload_bytes {
        c.dropped += 1;
        c.invalid = true;
        return;
    }
    c.payload_bytes += bytes;
    c.rows.push(row);
}
pub(super) struct FrameGuard(Option<(u64, usize)>);
impl Drop for FrameGuard {
    fn drop(&mut self) {
        let Some((epoch, id)) = self.0 else {
            return;
        };
        CAPTURE.with(|s| {
            let mut s = s.borrow_mut();
            let Some(c) = s.as_mut().filter(|c| c.epoch == epoch) else {
                return;
            };
            if c.stack.last().map(|f| f.id) != Some(id) {
                c.invalid = true;
                return;
            }
            let f = c.stack.pop().expect("observed frame");
            push(
                c,
                "lexical_exit",
                f.ticket,
                json!({"kind":f.kind,"phase":f.phase,
                "proof":"lexical exit only; no allocation/free or native lease event"}),
                Value::Null,
            );
        });
    }
}
fn frame(ticket: Option<Ticket>, kind: &'static str, phase: &'static str) -> FrameGuard {
    CAPTURE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(c) = s.as_mut() else {
            return FrameGuard(None);
        };
        if c.stack.len() == MAX_ACTIVE {
            c.invalid = true;
            c.dropped += 1;
            return FrameGuard(None);
        }
        c.next_frame += 1;
        let id = c.next_frame;
        c.stack.push(Frame {
            id,
            ticket,
            kind,
            phase,
            index: None,
        });
        FrameGuard(Some((c.epoch, id)))
    })
}
pub(super) struct CallScope {
    ticket: Option<Ticket>,
    _frame: FrameGuard,
}
impl CallScope {
    pub(super) fn ticket(&self) -> Option<Ticket> {
        self.ticket
    }
}
pub(super) fn call(
    renderer: &StageAudio,
    caller: Caller,
    start: SignalSample,
    frames: u32,
    depth: usize,
) -> CallScope {
    if !enabled() {
        return CallScope {
            ticket: None,
            _frame: FrameGuard(None),
        };
    }
    let snapshot = snapshots(renderer, None);
    let ticket = CAPTURE.with(|s| {
        let mut s = s.borrow_mut();
        let c = s.as_mut()?;
        if c.calls.len() == MAX_CALLS {
            c.invalid = true;
            c.dropped += 1;
            return None;
        }
        let t = Ticket {
            epoch: c.epoch,
            ordinal: c.calls.len() + 1,
        };
        let parent = c
            .stack
            .iter()
            .rev()
            .find(|f| f.kind == "call")
            .and_then(|f| f.ticket)
            .map(|t| t.ordinal);
        let parent_selected_ticket = selected(c).map(|t| t.ordinal);
        c.calls.push(
            json!({"ordinal":t.ordinal,"caller":format!("{caller:?}"),"parent":parent,
            "parent_selected_ticket":parent_selected_ticket,"start":start,"frames":frames,
            "depth":depth,"entered_read_signal":false,"producer_entered":false}),
        );
        push(c, "caller_enter", Some(t), json!({}), snapshot);
        Some(t)
    });
    CallScope {
        ticket,
        _frame: frame(ticket, "call", "caller"),
    }
}
pub(super) fn invocation(ticket: Option<Ticket>, input: SignalInput<'_, '_>) -> FrameGuard {
    if !enabled() {
        return FrameGuard(None);
    }
    CAPTURE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(c) = s.as_mut() else {
            return;
        };
        if let Some(t) =
            ticket.filter(|t| t.epoch == c.epoch && t.ordinal > 0 && t.ordinal <= c.calls.len())
        {
            let mode = match input {
                SignalInput::Signal(_) => "Signal",
                SignalInput::Tape(_) => "Tape",
                SignalInput::PreserveTape(_) => "PreserveTape",
                SignalInput::Mix(_) => "Mix",
            };
            c.calls[t.ordinal - 1]["entered_read_signal"] = json!(true);
            c.calls[t.ordinal - 1]["input"] = json!(mode);
        }
    });
    frame(
        ticket,
        if matches!(input, SignalInput::Mix(_)) {
            "invocation_mix"
        } else {
            "invocation"
        },
        "invocation",
    )
}
pub(super) fn consumer(ticket: Option<Ticket>, kind: Consumer) -> FrameGuard {
    frame(
        ticket,
        "consumer",
        match kind {
            Consumer::Preflight => "preflight",
            Consumer::Numerical => "numerical",
        },
    )
}
pub(super) fn phase(ticket: Option<Ticket>, phase: &'static str) -> FrameGuard {
    frame(ticket, "phase", phase)
}
pub(super) fn phase_index(
    ticket: Option<Ticket>,
    phase_name: &'static str,
    index: usize,
) -> FrameGuard {
    let guard = phase(ticket, phase_name);
    if let Some((epoch, id)) = guard.0 {
        CAPTURE.with(|slot| {
            if let Some(c) = slot.borrow_mut().as_mut().filter(|c| c.epoch == epoch) {
                if let Some(f) = c.stack.last_mut().filter(|f| f.id == id) {
                    f.index = Some(index);
                } else {
                    c.invalid = true;
                }
            }
        });
    }
    guard
}
pub(super) fn mark(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    payload: impl FnOnce() -> Value,
) {
    mark_optional(renderer, Some(control), ticket, phase, payload);
}
fn mark_optional(
    renderer: &StageAudio,
    control: Option<WorkControl<'_>>,
    ticket: Option<Ticket>,
    phase: &'static str,
    payload: impl FnOnce() -> Value,
) {
    if !enabled() {
        return;
    }
    let selected_phase = CAPTURE.with(|slot| {
        slot.borrow().as_ref().is_some_and(|c| {
            c.mode == CaptureMode::Ordinary
                || matches!(
                    phase,
                    "saved_mark"
                        | "saved_result"
                        | "restore_result"
                        | "progress_enter"
                        | "progress_report_before"
                        | "progress_report_after"
                        | "saved_status_before"
                        | "saved_status_after"
                        | "saved_key_disabled"
                        | "saved_key_scope_enter"
                        | "saved_key_scope_exit"
                )
        })
    });
    if !selected_phase {
        return;
    }
    let value = payload();
    let snapshot = snapshots(renderer, control);
    CAPTURE.with(|s| {
        if let Some(c) = s.borrow_mut().as_mut() {
            if let Some(t) =
                ticket.filter(|t| t.epoch == c.epoch && t.ordinal > 0 && t.ordinal <= c.calls.len())
            {
                c.calls[t.ordinal - 1]["last_phase"] = json!(phase);
                if value.get("status").and_then(Value::as_str) == Some("Err")
                    && c.calls[t.ordinal - 1].get("stop_phase").is_none()
                {
                    c.calls[t.ordinal - 1]["stop_phase"] = json!(phase);
                }
                if phase == "policy_limits" {
                    c.calls[t.ordinal - 1]["producer_entered"] = json!(true);
                }
            }
            push(c, phase, ticket, value, snapshot);
        }
    });
}
pub(super) fn result<T>(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    r: &Result<T, StageAudioError>,
) {
    caller_result(renderer, Some(control), ticket, phase, r);
}
pub(super) fn caller_result<T>(
    renderer: &StageAudio,
    control: Option<WorkControl<'_>>,
    ticket: Option<Ticket>,
    phase: &'static str,
    r: &Result<T, StageAudioError>,
) {
    mark_optional(renderer, control, ticket, phase, || match r {
        Ok(_) => json!({"status":"Ok","success_payload":"unexposed"}),
        Err(e) => json!({"status":"Err","error":error(e)}),
    });
}
pub(super) fn limits(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    r: Result<AudioQueryLimits, StageAudioError>,
) -> Result<AudioQueryLimits, StageAudioError> {
    mark(renderer, control, ticket, phase, || match &r {
        Ok(l) => {
            json!({"status":"Ok","maximum_spans":l.maximum_spans,"maximum_work":l.maximum_work})
        }
        Err(e) => json!({"status":"Err","error":error(e)}),
    });
    r
}
fn plan_error(e: &PlanError) -> Value {
    json!({"kind":"Plan","message":e.to_string(),
    "detail":format!("{e:?}"),"nested_fields_unexposed":true})
}
pub(super) fn graph_result(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    r: &Result<AudioSignalQuery<'_>, PlanError>,
) {
    mark(renderer, control, ticket, "graph_query", || match r {
        Ok(q) => json!({"status":"Ok","query":q,"work":q.work,
        "spans_len":q.spans.len(),"spans_storage":layout::<AudioSignalSpan<'_>>(q.spans.capacity()),
        "storage_scope":"outer spans Vec only; nested heap capacities unexposed"}),
        Err(e) => json!({"status":"Err","error":plan_error(e),"local_work":"unexposed"}),
    });
}
fn policy_value(p: &AudioPolicyQuery<SignalSample>) -> Value {
    json!({"suppressed":p.suppressed,
    "contents":p.contents,"lookup":p.lookup,"work":p.work,"suppressed_len":p.suppressed.len(),
    "suppressed_storage":layout::<Range<SignalSample>>(p.suppressed.capacity()),
    "contents_len":p.contents.len(),"contents_storage":layout::<AudioContent>(p.contents.capacity()),
    "storage_scope":"ordinary outer Vecs only; nested heap capacities unexposed"})
}
pub(super) fn policy_result(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    r: &Result<AudioPolicyQuery<SignalSample>, PlanError>,
) {
    mark(renderer, control, ticket, "policy_query", || match r {
        Ok(q) => json!({"status":"Ok","policy":policy_value(q)}),
        Err(e) => json!({"status":"Err","error":plan_error(e),"local_work":"unexposed"}),
    });
}
pub(super) fn policy(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    p: &SignalPolicyInput<'_>,
) {
    if !enabled() || ticket.is_none() {
        return;
    }
    if CAPTURE.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|c| c.mode == CaptureMode::KeysOnly)
    }) {
        return;
    }
    match p {
        SignalPolicyInput::Ordinary(q) => {
            mark(renderer, control, ticket, phase, || policy_value(q))
        }
        SignalPolicyInput::Admitted(_) => CAPTURE.with(|s| {
            if let Some(c) = s.borrow_mut().as_mut() {
                c.invalid = true;
            }
        }),
    }
}
fn pcm(samples: &[[f32; 2]]) -> Value {
    json!(
        samples
            .iter()
            .map(|s| [s[0].to_bits(), s[1].to_bits()])
            .collect::<Vec<_>>()
    )
}
fn mask_value(m: &SignalSuppressionMask) -> Value {
    json!({"ranges":m.as_slice(),"len":m.len(),
    "storage":layout::<Range<SignalSample>>(m.capacity())})
}
pub(super) fn mask(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    m: &SignalSuppressionMask,
) {
    mark(renderer, control, ticket, phase, || mask_value(m));
}
pub(super) fn pcm_value(samples: &[[f32; 2]]) -> Value {
    pcm(samples)
}
pub(super) fn block_result(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    r: &Result<SignalBlock, StageAudioError>,
) {
    mark(renderer, control, ticket, phase, || match r {
        Ok(b) => json!({"status":"Ok","pcm_bits":pcm(&b.samples),
        "dependencies":b.dependencies,"relative_depth":b.relative_depth,"mask":mask_value(&b.suppressed)}),
        Err(e) => json!({"status":"Err","error":error(e)}),
    });
}

pub(super) fn samples_result(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    r: &Result<Vec<[f32; 2]>, StageAudioError>,
) {
    mark(renderer, control, ticket, phase, || match r {
        Ok(v) => json!({"status":"Ok","pcm_bits":pcm(v)}),
        Err(e) => json!({"status":"Err","error":error(e)}),
    });
}
pub(super) fn preparation_result<T>(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    r: &Result<T, PreparationError>,
) {
    mark(renderer, control, ticket, phase, || match r {
        Ok(_) => json!({"status":"Ok","success_payload":"unexposed"}),
        Err(e) => json!({"status":"Err","error":{"kind":"Preparation","message":e.to_string(),
        "detail":format!("{e:?}"),"typed_payload":match e {
            PreparationError::InvalidRecipe(reason)=>json!({"variant":"InvalidRecipe","reason":reason}),
            PreparationError::SourceUnavailable(source)=>json!({"variant":"SourceUnavailable","source":source}),
            PreparationError::Cancelled=>json!({"variant":"Cancelled"}),
            PreparationError::UnsupportedLayout=>json!({"variant":"UnsupportedLayout"}),
            PreparationError::InvalidSamples=>json!({"variant":"InvalidSamples"}),
            PreparationError::IndexMismatch=>json!({"variant":"IndexMismatch"}),
            PreparationError::Time(_)=>json!({"variant":"Time","nested_fields_unexposed":true}),
            #[cfg(any(target_os="macos",target_os="linux"))]
            PreparationError::Session(_)=>json!({"variant":"Session","nested_fields_unexposed":true}),
        }}}),
    });
}
pub(super) fn saved_mark(renderer: &StageAudio, control: WorkControl<'_>, phase: SavedPhase) {
    mark(
        renderer,
        control,
        current_ticket(),
        "saved_mark",
        || json!({"phase":format!("{phase:?}")}),
    );
}
pub(super) fn activity_value(value: PreparationActivity) -> Value {
    activity(value)
}
pub(super) fn monitor(renderer: &StageAudio, phase: &'static str, payload: impl FnOnce() -> Value) {
    mark_optional(renderer, None, current_ticket(), phase, payload);
}
fn current_ticket() -> Option<Ticket> {
    CAPTURE.with(|s| s.borrow().as_ref().and_then(selected))
}
pub(super) fn saved_result(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    phase: SavedPhase,
    r: &Result<Option<stage_cache::SavedStage>, StageAudioError>,
) {
    if matches!(phase, SavedPhase::RawResult) {
        key_result(r);
    }
    mark(renderer, control, current_ticket(), "saved_result", || {
        json!({"phase":format!("{phase:?}"),
        "result":match r {Ok(v)=>json!({"status":"Ok","saved":v.as_ref().map(|s|json!({
            "key":s.key.0,"dependencies":s.dependencies,"relative_depth":s.relative_depth}))}),
        Err(e)=>json!({"status":"Err","error":error(e)})}})
    });
}
pub(super) fn restore_result(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    r: &Result<Option<PreparedBlock>, StageAudioError>,
) {
    mark(
        renderer,
        control,
        current_ticket(),
        "restore_result",
        || match r {
            Ok(v) => {
                json!({"status":"Ok","restored":v.as_ref().map(|b|json!({"resident_frames":b.samples.resident_frames(),
            "disk_bytes":b.samples.disk_bytes(),"dependencies":b.dependencies,"relative_depth":b.relative_depth}))})
            }
            Err(e) => json!({"status":"Err","error":error(e)}),
        },
    );
}
pub(super) fn context(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    phase: &'static str,
    payload: impl FnOnce() -> Value,
) {
    mark(renderer, control, current_ticket(), phase, payload);
}
fn work(control: WorkControl<'_>) -> Value {
    let w = control.work.borrow();
    json!({"prepared_stages":w.prepared_stages,"prepared_frames":w.prepared_frames,
        "source_checks":w.source_checks,"observed":w.observed,"preflight_assets":w.preflight_assets,
        "plan_work":w.plan_work,"cache_plan_work":w.cache_plan_work,"cache_key_scopes":w.cache_key_scopes,
        "billing_lane":if w.cache_key_scopes>0 {"Cache"} else {"Plan"},
        "projected_len":w.projected.len(),"projected_capacity":w.projected.capacity(),
        "projected_active_len":w.projected_active.len(),
        "projected_preflight_len":w.projected_preflight.len(),"intrinsic_preflight_len":w.intrinsic_preflight.len(),
        "projected_resident_frames":w.projected_resident_frames,
        "sound_context_frames":w.sound_context_frames,"projected_disk_bytes":w.projected_disk_bytes,
        "cancelled":control.cancelled.load(std::sync::atomic::Ordering::Relaxed)})
}
fn activity(a: PreparationActivity) -> Value {
    match a {
        PreparationActivity::CheckingPlan => json!({"kind":"CheckingPlan"}),
        PreparationActivity::LoadingSource => json!({"kind":"LoadingSource"}),
        PreparationActivity::DecodingSource => json!({"kind":"DecodingSource"}),
        PreparationActivity::PreparingOutput => json!({"kind":"PreparingOutput"}),
        PreparationActivity::SavingAudioCache => json!({"kind":"SavingAudioCache"}),
        PreparationActivity::CheckingSavedAudio { completed, total } => {
            json!({"kind":"CheckingSavedAudio","completed":completed,"total":total})
        }
        PreparationActivity::RestoringSavedAudio { completed, total } => {
            json!({"kind":"RestoringSavedAudio","completed":completed,"total":total})
        }
        PreparationActivity::CheckingStretch {
            depth,
            completed,
            total,
        } => json!({"kind":"CheckingStretch","depth":depth,"completed":completed,"total":total}),
        PreparationActivity::Stretching {
            depth,
            completed,
            total,
        } => json!({"kind":"Stretching","depth":depth,"completed":completed,"total":total}),
        PreparationActivity::CheckingStretchInput {
            depth,
            completed,
            total,
        } => {
            json!({"kind":"CheckingStretchInput","depth":depth,"completed":completed,"total":total})
        }
        PreparationActivity::SavingStretch { depth } => {
            json!({"kind":"SavingStretch","depth":depth})
        }
    }
}
fn saved_event(e: crate::SavedAudioEvent) -> &'static str {
    use crate::SavedAudioEvent::*;
    match e {
        Missing => "Missing",
        Restored => "Restored",
        Saved => "Saved",
        Unavailable => "Unavailable",
        Damaged => "Damaged",
        Busy => "Busy",
        Full => "Full",
        BudgetSkipped => "BudgetSkipped",
    }
}
fn snapshots(renderer: &StageAudio, control: Option<WorkControl<'_>>) -> Value {
    let l = renderer.limits;
    json!({"work":control.map(work),"controller":{
        "active_frames":renderer.active_frames,"active_disk_bytes":renderer.active_disk_bytes,
        "cached_resident_frames":renderer.cached_resident_frames(),"cached_disk_bytes":renderer.cached_disk_bytes(),
        "cache_len":renderer.cache.len(),"cache_capacity":renderer.cache.capacity(),
        "original_inputs_len":renderer.original_inputs.len(),"owned_suppression_live_frames":renderer.owned_suppression.live_frames(),
        "has_persistent_cache":renderer.persistent_cache.is_some(),
        "limits":{"maximum_input_frames":l.maximum_input_frames,"maximum_output_frames":l.maximum_output_frames,
            "maximum_resident_frames":l.maximum_resident_frames,"maximum_cached_stages":l.maximum_cached_stages,
            "maximum_depth":l.maximum_depth,"maximum_prepared_stages":l.maximum_prepared_stages,
            "maximum_prepared_frames":l.maximum_prepared_frames,"maximum_memory_stage_frames":l.maximum_memory_stage_frames,
            "maximum_disk_bytes":l.maximum_disk_bytes}},
        "progress":renderer.preparation_monitor.as_ref().map(|m|json!({"activity":activity(m.activity()),
            "saved_audio":m.saved_audio_status().map(|s|json!({"key":s.key,"bytes":s.bytes,
                "event":saved_event(s.event),"regenerated_from":s.regenerated_from.map(saved_event)}))}))})
}
pub(super) fn error(e: &StageAudioError) -> Value {
    let kind = match e {
        StageAudioError::Range => "Range",
        StageAudioError::PreparationRange => "PreparationRange",
        StageAudioError::ForeignDomain => "ForeignDomain",
        StageAudioError::ForeignDefinition => "ForeignDefinition",
        StageAudioError::InvalidLimits => "InvalidLimits",
        StageAudioError::Limit(_) => "Limit",
        StageAudioError::Timeout => "Timeout",
        StageAudioError::Unsupported(_) => "Unsupported",
        StageAudioError::Preparation(_) => "Preparation",
        StageAudioError::Plan(_) => "Plan",
        StageAudioError::Time(_) => "Time",
        StageAudioError::Gain(_) => "Gain",
        StageAudioError::Dsp(_) => "Dsp",
        StageAudioError::Filter(_) => "Filter",
        StageAudioError::Wet(_) => "Wet",
        StageAudioError::Transfer(_) => "Transfer",
        StageAudioError::Cache(_) => "Cache",
    };
    let mut result = json!({"kind":kind,"message":e.to_string(),"detail":format!("{e:?}"),"cancelled":e.is_cancelled()});
    result["typed_payload"] = match e {
        StageAudioError::Range
        | StageAudioError::PreparationRange
        | StageAudioError::ForeignDomain
        | StageAudioError::ForeignDefinition
        | StageAudioError::InvalidLimits
        | StageAudioError::Timeout => json!({"shape":"unit"}),
        StageAudioError::Limit(reason) | StageAudioError::Unsupported(reason) => {
            json!({"reason":reason})
        }
        StageAudioError::Preparation(value) => match value {
            PreparationError::InvalidRecipe(reason) => {
                json!({"variant":"InvalidRecipe","reason":reason})
            }
            PreparationError::UnsupportedLayout => {
                json!({"variant":"UnsupportedLayout","shape":"unit"})
            }
            PreparationError::InvalidSamples => json!({"variant":"InvalidSamples","shape":"unit"}),
            PreparationError::Cancelled => json!({"variant":"Cancelled","shape":"unit"}),
            PreparationError::IndexMismatch => json!({"variant":"IndexMismatch","shape":"unit"}),
            PreparationError::SourceUnavailable(source) => {
                json!({"variant":"SourceUnavailable","source":source})
            }
            PreparationError::Time(_) => json!({"variant":"Time","nested_fields_unexposed":true}),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            PreparationError::Session(_) => {
                json!({"variant":"Session","nested_fields_unexposed":true})
            }
        },
        StageAudioError::Cache(value) => json!({"error_kind":format!("{:?}",value.kind()),
            "raw_os_error":value.raw_os_error(),"display":value.to_string()}),
        StageAudioError::Plan(_)
        | StageAudioError::Time(_)
        | StageAudioError::Gain(_)
        | StageAudioError::Dsp(_)
        | StageAudioError::Filter(_)
        | StageAudioError::Wet(_)
        | StageAudioError::Transfer(_) => json!({"nested_fields_unexposed":true}),
    };
    result["error_schema_scope"] = json!(
        "exact outer kind and selected direct payloads; original Debug/display supplemental, no complete recursive typed claim"
    );
    result
}
fn layout<T>(capacity: usize) -> Value {
    match Layout::array::<T>(capacity) {
        Ok(l) => json!({"capacity":capacity,"bytes":l.size(),"align":l.align()}),
        Err(_) => json!({"capacity":capacity,"layout":"overflow"}),
    }
}
// A stream is the actual scan's hasher, joined to caller/build scopes without
// retaining that hasher or assigning an allocation identity to it.
pub(super) struct KeyStreamGuard(Option<(u64, usize)>);
impl Drop for KeyStreamGuard {
    fn drop(&mut self) {
        let Some((epoch, ordinal)) = self.0 else {
            return;
        };
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(c) = slot.as_mut().filter(|c| c.epoch == epoch) else {
                return;
            };
            if c.key_stack.pop() != Some(ordinal) {
                c.invalid = true;
                c.key_failed = true;
                return;
            }
            if c.key_result_pending.replace(ordinal).is_some() {
                c.invalid = true;
                c.key_failed = true;
            }
            c.key_streams[ordinal - 1].exited = true;
        });
    }
}
fn key_failure(c: &mut Capture) {
    c.invalid = true;
    c.key_failed = true;
    c.dropped += 1;
}
pub(super) fn key_stream() -> KeyStreamGuard {
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else {
            return KeyStreamGuard(None);
        };
        if c.key_failed {
            return KeyStreamGuard(None);
        }
        if c.key_streams.len() == MAX_KEY_STREAMS || c.key_stack.len() == MAX_ACTIVE {
            key_failure(c);
            return KeyStreamGuard(None);
        }
        let ordinal = c.key_streams.len() + 1;
        let stream = KeyStream {
            ordinal,
            parent: c.key_stack.last().copied(),
            entry_position: position_value(c),
            updates: Vec::new(),
            finish: None,
            result: None,
            exited: false,
        };
        c.key_streams.push(stream);
        c.key_stack.push(ordinal);
        KeyStreamGuard(Some((c.epoch, ordinal)))
    })
}
pub(super) fn key_update(site: &'static str, bytes: &[u8]) {
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else {
            return;
        };
        if c.key_failed {
            return;
        }
        let Some(ordinal) = c.key_stack.last().copied() else {
            key_failure(c);
            return;
        };
        if c.key_updates == MAX_KEY_UPDATES || bytes.len() > MAX_KEY_BYTES - c.key_bytes {
            key_failure(c);
            return;
        }
        let stream = &mut c.key_streams[ordinal - 1];
        if stream.finish.is_some() {
            key_failure(c);
            return;
        }
        c.key_updates += 1;
        c.key_bytes += bytes.len();
        stream.updates.push(json!({"ordinal":stream.updates.len()+1,
            "capture_ordinal":c.key_updates,"site":site,"bytes":bytes}));
    });
}
pub(super) fn key_finish(actual: [u8; 32]) {
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else {
            return;
        };
        if c.key_failed {
            return;
        }
        let Some(ordinal) = c.key_stack.last().copied() else {
            key_failure(c);
            return;
        };
        let stream = &mut c.key_streams[ordinal - 1];
        if stream.finish.replace(actual).is_some() {
            key_failure(c);
        }
    });
}

// saved_stage_key observes this raw Result immediately after the scan scope
// exits, before dropping the cache billing guard or mapping optional failures.
fn key_result(actual: &Result<Option<stage_cache::SavedStage>, StageAudioError>) {
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else {
            return;
        };
        if c.key_failed {
            return;
        }
        let Some(ordinal) = c.key_result_pending.take() else {
            key_failure(c);
            return;
        };
        let result = match actual {
            Ok(saved) => json!({"status":"Ok","saved":saved.as_ref().map(|s|json!({
                "key":s.key.0,"dependencies":s.dependencies,"relative_depth":s.relative_depth}))}),
            Err(e) => json!({"status":"Err","error":error(e)}),
        };
        let stream = &mut c.key_streams[ordinal - 1];
        if stream.result.replace(result).is_some() {
            key_failure(c);
        }
    });
}
