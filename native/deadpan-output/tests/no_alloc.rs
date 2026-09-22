//! Instrument the Rust queue callback, including failure and invalidation paths.
//! This does not wrap CoreAudio or CPAL's work preceding our callback.
use deadpan_output::{MAX_CALLBACK_FRAMES, QUEUE_PACKETS, channel};

#[allow(unsafe_code)]
mod allocation_probe {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static ACTIVE: Cell<bool> = const { Cell::new(false) };
        static COUNTS: Cell<[usize; 4]> = const { Cell::new([0; 4]) };
    }

    struct Counting;

    fn count(kind: usize) {
        if ACTIVE.try_with(Cell::get).unwrap_or(false) {
            let _ = COUNTS.try_with(|counts| {
                let mut value = counts.get();
                value[kind] += 1;
                counts.set(value);
            });
        }
    }

    // SAFETY: Every allocation/deallocation forwards the original pointer,
    // layout and size unchanged to System. TLS counters own no heap storage.
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            count(0);
            unsafe { System.alloc(layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            count(1);
            unsafe { System.alloc_zeroed(layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            count(2);
            unsafe { System.realloc(ptr, layout, size) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            count(3);
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: Counting = Counting;

    struct Scope;
    impl Drop for Scope {
        fn drop(&mut self) {
            ACTIVE.set(false);
        }
    }

    pub fn measure<T>(f: impl FnOnce() -> T) -> (T, [usize; 4]) {
        COUNTS.set([0; 4]);
        ACTIVE.set(true);
        let scope = Scope;
        let result = f();
        drop(scope);
        (result, COUNTS.get())
    }
}

#[test]
fn callback_has_no_heap_operations_on_content_silence_seek_or_fault() {
    let (mut feed, mut callback) = channel().unwrap();
    let mut output = [0.0; MAX_CALLBACK_FRAMES * 2];
    let mut render = |size: usize| {
        let (_, counts) = allocation_probe::measure(|| callback.render(&mut output[..size]));
        assert_eq!(counts, [0; 4], "alloc/zeroed/realloc/dealloc");
    };
    render(512); // paused
    let generation = feed.restart(0).unwrap();
    for _ in 0..QUEUE_PACKETS {
        feed.submit(generation, &[[0.1, -0.1]; 256]).unwrap();
    }
    feed.activate(generation).unwrap();
    render(2); // retain most of a partial packet
    feed.restart(9_000).unwrap();
    render(512); // discard a full stale queue and partial packet
    render(512); // preparation stays silent before activation
    let generation = feed.restart(10_000).unwrap();
    feed.submit(generation, &[[0.2, -0.2]; 256]).unwrap();
    feed.finish(generation).unwrap();
    feed.activate(generation).unwrap();
    render(1_024); // content and end marker
    render(512); // ended
    let generation = feed.restart(11_000).unwrap();
    feed.submit(generation, &[[0.1, -0.1]; 2]).unwrap();
    feed.activate(generation).unwrap();
    render(512); // starvation
    render(512); // starvation remains latched
    feed.pause().unwrap();
    render(512);
    render(511); // invalid shape, permanent fault
    render(512);
}
