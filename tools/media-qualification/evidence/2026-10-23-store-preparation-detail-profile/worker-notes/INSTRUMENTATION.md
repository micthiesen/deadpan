# Temporary Store preparation detail overlay

Implemented only in /tmp/deadpan-store-preparation-detail-profile-2026-10-23/source.
Uncompiled and unformatted. No Cargo, formatter, Git, tests, app/runtime, or measurement job was run by this worker. Source review and hash/schema consistency checks only. Root owns compilation, formatting, checks and all trials. No measured costs or overhead claims.

The source basis matches all seven hashes in the design source-hashes.json. The full source tree hash comparison found exactly the four assigned existing files and new temporary module changed. source-hashes.json records before/after target identities; before-source-hashes.json records the entire baseline file map. Core, CLI and every other file remain byte-identical. Existing serializers, historical decodes, equality checks, inverse restore, work charges and refusal/SQL ordering are retained. The extracted final-validation expression evaluates the existing first Arc-clone argument before starting final validation.

Only native user-lane Macro dispatch calls begin_macro. Gate: DEADPAN_COMPOUND_PREPARATION_DETAIL_PROFILE=1.
The fixed-size TLS buffer contains 33 numeric slots. Store does not self-enable. Zero rows remain in output. No IDs, paths, content, error strings or dynamic labels enter traces.
Inclusive function guards are declared first and use RAII return cleanup; each disjoint child explicitly drops before the next begins on success and naturally drops on error. The app explicitly drops publication and outer service guards after publish before finishing. Finish takes the buffer once and discards it unless refresh-success count is exactly one. Refresh result branches and native receipt/publication behavior are preserved. Stderr writes ignore I/O errors and cannot turn a completed save into an I/O panic.

Exact 35-line profile format:

    DEADPAN_COMPOUND_PREPARATION_DETAIL_PROFILE BEGIN count=1
    DEADPAN_COMPOUND_PREPARATION_DETAIL_PROFILE PHASE phase=<constant-name> kind=<constant-kind> elapsed_ns=<nonnegative-u128-decimal> count=<nonnegative-u64-decimal>
    [33 PHASE rows total, in the exact order below]
    DEADPAN_COMPOUND_PREPARATION_DETAIL_PROFILE END count=1

One successful release first-pause contract:
| Phase | Kind | Count | ns constraint |
|---|---|---:|---|
| app_service_total | inclusive_total | 1 | nonnegative |
| app_perform_macro_total | nested_inclusive_total | 1 | nonnegative |
| app_semantic_planning | disjoint_app_child | 1 | nonnegative |
| app_prepared_commit_total | nested_inclusive_app_child | 1 | nonnegative |
| app_service_refresh | disjoint_app_child | 1 | nonnegative |
| app_publication | disjoint_service_child | 1 | nonnegative |
| store_prepared_commit_total | nested_inclusive_store_total | 1 | nonnegative |
| store_admission_immediate_guard | disjoint_store_child | 1 | nonnegative |
| store_prepare_command | disjoint_store_child | 1 | nonnegative |
| store_host_callback | disjoint_store_child | 1 | nonnegative |
| store_write_plan_or_bank | disjoint_store_child | 1 | nonnegative |
| store_durable_sql_commit | disjoint_store_child | 1 | nonnegative |
| store_cache_publication | disjoint_store_child | 1 | nonnegative |
| app_refresh_succeeded | success_marker | 1 | zero |
| store_compound_prepare_total | nested_compound_total | 1 | nonnegative |
| store_compound_preflight | disjoint_compound_child | 1 | nonnegative |
| store_compound_execute_total | nested_execute_total | 1 | nonnegative |
| store_execute_entry_size | disjoint_execute_child | 1 | nonnegative |
| store_core_replay_total | nested_replay_total | 1 | nonnegative |
| store_core_host_visit_total | nested_host_visit_total | 1 | nonnegative |
| store_visit_admission | disjoint_host_visit_child | 1 | nonnegative |
| store_visit_after_size | disjoint_host_visit_child | 1 | nonnegative |
| store_visit_accounting | disjoint_host_visit_child | 1 | nonnegative |
| store_execute_final_size_guard | disjoint_execute_child | 1 | nonnegative |
| store_prepare_register_bank | disjoint_compound_child | 1 | nonnegative |
| store_prepare_final_validation | disjoint_compound_child | 1 | nonnegative |
| store_command_plan_total | nested_command_plan_total | 1 | nonnegative |
| store_request_serialize_and_bound | disjoint_command_plan_child | 1 | nonnegative |
| store_request_historical_decode | disjoint_command_plan_child | 1 | nonnegative |
| store_edit_serialize_and_bound | disjoint_command_plan_child | 1 | nonnegative |
| store_edit_historical_decode | disjoint_command_plan_child | 1 | nonnegative |
| store_inverse_restore_check | disjoint_command_plan_child | 1 | nonnegative |
| store_debug_verification | debug_only_child | 0 | zero |

Containment equations are copied unchanged in phases.json. Replay minus host visitor is a residual including Core work, callback dispatch and timing overhead, not a separately measured Core phase. Parent totals must not be summed with descendants.

Unresolved: compile/format verification and runtime evidence remain exclusively with Root. No implementation ambiguity remains from source review. The intended one-Edit workload must independently validate counts, containment and unchanged correctness. Cached/stale Macro, refresh failure, different multiplicities, missing/partial output or extra report blocks invalidate a trial. Environment-off TLS checks and environment-on timing/output overhead remain unqualified.
