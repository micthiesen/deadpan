//! O observations at the explicit post-DSP caller; billing stays in real admission.
use super::*;
use deadpan_plan::SignalPolicyView;
use serde_json::{Value, json};
use std::alloc::Layout;
use std::sync::atomic::Ordering;

const MAX_ROWS: usize = 32768;
const MAX_CALLS: usize = 4096;
const MAX_ACTIVE: usize = 128;
const MAX_REQUESTS: usize = 4096;
const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(super) struct Ticket {
    epoch: u64,
    ordinal: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(super) enum UsePoint {
    BeforeLimits,
    ContentBefore(usize),
    FinalCheck,
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
    CancelAtCallUse(usize, UsePoint),
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
struct Frame {
    id: usize,
    ticket: usize,
    kind: &'static str,
}
#[derive(Serialize)]
struct Call {
    ordinal: usize,
    request: usize,
    parent: Option<usize>,
    input: &'static str,
    start: i64,
    sample_len: usize,
    entry: Boundary,
    method_entry: Option<Boundary>,
    method_exit: Option<Boundary>,
    exit: Option<Boundary>,
    factory_entry: Option<Boundary>,
    factory_active: bool,
    factory_done: bool,
    accepted_spend: [usize; 2],
    accepted_host: [usize; 2],
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
    cancelled_at_use: bool,
    requests: Vec<usize>,
    calls: Vec<Call>,
    stack: Vec<Frame>,
    rows: Vec<Value>,
    row_bytes: usize,
}
std::thread_local! {
    static CAPTURE: RefCell<Option<State>> = const { RefCell::new(None) };
    static EPOCH: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
fn enabled() -> bool {
    CAPTURE.with(|slot| slot.borrow().as_ref().is_some_and(|s| !s.invalid))
}
fn current_ticket(ticket: Ticket) -> bool {
    CAPTURE.with(|slot| slot.borrow().as_ref().is_some_and(|s| s.valid(ticket)))
}
impl State {
    fn overflow(&mut self) {
        self.invalid = true;
        self.overflow = true;
        self.dropped = self.dropped.saturating_add(1);
    }
    fn valid(&self, ticket: Ticket) -> bool {
        ticket.epoch == self.epoch && ticket.ordinal > 0 && ticket.ordinal <= self.calls.len()
    }
    fn storage(&mut self) -> bool {
        let calls = self.calls.iter().try_fold(0usize, |sum, call| {
            sum.checked_add(serde_json::to_vec(call).expect("O call serializes").len())
        });
        if calls
            .and_then(|n| self.row_bytes.checked_add(n))
            .is_none_or(|n| n > MAX_BYTES)
        {
            self.overflow();
            return false;
        }
        true
    }
    fn push(
        &mut self,
        phase: &'static str,
        ticket: Option<usize>,
        request: Option<usize>,
        value: Value,
    ) {
        if self.invalid {
            return;
        }
        if self.rows.len() >= MAX_ROWS {
            self.overflow();
            return;
        }
        let Some(at) = self.event.checked_add(1) else {
            self.overflow();
            return;
        };
        let row = json!({"ordinal":at,"phase":phase,"ticket":ticket,"request":request,
            "position":{"active_stack":self.stack,"last_ordinal":at},"value":value});
        let size = serde_json::to_vec(&row).expect("O row serializes").len();
        let Some(bytes) = self.row_bytes.checked_add(size) else {
            self.overflow();
            return;
        };
        let previous = self.row_bytes;
        self.row_bytes = bytes;
        if !self.storage() {
            self.row_bytes = previous;
            return;
        }
        self.event = at;
        self.rows.push(row);
    }
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
    let epoch = EPOCH.with(|value| {
        let next = value.get().checked_add(1).expect("O trace epoch exhausted");
        value.set(next);
        next
    });
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(slot.is_none(), "O native capture already active");
        *slot = Some(State {
            epoch,
            fault,
            invalid: false,
            overflow: false,
            dropped: 0,
            event: 0,
            next_frame: 0,
            attempts: [0; 2],
            cancelled_at_use: false,
            requests: Vec::new(),
            calls: Vec::new(),
            stack: Vec::new(),
            rows: Vec::new(),
            row_bytes: 0,
        });
    });
    CaptureGuard(epoch)
}
fn request(s: &mut State, ledger: &RefCell<ReadWork>) -> Option<usize> {
    // Private equality key only. It is never exposed or dereferenced.
    let key = ledger as *const RefCell<ReadWork> as usize;
    if let Some(i) = s.requests.iter().position(|p| *p == key) {
        return Some(i + 1);
    }
    if s.requests.len() >= MAX_REQUESTS {
        s.overflow();
        return None;
    }
    s.requests.push(key);
    Some(s.requests.len())
}
fn selected(s: &State) -> Option<Ticket> {
    s.stack.last().map(|f| Ticket {
        epoch: s.epoch,
        ordinal: f.ticket,
    })
}
fn factory(s: &State) -> Option<Ticket> {
    let top = s.stack.last()?;
    if top.kind != "method" || !s.calls[top.ticket - 1].factory_active {
        return None;
    }
    Some(Ticket {
        epoch: s.epoch,
        ordinal: top.ticket,
    })
}
pub(super) fn position() -> Option<Value> {
    CAPTURE.with(|slot| {
        slot.borrow().as_ref().map(|s| {
            json!({
                "schema":"stage-block-policy-native-position-v1","last_ordinal":s.event,
                "active_stack":s.stack,"selected_factory":factory(s).map(|t|t.ordinal)
            })
        })
    })
}
pub(super) struct FrameGuard(Option<(u64, usize)>);
fn frame(ticket: Option<Ticket>, kind: &'static str) -> FrameGuard {
    if !enabled() {
        return FrameGuard(None);
    }
    let id = CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut()?;
        let ticket = ticket?;
        if !s.valid(ticket) {
            s.invalid = true;
            return None;
        }
        if s.stack.len() >= MAX_ACTIVE {
            s.overflow();
            return None;
        }
        let Some(id) = s.next_frame.checked_add(1) else {
            s.overflow();
            return None;
        };
        s.next_frame = id;
        s.stack.push(Frame {
            id,
            ticket: ticket.ordinal,
            kind,
        });
        Some((s.epoch, id))
    });
    FrameGuard(id)
}
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
            if s.stack.last().is_some_and(|f| f.id == id) {
                s.stack.pop();
            } else {
                s.invalid = true;
            }
        });
    }
}
fn logical(
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    value: impl FnOnce() -> Value,
) {
    if !enabled() || ticket.is_some_and(|t| !current_ticket(t)) {
        return;
    }
    let now = boundary(control.work);
    let value = value();
    let n_position = read_signal_policy_native_trace::position();
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else {
            return;
        };
        if ticket.is_some_and(|t| !s.valid(t)) {
            s.invalid = true;
            return;
        }
        let req = request(s, control.work);
        s.push(
            phase,
            ticket.map(|t| t.ordinal),
            req,
            json!({"boundary":now,"n_position":n_position,"data":value}),
        );
    });
}
pub(super) struct CallScope<'work> {
    pub(super) ticket: Option<Ticket>,
    control: WorkControl<'work>,
    _frame: FrameGuard,
}
pub(super) fn call<'work>(
    renderer: &StageAudio,
    control: WorkControl<'work>,
    input: SignalInput<'_, '_>,
    start: SignalSample,
    sample_len: usize,
) -> CallScope<'work> {
    let ticket = if !enabled() {
        None
    } else {
        let entry = boundary(control.work);
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let s = slot.as_mut()?;
            if s.calls.len() >= MAX_CALLS {
                s.overflow();
                return None;
            }
            let req = request(s, control.work)?;
            let ordinal = s.calls.len() + 1;
            let parent = selected(s).map(|t| t.ordinal);
            let input = match input {
                SignalInput::Signal(_) => "Signal",
                SignalInput::Tape(_) => "Tape",
                SignalInput::PreserveTape(_) => "PreserveTape",
                SignalInput::Mix(_) => "Mix",
            };
            s.calls.push(Call {
                ordinal,
                request: req,
                parent,
                input,
                start: start.0,
                sample_len,
                entry,
                method_entry: None,
                method_exit: None,
                exit: None,
                factory_entry: None,
                factory_active: false,
                factory_done: false,
                accepted_spend: [0; 2],
                accepted_host: [0; 2],
            });
            if !s.storage() {
                return None;
            }
            Some(Ticket {
                epoch: s.epoch,
                ordinal,
            })
        })
    };
    let scope = CallScope {
        ticket,
        control,
        _frame: frame(ticket, "caller"),
    };
    logical(control, ticket, "caller_enter", || {
        json!({"resident":resident(renderer,control,0),
        "origin":"BuildStageBlock only; after DSP read/check and before writer push"})
    });
    scope
}
impl Drop for CallScope<'_> {
    fn drop(&mut self) {
        if let Some(ticket) = self.ticket.filter(|t| current_ticket(*t)) {
            let now = boundary(self.control.work);
            CAPTURE.with(|slot| {
                let mut slot = slot.borrow_mut();
                let Some(s) = slot.as_mut().filter(|s| s.valid(ticket)) else {
                    return;
                };
                if s.calls[ticket.ordinal - 1].method_exit.is_none() {
                    s.invalid = true;
                }
                s.calls[ticket.ordinal - 1].exit = Some(now);
                s.storage();
            });
            logical(self.control, self.ticket, "caller_exit", || json!({}));
        }
    }
}
pub(super) struct Scope<'work> {
    control: WorkControl<'work>,
    ticket: Option<Ticket>,
    _frame: FrameGuard,
}
pub(super) fn invocation<'work>(
    control: WorkControl<'work>,
    ticket: Option<Ticket>,
) -> Scope<'work> {
    if enabled() {
        let now = boundary(control.work);
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(s) = slot.as_mut() else {
                return;
            };
            if let Some(t) = ticket.filter(|t| s.valid(*t)) {
                if s.calls[t.ordinal - 1].method_entry.is_some() {
                    s.invalid = true;
                }
                s.calls[t.ordinal - 1].method_entry = Some(now);
                s.storage();
            }
        });
    }
    let scope = Scope {
        control,
        ticket,
        _frame: frame(ticket, "method"),
    };
    logical(control, ticket, "method_enter", || json!({}));
    scope
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        if let Some(t) = self.ticket.filter(|t| current_ticket(*t)) {
            let now = boundary(self.control.work);
            CAPTURE.with(|slot| {
                let mut slot = slot.borrow_mut();
                let Some(s) = slot.as_mut().filter(|s| s.valid(t)) else {
                    return;
                };
                if s.calls[t.ordinal - 1].factory_active {
                    s.invalid = true;
                }
                let entry = s.calls[t.ordinal - 1].method_entry;
                if entry.is_none_or(|v| v.ledger != now.ledger) {
                    s.invalid = true;
                }
                s.calls[t.ordinal - 1].method_exit = Some(now);
                s.storage();
            });
            logical(
                self.control,
                self.ticket,
                "method_exit_after_owner_cleanup",
                || json!({}),
            );
        }
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
        let Some(t) = ticket.filter(|t| s.valid(*t)) else {
            return;
        };
        let c = &mut s.calls[t.ordinal - 1];
        if c.factory_active || c.factory_done || c.factory_entry.is_some() {
            s.invalid = true;
            return;
        }
        c.factory_active = true;
        c.factory_entry = Some(now);
        s.storage();
    });
    logical(control, ticket, "factory_begin", || json!({}));
}
fn view_value(view: SignalPolicyView<'_>) -> Value {
    json!({"suppressed":view.suppressed,"contents":view.contents,"lookup":view.lookup,"work":view.work,
        "suppressed_len":view.suppressed.len(),"contents_len":view.contents.len(),
        "capacity":"unexposed borrowed slices; never inferred from length"})
}
pub(super) fn factory_result(
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    result: &Result<SignalPolicyInput<'_>, StageAudioError>,
) {
    if !enabled() {
        return;
    }
    let now = boundary(control.work);
    let data = match result {
        Ok(SignalPolicyInput::Admitted(q)) => {
            json!({"status":"Ok","mode":"Admitted","view":view_value(q.view()),
            "canonical":q.canonical_units(),"additional":q.additional_units(),"total":q.work(),"forwarded":q.was_forwarded()})
        }
        Ok(SignalPolicyInput::Ordinary(q)) => {
            json!({"status":"Ok","mode":"Ordinary","suppressed":q.suppressed,
            "contents":q.contents,"lookup":q.lookup,"work":q.work,"query_local_admission":"unexposed ordinary"})
        }
        Err(e) => {
            json!({"status":"Err","error":read_signal_policy_before_trace::error(e),"query_local_work":"unexposed failed query"})
        }
    };
    let equations=CAPTURE.with(|slot|{
        let mut slot=slot.borrow_mut();let Some(s)=slot.as_mut() else{return Value::Null;};
        let Some(t)=ticket.filter(|t|s.valid(*t)) else{return Value::Null;};
        let c=&mut s.calls[t.ordinal-1];
        if !c.factory_active {s.invalid=true;return Value::Null;}
        c.factory_active=false;c.factory_done=true;
        let delta=c.factory_entry.and_then(|b|now.work[0].checked_sub(b.work[0]).zip(now.work[1].checked_sub(b.work[1]))).map(|(a,b)|[a,b]);
        let equation=(0..2).all(|i|c.accepted_spend[i].checked_add(c.accepted_host[i])==delta.map(|v|v[i]));
        let forwarding=match result {
            Ok(SignalPolicyInput::Admitted(q))=>Some(q.canonical_units().checked_add(q.additional_units())==Some(q.work())&&q.was_forwarded()&&c.accepted_spend[0].checked_add(c.accepted_spend[1])==Some(q.work())),
            _=>None,
        };
        let value=json!({"entry":c.factory_entry,"exit":now,"accepted_spend":c.accepted_spend,
            "accepted_host":c.accepted_host,"work_delta":delta,"factory_equation":equation,"forwarding_equation":forwarding});
        if !equation||forwarding==Some(false){s.invalid=true;}
        s.storage();value
    });
    logical(
        control,
        ticket,
        "factory_result",
        || json!({"result":data,"equations":equations}),
    );
}
pub(super) struct OwnerView<'value, 'work> {
    control: WorkControl<'work>,
    ticket: Option<Ticket>,
    policy: &'value SignalPolicyInput<'work>,
    view: Option<SignalPolicyView<'value>>,
}
pub(super) fn owner<'value, 'work>(
    control: WorkControl<'work>,
    ticket: Option<Ticket>,
    policy: &'value SignalPolicyInput<'work>,
) -> OwnerView<'value, 'work> {
    let view = enabled().then(|| policy.view());
    let guard = OwnerView {
        control,
        ticket,
        policy,
        view,
    };
    guard.observe("owner_borrow_begin", policy);
    guard
}
impl OwnerView<'_, '_> {
    pub(super) fn observe(&self, phase: &'static str, policy: &SignalPolicyInput<'_>) {
        if !enabled() || !self.ticket.is_some_and(current_ticket) {
            return;
        }
        let current = policy.view();
        let same = self.view.as_ref().is_some_and(|v| {
            std::ptr::eq(self.policy, policy)
                && std::ptr::eq(v.suppressed, current.suppressed)
                && std::ptr::eq(v.contents, current.contents)
                && v.lookup == current.lookup
                && v.work == current.work
        });
        if !same {
            CAPTURE.with(|slot| {
                if let Some(s) = slot.borrow_mut().as_mut() {
                    s.invalid = true;
                }
            });
        }
        logical(
            self.control,
            self.ticket,
            phase,
            || json!({"same_owner_and_borrowed_slices":same,"view":view_value(current)}),
        );
    }
}
impl Drop for OwnerView<'_, '_> {
    fn drop(&mut self) {
        self.observe("owner_borrow_end", self.policy);
    }
}
pub(super) fn disposed(control: WorkControl<'_>, ticket: Option<Ticket>, phase: &'static str) {
    logical(
        control,
        ticket,
        phase,
        || json!({"scope":"actual boundary around complete policy owner disposal; no native mask handoff"}),
    );
}
pub(super) fn cancel_at(control: WorkControl<'_>, ticket: Option<Ticket>, point: UsePoint) {
    if !enabled() || ticket.is_none() {
        return;
    }
    let cancel = CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut().unwrap();
        let hit = ticket.is_some_and(|t| {
            s.valid(t)
                && (s.fault == Fault::CancelAtUse(point)
                    || s.fault == Fault::CancelAtCallUse(t.ordinal, point))
        }) && !s.cancelled_at_use;
        s.cancelled_at_use |= hit;
        hit
    });
    logical(
        control,
        ticket,
        "use_point",
        || json!({"point":point,"set_real_cancellation":cancel}),
    );
    if cancel {
        control.cancelled.store(true, Ordering::Relaxed);
    }
}
pub(super) fn mark(
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    value: impl FnOnce() -> Value,
) {
    logical(control, ticket, phase, value);
}
pub(super) fn result<T>(
    control: WorkControl<'_>,
    ticket: Option<Ticket>,
    phase: &'static str,
    result: &Result<T, StageAudioError>,
) {
    logical(control, ticket, phase, || match result {
        Ok(_) => json!({"status":"Ok","success_payload":"unexposed"}),
        Err(e) => json!({"status":"Err","error":read_signal_policy_before_trace::error(e)}),
    });
}

