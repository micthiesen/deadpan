# Explicit Repeat input qualification, 2026-09-27

Eight `rr` commands in one native event batch now produce eight separate nested
Repeats and eight undo steps. Previously seven were rejected while the writer
was busy. This increment retains at most sixteen waiting explicit wraps behind
one submitted command. It does not queue setters or general edit commands.
DP-05, DP-20, DP-24 and all delivery gates remain partial or open.

The [evidence index](../../tools/ui-feedback/evidence/2026-09-27-repeat/README.md)
links original reports, inspected captures, logs and source identities. The
[workspace contract](../NATIVE_WORKSPACE.md) describes the user behavior.

## Command and input boundaries

The UI stores semantic wrap counts, never stale writer requests. It consumes a
same-session completion whose document revision, Sequence scope, selected new
Repeat, child, total plays and absent gap match the submitted edit. It then
builds the next request from that committed wrapper and fresh revision. One
request remains in flight; the writer mailbox and revision guards are unchanged.
Writer-idle alone cannot advance the queue. Same-revision progress does not
consume an intent. Failure or a mismatching completion cancels the waiting tail.

Keyboard and pointer input run before continuation dispatch. Escape, another
resolved action, context change, modal entry, window blur or close cancels
waiting wraps. The submitted edit may finish. The notice reports exact waiting,
overflow and cancellation counts. The header stays Working while the queue is
active, including when the writer has finished but its completion is not yet
consumed. Incomplete operators/counts survive the chain's own commits.

The queue is session-local and is not recovery history. Only successful store
transactions become durable edits. Other busy commands retain explicit rejection.
Queued telemetry preserves each intent's original input timestamp in FIFO order.

## Verification and review

- Normal app: 214 unit tests and 2 integration tests pass.
- App with `ui-harness`: 243 unit tests and 2 integration tests pass. Added pure
  tests use real core wrapping transactions to cover FIFO counts, exact completion
  guards, failure, capacity, cancellation, context changes and single dispatch.
  A diagnostic test checks input-origin retention and cancellation.
- Strict Clippy passes for both app configurations with all targets and warnings
  denied. Formatting passes. No full workspace gate was repeated for this
  app-only increment.
- Independent integration review caught first-`r` cancellation through the hint
  action. It is fixed and replayed across an automatic queued continuation.
  Review also strengthened the exact original-node and undo oracles. Undo now
  compares the entire authored document except its deliberately fresh revision.
- Independent image review caught Saved appearing while wraps waited. The header
  fix passes actual paint checks with the writer idle and completion held, at
  960×640 and 1280×820. No new Repeat feedback finding remains.

| Run | Result | Scope |
| --- | --- | --- |
| `visual-01` | Pass, 52 checks | Initial burst, separate commits/undo, held prefix, pointer cancellation, overflow and Escape. Header review subsequently found the misleading Saved state. |
| `rapid-input-02` | Pass, 56 checks | Adds partial `rr` across an automatically dispatched continuation and first-resize notice visibility. |
| `editing-02` | Pass, 30 checks | Existing counted Repeat, setter, pause, duration and undo workflow. |
| `rapid-input-03` | Pass, 60 checks | Adds Working/no-Saved assertions after writer completion at both viewport sizes. |
| `menus-03` | Pass, 9 checks | Existing menu/help and text-focus routing. |
| `rapid-input-release-01` | Pass, 319 checks | Final full-document undo oracle, all Repeat boundaries and warm navigation budgets. |
| `edit-latency-release-01` | Pass, 449 checks | Repeat and silent-freeze Hold commit/picture budgets, each followed by undo. |

Counts include one shortcut-audit assertion per run. Each audit checks 3,472
production-router cases against 62 Kestrel reservations and the live source.
Bindings and reservations did not change. No replay waits were inserted between
the eight burst commands. Some boundary checks intentionally delay delivery of
real SQLite updates while the writer continues normally. No authored document
or completion is fabricated.

