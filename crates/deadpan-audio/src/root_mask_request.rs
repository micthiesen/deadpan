//! Exact final Root mask collector before native destination admission.
thread_local! {
    static LEGACY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub(super) fn legacy() -> bool {
    LEGACY.with(std::cell::Cell::get)
}
pub(super) fn with_legacy_root_masks<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY.with(|legacy| legacy.set(self.0));
        }
    }
    let _restore = Restore(LEGACY.with(|legacy| legacy.replace(true)));
    super::shared_mix_mask_request::with_legacy_shared_mix(action)
}
