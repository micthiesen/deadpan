//! Private child of ops_trace; no exported epoch setter or admission modification.
use super::*;

#[test]
fn native_reset_epoch_overflow_stays_unknown_through_disable_and_reset() {
    struct Restore {
        epoch: Option<u64>,
        enabled: bool,
        trace: Snapshot,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            RESET_EPOCH.with(|value| value.set(self.epoch));
            ENABLED.with(|value| value.set(self.enabled));
            TRACE.with(|value| value.set(self.trace));
        }
    }
    let _restore = Restore {
        epoch: RESET_EPOCH.with(Cell::get),
        enabled: ENABLED.with(Cell::get),
        trace: TRACE.with(Cell::get),
    };
    // Overflow only this observer's private reset generation, not real work.
    RESET_EPOCH.with(|value| value.set(Some(u64::MAX)));
    reset();
    assert!(work_boundary().enabled);
    assert_eq!(work_boundary().reset_epoch, None);
    assert_eq!(
        serde_json::to_value(snapshot()).unwrap(),
        serde_json::to_value(Snapshot::default()).unwrap()
    );
    disable();
    assert!(!work_boundary().enabled);
    assert_eq!(work_boundary().reset_epoch, None);
    reset();
    assert!(work_boundary().enabled);
    assert_eq!(work_boundary().reset_epoch, None);
    assert!(
        !serde_json::to_value(snapshot())
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("reset_epoch")
    );
}
