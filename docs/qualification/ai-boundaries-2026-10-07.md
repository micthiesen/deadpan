# Source-boundary AI replacements

Source-boundary replacement is implemented and verified on this Mac. The
repository gate completed in parts; real-model generation, native replays,
release responsiveness and model-free packaged export passed. DP-12 remains
Partial: one-sided extension and the existing owner-only quality checks remain
separate from this milestone.

## Implemented behavior

- The store derives required accepted-Hold fallbacks from the complete base
  edit, then commits them and the original edit as one reversible transaction.
  Compound leaf identities, register captures and checkpoints remain intact.
  Serialized replacement lists must equal the store's independent derivation.
- Conditioning equality uses retained source qualifications and measured frame
  ordinals. Generated inputs bind the sampled master, ordinal and saved crop;
  explicit black differs from an absent definition endpoint. Ordinary zoom,
  captions, shortening and whole-group movement preserve matching raw inputs.
- Neighboring generated Holds are solved together. A fallback that makes a
  neighbor valid preserves that neighbor. Cyclic dependencies follow a bounded,
  deterministic policy with an explicit cyclic cause; fresh acceptance also
  participates in this calculation.
- Small accepted-origin receipts retain exact controls, measured inputs and
  qualified bundle evidence. Discarding a previously accepted candidate does
  not invalidate its historical admission. Full validation checks chronology,
  fresh aliases, exact asset records and the request's mapped acceptance scope.
- Current automatic intent survives conditioning, fulfilment, Ready and work
  metadata compaction. Replacement consumes its exact predecessor once.
  Explicit cancellation/provider choice closes it; Undo never revives a closed
  activation, and Redo creates fresh work from immutable original authority.
- Native explicit cancellation closes the captured automatic intent separately
  from the worker stop used for rendering or shutdown. Ready still requires
  explicit acceptance. Native comparison and acceptance feedback report the
  final saved provider, including a fallback caused by changed neighboring
  inputs. Editing paths retain queue-capacity notices without hiding the saved
  edit.

No decoder, model or provenance-sidecar I/O was added to command planning.
The database is schema 72; older unused development packages need no migration
under the owner's session authorization.

## Repository and native verification

The evidence root is `/tmp/deadpan-resume-20261006`. A retained copy of reports,
exact scripts and selected captures is in
[`tools/model-qualification/evidence/2026-10-07-boundaries`](../../tools/model-qualification/evidence/2026-10-07-boundaries/manifest.json).
The manifest records hashes of the original uncompressed evidence. Large logs
and JSON reports are gzip-compressed; media and executables stay outside Git.

| Check | Result |
| --- | --- |
| Core envelopes, exact plan queries, measured identities and context foundations | 103 passing focused tests before durable integration; strict Clippy passed then. |
| Store/CLI/app compilation | Passed through the workspace gate, strict all-target lint and optimized builds. |
| Boundary decisions and host derivation | 25 passed, including 14 host cases and 11 graph cases. |
| Durable intent and accepted-origin tests | All 15 intent and 14 origin tests passed. An earlier expanded invocation failed because a new index test expected an obsolete fixture iteration ID; the corrected test passes in the final 178-test store unit run. |
| Transaction integration | Three new tests passed: atomic replacement/renewal/cancel/Redo, Compound checkpoints and final-boundary comparison, and rejection of a forged tail. The full bundle rerun also passed the scoped-preview extension. |
| Existing bundle integration | All 61 passed in `boundary-bundles-final.log`, 131.27 seconds for the suite, then passed again within the workspace run after the allocation-bounds correction. The earlier individual compaction run also passed in 254.02 seconds. These debug timings under concurrent test load are not release latency measurements. |
| Final store unit suite | All 178 passed in `boundary-store-all-unit-final.log`, including the bounded 99,000-Hold/250-Repeat case, allocation refusal, canonical ownership and nested isolation. |
| Native final-provider feedback | All eight tests passed in `boundary-native-provider.log`. |
| Native preparation service | All six tests exercised the actual helper binaries and passed in `boundary-native-preparations-required.log`. The earlier 0.01-second invocation skipped work internally; it is not verification. Required-helper checks and discovery of a complete helper pair now pass in the workspace and final focused runs. |
| Final focused native tests | All 47 passed in `boundary-final-native-default.log`, including actual generation helpers, final-provider feedback and capacity-notice delivery. One passing test carried a nextest cleanup warning, retained below. |
| Strict Clippy | Workspace and UI-harness all-target lint passed in `boundary-final-clippy.log` and `boundary-final-clippy-ui.log`. |
| Workspace regression | 5,084 tests ran: 5,080 passed, four failed and ten skipped in `boundary-workspace-tests.log`. All four corrected cases passed in the focused continuations described below. The original invocation remains failed. |
| Python and fixture checks | Two source-audio fixtures verified. All 113 Python tests passed: audio measurement 20, model worker 84, FFV1 five and native media harness four. Commands and exit codes are in `boundary-python-checks.json`. |
| Native boundary replay | All 26 checks plus the Kestrel routing audit passed in `boundary-native-replay-2/report.json`. It covers Slip, final fallback, atomic Undo/Redo, Retry and Discard. |
| Final UI gate | All 1,069 tests passed, with two explicit skips, in `boundary-final-native-ui.log`; both workspace doc tests passed. |
| Final release visual replays | All 178 scenario checks passed: boundaries 26, duration replacements 14, scoped generation 40, variants 42 and counted insertion 56. Each run also passed its Kestrel routing audit. `boundary-final-visual-*/report.json` retains layout warnings and all frames. |
| Timing regression | After the harness correction below, strict default/UI lint and all 12 telemetry tests passed in `boundary-timing-checks.json`. |

