# Mixed Hold duration review and evidence notes

## Runtime status at this checkpoint

- The first focused executable run selected 15 tests: 13 passed, 2 failed, 4,041 skipped. Core behavior, both native admission cases, and the accepted-generation preparation case passed.
- The CLI real-media failure was a fixture-oracle mismatch, not a duration implementation failure. It expected 5/4 cloned nodes, but the real inserted body contains two speech Partition Retime wrappers around Source leaves. The retained tree has six nodes in the inner body; an outer-play isolation also clones the inner Repeat, for 7/6. The revised test now asserts the typed shape before asserting 7/6.
- The subsequent two-test probe passed the corrected real-media case in 21.268s. Its paired store scope test still failed during request registration with the message scoped target names a retired Repeat play; it did not reach the duration edit or history assertions.
- The original store fixture constructed its Repeat orders with outer-order/inner-order, then inserted the nested subtree. prepare_subtree rewrites every inserted Repeat order to the Insert command allocation (mixed-fixture), so the original test target carried retired IDs. The fixture was changed to read live IDs from the resulting document. That later source change postdates the retained failing run; the retired-play rejection is not yet proven fixed and needs a fresh focused run before the store lifecycle test can be claimed.
- The source/test compile failures in check-first-missing-export.log, focused-first-proof-api.log, focused-host-first-borrow.log, and focused-second-artifact-api.log are integration/setup failures (E0425, E0599, E0505, E0609). They ran no behavioral tests and are not old-code red witnesses. Keep their logs, label them accurately, and do not count them as passing/failing behavior.
- The valid old-code red witnesses are the two new native admission tests failing only at the unchanged State::hold_parameter positive assertion, after their preconditions and raw positive control pass. Baseline executable identity and test-selection evidence are in baseline.json, baseline-list.json, and baseline.log. The earlier --lib invocation that selected zero tests is setup failure only.

## Evidence collection checklist

1. Preserve the exact base/source revision and dirty-path inventory; exclude fuzz/Cargo.lock and .claude/. Record exact Rust/Cargo, FFmpeg prefix, hardware/OS, fixture hashes, build environment, and executed binary hashes.
2. Retain the valid two-test old-code baseline: test-list metadata, exact command, full log and exit status, test executable SHA-256, source snapshot, and a short note that both failed at the expected unchanged positive assertion. Keep zero-test selection and every compiler/setup error as separate nonbehavioral records.
3. Retain the first focused 13/15 run and corrected-media 2-test probe, including their exact source/binary identities and raw logs. For the store failure, retain the target/Repeat allocation fixture details and only call it a fixture failure if the current-source rerun passes after the ID correction.
4. Once repaired, run and retain the full focused suite. Then retain the broader package tests, UI tests, format/xtask/strict-Clippy checks, command-reference check and release app build. Record exact counts, durations, filters, exit statuses and commands.
5. Retain final native visual and quiet-performance replay results, full reports/logs, Kestrel routing/source identity, inspected named captures and any retry/latency metrics. The quiet performance report must come from the final executed binary/source pair.
6. Retain the real-media semantic preview/export run and the separate complete encoded-output run with their reports, source and binary identities, fixture hashes, private package/output hashes, picture/audio coverage and any SNR/offset measurements. Do not project earlier shared-duration measurements onto mixed scope.
7. Refresh the complete tracked-source manifest after the last code change; preserve source snapshots by content hash and the exact binaries actually executed. Collect final reports, logs, reviews, environment, scripts and chosen captures; exclude raw executables and temporary project/media packages.
8. Run the collector only after final source identities and all results are ready. Rehash the completed inventory and verify every listed file. Update the qualification, Requirements, handoff and progress record from those retained facts, then publish the progress artifact from the pushed progress JSON and verify fetched JSON equals it and the mobile render succeeds.

## Claims still requiring final results

- Replace the qualification WIP/pending language only after the corrected full focused suite, store lifecycle/history test, full native replay, and encoded output run complete.
- Report the real-media 7/6 fixture shape and 21.268s targeted probe as a passing targeted correction only. Do not call the mixed media/export suite complete until its full final-source run passes and its output checks are retained.
- The store request's retired-play error remains unresolved in the latest retained runtime log; current-source changes after that log need a new binary and rerun. Do not claim request mapping, undo/redo, or preparation lifecycle integration until that succeeds.
- Keep the compiler-failure groups separate from behavioral test results. They do not establish regression or a valid baseline.
- Keep all requirements and gates open unless their exact full-scope criteria are met. Keep hardware, keyboard/IME, VoiceOver speech and listening owner-only items explicit.

## Later checkpoint

Fresh full focused run passes15/15 in23.217s, including the live-ID store correction. Broad normal run passes3431/3431 in142.694s with3 slow markers and1 captured-output leak marker; UI197/197 in15.090s with2 such markers. Strict lint then found the inline mixed authority enlarged ProjectRequest::Macro to688bytes versus Render424. Only that new optional context is boxed. Independent follow-up review reports no guard, dispatch, proof mapping or receipt semantic change. Final affected tests/lint and quiet leak followup remain pending at this checkpoint.

## Visual replay follow-up review

- The retained report has one failed scoped-play check at 885 checks. Its command field is `silence`; the response is `Unknown command: silence`, so it never tests the duration-only recording guard. The source has that spelling in `crates/deadpan-app/src/preview/harness/scoped_plays/mixed_hold_duration.rs` in the action-refusal loop (line 220 in the inspected source). Root's planned `hold-silence` correction matches the canonical command spelling used by the qualified shared-duration replay. Keep all guard assertions unchanged; the failure is a harness input error, not a product behavior witness.
- The other two refusal inputs, `gain -6` and `room-tone`, match the established shared-duration replay. Other mixed replay command spellings inspected (`scope play`, `hold-duration`, `source`, `sequence`) are also used by existing harness scenarios.
- Wait conditions mirror the qualified shared-duration lifecycle: the delayed navigation scenario waits for service idle before capturing the applied target, then delays UI feedback, navigates away/back, restores feedback, and waits for the exact revision and macro settlement. The general `settled` helper waits for service idle plus no pending macro before checking UI state. No wait/history mismatch found in static comparison.
- The repeated undo/redo sequence is internally consistent with each saved document checkpoint: undo latest 3f to longer 4f, redo to latest 3f, reopen latest, then undo through longer, first, and baseline. Dormant flow likewise captures post-edit state, undo/redo verifies it, and reopens that saved state. These are static conclusions only; the replay stopped before exercising them.
- Current replay evidence is limited to the first failure; fix the command token and rerun before treating any later mixed checks as exercised. The report's separate `kestrel-shortcuts` scenario is not failed.

## Final runtime outcome

Both final release replays pass1222 scoped checks plus Kestrel,7631visual/7740performance frames. All six final960x640 captures were inspected. Quiet input-state p991.853625ms; commit p9931.978542/max82.201708ms; picture p995.306542/max10.495834ms. First native replay failed only at nonexistent silence; canonical hold-silence repair preserved all guard assertions. Second passed1145checks before screenshot quota160; scoped-only allowance180 retains all named comparisons without changing waits/assertions. Final source has54 frozen Rust files. Release media1PASS20.443s49filtered,4cases,240pictures384384samples across8zero-offset windows, no flags/uncovered samples. SNR minima29.923751grow/32.589322shrink dB. Source/binary/media identities are retained; no requirement or gate closes.
