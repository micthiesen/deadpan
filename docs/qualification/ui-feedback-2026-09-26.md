# UI feedback qualification, 2026-09-26

The application now has a repeatable offscreen UI feedback loop using production
widgets, event routing, project/media services and Metal. The complete visual run
passed nine UI scenarios and the Kestrel shortcut audit. The final release run
passed navigation budgets and failed both edit-to-picture budgets. No requirement
or delivery gate becomes complete from this evidence.

The loop found a real selected-card resize defect, now fixed and covered by a
regression. It also preserves two current interaction warnings: Camera requires
inspector scrolling at the default size, and seven of eight rapid Repeat intents
were rejected while the service was busy. See [Interaction review](../INTERACTION_REVIEW.md)
for follow-up priorities and [UI feedback](../UI_FEEDBACK.md) for the run contract.

The [retained summary](../../tools/ui-feedback/evidence/2026-09-26/summary.json)
contains source-report hashes, all check outcomes, selected diagnostic values,
raw measured distributions and named checkpoint state. It identifies the final
visual06, performance02 and playback08/comparison runs. Full per-frame reports
and contact sheets remain in scratch. Four representative images and the
verification logs are retained beside the summary; the original reports were
not edited. The [evidence index](../../tools/ui-feedback/evidence/2026-09-26/README.md)
describes the retained files.

## Environment and build identity

| Item | Recorded value |
| --- | --- |
| Hardware / OS | Apple M5 Max, macOS 26.5.2 |
| Rust | `rustc 1.97.1 (8bab26f4f 2026-07-14)` |
| Git HEAD | `c03a5edde5f28d27074745eb15711cb28b1f2e50`, with concurrent uncommitted changes |
| Visual executable SHA-256 | `e609d59a09c1c70d2327ccaf5960a0ab489804e2070d07234ac5d0e3d8c310a1` |
| Release executable SHA-256 | `20aea675e3f60ed915d5bb32dbb539288c5587852a0850de456c5ddb9307cd50` |
| Cargo.lock SHA-256 at both runs | `47a1b10840b4ffe6f38e5f930ce9c43d303b600988d8bd289ac788d4b154be5b` |
| Fixture | `cfr-bframes.mp4`, 120 frames, SHA-256 `5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918` |
| Initial viewport / replay time | 1280×820 points at 1×; 60 Hz simulated timestep |
| Cache state | First import/index cold; subsequent operations warm; OS file cache uncontrolled |
| Power, thermal and external load | Uncontrolled. The final release invocation ran as the only UI replay job; other agent/system work was not controlled. |

The visual tracked-diff SHA-256 was
`6dfde159901fd8b568257ed21a393c3affaa47d4204e8615855e91b946b86908`;
the release tracked-diff SHA-256 was
`0013312188d0cfba59f4dbb12e017303546b54d7394360d3ab46d943a77e56a5`.
Git metadata describes the checkout at replay start. The executable digest
identifies the actual build while another agent changes the shared checkout.

The Codex sandbox did not expose a Metal adapter. Successful replay used host
execution with Metal access. No software-renderer result was substituted.

## Visual replay

Run: `/tmp/deadpan-ui-visual-06/report.json` and its sibling `report.html`.
This debug run took 208.98 seconds and retained 425 PNG captures. All 358
assertions passed. All nine scenario entries plus the shortcut audit passed:

| Scenario | Result and relevant scope |
| --- | --- |
| `workspace` | Pointer navigation, `,i` insertion, selection reveal, resized/scaled viewports and monitor drag passed. |
| `editing` | Repeat/setter, undo, exact Hold insertion/duration and selected scope passed. |
| `camera` | Numeric draft, restoration after Cancel and Apply passed. A warning records required inspector scrolling before the Camera button is fully clickable. |
| `menus` | Menu key ownership, actual help scrolling/paint and text cancellation to the exact pane passed. |
| `delayed-preview` | Held real reply, stale delivery, resize, failure injection and recovery passed. |
| `rapid-input` | All eight edit intents were accounted for; one committed and seven were rejected. Final navigation intent and idle repaint passed. Rejection remains a warning. |
| `playback-feedback` | Injected preparation, playback, pause, stale update, device failure and cancellation passed at the state/routing boundary. Native output and PCM were not exercised. |
| `large-project` | Real SQLite fixture with 10,000 silent Background Holds passed bounded-card, wheel, selection and resize checks. |
| `edit-latency` | Two visual Repeat/undo and Hold/undo cycles per type passed exact structure and baseline restoration checks. Debug timings are not performance claims. |

