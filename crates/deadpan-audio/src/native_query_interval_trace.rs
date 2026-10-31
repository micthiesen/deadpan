//! Additive raw/Point constructor intervals; existing receipts remain untouched.
use super::point_suppression::ops::trace::{self as native, WorkBoundary};
use super::{ReadWork, WorkControl};
use serde::Serialize;
use std::cell::RefCell;

const MAX_ACTIVE: usize = 16;
const MAX_ROWS: usize = 128;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    RawDomain,
    PointContent,
}

#[derive(Clone, Copy, Serialize)]
pub(super) struct Claim {
    pub(super) canonical: usize,
    pub(super) additional: usize,
    pub(super) total: usize,
    pub(super) expected_debit: usize,
    pub(super) forwarded: bool,
    pub(super) ordinary: bool,
    pub(super) raw_kind: Option<&'static str>,
    pub(super) result_spans: Option<usize>,
}

#[derive(Clone, Copy, Serialize)]
struct HostBoundary {
    units: [usize; 2],
    overflow: bool,
    reset_epoch: Option<u64>,
}
fn host_boundary() -> HostBoundary {
    let (units, overflow, reset_epoch) =
        super::original_request::flattened_work_trace::host_eviction_boundary();
    HostBoundary {
        units,
        overflow,
        reset_epoch,
    }
}
fn work_boundary(control: WorkControl<'_>) -> [usize; 2] {
    let work = control.work.borrow();
    [work.plan_work, work.cache_plan_work]
}

#[derive(Clone, Serialize)]
struct Row {
    ordinal: usize,
    parent_ordinal: Option<usize>,
    depth: usize,
    kind: Kind,
    // 0 is Plan; 1 is cache. Both observed work lanes remain literal below.
    selected_lane: usize,
    work_before: [usize; 2],
    work_after: [usize; 2],
    native_before: WorkBoundary,
    native_after: WorkBoundary,
    host_before: HostBoundary,
    host_after: HostBoundary,
    claim: Option<Claim>,
    foreign_work_overlap: bool,
    attributable: bool,
}

#[derive(Clone, Serialize)]
pub(super) struct Snapshot {
    schema: &'static str,
    phase_order: [&'static str; 8],
    rows: Vec<Row>,
    active_intervals: usize,
    max_active_intervals: usize,
    dropped_intervals: usize,
    nested: bool,
    misnested: bool,
    overflow: bool,
}
#[derive(Clone, Copy)]
struct Active {
    ordinal: usize,
    // Equality only while the actual scopes are live; never dereferenced/exported.
    work: *const RefCell<ReadWork>,
    foreign: bool,
}
#[derive(Default)]
struct State {
    enabled: bool,
    next: usize,
    rows: Vec<Row>,
    active: Vec<Active>,
    max_active: usize,
    dropped: usize,
    nested: bool,
    misnested: bool,
    overflow: bool,
}
thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub(super) fn reset() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        assert!(
            state.active.is_empty(),
            "reset between actual constructor intervals"
        );
        *state = State {
            enabled: true,
            next: 1,
            rows: Vec::with_capacity(MAX_ROWS),
            active: Vec::with_capacity(MAX_ACTIVE),
            ..State::default()
        };
    });
}
pub(super) fn disable() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        assert!(state.active.is_empty(), "disable after constructor cleanup");
        state.enabled = false;
    });
}
pub(super) fn snapshot() -> Snapshot {
    STATE.with(|state| {
        let state = state.borrow();
        Snapshot {
            schema: "native-mask-query-intervals-v1",
            phase_order: [
                "account",
                "scan",
                "map",
                "zero",
                "mark",
                "allocate_vector",
                "allocate_storage",
                "host_residency",
            ],
            rows: state.rows.clone(),
            active_intervals: state.active.len(),
            max_active_intervals: state.max_active,
            dropped_intervals: state.dropped,
            nested: state.nested,
            misnested: state.misnested,
            overflow: state.overflow,
        }
    })
}

