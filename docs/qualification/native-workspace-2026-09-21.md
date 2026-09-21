# Native project workspace qualification, 2026-09-21

The app now connects create/open, source registration, explicit insertion,
durable history and exact Source/Sequence frame inspection. The implemented
boundary is described in [Native workspace](../NATIVE_WORKSPACE.md). It is an
initial editing workflow; playback, full structural editing, generated-provider
preview, export and release qualification remain open.

This work starts from `4f7adf54af0361bb80c1d493f8a3f84a120df97e`, whose
[CI run 35652876756](https://github.com/micthiesen/deadpan/actions/runs/35652876756)
passed. Hardware is Apple M5 Max, 128 GiB, macOS 26.5.2, Rust 1.97.1, Metal and
the pinned LGPL FFmpeg 8.0.3 developer prefix at
`/tmp/deadpan-media-compatible-xyhilms4/prefix`. Core schema 10 and database
schema 15 are unchanged. Evidence belongs to the
[native workspace record](../../tools/media-qualification/evidence/2026-09-21-native-workspace/).

## Headless and actual-media checks

The app service tests use real original retention, selected video/audio decoders,
qualified receipts, SQLite history and package reopen. Import registers without
inserting; explicit insertion selects the automatic project basis, creates an
ordinary Source beat, and supports complete undo/redo across reopen. Explicit
PCM audio import retains its measured samples; an invalid selected stream fails.
An offline linked source invalidates cached insertion without losing registration.

Controlled channels hold actual prepared results at worker boundaries. Edits and
history continue during preparation, failed project switching preserves the prior
session, progress preserves command errors, cancellation/switch reject late
results, and a lost worker reports failure. Uncached insertion retains its exact
revision/target and cannot retarget after undo. Committed insertion revision/node
markers survive an unread mailbox, concurrent import stages and stale replies;
consecutive insertions produce distinct identities. Shutdown rejects new work,
finishes an admitted command, cancels preparation and releases the writer lock.

Preview tests decode real frames from registered Source and immutable Sequence
requests, compare exact pixels, and check Freeze, Background and empty sequence
behavior. Revision, receipt, stream/index and closed-session guards reject
mismatches. A duration change maps project frame 20 to original frame 41; endpoint
policy holds original frame 119 at the final boundary. Cancelling an earlier open
does not strand a later self-contained project request. Clearing releases retained
raw state and still allows later project requests. Chunked index comparison
observes cancellation before completing a long metadata scan.

Binding tests cover native Command flags, Ctrl-R aliasing, counts/overflow,
no-timeout prefixes, invalid-prefix reset, boundaries, pane cycles, logical
punctuation/layout modifiers, text editing and IME suppression. Headless egui
frames verify that native arrows do not steal pane focus, Tab moves to a real
pane target, and Escape/Return retain same-frame text before leaving the field.
Dialog tests cover pending/cancelled/completed futures, one-result ownership and
project-extension handling. Geometry checks bound Retina render targets and
preserve wide/portrait project aspect.

## Native appearance and interaction

The Metal startup/shutdown smoke test passed. Live interaction used a temporary
`.app` development wrapper because the automation inventory cannot select Cargo's
unbundled process. The wrapper contains the same debug executable and references
the developer libraries; it is not a signed or portable distribution.

The native save sheet created a disposable `Keyboard review.deadpan` package.
The native open panel imported `cfr-bframes.mp4`, leaving zero sequence beats.
Command-Return inserted one 120-frame source and adopted 320×180 at 30000/1001.
The large picture area showed the numbered/color fixture, with source controls
left, sequence below, readable context/boundary status and labeled accessibility
nodes. The final boundary displayed authored frame 119 while reporting boundary
120/120 and displayed frame 120 in one-based UI numbering.

Observed keyboard behavior includes `12l` reaching boundary 12, Tab from Sequence
to Sources and onward to Viewer, reverse pane cycling, `/` source search,
native text arrows without changing picture position, `:source` with Return,
Command-Z/Command-Shift-Z undo/redo, and native Option-E then E producing `é`
without editing the sequence. Command-O selected the already-open package and
reported that it was already open, preserving its cursor and writer session.
Normal Command-Q left no app process. A rebuilt app reopened the saved package
with `--project`; the final Escape check restored the labeled Sources pane,
`:sequence`/G displayed the same last frame, and `:help` showed readable shortcut
help. Headless validation of the UI-created package reports two nodes, 120 frames,
320×180, 30000/1001 and `valid: true`.

Review found that egui could surrender text focus before Escape routing. Text
widgets now retain that event until after same-frame input is processed, and
then restore pane focus. This is covered by real headless egui frames in addition
to the native follow-up. An early native file-panel automation batch entered only
a suffix while the Go To sheet appeared; entering the path after observing the
sheet completed import. A clipboard-based retry timed out. These were recorded
as automation failures, not successful interactions.

The observed full window retains picture dominance and clear selected states.
Attempted corner/zoom operations did not change its dimensions, so this run makes
no new small-window layout claim. Geometry is tested headlessly. Full VoiceOver,
CJK IME, non-US physical keyboards, light appearance and the complete keyboard-only
editorial acceptance session remain untested. Accent composition and structural
accessibility inspection do not establish those broader claims.

## Independent review and verification

Three independent reviews cover general workspace/keyboard integration, service
concurrency/persistence, and preview identity/cancellation. The service review
found a shutdown race that could discard an accepted command. Admission now
rechecks stopping after acquiring the busy slot; shutdown drains that slot and
stops processing import replies. The native window cancels close while a command
finishes. The deterministic shutdown regression and focused recheck pass.

Preview review found an uninterruptible full frame-index comparison. Both
catalog and reopened-session comparisons now poll cancellation between bounded
chunks. A proposed per-frame original freshness check was dismissed: retained
private media intentionally represents the verified immutable committed revision,
including after its external original changes. New insertion still verifies the
current original before committing. Decoder reuse cannot admit modified bytes or
change authored state. The reviewer withdrew that finding after rechecking the
contract. Display resets now retain the verified decoder across ordinary context
and revision requests; session closure or loss of a source still clears it.

The general review found two avoidable UI failures. Native creation now appends
`.deadpan` to names with other suffixes instead of passing an invalid package path
to the store. Import controls and their keyboard entrypoint reject a second
chooser while preparation is active. The extension regression preserves the
authored filename while ensuring the required package suffix.

The final repository gate passed formatting, strict workspace Clippy, **630 Rust
tests** with zero failures or ignored tests, locked build, doctor and native
Metal startup/shutdown smoke. The app contributes 43 unit/integration tests.
[Gate commands and durations](../../tools/media-qualification/evidence/2026-09-21-native-workspace/gate/report.json),
compressed logs and the native observations preserve the exact results. Earlier
629-test and 630-test pre-review runs are retained separately. Earlier checks caught test harness texture
cleanup requirements and Clippy style failures; they were corrected before the
final gate. Native adapter code and shaders are unchanged, so sanitizer and
GPU pixel-matrix suites were not repeated. Existing actual-decoder fixtures run
through the app's new source/sequence boundary.

Full-size media latency/memory budgets, audio playback/scheduling, recovery UI,
legacy media requalification, model runtime ownership, generated-provider preview,
shared export and clean-machine packaging remain open. All DP requirements and
Gates A through G remain open or partial.
