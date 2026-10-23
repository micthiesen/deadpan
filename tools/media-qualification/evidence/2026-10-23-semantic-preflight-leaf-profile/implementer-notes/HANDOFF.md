# Source-only V3 microprofile handoff

Source writes stopped. Unformatted and uncompiled. No Cargo, rustfmt, compiler, tests, runtime, Git or Main writes were performed.

Exactly the seven assigned private source files changed; before/after .rs.txt snapshots, owned hashes, full proposed 2054 Rust map, instrumentation.patch, schema.json and eight-test name manifest are beside this note. Hash audit found 2047 nonowned Rust unchanged and all six pinned inputs unchanged.

## Schema and bounded storage

Prefix DEADPAN_PAUSE_ROUTE_PROFILE_V3. Exactly 37 lines: BEGIN count=1,35 PHASE rows in schema.json order, END count=1 elapsed_ns=<u64> committed=1 refreshed=1 published=1. Phase line grammar remains PHASE name=<fixed> elapsed_ns=<u64 decimal> count=<fixed>. Original 26 names/order/counts are preserved, including 4 semantic wire, 4 replay wire, 3 Store compact size and 0 checkpoint. Nine appended counts are1. Zero count requires 0 ns; checked exclusive sum equals END elapsed_ns.

The fixed stack buffer remains 4096 bytes. With every nonzero row independently 20 digits and checkpoint 0 ns, the complete trace is at most 3699 bytes, including newlines. The tighter maximum under exclusive u64 conservation is 3647 bytes: 34 nonzero-count rows can afford 16 nineteen-digit and 18 eighteen-digit durations (628 digits total), plus the zero row and 20-digit outer total. The remaining budget can be assigned without changing those lengths. Both bounds fit; no heap formatting was added. Stack limit remains 8; the new leaf nesting reaches 4 frames and the existing Store/visitor nesting remains within that bound.

## Exact new boundaries

| Appended row | Boundary |
| --- | --- |
| semantic_head_validate | Existing document.validate only, after program.validate and before revision check. |
| semantic_context_admission | Existing validate_context call only, before unchanged Original-context check. |
| semantic_planner_setup | After initial document-wire span through original sets/indices/Planner initializer, before execute; field expressions untouched. |
| semantic_pause_target | Original target if/expression, after existing at/scoped locals and before within_scope check. |
| semantic_pause_provider | Original provider(self) call only. |
| semantic_children_refresh | Whole original refresh_children method, including existing same-borrow proof path and fallback. |
| leaf_insert_known | Original ordinary InsertTime reduction region, including scoped capture and complete-capture fallback selection, with one guard over both. |
| leaf_input_durations | Original document.durations call in ordinary apply_known; each actual call counts. |
| leaf_net_construction | Original final net_transaction_known call site in apply_with_durations_captured; not generic helper/replay aggregate. |

Existing semantic_plan, semantic_leaf_apply and nested leaf_insert_known durations are exclusive residuals while children run. No old-run median subtraction is valid. Original authored calls, argument expressions, error/refusal paths and lexical authored-value ownership remain in place. Explicit timer drops bound successful call results; errors naturally drop timer guards. No additional queries, serializers, authority checks or authored state were added. Core helper still contains no env or I/O; App activation and success/publication lifecycle are unchanged. Numeric TLS accesses return before measured calls, so no TLS borrow crosses authored work. App emission stays best-effort after publication and outside active timing.

## Eight helper tests

- pause_route_profile_nested_exclusive_durations_conserve_and_resume_without_counts
- pause_route_profile_live_success_clears_before_returning_scalars
- pause_route_profile_failure_and_cancel_leave_no_finished_block
- pause_route_profile_out_of_order_drop_and_reentrant_activation_discard
- pause_route_profile_borrow_conflict_invalidates_without_panicking
- pause_route_profile_invalid_counters_markers_clock_and_stack_are_rejected
- pause_route_profile_wrong_counts_missing_success_and_broken_conservation_discard
- pause_route_profile_inactive_hooks_do_not_create_a_session

The deterministic/live success graphs now contain all 35 rows. The deterministic oracle checks exact child/residual durations, 4/4/3/0 counts and sum conservation. Wrong-count rejection now includes all nine new rows. Existing failure/cancel, stale/out-of-order drop, reentrancy, borrow conflict, checked elapsed/count/marker overflow, backwards clock, stack bound, zero-row ghost duration and inactive-session cases remain. These are source assertions only; Root owns execution.

## Remaining uncertainty

Root must format, independently review, standalone-test the exact helper source, strict-build the real App integration and qualify counts/state parity gate off/on. In particular, if ordinary InsertTime takes its existing complete-capture fallback and reaches input durations twice, the actual count becomes 2 and the diagnostic block is discarded rather than hiding that work or emitting an expected 1 claim. Derived/alternate instruction shapes may likewise fail the qualified single-pause mask. No timing, overhead, cost attribution or optimization gain is claimed.

Patch SHA256: bb752631840e0b98ad35a765b448cf685c6169e0782a94c58d2e4bfb024093ac
