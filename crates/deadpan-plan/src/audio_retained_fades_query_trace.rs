//! Synchronous scalar observations; test observers own any resource assertions.
use std::cell::Cell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::plan) enum Phase {
    ProcessingCopyBefore,
    ProcessingCopyAfter,
    BoundBoxBefore,
    BoundBoxAfter,
    EditorialCopyBefore,
    EditorialCopyAfter,
    OuterPushBefore { old_len: usize, old_capacity: usize },
    OuterPushAfter { new_len: usize, new_capacity: usize },
}
thread_local! {
    static OBSERVER:Cell<Option<fn(Phase)>>=const {Cell::new(None)};
}
pub(super) fn observe(phase: Phase) {
    if let Some(observer) = OBSERVER.with(Cell::get) {
        observer(phase);
    }
}
pub(in crate::plan) fn with_observer<T>(observer: fn(Phase), action: impl FnOnce() -> T) -> T {
    struct Restore(Option<fn(Phase)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            OBSERVER.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(OBSERVER.with(|slot| slot.replace(Some(observer))));
    action()
}
