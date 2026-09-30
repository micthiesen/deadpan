# Shared render workflow qualification, 2026-09-30

The [workflow coordinator](../RENDER_JOBS.md#shared-workflow-and-native-ownership)
passes real immutable capture, encoding, verification, publication, cancellation,
checkpoint retry after reopen and publication reconciliation. The native project
service uses the same coordinator and retains its writer through shutdown.
Public Render controls, automatic output policy, full mastering/effects, HDR
and release qualification remain open. No DP requirement or gate is complete.

## Native media evidence

Run on Apple M5 Max, 128 GiB, macOS 26.5.2 build 25F84, Metal, Rust 1.97.1 and
the pinned LGPL FFmpeg prefix. The engineering policy selects hardware encoding
without B frames. These small fixtures do not qualify the full output policy.

| Fixture | Captured range | Picture | Authored audio sample frames |
| --- | --- | --- | --- |
| Structural Source and Background | `[20,128)` | 108 frames, 320×180, 30000/1001 fps | 172,973 |
| Accepted Generated Hold | `[0,30)` | 30 frames, 1920×1080, 30000/1001 fps | 48,048 |

All 21 workflow assertions pass. Two complete encodes produce four published
files: the original publication and a new destination after checkpoint retry for
each fixture. Six fresh verifier attempts cover initial publication, retry and
reconciliation. A separate structural encode is cancelled after observed interior
frame progress; its attempt becomes Cancelled only after explicit cleanup, and
no destination movie appears. The two retries retain the exact original movie
hash and checkpoint, with no second encode.

During the first encode, a framing edit, undo, redo and undo run after frame 1
of 108. Those four writer transactions take 21.763 ms total in this one debug
run. This is concurrency evidence, not a latency percentile. The export keeps
the earlier `durable-live-restored` revision while the editor reaches a fresh
`workflow-live-restored` revision with restored content. All later recovery work
preserves exact authored/history cells; the Generated case preserves them
throughout. Both final stores pass full validation.

Fresh direct Metal I420 and canonical limited PCM references are captured from
the exact historical revisions. Independent FFmpeg and AVFoundation readers
pass 138 pictures, 221,021 authored sample frames, 414 complete image planes
and 102,643,200 image codes. Comparisons include actual packet/GOP clocks,
ordinary/manual AAC decoding and unmodified absolute PCM coordinates. All
169 final artifact admission checks pass. No sample realignment or relaxed
AAC-block tolerance is used.

## Review and failure evidence

Independent source review covered process cleanup, writer ownership, bounded
stage transport, cancellation ordering, late movie commits and native session
changes. It found a destructor joining unfinished pipe readers after its cleanup
deadline. The correction returns explicit uncertainty and leaves the workflow
fenced. A held-reader test releases its fixture after observing failure delivery;
it fails against the old blocking behavior without stranding the fixture.

Controller tests preserve positive movie-commit knowledge through cancellation,
lost channels, unconfirmed cleanup and journal failures. Cancelling or failing
reconciliation leaves publication unresolved until inspection can establish its
state. Native actor tests hold admission of a real capture result and verify
edits, feedback, stale targets, close, switch and shutdown. They cancel before
encoder dispatch and do not establish native UI render throughput.

The first compile found an ambiguous `Command` import in new coordinator tests;
explicit core/worker names corrected it. The first focused render run found a
native shutdown notification failure. A diagnostic reproduction retained
`busy=false`, `stopping=true`, `shutdown_complete=false` and a stale Capturing
update with `cancellation_requested=false`. Shutdown admission now participates
in the service's publication condition. The exact regression passes after the
fix. Original failed logs are retained with subsequent checks.

The final locked workspace invocation passes **2,347 tests** across 167 result
groups, including doctests, with zero failures or ignored tests. Strict
workspace/all-target Clippy and formatting pass. The optional `ui-harness` app
configuration passes strict all-target Clippy and 309 tests. The native smoke
run initializes Metal and completes its shutdown callback. An earlier test command
was cancelled while waiting for Clippy's build lock; it ran no tests and is
retained separately.

Release performance replay passes 2,348 checks across the 17 ordinary UI scenarios
and the live Kestrel audit, with no findings or failed/timed-out timing samples.
Warm navigation input-to-picture completion p95 is 1.489 ms over 120 samples;
cached Repeat and silent Hold edits measure 5.712 and 5.816 ms over 40 samples
each. The 10,000-beat navigation CPU p95 is 0.346 ms over 160 samples. These are
small-fixture offscreen measurements without a concurrent render. The separate
Generated-picture UI scenario requires an explicit fixture and is skipped by
this default replay. No new visual baseline or physical-input claim is made.

Exact commands, failed and successful logs, source inventories and the full
performance report are retained with the
[validation record](../../tools/media-qualification/evidence/2026-09-30-render-workflow/validation.json).

The native build's source inventory is
`5b3970b5658e6daf5b39bf773fe5c8e94898800f6656e28b04e9b013c129b8f1`.
The final workspace source inventory is
`fd099921ac67cc84b2e2e9a897a50135b9d9b9ee9859b9e26595d2403f42e403`.
Only five native app files differ, for shutdown notification, test diagnostics
and explicit dormant engineering-API lint expectations. All encoded-render,
process, store and native qualification source is unchanged. The later app
checks cover that correction; media evidence is not attributed to unexecuted
backend changes.

The retained [native artifact inventory](../../tools/media-qualification/evidence/2026-09-30-render-workflow/native-artifact-audit.json)
contains all eight published movie/report files, fresh references, complete
decoder outputs, copied fixture packages and final SQLite backups. Its archive
has 154 files, 219,560,338 uncompressed bytes and 4,117,769 compressed bytes,
SHA-256 `3daea88063295c3c484816783ee535c4ee6c191cc93b7432c2aaec7324c6e96d`.
Every archived file was independently streamed and rehashed against that inventory.

## Scope and reproduction

The executable example is
`crates/deadpan-cli/examples/qualify_render_workflow.rs`. It requires two distinct
scratch packages, a trusted worker executable, a new report path and a new
output directory. Inputs here are coherent SQLite backups plus copied immutable
objects from the preceding publication-recovery qualification. No user project
is modified. Scratch work is `/tmp/deadpan-render-workflow-EtO3Qu1W`.

This run uses orderly reopen. The earlier
[20-case SIGKILL qualification](publication-recovery-2026-09-30.md) covers the
unchanged publication journal; it does not become a new coordinator crash or
physical power-loss claim. Native actor tests and startup checks do not qualify
physical keyboard input, VoiceOver, listening quality or a public Render flow.
Intent admission still validates the bounded full document on the writer.
