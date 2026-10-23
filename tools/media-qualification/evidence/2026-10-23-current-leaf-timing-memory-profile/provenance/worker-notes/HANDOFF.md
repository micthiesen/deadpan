# Current leaf timing overlay: source-only handoff

Fresh source came from git archive of commit3a32c923358c0cb7ca53f821bfb6db3e01ebb466 into this scratch root. No previous instrumented source or Cargo cache was reused. Before edits, all2051 Rust identities and the six inputs exactly matched the qualified release-native-source-identities.json/release-native-lock.json freeze. Qualified control appSHAa9ce60a4d227e8c8107b06602915e6323e0cfabbf12610b67a85eda77bdd4a0f was independently read and matched. The archive SHA is a7fa4c4c150db04776e21ca67eab2636afd5b047b02eee8b3969197a93a68cae.

Exactly five existing files changed:
- crates/deadpan-app/src/project/service.rs
- crates/deadpan-app/src/project/service/macros.rs
- crates/deadpan-core/src/lib.rs
- crates/deadpan-core/src/semantic/planner/pause.rs
- crates/deadpan-core/src/compound.rs

Exactly one new temporary module:
- crates/deadpan-core/src/leaf_profile.rs

All2046 nonowned baseline Rust files and all34580 nonowned baseline files remain byte-identical. The production refresh implementation and its new semantic_refresh tests are included unchanged. The new source map has2052 Rust files. No Cargo manifest/input/runtime memory adapter/source API plumbing changes. Main checkout was not edited.

Instrumentation:
- App alone reads DEADPAN_CURRENT_LEAF_PROFILE=1, and only native user-lane Macro dispatch activates the four-slot numeric TLS buffer.
- app_service_total starts immediately before dispatch and ends after reconciliation and shared publish, before buffered output. The original dispatch expression remains intact.
- semantic_leaf_apply_call brackets the unchanged apply_with_result call in commit_leaf, including its LeafEdit::request argument evaluation and ? result. Charge, document replacement and refresh remain afterward.
- replay_leaf_apply_call brackets only the ordinary non-isolation replay apply_with_result(...)? .1 expression. A local result permits explicit guard drop. Isolation, pre/post size charges, identity ledger and visitor are unchanged.
- app_refresh_succeeded is a zero-time marker after the same service refresh result succeeds. Both existing refresh result branches remain unchanged.
- Core imports only RefCell and Instant; no environment, I/O, content, Store or CLI dependency. elapsed_ns/count use checked_add. Overflow marks the snapshot invalid; app consumes/discards it after publication without changing the saved result.
- Explicit guard drops on success, natural RAII on original ?/error returns. App takes the buffer once after publish and the outer guard drop, requiring exactly one refresh-success marker and no numeric overflow. Refusals/cached paths/refresh failures without that marker emit no block.
- Gate off: no buffer activation, clock acquisition or output. Disabled guard TLS/branch overhead remains for control qualification.
- Stderr output is best effort on the service thread after the timed work, never the UI thread. I/O errors cannot turn a completed save into an I/O panic; missing/partial reports invalidate evidence.

Exact six-line output:
    DEADPAN_CURRENT_LEAF_PROFILE BEGIN count=1
    DEADPAN_CURRENT_LEAF_PROFILE PHASE phase=app_service_total kind=inclusive_native_route elapsed_ns=<u128> count=1
    DEADPAN_CURRENT_LEAF_PROFILE PHASE phase=semantic_leaf_apply_call kind=disjoint_leaf_call_in_semantics elapsed_ns=<u128> count=1
    DEADPAN_CURRENT_LEAF_PROFILE PHASE phase=replay_leaf_apply_call kind=disjoint_leaf_call_in_replay elapsed_ns=<u128> count=1
    DEADPAN_CURRENT_LEAF_PROFILE PHASE phase=app_refresh_succeeded kind=zero_time_success_marker elapsed_ns=0 count=1
    DEADPAN_CURRENT_LEAF_PROFILE END count=1

Only containment is outer >= sum of the two disjoint leaf calls. Residual includes other service work, cleanup and overhead. No overlapping subtraction or per-algorithm attribution. Counts are fixed by the one ordinary fresh first-pause workload, not coerced to fit output.

Artifacts:
- source-basis.json / original-source-identities.json / input-identities.json: qualified baseline.
- instrumented-source-identities.json: proposed2052-Rust map.
- worker-notes/before/: five original .rs.txt snapshots.
- worker-notes/after/: six proposed .rs.txt snapshots.
- worker-notes/instrumentation.patch: complete preformat patch.
- worker-notes/source-hashes.json: all six before/proposed identities and full audit.
- worker-notes/source-review.json: source-only review receipt.
- schema.json: implemented constant rows/contract.
- check_profile.py, test_profile_parser.py and memory_observer.rs remain separate, previously drafted scratch runners. They were not executed or changed by this source-overlay task. DRAFT_REVIEW.md retains observer limitations/failure paths.

Status: uncompiled, unformatted, untested and unexecuted. This worker ran no Cargo, formatter, tests, app, observer, parser, benchmark or profiling command. The only Git operation was a read-only archive of the supplied commit to scratch. Root owns formatting, independent review, builds, both measurement lanes, runner execution, evidence and adoption.

Owned-memory evidence remains a sampled lower bound for the whole main-app replay lifetime; it cannot establish a memory ceiling, exact peak or per-leaf peak. No cost, gain, overhead or memory result is claimed. No source implementation ambiguity remains from review; runtime count/containment and failure behavior still require Root's execution.
