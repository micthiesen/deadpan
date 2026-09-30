# Native and public Render qualification

Native Cmd-E, `:render` and the visible Render control now use the shared
automatic SDR workflow. Camera, Gain and Room tone previews require an explicit
commit/discard/keep choice. A changed preview renders from its exact durable
commit receipt. Destination cancellation authors nothing; a later editor Undo
cannot change the captured movie revision. The public closed-project CLI exposes
start, stored status, checkpoint retry, fresh encoding and reconciliation.

This qualifies the bounded development paths below. Full mastering, HDR,
authenticated routing into an open native project, native persisted-job recovery,
expanded runtime/content coverage and release packaging remain open.

## Native interaction

The production `render` replay passes 71 checks with actual media and publication.
It covers Cmd-E and command entry, Camera/Gain/Room tone decisions, source text
and a Render click in one event batch, cancelled destination pickers, a concurrent
revision invalidating a captured decision, and later Undo during real export.
Paint/accessibility assertions cover the decision and result at 960×640 and
1280×820. The Kestrel audit passes against the matching live source digest
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.

The first replay stopped at the modal's invisible sizing pass before export.
The assertion now waits at most three frames for its visible control, then keeps
the complete paint and hit-target checks. That failed report is retained. The
passing run reached the intermediate screenshot allowance; named checkpoints
and semantic frames continued. The parent inspected the final Metal captures.

A native CUA check used an isolated scratch app bundle containing the built
executable. Cmd-E opened the real macOS save sheet in `Exports` with a fresh
project-derived MP4 name. Escape preserved the saved editor. In Camera, Tab/Up
changed Center X from 50 to 51; Tab traversed Commit, Discard and Keep editing in
the Render decision. Escape returned to the draft; another Escape discarded it.
The actual save produced a verified 37,964-byte movie and local report, with no
stale partial path in the success panel. Normal UI shutdown completed. All 13
non-render database tables remained unchanged.

Native filename entry required an accessibility correction and pointer focus
before Return. This is not fully keyboard-only save acceptance. Physical input,
non-US layout and IME behavior were not tested. CUA exposed screenshots inline
but no save-to-file API, so the retained images are the automated Metal captures.
The scratch bundle establishes an addressable test application, not distributable
packaging. The directly launched raw executable could not be addressed by CUA.

## Public commands and emitted files

Eight public `deadpan-app --headless` invocations pass: initial export, stored
status, checkpoint retry, publication reconciliation, stale-revision rejection,
existing-destination refusal, SIGINT cancellation during qualification and JSON
fresh encoding. Retry/reconciliation do not enter encoding or qualification.
Retry retains the original movie hash. Collision refusal preserves the existing
file, cancellation leaves no final movie, and all 13 non-render tables remain
identical to a consistent pre-run SQLite backup.

The synthetic project captures revision `workflow-live-restored`, document hash
`564af137ff203f7a10344a9e50db7a4167cee0e1dfbbecf44ed3b39a3c6ef638`, at 320×180
and 30000/1001 fps. The full range has 128 pictures and 205,005 authored audio
sample frames. Initial and fresh encodes each produce 32,553 bytes, with SHA-256:

- Initial: `1f6ae19f14fee98f8d5c53a6de85ac5bca3e53f3d3e7bd61a521d2ed19256d08`.
- Fresh: `025bc405ceed8302a21348b34ba20d037cad94c59cb54c1a11c4e3b8ff9e0ea8`.

Independent readers pass all 768 picture planes and all authored stereo PCM in
both files. Ordinary/manual FFmpeg reads retain 205,824/206,848 physical sample
frames; AVFoundation returns 205,005. Comparisons use observed absolute PTS with
no realignment, sample dropping or gain adjustment. The unchanged maximum/RMS
audio bounds are 0.25/0.02; observed maxima stay below 0.064974/0.000283. Retained reader,
library, oracle and canonical-reference hashes are rechecked. Each new published
report binds the exact same document and complete contract to those references.

## Verification and evidence

Host: Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1, pinned LGPL
FFmpeg 8.0.3. Media and visual checks use debug builds; the separate performance
replay uses release. OS cache and power/thermal state are uncontrolled. These
runs do not establish release performance on representative user media.
Commands record their source inventory
and base commit `796b20d6b4da141929fdd47221c38c7fbb6796dd`.

All 2,463 workspace tests and 318 optional UI-feature tests pass, with no failed
or ignored tests. Strict workspace/UI Clippy, formatting and native Metal
startup/shutdown pass. The final visual replay passes 71 Render checks plus the
shortcut audit. Its eight named decision/result captures were inspected against
the workspace and workflow boards.

The full release replay passes 2,418 checks with no findings or failed/timed-out
timing samples. It uses `egui_repaint_callback_v1` waits and
`request_start_finish_publication_receipt_v1` picture timings, without screenshot
readback. The dedicated accepted-generated-picture fixture is not supplied and
that scenario remains explicitly skipped. Warm offscreen picture completion:

| Input | Samples | p50 ms | p95 ms | Maximum ms |
| --- | ---: | ---: | ---: | ---: |
| Frame navigation | 120 | 1.132 | 1.502 | 1.681 |
| Cached Repeat | 40 | 4.945 | 5.698 | 5.954 |
| Hold fallback | 40 | 5.232 | 5.607 | 9.056 |

The one small 120-frame automatic Render completes in 1.656 seconds in that
release replay. This is one development observation, not an export throughput
benchmark or physical-display measurement.

[Retained evidence](../../tools/media-qualification/evidence/2026-09-30-public-render/README.md)
includes exact source inventories and per-check differences, journals, reports,
final captures, consistent SQLite snapshots, movies, independent decoded bytes
and canonical references. Earlier native/public checks retain their own source
identities; later source changes are not retrospectively labelled tested by them.

Earlier failures
are retained: macOS `mkfifoat` unavailability in a new test, two UI-harness build
errors, the modal sizing assertion, the old doctor test that expected Render
to be unknown, and an oversized project-message variant caught by Clippy. The
new commit-preview payload is boxed without enlarging ordinary requests.
Independent review corrected terminal/recovery output fallback,
CLOEXEC output descriptors and zero-valued incompatible status cursors. New tests
cover broken/full output streams, strict requests, writer coexistence and exact
preview receipts, including refresh/admission failure after a durable commit.