The Kestrel audit passed 3,472 routing cases for 62 global reservations. Both the
evaluated fixture and supplied live source had SHA-256
`2b10b5b9428ef9745829576267c2cc5478111c5839e728b6c19e79c5dc8e21a0`.
Deadpan's former Cmd+Return insertion conflicted with Kestrel's Ghostty launcher;
the application now uses `,i`, retaining `:insert`.

The selected-card defect occurred when beat 9994 was selected in a 10,000-beat
outline and a 1280×820 window shrank to 960×640. The old scroll offset no longer
matched the changed card spacing. The strip now reveals selection when card
spacing or viewport width changes. The focused regression and final replay pass.

Representative static inspection confirmed the selected final card remains fully
visible at 960×640 ([retained image](../../tools/ui-feedback/evidence/2026-09-26/large-project-minimum.png)) and the initial Camera action is
partly below the inspector clip at 1280×820 (`camera-007.png`). These observations
do not claim that every retained image has had a complete aesthetic review.
The final focused playback08 follow-up passed its painted-state check, following
inspection of the preceding playback07 result. It retained a first-frame defect:
the error text's bounds were y=818..830 in an 820-point-high viewport, so the
message was clipped. On the next frame the bounds were y=795..807 and fully
visible. Compare the retained [entry frame](../../tools/ui-feedback/evidence/2026-09-26/playback-error-entry.png)
and [next frame](../../tools/ui-feedback/evidence/2026-09-26/playback-error-visible.png).
First-frame clipping remains interaction work; the later visible frame does not
erase it. The [loaded workspace](../../tools/ui-feedback/evidence/2026-09-26/workspace.png)
and the retained fully visible error frame were inspected at full size.

Baseline comparison initially failed because the valid full visual report exceeded
the comparator's 16 MiB report limit. The limit is now 128 MiB with two regression
tests. Playback08 successfully compared against the full visual06 report and
generated 12 baseline/difference images. The subset run explicitly warns about
omitted baseline scenarios, a new checkpoint without a baseline and pixel changes.
Those warnings are review evidence, not silently accepted baselines.

## Performance results

Final run: `/tmp/deadpan-ui-performance-02/report.json`. This release run took
34.89 seconds, recorded 1,328 assertions and captured no screenshots. Every
scenario except `edit-latency` passed. Both of that scenario's timing gates failed;
its structural and sample-accounting checks passed. The earlier preliminary run
is not the source of the results below.

| Measured interval | Samples | p50 ms | p95 ms | p99 ms | Maximum ms | Gate |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Warm navigation input-frame CPU | 120 | 1.06 | 1.32 | 1.47 | 2.28 | Pass, p95 < 8 ms |
| Warm navigation input to offscreen picture completion | 120 | 5.56 | 21.16 | 69.85 | 71.59 | Pass, p95 < 80 ms |
| 10,000-beat navigation input-frame CPU | 160 | 1.08 | 1.23 | 2.19 | 2.35 | Pass, p95 < 8 ms |
| Cached Repeat input to observed commit | 40 | 2.64 | 3.26 | 65.78 | 65.78 | Diagnostic interval |
| Cached Repeat input to offscreen picture completion | 40 | 56.36 | 75.26 | 127.37 | 127.37 | **Fail**, p95 < 50 ms |
| Hold input to observed commit | 40 | 42.02 | 69.65 | 71.03 | 71.03 | Diagnostic interval |
| Hold input to offscreen picture completion | 40 | 84.20 | 138.65 | 141.87 | 141.87 | **Fail**, p95 < 100 ms |

All measured samples in these rows completed; there were no failed or timed-out
samples in these subsets. A completed sample can still exceed its performance
budget. Navigation excludes 16 warm-up inputs; edit measurements exclude four
warm-up cycles per type and each following undo. Every measured edit matched one
observed commit and one picture completion before undo restored the authored
Original baseline under a fresh revision.

