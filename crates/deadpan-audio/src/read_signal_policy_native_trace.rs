//! N request observations; all ownership and billing remain in the real renderer.
use super::*;
use deadpan_plan::SignalPolicyView;
use serde_json::{Value, json};
use std::alloc::Layout;
use std::sync::atomic::Ordering;

const MAX_ROWS: usize = 32768;
const MAX_CALLS: usize = 4096;
const MAX_ACTIVE: usize = 128;
const MAX_HANDOFFS: usize = 4096;
const MAX_ACCOUNTS: usize = 4096;
const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(super) struct Ticket {
    epoch: u64,
    ordinal: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(super) enum UsePoint {
    ContentBefore(usize),
    SpanBefore(usize),
    FinalCheck,
    BeforeHandoff,
    PrepareInitialCheck,
    PrepareRecheck,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub(super) enum Fault {
    #[default]
    None,
    RefuseSpend(usize),
    RefuseReserve(usize),
    CancelBeforeSpend(usize),
    CancelBeforeReserve(usize),
    CancelAtUse(UsePoint),
}
#[derive(Clone, Copy, Debug, Serialize)]
enum Kind {
    SpendAttempt,
    SpendResult,
    ReserveAttempt,
    ReserveResult,
    MakeRoomBefore,
    MakeRoomAfter,
    ReleaseBefore,
    ReleaseAfter,
    MergeBefore,
    MergeAfter,
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
enum Action {
    #[default]
    None,
    Refuse,
    Cancel,
}
#[derive(Clone, Copy, Serialize)]
struct Boundary {
    work: [usize; 2],
    ledger: u64,
    lane: usize,
    ops: point_suppression::ops::trace::WorkBoundary,
    host_units: [usize; 2],
    host_overflow: bool,
    host_epoch: Option<u64>,
}
fn boundary(ledger: &RefCell<ReadWork>) -> Boundary {
    let (work, frames, lane) = {
        let w = ledger.borrow();
        (
            [w.plan_work, w.cache_plan_work],
            w.sound_context_frames,
            usize::from(w.cache_key_scopes > 0),
        )
    };
    let (host_units, host_overflow, host_epoch) =
        original_request::flattened_work_trace::host_eviction_boundary();
    Boundary {
        work,
        ledger: frames,
        lane,
        ops: point_suppression::ops::trace::work_boundary(),
        host_units,
        host_overflow,
        host_epoch,
    }
}
fn coherent(a: Boundary, b: Boundary) -> bool {
    a.ops.enabled
        && b.ops.enabled
        && !a.ops.overflow
        && !b.ops.overflow
        && a.ops.reset_epoch.is_some()
        && a.ops.reset_epoch == b.ops.reset_epoch
        && !a.host_overflow
        && !b.host_overflow
        && a.host_epoch.is_some()
        && a.host_epoch == b.host_epoch
}
#[derive(Clone, Copy, Serialize)]
struct Resident {
    cached: u64,
    active: u64,
    projected: u64,
    sound_context: u64,
    owned_suppression: u64,
    pending_additional: u64,
    sum_before_pending: Option<u64>,
    limit: u32,
}
fn resident(renderer: &StageAudio, control: WorkControl<'_>, pending: u64) -> Resident {
    let (projected, sound_context) = {
        let w = control.work.borrow();
        (w.projected_resident_frames, w.sound_context_frames)
    };
    let cached = renderer.cached_resident_frames();
    let active = renderer.active_frames;
    let owned_suppression = renderer.owned_suppression.live_frames();
    Resident {
        cached,
        active,
        projected,
        sound_context,
        owned_suppression,
        pending_additional: pending,
        sum_before_pending: cached
            .checked_add(active)
            .and_then(|v| v.checked_add(projected))
            .and_then(|v| v.checked_add(sound_context))
            .and_then(|v| v.checked_add(owned_suppression)),
        limit: renderer.limits.maximum_resident_frames,
    }
}
#[derive(Clone, Copy, Serialize)]
struct NativeError {
    kind: &'static str,
    reason: Option<&'static str>,
    cancelled: bool,
}
fn native_error(e: &StageAudioError) -> NativeError {
    let (kind, reason) = match e {
        StageAudioError::Range => ("Range", None),
        StageAudioError::PreparationRange => ("PreparationRange", None),
        StageAudioError::ForeignDomain => ("ForeignDomain", None),
        StageAudioError::ForeignDefinition => ("ForeignDefinition", None),
        StageAudioError::InvalidLimits => ("InvalidLimits", None),
        StageAudioError::Limit(v) => ("Limit", Some(*v)),
        StageAudioError::Timeout => ("Timeout", None),
        StageAudioError::Unsupported(v) => ("Unsupported", Some(*v)),
        StageAudioError::Preparation(_) => ("Preparation", None),
        StageAudioError::Plan(_) => ("Plan", None),
        StageAudioError::Time(_) => ("Time", None),
        StageAudioError::Gain(_) => ("Gain", None),
        StageAudioError::Dsp(_) => ("Dsp", None),
        StageAudioError::Filter(_) => ("Filter", None),
        StageAudioError::Wet(_) => ("Wet", None),
        StageAudioError::Transfer(_) => ("Transfer", None),
        StageAudioError::Cache(_) => ("Cache", None),
    };
    NativeError {
        kind,
        reason,
        cancelled: e.is_cancelled(),
    }
}

#[derive(Clone, Copy, Serialize)]
struct NativeRow {
    ordinal: usize,
    kind: Kind,
    attempt: usize,
    eligible: bool,
    ticket: Option<usize>,
    cause: &'static str,
    request: usize,
    lane: &'static str,
    amount: usize,
    layout: Option<[usize; 2]>,
    frames: Option<u64>,
    before: Boundary,
    after: Option<Boundary>,
    error: Option<NativeError>,
    action: Action,
    actual_injected_refusal: bool,
    resident: Option<Resident>,
    host_delta: Option<[usize; 2]>,
    host_crosscheck: Option<bool>,
    merge_frames: Option<[u64; 4]>,
}
#[derive(Clone, Copy)]
struct Frame {
    id: usize,
    ticket: Option<Ticket>,
    kind: &'static str,
    phase: &'static str,
    index: Option<usize>,
    scope: Option<usize>,
}
#[derive(Clone, Copy)]
struct Timeline {
    at: usize,
    request: usize,
    before: u64,
    after: u64,
}
struct ScopeRow {
    ticket: Ticket,
    request: usize,
    tag: &'static str,
    start: i64,
    end: i64,
    depth: usize,
    entry: Boundary,
    exit: Option<Boundary>,
    factory: bool,
    factory_entry: Option<Boundary>,
    accepted_spend: [usize; 2],
    accepted_host: [usize; 2],
    view: Option<[usize; 4]>,
}
struct Handoff {
    id: usize,
    ticket: Option<Ticket>,
    request: usize,
    ptr: usize,
    len: usize,
    entry: Boundary,
    exit: Option<Boundary>,
    capacity: Option<usize>,
    lease_frames: Option<u64>,
    source_identity: Option<bool>,
    pre_at: Option<usize>,
    pre: Option<Boundary>,
    vector_frames: Option<u64>,
    account_before: Option<Value>,
    finish_at: Option<usize>,
    result: Option<Value>,
    success: bool,
    same_pointer_len: Option<bool>,
    capacity_equal: Option<bool>,
}
#[derive(Clone, Copy, Serialize)]
struct AccountRow {
    ordinal: usize,
    kind: &'static str,
    account_id: Option<usize>,
    before: u64,
    after: u64,
    frames: u64,
    ticket: Option<usize>,
    handoff: Option<usize>,
    work: Option<Boundary>,
}
#[derive(Clone, Copy, Serialize)]
struct ResidentRow {
    ordinal: usize,
    request: usize,
    ticket: Option<usize>,
    boundary: Boundary,
    cached: u64,
    active: u64,
    projected: u64,
    sound_context: u64,
    owned_suppression: u64,
    additional: u64,
    sum_with_pending: Option<u64>,
    maximum_resident_frames: u32,
    disk: u64,
    maximum_disk_bytes: u64,
    new_entry: bool,
    cache_entries: usize,
    original_entries: usize,
    maximum_cached_stages: usize,
}
struct AccountKey {
    ptr: usize,
    id: usize,
    retired: bool,
}
struct State {
    epoch: u64,
    fault: Fault,
    invalid: bool,
    overflow: bool,
    dropped: usize,
    event: usize,
    next_frame: usize,
    attempts: [usize; 2],
    use_cancelled: bool,
    frames: Vec<Frame>,
    calls: Vec<Value>,
    scopes: Vec<ScopeRow>,
    native: Vec<NativeRow>,
    factory_receipts: Vec<Value>,
    uses: Vec<Value>,
    accounts: Vec<AccountRow>,
    residents: Vec<ResidentRow>,
    handoffs: Vec<Handoff>,
    handoff_stack: Vec<usize>,
    requests: Vec<usize>,
    timeline: Vec<Timeline>,
    account_ids: Vec<AccountKey>,
}
thread_local! {
    static CAPTURE:RefCell<Option<State>>=const {RefCell::new(None)};
    static EPOCH:std::cell::Cell<u64>=const {std::cell::Cell::new(0)};
}
pub(super) struct CaptureGuard(u64);
impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.as_ref().is_some_and(|s| s.epoch == self.0) {
                *slot = None;
            }
        });
    }
}
pub(super) fn begin(fault: Fault) -> CaptureGuard {
    let epoch = EPOCH.with(|e| {
        let next = e
            .get()
            .checked_add(1)
            .expect("N native trace epoch exhausted");
        e.set(next);
        next
    });
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(slot.is_none(), "N native trace already active");
        *slot = Some(State {
            epoch,
            fault,
            invalid: false,
            overflow: false,
            dropped: 0,
            event: 0,
            next_frame: 0,
            attempts: [0; 2],
            use_cancelled: false,
            frames: Vec::new(),
            calls: Vec::new(),
            scopes: Vec::new(),
            native: Vec::new(),
            factory_receipts: Vec::new(),
            uses: Vec::new(),
            accounts: Vec::new(),
            residents: Vec::new(),
            handoffs: Vec::new(),
            handoff_stack: Vec::new(),
            requests: Vec::new(),
            timeline: Vec::new(),
            account_ids: Vec::new(),
        });
    });
    CaptureGuard(epoch)
}
fn enabled() -> bool {
    CAPTURE.with(|slot| slot.borrow().is_some())
}
fn ordinal(s: &mut State) -> usize {
    s.event += 1;
    s.event
}
fn selected(s: &State) -> Option<Ticket> {
    for f in s.frames.iter().rev() {
        if f.kind == "consumer" || f.kind == "invocation" || f.kind == "mix" {
            return if f.kind == "mix" { None } else { f.ticket };
        }
    }
    None
}
fn factory_index(s: &State) -> Option<usize> {
    for f in s.frames.iter().rev() {
        if matches!(f.kind, "consumer" | "invocation" | "mix") {
            return None;
        }
        if let Some(i) = f.scope
            && s.scopes[i - 1].factory
        {
            return Some(i);
        }
    }
    None
}
fn scope_for(s: &State, ticket: Ticket) -> Option<usize> {
    s.scopes
        .iter()
        .rposition(|v| v.ticket == ticket)
        .map(|i| i + 1)
}
fn request(s: &mut State, ledger: &RefCell<ReadWork>) -> usize {
    let ptr = ledger as *const _ as usize;
    if let Some(i) = s.requests.iter().position(|p| *p == ptr) {
        return i + 1;
    }
    if s.requests.len() >= MAX_CALLS {
        s.invalid = true;
        s.overflow = true;
        return 0;
    }
    s.requests.push(ptr);
    s.requests.len()
}
fn position_value(s: &State) -> Value {
    json!({"event":s.event,"selected_ticket":selected(s).map(|t|t.ordinal),
        "factory_ticket":factory_index(s).map(|i|s.scopes[i-1].ticket.ordinal),
        "handoff":s.handoff_stack.last(),"active_stack":s.frames.iter().map(|f|json!({
            "frame":f.id,"ticket":f.ticket.map(|t|t.ordinal),"kind":f.kind,"phase":f.phase,
            "index":f.index,"scope":f.scope})).collect::<Vec<_>>()})
}
pub(super) fn position() -> Option<Value> {
    CAPTURE.with(|slot| slot.borrow().as_ref().map(position_value))
}
fn observe(s: &mut State, req: usize, at: usize, b: Boundary) {
    if s.timeline.len() >= MAX_ROWS * 4 {
        s.invalid = true;
        s.overflow = true;
        s.dropped += 1;
        return;
    }
    s.timeline.push(Timeline {
        at,
        request: req,
        before: b.ledger,
        after: b.ledger,
    });
}
fn mutate(s: &mut State, req: usize, at: usize, before: Boundary, after: Boundary) {
    if s.timeline.len() >= MAX_ROWS * 4 {
        s.invalid = true;
        s.overflow = true;
        s.dropped += 1;
        return;
    }
    s.timeline.push(Timeline {
        at,
        request: req,
        before: before.ledger,
        after: after.ledger,
    });
}
fn push_native(s: &mut State, mut row: NativeRow) {
    if s.native.len() >= MAX_ROWS {
        s.overflow = true;
        s.invalid = true;
        s.dropped += 1;
        return;
    }
    row.ordinal = ordinal(s);
    s.native.push(row);
}
fn logical(
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    value: impl FnOnce() -> Value,
) {
    if !enabled() {
        return;
    }
    let now = boundary(control.work);
    let value = value();
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else {
            return;
        };
        if s.uses.len() >= MAX_ROWS {
            s.overflow = true;
            s.invalid = true;
            s.dropped += 1;
            return;
        }
        let req = request(s, control.work);
        let at = ordinal(s);
        observe(s, req, at, now);
        let pos = position_value(s);
        s.uses.push(
            json!({"ordinal":at,"ticket":ticket.map(|t|t.ordinal),"request":req,
            "phase":phase,"boundary":now,"position":pos,"value":value}),
        );
    });
}
pub(super) struct FrameGuard(Option<(u64, usize)>);
impl Drop for FrameGuard {
    fn drop(&mut self) {
        let Some((epoch, id)) = self.0 else {
            return;
        };
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(s) = slot.as_mut().filter(|s| s.epoch == epoch) else {
                return;
            };
            if s.frames.last().map(|f| f.id) != Some(id) {
                s.invalid = true;
                return;
            }
            s.frames.pop();
        });
    }
}
fn frame(
    ticket: Option<Ticket>,
    kind: &'static str,
    phase: &'static str,
    index: Option<usize>,
    scope: Option<usize>,
) -> FrameGuard {
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else {
            return FrameGuard(None);
        };
        if s.frames.len() >= MAX_ACTIVE {
            s.invalid = true;
            s.overflow = true;
            return FrameGuard(None);
        }
        if ticket.is_some_and(|t| t.epoch != s.epoch || t.ordinal == 0 || t.ordinal > s.calls.len())
        {
            s.invalid = true;
            return FrameGuard(None);
        }
        s.next_frame += 1;
        let id = s.next_frame;
        s.frames.push(Frame {
            id,
            ticket,
            kind,
            phase,
            index,
            scope,
        });
        FrameGuard(Some((s.epoch, id)))
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
    caller: read_signal_policy_before_trace::Caller,
    start: SignalSample,
    frames: u32,
    depth: usize,
) -> CallScope {
    let ticket = CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut()?;
        if s.calls.len() >= MAX_CALLS {
            s.invalid = true;
            s.overflow = true;
            return None;
        }
        let ticket = Ticket {
            epoch: s.epoch,
            ordinal: s.calls.len() + 1,
        };
        let parent = s
            .frames
            .iter()
            .rev()
            .find(|f| f.kind == "call")
            .and_then(|f| f.ticket)
            .map(|t| t.ordinal);
        let parent_selected = selected(s).map(|t| t.ordinal);
        s.calls.push(
            json!({"ordinal":ticket.ordinal,"caller":format!("{caller:?}"),"parent":parent,
            "parent_selected_ticket":parent_selected,"start":start,"frames":frames,"depth":depth}),
        );
        Some(ticket)
    });
    CallScope {
        ticket,
        _frame: frame(ticket, "call", "caller", None, None),
    }
}
pub(super) fn invocation(ticket: Option<Ticket>, input: SignalInput<'_, '_>) -> FrameGuard {
    frame(
        ticket,
        if matches!(input, SignalInput::Mix(_)) {
            "mix"
        } else {
            "invocation"
        },
        "invocation",
        None,
        None,
    )
}
pub(super) fn consumer(ticket: Option<Ticket>, kind: &'static str) -> FrameGuard {
    frame(ticket, "consumer", kind, None, None)
}
pub(super) fn phase(
    ticket: Option<Ticket>,
    phase: &'static str,
    index: Option<usize>,
) -> FrameGuard {
    frame(ticket, "phase", phase, index, None)
}
pub(super) struct Scope<'work> {
    control: WorkControl<'work>,
    id: Option<(u64, usize)>,
    _frame: FrameGuard,
}
pub(super) fn scope<'work>(
    control: WorkControl<'work>,
    ticket: Option<Ticket>,
    input: SignalInput<'_, '_>,
    range: Range<SignalSample>,
    depth: usize,
) -> Scope<'work> {
    if !enabled() || ticket.is_none() {
        return Scope {
            control,
            id: None,
            _frame: FrameGuard(None),
        };
    }
    let entry = boundary(control.work);
    let id = CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut()?;
        let ticket = ticket?;
        if s.scopes.len() >= MAX_CALLS {
            s.invalid = true;
            s.overflow = true;
            return None;
        }
        let req = request(s, control.work);
        let tag = match input {
            SignalInput::Signal(_) => "Signal",
            SignalInput::Tape(_) => "Tape",
            SignalInput::PreserveTape(_) => "PreserveTape",
            SignalInput::Mix(_) => "Mix",
        };
        let id = s.scopes.len() + 1;
        s.scopes.push(ScopeRow {
            ticket,
            request: req,
            tag,
            start: range.start.0,
            end: range.end.0,
            depth,
            entry,
            exit: None,
            factory: false,
            factory_entry: None,
            accepted_spend: [0; 2],
            accepted_host: [0; 2],
            view: None,
        });
        let at = ordinal(s);
        observe(s, req, at, entry);
        Some((s.epoch, id))
    });
    Scope {
        control,
        id,
        _frame: frame(ticket, "scope", "policy_owner", None, id.map(|x| x.1)),
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        let Some((epoch, id)) = self.id else {
            return;
        };
        let now = boundary(self.control.work);
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(s) = slot.as_mut().filter(|s| s.epoch == epoch) else {
                return;
            };
            if s.scopes[id - 1].factory {
                s.invalid = true;
            }
            s.scopes[id - 1].exit = Some(now);
            let req = s.scopes[id - 1].request;
            let at = ordinal(s);
            observe(s, req, at, now);
        });
    }
}
pub(super) fn factory_begin(control: WorkControl<'_>, ticket: Option<Ticket>) {
    if !enabled() {
        return;
    }
    let now = boundary(control.work);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else {
            return;
        };
        let Some(i) = ticket.and_then(|t| scope_for(s, t)) else {
            return;
        };
        if s.scopes[i - 1].factory || s.scopes[i - 1].factory_entry.is_some() {
            s.invalid = true;
        }
        s.scopes[i - 1].factory = true;
        s.scopes[i - 1].factory_entry = Some(now);
    });
    logical(control, ticket, "factory_begin", || Value::Null);
}
fn view_value(view: SignalPolicyView<'_>) -> Value {
    json!({"suppressed":view.suppressed,"contents":view.contents,
    "lookup":view.lookup,"work":view.work,"suppressed_len":view.suppressed.len(),"contents_len":view.contents.len(),
    "capacities":"unexposed borrowed slices"})
}
pub(super) fn error(e: &StageAudioError) -> Value {
    read_signal_policy_before_trace::error(e)
}
pub(super) fn factory_result(
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    r: &Result<SignalPolicyInput<'_>, StageAudioError>,
) {
    if !enabled() {
        return;
    }
    let now = boundary(control.work);
    let value = match r {
        Ok(SignalPolicyInput::Admitted(q)) => {
            json!({"status":"Ok","mode":"Admitted","view":view_value(q.view()),
            "canonical":q.canonical_units(),"additional":q.additional_units(),"total":q.work(),"forwarded":q.was_forwarded()})
        }
        Ok(SignalPolicyInput::Ordinary(q)) => {
            json!({"status":"Ok","mode":"Ordinary","suppressed":q.suppressed,
            "contents":q.contents,"lookup":q.lookup,"work":q.work,
            "suppressed_len":q.suppressed.len(),"suppressed_storage":layout::<Range<SignalSample>>(q.suppressed.capacity()),
            "contents_len":q.contents.len(),"contents_storage":layout::<AudioContent>(q.contents.capacity()),
            "admission_fields":"unexposed ordinary"})
        }
        Err(e) => {
            json!({"status":"Err","error":error(e),"query_local_work":"unexposed failed query"})
        }
    };
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else {
            return;
        };
        let Some(i) = ticket.and_then(|t| scope_for(s, t)) else {
            return;
        };
        let row = &mut s.scopes[i - 1];
        if let Ok(policy) = r {
            let view = policy.view();
            row.view = Some([
                view.suppressed.as_ptr() as usize,
                view.suppressed.len(),
                view.contents.as_ptr() as usize,
                view.contents.len(),
            ]);
        }
        if !row.factory {
            s.invalid = true;
        }
        row.factory = false;
        let accepted = row.accepted_spend;
        let host = row.accepted_host;
        let entry = row.factory_entry;
        let delta = entry
            .and_then(|b| {
                now.work[0]
                    .checked_sub(b.work[0])
                    .zip(now.work[1].checked_sub(b.work[1]))
            })
            .map(|(a, b)| [a, b]);
        let equation =
            (0..2).all(|lane| accepted[lane].checked_add(host[lane]) == delta.map(|v| v[lane]));
        if !equation {
            s.invalid = true;
        }
        let forwarded = match r {
            Ok(SignalPolicyInput::Admitted(q)) => Some(
                q.canonical_units().checked_add(q.additional_units()) == Some(q.work())
                    && q.was_forwarded()
                    && accepted[0].checked_add(accepted[1]) == Some(q.work()),
            ),
            _ => None,
        };
        if forwarded == Some(false) {
            s.invalid = true;
        }
        let receipt = json!({"ticket":ticket.map(|t|t.ordinal),"scope":i,"entry":entry,"exit":now,
            "accepted_spend":accepted,"accepted_host":host,"work_delta":delta,
            "factory_equation":equation,"forwarding_equation":forwarded,"result":value});
        s.factory_receipts.push(receipt);
    });
    logical(control, ticket, "factory_result", || Value::Null);
}
pub(super) fn use_policy(
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    p: &SignalPolicyInput<'_>,
) {
    if !enabled() || ticket.is_none() {
        return;
    }
    let view = p.view();
    let same = CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut()?;
        let i = ticket.and_then(|t| scope_for(s, t))?;
        let current = [
            view.suppressed.as_ptr() as usize,
            view.suppressed.len(),
            view.contents.as_ptr() as usize,
            view.contents.len(),
        ];
        let previous = s.scopes[i - 1].view;
        if view.suppressed.is_empty() && view.contents.is_empty() {
            None
        } else {
            let same = previous.map(|p| p == current);
            if same != Some(true) {
                s.invalid = true;
            }
            same
        }
    });
    logical(control, ticket, phase, || {
        json!({"view":view_value(view),"same_local_view_identity":same,
        "mode":match p{SignalPolicyInput::Ordinary(_)=>"Ordinary",SignalPolicyInput::Admitted(_)=>"Admitted"}})
    });
}
pub(super) fn cancel_at(control: WorkControl<'_>, ticket: Option<Ticket>, point: UsePoint) {
    if !enabled() || ticket.is_none() {
        return;
    }
    let cancel = CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut().unwrap();
        let cancel = !s.use_cancelled && s.fault == Fault::CancelAtUse(point);
        s.use_cancelled |= cancel;
        cancel
    });
    logical(
        control,
        ticket,
        "use_point",
        || json!({"point":point,"action":if cancel{"set_real_cancellation"}else{"none"}}),
    );
    if cancel {
        control.cancelled.store(true, Ordering::Relaxed);
    }
}
pub(super) fn result<T>(
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    r: &Result<T, StageAudioError>,
) {
    logical(control, ticket, phase, || match r {
        Ok(_) => json!({"status":"Ok","success_payload":"unexposed"}),
        Err(e) => json!({"status":"Err","error":error(e)}),
    });
}
fn layout<T>(capacity: usize) -> Value {
    match Layout::array::<T>(capacity) {
        Ok(v) => json!({"capacity":capacity,"bytes":v.size(),"align":v.align()}),
        Err(_) => json!({"capacity":capacity,"layout":"overflow"}),
    }
}

