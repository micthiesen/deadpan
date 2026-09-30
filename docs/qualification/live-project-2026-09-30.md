# Open-project command qualification

Headless structural edits, undo/redo, primary geometry and automatic Render now
route to the native app holding the project writer. The endpoint binds the actual
package and writable open. Closing that owner revokes discovery and pending work;
an uncertain reply cannot trigger local fallback or command replay. Durable edit
receipts remain separate from native UI feedback. See [the contract](../LIVE_PROJECT.md).

This qualifies the bounded development paths below. Original retention,
relinking, registration and checkpoints still require a closed project through
the CLI. Native persisted-job recovery, full mastering, HDR, representative media
coverage and release qualification remain open. All DP requirements and gates
retain their existing status.

## Native owner and public commands

A scratch native app opened a consistent copy of the retained synthetic project
and held its real writer lock throughout 36 recorded CLI invocations. The fixture
starts at 320×180, 30000/1001 fps, 128 pictures and 205,005 authored audio sample
frames. The script verifies the lock before each mutation and never reads the
discovery secret. It covers:

- A visible rename, stale edit and stale Render refusal, and exact undo/redo
  content with fresh revisions. Access through a symlink reaches the same owner.
- Real automatic Render through that alias, followed by a separate edit growing
  the Freeze Hold from 3 to 33 frames. A read-only durable status captured after
  that commit still reports the original exact attempt as `queued`. Native UI
  inspection shows the 158-frame edit while export retains its captured
  128-frame revision. Undo restores the editor's previous content.
- Rejection of an incorrect cancellation token, then cancellation from a second
  real CLI process. Both the original observer and separate canceller receive
  the same terminal cancelled target with cleanup confirmed. No movie is
  published and authored state is unchanged.
- Historical checkpoint retry and publication reconciliation after the editor
  revision has changed. Both preserve the original intent and movie hash, never
  enter qualification or encoding, and leave the current document unchanged.

The native app displayed the remotely renamed Hold and the concurrent duration
change. Normal Cmd-Q removed discovery and released the writer lock; the parent
then verified the exact current document. These checks used debug binary SHA-256
`c02f095bd18514db60eb05c679fd4b3b30b74c0e8a28f92fba8844776efb68b7` in an
isolated scratch bundle. This bundle is a test application, not release packaging.

## Temporary previews and native observations

Actual Camera, Gain and Room tone drafts each caused a separate headless Render
to fail with `RenderPreviewDecisionRequired`. Each refusal left the complete
document and render job, attempt, publication and decision tables unchanged,
emitted no progress events and created no movie. The Camera draft changed Center
X from 50 to 48.6%; Gain proposed 1.5 dB; Room tone showed In 0 and exclusive Out
4804 at 48 kHz for the selected three-frame Hold. Escape cancelled each draft.

During Room tone exploration the operator repeated command entry and applied a
draft, then used Undo. The retained investigation proves exact content restoration
with a fresh revision before Render. The subsequent draft and refusal check
passed. This history is retained rather than presented as a clean first attempt.

No automatic native Render status window appeared for CLI-started work. The
historical recovery finished before native inspection; Cmd-E then opened the
normal save sheet, which was cancelled. CLI progress and terminal outcomes are
fully retained, but live native progress visibility was not qualified. Native
persisted-job recovery and better access to externally started jobs remain open.

CUA exposed images inline but no file-save API. The Room tone modal was visibly
painted, while its controls were beyond the truncated accessibility result;
native accessibility exposure is unproven. This run does not qualify VoiceOver,
IME, non-US layouts, physical display color or fully keyboard-only save behavior.

## Emitted files and independent readers

The initial Render captured revision
`45c3e0f8-f634-4d26-8c74-ff3add325dd6`, document SHA-256
`aff6df4728a7882084dd4b26e598017d336c6b2b9f29307dd75c3727a2b54c05`.
The initial and checkpoint-retry movies are each 32,553 bytes with SHA-256
`d91af1cef5b01d5f15a4d5bf84e535c2017771fc83d21d71b7fcf0dcb0ceceb8`.
Fresh publication reports bind that document and the complete exact contract.

