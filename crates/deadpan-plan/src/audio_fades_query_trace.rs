//! Synchronous test-only phase observations. Observers own any ledger assertions.
use deadpan_core::AudioSample;
use std::cell::Cell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::plan) enum Phase {
    PrimaryLeaf,
    PrimaryStage,
    PrimaryBound,
    EnvelopeNone,
    EnvelopeRootRoundEven,
    EnvelopePointCeil,
    BeforeEnvelopeSupport,
    AfterEnvelopeSupport,
    RetainedGeometryLeaf,
    InitialStartBefore,
    InitialStartAfter,
    InitialEndBefore,
    InitialEndAfter,
    OriginGrowthBefore,
    OriginGrowthAfter,
    OriginMergeBefore,
    OriginMergeReady,
    OriginMergeAfter,
    SideGrowthBefore,
    SideGrowthAfter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::plan) struct Event {
    pub(in crate::plan) phase: Phase,
    pub(in crate::plan) sample: Option<i64>,
    pub(in crate::plan) old_len: usize,
    pub(in crate::plan) new_len: usize,
}
impl Event {
    pub(super) fn at(phase: Phase, sample: AudioSample) -> Self {
        Self {
            phase,
            sample: Some(sample.0),
            old_len: 0,
            new_len: 0,
        }
    }
    pub(super) fn lengths(phase: Phase, old_len: usize, new_len: usize) -> Self {
        Self {
            phase,
            sample: None,
            old_len,
            new_len,
        }
    }
}
thread_local! {
    static OBSERVER: Cell<Option<fn(Event)>> = const { Cell::new(None) };
}
pub(super) fn observe(event: Event) {
    if let Some(observer) = OBSERVER.with(Cell::get) {
        observer(event);
    }
}
pub(in crate::plan) fn with_observer<T>(observer: fn(Event), action: impl FnOnce() -> T) -> T {
    struct Restore(Option<fn(Event)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            OBSERVER.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(OBSERVER.with(|slot| slot.replace(Some(observer))));
    action()
}
