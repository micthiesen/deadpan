//! Bounded test-only receipt trace for the fresh raw-domain constructor.
use super::WorkControl;
use serde::Serialize;
use std::cell::Cell;

#[derive(Clone, Copy, Default, Serialize)]
pub(super) struct Lane {
    constructor_calls: usize,
    ordinary_calls: usize,
    admitted_calls: usize,
    // Admitted successes; ordinary success is counted only in ordinary_calls.
    completed_queries: usize,
    failed_queries: usize,
    accepted_work: usize,
    canonical_units: usize,
    additional_units: usize,
    partial_work: usize,
    host_eviction_work: usize,
    kind_counts: [usize; 3],
}

#[derive(Clone, Copy, Default, Serialize)]
pub(super) struct Snapshot {
    plan: Lane,
    cache: Lane,
    active_depth: usize,
    max_active_depth: usize,
    live_results: usize,
    peak_live_results: usize,
    result_creations: usize,
    result_drops: usize,
    nested_root_callbacks: usize,
    nested_point_callbacks: usize,
    nested: bool,
    invalid: bool,
    overflow: bool,
}

#[derive(Clone, Copy)]
pub(super) enum Product {
    Root,
    Point,
    Empty,
}
impl Product {
    const fn index(self) -> usize {
        match self {
            Self::Root => 0,
            Self::Point => 1,
            Self::Empty => 2,
        }
    }
}
#[derive(Clone, Copy)]
enum LaneKind {
    Plan,
    Cache,
}
impl LaneKind {
    const fn index(self) -> usize {
        match self {
            Self::Plan => 0,
            Self::Cache => 1,
        }
    }
}

thread_local! {
    static TRACE: Cell<Snapshot> = Cell::new(Snapshot::default());
    static ENABLED: Cell<bool> = const { Cell::new(false) };
}

fn host_evictions() -> ([usize; 2], bool) {
    super::original_request::flattened_work_trace::host_eviction_units()
}

pub(super) fn reset() {
    TRACE.with(|trace| {
        assert_eq!(trace.get().active_depth, 0);
        assert_eq!(trace.get().live_results, 0);
        trace.set(Snapshot::default());
    });
    ENABLED.with(|enabled| enabled.set(true));
}

pub(super) fn disable() {
    TRACE.with(|trace| assert_eq!(trace.get().active_depth, 0));
    ENABLED.with(|enabled| enabled.set(false));
}

pub(super) fn snapshot() -> Snapshot {
    TRACE.with(Cell::get)
}

fn add(value: &mut usize, amount: usize, overflow: &mut bool) {
    if let Some(next) = value.checked_add(amount) {
        *value = next;
    } else {
        *overflow = true;
    }
}

pub(super) struct ScopeGuard<'work> {
    control: WorkControl<'work>,
    lane: LaneKind,
    before: [usize; 2],
    before_host: [usize; 2],
    tracked: bool,
    completed: Option<(usize, usize, usize, bool, Product)>,
}

impl<'work> ScopeGuard<'work> {
    pub(super) fn new(control: WorkControl<'work>) -> Self {
        let enabled = ENABLED.with(Cell::get);
        if !enabled {
            return Self {
                control,
                lane: LaneKind::Plan,
                before: [0; 2],
                before_host: [0; 2],
                tracked: false,
                completed: None,
            };
        }
        let (before, lane) = {
            let work = control.work.borrow();
            (
                [work.plan_work, work.cache_plan_work],
                if work.cache_key_scopes > 0 {
                    LaneKind::Cache
                } else {
                    LaneKind::Plan
                },
            )
        };
        let before_host = host_evictions().0;
        let tracked = TRACE.with(|trace| {
            let mut state = trace.get();
            state.nested |= state.active_depth != 0;
            if let Some(depth) = state.active_depth.checked_add(1) {
                state.active_depth = depth;
                state.max_active_depth = state.max_active_depth.max(depth);
                trace.set(state);
                true
            } else {
                state.overflow = true;
                trace.set(state);
                false
            }
        });
        Self {
            control,
            lane,
            before,
            before_host,
            tracked,
            completed: None,
        }
    }

    pub(super) fn completed(
        &mut self,
        canonical: usize,
        additional: usize,
        total: usize,
        forwarded: bool,
        product: Product,
    ) {
        if self.tracked {
            self.completed = Some((canonical, additional, total, forwarded, product));
        }
    }
}

