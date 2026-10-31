//! Inactive test observation of the actual admitted Point policy arm.
use std::{cell::RefCell, ops::Range};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::plan) enum Phase {
    BeforeClone,
    AfterClone,
    BeforeOffset,
    AfterOffset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::plan) struct Context {
    pub(in crate::plan) origin: i64,
    pub(in crate::plan) reference_support: [i64; 2],
    pub(in crate::plan) selection: [i64; 2],
    pub(in crate::plan) signal_range: [i64; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::plan) struct Event {
    pub(in crate::plan) phase: Phase,
    pub(in crate::plan) context: Context,
    pub(in crate::plan) ranges: usize,
    pub(in crate::plan) first: Option<[i64; 2]>,
    pub(in crate::plan) last: Option<[i64; 2]>,
    pub(in crate::plan) canonical: usize,
    pub(in crate::plan) additional: usize,
}

type Observer = Box<dyn FnMut(Event)>;
thread_local! {
    static OBSERVER: RefCell<Option<Observer>> = const { RefCell::new(None) };
}

pub(in crate::plan) struct Scope;
impl Scope {
    pub(in crate::plan) fn start(observer: impl FnMut(Event) + 'static) -> Self {
        OBSERVER.with(|slot| {
            let mut slot = slot.borrow_mut();
            assert!(slot.is_none(), "nested admitted Point observation");
            *slot = Some(Box::new(observer));
        });
        Self
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        OBSERVER.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

pub(in crate::plan) fn record(
    phase: Phase,
    context: Context,
    ranges: &[Range<i64>],
    work: (usize, usize),
) {
    OBSERVER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(observer) = slot.as_mut() else {
            return;
        };
        observer(Event {
            phase,
            context,
            ranges: ranges.len(),
            first: ranges.first().map(|range| [range.start, range.end]),
            last: ranges.last().map(|range| [range.start, range.end]),
            canonical: work.0,
            additional: work.1,
        });
    });
}
