//! Bookkeeping scopes only; these are not real raw/Point constructor evidence.
use super::*;
use crate::stages::original_request::flattened_work_trace as host;
use crate::stages::point_suppression::ops::{self, MaskWorkPhase};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

struct Session;
impl Session {
    fn new(enabled: bool) -> Self {
        STATE.with(|state| {
            assert!(state.borrow().active.is_empty());
            *state.borrow_mut() = State::default();
        });
        native::reset();
        host::reset();
        if enabled {
            reset();
        }
        Self
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        disable();
        native::disable();
    }
}

fn control<'work>(
    work: &'work RefCell<ReadWork>,
    cancelled: &'work AtomicBool,
) -> WorkControl<'work> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(30),
        work,
    }
}
fn spend(control: WorkControl<'_>, phase: MaskWorkPhase, units: usize) {
    ops::spend_work(control, phase, units).unwrap();
}
fn spend_host(control: WorkControl<'_>) {
    // Reproduce the accepted-debit/receipt ordering without claiming an eviction.
    control.spend_plan_work(1).unwrap();
    host::host_eviction_spent(control);
}
fn marker_claim() -> Claim {
    // Completion metadata for this bookkeeping test, not a real query receipt.
    Claim {
        canonical: 0,
        additional: 0,
        total: 0,
        expected_debit: 0,
        forwarded: false,
        ordinary: true,
        raw_kind: Some("point"),
        result_spans: None,
    }
}
fn lane_delta(row: &Row, lane: usize, phase: usize) -> usize {
    row.native_after.lanes[lane][phase]
        .checked_sub(row.native_before.lanes[lane][phase])
        .unwrap()
}
fn ledger_delta(row: &Row, lane: usize) -> usize {
    row.work_after[lane]
        .checked_sub(row.work_before[lane])
        .unwrap()
}

#[test]
fn default_off_records_no_intervals_or_observer_capacity_but_real_work_still_spends() {
    let _session = Session::new(false);
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    let mut guard = ScopeGuard::new(Kind::RawDomain, control);
    spend(control, MaskWorkPhase::Map, 3);
    guard.completed(marker_claim());
    drop(guard);
    assert_eq!(work.borrow().plan_work, 3);
    assert_eq!(native::work_boundary().lanes[0][2], 3);
    let result = snapshot();
    assert!(result.rows.is_empty());
    assert_eq!(result.active_intervals, 0);
    assert!(!result.overflow);
    STATE.with(|state| {
        assert!(!state.borrow().enabled);
        assert_eq!(state.borrow().rows.capacity(), 0);
        assert_eq!(state.borrow().active.capacity(), 0);
    });
}

#[test]
fn exact_interval_excludes_before_after_work_and_keeps_plan_cache_host_lanes_literal() {
    let _session = Session::new(true);
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    spend(control, MaskWorkPhase::Map, 3);
    {
        let _cache = control.cache_key_scope();
        let mut guard = ScopeGuard::new(Kind::PointContent, control);
        spend(control, MaskWorkPhase::Map, 4);
        spend_host(control);
        guard.completed(marker_claim());
    }
    spend(control, MaskWorkPhase::Map, 5);
    let result = snapshot();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.active_intervals, 0);
    let row = &result.rows[0];
    assert!(row.attributable);
    assert!(!row.foreign_work_overlap);
    assert_eq!(row.selected_lane, 1);
    assert_eq!(row.work_before, [3, 0]);
    assert_eq!(row.work_after, [3, 5]);
    assert_eq!(lane_delta(row, 0, 2), 0);
    assert_eq!(lane_delta(row, 1, 2), 4);
    assert_eq!(row.host_before.units, [0, 0]);
    assert_eq!(row.host_after.units, [0, 1]);
    assert_eq!(ledger_delta(row, 1), lane_delta(row, 1, 2) + 1);
    assert_eq!(row.claim.as_ref().unwrap().expected_debit, 0);
    assert_eq!(
        [work.borrow().plan_work, work.borrow().cache_plan_work],
        [8, 5]
    );
    assert_eq!(native::work_boundary().lanes[0][2], 8);
    assert_eq!(native::work_boundary().lanes[1][2], 4);
}

