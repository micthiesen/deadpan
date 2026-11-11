//! M-only replay preserves the older intrinsic-policy replay chain.
thread_local! {
    static LEGACY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(super) fn legacy() -> bool {
    LEGACY.with(std::cell::Cell::get) || super::intrinsic_preflight_policy_request::legacy()
}