Input-frame CPU covers the complete egui input frame, an upper bound on CPU work
to its state update. Picture completion is the actual offscreen composition's GPU
completion. Neither measures native compositor delay or physical scanout.
PNG/readback work is absent from the performance run. Ticket-bound telemetry
excludes failed, superseded, stale and repeated/resize work from successful
latency distributions while retaining those events in the trace.

The Repeat and Hold misses remain open. The paired samples and stages support
investigation; subtracting unrelated percentile values does not identify a single
bottleneck. No threshold was relaxed to make this run pass.

## Repository checks

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | [Passed](../../tools/ui-feedback/evidence/2026-09-26/fmt.log). |
| Default workspace Clippy | [Passed](../../tools/ui-feedback/evidence/2026-09-26/workspace-clippy.log). |
| `cargo clippy --workspace --all-targets --features deadpan-app/ui-harness --locked -- -D warnings` | [Passed](../../tools/ui-feedback/evidence/2026-09-26/workspace-feature-clippy.log); final [app-feature Clippy](../../tools/ui-feedback/evidence/2026-09-26/app-feature-clippy.log) also passed. |
| `cargo test -p deadpan-app --features ui-harness --locked` | [180 unit and 2 integration tests passed](../../tools/ui-feedback/evidence/2026-09-26/app-feature-tests.log), including the report-size regressions. |
| Default workspace tests | The [full invocation failed](../../tools/ui-feedback/evidence/2026-09-26/workspace-tests-failed.log) at `canonical_source_pcm_uses_fixed_monitor_gain_and_delivery_clock` in `deadpan-playback`; a [focused rerun also timed out](../../tools/ui-feedback/evidence/2026-09-26/playback-focused-failed.log). The same workspace binary later [passed directly](../../tools/ui-feedback/evidence/2026-09-26/playback-exact-binary-passed.log) in 12.48 s with `RUST_BACKTRACE=1`, `--exact tests::canonical_source_pcm_uses_fixed_monitor_gain_and_delivery_clock --nocapture --test-threads=1`. The timeout cause is unestablished. |
| Remaining render/source/store test binaries and doctests | [Remaining binaries](../../tools/ui-feedback/evidence/2026-09-26/remaining-test-results.json) and [doctests](../../tools/ui-feedback/evidence/2026-09-26/doctests.log) passed separately after the workspace failure. This does not erase that failure. |
| Workspace build and doctor | [Build](../../tools/ui-feedback/evidence/2026-09-26/workspace-build.log) and [doctor](../../tools/ui-feedback/evidence/2026-09-26/doctor.json) passed. |
| Native startup/shutdown smoke | [Passed](../../tools/ui-feedback/evidence/2026-09-26/native-smoke.log). This checks lifecycle, not the editorial workflow, native accessibility or media quality. |

The optional-feature lint/tests are now in CI. They exercise contracts and compile
the harness; they do not run this Metal scenario suite or inspect its images.

## Evidence boundaries and remaining checks

- Picker results are scripted; macOS panels, physical key delivery, global
  interception, native focus, CJK IME and non-US layouts remain native checks.
- Accessibility labels/geometry are inspected through the headless tree. VoiceOver
  navigation and accessibility conformance remain unverified.
- Playback updates are injected. Real PCM, native device timing, audible silence,
  acoustic synchronization and listening quality are outside this replay.
- The 120-frame video and 10,000 Background Holds do not qualify long media,
  full-size playback, large asset inventories, memory pressure or all effects.
- The final visual suite passed its assertions; Camera reachability, rapid edit
  rejection and the two measured performance misses remain product work.
- The focused painted-playback follow-up and corrected baseline comparison passed.
  First-frame error clipping remains visible in the retained images and open as
  product work. The compact summary and logs remain available if scratch reports
  expire.

The later focused playback pass and the two earlier timeouts remain part of the
retained record. There was no uninterrupted green run of the whole workspace suite,
and the cause of the timeout is unestablished. This qualification does not convert
a retry, a visual assertion or an offscreen GPU measurement into evidence for a
boundary it did not observe.