/// Observe only after a real accepted native/residency/eviction debit.
/// A different request invalidates attribution instead of being subtracted.
pub(super) fn accepted_work(control: WorkControl<'_>) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if !state.enabled {
            return;
        }
        let key = control.work as *const RefCell<ReadWork>;
        for active in &mut state.active {
            active.foreign |= !std::ptr::eq(active.work, key);
        }
    });
}

struct Open {
    ordinal: usize,
    parent: Option<usize>,
    depth: usize,
    kind: Kind,
    lane: usize,
    work_before: [usize; 2],
    native_before: WorkBoundary,
    host_before: HostBoundary,
}
pub(super) struct ScopeGuard<'work> {
    control: WorkControl<'work>,
    open: Option<Open>,
    claim: Option<Claim>,
}
impl<'work> ScopeGuard<'work> {
    pub(super) fn new(kind: Kind, control: WorkControl<'work>) -> Self {
        let open = STATE.with(|state| {
            let mut state = state.borrow_mut();
            if !state.enabled {
                return None;
            }
            if state.active.len() >= MAX_ACTIVE || state.rows.len() + state.active.len() >= MAX_ROWS
            {
                state.overflow = true;
                if let Some(next) = state.dropped.checked_add(1) {
                    state.dropped = next;
                }
                return None;
            }
            let ordinal = state.next;
            let Some(next) = ordinal.checked_add(1) else {
                state.overflow = true;
                return None;
            };
            state.next = next;
            let parent = state.active.last().map(|active| active.ordinal);
            state.nested |= parent.is_some();
            let key = control.work as *const RefCell<ReadWork>;
            let foreign = state
                .active
                .iter()
                .any(|active| !std::ptr::eq(active.work, key));
            // Entering another observed request taints its enclosing intervals too,
            // even when no native debit occurs before the child completes.
            if foreign {
                for active in &mut state.active {
                    active.foreign = true;
                }
            }
            state.active.push(Active {
                ordinal,
                work: key,
                foreign,
            });
            state.max_active = state.max_active.max(state.active.len());
            let lane = usize::from(control.work.borrow().cache_key_scopes > 0);
            Some(Open {
                ordinal,
                parent,
                depth: state.active.len(),
                kind,
                lane,
                work_before: work_boundary(control),
                native_before: native::work_boundary(),
                host_before: host_boundary(),
            })
        });
        Self {
            control,
            open,
            claim: None,
        }
    }

    pub(super) fn completed(&mut self, claim: Claim) {
        if self.open.is_some() {
            self.claim = Some(claim);
        }
    }
}
impl Drop for ScopeGuard<'_> {
    fn drop(&mut self) {
        let Some(open) = self.open.take() else {
            return;
        };
        let work_after = work_boundary(self.control);
        let native_after = native::work_boundary();
        let host_after = host_boundary();
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let Some(index) = state
                .active
                .iter()
                .position(|active| active.ordinal == open.ordinal)
            else {
                state.misnested = true;
                return;
            };
            let ordered = index + 1 == state.active.len();
            state.misnested |= !ordered;
            let active = state.active.remove(index);
            let attributable = ordered
                && work_after
                    .into_iter()
                    .zip(open.work_before)
                    .all(|(after, before)| after >= before)
                && !active.foreign
                && open.native_before.enabled
                && native_after.enabled
                && open.native_before.reset_epoch.is_some()
                && open.native_before.reset_epoch == native_after.reset_epoch
                && open.host_before.reset_epoch.is_some()
                && open.host_before.reset_epoch == host_after.reset_epoch
                && !open.native_before.overflow
                && !native_after.overflow
                && !open.host_before.overflow
                && !host_after.overflow;
            state.rows.push(Row {
                ordinal: open.ordinal,
                parent_ordinal: open.parent,
                depth: open.depth,
                kind: open.kind,
                selected_lane: open.lane,
                work_before: open.work_before,
                work_after,
                native_before: open.native_before,
                native_after,
                host_before: open.host_before,
                host_after,
                claim: self.claim,
                foreign_work_overlap: active.foreign,
                attributable,
            });
        });
    }
}

#[path = "native_query_interval_trace_tests.rs"]
mod tests;
