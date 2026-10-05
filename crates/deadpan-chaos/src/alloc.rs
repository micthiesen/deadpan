//! Per-thread allocation accounting for bounded-memory assertions.
//!
//! A test binary opts in with
//! `#[global_allocator] static ALLOC: deadpan_chaos::CountingAllocator = deadpan_chaos::CountingAllocator;`.
//! Only the calling thread's allocations are charged, so parallel test threads
//! do not interfere. Work moved to other threads is not measured; targets that
//! spawn workers use the child-process runner and its peak RSS instead.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

static INSTALLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    // (current live bytes, peak live bytes). Copy-only state: no destructor,
    // so the allocator never registers TLS cleanup or allocates recursively.
    static STATE: Cell<(isize, isize)> = const { Cell::new((0, 0)) };
}

/// Forwarding allocator that records live and peak bytes per thread.
pub struct CountingAllocator;

fn record(delta: isize) {
    INSTALLED.store(true, Ordering::Relaxed);
    let _ = STATE.try_with(|state| {
        let (current, peak) = state.get();
        let current = current.saturating_add(delta);
        state.set((current, peak.max(current)));
    });
}

fn signed(size: usize) -> isize {
    isize::try_from(size).unwrap_or(isize::MAX)
}

// SAFETY: every method forwards the caller's layout and pointer unchanged to
// `System`, which upholds the `GlobalAlloc` contract; bookkeeping only touches
// a const-initialized, destructor-free thread-local and never allocates.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged; see the impl comment.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record(signed(layout.size()));
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged; see the impl comment.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record(signed(layout.size()));
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: forwarded unchanged; see the impl comment.
        unsafe { System.dealloc(pointer, layout) };
        record(-signed(layout.size()));
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded unchanged; see the impl comment.
        let resized = unsafe { System.realloc(pointer, layout, new_size) };
        if !resized.is_null() {
            record(signed(new_size).saturating_sub(signed(layout.size())));
        }
        resized
    }
}

/// Whether the current test binary installed [`CountingAllocator`].
pub fn installed() -> bool {
    INSTALLED.load(Ordering::Relaxed)
}

/// Start a measurement window on this thread; returns its baseline.
pub(crate) fn begin() -> isize {
    STATE.with(|state| {
        let (current, _) = state.get();
        state.set((current, current));
        current
    })
}

/// Peak bytes allocated by this thread above `baseline` since [`begin`].
pub(crate) fn peak_since(baseline: isize) -> u64 {
    STATE.with(|state| u64::try_from(state.get().1.saturating_sub(baseline)).unwrap_or(0))
}

/// Starts a whole-test allocation window on the calling thread.
pub fn allocation_baseline() -> isize {
    begin()
}

/// Peak bytes the calling thread allocated above `baseline`; zero when the
/// counting allocator is not installed.
pub fn allocation_peak_since(baseline: isize) -> u64 {
    peak_since(baseline)
}
