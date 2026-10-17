# AI generation sleep interruption, 2026-10-17

This advances DP-18; it does not complete the requirement. Base revision:
`4742c901`. Environment: Mac17,7, 128 GiB RAM, macOS 26.5.2 (25F84),
Rust 1.97.1, locked dependencies and the qualified FFmpeg 8.0.3 prefix.

## Behavior and ordering

The native observer delivers typed WillSleep and DidWake events to an owned
atomic lifecycle epoch, retaining the existing playback stop behavior.
Callbacks do not take the Jobs or project-writer mutex. The epoch retains
coalesced sleep/wake events and invalidates old AI generations even after wake.

Queued AI work never loads a model after interruption. Running work uses its
existing cooperative cancellation and teardown; it retains its resource while
draining. The sole writer records Interrupted rather than user Cancelled,
preserves request inputs and interrupts queued/claimed automatic preparation
intents. Wake does not retry them. Explicit retry revalidates inputs and creates
a new attempt. Earlier user cancellation retains its own reason. Manual
conditioning interrupted before inputs are recorded instead tells the user to
select the pause and run `:generate` again; it does not advertise an unavailable
Jobs retry.

Publication observes interruption between object promotions and at the final
Ready transaction boundary. An interrupted provisional Ready transition rolls
back. The final successful guard observation is the ordering point: a later
notification overlapping SQLite commit does not retract that publication.
Accepted providers and previously Ready variants remain intact.

## Verification

Private Foundation notification-center tests pass for exact event ordering,
token removal and refusal of off-main registration. They do not request physical
sleep or post notifications to NSWorkspace.

Regression coverage includes coalesced epochs, sleeping admission and Run anyway,
running and queued service interruptions, unchanged authored revisions,
same-request/new-attempt retry, cancellation precedence, interruption during
object promotion and final Ready commit, retained automatic preparation intent,
reopen, and preservation of an independent worker failure during finalization.

Independent review found cancellation precedence and independent failure
reclassification races. Both were corrected; publication ordering is explicit
above. The first scoped gate stopped on the new parity check's strict Clippy
`manual_is_multiple_of` lint; the check now uses `is_multiple_of`.

`cargo xtask check` passes both normal and UI-harness strict Clippy. The first
test selection passed 130 tests in 212.854 seconds, including the existing
preparation-compaction regression in 207.343 seconds. Nextest reported an output
handle leak for the existing power/playback/memory queue test; its isolated
rerun passed in 1.043 seconds without a leak, as did the final selection.
After review fixes, the expanded selection passed 144 tests and found one test
setup error: the new corruption fixture attempted to overwrite a read-only
published object. Replacing that disposable fixture file corrected the setup;
the scoped gate and regression then passed (0.227 seconds). All 145 tests in
the expanded selection are covered, plus the earlier compaction test. That
unaffected long-running test was not repeated after publication-only fixes.
The native observer's two focused tests also pass. No assertion was removed
and no timeout was raised. The linker reports the existing oversized DWARF
unwind-table warning in the debug app build.

Logs: `/tmp/deadpan-sleep-check-2026-10-17-v2.log`,
`/tmp/deadpan-sleep-isolated-2026-10-17.log`,
`/tmp/deadpan-sleep-check-2026-10-17-final.log`, and
`/tmp/deadpan-sleep-check-2026-10-17-final-fix.log`.

The first release visual/performance Jobs replay passed 34 checks, and native
Metal startup/shutdown passed. Visual inspection then found that lower Jobs
rows could be clipped without keyboard selection scrolling their action buttons
into view. Keyboard row movement now reveals the whole selected row, and the
list keeps a visible scrollbar. The initial captures remain in
`/tmp/deadpan-sleep-jobs-visual-2026-10-17`; they are not the final UI evidence.

The keyboard regression initially failed compilation because it accessed a
private selection field. A read-only accessor confined to `ui-harness` fixed
that without changing production visibility. The final UI gate passes both
strict Clippy configurations and all 37 selected Jobs tests in 2.031 seconds
(`/tmp/deadpan-sleep-check-ui-2026-10-17-v2.log`).

## Final release UI and native checks

Release app SHA-256:
`ef25ec807b23aae3503886cbb07bd45ef412c270f267cf4ac3b5a5c82cef5f48`.
The retained executable and its helpers are in
`/tmp/deadpan-sleep-final-bin-2026-10-17`.
Visual and isolated performance Jobs replays each pass 37 checks, plus the
production Kestrel routing audit. Reports are in
`/tmp/deadpan-sleep-jobs-visual-final-2026-10-17` and
`/tmp/deadpan-sleep-jobs-performance-final-2026-10-17`.
The interrupted rows, selected lower row and explicit retry captures were
inspected: labels wrap, focus is clear, and the selected row's complete Retry
and Discard buttons are visible. The replay checks actual paint clips and
opaque overlap, not only accessibility nodes. Native startup on Apple M5 Max
Metal and window shutdown pass with this same executable
(`/tmp/deadpan-sleep-native-smoke-final-2026-10-17.log`).

In the small scripted fixture, performance replay UI frame CPU p95 is 0.461 ms
over 2,297 frames; input-to-state p95 is 1.319 ms over 98 inputs. Seven
input-to-commit samples have p95/max 85.779 ms. These are not real-inference
interference measurements. Reports retain layout warnings for second-pass
footer sizing and separate-input retry sequences; no layout assertion failed.

## Limits

No physical sleep/wake cycle or real model interruption was exercised. The
service tests use the real writer and durable attempt path with scripted worker
progress; publication tests use qualified synthetic media. Other model jobs,
running inference/playback interference, byte budgets and the remaining DP-18
scope stay open. Physical sleep steps are recorded in Requirements' owner list.
