//! Literal permission projection before the non-separate SharedMix destination.
thread_local! {
    static LEGACY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub(super) fn legacy() -> bool {
    LEGACY.with(std::cell::Cell::get)
}
pub(super) fn with_legacy_shared_mix<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY.with(|legacy| legacy.set(self.0));
        }
    }
    let _restore = Restore(LEGACY.with(|legacy| legacy.replace(true)));
    action()
}
