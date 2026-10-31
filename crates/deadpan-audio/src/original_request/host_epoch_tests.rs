//! Private child of flattened_work_trace; no exported bookkeeping setter.
use super::*;

#[test]
fn host_reset_epoch_overflow_is_sticky_without_changing_legacy_snapshot_shape() {
    struct Restore {
        epoch: Option<u64>,
        trace: [Snapshot; 4],
        host: HostEvictions,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            HOST_RESET_EPOCH.with(|value| value.set(self.epoch));
            TRACE.with(|value| value.set(self.trace));
            HOST_EVICTIONS.with(|value| value.set(self.host));
        }
    }
    let _restore = Restore {
        epoch: HOST_RESET_EPOCH.with(Cell::get),
        trace: TRACE.with(Cell::get),
        host: HOST_EVICTIONS.with(Cell::get),
    };
    TRACE.with(|value| value.set([Snapshot::default(); 4]));
    HOST_RESET_EPOCH.with(|value| value.set(Some(u64::MAX)));
    reset();
    assert_eq!(host_eviction_boundary().2, None);
    assert_eq!(host_eviction_boundary().0, [0; 2]);
    reset();
    assert_eq!(host_eviction_boundary().2, None);
    assert_eq!(
        serde_json::to_value(snapshot()).unwrap(),
        serde_json::to_value(Snapshot::default()).unwrap()
    );
    assert!(
        !serde_json::to_value(snapshot())
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("reset_epoch")
    );
}
