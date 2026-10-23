# Scratch-only first UI pause service profile

This is an instrumentation proposal for a separate source copy and separate Cargo target directory. Nothing in the live repository was edited. No Cargo, rustc, Git, GUI, or worktree operation was run for this draft. Parent owns compilation, the real controlled replay, source/binary provenance, and all existing correctness checks.

The supplied comparison is qualified direct InsertTime store + refresh median 93.633 ms; the new quiet release diagnostic sample reports input to commit 389.723625 ms and input to accepted offscreen composition 392.35325 ms. These totals have different boundaries. This draft makes no phase-dominance claim.

## Files and application

- `instrumentation.patch` is the unified diff against original SHA-256 values in `source-hashes.json`.
- The three `*.rs.txt` files are complete proposed contents, including new `service/macro_profile.rs`.
- `draft.py` regenerates the proposal from current original files, requiring every exact replacement anchor once. Running it changes the proposal's source basis; retain the manifest used for the trial.
- Verify both original hashes before applying to the isolated copy. The helper must be absent initially. Copying complete `.rs.txt` contents avoids patch-tool ambiguity.
- Only `DEADPAN_UI_PAUSE_PROFILE=1` enables the report. The gate applies to every user-lane `ProjectRequest::Macro` dispatch. Run exactly one fresh pause Macro operation in the trial. Rejected, cached, save-only, or selection-only operations will fail the successful-pause multiplicity expectation.
- No request structures, UI checks, store/CLI implementations, authority resolution, inverse construction, validation, durability calls, or project contents change.

`macro_profile::begin` creates fixed-size thread-local measurement storage immediately before dispatch. Timers use `std::time::Instant`. RAII drops capture elapsed nanoseconds and counts, including attempted phases on an early error. The helper prints one buffered BEGIN/13-PHASE/END block only after the service route and shared workspace publication finish. It prints constant phase/kind names and timing/count numbers. It prints no error text, paths, project data, content data, identifiers, or request serials. A rejected/replayed operation still has a report; the existing replay result and strict phase counts must qualify its interpretation.

## Boundaries and nesting

`service_total` is inclusive from immediately before `dispatch_request` through `service.publish()` return, including `shared.store_update` and `shared.wake`. It contains inclusive `perform_macro_total`, then disjoint `post_dispatch_publication`. Its residual includes other dispatch overhead and `macro_command` receipt/update publication.

`perform_macro_total` is a nested inclusive total from entry to `perform_macro` until it returns and its locals drop. Never add this total to its children. These direct child spans are sequential and mutually disjoint:

| Phase | Exact work |
| --- | --- |
| `context_validation` | `observe_semantic`, operation ID lookup, `check_macro_context`, including snapshot/register/runtime validation |
| `pre_edit_plan_scope` | pre-edit `RenderPlan::compile`, scope resolution/owner check, `scoped_macro_target`, one-instruction `SemanticProgram::new` |
| `semantic_program_plan` | actual `plan_program_with_transcript_guard` expression through mapped result, including existing argument evaluation |
| `transcript_guard` | existing displayed/consumed transcript guard comparison |
| `final_plan_scope_receipt` | final `RenderPlan::compile`, historical scope derivation, scoped receipt and initial native receipt construction |
| `preview_runtime_bank` | `plan.preview` and `prepare_runtime_bank`, including runtime-bank access |
| `commit_compound` | actual `plan.commit_compound(self.writer()?)` expression through durable return, including writer acquisition and existing error mapping |
| `post_commit_receipt` | preparation notices, marks/semantic proof, committed edit/receipt/register/runtime publication, saved receipt, existing test-only refresh fault setup |
| `service_refresh` | single unchanged `self.refresh()` call through return; refreshes registers and snapshot/workspace and reconciles splice/slip/trim |
| `post_refresh_receipt` | unchanged refresh result handling and final saved receipt publication |

Operation/repeat preparation before the pre-edit plan, the request-presence branch, timer bookkeeping, and some local destruction remain in the inclusive perform residual. `post_dispatch_publication` starts after `dispatch_request` returns and includes unchanged completion/busy updates, splice/slip/trim reconciliation, and `publish()`.

The two original control-flow changes extract unchanged `dispatch_request` and `refresh` results into locals solely to time boundaries. Original side-effecting calls retain relative order and arguments; `?` and refresh-success/failure handling retain behavior.

## One controlled trial and checks

Retain the original baseline fixture, fresh package, environment, release profile, replay correctness/semantic checks, and load qualification. Do not change tolerances or accept a failing check to obtain a timing.

Capture stderr from the isolated executable with the gate enabled. Check source hashes and executed binary hash alongside isolated source identity. Then run:

```sh
python /tmp/deadpan-ui-pause-profile-2026-10-23/check_profile.py <captured-stderr.log>
```

The parser requires exactly one matched BEGIN/END, one line for each constant phase in order, execution count one for every phase, and nonnegative inclusive-minus-disjoint residuals. Passing only verifies instrumentation shape. The unchanged replay's correctness and load checks remain required. Missing phases, count zero, extra reports, replay failures, or source mismatches invalidate the intended fresh successful pause sample.

## Limits

- Queue admission/wait is not measured. The route starts only after service receive. No request timestamp or request-struct change was added.
- UI submission, wait before service dispatch, UI polling/receipt processing, frame preparation, GPU acceptance, and scanout are outside these spans.
- `service_refresh` covers the entire app refresh without attributing validated snapshots, plan compilation, and workspace assembly. Planning and commit likewise remain coarse app boundaries. Add CLI/store spans only after a controlled sample identifies a phase worth subdividing.
- Service workspace replacement and subsequent shared `ProjectUpdate` publication are measured separately. Shared publication does not prove UI consumption.
- Timers add small uncalibrated TLS/clock overhead. Buffered stderr output occurs after the measured service span but can run concurrently with downstream UI work. The instrumented end-to-end sample is diagnostic evidence, not a replacement for uninstrumented latency qualification.
- Early errors can record partial phase timings. Counts do not establish success; the unchanged scenario result does.
