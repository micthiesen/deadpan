//! M output-validation observations. Witness storage is outside native admission.
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
pub(super) enum ProviderTag {
    Signal,
    Tape,
    PreserveTape,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(super) enum Role {
    Output,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(super) enum UsePhase {
    PolicyReady,
    DebitBefore,
    Debited,
    ContentBefore(usize),
    ContentAccepted(usize),
    ProgressBefore,
    ProgressAfter,
    IterationComplete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(super) enum Mark {
    CheckBefore,
    CheckAccepted,
    OwnerDropBefore,
    OwnerDropped,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub(super) enum Fault {
    #[default]
    None,
    RefuseSpend(usize),
    RefuseReserve(usize),
    CancelBeforeSpend(usize),
    CancelBeforeReserve(usize),
    CancelAtUseSeam(UsePhase),
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
#[derive(Clone, Copy, Default, Serialize)]
enum Action {
    #[default]
    None,
    Refuse,
    Cancel,
}
#[derive(Clone, Copy, Serialize)]
struct NativeRow {
    ordinal: usize,
    scope: usize,
    request: usize,
    factory: bool,
    kind: Kind,
    attempt: usize,
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
    merge_frames: Option<[u64; 2]>,
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
    native_entry_position: usize,
    native_exit_position: Option<usize>,
}
#[derive(Clone, Copy)]
struct Active {
    scope: usize,
    ledger: *const RefCell<ReadWork>,
    expected: u64,
    view: Option<(usize, usize, usize, usize)>,
    factory: bool,
}
struct State {
    epoch: u64,
    fault: Fault,
    attempts: [usize; 2],
    scopes: Vec<ScopeRow>,
    active: Vec<Active>,
    native: Vec<NativeRow>,
    logical: Vec<Value>,
    ledger: Option<*const RefCell<ReadWork>>,
    peak: u64,
    use_cancelled: bool,
    invalid: bool,
    overflow: bool,
    dropped: usize,
    foreign_request: bool,
}
thread_local! {
    static CAPTURE:RefCell<Option<State>>=const {RefCell::new(None)};
    static EPOCH:Cell<u64>=const {Cell::new(0)};
}
#[must_use]
pub(super) struct CaptureGuard(u64);
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
pub(super) fn begin(fault: Fault) -> CaptureGuard {
    let epoch = EPOCH.with(|e| {
        let n = e
            .get()
            .checked_add(1)
            .expect("M native trace epoch overflow");
        e.set(n);
        n
    });
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(slot.is_none(), "M native capture already active");
        *slot = Some(State {
            epoch,
            fault,
            attempts: [0; 2],
            scopes: Vec::with_capacity(MAX_SCOPES),
            active: Vec::with_capacity(MAX_ACTIVE),
            native: Vec::with_capacity(MAX_ROWS),
            logical: Vec::with_capacity(MAX_ROWS),
            ledger: None,
            peak: 0,
            use_cancelled: false,
            invalid: false,
            overflow: false,
            dropped: 0,
            foreign_request: false,
        });
    });
    CaptureGuard(epoch)
}
pub(super) fn take() -> Value {
    CAPTURE.with(|slot|slot.borrow_mut().take()).map_or(Value::Null,|s|json!({
        "schema":"stage-validation-native-ledger-v1","epoch":s.epoch,"fault":s.fault,
        "fault_ordinal_scope":"separate spend and reserve ordinal namespaces, each global across selected M output chunks in one fresh request",
        "attempts":s.attempts,"native_rows":s.native,"logical_rows":s.logical,"scopes":s.scopes,
        "active_scopes":s.active.len(),"invalid":s.invalid||!s.active.is_empty(),"overflow":s.overflow,
        "dropped":s.dropped,"foreign_request":s.foreign_request,"selected_scope_ledger_peak":s.peak,
        "claim":"actual request ledger through M output content/progress only; no selected owner into later child PCM/key scan",
        "capacity_scope":"borrowed slice capacities, private leases and native handoff unexposed",
        "metadata":"test witness storage outside native accounting; ledger identity equality only, no serialized addresses",
        "host_counter_scope":"actual make_room intervals crosschecked against existing native and host counters, never residual canonical attribution"
    }))
}
fn selected(ledger: &RefCell<ReadWork>) -> bool {
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else { return false };
        let Some(a) = s.active.last() else {
            return false;
        };
        if !std::ptr::eq(a.ledger, ledger) {
            s.invalid = true;
            s.foreign_request = true;
            return false;
        }
        true
    })
}
fn inspect(s: &mut State, now: Boundary) {
    if s.active.last().is_some_and(|a| a.expected != now.ledger) {
        s.invalid = true;
    }
    for active in &s.active {
        let row = &mut s.scopes[active.scope - 1];
        row.peak = row.peak.max(now.ledger);
        if !coherent(row.entry, now) {
            s.invalid = true;
        }
    }
    s.peak = s.peak.max(now.ledger);
}
fn expected(s: &mut State, now: u64) {
    for a in &mut s.active {
        a.expected = now;
    }
}
fn push(s: &mut State, mut row: NativeRow) {
    if s.native.len() == MAX_ROWS {
        s.overflow = true;
        s.dropped += 1;
        return;
    }
    row.ordinal = s.native.len() + 1;
    s.native.push(row);
}
fn logical(s: &mut State, row: Value) {
    if s.logical.len() == MAX_ROWS {
        s.overflow = true;
        s.dropped += 1;
        return;
    }
    s.logical.push(row);
}
pub(super) fn position() -> Option<Value> {
    CAPTURE.with(|slot| {
        let slot=slot.borrow();
        let s=slot.as_ref()?;
        let a=s.active.last()?;
        let row=s.scopes[a.scope-1];
        Some(json!({"scope":row.ordinal,"parent":row.parent,"request":row.request,
            "provider":row.provider,"role":row.role,"start":row.start,"end":row.end,"depth":row.depth,
            "factory_active":a.factory,"native_position":s.native.len(),"logical_position":s.logical.len()}))
    })
}
fn seam(control: WorkControl<'_>, phase: &'static str, value: impl FnOnce() -> Value) {
    if !selected(control.work) {
        return;
    }
    let now = boundary(control.work);
    let value = value();
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut().unwrap();
        inspect(s, now);
        let a = *s.active.last().unwrap();
        let row = s.scopes[a.scope - 1];
        let ordinal = s.logical.len() + 1;
        let native_position = s.native.len();
        logical(
            s,
            json!({"ordinal":ordinal,"scope":row.ordinal,"request":row.request,"phase":phase,
            "boundary":now,"native_position":native_position,"value":value}),
        );
    });
}
#[must_use]
pub(super) struct Scope<'work> {
    control: WorkControl<'work>,
    epoch: Option<u64>,
    scope: usize,
}
pub(super) fn scope<'work>(
    control: WorkControl<'work>,
    provider: ProviderTag,
    role: Role,
    range: Range<SignalSample>,
    depth: usize,
) -> Scope<'work> {
    let mut guard = Scope {
        control,
        epoch: None,
        scope: 0,
    };
    if !CAPTURE.with(|slot| slot.borrow().is_some()) {
        return guard;
    }
    let entry = boundary(control.work);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut().unwrap();
        if s.active.len() == MAX_ACTIVE || s.scopes.len() == MAX_SCOPES {
            s.overflow = true;
            s.dropped += 1;
            return;
        }
        if s.ledger.is_some_and(|p| !std::ptr::eq(p, control.work)) {
            s.invalid = true;
            s.foreign_request = true;
            return;
        }
        s.ledger = Some(control.work);
        inspect(s, entry);
        let ordinal = s.scopes.len() + 1;
        let parent = s.active.last().map(|a| a.scope);
        s.scopes.push(ScopeRow {
            ordinal,
            parent,
            request: 1,
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
            native_entry_position: s.native.len(),
            native_exit_position: None,
        });
        s.active.push(Active {
            scope: ordinal,
            ledger: control.work,
            expected: entry.ledger,
            view: None,
            factory: false,
        });
        guard.epoch = Some(s.epoch);
        guard.scope = ordinal;
    });
    if guard.epoch.is_some() {
        seam(
            control,
            "scope_enter",
            || json!({"before_trace_position":stage_validation_policy_before_trace::position()}),
        );
    }
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
                Some(a) if a.scope == self.scope => {
                    if a.factory {
                        s.invalid = true;
                    }
                    let row = &mut s.scopes[a.scope - 1];
                    if now.ledger != row.entry.ledger {
                        s.invalid = true;
                    }
                    row.exit = Some(now);
                    row.native_exit_position = Some(s.native.len());
                }
                _ => s.invalid = true,
            }
        });
    }
}
fn view_value(view: SignalPolicyView<'_>) -> Value {
    json!({"suppressed":view.suppressed,"contents":view.contents,"lookup":view.lookup,"work":view.work,
        "suppressed_len":view.suppressed.len(),"contents_len":view.contents.len(),
        "capacities":"unexposed borrowed slices"})
}
fn remember(control: WorkControl<'_>, view: SignalPolicyView<'_>) -> Option<bool> {
    if !selected(control.work) {
        return None;
    }
    let identity = (
        view.suppressed.as_ptr() as usize,
        view.suppressed.len(),
        view.contents.as_ptr() as usize,
        view.contents.len(),
    );
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut().unwrap();
        let a = s.active.last_mut().unwrap();
        let same = a.view.map(|old| old == identity);
        a.view = Some(identity);
        same
    })
}
pub(super) fn factory_begin(control: WorkControl<'_>) {
    if !selected(control.work) {
        return;
    }
    let now = boundary(control.work);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut().unwrap();
        inspect(s, now);
        let a = s.active.last_mut().unwrap();
        if a.factory || s.scopes[a.scope - 1].factory_entry.is_some() {
            s.invalid = true;
        }
        a.factory = true;
        s.scopes[a.scope - 1].factory_entry = Some(now);
    });
    seam(control, "factory_begin", || Value::Null);
}
#[derive(Clone, Copy, Serialize)]
struct Totals {
    entry: Boundary,
    observed: Boundary,
    accepted_spend: [usize; 2],
    accepted_host: [usize; 2],
    exact: bool,
}
fn totals(control: WorkControl<'_>) -> Option<Totals> {
    if !selected(control.work) {
        return None;
    }
    let observed = boundary(control.work);
    CAPTURE.with(|slot| {
        let slot = slot.borrow();
        let s = slot.as_ref()?;
        let a = s.active.last()?;
        let row = s.scopes[a.scope - 1];
        let entry = row.factory_entry?;
        Some(Totals {
            entry,
            observed,
            accepted_spend: row.accepted_spend,
            accepted_host: row.accepted_host,
            exact: (0..2).all(|lane| {
                row.accepted_spend[lane].checked_add(row.accepted_host[lane])
                    == observed.work[lane].checked_sub(entry.work[lane])
            }),
        })
    })
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
        let a = s.active.last_mut().unwrap();
        if !a.factory {
            s.invalid = true;
        }
        a.factory = false;
        s.scopes[a.scope - 1].factory_exit = Some(now);
    });
}
pub(super) fn factory_success(
    control: WorkControl<'_>,
    view: SignalPolicyView<'_>,
    canonical: usize,
    additional: usize,
    total: usize,
    forwarded: bool,
) {
    remember(control, view);
    let t = totals(control);
    let exact = t.map(|t| {
        t.exact
            && forwarded
            && view.work == total
            && canonical.checked_add(additional) == Some(total)
            && t.accepted_spend[0].checked_add(t.accepted_spend[1]) == Some(total)
    });
    if exact == Some(false) {
        CAPTURE.with(|slot| slot.borrow_mut().as_mut().unwrap().invalid = true);
    }
    seam(control, "factory_success", || {
        json!({"view":view_value(view),"canonical":canonical,"additional":additional,
        "total":total,"forwarded":forwarded,"native_totals":t,"actual_success_equations_exact":exact})
    });
    close_factory(control);
}
pub(super) fn factory_ordinary(control: WorkControl<'_>, view: SignalPolicyView<'_>) {
    remember(control, view);
    seam(control, "factory_ordinary", || {
        json!({"view":view_value(view),"forwarded":false,
        "accepted_spend_equation":"not claimed for ordinary source"})
    });
    close_factory(control);
}
pub(super) fn factory_error(control: WorkControl<'_>, e: &StageAudioError) {
    let t = totals(control);
    if t.is_some_and(|v| !v.exact) {
        CAPTURE.with(|slot| slot.borrow_mut().as_mut().unwrap().invalid = true);
    }
    seam(control, "factory_error", || {
        json!({"error":stage_validation_policy_before_trace::error(e),
        "native_accepted_prefix":t,"canonical_additional_split":"unexposed on failed factory"})
    });
    close_factory(control);
}
pub(super) fn use_view(control: WorkControl<'_>, phase: UsePhase, view: SignalPolicyView<'_>) {
    if !selected(control.work) {
        return;
    }
    let same = remember(control, view);
    let cancel = CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut().unwrap();
        let cancel = !s.use_cancelled && s.fault == Fault::CancelAtUseSeam(phase);
        s.use_cancelled |= cancel;
        cancel
    });
    seam(control, "use", || {
        json!({"phase":phase,"view":view_value(view),"same_local_view_identity":same,
        "action":if cancel {"set_real_cancellation"} else {"none"}})
    });
    if cancel {
        control.cancelled.store(true, Ordering::Relaxed);
    }
}
pub(super) fn mark(control: WorkControl<'_>, phase: Mark) {
    if phase == Mark::OwnerDropped && selected(control.work) {
        let ledger = control.work.borrow().sound_context_frames;
        CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let s = slot.as_mut().unwrap();
            let a = s.active.last().unwrap();
            if ledger != s.scopes[a.scope - 1].entry.ledger {
                s.invalid = true;
            }
        });
    }
    seam(control, "mark", || json!({"phase":phase}));
}
pub(super) fn result<T>(
    control: WorkControl<'_>,
    phase: &'static str,
    result: &Result<T, StageAudioError>,
) {
    seam(control, phase, || match result {
        Ok(_) => json!({"status":"Ok","success_payload":"unexposed"}),
        Err(e) => json!({"status":"Err","error":stage_validation_policy_before_trace::error(e)}),
    });
}
#[derive(Clone, Copy)]
pub(super) struct Attempt {
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
        scope: 0,
        request: 0,
        ordinal: 0,
        amount,
        layout: layout.map(|l| [l.size(), l.align()]),
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
        let lane = usize::from(reserve);
        let Some(ordinal) = s.attempts[lane].checked_add(1) else {
            s.overflow = true;
            return;
        };
        s.attempts[lane] = ordinal;
        let action = match (reserve, s.fault) {
            (false, Fault::RefuseSpend(n)) | (true, Fault::RefuseReserve(n)) if n == ordinal => {
                Action::Refuse
            }
            (false, Fault::CancelBeforeSpend(n)) | (true, Fault::CancelBeforeReserve(n))
                if n == ordinal =>
            {
                Action::Cancel
            }
            _ => Action::None,
        };
        let a = *s.active.last().unwrap();
        let row = s.scopes[a.scope - 1];
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
        push(
            s,
            NativeRow {
                ordinal: 0,
                scope: row.ordinal,
                request: row.request,
                factory: a.factory,
                kind: if reserve {
                    Kind::ReserveAttempt
                } else {
                    Kind::SpendAttempt
                },
                attempt: ordinal,
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
        let Some(a) = s.active.last().copied() else {
            s.invalid = true;
            return;
        };
        if a.scope != token.scope {
            s.invalid = true;
        }
        if token.reserve && error.is_none() {
            let layout_frames = token
                .layout
                .and_then(|layout| u64::try_from(layout[0]).ok())
                .map(|bytes| bytes.div_ceil(8));
            if frames != layout_frames
                || frames.and_then(|f| a.expected.checked_add(f)) != Some(now.ledger)
            {
                s.invalid = true;
            }
            expected(s, now.ledger);
        }
        inspect(s, now);
        if !token.reserve {
            let mut delta = [0; 2];
            delta[before.lane] = if error.is_none() { token.amount } else { 0 };
            if (0..2).any(|i| now.work[i].checked_sub(before.work[i]) != Some(delta[i])) {
                s.invalid = true;
            }
            if error.is_none() && a.factory {
                let value = &mut s.scopes[a.scope - 1].accepted_spend[before.lane];
                match value.checked_add(token.amount) {
                    Some(n) => *value = n,
                    None => s.overflow = true,
                }
            }
        }
        push(
            s,
            NativeRow {
                ordinal: 0,
                scope: token.scope,
                request: token.request,
                factory: a.factory,
                kind: if token.reserve {
                    Kind::ReserveResult
                } else {
                    Kind::SpendResult
                },
                attempt: token.ordinal,
                amount: token.amount,
                layout: token.layout,
                frames,
                before,
                after: Some(now),
                error: error.map(native_error),
                action: token.action,
                actual_injected_refusal: error.is_some_and(|e| {
                    matches!(
                        e,
                        StageAudioError::Limit(
                            "test-only stage validation policy spend refusal"
                                | "test-only stage validation policy reserve refusal"
                        )
                    )
                }),
                resident: None,
                host_delta: None,
                host_crosscheck: None,
                merge_frames: None,
            },
        );
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
        let Some(s) = slot.as_mut() else { return };
        if Some(s.epoch) != token.epoch {
            s.invalid = true;
            return;
        }
        inspect(s, before);
        let factory = s.active.last().is_some_and(|a| a.factory);
        push(
            s,
            NativeRow {
                ordinal: 0,
                scope: token.scope,
                request: token.request,
                factory,
                kind: Kind::MakeRoomBefore,
                attempt: token.ordinal,
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
    });
    room
}
pub(super) fn room_result(
    room: Room,
    renderer: &StageAudio,
    control: WorkControl<'_>,
    r: &Result<(), StageAudioError>,
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
        let host = now.host_units[lane].checked_sub(before.host_units[lane]);
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
        let a = s.active.last().copied();
        if let (Some(a), Some(delta)) = (a, delta)
            && a.factory
        {
            for (lane, units) in delta.into_iter().enumerate() {
                let value = &mut s.scopes[a.scope - 1].accepted_host[lane];
                match value.checked_add(units) {
                    Some(n) => *value = n,
                    None => s.overflow = true,
                }
            }
        }
        push(
            s,
            NativeRow {
                ordinal: 0,
                scope: room.attempt.scope,
                request: room.attempt.request,
                factory: a.is_some_and(|a| a.factory),
                kind: Kind::MakeRoomAfter,
                attempt: room.attempt.ordinal,
                amount: 0,
                layout: room.attempt.layout,
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
    });
}
#[derive(Clone, Copy)]
pub(super) struct Mutation {
    epoch: Option<u64>,
    scope: usize,
    before: Option<Boundary>,
    values: [u64; 2],
    merge: bool,
}
fn mutation(ledger: &RefCell<ReadWork>, values: [u64; 2], merge: bool) -> Mutation {
    let mut token = Mutation {
        epoch: None,
        scope: 0,
        before: None,
        values,
        merge,
    };
    if !selected(ledger) {
        return token;
    }
    let before = boundary(ledger);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut().unwrap();
        inspect(s, before);
        let a = *s.active.last().unwrap();
        token = Mutation {
            epoch: Some(s.epoch),
            scope: a.scope,
            before: Some(before),
            values,
            merge,
        };
        push(
            s,
            NativeRow {
                ordinal: 0,
                scope: a.scope,
                request: 1,
                factory: a.factory,
                kind: if merge {
                    Kind::MergeBefore
                } else {
                    Kind::ReleaseBefore
                },
                attempt: 0,
                amount: 0,
                layout: None,
                frames: if merge { None } else { Some(values[0]) },
                before,
                after: None,
                error: None,
                action: Action::None,
                actual_injected_refusal: false,
                resident: None,
                host_delta: None,
                host_crosscheck: None,
                merge_frames: if merge { Some(values) } else { None },
            },
        );
    });
    token
}
pub(super) fn release_begin(ledger: &RefCell<ReadWork>, frames: u64) -> Mutation {
    mutation(ledger, [frames, 0], false)
}
pub(super) fn merge_begin(control: WorkControl<'_>, target: u64, source: u64) -> Mutation {
    mutation(control.work, [target, source], true)
}
fn mutation_result(token: Mutation, ledger: &RefCell<ReadWork>, values: [u64; 2]) {
    let Some(epoch) = token.epoch else { return };
    let before = token.before.unwrap();
    let now = boundary(ledger);
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else { return };
        if s.epoch != epoch {
            s.invalid = true;
            return;
        }
        let Some(a) = s.active.last().copied() else {
            s.invalid = true;
            return;
        };
        if a.scope != token.scope {
            s.invalid = true;
        }
        if token.merge {
            if now.ledger != before.ledger
                || token.values[0].checked_add(token.values[1]) != Some(values[0])
                || values[1] != 0
            {
                s.invalid = true;
            }
        } else {
            if a.expected.checked_sub(token.values[0]) != Some(now.ledger) {
                s.invalid = true;
            }
            expected(s, now.ledger);
        }
        inspect(s, now);
        push(
            s,
            NativeRow {
                ordinal: 0,
                scope: a.scope,
                request: 1,
                factory: a.factory,
                kind: if token.merge {
                    Kind::MergeAfter
                } else {
                    Kind::ReleaseAfter
                },
                attempt: 0,
                amount: 0,
                layout: None,
                frames: if token.merge {
                    None
                } else {
                    Some(token.values[0])
                },
                before,
                after: Some(now),
                error: None,
                action: Action::None,
                actual_injected_refusal: false,
                resident: None,
                host_delta: None,
                host_crosscheck: None,
                merge_frames: if token.merge { Some(values) } else { None },
            },
        );
    });
}
pub(super) fn release_result(token: Mutation, ledger: &RefCell<ReadWork>) {
    mutation_result(token, ledger, [0, 0]);
}
pub(super) fn merge_result(token: Mutation, control: WorkControl<'_>, target: u64, source: u64) {
    mutation_result(token, control.work, [target, source]);
}
