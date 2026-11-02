//! O-only replay preserves the N and earlier policy replay chain.
thread_local! {
    static LEGACY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(super) fn legacy() -> bool {
    LEGACY.with(std::cell::Cell::get) || super::read_signal_policy_request::legacy()
}

pub(super) fn with_legacy_stage_block_policy<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY.with(|legacy| legacy.set(self.0));
        }
    }
    let _restore = Restore(LEGACY.with(|legacy| legacy.replace(true)));
    action()
}
