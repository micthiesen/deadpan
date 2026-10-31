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
    issuer_checks: u64,
    issuer_work: usize,
}

#[derive(Clone, Copy, Default, Serialize)]
pub(in crate::stages) struct Snapshot {
    plan: Lane,
    cache: Lane,
    active_depth: u32,
    max_active_depth: u32,
    nested: bool,
    invalid: bool,
    overflow: bool,
}

thread_local! {
    static TRACE: Cell<Snapshot> = Cell::new(Snapshot::default());
}

pub(in crate::stages) fn reset() {
    TRACE.with(|trace| {
        assert_eq!(trace.get().active_depth, 0);
        trace.set(Snapshot::default());
    });
}

pub(in crate::stages) fn snapshot() -> Snapshot {
    TRACE.with(Cell::get)
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

pub(super) fn issuer_spent(control: WorkControl<'_>) {
    let cache = control.work.borrow().cache_key_scopes > 0;
    TRACE.with(|trace| {
        let mut state = trace.get();
        let lane = if cache {
            &mut state.cache
        } else {
            &mut state.plan
        };
        count(&mut lane.issuer_checks, &mut state.overflow);
        add(&mut lane.issuer_work, 1, &mut state.overflow);
        trace.set(state);
    });
}

pub(super) struct ScopeGuard<'work> {
    control: WorkControl<'work>,
    before: [usize; 2],
    cache: bool,
    tracked_depth: bool,
    completed: Option<(usize, usize, usize, bool)>,
}

impl<'work> ScopeGuard<'work> {
    pub(super) fn new(control: WorkControl<'work>) -> Self {
        let work = control.work.borrow();
        let before = [work.plan_work, work.cache_plan_work];
        let cache = work.cache_key_scopes > 0;
        drop(work);
        let tracked_depth = TRACE.with(|trace| {
            let mut state = trace.get();
            state.nested |= state.active_depth != 0;
            let tracked = if let Some(depth) = state.active_depth.checked_add(1) {
                state.active_depth = depth;
                state.max_active_depth = state.max_active_depth.max(depth);
                true
            } else {
                state.overflow = true;
                false
            };
            trace.set(state);
            tracked
        });
        Self {
            control,
            before,
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
        TRACE.with(|trace| {
            let mut state = trace.get();
            let deltas = [
                after[0].checked_sub(self.before[0]),
                after[1].checked_sub(self.before[1]),
            ];
            if deltas.iter().any(Option::is_none) {
                state.invalid = true;
            }
            let deltas = deltas.map(|delta| delta.unwrap_or(0));
            let selected = usize::from(self.cache);
            state.invalid |= deltas[1 - selected] != 0;
            if let Some((canonical, additional, total, forwarded)) = self.completed {
                state.invalid |= !forwarded
                    || canonical.checked_add(additional) != Some(total)
                    || deltas[selected] != total;
            }
            for (index, accepted) in deltas.into_iter().enumerate() {
                let lane = if index == 0 {
                    &mut state.plan
                } else {
                    &mut state.cache
                };
                add(&mut lane.accepted_work, accepted, &mut state.overflow);
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
                    add(&mut lane.partial_work, accepted, &mut state.overflow);
                }
            }
            if self.tracked_depth {
                if let Some(depth) = state.active_depth.checked_sub(1) {
                    state.active_depth = depth;
                } else {
                    state.invalid = true;
                }
            }
            trace.set(state);
        });
    }
}
