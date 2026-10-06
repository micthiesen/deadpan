//! Local replacements for whole-document command work, and their oracle.
//!
//! Several command steps used to repeat a complete pass over the document:
//! intermediate results were validated again, lineage and mark transforms
//! each rebuilt the structural durations of the same result, and capture
//! serialized the whole binding state only to count its bytes. The current
//! code computes each of those once, or computes an exact local equivalent;
//! see `docs/TIMING_STORAGE.md#command-work-reuse` for each argument.
//!
//! [`with_reference_command_work`] restores the previous computations on the
//! current thread. Production code never calls it; equivalence tests run the
//! same random edit sequences both ways and require identical transactions,
//! results and refusals.

use std::cell::Cell;

thread_local! {
    static REFERENCE: Cell<bool> = const { Cell::new(false) };
}

/// Run `operation` with every whole-document command step computed as it was
/// before its local replacement. The setting is thread-local and restored on
/// return, including on unwind.
#[doc(hidden)]
#[cfg(any(test, feature = "test-support"))]
pub fn with_reference_command_work<R>(operation: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            REFERENCE.with(|flag| flag.set(self.0));
        }
    }
    let _restore = Restore(REFERENCE.with(|flag| flag.replace(true)));
    operation()
}

/// True when the local replacements apply (always, outside the oracle).
pub(crate) fn local() -> bool {
    !REFERENCE.with(Cell::get)
}

/// The merged map difference used by every patch, as `(key, before, after)`
/// rows, for equivalence tests outside the crate.
#[doc(hidden)]
#[cfg(any(test, feature = "test-support"))]
pub fn diff_for_tests<K: Ord + Clone, V: Eq + Clone>(
    before: &std::collections::BTreeMap<K, V>,
    after: &std::collections::BTreeMap<K, V>,
) -> Vec<(K, Option<V>, Option<V>)> {
    crate::command::diff(before, after)
        .into_iter()
        .map(|(key, change)| (key, change.before, change.after))
        .collect()
}

/// The capture's binding byte check, as `(structural bound, outcome)`, for
/// tests comparing the bounded path with `AudioBindingState::to_json`.
#[doc(hidden)]
#[cfg(any(test, feature = "test-support"))]
pub fn binding_wire_check_for_tests(
    state: &crate::AudioBindingState,
) -> (Option<usize>, Result<(), crate::DocumentError>) {
    (state.wire_bound_for_tests(), state.check_wire_size())
}
