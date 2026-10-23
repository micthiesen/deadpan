# Fine pause diagnostic: source-only handoff

Writes stopped. Only these nine PRIVATE source paths changed:
- crates/deadpan-app/src/project/service.rs
- crates/deadpan-core/src/compound.rs
- crates/deadpan-core/src/one_leaf_replay.rs
- crates/deadpan-core/src/pause_route_profile.rs
- crates/deadpan-core/src/semantic/planner.rs
- crates/deadpan-core/src/semantic/planner/content.rs
- crates/deadpan-core/src/semantic/planner/pause.rs
- crates/deadpan-store/src/compound.rs
- crates/deadpan-store/src/lib.rs

Exact scope: semantic initial/final document sites are in planner.rs; charge_step in planner/content.rs; post-leaf admission in planner/pause.rs; pure apply/snapshot/reuse in one_leaf_replay.rs; replay in Core compound.rs; exact sizes/visitor in Store compound.rs; history codec/inverse in Store lib.rs. App service.rs changes only emitter names/counts/prefix. Gate, native selection, success/error/publication lifecycle and original13exclusive categories remain unchanged.

V2 emits28lines: BEGIN,26ordered PHASE rows, END. Original13counts remain1. Added rows/counts:
- semantic_document_wire: 4
- semantic_leaf_apply: 1
- capability_snapshot: 1
- replay_document_wire: 4
- reuse_eligibility_eq: 1
- reuse_clone_exit: 1
- replay_net_aggregate: 1
- replay_misc: 1
- store_compact_size: 3
- store_checkpoint_wire: 0
- store_visitor_authority: 1
- history_codec_readback: 1
- history_inverse_restore: 1

EXPECTED_COUNTS is constant. Parent times remain exclusive residuals when children run. Zero-count checkpoint time must also be zero. All original serializers, charges, Eq/clones, TLS entry/exit, net construction, visitor authority, history codecs/inverse and argument expressions remain in their existing order. No algorithm/optimization or Main change.

Named4+4+3 counts identify only these original document-size sites, not every internal serialization. Store final compact-size region includes its existing byte-limit check; semantic/replay wire regions include their original charge where present. No new serialization or extra authority work is added.

Static maximum complete trace size with expected counts/u64elapsed is2774bytes; even20digit u64counts everywhere would need3268bytes. Existing4096byte App stack buffer is retained. Exact prefix/schema/counts/bounds and8testnames: schema-and-tests.json. Before/after snapshots, owned hashes/nonowned audit and instrumentation.patch are beside this note. All2045nonowned Rust files and6inputs are byte-identical; source remains2054Rustfiles.

Eight helper tests, unchanged names with refined shape/count/zero/conservation coverage:
- pause_route_profile_nested_exclusive_durations_conserve_and_resume_without_counts
- pause_route_profile_live_success_clears_before_returning_scalars
- pause_route_profile_failure_and_cancel_leave_no_finished_block
- pause_route_profile_out_of_order_drop_and_reentrant_activation_discard
- pause_route_profile_borrow_conflict_invalidates_without_panicking
- pause_route_profile_invalid_counters_markers_clock_and_stack_are_rejected
- pause_route_profile_wrong_counts_missing_success_and_broken_conservation_discard
- pause_route_profile_inactive_hooks_do_not_create_a_session

Root owns standalone exact-helper testing, formatting, strictlint, releaseApp and native trials. No Cargo/rustfmt/compiler/runtime/Git ran. Remaining uncertainty is compilation/strictlint and actual target call counts/disabled-path parity; none is claimed passed. No fine phase costs or overhead results are claimed.
