//! Test-only exact native mask operations before internal ownership.
thread_local! {
    static LEGACY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub(super) fn legacy() -> bool {
    LEGACY.with(std::cell::Cell::get)
}
pub(super) fn with_legacy_internal_masks<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY.with(|legacy| legacy.set(self.0));
        }
    }
    let _restore = Restore(LEGACY.with(|legacy| legacy.replace(true)));
    super::point_policy_request::with_legacy_policy(action)
}