The bundle integration fixtures contain synthetic admitted objects. They prove
store behavior and history, not generated-picture decoding or model quality.
Independent reviews identified and corrected candidate-discard proof loss,
fresh-birth cancellation, scope preview using the unwrapped command, and missing
first-admission proof details. Review also found that eagerly copying every
Hold's Repeat ancestry could allocate excessive memory in deep documents; the
bounded shared index now passes unit tests and independent review. Eight native
feedback tests cover acceptance that returns to fallback and its queued,
unavailable, cancelled or absent replacement status.

The workspace failures exposed outdated fixture assumptions. The generated
neighbor and repeated-pause examples now author their final context before
genuine qualified acceptance; wrapping or inserting after acceptance correctly
changes those inputs. Both CLI checks pass in `boundary-fixture-tests-cli.log`.
The store history fixture now uses dedicated bundle admission and explicit
relevance for edits and Undo/Redo instead of injecting an unproven generated
snapshot. Its first correction omitted relevance and failed; the final test
passes in `boundary-final-store-fixture.log`. Schema tests now require version
72 and reject every obsolete version through 71 without writes. All 26 other
store fixture/migration checks passed in `boundary-fixture-tests-store.log`.

Nextest reported a cleanup warning for the passing workspace test
`deadpan-playback::tests::original::loop_faults_invalid_windows_and_overflow_are_explicit`
and the passing focused native test
`project::tests::generation::an_unavailable_runtime_reports_its_reason_and_records_nothing`.
These warnings remain in their logs.
The isolated playback rerun passed without that warning in 0.122 seconds;
the same native test passed in the full UI suite without a cleanup warning.

The first native replay failed because it assumed a selected Source fragment
was a direct Source node; this fixture uses a neutral Sequence partition.
The corrected check compares the resolved physical Source. Offline inspection
of `ai-boundaries-053.png` and `ai-boundaries-063.png` confirms the saved
12-frame fallback and readable Jobs Retry/Discard controls. Five frames requested
a second layout pass; no ignored retry or three-frame identical retry failed.
The debug linker's large-unwind-section warning remains in the build log.
Final optimized captures `boundary-final-visual-ai-boundaries/ai-boundaries-062.png`
and `ai-boundaries-076.png` were also inspected. They show the saved fallback,
selected 12-frame pause, separate Original/Edit clocks, complete unavailable
footer and readable Jobs controls. `boundary-visual-review.json` records the
review and capture hashes. This is offscreen visual evidence; no live-window
screenshots were used.

## Real-model generation and development export

`boundary-real-generation-1/summary.json` passes all **119 checks**. The script
`verify-boundary-generation.py` creates a fresh schema-72 project from
`native/deadpan-source/tests/fixtures/cfr-bframes.mp4`, inserts a silent 30-frame
Hold at frame 20, runs the local model and explicitly accepts its result. A
five-frame Slip of the preceding Source changes only the measured left input.
The test proves one source-edit/fallback/intent transaction, a read-only preview,
stale-candidate refusal without writes, atomic Undo and fresh work on Redo. The
second actual model run consumes the retained final inputs and controls; Ready
leaves the fallback intact until explicit acceptance. Acceptance Undo/Redo and
both generations' retained object hashes pass.

Both inference commands took about 83.5 seconds. The complete script took
170.886 seconds while other verification work ran; this is functional evidence,
not an isolated performance measurement. Rendering with an intentionally absent
model and Python passes emitted-file verification and leaves the final document
unchanged. The resulting MP4 is 180,233 bytes, SHA-256
`ed8d872a619467dbe018a0b56adeeff5865093274de112b858d8608ecf3829b0`.

`boundary-release-build/summary.json` records the frozen executables;
`boundary-release-source.json` records all 66 changed source paths at base
`411b93c95ff7143810299f3c1bf9951f28330c42`. The executed app hash is
`47ef7aa3f3c9d14c262e3230abf12333f275905c38daf2e8b82cee242d4a65fe`
and CLI hash is
`87156574a111f2340dc1ac4c26faa031bbc3a50993576524c54f3b97799ce976`.
This development build includes the replay harness. Its report explicitly ends
with packaged verification pending; the separate follow-up below closes that
obligation without changing the original report.

## Optimized exports and packaged reopen

