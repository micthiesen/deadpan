# Isolated Core work profile: source-only handoff

The overlay is implemented only in /tmp/deadpan-core-planning-detail-profile-2026-10-23/source. It starts from the fresh 90850b09 copy; none of the previous Store detail overlay was layered.

Status: uncompiled and unformatted. This worker ran no Cargo, formatter, Git, tests, app/profile process, runtime command, or measurement job. Root owns all of those. Review was source-only. All 2050 original Rust hashes match Root's baseline before edits. A broader before/after hash audit covers 33670 baseline files and confirms only the eight assigned existing files plus the new temporary Core work_profile.rs changed. All 33662 nonowned baseline files remain byte-identical, including all CLI sources and Store lib.rs.

Artifacts:
- instrumentation.patch: complete preformat unified patch
- source-hashes.json: exact before/after changed source identities and audit counts
- before-source-hashes.json: all33670 baseline file identities
- changed-files.txt: exactly8existing+1new targets
- before/: eight original .rs.txt snapshots
- after/: nine proposed .rs.txt snapshots
- schema.json: unchanged design schema, including nine containment equations

Core work_profile.rs imports only std::cell::RefCell and std::time::Instant. It owns a fixed24-slot numeric TLS array, explicit phase indices, numeric mark, guarded clocks and snapshot take. Core reads no environment and formats or writes nothing. No existing production API forwards counts. Store adds only an inclusive callback guard through the temporary Core helper.

App service.rs alone checks DEADPAN_CORE_WORK_PROFILE=1 and begins on native user-lane Macro dispatch. App alone owns the24 constant name/kind pairs and best-effort stderr output. It takes the buffer once after service.publish returns and the publication and outer service guards drop. It discards the buffer unless app_refresh_succeeded count is exactly1. Refresh result branches/receipts and ordinary publication remain intact. Errors, rejections and cached paths that do not refresh emit nothing. Output I/O errors are ignored, so partial/missing output invalidates the trial without changing a saved result. Trace rows contain only constant labels and numeric values.

Exact26-line format:
    DEADPAN_CORE_WORK_PROFILE BEGIN count=1
    DEADPAN_CORE_WORK_PROFILE PHASE phase=<constant-name> kind=<constant-kind> elapsed_ns=<u128-decimal> count=<u64-decimal>
    [24 PHASE rows in the order below]
    DEADPAN_CORE_WORK_PROFILE END count=1

| Phase | Kind | Count | ns constraint |
|---|---|---:|---|
| app_service_total | inclusive_native_route | 1 | nonnegative |
| app_semantic_call | inclusive_cli_semantic_call | 1 | nonnegative |
| app_publication | disjoint_native_child | 1 | nonnegative |
| app_refresh_succeeded | zero_time_success_marker | 1 | zero |
| semantic_total | inclusive_core_semantic | 1 | nonnegative |
| semantic_entry_setup | disjoint_semantic_child | 1 | nonnegative |
| semantic_execute | inclusive_semantic_child | 1 | nonnegative |
| semantic_leaf_apply_call | inclusive_leaf_call_in_semantics | 1 | nonnegative |
| semantic_after_leaf_size | disjoint_semantic_execute_child | 1 | nonnegative |
| semantic_outer_finish | disjoint_semantic_child | 1 | nonnegative |
| replay_total | inclusive_core_replay | 1 | nonnegative |
| replay_host_visit | inclusive_store_visitor | 1 | nonnegative |
| replay_wire_size_checks | sum_disjoint_replay_counts | 4 | nonnegative |
| replay_leaf_apply_call | inclusive_leaf_call_in_replay | 1 | nonnegative |
| replay_aggregate_call | inclusive_final_net_call | 1 | nonnegative |
| leaf_apply_total | sum_two_disjoint_apply_calls | 2 | nonnegative |
| leaf_preflight | sum_disjoint_apply_children | 2 | nonnegative |
| leaf_reduce | sum_disjoint_apply_children | 2 | nonnegative |
| leaf_postprocess | sum_disjoint_apply_children | 2 | nonnegative |
| leaf_net_call | sum_two_disjoint_net_calls | 2 | nonnegative |
| net_total | sum_three_disjoint_net_calls | 3 | nonnegative |
| net_before_and_patch | sum_disjoint_net_children | 3 | nonnegative |
| net_validation | sum_disjoint_net_children | 3 | nonnegative |
| net_finish | sum_disjoint_net_children | 3 | nonnegative |

Boundaries and lifetime review:
- Inclusive guards are first locals and drop through normal RAII, retaining original local cleanup coverage.
- Sequential child guards explicitly drop after each unchanged successful expression; existing ? and return paths drop active guards naturally.
- Semantic entry setup ends immediately before execute; outer finish ends before SemanticPlan field moves.
- Semantic leaf apply retains its existing LeafEdit::request argument evaluation, then the existing size/charge expression runs unchanged.
- Replay retains all four original ordinary-path size/charge executions in their exact positions. The ordinary apply expression is bound to a local solely to explicitly drop its call guard; .1 selection and original argument order remain intact. Isolation branch is unchanged.
- Shared apply preflight includes the unchanged early-return decisions. Reduce, postprocess and net-call boundaries match the design, with the final identity check retained afterward.
- Net before/patch, previous/known/final-validation selection and finish preserve their existing expressions and order.
- Store visitor body, historical decodes/equalities, inverse checks, SQL and publication are unchanged.

Counts are contractual rather than inferred from output: semantics/replay1, shared apply2, net3 and replay wire4. Unexpected recursion or multiplicity invalidates the fixture. The union equations compare semantic_leaf_apply_call+replay_leaf_apply_call against leaf_apply_total and leaf_net_call+replay_aggregate_call against net_total; totals are not additive attribution. Remaining residuals include cleanup, control/bookkeeping and timer overhead.

Unresolved work belongs to Root: formatting/compilation, unchanged replay correctness, counts/containment validation, and rotated control/off/on qualification. No measured subphase costs, instrumentation overhead, latency gain, memory result or production optimization is claimed. No implementation ambiguity remains from source review.
