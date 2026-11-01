//! L request-ledger observations. Test metadata is outside native admission.
use super::*;
use deadpan_plan::SignalPolicyView;
use serde_json::{Value, json};
use std::alloc::Layout;
use std::cell::Cell;
use std::sync::atomic::Ordering;

const MAX_ROWS: usize = 16384;
const MAX_SCOPES: usize = 256;
const MAX_ACTIVE: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(in crate::stages) enum ProviderTag {
    Signal,
    Tape,
    PreserveTape,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(in crate::stages) enum Role {
    Input,
    Output,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(in crate::stages) enum UsePhase {
    ContentBefore(usize),
    ContentAccepted(usize),
    GraphBefore,
    GraphDebited,
    RecursiveComplete,
    OutputComplete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(in crate::stages) enum GraphPhase {
    Begin,
    Debited,
    RecursiveComplete,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub(in crate::stages) enum Fault {
    #[default]
    None,
    RefuseSpend(usize),
    RefuseReserve(usize),
    CancelBeforeSpend(usize),
    CancelBeforeReserve(usize),
    CancelAtUseSeam(UsePhase),
}
#[derive(Clone, Copy, Default, Serialize)]
struct HostBoundary {
    units: [usize; 2],
    overflow: bool,
    epoch: Option<u64>,
}
#[derive(Clone, Copy, Serialize)]
struct Boundary {
    work: [usize; 2],
    ledger: u64,
    selected_lane: usize,
    ops: point_suppression::ops::trace::WorkBoundary,
    host: HostBoundary,
}
fn boundary(ledger: &RefCell<ReadWork>) -> Boundary {
    let (work, frames, lane) = {
        let work = ledger.borrow();
        (
            [work.plan_work, work.cache_plan_work],
            work.sound_context_frames,
            usize::from(work.cache_key_scopes > 0),
        )
    };
    let (units, overflow, epoch) = original_request::flattened_work_trace::host_eviction_boundary();
    Boundary {
        work,
        ledger: frames,
        selected_lane: lane,
        ops: point_suppression::ops::trace::work_boundary(),
        host: HostBoundary {
            units,
            overflow,
            epoch,
        },
    }
}
#[derive(Clone, Copy, Serialize)]
struct Resident {
    cached: u64,
    active: u64,
    projected: u64,
    sound_context: u64,
    owned_suppression: u64,
    pending_additional: u64,
    limit: u32,
    sum_before_pending: Option<u64>,
}
fn resident(renderer: &StageAudio, control: WorkControl<'_>, pending: u64) -> Resident {
    let (projected, sound_context) = {
        let work = control.work.borrow();
        (work.projected_resident_frames, work.sound_context_frames)
    };
    let cached = renderer.cached_resident_frames();
    let active = renderer.active_frames;
    let owned_suppression = renderer.owned_suppression.live_frames();
    let sum_before_pending = cached
        .checked_add(active)
        .and_then(|n| n.checked_add(projected))
        .and_then(|n| n.checked_add(sound_context))
        .and_then(|n| n.checked_add(owned_suppression));
    Resident {
        cached,
        active,
        projected,
        sound_context,
        owned_suppression,
        pending_additional: pending,
        limit: renderer.limits.maximum_resident_frames,
        sum_before_pending,
    }
}
#[derive(Clone, Copy, Serialize)]
struct ErrorClass {
    kind: &'static str,
    detail: Option<&'static str>,
    cancelled: bool,
}
fn error_class(error: &StageAudioError) -> ErrorClass {
    let (kind, detail) = match error {
        StageAudioError::Range => ("Range", None),
        StageAudioError::PreparationRange => ("PreparationRange", None),
        StageAudioError::ForeignDomain => ("ForeignDomain", None),
        StageAudioError::ForeignDefinition => ("ForeignDefinition", None),
        StageAudioError::InvalidLimits => ("InvalidLimits", None),
        StageAudioError::Limit(message) => ("Limit", Some(*message)),
        StageAudioError::Timeout => ("Timeout", None),
        StageAudioError::Unsupported(message) => ("Unsupported", Some(*message)),
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
    ErrorClass {
        kind,
        detail,
        cancelled: error.is_cancelled(),
    }
}
#[derive(Clone, Copy, Serialize)]
enum NativeKind {
    SpendAttempt,
    SpendResult,
    ReserveAttempt,
    ReserveResult,
    MakeRoomBefore,
    MakeRoomAfter,
    Release,
    Merge,
}
#[derive(Clone, Copy, Default, Serialize)]
enum Action {
    #[default]
    None,
    InjectedRefusal,
    SetRealCancellation,
}
#[derive(Clone, Copy, Serialize)]
struct NativeRow {
    ordinal: usize,
    scope: usize,
    request: usize,
    factory: bool,
    kind: NativeKind,
    attempt: usize,
    amount: usize,
    layout: Option<[usize; 2]>,
    frames: Option<u64>,
    before: Option<Boundary>,
    after: Option<Boundary>,
    error: Option<ErrorClass>,
    scheduled_action: Action,
    actual_injected_refusal: bool,
    resident: Option<Resident>,
    host_delta: Option<[usize; 2]>,
    host_crosscheck: Option<bool>,
}
#[derive(Clone, Copy, Serialize)]
struct ScopeRow {
    ordinal: usize,
    parent: Option<usize>,
    request: usize,
    provider: ProviderTag,
    role: Role,
    start: i64,
    end: i64,
    depth: usize,
    entry: Boundary,
    exit: Option<Boundary>,
    peak: u64,
    factory_entry: Option<Boundary>,
    factory_exit: Option<Boundary>,
    accepted_spend: [usize; 2],
    accepted_host: [usize; 2],
    factory_closed: bool,
    native_entry_position: usize,
    native_exit_position: Option<usize>,
}
#[derive(Clone, Copy)]
struct Active {
    row: usize,
    ledger: *const RefCell<ReadWork>,
    expected: u64,
    view: Option<(usize, usize, usize, usize)>,
    factory_active: bool,
}
struct State {
    epoch: u64,
    fault: Fault,
    native: Vec<NativeRow>,
    logical: Vec<Value>,
    scopes: Vec<ScopeRow>,
    active: Vec<Active>,
    attempts: [usize; 2],
    next_request: usize,
    request_ledger: Option<*const RefCell<ReadWork>>,
    selected_peak: u64,
    use_cancelled: bool,
    invalid: bool,
    overflow: bool,
    dropped: usize,
    foreign_request: bool,
}
thread_local! {
    static CAPTURE:RefCell<Option<State>>=const {RefCell::new(None)};
    static EPOCH:Cell<Option<u64>>=const {Cell::new(Some(0))};
}
#[must_use]
pub(in crate::stages) struct CaptureGuard(u64);
impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.as_ref().is_some_and(|s| s.epoch == self.0) {
                slot.take();
            }
        });
    }
}
pub(in crate::stages) fn begin(fault: Fault) -> CaptureGuard {
    CAPTURE.with(|slot| assert!(slot.borrow().is_none(), "L native capture already enabled"));
    let epoch = EPOCH.with(|e| {
        let n = e.get().and_then(|n| n.checked_add(1));
        e.set(n);
        n
    });
    let epoch = epoch.expect("L trace epoch overflow");
    CAPTURE.with(|slot| {
        *slot.borrow_mut() = Some(State {
            epoch,
            fault,
            native: Vec::with_capacity(MAX_ROWS),
            logical: Vec::with_capacity(MAX_ROWS),
            scopes: Vec::with_capacity(MAX_SCOPES),
            active: Vec::with_capacity(MAX_ACTIVE),
            attempts: [0; 2],
            next_request: 0,
            request_ledger: None,
            selected_peak: 0,
            use_cancelled: false,
            invalid: false,
            overflow: false,
            dropped: 0,
            foreign_request: false,
        })
    });
    CaptureGuard(epoch)
}
pub(in crate::stages) fn take() -> Value {
    let value = CAPTURE.with(|slot| slot.borrow_mut().take());
    let Some(s) = value else { return Value::Null };
    json!({"schema":"intrinsic-preflight-native-ledger-v1","epoch":s.epoch,
        "fault":s.fault,"fault_ordinal_scope":"global spend/reserve attempt across selected L scopes in this capture",
        "native_rows":s.native,"logical_rows":s.logical,"scopes":s.scopes,
        "active_scopes":s.active.len(),"attempts":s.attempts,"selected_scope_ledger_peak":s.selected_peak,
        "invalid":s.invalid || !s.active.is_empty(),"overflow":s.overflow,"dropped":s.dropped,"foreign_request":s.foreign_request,
        "claim":"actual sound_context_frames over selected L use scopes, including entry owners; not physical memory or per-lease origin",
        "capacity_scope":"active view capacities are unexposed; no native handoff/capacity extraction",
        "metadata":"trace buffers and borrowed logical snapshots outside admitted ledger",
        "host_counter_scope":"native ops enabled+epoch; eviction counter always updated with epoch; no resets by this observer"})
}
fn enabled() -> bool {
    CAPTURE.with(|slot| slot.borrow().is_some())
}
fn selected(ledger: &RefCell<ReadWork>) -> bool {
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else { return false };
        let Some(active) = s.active.last() else {
            return false;
        };
        if !std::ptr::eq(active.ledger, ledger) {
            s.invalid = true;
            s.foreign_request = true;
            return false;
        }
        true
    })
}
fn coherent(a: Boundary, b: Boundary) -> bool {
    a.ops.enabled
        && b.ops.enabled
        && !a.ops.overflow
        && !b.ops.overflow
        && !a.host.overflow
        && !b.host.overflow
        && a.ops.reset_epoch.is_some()
        && a.ops.reset_epoch == b.ops.reset_epoch
        && a.host.epoch.is_some()
        && a.host.epoch == b.host.epoch
}
fn inspect(s: &mut State, now: Boundary) {
    if let Some(active) = s.active.last()
        && active.expected != now.ledger
    {
        s.invalid = true;
    }
    for active in &s.active {
        let row = &mut s.scopes[active.row];
        row.peak = row.peak.max(now.ledger);
        if !coherent(row.entry, now) {
            s.invalid = true;
        }
    }
    s.selected_peak = s.selected_peak.max(now.ledger);
}
fn set_expected(s: &mut State, frames: u64) {
    for active in &mut s.active {
        active.expected = frames;
    }
}
fn push_native(s: &mut State, mut row: NativeRow) {
    if s.native.len() == MAX_ROWS {
        s.overflow = true;
        s.dropped += 1;
        return;
    }
    row.ordinal = s.native.len() + 1;
    s.native.push(row);
}
fn push_logical(s: &mut State, value: Value) {
    if s.logical.len() == MAX_ROWS {
        s.overflow = true;
        s.dropped += 1;
        return;
    }
    s.logical.push(value);
}
#[must_use]
pub(in crate::stages) struct Scope<'work> {
    control: WorkControl<'work>,
    epoch: Option<u64>,
    ordinal: usize,
}
pub(in crate::stages) fn scope<'work>(
    control: WorkControl<'work>,
    provider: ProviderTag,
    role: Role,
    range: Range<SignalSample>,
    depth: usize,
) -> Scope<'work> {
    let mut guard = Scope {
        control,
        epoch: None,
        ordinal: 0,
    };
    if !enabled() {
        return guard;
    }
    let entry = boundary(control.work);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else { return };
        if s.active.len() == MAX_ACTIVE || s.scopes.len() == MAX_SCOPES {
            s.overflow = true;
            s.dropped += 1;
            return;
        }
        if let Some(parent) = s.active.last() {
            if !std::ptr::eq(parent.ledger, control.work) {
                s.foreign_request = true;
                s.invalid = true;
                return;
            }
            inspect(s, entry);
        } else {
            if !s
                .request_ledger
                .is_some_and(|p| std::ptr::eq(p, control.work))
            {
                s.next_request += 1;
                s.request_ledger = Some(control.work);
            }
            s.selected_peak = s.selected_peak.max(entry.ledger);
        }
        let ordinal = s.scopes.len() + 1;
        let parent = s.active.last().map(|a| s.scopes[a.row].ordinal);
        s.scopes.push(ScopeRow {
            ordinal,
            parent,
            request: s.next_request,
            provider,
            role,
            start: range.start.0,
            end: range.end.0,
            depth,
            entry,
            exit: None,
            peak: entry.ledger,
            factory_entry: None,
            factory_exit: None,
            accepted_spend: [0; 2],
            accepted_host: [0; 2],
            factory_closed: false,
            native_entry_position: s.native.len(),
            native_exit_position: None,
        });
        s.active.push(Active {
            row: ordinal - 1,
            ledger: control.work,
            expected: entry.ledger,
            view: None,
            factory_active: false,
        });
        guard.epoch = Some(s.epoch);
        guard.ordinal = ordinal;
    });
    guard
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        let Some(epoch) = self.epoch else { return };
        let now = boundary(self.control.work);
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(s) = slot.as_mut() else { return };
            if s.epoch != epoch {
                s.invalid = true;
                return;
            }
            inspect(s, now);
            match s.active.pop() {
                Some(active) if s.scopes[active.row].ordinal == self.ordinal => {
                    if active.factory_active {
                        s.invalid = true;
                    }
                    s.scopes[active.row].exit = Some(now);
                    s.scopes[active.row].native_exit_position = Some(s.native.len());
                }
                _ => {
                    s.invalid = true;
                }
            }
        });
    }
}
fn seam(control: WorkControl<'_>, kind: &'static str, value: impl FnOnce() -> Value) {
    if !selected(control.work) {
        return;
    }
    let now = boundary(control.work);
    CAPTURE.with(|slot| {
        let mut slot=slot.borrow_mut();
        let Some(s)=slot.as_mut() else {return};
        inspect(s,now);
        let active=s.active.last().unwrap();
        let row=s.scopes[active.row];
        let position=s.native.len();
        let logical_ordinal=s.logical.len()+1;
        let attempts=s.attempts;
        push_logical(s,json!({"scope":row.ordinal,"request":row.request,"phase":kind,"boundary":now,
            "native_position":position,"logical_ordinal":logical_ordinal,"attempts":attempts,"value":value()}));
    });
}
pub(in crate::stages) fn factory_begin(control: WorkControl<'_>) {
    if !selected(control.work) {
        return;
    }
    let now = boundary(control.work);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut().unwrap();
        inspect(s, now);
        let active = s.active.last_mut().unwrap();
        if active.factory_active {
            s.invalid = true
        }
        active.factory_active = true;
        if s.scopes[active.row].factory_entry.is_some() {
            s.invalid = true;
        }
        s.scopes[active.row].factory_entry = Some(now);
    });
    seam(control, "factory_begin", || Value::Null);
}
fn view_value(view: SignalPolicyView<'_>) -> Value {
    let SignalPolicyView {
        suppressed,
        contents,
        lookup,
        work,
    } = view;
    json!({"suppressed":suppressed,"contents":contents,"lookup":lookup,"work":work,
        "suppressed_len":suppressed.len(),"contents_len":contents.len(),"capacities":"unexposed borrowed slices"})
}
fn close_factory(control: WorkControl<'_>) {
    if !selected(control.work) {
        return;
    }
    let now = boundary(control.work);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut().unwrap();
        inspect(s, now);
        let active = s.active.last_mut().unwrap();
        if !active.factory_active {
            s.invalid = true;
        }
        active.factory_active = false;
        s.scopes[active.row].factory_exit = Some(now);
        s.scopes[active.row].factory_closed = true;
    });
}
#[derive(Clone, Copy, Serialize)]
struct FactoryTotals {
    entry: Boundary,
    observed: Boundary,
    accepted_spend: [usize; 2],
    accepted_host: [usize; 2],
    accepted_native_delta_exact: bool,
}
fn factory_totals(control: WorkControl<'_>) -> Option<FactoryTotals> {
    if !selected(control.work) {
        return None;
    }
    let observed = boundary(control.work);
    CAPTURE.with(|slot| {
        let slot = slot.borrow();
        let s = slot.as_ref()?;
        let active = s.active.last()?;
        let row = s.scopes[active.row];
        let entry = row.factory_entry?;
        let exact = (0..2).all(|lane| {
            row.accepted_spend[lane].checked_add(row.accepted_host[lane])
                == observed.work[lane].checked_sub(entry.work[lane])
        });
        Some(FactoryTotals {
            entry,
            observed,
            accepted_spend: row.accepted_spend,
            accepted_host: row.accepted_host,
            accepted_native_delta_exact: exact,
        })
    })
}
fn remember_view(control: WorkControl<'_>, view: SignalPolicyView<'_>) {
    if !selected(control.work) {
        return;
    }
    let identity = (
        view.suppressed.as_ptr() as usize,
        view.suppressed.len(),
        view.contents.as_ptr() as usize,
        view.contents.len(),
    );
    CAPTURE.with(|slot| {
        if let Some(s) = slot.borrow_mut().as_mut()
            && let Some(active) = s.active.last_mut()
        {
            active.view = Some(identity);
        }
    });
}
pub(in crate::stages) fn factory_success(
    control: WorkControl<'_>,
    view: SignalPolicyView<'_>,
    canonical: usize,
    additional: usize,
    total: usize,
    forwarded: bool,
) {
    remember_view(control, view);
    let totals = factory_totals(control);
    let exact = totals.map(|t| {
        t.accepted_native_delta_exact
            && forwarded
            && canonical.checked_add(additional) == Some(total)
            && view.work == total
            && t.accepted_spend[0].checked_add(t.accepted_spend[1]) == Some(total)
    });
    if exact == Some(false) {
        CAPTURE.with(|slot| {
            if let Some(s) = slot.borrow_mut().as_mut() {
                s.invalid = true;
            }
        });
    }
    seam(control, "factory_success", || {
        json!({"view":view_value(view),"canonical":canonical,
        "additional":additional,"total":total,"forwarded":forwarded,
        "native_totals":totals,"actual_success_equations_exact":exact})
    });
    close_factory(control);
}
pub(in crate::stages) fn factory_ordinary(control: WorkControl<'_>, view: SignalPolicyView<'_>) {
    remember_view(control, view);
    seam(control, "factory_ordinary", || {
        json!({"view":view_value(view),"forwarded":false,
        "accepted_spend_equation":"not claimed for ordinary legacy"})
    });
    close_factory(control);
}
pub(in crate::stages) fn factory_error(control: WorkControl<'_>, error: &StageAudioError) {
    let totals = factory_totals(control);
    if totals.is_some_and(|t| !t.accepted_native_delta_exact) {
        CAPTURE.with(|slot| {
            if let Some(s) = slot.borrow_mut().as_mut() {
                s.invalid = true;
            }
        });
    }
    seam(control, "factory_error", || {
        json!({"kind":error_class(error),"display":error.to_string(),
        "debug":format!("{error:?}"),"canonical_additional_split":"unexposed on failed factory",
        "native_accepted_prefix":totals})
    });
    close_factory(control);
}
pub(in crate::stages) fn use_view(
    control: WorkControl<'_>,
    phase: UsePhase,
    view: SignalPolicyView<'_>,
) {
    if !selected(control.work) {
        return;
    }
    let identity = (
        view.suppressed.as_ptr() as usize,
        view.suppressed.len(),
        view.contents.as_ptr() as usize,
        view.contents.len(),
    );
    let (same, cancel) = CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut().unwrap();
        let active = s.active.last_mut().unwrap();
        let same = active.view.map(|old| old == identity);
        active.view = Some(identity);
        let cancel = !s.use_cancelled && s.fault == Fault::CancelAtUseSeam(phase);
        s.use_cancelled |= cancel;
        (same, cancel)
    });
    seam(control, "use", || {
        json!({"phase":phase,"view":view_value(view),"same_local_view_identity":same,
        "action":if cancel {"set_real_cancellation"} else {"none"}})
    });
    if cancel {
        control.cancelled.store(true, Ordering::Relaxed);
    }
}
pub(in crate::stages) fn mark(control: WorkControl<'_>, phase: GraphPhase) {
    seam(
        control,
        "ordinary_graph_interval",
        || json!({"phase":phase,"scope":"manual/recursive native work, not factory canonical residual"}),
    );
}
#[derive(Clone, Copy)]
pub(in crate::stages) struct Attempt {
    epoch: Option<u64>,
    scope: usize,
    request: usize,
    ordinal: usize,
    amount: usize,
    layout: Option<[usize; 2]>,
    before: Option<Boundary>,
    action: Action,
    reserve: bool,
}
impl Attempt {
    pub(in crate::stages) fn apply_cancellation(self, control: WorkControl<'_>) {
        if matches!(self.action, Action::SetRealCancellation) {
            control.cancelled.store(true, Ordering::Relaxed);
        }
    }
    pub(in crate::stages) fn injected_refusal(self) -> bool {
        matches!(self.action, Action::InjectedRefusal)
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
        scope: 0,
        request: 0,
        ordinal: 0,
        amount,
        layout: layout.map(|v| [v.size(), v.align()]),
        before: None,
        action: Action::None,
        reserve,
    };
    if !selected(control.work) {
        return token;
    }
    let before = boundary(control.work);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut().unwrap();
        inspect(s, before);
        let index = usize::from(reserve);
        let Some(ordinal) = s.attempts[index].checked_add(1) else {
            s.overflow = true;
            return;
        };
        s.attempts[index] = ordinal;
        let action = match (reserve, s.fault) {
            (false, Fault::RefuseSpend(n)) | (true, Fault::RefuseReserve(n)) if n == ordinal => {
                Action::InjectedRefusal
            }
            (false, Fault::CancelBeforeSpend(n)) | (true, Fault::CancelBeforeReserve(n))
                if n == ordinal =>
            {
                Action::SetRealCancellation
            }
            _ => Action::None,
        };
        let active = *s.active.last().unwrap();
        let row = s.scopes[active.row];
        token = Attempt {
            epoch: Some(s.epoch),
            scope: row.ordinal,
            request: row.request,
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
                scope: row.ordinal,
                request: row.request,
                factory: active.factory_active,
                kind: if reserve {
                    NativeKind::ReserveAttempt
                } else {
                    NativeKind::SpendAttempt
                },
                attempt: ordinal,
                amount,
                layout: token.layout,
                frames: None,
                before: Some(before),
                after: None,
                error: None,
                scheduled_action: action,
                actual_injected_refusal: false,
                resident: None,
                host_delta: None,
                host_crosscheck: None,
            },
        );
    });
    token
}
pub(in crate::stages) fn spend_begin(control: WorkControl<'_>, amount: usize) -> Attempt {
    attempt(control, amount, None, false)
}
pub(in crate::stages) fn reserve_begin(control: WorkControl<'_>, layout: Layout) -> Attempt {
    attempt(control, 0, Some(layout), true)
}
fn result_row(
    token: Attempt,
    control: WorkControl<'_>,
    error: Option<&StageAudioError>,
    frames: Option<u64>,
    accepted_reserve: bool,
) {
    let Some(epoch) = token.epoch else { return };
    let now = boundary(control.work);
    let before = token.before.unwrap();
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else { return };
        if s.epoch != epoch {
            s.invalid = true;
            return;
        }
        if accepted_reserve {
            let expected = s.active.last().map(|a| a.expected);
            if expected.zip(frames).and_then(|(n, f)| n.checked_add(f)) != Some(now.ledger) {
                s.invalid = true;
            }
            set_expected(s, now.ledger);
        }
        inspect(s, now);
        let Some(active) = s.active.last().copied() else {
            s.invalid = true;
            return;
        };
        if s.scopes[active.row].ordinal != token.scope {
            s.invalid = true;
        }
        if !token.reserve {
            let expected = if error.is_none() { token.amount } else { 0 };
            let mut deltas = [0; 2];
            deltas[before.selected_lane] = expected;
            if now.work[0].checked_sub(before.work[0]) != Some(deltas[0])
                || now.work[1].checked_sub(before.work[1]) != Some(deltas[1])
            {
                s.invalid = true;
            }
            if error.is_none() && active.factory_active {
                let row = &mut s.scopes[active.row];
                match row.accepted_spend[before.selected_lane].checked_add(token.amount) {
                    Some(next) => row.accepted_spend[before.selected_lane] = next,
                    None => s.overflow = true,
                }
            }
        }
        push_native(
            s,
            NativeRow {
                ordinal: 0,
                scope: token.scope,
                request: token.request,
                factory: active.factory_active,
                kind: if token.reserve {
                    NativeKind::ReserveResult
                } else {
                    NativeKind::SpendResult
                },
                attempt: token.ordinal,
                amount: token.amount,
                layout: token.layout,
                frames,
                before: Some(before),
                after: Some(now),
                error: error.map(error_class),
                scheduled_action: token.action,
                actual_injected_refusal: error.is_some_and(|e| {
                    matches!(
                        e,
                        StageAudioError::Limit(
                            "test-only intrinsic policy spend refusal"
                                | "test-only intrinsic policy reserve refusal"
                        )
                    )
                }),
                resident: None,
                host_delta: None,
                host_crosscheck: None,
            },
        );
    });
}
pub(in crate::stages) fn spend_result(
    token: Attempt,
    control: WorkControl<'_>,
    result: &Result<(), StageAudioError>,
) {
    result_row(token, control, result.as_ref().err(), None, false);
}
pub(in crate::stages) fn reserve_result(
    token: Attempt,
    control: WorkControl<'_>,
    result: Result<u64, &StageAudioError>,
) {
    result_row(
        token,
        control,
        result.as_ref().err().copied(),
        result.ok(),
        result.is_ok(),
    );
}
#[derive(Clone, Copy)]
pub(in crate::stages) struct Room {
    attempt: Attempt,
    before: Option<Boundary>,
    frames: u64,
}
pub(in crate::stages) fn room_begin(
    token: Attempt,
    renderer: &StageAudio,
    control: WorkControl<'_>,
    frames: u64,
) -> Room {
    let mut room = Room {
        attempt: token,
        before: None,
        frames,
    };
    if token.epoch.is_none() {
        return room;
    }
    let before = boundary(control.work);
    let components = resident(renderer, control, frames);
    room.before = Some(before);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else { return };
        if Some(s.epoch) != token.epoch {
            s.invalid = true;
            return;
        }
        inspect(s, before);
        let factory = s.active.last().is_some_and(|v| v.factory_active);
        push_native(
            s,
            NativeRow {
                ordinal: 0,
                scope: token.scope,
                request: token.request,
                factory,
                kind: NativeKind::MakeRoomBefore,
                attempt: token.ordinal,
                amount: 0,
                layout: token.layout,
                frames: Some(frames),
                before: Some(before),
                after: None,
                error: None,
                scheduled_action: Action::None,
                actual_injected_refusal: false,
                resident: Some(components),
                host_delta: None,
                host_crosscheck: None,
            },
        );
    });
    room
}
pub(in crate::stages) fn room_result(
    room: Room,
    renderer: &StageAudio,
    control: WorkControl<'_>,
    result: &Result<(), StageAudioError>,
) {
    let Some(before) = room.before else { return };
    let now = boundary(control.work);
    let components = resident(renderer, control, room.frames);
    let delta = now.work[0]
        .checked_sub(before.work[0])
        .zip(now.work[1].checked_sub(before.work[1]))
        .map(|(a, b)| [a, b]);
    let mut cross = coherent(before, now);
    for lane in 0..2 {
        let native = now.ops.lanes[lane][7].checked_sub(before.ops.lanes[lane][7]);
        let host = now.host.units[lane].checked_sub(before.host.units[lane]);
        cross &= native.zip(host).and_then(|(a, b)| a.checked_add(b)) == delta.map(|v| v[lane]);
        for phase in 0..7 {
            cross &= before.ops.lanes[lane][phase] == now.ops.lanes[lane][phase];
        }
    }
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else { return };
        if Some(s.epoch) != room.attempt.epoch {
            s.invalid = true;
            return;
        }
        inspect(s, now);
        if !cross {
            s.invalid = true;
        }
        let active = s.active.last().copied();
        if let (Some(active), Some(delta)) = (active, delta)
            && active.factory_active
        {
            for (lane, units) in delta.into_iter().enumerate() {
                match s.scopes[active.row].accepted_host[lane].checked_add(units) {
                    Some(next) => s.scopes[active.row].accepted_host[lane] = next,
                    None => s.overflow = true,
                }
            }
        }
        push_native(
            s,
            NativeRow {
                ordinal: 0,
                scope: room.attempt.scope,
                request: room.attempt.request,
                factory: active.is_some_and(|v| v.factory_active),
                kind: NativeKind::MakeRoomAfter,
                attempt: room.attempt.ordinal,
                amount: 0,
                layout: room.attempt.layout,
                frames: Some(room.frames),
                before: Some(before),
                after: Some(now),
                error: result.as_ref().err().map(error_class),
                scheduled_action: Action::None,
                actual_injected_refusal: false,
                resident: Some(components),
                host_delta: delta,
                host_crosscheck: Some(cross),
            },
        );
    });
}
pub(in crate::stages) fn released(ledger: &RefCell<ReadWork>, frames: u64) {
    if !selected(ledger) {
        return;
    }
    let now = boundary(ledger);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut().unwrap();
        let active = *s.active.last().unwrap();
        let expected = active.expected;
        if expected.checked_sub(frames) != Some(now.ledger) {
            s.invalid = true;
        }
        set_expected(s, now.ledger);
        inspect(s, now);
        let row = s.scopes[active.row];
        push_native(
            s,
            NativeRow {
                ordinal: 0,
                scope: row.ordinal,
                request: row.request,
                factory: active.factory_active,
                kind: NativeKind::Release,
                attempt: 0,
                amount: 0,
                layout: None,
                frames: Some(frames),
                before: None,
                after: Some(now),
                error: None,
                scheduled_action: Action::None,
                actual_injected_refusal: false,
                resident: None,
                host_delta: None,
                host_crosscheck: None,
            },
        );
    });
}
pub(in crate::stages) fn merged(control: WorkControl<'_>) {
    if !selected(control.work) {
        return;
    }
    let now = boundary(control.work);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut().unwrap();
        inspect(s, now);
        let active = *s.active.last().unwrap();
        let row = s.scopes[active.row];
        push_native(
            s,
            NativeRow {
                ordinal: 0,
                scope: row.ordinal,
                request: row.request,
                factory: active.factory_active,
                kind: NativeKind::Merge,
                attempt: 0,
                amount: 0,
                layout: None,
                frames: None,
                before: None,
                after: Some(now),
                error: None,
                scheduled_action: Action::None,
                actual_injected_refusal: false,
                resident: None,
                host_delta: None,
                host_crosscheck: None,
            },
        );
    });
}
