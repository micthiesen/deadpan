//! L-only replay preserves every older native replay through J's legacy chain.
thread_local! {
    static LEGACY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(super) fn legacy() -> bool {
    LEGACY.with(std::cell::Cell::get) || super::routed::projected_tape_source_request::legacy()
}

pub(super) fn with_legacy_intrinsic_preflight_policy<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY.with(|legacy| legacy.set(self.0));
        }
    }
    let _restore = Restore(LEGACY.with(|legacy| legacy.replace(true)));
    action()
}