#[test]
fn same_actual_work_nesting_is_inclusive_and_partial_completion_is_not_invented() {
    let _session = Session::new(true);
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    let outer = ScopeGuard::new(Kind::RawDomain, control);
    spend(control, MaskWorkPhase::Scan, 1);
    {
        let mut inner = ScopeGuard::new(Kind::PointContent, control);
        spend(control, MaskWorkPhase::Map, 2);
        inner.completed(marker_claim());
    }
    let mid = snapshot();
    assert_eq!(mid.active_intervals, 1);
    assert_eq!(mid.rows.len(), 1);
    assert_eq!(mid.rows[0].ordinal, 2);
    assert_eq!(mid.rows[0].parent_ordinal, Some(1));
    assert_eq!(mid.rows[0].depth, 2);
    spend(control, MaskWorkPhase::Scan, 3);
    spend_host(control);
    drop(outer);
    let result = snapshot();
    assert!(result.nested);
    assert!(!result.misnested);
    assert_eq!(result.max_active_intervals, 2);
    assert_eq!(result.active_intervals, 0);
    let inner = &result.rows[0];
    let outer = &result.rows[1];
    assert!(inner.attributable && outer.attributable);
    assert_eq!(ledger_delta(inner, 0), 2);
    assert_eq!(lane_delta(inner, 0, 2), 2);
    assert_eq!(ledger_delta(outer, 0), 7);
    assert_eq!(lane_delta(outer, 0, 1), 4);
    assert_eq!(lane_delta(outer, 0, 2), 2);
    assert_eq!(outer.host_after.units[0] - outer.host_before.units[0], 1);
    assert!(inner.claim.is_some());
    assert!(outer.claim.is_none());
    // The parent already includes the child, so these are not disjoint fees.
    assert_eq!(lane_delta(outer, 0, 2), lane_delta(inner, 0, 2));
}

#[test]
fn foreign_actual_work_events_and_foreign_nesting_disable_attribution_without_rewriting_data() {
    let _session = Session::new(true);
    let own_work = RefCell::new(ReadWork::default());
    let other_work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let own = control(&own_work, &cancelled);
    let other = control(&other_work, &cancelled);
    let guard = ScopeGuard::new(Kind::RawDomain, own);
    spend(own, MaskWorkPhase::Scan, 1);
    spend(other, MaskWorkPhase::Map, 2);
    spend_host(other);
    drop(guard);
    let result = snapshot();
    let row = &result.rows[0];
    assert!(row.foreign_work_overlap);
    assert!(!row.attributable);
    assert_eq!(ledger_delta(row, 0), 1);
    assert_eq!(lane_delta(row, 0, 1), 1);
    assert_eq!(lane_delta(row, 0, 2), 2);
    assert_eq!(row.host_after.units[0] - row.host_before.units[0], 1);
    assert_eq!(own_work.borrow().plan_work, 1);
    assert_eq!(other_work.borrow().plan_work, 3);

    // Reset only observer rows between completed intervals, never the real work.
    reset();
    let outer = ScopeGuard::new(Kind::RawDomain, own);
    let inner = ScopeGuard::new(Kind::PointContent, other);
    drop(inner);
    drop(outer);
    let result = snapshot();
    assert_eq!(result.rows.len(), 2);
    assert!(
        result
            .rows
            .iter()
            .all(|row| row.foreign_work_overlap && !row.attributable)
    );
    assert!(
        result
            .rows
            .iter()
            .all(|row| row.work_before == row.work_after)
    );
    assert_eq!(own_work.borrow().plan_work, 1);
    assert_eq!(other_work.borrow().plan_work, 3);
}