impl Drop for ScopeGuard<'_> {
    fn drop(&mut self) {
        if !self.tracked {
            return;
        }
        let after = {
            let work = self.control.work.borrow();
            [work.plan_work, work.cache_plan_work]
        };
        let (host_after, host_overflow) = host_evictions();
        TRACE.with(|trace| {
            let mut state = trace.get();
            state.overflow |= host_overflow;
            let deltas = [
                after[0].checked_sub(self.before[0]),
                after[1].checked_sub(self.before[1]),
            ];
            let host_deltas = [
                host_after[0].checked_sub(self.before_host[0]),
                host_after[1].checked_sub(self.before_host[1]),
            ];
            if deltas.iter().any(Option::is_none) || host_deltas.iter().any(Option::is_none) {
                state.invalid = true;
            }
            let deltas = deltas.map(|value| value.unwrap_or(0));
            let host_deltas = host_deltas.map(|value| value.unwrap_or(0));
            let mut forwarded = [0; 2];
            for lane in 0..2 {
                match deltas[lane].checked_sub(host_deltas[lane]) {
                    Some(value) => forwarded[lane] = value,
                    None => state.invalid = true,
                }
            }
            let selected = self.lane.index();
            let other = 1 - selected;
            state.invalid |= deltas[other] != 0 || host_deltas[other] != 0;

            let mut plan = state.plan;
            let mut cache = state.cache;
            let mut overflow = state.overflow;
            for lane_index in 0..2 {
                let lane = if lane_index == 0 {
                    &mut plan
                } else {
                    &mut cache
                };
                add(&mut lane.accepted_work, deltas[lane_index], &mut overflow);
                add(
                    &mut lane.host_eviction_work,
                    host_deltas[lane_index],
                    &mut overflow,
                );
            }
            let lane = if selected == 0 { &mut plan } else { &mut cache };
            add(&mut lane.constructor_calls, 1, &mut overflow);
            if let Some((canonical, additional, total, was_forwarded, product)) = self.completed {
                let expected = if was_forwarded { total } else { 0 };
                state.invalid |= canonical.checked_add(additional) != Some(total)
                    || forwarded[selected] != expected
                    || (was_forwarded && canonical != 0);
                add(&mut lane.canonical_units, canonical, &mut overflow);
                add(&mut lane.additional_units, additional, &mut overflow);
                add(&mut lane.kind_counts[product.index()], 1, &mut overflow);
                if was_forwarded {
                    add(&mut lane.completed_queries, 1, &mut overflow);
                    add(&mut lane.admitted_calls, 1, &mut overflow);
                } else {
                    add(&mut lane.ordinary_calls, 1, &mut overflow);
                }
            } else {
                add(&mut lane.failed_queries, 1, &mut overflow);
                add(&mut lane.partial_work, forwarded[selected], &mut overflow);
            }
            state.plan = plan;
            state.cache = cache;
            state.overflow = overflow;
            if let Some(depth) = state.active_depth.checked_sub(1) {
                state.active_depth = depth;
            } else {
                state.invalid = true;
            }
            trace.set(state);
        });
    }
}

/// Declared before the actual raw value so its drop observes that value's cleanup.
pub(super) struct ResultGuard {
    created: bool,
}
impl ResultGuard {
    pub(super) fn new() -> Self {
        Self { created: false }
    }
    pub(super) fn acquired(&mut self) {
        assert!(!self.created);
        if !ENABLED.with(Cell::get) {
            return;
        }
        self.created = TRACE.with(|trace| {
            let mut state = trace.get();
            let Some(live) = state.live_results.checked_add(1) else {
                state.overflow = true;
                trace.set(state);
                return false;
            };
            state.live_results = live;
            state.peak_live_results = state.peak_live_results.max(live);
            let mut overflow = state.overflow;
            add(&mut state.result_creations, 1, &mut overflow);
            state.overflow = overflow;
            trace.set(state);
            true
        });
    }
}
impl Drop for ResultGuard {
    fn drop(&mut self) {
        if !self.created {
            return;
        }
        TRACE.with(|trace| {
            let mut state = trace.get();
            if let Some(live) = state.live_results.checked_sub(1) {
                state.live_results = live;
            } else {
                state.invalid = true;
            }
            let mut overflow = state.overflow;
            add(&mut state.result_drops, 1, &mut overflow);
            state.overflow = overflow;
            trace.set(state);
        });
    }
}
pub(super) fn nested_callback(product: Product) {
    if !ENABLED.with(Cell::get) {
        return;
    }
    TRACE.with(|trace| {
        let mut state = trace.get();
        if state.live_results == 0 {
            return;
        }
        let mut overflow = state.overflow;
        match product {
            Product::Root => add(&mut state.nested_root_callbacks, 1, &mut overflow),
            Product::Point => add(&mut state.nested_point_callbacks, 1, &mut overflow),
            Product::Empty => state.invalid = true,
        }
        state.overflow = overflow;
        trace.set(state);
    });
}
