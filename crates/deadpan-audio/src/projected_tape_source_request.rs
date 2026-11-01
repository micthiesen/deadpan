//! Separate J source replay; G destination replay also requires the old source.
thread_local! {
    static LEGACY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(in crate::stages) fn legacy() -> bool {
    LEGACY.with(std::cell::Cell::get) || super::super::projected_signal_mask_request::legacy()
}

pub(in crate::stages) fn with_legacy_projected_tape_source<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY.with(|legacy| legacy.set(self.0));
        }
    }
    let _restore = Restore(LEGACY.with(|legacy| legacy.replace(true)));
    action()
}
