//! Test-only actual work observations. Fixed TLS storage never bills work.

use super::WorkControl;
use serde::Serialize;
use std::cell::Cell;

#[derive(Clone, Copy)]
pub(super) enum Phase {
    Queries,
    Prediction,
    OriginalRead,
    Probe,
}

impl Phase {
    const fn index(self) -> usize {
        match self {
            Self::Queries => 0,
            Self::Prediction => 1,
            Self::OriginalRead => 2,
            Self::Probe => 3,
        }
    }
}

#[derive(Clone, Copy, Serialize)]
pub(super) struct Work {
    pub(super) plan_work: usize,
    pub(super) cache_plan_work: usize,
}

impl Work {
    const ZERO: Self = Self {
        plan_work: 0,
        cache_plan_work: 0,
    };

    fn read(control: WorkControl<'_>) -> Self {
        let work = control.work.borrow();
        Self {
            plan_work: work.plan_work,
            cache_plan_work: work.cache_plan_work,
        }
    }

    fn delta(self, before: Self) -> Option<Self> {
        Some(Self {
            plan_work: self.plan_work.checked_sub(before.plan_work)?,
            cache_plan_work: self.cache_plan_work.checked_sub(before.cache_plan_work)?,
        })
    }

    fn checked_add(self, delta: Self) -> Option<Self> {
        Some(Self {
            plan_work: self.plan_work.checked_add(delta.plan_work)?,
            cache_plan_work: self.cache_plan_work.checked_add(delta.cache_plan_work)?,
        })
    }
}

#[derive(Clone, Copy, Serialize)]
pub(super) struct Snapshot {
    /// Ordered Queries, Prediction, OriginalRead, Probe. Inclusive if nested.
    pub(super) work: [Work; 4],
    pub(super) calls: [u64; 4],
    pub(super) active_depth: u32,
    pub(super) max_active_depth: u32,
    pub(super) nested: bool,
    /// Sticky 0/1 flag for any counter, subtraction or depth overflow.
    pub(super) overflow: u8,
}

impl Snapshot {
    const ZERO: Self = Self {
        work: [Work::ZERO; 4],
        calls: [0; 4],
        active_depth: 0,
        max_active_depth: 0,
        nested: false,
        overflow: 0,
    };

    pub(super) fn phase(self, phase: Phase) -> Work {
        self.work[phase.index()]
    }
}

thread_local! {
    static TRACE: Cell<Snapshot> = const { Cell::new(Snapshot::ZERO) };
    static LEGACY: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn legacy() -> bool {
    LEGACY.with(Cell::get)
}

pub(super) fn with_legacy<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(LEGACY.with(|mode| mode.replace(true)));
    super::original_request::with_legacy_original_queries(action)
}

pub(super) fn reset() {
    TRACE.with(|trace| {
        assert_eq!(
            trace.get().active_depth,
            0,
            "cannot reset active work observations"
        );
        trace.set(Snapshot::ZERO);
    });
}

pub(super) fn snapshot() -> Snapshot {
    TRACE.with(Cell::get)
}

pub(super) struct ScopeGuard<'work> {
    phase: Phase,
    control: WorkControl<'work>,
    before: Work,
    tracked_depth: bool,
}

impl<'work> ScopeGuard<'work> {
    pub(super) fn new(phase: Phase, control: WorkControl<'work>) -> Self {
        let before = Work::read(control);
        let tracked_depth = TRACE.with(|trace| {
            let mut state = trace.get();
            let next = state.active_depth.checked_add(1);
            if let Some(depth) = next {
                state.active_depth = depth;
                state.max_active_depth = state.max_active_depth.max(depth);
                state.nested |= depth > 1;
            } else {
                state.overflow = 1;
            }
            trace.set(state);
            next.is_some()
        });
        Self {
            phase,
            control,
            before,
            tracked_depth,
        }
    }
}

impl Drop for ScopeGuard<'_> {
    fn drop(&mut self) {
        // Observe already committed counters, including an early ? exit.
        let delta = Work::read(self.control).delta(self.before);
        TRACE.with(|trace| {
            let mut state = trace.get();
            let index = self.phase.index();
            match delta.and_then(|delta| state.work[index].checked_add(delta)) {
                Some(work) => state.work[index] = work,
                None => state.overflow = 1,
            }
            match state.calls[index].checked_add(1) {
                Some(calls) => state.calls[index] = calls,
                None => state.overflow = 1,
            }
            if self.tracked_depth {
                match state.active_depth.checked_sub(1) {
                    Some(depth) => state.active_depth = depth,
                    None => state.overflow = 1,
                }
            }
            trace.set(state);
        });
    }
}