The unit suites precede the final header adjustment and stronger replay oracle;
strict lint, actual final visual frames and the release replay cover those
subsequent changes. `rapid-input-03` predates the final all-fields undo assertion;
the release run includes it. The normal editing result predates only the header
adjustment and later harness assertions. This is composite evidence, not a claim
that every prior scenario reran against the final binary. Formatting after the
release run changed only one line wrap in the harness serialization expression.

Development failures remain in logs: the initial default compile found private
child-module methods; the strengthened replay initially attempted to serialize
an Arc instead of its document. Both were fixed. One invocation used the invalid
scenario name `menus-focus` and exited before running a scenario; `menus-03`
uses the actual `menus` scenario and passes. These are not product test failures
or environment blocks. Cargo invocations remained serial and each process reached
a terminal result before another started.

## Visual comparison and remaining gap

The inspected default/minimum captures preserve the saved workspace board's
large picture, quiet panels, restrained lavender selection and visible keyboard
guidance. Pending feedback occupies the existing notice area; overflow uses the
existing error treatment. It does not add another panel beside the picture.
Both cancellation counts and the Escape hint are readable in the actual images.

The first burst frame immediately follows `:sequence` closing. Its notice is
present, including actual rasterized text, but the status panel retains the
prior command height for one frame. This leaves about seventy points of blank
space and a temporary picture/footer shift. The next frame settles. Both frames
are retained as `queue-first-paint.png` and `queue-progress.png`. Independent
review confirms the size logic is unchanged from the base commit. The
[interaction review](../INTERACTION_REVIEW.md) keeps this separate layout defect
open; the text-visibility assertion does not establish stable overall geometry.

## Release responsiveness

Both release runs pass with executable SHA-256
`6b9aad852505941337f42cac9cbee842d7342a2731b0f75ff826e6f6c8b7bd30`.
The optimized build completed in 4m57s before measurement. No other test or
replay ran concurrently, and no timing sample includes screenshot readback.

| Interval | Samples | p50 ms | p95 ms | p99 ms | Maximum ms | Gate |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Warm navigation input-frame CPU | 120 | 0.53 | 0.69 | 0.75 | 0.77 | Pass, p95 < 8 ms |
| Warm navigation input to picture completion | 120 | 3.79 | 4.91 | 5.74 | 5.78 | Pass, p95 < 80 ms |
| Cached Repeat input to observed commit | 40 | 1.59 | 3.03 | 3.17 | 3.17 | Diagnostic |
| Cached Repeat input to picture completion | 40 | 5.52 | 6.76 | 6.79 | 6.79 | Pass, p95 < 50 ms |
| Hold input to observed commit | 40 | 3.63 | 4.49 | 4.71 | 4.71 | Diagnostic |
| Hold input to picture completion | 40 | 7.85 | 8.98 | 9.50 | 9.50 | Pass, p95 < 100 ms |

All samples in these subsets completed without failures or timeouts. Navigation
excludes sixteen warm-up inputs and now runs on the Original restored after
the burst/cancellation checks; the prior layout run navigated its single admitted
Repeat. Each edit type excludes four warm-up cycles, measures forty edits, and
undoes each to the original authored baseline. Budgets are unchanged. These are
small-fixture offscreen GPU intervals, not physical scanout, heard audio or a
controlled estimate of improvement from this queue. Power, thermals, OS cache
and external host load remain uncontrolled. The 10,000-beat workload was not
rerun in this increment.

## Host and limits

The host is an Apple M5 Max on macOS 26.5.2, using Rust 1.97.1 and the qualified
FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`. The 120-frame `cfr-bframes.mp4`
fixture has SHA-256
`5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.
Base commit is `c09a5f616ed96c4b8a9621eff3a0e3afc833a110`. Reports retain binary,
lockfile and checkout identities; `source-identity.json` hashes final changed
source independently of report-time dirty metadata.

Imports, store commands, decoding, production event routing and Metal composition
are real. UI delivery holds are controlled harness boundaries. This does not
qualify physical keyboard delivery, native IME, VoiceOver, acoustic output,
physical display latency, crash recovery of unsubmitted intents or packaging.
Visual readback timings are not performance evidence. Thumbnails, full sound
placement, remaining editing grammar, AI application workflows and export remain
required by the full specification.
