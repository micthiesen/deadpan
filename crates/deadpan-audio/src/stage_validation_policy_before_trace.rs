//! Default-off ordinary stage-validation witness. No admission or billing authority.
use super::*;
use serde_json::{Value, json};
use std::alloc::Layout;

const MAX_ROWS: usize = 32768;
const MAX_ACTIVE: usize = 128;
// Witness storage bounds are separate from native admission and render limits.
const MAX_KEY_STREAMS: usize = 256;
const MAX_KEY_UPDATES: usize = 262144;
const MAX_KEY_BYTES: usize = 16 * 1024 * 1024;
struct KeyStream {
    ordinal: usize,
    parent: Option<usize>,
    entry_position: Value,
    updates: Vec<Value>,
    finish: Option<[u8; 32]>,
    result: Option<Value>,
    exited: bool,
}
struct Capture {
    epoch: u64,
    attempted: usize,
    dropped: usize,
    invalid: bool,
    next_scope: usize,
    stack: Vec<Value>,
    rows: Vec<Value>,
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
        let next = value.get().checked_add(1).expect("trace epoch exhausted");
        value.set(next);
        next
    });
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(slot.is_none(), "stage validation trace already active");
        *slot = Some(Capture {
            epoch,
            attempted: 0,
            dropped: 0,
            invalid: false,
            next_scope: 0,
            stack: Vec::new(),
            rows: Vec::new(),
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
fn position_value(c: &Capture) -> Value {
    json!({"last_attempted_ordinal":c.attempted,"active_stack":c.stack,
        "active_key_streams":c.key_stack})
}
pub(super) fn position() -> Option<Value> {
    CAPTURE.with(|slot| slot.borrow().as_ref().map(position_value))
}
pub(super) fn take() -> Value {
    CAPTURE.with(|slot| {
        slot.borrow_mut().take().map_or(Value::Null, |mut c| {
            if !c.key_stack.is_empty() || c.key_result_pending.is_some() {
                c.invalid = true;
            }
            json!({"schema":"stage-validation-before-v1","attempted":c.attempted,
                "dropped":c.dropped,"invalid":c.invalid,"active_at_take":c.stack,"rows":c.rows,
                "key_transcript_schema":"stage-pcm-hash-updates-v1",
                "key_updates":c.key_updates,"key_bytes":c.key_bytes,
                "key_failed":c.key_failed,"active_key_streams_at_take":c.key_stack,
                "key_result_pending_at_take":c.key_result_pending,
                "key_transcripts":c.key_streams.iter().map(|s|json!({
                    "ordinal":s.ordinal,"parent":s.parent,"entry_position":s.entry_position,
                    "updates":s.updates,"actual_finish":s.finish,"actual_scan_result":s.result,"exited":s.exited,
                    "termination":if s.finish.is_some() {"finalized"} else {"prefix_only"}
                })).collect::<Vec<_>>()})
        })
    })
}
fn push(c: &mut Capture, phase: &str, value: Value, snapshots: Value) {
    c.attempted += 1;
    if c.rows.len() == MAX_ROWS {
        c.dropped += 1;
        return;
    }
    let mut row = json!({"ordinal":c.attempted,"phase":phase,
        "position":position_value(c),"value":value});
    if let Value::Object(fields) = snapshots {
        row.as_object_mut().unwrap().extend(fields);
    }
    c.rows.push(row);
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
fn snapshots(renderer: &StageAudio, control: WorkControl<'_>) -> Value {
    let l = renderer.limits;
    json!({"work":work(control),"controller":{
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
pub(super) fn mark(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    phase: &'static str,
    value: impl FnOnce() -> Value,
) {
    if position().is_none() {
        return;
    }
    let value = value();
    let snapshots = snapshots(renderer, control);
    CAPTURE.with(|slot| {
        if let Some(c) = slot.borrow_mut().as_mut() {
            if let Some(top) = c.stack.last_mut() {
                top["phase"] = json!(phase);
            }
            push(c, phase, value, snapshots);
        }
    });
}
pub(super) struct Scope<'a> {
    id: Option<(u64, usize)>,
    control: WorkControl<'a>,
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        let Some((epoch, id)) = self.id else {
            return;
        };
        let work = work(self.control);
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(c) = slot.as_mut().filter(|c| c.epoch == epoch) else {
                return;
            };
            let frame = c.stack.pop();
            if frame.as_ref().and_then(|f| f["ordinal"].as_u64()) != Some(id as u64) {
                c.invalid = true;
            }
            push(
                c,
                "scope_exit",
                json!({"frame":frame,
                "meaning":"lexical exit only; no ordinary allocation/free or native lease event"}),
                json!({"work":work}),
            );
        });
    }
}
pub(super) fn enter<'a>(
    renderer: &StageAudio,
    control: WorkControl<'a>,
    kind: &'static str,
    value: impl FnOnce() -> Value,
) -> Scope<'a> {
    if position().is_none() {
        return Scope { id: None, control };
    }
    let value = value();
    let snapshots = snapshots(renderer, control);
    let id = CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let c = slot.as_mut()?;
        if c.stack.len() == MAX_ACTIVE {
            c.invalid = true;
            return None;
        }
        c.next_scope += 1;
        let id = c.next_scope;
        let parent = c.stack.last().map(|f| f["ordinal"].clone());
        c.stack.push(
            json!({"ordinal":id,"parent":parent,"kind":kind,"phase":"scope_enter","context":value}),
        );
        push(c, "scope_enter", json!({"kind":kind}), snapshots);
        Some((c.epoch, id))
    });
    Scope { id, control }
}
pub(super) fn mode(input: SignalInput<'_, '_>) -> &'static str {
    match input {
        SignalInput::Signal(_) => "Signal",
        SignalInput::Tape(_) => "Tape",
        SignalInput::PreserveTape(_) => "PreserveTape",
        SignalInput::Mix(_) => "Mix",
    }
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
pub(super) fn result<T>(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    phase: &'static str,
    r: &Result<T, StageAudioError>,
) {
    mark(renderer, control, phase, || match r {
        Ok(_) => json!({"status":"Ok","success_payload":"unexposed"}),
        Err(e) => json!({"status":"Err","error":error(e)}),
    });
}
pub(super) fn step<T>(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    phase: &'static str,
    r: Result<T, StageAudioError>,
) -> Result<T, StageAudioError> {
    result(renderer, control, phase, &r);
    r
}
fn layout<T>(capacity: usize) -> Value {
    match Layout::array::<T>(capacity) {
        Ok(l) => json!({"capacity":capacity,"bytes":l.size(),"align":l.align()}),
        Err(_) => json!({"capacity":capacity,"layout":"overflow"}),
    }
}
pub(super) fn policy(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    phase: &'static str,
    policy: &SignalPolicyInput<'_>,
    completed: usize,
) {
    match policy {
        SignalPolicyInput::Ordinary(query) => {
            ordinary_policy(renderer, control, phase, query, completed);
        }
        SignalPolicyInput::Admitted(query) => mark(renderer, control, phase, || {
            let view = query.view();
            json!({"suppressed":view.suppressed,"contents":view.contents,
                "lookup":view.lookup,"work":view.work,
                "suppressed_len":view.suppressed.len(),"contents_len":view.contents.len(),
                "completed_contents":completed,
                "storage_scope":"admitted borrowed slices; owning capacities unexposed",
                "ownership":"admitted policy owner retained through contents and progress",
                "admission":{"canonical":query.canonical_units(),
                    "additional":query.additional_units(),"total":query.work(),
                    "forwarded":query.was_forwarded()}})
        }),
    }
}
fn ordinary_policy(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    phase: &'static str,
    p: &AudioPolicyQuery<SignalSample>,
    completed: usize,
) {
    mark(renderer, control, phase, || {
        json!({"suppressed":p.suppressed,"contents":p.contents,"lookup":p.lookup,"work":p.work,
        "suppressed_len":p.suppressed.len(),"suppressed_storage":layout::<Range<SignalSample>>(p.suppressed.capacity()),
        "contents_len":p.contents.len(),"contents_storage":layout::<AudioContent>(p.contents.capacity()),
        "completed_contents":completed,"storage_scope":"outer vectors only; nested heap capacities unexposed",
        "ownership":"ordinary unadmitted policy; disposal established lexically"})
    });
}
pub(super) fn reserved(renderer: &StageAudio, control: WorkControl<'_>, r: &StreamReservation) {
    mark(renderer, control, "stream_reserved", || {
        json!({"resident_frames":r.resident_frames,"disk_bytes":r.disk_bytes,
        "origin":"actual returned StreamReservation fields, linked by caller scope"})
    });
}
pub(super) fn block_result(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    phase: &'static str,
    r: &Result<PreparedBlock, StageAudioError>,
) {
    mark(renderer, control, phase, || match r {
        Ok(b) => {
            json!({"status":"Ok","resident_frames":b.samples.resident_frames(),"disk_bytes":b.samples.disk_bytes(),
            "dependencies":b.dependencies,"relative_depth":b.relative_depth})
        }
        Err(e) => json!({"status":"Err","error":error(e)}),
    });
}
pub(super) fn saved_result(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    phase: &'static str,
    r: &Result<Option<stage_cache::SavedStage>, StageAudioError>,
) {
    if phase == "saved_key_raw_result" {
        key_result(r);
    }
    mark(renderer, control, phase, || match r {
        Ok(v) => {
            json!({"status":"Ok","saved":v.as_ref().map(|s|json!({"key":s.key.0,"dependencies":s.dependencies,"relative_depth":s.relative_depth}))})
        }
        Err(e) => json!({"status":"Err","error":error(e)}),
    });
}
pub(super) fn restore_result(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    r: &Result<Option<PreparedBlock>, StageAudioError>,
) {
    mark(renderer, control, "restore_return", || match r {
        Ok(v) => {
            json!({"status":"Ok","restored":v.as_ref().map(|b|json!({"resident_frames":b.samples.resident_frames(),
            "disk_bytes":b.samples.disk_bytes(),"dependencies":b.dependencies,"relative_depth":b.relative_depth}))})
        }
        Err(e) => json!({"status":"Err","error":error(e)}),
    });
}

pub(super) fn limits_result(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    result: Result<AudioQueryLimits, StageAudioError>,
) -> Result<AudioQueryLimits, StageAudioError> {
    mark(renderer, control, "query_limits_result", || match &result {
        Ok(limits) => json!({"status":"Ok","maximum_spans":limits.maximum_spans,
            "maximum_work":limits.maximum_work}),
        Err(error_value) => json!({"status":"Err","error":error(error_value)}),
    });
    result
}

// A stream is the actual scan's hasher, joined to caller/build scopes without
// retaining that hasher or assigning an allocation identity to it.
pub(super) struct KeyScope(Option<(u64, usize)>);
impl Drop for KeyScope {
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
pub(super) fn key_stream() -> KeyScope {
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else {
            return KeyScope(None);
        };
        if c.key_failed {
            return KeyScope(None);
        }
        if c.key_streams.len() == MAX_KEY_STREAMS || c.key_stack.len() == MAX_ACTIVE {
            key_failure(c);
            return KeyScope(None);
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
        KeyScope(Some((c.epoch, ordinal)))
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