#[derive(Clone, Copy)]
pub(super) struct Attempt {
    epoch: Option<u64>,
    factory: Option<usize>,
    ticket: Option<Ticket>,
    request: usize,
    ordinal: usize,
    amount: usize,
    layout: Option<[usize; 2]>,
    before: Option<Boundary>,
    action: Action,
    reserve: bool,
}
impl Attempt {
    pub(super) fn apply_cancellation(self, control: WorkControl<'_>) {
        if matches!(self.action, Action::Cancel) {
            control.cancelled.store(true, Ordering::Relaxed);
        }
    }
    pub(super) fn injected_refusal(self) -> bool {
        matches!(self.action, Action::Refuse)
    }
}
fn attempt(
    control: WorkControl<'_>,
    amount: usize,
    layout: Option<Layout>,
    reserve: bool,
) -> Attempt {
    let mut token = Attempt {
        epoch: None,
        factory: None,
        ticket: None,
        request: 0,
        ordinal: 0,
        amount,
        layout: layout.map(|v| [v.size(), v.align()]),
        before: None,
        action: Action::None,
        reserve,
    };
    if !enabled() {
        return token;
    }
    let before = boundary(control.work);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else {
            return;
        };
        let factory = factory_index(s);
        let eligible = factory.is_some();
        let req = request(s, control.work);
        let index = usize::from(reserve);
        let ordinal = if eligible {
            s.attempts[index] += 1;
            s.attempts[index]
        } else {
            0
        };
        let action = if !eligible {
            Action::None
        } else {
            match (reserve, s.fault) {
                (false, Fault::RefuseSpend(n)) | (true, Fault::RefuseReserve(n))
                    if n == ordinal =>
                {
                    Action::Refuse
                }
                (false, Fault::CancelBeforeSpend(n)) | (true, Fault::CancelBeforeReserve(n))
                    if n == ordinal =>
                {
                    Action::Cancel
                }
                _ => Action::None,
            }
        };
        let ticket = factory
            .map(|i| s.scopes[i - 1].ticket)
            .or_else(|| selected(s));
        token = Attempt {
            epoch: Some(s.epoch),
            factory,
            ticket,
            request: req,
            ordinal,
            amount,
            layout: token.layout,
            before: Some(before),
            action,
            reserve,
        };
        push_native(
            s,
            NativeRow {
                ordinal: 0,
                kind: if reserve {
                    Kind::ReserveAttempt
                } else {
                    Kind::SpendAttempt
                },
                attempt: ordinal,
                eligible,
                ticket: ticket.map(|t| t.ordinal),
                cause: if eligible {
                    "SelectedFactory"
                } else {
                    "Ambient"
                },
                request: req,
                lane: if before.lane == 0 { "Plan" } else { "Cache" },
                amount,
                layout: token.layout,
                frames: None,
                before,
                after: None,
                error: None,
                action,
                actual_injected_refusal: false,
                resident: None,
                host_delta: None,
                host_crosscheck: None,
                merge_frames: None,
            },
        );
        {
            let event = s.event;
            observe(s, req, event, before);
        }
    });
    token
}
pub(super) fn spend_begin(control: WorkControl<'_>, amount: usize) -> Attempt {
    attempt(control, amount, None, false)
}
pub(super) fn reserve_begin(control: WorkControl<'_>, layout: Layout) -> Attempt {
    attempt(control, 0, Some(layout), true)
}
fn finish_attempt(
    token: Attempt,
    control: WorkControl<'_>,
    error: Option<&StageAudioError>,
    frames: Option<u64>,
) {
    let Some(epoch) = token.epoch else {
        return;
    };
    let now = boundary(control.work);
    let before = token.before.unwrap();
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut().filter(|s| s.epoch == epoch) else {
            return;
        };
        if token.reserve {
            if error.is_none() {
                let expected = token
                    .layout
                    .and_then(|l| u64::try_from(l[0]).ok())
                    .map(|n| n.div_ceil(8));
                if frames != expected
                    || frames.and_then(|n| before.ledger.checked_add(n)) != Some(now.ledger)
                {
                    s.invalid = true;
                }
            } else if before.ledger != now.ledger {
                s.invalid = true;
            }
        } else {
            let mut delta = [0; 2];
            if error.is_none() {
                delta[before.lane] = token.amount;
            }
            if (0..2).any(|i| now.work[i].checked_sub(before.work[i]) != Some(delta[i])) {
                s.invalid = true;
            }
            if let Some(i) = token.factory.filter(|_| error.is_none()) {
                match s.scopes[i - 1].accepted_spend[before.lane].checked_add(token.amount) {
                    Some(n) => s.scopes[i - 1].accepted_spend[before.lane] = n,
                    None => {
                        s.invalid = true;
                        s.overflow = true;
                    }
                }
            }
        }
        let injected = error.is_some_and(|e| {
            matches!(
                e,
                StageAudioError::Limit(
                    "test-only read signal policy spend refusal"
                        | "test-only read signal policy reserve refusal"
                )
            )
        });
        push_native(
            s,
            NativeRow {
                ordinal: 0,
                kind: if token.reserve {
                    Kind::ReserveResult
                } else {
                    Kind::SpendResult
                },
                attempt: token.ordinal,
                eligible: token.factory.is_some(),
                ticket: token.ticket.map(|t| t.ordinal),
                cause: if token.factory.is_some() {
                    "SelectedFactory"
                } else {
                    "Ambient"
                },
                request: token.request,
                lane: if before.lane == 0 { "Plan" } else { "Cache" },
                amount: token.amount,
                layout: token.layout,
                frames,
                before,
                after: Some(now),
                error: error.map(native_error),
                action: token.action,
                actual_injected_refusal: injected,
                resident: None,
                host_delta: None,
                host_crosscheck: None,
                merge_frames: None,
            },
        );
        if token.reserve {
            {
                let event = s.event;
                mutate(s, token.request, event, before, now);
            }
        } else {
            {
                let event = s.event;
                observe(s, token.request, event, now);
            }
        }
    });
}
pub(super) fn spend_result(
    token: Attempt,
    control: WorkControl<'_>,
    r: &Result<(), StageAudioError>,
) {
    finish_attempt(token, control, r.as_ref().err(), None);
}
pub(super) fn reserve_result(
    token: Attempt,
    control: WorkControl<'_>,
    r: Result<u64, &StageAudioError>,
) {
    finish_attempt(token, control, r.as_ref().err().copied(), r.ok());
}
#[derive(Clone, Copy)]
pub(super) struct Room {
    attempt: Attempt,
    before: Option<Boundary>,
    frames: u64,
}
pub(super) fn room_begin(
    token: Attempt,
    renderer: &StageAudio,
    control: WorkControl<'_>,
    frames: u64,
) -> Room {
    let room = Room {
        attempt: token,
        before: token.epoch.map(|_| boundary(control.work)),
        frames,
    };
    let Some(before) = room.before else {
        return room;
    };
    let components = resident(renderer, control, frames);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut().filter(|s| Some(s.epoch) == token.epoch) else {
            return;
        };
        push_native(
            s,
            NativeRow {
                ordinal: 0,
                kind: Kind::MakeRoomBefore,
                attempt: token.ordinal,
                eligible: token.factory.is_some(),
                ticket: token.ticket.map(|t| t.ordinal),
                cause: if token.factory.is_some() {
                    "SelectedFactory"
                } else {
                    "Ambient"
                },
                request: token.request,
                lane: if before.lane == 0 { "Plan" } else { "Cache" },
                amount: 0,
                layout: token.layout,
                frames: Some(frames),
                before,
                after: None,
                error: None,
                action: Action::None,
                actual_injected_refusal: false,
                resident: Some(components),
                host_delta: None,
                host_crosscheck: None,
                merge_frames: None,
            },
        );
        {
            let event = s.event;
            observe(s, token.request, event, before);
        }
    });
    room
}
pub(super) fn room_result(
    room: Room,
    renderer: &StageAudio,
    control: WorkControl<'_>,
    r: &Result<(), StageAudioError>,
) {
    let Some(before) = room.before else {
        return;
    };
    let now = boundary(control.work);
    let components = resident(renderer, control, room.frames);
    let delta = now.work[0]
        .checked_sub(before.work[0])
        .zip(now.work[1].checked_sub(before.work[1]))
        .map(|(a, b)| [a, b]);
    let mut cross = coherent(before, now);
    for lane in 0..2 {
        let host = now.host_units[lane].checked_sub(before.host_units[lane]);
        let native = now.ops.lanes[lane][7].checked_sub(before.ops.lanes[lane][7]);
        cross &= host.zip(native).and_then(|(a, b)| a.checked_add(b)) == delta.map(|v| v[lane]);
        for phase in 0..7 {
            cross &= before.ops.lanes[lane][phase] == now.ops.lanes[lane][phase];
        }
    }
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot
            .as_mut()
            .filter(|s| Some(s.epoch) == room.attempt.epoch)
        else {
            return;
        };
        if !cross {
            s.invalid = true;
        }
        if let (Some(i), Some(delta)) = (room.attempt.factory, delta) {
            for (lane, amount) in delta.into_iter().enumerate() {
                match s.scopes[i - 1].accepted_host[lane].checked_add(amount) {
                    Some(n) => s.scopes[i - 1].accepted_host[lane] = n,
                    None => {
                        s.invalid = true;
                        s.overflow = true;
                    }
                }
            }
        }
        let token = room.attempt;
        push_native(
            s,
            NativeRow {
                ordinal: 0,
                kind: Kind::MakeRoomAfter,
                attempt: token.ordinal,
                eligible: token.factory.is_some(),
                ticket: token.ticket.map(|t| t.ordinal),
                cause: if token.factory.is_some() {
                    "SelectedFactory"
                } else {
                    "Ambient"
                },
                request: token.request,
                lane: if before.lane == 0 { "Plan" } else { "Cache" },
                amount: 0,
                layout: token.layout,
                frames: Some(room.frames),
                before,
                after: Some(now),
                error: r.as_ref().err().map(native_error),
                action: Action::None,
                actual_injected_refusal: false,
                resident: Some(components),
                host_delta: delta,
                host_crosscheck: Some(cross),
                merge_frames: None,
            },
        );
        {
            let event = s.event;
            observe(s, token.request, event, now);
        }
    });
}
#[derive(Clone, Copy)]
pub(super) struct Mutation {
    epoch: Option<u64>,
    request: usize,
    before: Option<Boundary>,
    frames: [u64; 2],
    merge: bool,
    ticket: Option<Ticket>,
}
fn mutation(ledger: &RefCell<ReadWork>, frames: [u64; 2], merge: bool) -> Mutation {
    let mut token = Mutation {
        epoch: None,
        request: 0,
        before: None,
        frames,
        merge,
        ticket: None,
    };
    if !enabled() {
        return token;
    }
    let before = boundary(ledger);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else {
            return;
        };
        let req = request(s, ledger);
        let ticket = selected(s);
        token = Mutation {
            epoch: Some(s.epoch),
            request: req,
            before: Some(before),
            frames,
            merge,
            ticket,
        };
        push_native(
            s,
            NativeRow {
                ordinal: 0,
                kind: if merge {
                    Kind::MergeBefore
                } else {
                    Kind::ReleaseBefore
                },
                attempt: 0,
                eligible: false,
                ticket: ticket.map(|t| t.ordinal),
                cause: "LeaseMutation",
                request: req,
                lane: if before.lane == 0 { "Plan" } else { "Cache" },
                amount: 0,
                layout: None,
                frames: Some(frames[0]),
                before,
                after: None,
                error: None,
                action: Action::None,
                actual_injected_refusal: false,
                resident: None,
                host_delta: None,
                host_crosscheck: None,
                merge_frames: if merge {
                    Some([frames[0], frames[1], 0, 0])
                } else {
                    None
                },
            },
        );
        {
            let event = s.event;
            observe(s, req, event, before);
        }
    });
    token
}
pub(super) fn release_begin(ledger: &RefCell<ReadWork>, frames: u64) -> Mutation {
    mutation(ledger, [frames, 0], false)
}
pub(super) fn merge_begin(control: WorkControl<'_>, target: u64, source: u64) -> Mutation {
    mutation(control.work, [target, source], true)
}
fn finish_mutation(token: Mutation, ledger: &RefCell<ReadWork>, target: u64, source: u64) {
    let Some(epoch) = token.epoch else {
        return;
    };
    let before = token.before.unwrap();
    let now = boundary(ledger);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut().filter(|s| s.epoch == epoch) else {
            return;
        };
        if token.merge {
            if now.ledger != before.ledger
                || token.frames[0].checked_add(token.frames[1]) != Some(target)
                || source != 0
            {
                s.invalid = true;
            }
        } else if before.ledger.checked_sub(token.frames[0]) != Some(now.ledger) {
            s.invalid = true;
        }
        push_native(
            s,
            NativeRow {
                ordinal: 0,
                kind: if token.merge {
                    Kind::MergeAfter
                } else {
                    Kind::ReleaseAfter
                },
                attempt: 0,
                eligible: false,
                ticket: token.ticket.map(|t| t.ordinal),
                cause: "LeaseMutation",
                request: token.request,
                lane: if before.lane == 0 { "Plan" } else { "Cache" },
                amount: 0,
                layout: None,
                frames: Some(token.frames[0]),
                before,
                after: Some(now),
                error: None,
                action: Action::None,
                actual_injected_refusal: false,
                resident: None,
                host_delta: None,
                host_crosscheck: None,
                merge_frames: if token.merge {
                    Some([token.frames[0], token.frames[1], target, source])
                } else {
                    None
                },
            },
        );
        {
            let event = s.event;
            mutate(s, token.request, event, before, now);
        }
    });
}
pub(super) fn release_result(token: Mutation, ledger: &RefCell<ReadWork>) {
    finish_mutation(token, ledger, 0, 0)
}
pub(super) fn merge_result(token: Mutation, control: WorkControl<'_>, target: u64, source: u64) {
    finish_mutation(token, control.work, target, source)
}

