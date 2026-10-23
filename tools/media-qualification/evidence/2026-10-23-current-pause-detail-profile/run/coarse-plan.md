# Current first-pause route: proposed private diagnostic

Status: plan only. No source overlay, parser, build, runtime or measurement was implemented. Main is frozen under Root's ongoing production qualification. The approximately 195 ms route is the question being investigated, not an attributed cost. Historical Store33/Core24 numbers do not establish any current subphase cost.

## Baseline and experiment boundary

Wait for terminal production qualification and its final source/input/executed-binary identities. Then create a fresh private source copy matching Root's complete 2,053-Rust-file map and six pinned inputs. Re-read these anchors immediately before patches. Use a fresh distinct target directory and copy/hash the actual executable; reuse neither the old instrumented source nor its Cargo output.

Reuse the qualified native first-pause fixture/driver, including its existing command receipts, exact document/history assertions, first-command order and teardown. The timed interval starts on the project service thread immediately before dispatching the selected native user-lane Macro request. It ends after the same iteration's invalidation/reconciliation and service.publish() have completed, including shared.store_update(update) and the wake call. This measures worker dispatch through publication, not UI queue delay, input routing before dispatch, painted-frame latency, GPU execution, decoder work after wake or physical key-to-display latency.

Activate only the selected native Macro pause lane. Remote macros, headless calls, annotation saves, background refreshes and unrelated operations cannot activate it. For multiple fixture commands emit one block per selected command and compare the first block by the driver's existing order; do not infer command identity from timestamps. No IDs, document content or paths appear in traces.

## Current route and authority anchors

- App project/service.rs user-request loop: dispatch_request(request), busy-state handling, invalidate_changed_splice/reconcile_slip/reconcile_trim, publish. dispatch routes ProjectRequest::Macro to macro_command.
- App project/service/macros.rs: macro_command duplicate-receipt handling; perform_macro; check_macro_context; initial RenderPlan::compile and SequenceScope resolution; plan_program_with_transcript_guard; final RenderPlan::compile, scope/scoped receipt and runtime-bank checks; HostSemanticPlan::commit_compound_prepared; post-commit continuation; refresh; final receipt.
- CLI macros.rs: HostSemanticPlan::commit_compound_prepared borrows plan.one_leaf_replay and the existing transcript guard into Store. No CLI edit is needed in the first overlay: the App call and Store entry delimit this wrapper.
- Store compound.rs: commit_compound_prepared_inner retains writer/type checks, Immediate transaction and transcript guard, prepare_command_with_one_leaf, prepared register-bank clone and host callback, write_command_plan, transaction.commit, documents.insert and returned outcome.
- Store lib.rs: read_command_snapshot; prepare_current_command_with_one_leaf clones the base request and current validated handle; prepare_base_command routes Compound; command_plan encodes and admits request/edit history and checks inverse restoration; write_command_plan performs generation/source authority, revision/history/step/bank writes, state/redo updates and receipt extension.
- Store compound.rs prepare/execute: current bank/input/allocation authority, Core replay, every leaf visitor admission, exact staged sizes, final result size and ValidatedDocument::new. A pure reused leaf result does not replace these checks.
- App service.rs refresh calls refresh_registers and snapshot. snapshot obtains validated head, compiles RenderPlan inside the validation scope, clones ProjectDocument, refreshes/reuses source metadata and original records, history/profile/analysis/color state, and builds Workspace. Its document clone is distinct from Core capability/replay clones.

## First overlay: 13 disjoint categories, not a broad profiler

Implement only four existing files plus one temporary helper:
1. crates/deadpan-app/src/project/service.rs
2. crates/deadpan-app/src/project/service/macros.rs
3. crates/deadpan-store/src/compound.rs
4. crates/deadpan-core/src/lib.rs, solely to expose the temporary numeric helper
5. NEW crates/deadpan-core/src/pause_route_profile.rs

Core already sits below App/Store, so the helper needs no new dependency. It is diagnostic-only and removed with the private overlay. No Core editing algorithm, CLI wrapper, Store serializer or runtime memory adapter changes in this first pass.

Use one fixed-size thread-local scalar slab and a bounded numeric phase stack. A category switch reads the monotonic clock once, charges the preceding active category and assigns that same timestamp to the next category. A child pauses its parent's accounting; return resumes it without incrementing its entry count. Therefore all emitted category durations are exclusive and pairwise disjoint. Do not emit inclusive parent rows that could be accidentally added to children. The sum of these categories must equal the slab's independently retained start-to-finish interval using the same boundary timestamps. Record counts on explicit entry, not on resumption.

