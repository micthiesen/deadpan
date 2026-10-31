//! Test-only ordinary Structural Point content-query lane.
thread_local! {
    static LEGACY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub(super) fn legacy() -> bool {
    LEGACY.with(std::cell::Cell::get)
}
pub(super) fn with_legacy_point_content<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(LEGACY.with(|mode| mode.replace(true)));
    super::point_suppression::with_legacy_point_suppression(action)
}
