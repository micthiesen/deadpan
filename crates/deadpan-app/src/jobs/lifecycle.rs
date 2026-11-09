//! Nonblocking sleep notifications shared with the sole project writer, and
//! the per-run capture every sleep-interrupted worker uses.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Cancelled,
    Interrupted,
}

#[derive(Clone)]
pub struct RunCapture {
    lifecycle: crate::jobs::Lifecycle,
    pub epoch: u64,
    cancel: Arc<AtomicBool>,
    cause: Arc<Mutex<Option<Stop>>>,
}

impl RunCapture {
    pub fn new(lifecycle: crate::jobs::Lifecycle, cancel: Arc<AtomicBool>) -> Self {
        let epoch = lifecycle.snapshot();
        Self {
            lifecycle,
            epoch,
            cancel,
            cause: Arc::new(Mutex::new(None)),
        }
    }

    /// Cancellation already requested when interruption is observed takes
    /// precedence. This does not claim chronological ordering of racing events.
    /// Retain our own cause so the flag we set is never read as user cancellation.
    pub fn stop(&self) -> Option<Stop> {
        let mut cause = self.cause.lock().unwrap_or_else(|error| error.into_inner());
        if cause.is_none() {
            if self.cancel.load(Ordering::Acquire) {
                *cause = Some(Stop::Cancelled);
            } else if self.lifecycle.interrupted_since(self.epoch) {
                *cause = Some(Stop::Interrupted);
                self.cancel.store(true, Ordering::Release);
            }
        }
        *cause
    }

    /// The scoped watcher can cancel native inference without waiting for a
    /// UI repaint or a progress callback. It ends when this worker returns.
    pub fn run<T>(&self, work: impl FnOnce() -> T) -> T {
        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                while !done.load(Ordering::Acquire) {
                    if self.stop().is_some() {
                        break;
                    }
                    std::thread::park_timeout(Duration::from_millis(20));
                }
            });
            struct Finish<'a>(&'a AtomicBool);
            impl Drop for Finish<'_> {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::Release);
                }
            }
            let _finish = Finish(&done);
            work()
        })
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

    #[test]
    fn sleeping_native_work_is_cancelled_without_ui_or_progress_callbacks() {
        let lifecycle = crate::jobs::Lifecycle::default();
        let cancel = Arc::new(AtomicBool::new(false));
        let run = RunCapture::new(lifecycle.clone(), cancel.clone());
        run.run(|| {
            lifecycle.will_sleep();
            lifecycle.did_wake();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while !cancel.load(Ordering::Acquire) {
                assert!(
                    std::time::Instant::now() < deadline,
                    "native worker cancellation was not signalled"
                );
                std::thread::park_timeout(Duration::from_millis(5));
            }
        });
        assert_eq!(run.stop(), Some(Stop::Interrupted));
        assert_eq!(
            run.stop(),
            Some(Stop::Interrupted),
            "own cancellation flag cannot change the cause"
        );
    }

    #[test]
    fn cancellation_already_requested_takes_precedence_over_coalesced_sleep() {
        let lifecycle = crate::jobs::Lifecycle::default();
        let cancel = Arc::new(AtomicBool::new(false));
        let run = RunCapture::new(lifecycle.clone(), cancel.clone());
        cancel.store(true, Ordering::Release);
        lifecycle.will_sleep();
        lifecycle.did_wake();
        assert_eq!(run.stop(), Some(Stop::Cancelled));
    }
}
