//! Default-off native mask work, independently recorded in the actual host lane.
use super::MaskWorkPhase;
use serde::Serialize;
use std::cell::Cell;

#[derive(Clone, Copy, Default, Serialize)]
pub(crate) struct Lane {
    accepted_account_units: usize,
    accepted_scan_units: usize,
    accepted_map_units: usize,
    accepted_zero_units: usize,
    accepted_mark_units: usize,
    accepted_vector_units: usize,
    accepted_storage_units: usize,
    initialized_account_shells: usize,
    initialized_account_frames: u64,
    accepted_payload_reservations: usize,
    accepted_payload_frames: u64,
    accepted_host_residency_units: usize,
}
#[derive(Clone, Copy, Default, Serialize)]
pub(crate) struct Snapshot {
    plan: Lane,
    cache: Lane,
    overflow: bool,
}
thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static TRACE: Cell<Snapshot> = Cell::new(Snapshot::default());
}
fn update(cache: bool, action: impl FnOnce(&mut Lane, &mut bool)) {
    if ENABLED.with(Cell::get) {
        TRACE.with(|trace| {
            let mut snapshot = trace.get();
            let lane = if cache {
                &mut snapshot.cache
            } else {
                &mut snapshot.plan
            };
            action(lane, &mut snapshot.overflow);
            trace.set(snapshot);
        });
    }
}
fn add_usize(target: &mut usize, amount: usize, overflow: &mut bool) {
    if let Some(next) = target.checked_add(amount) {
        *target = next;
    } else {
        *overflow = true;
    }
}
fn add_u64(target: &mut u64, amount: u64, overflow: &mut bool) {
    if let Some(next) = target.checked_add(amount) {
        *target = next;
    } else {
        *overflow = true;
    }
}
pub(crate) fn reset() {
    TRACE.with(|trace| trace.set(Snapshot::default()));
    ENABLED.with(|enabled| enabled.set(true));
}
pub(crate) fn disable() {
    ENABLED.with(|enabled| enabled.set(false));
}
pub(crate) fn snapshot() -> Snapshot {
    TRACE.with(Cell::get)
}
pub(super) fn spend_accepted(cache: bool, phase: MaskWorkPhase, units: usize) {
    update(cache, |lane, overflow| {
        let value = match phase {
            MaskWorkPhase::Account => &mut lane.accepted_account_units,
            MaskWorkPhase::Scan => &mut lane.accepted_scan_units,
            MaskWorkPhase::Map => &mut lane.accepted_map_units,
            MaskWorkPhase::Zero => &mut lane.accepted_zero_units,
            MaskWorkPhase::Mark => &mut lane.accepted_mark_units,
            MaskWorkPhase::AllocateVector => &mut lane.accepted_vector_units,
            MaskWorkPhase::AllocateStorage => &mut lane.accepted_storage_units,
        };
        add_usize(value, units, overflow);
    });
}
pub(crate) fn shell_initialized(cache: bool, frames: u64) {
    update(cache, |lane, overflow| {
        add_usize(&mut lane.initialized_account_shells, 1, overflow);
        add_u64(&mut lane.initialized_account_frames, frames, overflow);
    });
}
pub(crate) fn reservation_accepted(cache: bool, frames: u64) {
    update(cache, |lane, overflow| {
        add_usize(&mut lane.accepted_payload_reservations, 1, overflow);
        add_u64(&mut lane.accepted_payload_frames, frames, overflow);
    });
}
pub(crate) fn host_residency_accepted(cache: bool, units: usize) {
    update(cache, |lane, overflow| {
        add_usize(&mut lane.accepted_host_residency_units, units, overflow);
    });
}