| Fixed category | Exact boundary on the successful ordinary one-leaf route | Entry count |
| --- | --- | --- |
| native_admission | Outer dispatch start through check_macro_context, initial picture plan/scope/capture and invocation construction, ending immediately before host planning | 1 |
| semantic_plan | The existing plan_program_with_transcript_guard call, including Core work, sizes, providers and capability creation | 1 |
| native_continuation | After planning through transcript comparison, final picture plan/scope, scoped receipt, Receipt construction and runtime lookup before the prepared commit; wrapper gaps resume here | 1 |
| store_entry_misc | Entry of commit_compound_prepared_inner through exit, excluding the five Store child categories below; includes writer/type/Immediate/guard and remaining dispatch/result bookkeeping | 1 |
| store_prepare | The existing prepare_command_with_one_leaf call, including snapshot, request clone, bank/allocation checks, replay/visitor, result validation, boundary preparation and command_plan | 1 |
| host_runtime_bank | Existing prepared-bank clone and prepare callback; callback still calls prepare_runtime_bank with unchanged arguments | 1 |
| store_write | Existing write_command_plan call with all retained authority, history encoding already completed by prepare, and revision/history/step/bank/receipt work | 1 |
| sqlite_commit | Only the existing transaction.commit expression and its error mapping | 1 |
| store_cache_publication | Existing committed.map/documents.insert and returned CompoundCommitOutcome assembly after commit | 1 |
| native_postcommit | Successful prepared-call return through notices, semantic proof, committed continuation, saved receipt and runtime-register publication, ending before refresh | 1 |
| workspace_refresh | The existing self.refresh call, including register refresh, snapshot and refresh's final invalidation/reconciliation | 1 |
| native_finish | After refresh through message/error/receipt/macros update and dispatch return, including natural plan/capability/local drops | 1 |
| native_publication | User-loop invalidation/reconciliation and publish through store_update and wake return | 1 |

The Store parent row is exclusive miscellaneous time, not the inclusive transaction duration. All five Store children are excluded from it. No result should call it SQLite time. Likewise semantic_plan and workspace_refresh are deliberately coarse initial buckets.

Proposed exact opt-in is DEADPAN_PAUSE_ROUTE_PROFILE=1; proposed fixed output prefix is DEADPAN_PAUSE_ROUTE_PROFILE_V1. A 15-line block is sufficient: BEGIN count=1; 13 ordered PHASE rows with fixed name, elapsed_ns and count; END with count=1, outer elapsed_ns and committed/refreshed/published markers each 1. A successful target must have exactly these counts, and sum(PHASE.elapsed_ns)==END.elapsed_ns. There are no per-node, per-sample, per-SQL-row, per-serializer-write or dynamic-label events.

## Conditional refinement after the first result

Do not install all fine spans speculatively. If a coarse bucket is material and stable across repetitions, replace only that bucket with a direct disjoint partition in a second private overlay. Fresh rows are measured directly in that run, never obtained by subtracting old measurements or by subtracting overlapping parents. A remaining coarse category is explicitly miscellaneous and not assigned a narrower algorithm name.

### If semantic_plan / store_prepare dominate

Use these current anchors; additional existing files are Core semantic/planner.rs, planner/content.rs, planner/pause.rs, one_leaf_replay.rs, compound.rs and Store lib.rs. Core's helper remains numeric only; App alone controls activation/output.

- Semantic document wire+charge: four ordinary one-leaf sites, namely initial document_bytes in plan_semantic_with_provider; charge_step current document in content.rs; commit_leaf's existing admit_next callback; planner final outer-revision document charge. Aggregate them as one category with occurrence count 4, or keep four fixed count-1 rows only if location matters. These contain the original wire::size and charge, not an extra serializer call. Do not instrument generic Serialize::serialize/write callbacks. Any wire work inside the pure leaf remains in that leaf category.
- Pure semantic leaf application: exactly the original apply_with_result expression in apply_semantic_leaf, count 1. The post-leaf admission callback is outside this category. This does not claim which internal validator dominates.
- Capability snapshot construction: the existing successful .then closure in apply_semantic_leaf, count 1; contains three existing value clones (entry/request/next), not three calls of an invented phase. No clone is removed or forced earlier.
- Core replay document wire+charge: four sites in replay_compound_inner (initial, pre-leaf, post-leaf, final outer revision), count 4. Keep original ordering around ledger, reuse, visitor and final net.
- Exact reuse matching: OneLeafReplay::reuse's original eligibility/full document Eq/full request Eq condition through its success/miss decision, count 1 on the eligible target. The label must include eligibility, not pretend it times Eq alone. Natural RAII closes a miss before its existing fallback.
- Reuse clone/exit: after that condition through existing EXACT_EDIT entry reset, leaf revision check, next.clone, exit take_exact_edit and sealed_preserved, count 1. If clone alone later matters, only bracket that existing clone; do not move it or elide the TLS/checks.
- Core replay aggregate: exactly net_transaction, count 1. Core replay miscellaneous is a paused/resumed category around the remaining body, not replay-minus-visitor arithmetic.
- Store exact size traversals in execute: current.compact_json_size, visit.after.compact_json_size, outcome.document.compact_json_size, each count 1 on the no-capture ordinary target. Checkpoint serialization is zero for this shape and must remain an explicit zero count if that branch is represented.
- Store visitor: time the original visitor body excluding its staged-size child. It still performs validate_transition and all media/source/analysis authority. Call it visitor authority/bookkeeping, not a Core phase.
- History encoding/readback: in command_plan, request serialization/size/decode/Eq followed by edit serialization/size/decode/Eq, count 1. This includes real codec and equality work; no fake precomputed size substitutes for bytes.
- History inverse: original edit.inverse.restores check, count 1, after the codec region. Debug-only forward/full-validation checks are absent from the required release measurement and must not be silently timed as production work.
- Store remaining preparation includes ValidatedDocument::new, bank preparation, request/current-handle clones, current authority and boundary_transition::prepare. Do not attribute it to JSON or clone cost. If that remainder matters, isolate those existing calls in a later small refinement. Preserve argument evaluation/drop order; never pull a constructor out of the command_plan argument list just to make it easy to time.

