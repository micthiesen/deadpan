//! Default-off ordinary post-DSP observations. Test JSON is outside request admission.
use super::*;
use serde_json::{Value, json};
use std::alloc::Layout;

const MAX_ROWS: usize = 32768;
const MAX_CALLS: usize = 4096;
const MAX_ACTIVE: usize = 128;
const MAX_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(super) enum Caller {
    BuildStageBlock,
    SavedKeyOutputBits,
    RoutedLegacy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Ticket {
    epoch: u64,
    ordinal: usize,
}
pub(super) struct HelperContext<'a> {
    pub(super) caller: Caller,
    pub(super) renderer: &'a StageAudio,
    pub(super) ticket: Option<Ticket>,
}
struct Frame {
    id: usize,
    ticket: Option<Ticket>,
    kind: &'static str,
    phase: &'static str,
    index: Option<usize>,
    caller: Option<Caller>,
}
struct Capture {
    epoch: u64,
    attempted: usize,
    dropped: usize,
    invalid: bool,
    overflow: bool,
    next_frame: usize,
    calls: Vec<Value>,
    rows: Vec<Value>,
    stack: Vec<Frame>,
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
        let next = value.get().checked_add(1).expect("O trace epoch exhausted");
        value.set(next);
        next
    });
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(slot.is_none(), "stage block policy trace already active");
        *slot = Some(Capture {
            epoch,
            attempted: 0,
            dropped: 0,
            invalid: false,
            overflow: false,
            next_frame: 0,
            calls: Vec::new(),
            rows: Vec::new(),
            stack: Vec::new(),
            payload_bytes: 0,
        });
    });
    CaptureGuard(epoch)
}
fn enabled() -> bool {
    CAPTURE.with(|slot| slot.borrow().as_ref().is_some_and(|c| !c.invalid))
}
fn selected(c: &Capture) -> Option<Ticket> {
    c.stack
        .iter()
        .rev()
        .find(|frame| frame.kind == "caller")
        .and_then(|frame| {
            frame
                .ticket
                .filter(|_| frame.caller == Some(Caller::BuildStageBlock))
        })
}
fn position_value(c: &Capture) -> Value {
    json!({"schema":"stage-block-policy-before-position-v1",
        "last_attempted_ordinal":c.attempted,"selected_ticket":selected(c).map(|t| t.ordinal),
        "active_stack":c.stack.iter().map(|f| json!({
            "frame":f.id,"ticket":f.ticket.map(|t|t.ordinal),"kind":f.kind,
            "phase":f.phase,"index":f.index,"caller":f.caller
        })).collect::<Vec<_>>()})
}
pub(super) fn position() -> Option<Value> {
    CAPTURE.with(|slot| slot.borrow().as_ref().map(position_value))
}
pub(super) fn take() -> Value {
    CAPTURE.with(|slot| slot.borrow_mut().take().map_or(Value::Null, |mut c| {
        if !c.stack.is_empty() { c.invalid = true; }
        let selected_calls = c.calls.iter().filter(|v| v["selected"] == true).count();
        let mut value = json!({
            "schema":"stage-block-policy-before-v1",
            "attempted":c.attempted,"dropped":c.dropped,"invalid":c.invalid,"overflow":c.overflow,
            "selected_calls":selected_calls,"active_at_take":position_value(&c)["active_stack"],
            "calls":c.calls,"rows":c.rows,"payload_bytes":c.payload_bytes,
            "bounds":{"rows":MAX_ROWS,"calls":MAX_CALLS,"active_frames":MAX_ACTIVE,
                "serialized_payload_bytes":MAX_PAYLOAD_BYTES},
            "observer_storage":"post-construction serialized retained calls/rows accounting and final document check; not physical RSS; snapshots/JSON temporaries outside request ledger",
            "ordinary_ownership":"unadmitted source Vec and inventory; no O source lease, Admission callback, output account or handoff",
            "layout_scope":"capacity-derived Layout observations after actual construction, not allocator requests",
            "query_capacity_scope":"ordinary outer Vec capacities only; nested heaps unexposed"
        });
        if serde_json::to_vec(&value).expect("O trace JSON serializes").len() > MAX_PAYLOAD_BYTES {
            value["invalid"] = json!(true);
            value["overflow"] = json!(true);
            value["dropped"] = json!(c.dropped.saturating_add(1));
        }
        value
    }))
}
fn overflow(c: &mut Capture) {
    c.invalid = true;
    c.overflow = true;
    c.dropped = c.dropped.saturating_add(1);
}
fn account(c: &mut Capture, value: &Value) -> bool {
    let bytes = serde_json::to_vec(value)
        .expect("O trace JSON serializes")
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
fn valid(c: &Capture, ticket: Ticket) -> bool {
    ticket.epoch == c.epoch && ticket.ordinal > 0 && ticket.ordinal <= c.calls.len()
}
pub(super) struct FrameGuard(Option<(u64, usize)>);
impl Drop for FrameGuard {
    fn drop(&mut self) {
        let Some((epoch, id)) = self.0 else { return };
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(c) = slot.as_mut().filter(|c| c.epoch == epoch) else {
                return;
            };
            if c.stack.last().map(|f| f.id) != Some(id) {
                c.invalid = true;
                return;
            }
            c.stack.pop();
        });
    }
}
fn frame(
    ticket: Option<Ticket>,
    kind: &'static str,
    phase: &'static str,
    index: Option<usize>,
    caller: Option<Caller>,
) -> FrameGuard {
    FrameGuard(CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let c = slot.as_mut()?;
        if c.invalid {
            return None;
        }
        if ticket.is_some_and(|t| !valid(c, t)) {
            c.invalid = true;
            return None;
        }
        if c.stack.len() >= MAX_ACTIVE {
            overflow(c);
            return None;
        }
        let Some(id) = c.next_frame.checked_add(1) else {
            overflow(c);
            return None;
        };
        c.next_frame = id;
        c.stack.push(Frame {
            id,
            ticket,
            kind,
            phase,
            index,
            caller,
        });
        Some((c.epoch, id))
    }))
}
pub(super) fn phase(
    ticket: Option<Ticket>,
    phase: &'static str,
    index: Option<usize>,
) -> FrameGuard {
    let caller = CAPTURE.with(|slot| {
        slot.borrow().as_ref().and_then(|c| {
            c.stack
                .iter()
                .rev()
                .find(|f| f.ticket == ticket && ticket.is_some())
                .and_then(|f| f.caller)
        })
    });
    frame(ticket, "operation", phase, index, caller)
}
pub(super) struct CallScope {
    pub(super) ticket: Option<Ticket>,
    _frame: FrameGuard,
}
pub(super) fn call(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    caller: Caller,
    context: impl FnOnce() -> Value,
) -> CallScope {
    if !enabled() {
        return CallScope {
            ticket: None,
            _frame: FrameGuard(None),
        };
    }
    let context = context();
    let ticket = CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let c = slot.as_mut()?;
        if c.calls.len() >= MAX_CALLS {
            overflow(c);
            return None;
        }
        let ticket = Ticket { epoch:c.epoch,ordinal:c.calls.len()+1 };
        let row = json!({"ordinal":ticket.ordinal,"parent":c.stack.iter().rev().find_map(|f|f.ticket).map(|t|t.ordinal),
            "caller":caller,"selected":caller == Caller::BuildStageBlock,"context":context,"helper_result":Value::Null});
        if !account(c,&row) { return None; }
        c.calls.push(row);
        Some(ticket)
    });
    let scope = CallScope {
        ticket,
        _frame: frame(ticket, "caller", "caller", None, Some(caller)),
    };
    mark(renderer, control, ticket, "caller_enter", || json!({}));
    scope
}
pub(super) fn context_scope(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    kind: &'static str,
    context: impl FnOnce() -> Value,
) -> FrameGuard {
    if !enabled() {
        return FrameGuard(None);
    }
    let scope = frame(None, kind, kind, None, None);
    mark(renderer, control, None, "context_enter", context);
    scope
}
pub(super) fn invocation(ticket: Option<Ticket>, caller: Caller) -> FrameGuard {
    frame(
        ticket,
        "ordinary_query",
        "ordinary_query",
        None,
        Some(caller),
    )
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
pub(super) fn controller_mark(
    renderer: &StageAudio,
    phase: &'static str,
    payload: impl FnOnce() -> Value,
) {
    mark_optional(renderer, None, None, phase, payload);
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
    let value = payload();
    let snapshot = snapshots(renderer, control);
    let n_before_position = read_signal_policy_before_trace::position();
    let n_native_position = read_signal_policy_native_trace::position();
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        let Some(ordinal) = c.attempted.checked_add(1) else {
            overflow(c);
            return;
        };
        c.attempted = ordinal;
        if c.rows.len() >= MAX_ROWS {
            overflow(c);
            return;
        }
        if ticket.is_some_and(|t| !valid(c, t)) {
            c.invalid = true;
            return;
        }
        let row = json!({"ordinal":ordinal,"ticket":ticket.map(|t|t.ordinal),"phase":phase,
            "position":position_value(c),"value":value,"snapshot":snapshot,
            "n_before_position":n_before_position,"n_native_position":n_native_position});
        if account(c, &row) {
            c.rows.push(row);
        }
    });
}
pub(super) fn result<T>(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    value: &Result<T, StageAudioError>,
) {
    if !enabled() {
        return;
    }
    let result = result_value(value);
    if phase == "helper_result" {
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(c) = slot.as_mut() else { return };
            let Some(ticket) = ticket.filter(|t| valid(c, *t)) else {
                return;
            };
            let mut replacement = c.calls[ticket.ordinal - 1].clone();
            replacement["helper_result"] = result.clone();
            let previous_bytes = serde_json::to_vec(&c.calls[ticket.ordinal - 1])
                .expect("O call serializes")
                .len();
            c.payload_bytes -= previous_bytes;
            if account(c, &replacement) {
                c.calls[ticket.ordinal - 1] = replacement;
            } else {
                c.payload_bytes += previous_bytes;
            }
        });
    }
    mark(renderer, control, ticket, phase, || result);
}
pub(super) fn result_value<T>(value: &Result<T, StageAudioError>) -> Value {
    match value {
        Ok(_) => json!({"status":"Ok"}),
        Err(value) => json!({"status":"Err","error":error(value)}),
    }
}
pub(super) fn error(value: &StageAudioError) -> Value {
    let mut result = read_signal_policy_before_trace::error(value);
    match value {
        StageAudioError::Plan(value) => result["typed_payload"] = plan_error(value),
        StageAudioError::Time(value) => result["typed_payload"] = time_error(value),
        _ => {}
    }
    result
}
fn time_error(value: &TimeError) -> Value {
    let variant = match value {
        TimeError::InvalidRatio => "InvalidRatio",
        TimeError::InvalidFrameRate => "InvalidFrameRate",
        TimeError::InvalidSourceTimeBase => "InvalidSourceTimeBase",
        TimeError::NegativeDuration => "NegativeDuration",
        TimeError::ReversedRange => "ReversedRange",
        TimeError::ZeroPlays => "ZeroPlays",
        TimeError::EmptyRepeatChild => "EmptyRepeatChild",
        TimeError::Overflow => "Overflow",
    };
    json!({"variant":variant,"shape":"unit"})
}
pub(super) fn plan_error(error: &PlanError) -> Value {
    let typed_payload = match error {
        PlanError::AudioMetadataIssuerMismatch => json!({"variant":"AudioMetadataIssuerMismatch"}),
        PlanError::AudioSoundContextShapeChanged => {
            json!({"variant":"AudioSoundContextShapeChanged"})
        }
        PlanError::UnsupportedAudioBindings => json!({"variant":"UnsupportedAudioBindings"}),
        PlanError::AudioOnlyContext => json!({"variant":"AudioOnlyContext"}),
        PlanError::Document(_) => {
            json!({"variant":"Document","nested_typed_fields_unexposed":true})
        }
        PlanError::Time(value) => json!({"variant":"Time","error":time_error(value)}),
        PlanError::Framing(_) => json!({"variant":"Framing","nested_typed_fields_unexposed":true}),
        PlanError::FrameOutOfRange { frame, duration } => {
            json!({"variant":"FrameOutOfRange","frame":frame,"duration":duration})
        }
        PlanError::InvalidPictureDefinition(value) => {
            json!({"variant":"InvalidPictureDefinition","node":value})
        }
        PlanError::DefinitionPictureOutOfRange {
            definition,
            position,
            duration,
        } => json!({"variant":"DefinitionPictureOutOfRange",
            "definition":definition,"position":position,"duration":duration}),
        PlanError::InvalidScopedHold(reason) => {
            json!({"variant":"InvalidScopedHold","reason":reason})
        }
        PlanError::InvalidPictureOccurrence(reason) => {
            json!({"variant":"InvalidPictureOccurrence","reason":reason})
        }
        PlanError::HoldContextUnavailable(_) => {
            json!({"variant":"HoldContextUnavailable","nested_typed_fields_unexposed":true})
        }
        PlanError::PictureQueryLimit(reason) => {
            json!({"variant":"PictureQueryLimit","reason":reason})
        }
        PlanError::NoSourceFrame => json!({"variant":"NoSourceFrame"}),
        PlanError::AudioRangeOutOfRange => json!({"variant":"AudioRangeOutOfRange"}),
        PlanError::InvalidAudioLimits => json!({"variant":"InvalidAudioLimits"}),
        PlanError::InvalidAudioDefinitionSelector(value) => {
            json!({"variant":"InvalidAudioDefinitionSelector","selector":value})
        }
        PlanError::InvalidAudioSourceOccurrence(reason) => {
            json!({"variant":"InvalidAudioSourceOccurrence","reason":reason})
        }
        PlanError::InvalidAudioRootPlacement(reason) => {
            json!({"variant":"InvalidAudioRootPlacement","reason":reason})
        }
        PlanError::AudioQueryLimit(reason) => json!({"variant":"AudioQueryLimit","reason":reason}),
        PlanError::NoSourceAudio => json!({"variant":"NoSourceAudio"}),
        PlanError::IndexAssetMismatch { expected, actual } => {
            json!({"variant":"IndexAssetMismatch","expected":expected,"actual":actual})
        }
        PlanError::IndexClockMismatch { expected, actual } => {
            json!({"variant":"IndexClockMismatch","expected":expected,"actual":actual})
        }
        PlanError::MissingSourceFrame { frame } => {
            json!({"variant":"MissingSourceFrame","frame":frame})
        }
        PlanError::InvalidPlan(reason) => json!({"variant":"InvalidPlan","reason":reason}),
    };
    json!({"kind":"Plan","typed_payload":typed_payload,"detail":format!("{error:?}"),"message":error.to_string(),
        "error_schema_scope":"exact Plan outer variants and direct payloads; nested Document/Framing/Hold context fields explicitly unexposed"})
}
pub(super) fn policy_value(policy: &AudioPolicyQuery<SignalSample>) -> Value {
    json!({"suppressed":policy.suppressed,"suppressed_len":policy.suppressed.len(),
        "suppressed_capacity":policy.suppressed.capacity(),
        "suppressed_storage":layout::<Range<SignalSample>>(policy.suppressed.capacity()),
        "contents":policy.contents,"contents_len":policy.contents.len(),
        "contents_capacity":policy.contents.capacity(),
        "contents_storage":layout::<AudioContent>(policy.contents.capacity()),
        "lookup":policy.lookup,"work":policy.work,
        "source_ownership":"ordinary unadmitted Vec and complete inventory; no source lease exists"})
}
pub(super) fn pcm_value(samples: &[[f32; 2]]) -> Value {
    json!(
        samples
            .iter()
            .map(|frame| frame.map(f32::to_bits))
            .collect::<Vec<_>>()
    )
}
pub(super) fn vec_value(value: &Vec<Range<SignalSample>>) -> Value {
    json!({"values":value,"len":value.len(),"capacity":value.capacity(),
        "storage":layout::<Range<SignalSample>>(value.capacity())})
}
pub(super) fn layout<T>(capacity: usize) -> Value {
    match Layout::array::<T>(capacity) {
        Ok(value) => json!({"capacity":capacity,"bytes":value.size(),"align":value.align(),
            "scope":"capacity-derived after construction; not an observed allocator request"}),
        Err(_) => json!({"capacity":capacity,"layout":"overflow"}),
    }
}
pub(super) fn input_value(input: SignalInput<'_, '_>, plan: &RenderPlan) -> Value {
    match input {
        SignalInput::Signal(value) => json!({"kind":"Signal","support":value.support(),
            "belongs_to_plan":value.belongs_to(plan),"grid":"unexposed by public Signal API"}),
        SignalInput::Tape(value) | SignalInput::PreserveTape(value) => json!({
            "kind":if matches!(input,SignalInput::Tape(_)) {"Tape"} else {"PreserveTape"},
            "support":value.support(),"grid":value.grid(),"belongs_to_plan":value.belongs_to(plan)}),
        SignalInput::Mix(value) => json!({"kind":"Mix","support":value.support(),
            "grid":value.grid(),"belongs_to_plan":value.belongs_to(plan),
            "voices_len":value.voices().len(),"gates":value.gates().iter().map(|gate| {
                json!({"range":gate.range(),"voices":gate.voices()})
            }).collect::<Vec<_>>(),
            "borrowed_voices_capacity":"unexposed"}),
    }
}
pub(super) fn recipe_value(recipe: StreamRecipe) -> Value {
    json!({"input_frames":recipe.input_frames(),"output_frames":recipe.output_frames(),
        "rate":{"numerator":recipe.rate().numerator(),"denominator":recipe.rate().denominator()},
        "pitch_semitones":recipe.pitch_semitones(),"engine":recipe.engine_id()})
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
        "project_id":renderer.plan.metadata().project_id,
        "revision_id":renderer.plan.metadata().revision_id,
        "limits":{"maximum_input_frames":l.maximum_input_frames,"maximum_output_frames":l.maximum_output_frames,
            "maximum_resident_frames":l.maximum_resident_frames,"maximum_cached_stages":l.maximum_cached_stages,
            "maximum_depth":l.maximum_depth,"maximum_prepared_stages":l.maximum_prepared_stages,
            "maximum_prepared_frames":l.maximum_prepared_frames,"maximum_memory_stage_frames":l.maximum_memory_stage_frames,
            "maximum_disk_bytes":l.maximum_disk_bytes}},
        "progress":renderer.preparation_monitor.as_ref().map(|m|json!({"activity":activity(m.activity()),
            "saved_audio":m.saved_audio_status().map(|s|json!({"key":s.key,"bytes":s.bytes,
                "event":saved_event(s.event),"regenerated_from":s.regenerated_from.map(saved_event)}))}))})
}
