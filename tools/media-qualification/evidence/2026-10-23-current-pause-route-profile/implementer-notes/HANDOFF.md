# Coarse pause route diagnostic: source-only handoff

Writes stopped. Only these five PRIVATE source paths changed:
- crates/deadpan-app/src/project/service.rs
- crates/deadpan-app/src/project/service/macros.rs
- crates/deadpan-core/src/lib.rs
- crates/deadpan-core/src/pause_route_profile.rs
- crates/deadpan-store/src/compound.rs

Before/after .rs.txt snapshots, exact hashes, proposed Rust map, instrumentation.patch, trace-schema.json and unit-tests.json are in this directory. All2,049nonowned baseline Rust files and six inputs remain byte-identical; proposed Rust count2,054. Main was untouched. No Cargo/fmt/compiler/runtime/Git ran.

Gate DEADPAN_PAUSE_ROUTE_PROFILE=1 is read once per native service worker. Activation is native Macro Apply(InsertPause) only; named Run, remote/headless, annotation and background paths cannot activate it. The exact13categories/15line frozen grammar is implemented. All phase counters must be1, commit/refresh/publication markers1, stack balanced and checked disjoint sum equal checked u64 outerelapsed. Output is one best-effort fixed4096byte App stack buffer after publication and timing-state deactivation. Core retains only bounded numeric TLS, Instant origin and thread-bound guards; inactive hooks read no clock. Errors/cancellation/mismatch/overflow discard diagnostics. Existing commit, refresh/error and publication branches retain their result/authority behavior.

Eight new helper tests (unexecuted):
- pause_route_profile_nested_exclusive_durations_conserve_and_resume_without_counts
- pause_route_profile_live_success_clears_before_returning_scalars
- pause_route_profile_failure_and_cancel_leave_no_finished_block
- pause_route_profile_out_of_order_drop_and_reentrant_activation_discard
- pause_route_profile_borrow_conflict_invalidates_without_panicking
- pause_route_profile_invalid_counters_markers_clock_and_stack_are_rejected
- pause_route_profile_wrong_counts_missing_success_and_broken_conservation_discard
- pause_route_profile_inactive_hooks_do_not_create_a_session

Root command: cargo test --locked --release -p deadpan-core --lib pause_route_profile

Remaining uncertainty is compilation/Clippy and actual native output/counts/disabled-path parity, all owned by Root. Discarded helper Options explicitly use let_. The tiny written tuple local in Store only preserves the original write result across explicit diagnostic guard drop before Some; no authored call/argument ordering changes. No phase costs, timing overhead or memory claims.