Do not conflate Core wire counting with Store compact_json_size or history JSON encoding. They traverse different values at distinct authority boundaries. Four+four+three is the expected number of the named document-size sites on this target, not a claim about every internal serialization performed by the route.

### If store_write dominates

Refine Store lib.rs write_command_plan into disjoint preflight (request decode/generation/source authority), revision persistence, and remaining history/steps/bank/state/generation/receipt writes; keep sqlite_commit separate. Optionally inspect revision_storage.rs::insert:
- A normal StoredPatch::Edit may write the elided literal rather than a full document.
- The fallback after.to_compact_json occurs only if its existing depth/bound decision requires a keyframe. Time that existing branch and count materialized versus elided outcomes (their counts sum to 1); never force either branch.
- Revision persistence otherwise includes a current SQL parent-depth/bound read and INSERT. Remaining history writes include compound::write_steps, history/state/redo rows and audit::extend. Do not describe the broad region as SQL-only or fsync-only, and do not add a timer per row.
This conditional refinement adds revision_storage.rs only if the branch is actually relevant.

### If workspace_refresh dominates

Refine only App service.rs snapshot/refresh:
- refresh_registers as its own original call, count 1;
- snapshot_validated/head acquisition, count 1;
- validated.scope(RenderPlan::compile), count 1;
- ProjectDocument::clone(&validated), count 1;
- existing source loop and original-record lookups/reuse, count 1;
- remaining history/profile/analysis/color/Workspace assembly and refresh reconciliation as direct miscellaneous.
No source decoder/preview worker timing or UI paint is inferred from snapshot. Retained metadata equality and clones are not automatically IO. Preserve previous-workspace reuse, conditional analysis refresh and error ordering.

## Activation, output and refusal contract

App reads a distinct opt-in env gate once for the owned worker; gate-off never activates Core TLS or reads phase clocks. No environment, filesystem, stderr, Store or GUI dependency enters Core. Inactive calls are fixed numeric no-ops. No clock read, allocation or emitted text occurs per tree node or serializer byte.

Use checked scalar durations/counts, fixed phase labels, bounded stack depth and nonpanicking try-borrow behavior. Overflow/reentrancy/stack mismatch invalidates the diagnostic block only, never the edit. Hold no TLS borrow over a profiled call or callback. No global cross-thread accumulator.

Success marker is set only for an authored native receipt with successful refresh; the current saved-but-refresh-failed branch remains exactly as implemented and merely discards the diagnostic. Errors, refusals, selection-only commands, duplicate receipts and unexpected phase counts discard the buffer. Do not move/check authoritatively before their current error boundary.

Finish/drop the outer timing state only after successful native publication. Then clear/deactivate TLS and emit a single bounded best-effort stderr block off the UI thread. IO/formatting errors cannot panic or turn a saved command into failure. Formatting/logging is outside the timed interval. The helper must not retain documents, requests, IDs, paths or closures.

## Qualification and interpretation

Root should first audit the small overlay against the terminal source map, compile/fmt and run focused producer/first-error tests. A synthetic parser must reject extra/missing/duplicate/out-of-order rows, malformed decimals, overflow, wrong counts/markers and failed conservation. Existing receipt/document/history checks remain the work oracle, not trace timing.

Use quiet, rotated uninstrumented-qualified / overlay-gate-off / overlay-gate-on trials on fresh equivalent packages, with at least three rotations and the same native driver. Record source/input/executed-app hashes, compiler at build entry, hardware, driver/command order and terminal receipts. Confirm disabled-path equality and enabled-path state parity before interpreting timings. Keep owned-memory observation in a separate matched lane using existing adapters; no memory polling or process inspection in timing trials.

Clock/helper cost is unmeasured until those controls. Exclusive accounting assigns helper overhead to adjacent categories; it does not remove it. Natural drops, scheduler stalls, allocator behavior and SQLite/durability variability remain in their observed regions. Treat an apparent small difference within off/on spread as unresolved. A stable coarse/fine result identifies where to study next; it does not authorize a cache, serializer bypass, weaker validation, new API or authority shortcut.