#[derive(Clone, Copy)]
pub(super) struct Attempt {
    epoch: Option<u64>,
    ticket: Option<Ticket>,
    factory: Option<Ticket>,
    request: Option<usize>,
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
        ticket: None,
        factory: None,
        request: None,
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
    let n_position = read_signal_policy_native_trace::position();
    CAPTURE.with(|slot|{
        let mut slot=slot.borrow_mut();let Some(s)=slot.as_mut() else{return;};
        let factory=factory(s);
        let ticket=selected(s);
        let request=request(s,control.work);
        let index=usize::from(reserve);
        let ordinal=if factory.is_some(){
            let Some(n)=s.attempts[index].checked_add(1) else{s.overflow();return;};
            s.attempts[index]=n;n
        }else{0};
        let action=if factory.is_none(){Action::None}else{match (reserve,s.fault){
            (false,Fault::RefuseSpend(n))|(true,Fault::RefuseReserve(n)) if n==ordinal=>Action::Refuse,
            (false,Fault::CancelBeforeSpend(n))|(true,Fault::CancelBeforeReserve(n)) if n==ordinal=>Action::Cancel,
            _=>Action::None,
        }};
        token=Attempt{epoch:Some(s.epoch),ticket,factory,request,ordinal,amount,layout:token.layout,before:Some(before),action,reserve};
        s.push(if reserve{"reserve_attempt"}else{"spend_attempt"},ticket.map(|t|t.ordinal),request,
            json!({"attempt":ordinal,"eligible":factory.is_some(),"cause":if factory.is_some(){"SelectedFactory"}else{"Ambient"},
                "amount":amount,"layout":token.layout,"before":before,"action":action,"n_position":n_position}));
        if s.invalid {token.action=Action::None;}
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
    CAPTURE.with(|slot|{
        let mut slot=slot.borrow_mut();let Some(s)=slot.as_mut().filter(|s|s.epoch==epoch) else{return;};
        let mut valid=true;
        if token.reserve{
            if error.is_none(){
                let expected=token.layout.and_then(|v|u64::try_from(v[0]).ok()).map(|n|n.div_ceil(8));
                valid&=frames==expected&&frames.and_then(|n|before.ledger.checked_add(n))==Some(now.ledger);
            }else{valid&=before.ledger==now.ledger;}
        }else{
            let mut delta=[0;2];if error.is_none(){delta[before.lane]=token.amount;}
            valid&=(0..2).all(|i|now.work[i].checked_sub(before.work[i])==Some(delta[i]));
            if let Some(t)=token.factory.filter(|_|error.is_none()){
                match s.calls[t.ordinal-1].accepted_spend[before.lane].checked_add(token.amount){
                    Some(n)=>s.calls[t.ordinal-1].accepted_spend[before.lane]=n,
                    None=>{s.overflow();return;},
                }
            }
        }
        let injected=error.is_some_and(|e|matches!(e,StageAudioError::Limit(
            "test-only stage block policy spend refusal"|"test-only stage block policy reserve refusal")));
        s.push(if token.reserve{"reserve_result"}else{"spend_result"},token.ticket.map(|t|t.ordinal),token.request,
            json!({"attempt":token.ordinal,"eligible":token.factory.is_some(),"cause":if token.factory.is_some(){"SelectedFactory"}else{"Ambient"},
                "amount":token.amount,"layout":token.layout,"frames":frames,"before":before,"after":now,"error":error.map(native_error),
                "action":token.action,"actual_injected_refusal":injected,"equation":valid}));
        if !valid{s.invalid=true;}
    });
}
pub(super) fn spend_result(
    token: Attempt,
    control: WorkControl<'_>,
    result: &Result<(), StageAudioError>,
) {
    finish_attempt(token, control, result.as_ref().err(), None);
}
pub(super) fn reserve_result(
    token: Attempt,
    control: WorkControl<'_>,
    result: Result<u64, &StageAudioError>,
) {
    finish_attempt(token, control, result.as_ref().err().copied(), result.ok());
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
        s.push(
            "make_room_before",
            token.ticket.map(|t| t.ordinal),
            token.request,
            json!({"attempt":token.ordinal,"eligible":token.factory.is_some(),"before":before,
                "layout":token.layout,"frames":frames,"resident":components}),
        );
    });
    room
}
pub(super) fn room_result(
    room: Room,
    renderer: &StageAudio,
    control: WorkControl<'_>,
    result: &Result<(), StageAudioError>,
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
    CAPTURE.with(|slot|{
        let mut slot=slot.borrow_mut();let Some(s)=slot.as_mut().filter(|s|Some(s.epoch)==room.attempt.epoch) else{return;};
        if let (Some(t),Some(delta))=(room.attempt.factory,delta){
            for (lane,amount) in delta.into_iter().enumerate(){
                match s.calls[t.ordinal-1].accepted_host[lane].checked_add(amount){
                    Some(n)=>s.calls[t.ordinal-1].accepted_host[lane]=n,None=>{s.overflow();return;},
                }
            }
        }
        let token=room.attempt;
        s.push("make_room_after",token.ticket.map(|t|t.ordinal),token.request,
            json!({"attempt":token.ordinal,"eligible":token.factory.is_some(),"before":before,"after":now,
                "layout":token.layout,"frames":room.frames,"resident":components,"host_work_delta":delta,
                "host_native_crosscheck":cross,"error":result.as_ref().err().map(native_error)}));
        if !cross{s.invalid=true;}
    });
}
#[derive(Clone, Copy)]
pub(super) struct Mutation {
    epoch: Option<u64>,
    ticket: Option<Ticket>,
    request: Option<usize>,
    before: Option<Boundary>,
    frames: [u64; 2],
    merge: bool,
}
fn mutation(ledger: &RefCell<ReadWork>, frames: [u64; 2], merge: bool) -> Mutation {
    let mut token = Mutation {
        epoch: None,
        ticket: None,
        request: None,
        before: None,
        frames,
        merge,
    };
    if !enabled() {
        return token;
    }
    let before = boundary(ledger);
    CAPTURE.with(|slot|{
        let mut slot=slot.borrow_mut();let Some(s)=slot.as_mut() else{return;};
        token=Mutation{epoch:Some(s.epoch),ticket:selected(s),request:request(s,ledger),before:Some(before),frames,merge};
        s.push(if merge{"merge_before"}else{"release_before"},token.ticket.map(|t|t.ordinal),token.request,
            json!({"before":before,"frames":frames,"cause":"ActualLeaseMutation","scope":"no ownership inferred from frame totals"}));
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
    let now = boundary(ledger);
    let before = token.before.unwrap();
    let valid = if token.merge {
        before.ledger == now.ledger
            && token.frames[0].checked_add(token.frames[1]) == Some(target)
            && source == 0
    } else {
        before.ledger.checked_sub(token.frames[0]) == Some(now.ledger)
    };
    CAPTURE.with(|slot|{
        let mut slot=slot.borrow_mut();let Some(s)=slot.as_mut().filter(|s|s.epoch==epoch) else{return;};
        s.push(if token.merge{"merge_after"}else{"release_after"},token.ticket.map(|t|t.ordinal),token.request,
            json!({"before":before,"after":now,"frames_before":token.frames,"frames_after":[target,source],
                "equation":valid,"cause":"ActualLeaseMutation"}));
        if !valid{s.invalid=true;}
    });
}
pub(super) fn release_result(token: Mutation, ledger: &RefCell<ReadWork>) {
    finish_mutation(token, ledger, 0, 0);
}
pub(super) fn merge_result(token: Mutation, control: WorkControl<'_>, target: u64, source: u64) {
    finish_mutation(token, control.work, target, source);
}
pub(super) fn take() -> Value {
    CAPTURE.with(|slot|slot.borrow_mut().take().map_or(Value::Null,|mut s|{
        if !s.stack.is_empty()||s.calls.iter().any(|c|c.exit.is_none()||c.method_exit.is_none()||c.factory_active){s.invalid=true;}
        let calls_bytes=s.calls.iter().try_fold(0usize,|sum,c|sum.checked_add(serde_json::to_vec(c).expect("O call serializes").len()));
        let bytes=calls_bytes.and_then(|n|s.row_bytes.checked_add(n));
        if bytes.is_none_or(|n|n>MAX_BYTES){s.overflow();}
        let mut value=json!({"schema":"stage-block-policy-native-v1","fault":s.fault,"attempts":s.attempts,
            "invalid":s.invalid,"overflow":s.overflow,"dropped":s.dropped,"active_at_take":s.stack,
            "calls":s.calls,"rows":s.rows,"retained_payload_bytes":bytes,
            "request_rows":(1..=s.requests.len()).map(|id|json!({"ordinal":id,
                "identity":"private exact live-Work reference equality; numeric key unexposed and never dereferenced"})).collect::<Vec<_>>(),
            "bounds":{"rows":MAX_ROWS,"calls":MAX_CALLS,"active_frames":MAX_ACTIVE,"requests":MAX_REQUESTS,"serialized_payload_bytes":MAX_BYTES},
            "storage":"postconstruction retained calls/rows accounting plus final JSON check; not physical RSS; temporary JSON outside ledger",
            "ownership":"discard-only complete query owner; no O native mask/Box/Account/handoff",
            "ledger_scope":"exact callback-local reserve/release/merge equations and O method entry/exit restoration; ambient gaps or JointTransfer are not inferred as frees or balanced residuals",
            "request_identity_scope":"one capture must not reuse a dead Work address; no numeric key dereference or inferred retirement"});
        if serde_json::to_vec(&value).expect("O capture serializes").len()>MAX_BYTES{
            value["invalid"]=json!(true);value["overflow"]=json!(true);value["dropped"]=json!(s.dropped.saturating_add(1));
        }
        value
    }))
}

#[test]
fn stage_block_native_trace_default_off_does_not_evaluate_payload() {
    assert!(position().is_none());
    let work = RefCell::new(ReadWork::default());
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let control = WorkControl {
        work: &work,
        cancelled: &cancelled,
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
    };
    mark(control, None, "disabled", || {
        panic!("disabled O trace evaluated payload")
    });
    assert!(position().is_none());
}
#[test]
fn stage_block_native_trace_old_capture_guard_cannot_clear_new_epoch() {
    let first = begin(Fault::None);
    let old = take();
    assert_eq!(old["invalid"], false);
    let second = begin(Fault::None);
    drop(first);
    assert!(position().is_some());
    let current = take();
    assert_eq!(current["invalid"], false);
    assert_eq!(current["active_at_take"], json!([]));
    assert_eq!(current["calls"], json!([]));
    assert_eq!(current["rows"], json!([]));
    drop(second);
    assert!(position().is_none());
}
