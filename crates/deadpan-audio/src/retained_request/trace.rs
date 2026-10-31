use super::WorkControl;
use serde::Serialize;
use std::cell::Cell;

#[derive(Clone, Copy, Default, Serialize)]
pub(in crate::stages) struct Lane {
    constructor_calls: u64,
    completed_queries: u64,
    failed_queries: u64,
    accepted_work: usize,
    canonical_units: usize,
    additional_units: usize,
    partial_work: usize,
    host_eviction_work: usize,
}

#[derive(Clone, Copy, Default, Serialize)]
pub(in crate::stages) struct Snapshot {
    plan: Lane,
    cache: Lane,
    active_depth: u32,
    max_active_depth: u32,
    all_host_eviction_work: [usize; 2],
    nested: bool,
    invalid: bool,
    overflow: bool,
}

#[derive(Clone, Copy)]
enum Product {
    Fade,
    Owners,
}

impl Product {
    const fn index(self) -> usize {
        match self {
            Self::Fade => 0,
            Self::Owners => 1,
        }
    }
}

thread_local! {
    static TRACE: Cell<[Snapshot; 2]> = Cell::new([Snapshot::default(); 2]);
}

fn host_evictions() -> ([usize; 2], bool) {
    super::super::original_request::flattened_work_trace::host_eviction_units()
}

pub(in crate::stages) fn reset() {
    TRACE.with(|trace| {
        let state = trace.get();
        for product in [Product::Fade, Product::Owners] {
            assert_eq!(state[product.index()].active_depth, 0);
        }
        trace.set([Snapshot::default(); 2]);
    });
}

pub(in crate::stages) fn fade_snapshot() -> Snapshot {
    snapshot_product(Product::Fade)
}

pub(in crate::stages) fn owners_snapshot() -> Snapshot {
    snapshot_product(Product::Owners)
}

fn snapshot_product(product: Product) -> Snapshot {
    let mut snapshot = TRACE.with(|trace| trace.get()[product.index()]);
    let (host_units, host_overflow) = host_evictions();
    snapshot.all_host_eviction_work = host_units;
    snapshot.overflow |= host_overflow;
    snapshot
}

fn add(value: &mut usize, amount: usize, overflow: &mut bool) {
    if let Some(next) = value.checked_add(amount) {
        *value = next;
    } else {
        *overflow = true;
    }
}

fn count(value: &mut u64, overflow: &mut bool) {
    if let Some(next) = value.checked_add(1) {
        *value = next;
    } else {
        *overflow = true;
    }
}

pub(super) struct ScopeGuard<'work> {
    control: WorkControl<'work>,
    product: Product,
    before: [usize; 2],
    before_host_evictions: [usize; 2],
    cache: bool,
    tracked_depth: bool,
    completed: Option<(usize, usize, usize, bool)>,
}

impl<'work> ScopeGuard<'work> {
    pub(super) fn new_fade(control: WorkControl<'work>) -> Self {
        Self::new_product(Product::Fade, control)
    }

    pub(super) fn new_owners(control: WorkControl<'work>) -> Self {
        Self::new_product(Product::Owners, control)
    }

    fn new_product(product: Product, control: WorkControl<'work>) -> Self {
        let work = control.work.borrow();
        let before = [work.plan_work, work.cache_plan_work];
        let cache = work.cache_key_scopes > 0;
        drop(work);
        let tracked_depth = TRACE.with(|trace| {
            let mut slots = trace.get();
            let state = &mut slots[product.index()];
            state.nested |= state.active_depth != 0;
            let tracked = if let Some(depth) = state.active_depth.checked_add(1) {
                state.active_depth = depth;
                state.max_active_depth = state.max_active_depth.max(depth);
                true
            } else {
                state.overflow = true;
                false
            };
            trace.set(slots);
            tracked
        });
        Self {
            control,
            product,
            before,
            before_host_evictions: host_evictions().0,
            cache,
            tracked_depth,
            completed: None,
        }
    }

    pub(super) fn completed(
        &mut self,
        canonical: usize,
        additional: usize,
        total: usize,
        forwarded: bool,
    ) {
        self.completed = Some((canonical, additional, total, forwarded));
    }
}

impl Drop for ScopeGuard<'_> {
    fn drop(&mut self) {
        let work = self.control.work.borrow();
        let after = [work.plan_work, work.cache_plan_work];
        drop(work);
        let (host_units, host_overflow) = host_evictions();
        TRACE.with(|trace| {
            let mut slots = trace.get();
            let state = &mut slots[self.product.index()];
            state.overflow |= host_overflow;
            let deltas = [
                after[0].checked_sub(self.before[0]),
                after[1].checked_sub(self.before[1]),
            ];
            if deltas.iter().any(Option::is_none) {
                state.invalid = true;
            }
            let deltas = deltas.map(|delta| delta.unwrap_or(0));
            let host_deltas = std::array::from_fn::<_, 2, _>(|lane| {
                host_units[lane].checked_sub(self.before_host_evictions[lane])
            });
            if host_deltas.iter().any(Option::is_none) {
                state.invalid = true;
            }
            let host_deltas = host_deltas.map(|delta| delta.unwrap_or(0));
            let forwarded_units =
                std::array::from_fn::<_, 2, _>(|lane| deltas[lane].checked_sub(host_deltas[lane]));
            if forwarded_units.iter().any(Option::is_none) {
                state.invalid = true;
            }
            let forwarded_units = forwarded_units.map(|units| units.unwrap_or(0));
            let selected = usize::from(self.cache);
            state.invalid |= deltas[1 - selected] != 0 || host_deltas[1 - selected] != 0;
            if let Some((canonical, additional, total, forwarded)) = self.completed {
                state.invalid |= !forwarded
                    || canonical.checked_add(additional) != Some(total)
                    || forwarded_units[selected] != total;
            }
            for (index, accepted) in deltas.into_iter().enumerate() {
                let lane = if index == 0 {
                    &mut state.plan
                } else {
                    &mut state.cache
                };
                add(&mut lane.accepted_work, accepted, &mut state.overflow);
                add(
                    &mut lane.host_eviction_work,
                    host_deltas[index],
                    &mut state.overflow,
                );
                if index != selected {
                    continue;
                }
                count(&mut lane.constructor_calls, &mut state.overflow);
                if let Some((canonical, additional, _, _)) = self.completed {
                    count(&mut lane.completed_queries, &mut state.overflow);
                    add(&mut lane.canonical_units, canonical, &mut state.overflow);
                    add(&mut lane.additional_units, additional, &mut state.overflow);
                } else {
                    count(&mut lane.failed_queries, &mut state.overflow);
                    add(
                        &mut lane.partial_work,
                        forwarded_units[index],
                        &mut state.overflow,
                    );
                }
            }
            if self.tracked_depth {
                if let Some(depth) = state.active_depth.checked_sub(1) {
                    state.active_depth = depth;
                } else {
                    state.invalid = true;
                }
            }
            trace.set(slots);
        });
    }
}