Independent readers pass all 768 picture planes and all 205,005 authored stereo
sample frames in both movies. Ordinary/manual FFmpeg reads retain
205,824/206,848 physical sample frames; AVFoundation returns 205,005. Comparisons
use observed absolute PTS without sample realignment, dropping or gain changes.
The unchanged maximum/RMS audio bounds are 0.25/0.02; observed maxima are below
0.064974/0.000283. Reader, library, oracle, movie and canonical-reference hashes
are checked before and after decoding.

The first decoder admission failed before running any reader: it compared the
pretty CLI document dump with the renderer's compact JSON identity. The corrected
script removes only JSON whitespace outside strings, preserves all other bytes
and matches both compact hashes exactly. Exact document comparison permits only
the new revision and the known Freeze label. Every other field matches the
retained canonical reference. The failed report, lexical admission proof and
passing decode report are all preserved; no media tolerance changed.

## Automated verification and visual review

Host: Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1 and pinned LGPL
FFmpeg 8.0.3. Journals record base commit
`37abb39f0cf99c2feb19ba7e5e06b1e81f965bf2` and exact source inventories. Final
runtime source inventory SHA-256 is
`2607b02b21a06fdef281192cfbb0e7669b64d37d4c3fb7bec6be7cec8bd428ba`.

The full workspace invocation completed with 2,518 passing tests and four
failures, with none ignored. One failure exposed Serde accepting unknown fields
on tagged unit variants; three expected the old bare-writer refusal codes. Strict
empty payload parsing and a regression test fix the first. The other assertions
now require the intended owner-unavailable errors while preserving no-mutation
checks. All four affected integration targets then pass 39 tests. After lint
corrections, 207 CLI library tests and nine live-project integration tests pass.
Combined coverage is 2,523 distinct passing workspace tests across those recorded
runs; this is not a clean full-workspace rerun. Per-command source differences
are retained in the evidence.

All 330 optional UI-feature tests pass. Strict workspace and optional UI Clippy,
formatting, debug/release UI builds and native Metal startup/shutdown pass. The
debug build retains the linker warning that `__eh_frame` exceeds 16 MB for compact
unwind offsets and may affect exception-handling performance. No suppression was
added. Earlier build and lint failures remain in the journals.

Independent review corrected reply-allocation accounting, compact durable
receipts, unread native completion protection, alias capture, nonblocking accept
under spawn-guard contention, and separate-canceller observation retention.
Focused tests cover those behaviors, owner revocation, strict schemas,
authentication failures, bounded frames/deadlines, descriptor inheritance and
stale native preview decisions. The private capability assumes trusted code
under the same UID. This run adds no endpoint crash/power-loss matrix, discovery
publication fault injection or exhaustive fragmented-body teardown coverage.

The final production visual replay passes 79 Render checks plus the Kestrel
audit against live shortcut-source digest
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
Eight actual Metal captures were inspected at 960×640 and 1280×820 against the
workspace/workflow hierarchy. All preview choices, result revisions and wrapped
final paths are readable. Minimum-size secondary inspector controls require
scrolling. The top-bar Saved label refers to the committed document; temporary
Gain/Room tone state is identified by its separate draft/decision UI.

The full release replay passes 2,426 checks with no findings or failed/timed-out
timing samples. It uses repaint-callback waits and published picture receipts,
without screenshot readback. The accepted-generated-picture scenario requires a
separate fixture and remains explicitly skipped. Warm offscreen measurements:

| Input | Samples | p50 ms | p95 ms | Maximum ms |
| --- | ---: | ---: | ---: | ---: |
| Frame navigation | 120 | 1.266 | 1.557 | 1.814 |
| Cached Repeat | 40 | 5.253 | 5.563 | 6.176 |
| Hold fallback | 40 | 5.199 | 5.769 | 9.363 |

One small 120-frame Render completed in 1.626 seconds in that release replay.
These observations do not establish representative export throughput or physical
display latency. OS caches and power/thermal conditions are uncontrolled.

[Retained evidence](../../tools/media-qualification/evidence/2026-09-30-live-project/README.md)
includes original failures, scoped corrections, source inventories, native
observations, command receipts, selected captures, movies, independent decoded
bytes, canonical references and consistent SQLite backups. Discovery secrets,
runtime sockets and direct copies of live SQLite main files are excluded.
