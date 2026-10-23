# Private semantic refresh duration reuse: source-only handoff

Source: /tmp/deadpan-semantic-refresh-duration-reuse-2026-10-23/source, fresh56f74856 baseline. This prototype is separate from the main checkout.

Only owned changes:
- crates/deadpan-core/src/semantic/planner/selection.rs:14-line guarded branch inside refresh_children.
- NEW crates/deadpan-core/tests/semantic_refresh.rs: four standalone integration regressions; all helper functions private.
No other source, Cargo, docs or API changes.

Production review:
The branch requires command_work::local() and empty current.derived_clocks(). It borrows the exact same current immutably, obtains durations_shared()? once, splits mutable bounds/ends/indices references, and executes the three original assignments in their original order inside current.with_proved_durations. Each question-mark assignment retains its previous partial-field update behavior on failure. The scope cannot outlive the immutable borrow or overlap current replacement. No mutable self capture or method call occurs inside it.
Removing exactly the new branch reproduces the original selection.rs byte-for-byte. Reference and derived fallback remains literally original. scope_bounds, both source_splice_boundary queries, both AnchorIndex constructions, child_ends, query order, charges and all edit/provider/allocation/history code are unchanged. No production counter, proof type, cache or public API was added.

Standalone tests (prefix semantic_refresh_):
1. wide_bound_document_replaces_the_proof_after_each_pause:128 four-frame Holds, genuine retained bindings from a seam pause and a real local mark. Interior pause/motion/pause/motion checks bounds514/517/519, ranges41..44 and48..50, cursor/selection and exact transaction replay. Motion after both edits exposes a stale refresh map.
2. nested_scope_keeps_zero_groups_marks_and_outside_siblings: ordinary nested Sequence, zero-duration groups, outside siblings and existing mark; pause/motion/cut/motion checks absolute bounds3..17 ->3..19 ->3..17 and a nonempty register write set.
3. late_provider_and_allocator_failures_keep_order_and_inputs: an earlier cut stages register writes, one pause succeeds, then the second provider or second pause allocator fails. Exact errors/event records/order match reference, including that provider failure prevents its allocation; input document bytes, context and bank remain equal.
4. nested_follow_speed_pauses_keep_the_derived_clock_fallback: proportionate copy of the existing nested FollowSpeed pause fixture, with two edits, exact164-frame result, derived clocks and unchanged reference result/replay.

Shared test comparison checks every public SemanticPlan field separately: document, request, context, selected_child, register_writes, trace. It compares exact compact document/request bytes, full resolver/allocator event records, local/reference Compound replay/edit/register writes and forward/inverse application/restoration. SemanticPlan remains Debug without a production Eq implementation.

Fixture uncertainties reported before compilation:
- All tests are source-only and unexecuted. Exact trace ranges, cursor/selection and empty-group survival are derived from the inspected behavior and asserted rather than measured.
- The derived fallback is included, so no derived-specific omission requires another fixture. The existing nested_follow_speed_semantic_pauses_refresh_exact_scope_and_replay_as_one_edit remains part of Root's later broader suite.
- The128-Hold integration fixture is correctness evidence only; native fresh10k remains the performance qualification workload.
No costs or gains are measured here. The older30ms semantic residual is not attributed to refresh.

Audit/status:
All eight design source hashes and all2050 original Rust hashes matched before editing. Full-tree audit confirms34420 nonowned baseline files unchanged among34421 original files. Only selection.rs changed and semantic_refresh.rs was added; no file deleted.
Before/proposed .rs.txt snapshots, instrumentation.patch, exact source-hashes.json and source-review.json are retained here.
Uncompiled, unformatted and untested. This worker ran no Cargo, formatter, Git, build, runtime or measurement command. Root owns all execution and qualification.
