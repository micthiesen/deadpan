//! Test-only switch for whole-read OLD raw-domain observations.
thread_local! {
    static LEGACY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub(super) fn legacy() -> bool {
    LEGACY.with(std::cell::Cell::get)
}
pub(super) fn with_legacy_raw_domain<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(LEGACY.with(|mode| mode.replace(true)));
    action()
}