#[test]
fn native_reset_disable_reenable_and_host_reset_are_visible_across_catching_up_counters() {
    for change in 0..5 {
        let _session = Session::new(true);
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = control(&work, &cancelled);
        if change == 3 {
            for _ in 0..3 {
                spend_host(control);
            }
        } else {
            spend(control, MaskWorkPhase::Map, 3);
        }
        if change == 4 {
            native::disable();
        }
        let guard = ScopeGuard::new(Kind::RawDomain, control);
        if change == 3 {
            spend_host(control);
            host::reset();
            for _ in 0..8 {
                spend_host(control);
            }
        } else {
            spend(control, MaskWorkPhase::Map, 1);
            match change {
                0 | 4 => native::reset(),
                1 => native::disable(),
                2 => {
                    native::disable();
                    native::reset();
                }
                _ => unreachable!(),
            }
            spend(control, MaskWorkPhase::Map, 8);
        }
        drop(guard);
        let result = snapshot();
        let row = &result.rows[0];
        assert!(!row.attributable);
        assert!(!row.foreign_work_overlap);
        assert_eq!(row.work_before[0], 3);
        assert_eq!(row.work_after[0], 12);
        match change {
            0 | 2 | 4 => {
                assert_eq!(row.native_before.enabled, change != 4);
                assert!(row.native_after.enabled);
                assert_ne!(row.native_before.reset_epoch, row.native_after.reset_epoch);
                assert_eq!(row.native_before.lanes[0][2], 3);
                assert_eq!(row.native_after.lanes[0][2], 8);
            }
            1 => {
                assert!(!row.native_after.enabled);
                assert_eq!(row.native_before.reset_epoch, row.native_after.reset_epoch);
                assert_eq!(row.native_after.lanes[0][2], 4);
            }
            3 => {
                assert_ne!(row.host_before.reset_epoch, row.host_after.reset_epoch);
                assert_eq!(row.host_before.units[0], 3);
                assert_eq!(row.host_after.units[0], 8);
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn non_lifo_and_active_row_ordinal_bounds_are_explicit_and_raii_releases_all_slots() {
    let _session = Session::new(true);
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    let outer = ScopeGuard::new(Kind::RawDomain, control);
    let inner = ScopeGuard::new(Kind::PointContent, control);
    drop(outer);
    drop(inner);
    let result = snapshot();
    assert!(result.misnested);
    assert_eq!(result.active_intervals, 0);
    assert!(!result.rows[0].attributable);

    reset();
    let mut active = Vec::new();
    for _ in 0..MAX_ACTIVE {
        active.push(ScopeGuard::new(Kind::RawDomain, control));
    }
    let rejected = ScopeGuard::new(Kind::PointContent, control);
    spend(control, MaskWorkPhase::Map, 1);
    let result = snapshot();
    assert_eq!(result.active_intervals, MAX_ACTIVE);
    assert_eq!(result.max_active_intervals, MAX_ACTIVE);
    assert_eq!(result.dropped_intervals, 1);
    assert!(result.overflow);
    drop(rejected);
    while let Some(guard) = active.pop() {
        drop(guard);
    }
    let result = snapshot();
    assert_eq!(result.rows.len(), MAX_ACTIVE);
    assert_eq!(result.active_intervals, 0);
    assert!(!result.misnested);

    reset();
    for _ in 0..MAX_ROWS {
        let guard = ScopeGuard::new(Kind::RawDomain, control);
        spend(control, MaskWorkPhase::Map, 1);
        drop(guard);
    }
    let rejected = ScopeGuard::new(Kind::PointContent, control);
    drop(rejected);
    let result = snapshot();
    assert_eq!(result.rows.len(), MAX_ROWS);
    assert_eq!(result.rows.first().unwrap().ordinal, 1);
    assert_eq!(result.rows.last().unwrap().ordinal, MAX_ROWS);
    assert_eq!(result.active_intervals, 0);
    assert_eq!(result.dropped_intervals, 1);
    assert!(result.overflow);
    STATE.with(|state| {
        assert_eq!(state.borrow().rows.capacity(), MAX_ROWS);
        assert_eq!(state.borrow().active.capacity(), MAX_ACTIVE);
    });

    reset();
    // Observer-local ordinal bookkeeping only; no admission/work counter changes.
    STATE.with(|state| state.borrow_mut().next = usize::MAX);
    let rejected = ScopeGuard::new(Kind::RawDomain, control);
    spend(control, MaskWorkPhase::Map, 1);
    drop(rejected);
    let result = snapshot();
    assert!(result.overflow);
    assert!(result.rows.is_empty());
    assert_eq!(result.active_intervals, 0);
    assert_eq!(work.borrow().plan_work, MAX_ROWS + 2);
}

#[test]
fn observer_reset_disable_refuse_open_scope_and_unwind_preserves_partial_interval() {
    let _session = Session::new(true);
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    let guard = ScopeGuard::new(Kind::RawDomain, control);
    assert!(catch_unwind(AssertUnwindSafe(reset)).is_err());
    assert!(catch_unwind(AssertUnwindSafe(disable)).is_err());
    assert_eq!(snapshot().active_intervals, 1);
    spend(control, MaskWorkPhase::Scan, 1);
    drop(guard);
    assert_eq!(snapshot().active_intervals, 0);

    reset();
    let failure = catch_unwind(AssertUnwindSafe(|| {
        let _guard = ScopeGuard::new(Kind::PointContent, control);
        spend(control, MaskWorkPhase::Map, 2);
        panic!("bookkeeping cleanup probe");
    }));
    assert!(failure.is_err());
    let result = snapshot();
    assert_eq!(result.active_intervals, 0);
    assert_eq!(result.rows.len(), 1);
    assert!(result.rows[0].claim.is_none());
    assert!(result.rows[0].attributable);
    assert_eq!(ledger_delta(&result.rows[0], 0), 2);
    assert_eq!(lane_delta(&result.rows[0], 0, 2), 2);
}