The four generated-picture recipe exports all pass independent emitted-file
verification: 181 pictures and five audio windows across ordinary, repeated,
reframed and shortened Holds. The repeated fixture is now 60 frames and authors
its one-frame Original endpoint handles before Default generation. Its audible
signal and the other three signals verify zero offset; the silent window has
explicitly inapplicable signal timing. This focused run does not claim a rerun
of every export fixture. See `boundary-generated-exports.json` and
`boundary-release-generated-exports.log`. The executed test CLI hash is
`8cd55c85f4b7827c54808a6da07c56c06c5da65c116db46a2bc004ccac17d54a`.

`cargo xtask bundle --output .../boundary-final-bundle --allow-dirty` produced
a 729.2 MiB ad hoc signed app. Its audit covered 74 Mach-O files, the private AI
runtime and relocated FFmpeg libraries. `bundle-verify` passed positive checks
in a scrubbed relocated copy and negative checks for damaged helpers, worker
code and missing notices. This remains same-Mac evidence under §29.1.

The exact real-model project then passed **30 packaged checks** in
`boundary-packaged-generation-1/summary.json`. With a fresh home, system-only
PATH and no installed model, the packaged CLI revalidated the project and
retained objects, rendered, verified and published the movie, and preserved the
authored document. No inference ran. The movie is 180,233 bytes, SHA-256
`07f3e4a0471afb6a6f78b5c0286fe6d1203aed2ead8e20ee1c949e143ab9dcd7`.
The packaged app hash is
`6bdcb403d4aaf60b3153124bc6cd3ddb4db4b2ddc7fcdc2447a6f2203e3ac6cc`;
the CLI hash is
`762bba9d6ee91bdc3c30477e42ef7504042691d1d5ee0e1e8dfe54cd5075138b`.
All helper hashes and exact command arguments are retained in the report.

## Separate release responsiveness

Four sequential performance runs followed all build, media and package work,
with no other goal build or media job running. Hardware was Apple M5 Max,
128 GiB, macOS 26.5.2, Rust 1.97.1. The harness used 1280×820 at 60 Hz and
the recorded `cfr-bframes.mp4` fixture. Initial import/index was cold, later
operations warm; OS cache, power and thermal state were uncontrolled.
One-minute host load was 3.09–3.88. No screenshot readback ran in measured mode.

The first boundary performance attempt failed its strict sample assertion.
Slip Apply lacked an admission trace, so its apparent commit sample came from
an earlier Undo. Idle dispatch also lost the commit's input origin before the
deferred picture request. The trace showed a real new picture reaching GPU
completion. The correction records actual Slip admission/rejection and retains
the current frame's commit origin through idle dispatch, while actual new input
takes precedence and the next frame clears it. The independent telemetry test
proves exact event intervals and no origin leaking to unrelated later requests.
The failed report remains in `boundary-performance-ai-boundaries/report.json`;
none of its apparent timings count as measurements.

The corrected run passes 382 boundary checks, including four warmups and 40
measured Slip/Undo cycles. Each cycle proves an actual one-frame source change,
one admitted commit, one completed picture, the exact fallback and retained
accepted artifact, and complete restoration with no pending intent after Undo.
Samples begin at Apply Enter after the preview is ready. Preparing that preview,
model preflight waits, durability inspections and Undo are outside the measured
interval. General navigation, edit and large-project runs pass 329, 450 and
526 checks respectively, plus their routing audits.

| Workload | Samples | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Prepared boundary Slip, Apply to durable commit | 40 | 18.573 ms | 20.148 ms |
| Prepared boundary Slip, Apply to completed GPU picture | 40 | 22.094 ms | 23.454 ms |
| Warm navigation input CPU | 120 | 0.246 ms | 0.279 ms |
| Warm navigation to completed GPU picture | 120 | 1.436 ms | 1.553 ms |
| Cached Repeat to completed picture | 40 | 9.534 ms | 9.907 ms |
| Hold insertion to fallback picture | 40 | 14.973 ms | 15.293 ms |
| Navigation among 10,000 root beats, CPU | 160 | 0.549 ms | 0.774 ms |

These general warm workloads meet their existing input, navigation, Repeat and
Hold budgets. Boundary Slip timings are a measured distribution; no new budget
was invented. The mixed startup/navigation trace includes a 226.201 ms picture
completion; the warm results do not qualify all startup or cold operations.
Physical display scanout and listening are outside this harness.
The separate optimized compaction/history test with 257 copied accepted Holds
passed in 12.392 seconds after compilation; that is a whole fixture run, not a
per-edit latency.

The executed performance app hash is
`f1383f111a1c0cea32c29b03de2280dcf911d91a00d9b1918647083796aad194`.
`boundary-performance-summary.json` summarizes the retained raw samples;
`boundary-timing-release-build/summary.json` records every executable.
`boundary-final-source.json` records the frozen source used for the final
visual and packaged builds. The later `boundary-timing-source.json` identifies
only the two harness instrumentation files changed for performance. Those
changes compile only with `ui-harness`; packaged runtime behavior is unchanged.

## Remaining scope

One-sided generation at video edges remains a separate implementation task.
Perceptual identity, seam quality and listening remain on the existing owner
verification list. This milestone does not close DP-12 or the full spec.
