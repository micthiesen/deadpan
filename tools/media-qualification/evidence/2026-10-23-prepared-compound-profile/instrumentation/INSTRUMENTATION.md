# Optional future prepared-Compound diagnostic draft

Conditional diagnostic only. Current candidate qualification remains authoritative and active. No gain, regression, phase dominance, or shipping claim follows from this unexecuted draft. The parent may choose not to use it after actual paired results.

Files: `instrumentation.patch`, complete proposed `*.rs.txt` contents, `source-hashes.json`, and `check_profile.py`. Source was read fresh from the transaction-callback implementation on 2026-10-23. The manifest pins every modified original plus the inspected, unmodified CLI route. This differs from the earlier first-pause instrumentation; do not apply both patches. The earlier profile/evidence directory and repository were untouched. No Cargo, rustc, Git, GUI or application runtime was run for this draft.

Verify original SHA-256 values and absence of the new helper before applying in a separate source copy and separate target directory. Retain the exact applied source identity and executed binary hash. `draft.py` is a scratch proposal generator; rerunning it would update the source basis, so do not regenerate an applied trial's evidence.

## Gate and buffering

Only `DEADPAN_PREPARED_COMPOUND_PROFILE=1` plus a native user-lane `ProjectRequest::Macro` begins profiling. A fixed-size, thread-local buffer in a temporary public Store helper receives app and Store timings. No Store method self-enables from the environment, so legacy calls outside the app gate and calls on other threads record no times or output. No request fields or input timestamps change.

The app calls `finish_after_shared_publication` only after `service.publish()` returns and the app's outer span drops. Publication includes `shared.store_update` and `shared.wake`. Store methods, the host callback, and refresh print nothing. The final block contains only constant phase/kind labels, counts and elapsed nanoseconds. Output failures are ignored by the diagnostic writer and yield a missing/partial report for the parser to reject; they cannot panic the service after a durable save.

## Timing tree

All clocks use `std::time::Instant`. An inclusive parent must never be added to its descendants:

- `app_service_total` includes `app_perform_macro_total`, later `app_publication`, and uninstrumented dispatch overhead.
- `app_perform_macro_total` includes disjoint `app_semantic_planning`, `app_prepared_commit_total`, `app_service_refresh`, and remaining native work.
- `app_prepared_commit_total` includes `store_prepared_commit_total`, plus writer acquisition, CLI routing/error mapping and wrapper overhead.
- `store_prepared_commit_total` includes six mutually disjoint Store children and uninstrumented local work/cleanup.

| Store child | Boundary |
| --- | --- |
| `store_admission_immediate_guard` | writer/Compound admission, documents reference, Immediate transaction acquisition, optional transcript guard check |
| `store_prepare_command` | actual `prepare_command` expression through its existing `?` result |
| `store_host_callback` | original host callback with the cloned authoritative bank through its mapped result |
| `store_write_plan_or_bank` | unchanged register-only or authored-plan write branch, including branch decision |
| `store_durable_sql_commit` | unchanged `transaction.commit()` through its mapped result |
| `store_cache_publication` | unchanged committed-result mapping and `documents.insert(next)` |

The final bank clone between prepare and callback remains in the Store residual. Store outcome construction, cleanup, and timer bookkeeping also remain there. App planning measures only the actual `plan_program_with_transcript_guard` expression; pre/final RenderPlans, scope/receipt work, authority checks and post-commit handling remain in the app residual. There is no Source/Core instrumentation and no added CLI instrumentation.

`app_publication` starts after dispatch returns and includes completion/busy updates, normal splice/slip/trim reconciliation and shared publication. `app_service_refresh` covers only the original `self.refresh()` call. `app_refresh_succeeded` is a zero-time, count-only marker, recorded only for an `Ok` refresh; it rejects a failed refresh even when all timer spans executed.

## Preservation and failure paths

All original side-effecting calls retain their arguments, relative order and `?` propagation. Authored authority, validation, exact inverses, callback privacy, write operations, durable commit, cache insertion, receipt and publication paths remain intact. Only dispatch and refresh results gain local bindings to bracket their unchanged control flow. No sleeps, injected work, dependencies or check changes are proposed.

Each function's inclusive guard is declared before original locals and drops last, so successful cleanup and error rollback remain within its total. Each child is explicitly dropped before the next child begins. On an early error, the active child drops before preceding original locals and transaction rollback; original resource drop order is unchanged. Partial failures have missing downstream counts. A refresh failure lacks the success marker. Panic/abort can omit the final block. These invalidate the trial rather than becoming performance samples.

## Future trial acceptance

Run only if further phase evidence is useful. Keep the original scenario, fresh fixture/package, release profile, load qualification and all correctness checks unchanged. Run one native Macro pause and capture stderr. Invoke `python check_profile.py <captured-stderr.log>` from this directory.

The parser requires exactly one BEGIN/END block; one execution of every named app/Store span; one successful refresh marker; zero marker timing; and nonnegative containment residuals. Exactly one `store_prepared_commit_total` establishes one observed Store helper entry under the gate. A cached Macro, extra/nested Store commit, selection-only operation, missing phase, partial failure or duplicate report is rejected. Parser success proves report shape only; the unchanged replay must still pass its semantic, durability and load checks.

Queue wait is explicitly unmeasured. UI input/submission, service waiting, UI receipt consumption, subsequent pictures/GPU and scanout are outside these spans. Shared publication proves neither UI consumption nor displayed acceptance. Timer/TLS overhead is uncalibrated; buffered logging can overlap downstream UI work. Keep diagnostic timing separate from uninstrumented end-to-end qualification and do not infer a gain from this draft.
