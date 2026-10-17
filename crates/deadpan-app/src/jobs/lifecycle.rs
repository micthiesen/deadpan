//! Nonblocking sleep notifications shared with the sole project writer.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Every notification advances the epoch; odd values mean asleep. Keeping
/// the epoch, rather than only an awake flag, retains a sleep/wake pair even
/// when the writer cannot observe it until the machine has already resumed.
#[derive(Clone, Default)]
pub struct Lifecycle(Arc<AtomicU64>);

impl Lifecycle {
    pub fn snapshot(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }

    pub fn is_awake(&self) -> bool {
        self.snapshot() & 1 == 0
    }

    pub fn interrupted_since(&self, captured: u64) -> bool {
        let current = self.snapshot();
        current != captured || current & 1 != 0
    }

    /// Safe in a native notification callback: no locks, I/O or allocation.
    pub fn will_sleep(&self) {
        self.advance(true);
    }

    /// Wake invalidates old work too, including when sleep was not observed.
    pub fn did_wake(&self) {
        self.advance(false);
    }

    fn advance(&self, asleep: bool) {
        let _ = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                // Exhaustion fails closed, rather than reviving an old epoch.
                Some(
                    old.checked_add(2)
                        .map_or(u64::MAX, |next| if asleep { next | 1 } else { next & !1 }),
                )
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesced_sleep_and_wake_cannot_revive_a_captured_job() {
        let lifecycle = Lifecycle::default();
        let before = lifecycle.snapshot();
        assert!(!lifecycle.interrupted_since(before));
        lifecycle.will_sleep();
        assert!(!lifecycle.is_awake());
        assert!(lifecycle.interrupted_since(lifecycle.snapshot()));
        lifecycle.did_wake();
        assert!(lifecycle.is_awake());
        assert!(lifecycle.interrupted_since(before));
        assert!(!lifecycle.interrupted_since(lifecycle.snapshot()));
    }

    #[test]
    fn counter_exhaustion_stays_interrupted_and_asleep() {
        let lifecycle = Lifecycle(Arc::new(AtomicU64::new(u64::MAX - 1)));
        lifecycle.did_wake();
        assert!(!lifecycle.is_awake());
        lifecycle.did_wake();
        assert_eq!(lifecycle.snapshot(), u64::MAX);
    }
}