fn account_id(s: &mut State, ptr: usize) -> Option<usize> {
    if let Some(a) = s
        .account_ids
        .iter()
        .rev()
        .find(|a| a.ptr == ptr && !a.retired)
    {
        return Some(a.id);
    }
    if s.account_ids.len() >= MAX_ACCOUNTS {
        s.invalid = true;
        s.overflow = true;
        s.dropped += 1;
        return None;
    }
    let id = s.account_ids.len() + 1;
    s.account_ids.push(AccountKey {
        ptr,
        id,
        retired: false,
    });
    Some(id)
}
pub(super) fn mask_scalar(
    kind: &'static str,
    ptr: Option<usize>,
    live: Option<u64>,
    charge: Option<u64>,
    len: usize,
    capacity: usize,
) -> Value {
    let id = CAPTURE.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .and_then(|s| ptr.and_then(|p| account_id(s, p)))
    });
    json!({"kind":kind,"mask_len":len,"mask_capacity":capacity,"account_live_frames":live,
        "charge_frames":charge,"account_id":id,"storage":layout::<Range<SignalSample>>(capacity)})
}
pub(super) fn mask_snapshot(mask: &SignalSuppressionMask) -> Value {
    point_suppression::read_signal_native_mask_snapshot(mask)
}
pub(super) fn account_event(ptr: usize, kind: &'static str, before: u64, after: u64, frames: u64) {
    if !enabled() {
        return;
    }
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else {
            return;
        };
        if s.accounts.len() >= MAX_ROWS {
            s.invalid = true;
            s.overflow = true;
            s.dropped += 1;
            return;
        }
        let id = account_id(s, ptr);
        let at = ordinal(s);
        let ticket = selected(s).map(|t| t.ordinal);
        let handoff = s.handoff_stack.last().copied();
        s.accounts.push(AccountRow {
            ordinal: at,
            kind,
            account_id: id,
            before,
            after,
            frames,
            ticket,
            handoff,
            work: None,
        });
        if kind == "LastHandleDrop"
            && let Some(a) = s
                .account_ids
                .iter_mut()
                .rev()
                .find(|a| a.ptr == ptr && !a.retired)
        {
            a.retired = true;
        }
    });
}
pub(super) fn renderer_account(renderer: &StageAudio) -> Value {
    point_suppression::read_signal_native_account_snapshot(renderer)
}
pub(super) fn account_scalar(ptr: Option<usize>, live: u64) -> Value {
    let id = CAPTURE.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .and_then(|s| ptr.and_then(|p| account_id(s, p)))
    });
    json!({"account_id":id,"live_frames":live})
}
pub(super) struct HandoffGuard<'work> {
    control: WorkControl<'work>,
    id: Option<(u64, usize)>,
}
pub(super) fn handoff_begin<'work>(
    renderer: &StageAudio,
    control: WorkControl<'work>,
    ticket: Option<Ticket>,
    view: SignalPolicyView<'_>,
) -> HandoffGuard<'work> {
    if !enabled() {
        return HandoffGuard { control, id: None };
    }
    let entry = boundary(control.work);
    let before_account = renderer_account(renderer);
    let id = CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut()?;
        if s.handoffs.len() >= MAX_HANDOFFS || s.handoff_stack.len() >= MAX_ACTIVE {
            s.invalid = true;
            s.overflow = true;
            return None;
        }
        let req = request(s, control.work);
        let id = s.handoffs.len() + 1;
        let at = ordinal(s);
        observe(s, req, at, entry);
        s.handoffs.push(Handoff {
            id,
            ticket,
            request: req,
            ptr: view.suppressed.as_ptr() as usize,
            len: view.suppressed.len(),
            entry,
            exit: None,
            capacity: None,
            lease_frames: None,
            source_identity: None,
            pre_at: None,
            pre: None,
            vector_frames: None,
            account_before: Some(before_account),
            finish_at: None,
            result: None,
            success: false,
            same_pointer_len: None,
            capacity_equal: None,
        });
        s.handoff_stack.push(id);
        Some((s.epoch, id))
    });
    HandoffGuard { control, id }
}
impl Drop for HandoffGuard<'_> {
    fn drop(&mut self) {
        let Some((epoch, id)) = self.id else {
            return;
        };
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(s) = slot.as_mut().filter(|s| s.epoch == epoch) else {
                return;
            };
            if s.handoff_stack.pop() != Some(id) || s.handoffs[id - 1].result.is_none() {
                s.invalid = true;
            }
        });
    }
}
pub(super) fn prepare_entry(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    capacity: usize,
    lease_frames: Option<u64>,
    identity: bool,
) {
    if !enabled() {
        return;
    }
    let now = boundary(control.work);
    let account = renderer_account(renderer);
    CAPTURE.with(|slot|{
        let mut slot=slot.borrow_mut();let Some(s)=slot.as_mut()else{return;};
        let Some(id)=s.handoff_stack.last().copied()else{return;};
        let req=s.handoffs[id-1].request;let at=ordinal(s);observe(s,req,at,now);
        let h=&mut s.handoffs[id-1];h.capacity=Some(capacity);h.lease_frames=lease_frames;h.source_identity=Some(identity);
        if s.uses.len()>=MAX_ROWS {s.invalid=true;s.overflow=true;s.dropped+=1;return;}
        s.uses.push(json!({"ordinal":at,"phase":"prepare_entry","handoff":id,"ticket":h.ticket.map(|t|t.ordinal),
            "capacity":capacity,"lease_frames":lease_frames,"source_identity":identity,"boundary":now,"account":account}));
    });
}
pub(super) fn pre_exclusive(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    vector_frames: u64,
    capacity: usize,
    destination_vector_frames: u64,
    source_identity: bool,
) {
    if !enabled() {
        return;
    }
    let now = boundary(control.work);
    let account = renderer_account(renderer);
    CAPTURE.with(|slot|{
        let mut slot=slot.borrow_mut();let Some(s)=slot.as_mut()else{return;};
        let Some(id)=s.handoff_stack.last().copied()else{return;};let req=s.handoffs[id-1].request;
        let at=ordinal(s);observe(s,req,at,now);
        let h=&mut s.handoffs[id-1];h.pre_at=Some(at);h.pre=Some(now);h.vector_frames=Some(vector_frames);h.account_before=Some(account.clone());
        if h.capacity!=Some(capacity)||h.lease_frames!=Some(vector_frames)||destination_vector_frames!=vector_frames||
            Layout::array::<Range<SignalSample>>(capacity).ok().and_then(|layout|u64::try_from(layout.size()).ok()).map(|v|v.div_ceil(8))!=Some(vector_frames)||!source_identity{
            s.invalid=true;
        }
        if s.uses.len()>=MAX_ROWS {s.invalid=true;s.overflow=true;s.dropped+=1;return;}
        s.uses.push(json!({"ordinal":at,"phase":"pre_exclusive","handoff":id,"ticket":h.ticket.map(|t|t.ordinal),
            "capacity":capacity,"vector_frames":vector_frames,"destination_vector_frames":destination_vector_frames,
            "source_identity":source_identity,"boundary":now,"account":account}));
    });
}
pub(super) fn prepare_error(control: WorkControl<'_>, error_value: &StageAudioError) {
    let ticket = CAPTURE.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|s| s.handoff_stack.last().map(|id| s.handoffs[*id - 1].ticket))
            .flatten()
    });
    logical(
        control,
        ticket,
        "prepare_error",
        || json!({"error":error(error_value)}),
    );
}
pub(super) fn handoff_result(
    guard: &HandoffGuard<'_>,
    renderer: &StageAudio,
    r: &Result<SignalSuppressionMask, StageAudioError>,
) {
    let Some((epoch, id)) = guard.id else {
        return;
    };
    let now = boundary(guard.control.work);
    let value = match r {
        Ok(mask) => {
            json!({"status":"Ok","mask":mask_snapshot(mask),"account":renderer_account(renderer)})
        }
        Err(e) => json!({"status":"Err","error":error(e),"account":renderer_account(renderer)}),
    };
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut().filter(|s| s.epoch == epoch) else {
            return;
        };
        let at = ordinal(s);
        let req = s.handoffs[id - 1].request;
        observe(s, req, at, now);
        let h = &mut s.handoffs[id - 1];
        h.finish_at = Some(at);
        h.exit = Some(now);
        h.result = Some(value);
        h.success = r.is_ok();
        if let Ok(mask) = r {
            h.capacity_equal = h.capacity.map(|cap| cap == mask.capacity());
            h.same_pointer_len = if h.capacity == Some(0) {
                None
            } else {
                Some(h.ptr == mask.as_slice().as_ptr() as usize && h.len == mask.len())
            };
            if h.capacity_equal != Some(true) || h.same_pointer_len == Some(false) {
                s.invalid = true;
            }
            if h.capacity == Some(0) {
                if h.lease_frames.is_some() || h.pre.is_some() || !mask.is_empty() {
                    s.invalid = true;
                }
            } else if h.pre.is_none() || h.source_identity != Some(true) {
                s.invalid = true;
            }
        }
    });
}
pub(super) fn prepare_cancel(control: WorkControl<'_>, point: UsePoint) {
    let ticket = CAPTURE.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|s| s.handoff_stack.last().map(|id| s.handoffs[*id - 1].ticket))
            .flatten()
    });
    cancel_at(control, ticket, point);
}
pub(super) fn resident_predicate(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    additional: u64,
    new_entry: bool,
    cached: u64,
    disk: u64,
    owned: u64,
) {
    if !enabled() {
        return;
    }
    let b = boundary(control.work);
    let (projected, native) = {
        let w = control.work.borrow();
        (w.projected_resident_frames, w.sound_context_frames)
    };
    let sum = cached
        .checked_add(renderer.active_frames)
        .and_then(|v| v.checked_add(projected))
        .and_then(|v| v.checked_add(native))
        .and_then(|v| v.checked_add(owned))
        .and_then(|v| v.checked_add(additional));
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else {
            return;
        };
        if s.residents.len() >= MAX_ROWS {
            s.invalid = true;
            s.overflow = true;
            s.dropped += 1;
            return;
        }
        let req = request(s, control.work);
        let at = ordinal(s);
        observe(s, req, at, b);
        let ticket = selected(s).map(|t| t.ordinal);
        s.residents.push(ResidentRow {
            ordinal: at,
            request: req,
            ticket,
            boundary: b,
            cached,
            active: renderer.active_frames,
            projected,
            sound_context: native,
            owned_suppression: owned,
            additional,
            sum_with_pending: sum,
            maximum_resident_frames: renderer.limits.maximum_resident_frames,
            disk,
            maximum_disk_bytes: renderer.limits.maximum_disk_bytes,
            new_entry,
            cache_entries: renderer.cache.len(),
            original_entries: renderer.original_inputs.len(),
            maximum_cached_stages: renderer.limits.maximum_cached_stages,
        });
    });
}
fn replay(s: &State) -> Vec<Value> {
    let mut expected: Vec<Option<u64>> = vec![None; s.requests.len() + 1];
    let mut used = vec![false; s.handoffs.len()];
    let mut gaps = Vec::new();
    for e in &s.timeline {
        let current = expected.get_mut(e.request);
        let Some(current) = current else {
            gaps.push(json!({"ordinal":e.at,"request":e.request,"reason":"unknown ledger"}));
            continue;
        };
        if let Some(old) = *current
            && old != e.before
        {
            let candidate = s.handoffs.iter().enumerate().find(|(i, h)| {
                !used[*i]
                    && h.success
                    && h.request == e.request
                    && h.pre_at.is_some_and(|a| a < e.at)
                    && h.finish_at.is_some_and(|a| e.at <= a)
                    && h.vector_frames.and_then(|v| old.checked_sub(v)) == Some(e.before)
            });
            if let Some((i, _)) = candidate {
                used[i] = true;
            } else {
                gaps.push(
                    json!({"ordinal":e.at,"request":e.request,"expected":old,"actual":e.before,
                    "reason":"unreconciled transfer; never attributed by residual"}),
                );
            }
        }
        *current = Some(e.after);
    }
    for (i, h) in s.handoffs.iter().enumerate() {
        if h.success && h.vector_frames.is_some_and(|v| v > 0) && !used[i] {
            gaps.push(
                json!({"handoff":h.id,"reason":"known successful vector debit not reconciled"}),
            );
        }
    }
    gaps
}
pub(super) fn take() -> Value {
    CAPTURE.with(|slot|slot.borrow_mut().take().map_or(Value::Null,|mut s|{
        let pending=s.handoffs.iter().filter(|h|h.result.is_none()).map(|h|h.id).collect::<Vec<_>>();
        let gaps=replay(&s);
        if !s.frames.is_empty()||!s.handoff_stack.is_empty()||!pending.is_empty()||!gaps.is_empty(){s.invalid=true;}
        let scopes=s.scopes.iter().enumerate().map(|(i,v)|json!({"ordinal":i+1,"ticket":v.ticket.ordinal,
            "request":v.request,"tag":v.tag,"start":v.start,"end":v.end,"depth":v.depth,
            "entry":v.entry,"exit":v.exit,"factory_active":v.factory,"accepted_spend":v.accepted_spend,
            "accepted_host":v.accepted_host})).collect::<Vec<_>>();
        let handoffs=s.handoffs.iter().map(|h|json!({"ordinal":h.id,"ticket":h.ticket.map(|t|t.ordinal),
            "request":h.request,"entry":h.entry,"exit":h.exit,"source_len":h.len,"prepare_capacity":h.capacity,
            "lease_frames":h.lease_frames,"source_identity":h.source_identity,"pre_exclusive_ordinal":h.pre_at,
            "pre_exclusive":h.pre,"vector_frames":h.vector_frames,"account_before":h.account_before,
            "finish_ordinal":h.finish_at,"success":h.success,"same_pointer_len":h.same_pointer_len,
            "capacity_equal":h.capacity_equal,"result":h.result})).collect::<Vec<_>>();
        let mut value=json!({"schema":"read-signal-policy-native-v1","fault":s.fault,"attempts":s.attempts,
            "invalid":s.invalid,"dropped":s.dropped,"overflow":s.overflow,"active_at_take":position_value(&s)["active_stack"],
            "pending_handoffs_at_take":pending,"unreconciled_transfers":gaps,"calls":s.calls,"scopes":scopes,
            "factory_receipts":s.factory_receipts,"use_rows":s.uses,"native_rows":s.native,
            "handoff_receipts":handoffs,"account_rows":s.accounts,"resident_rows":s.residents,
            "request_rows":(1..=s.requests.len()).map(|id|json!({"ordinal":id,"identity":"local pointer equality only; address unexposed"})).collect::<Vec<_>>()});
        if serde_json::to_vec(&value).expect("native trace JSON serializes").len()>MAX_BYTES{
            value["invalid"]=json!(true);value["overflow"]=json!(true);value["dropped"]=json!(s.dropped+1);
        }value
    }))
}
