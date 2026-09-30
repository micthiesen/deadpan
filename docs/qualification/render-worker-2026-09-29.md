# Isolated committed picture preparation, 2026-09-29

The [render worker](../RENDER_WORKER.md) now prepares real committed SDR pictures
in a separate supervised process. The host binds the complete authored document
hash and exact output contract, then admits a private raw range only after clean
process/group/pipe teardown and independent hash, geometry and code-range checks.
Generation keeps its existing wire protocol over the extracted shared transport.

This advances the DP-16, DP-17 and DP-18 foundation. It does not produce an encoded
movie, render audio, publish output or expose native Render controls. No
requirement or delivery gate is complete.

## Actual native result

Base commit: `9448ac9f4cf36373c4120071e27ae061ab741fcf`.
Host: Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1, wgpu 30.0.1
and the selected pinned LGPL FFmpeg 8.0.3 prefix. Power, thermal state, other
host work and caches were uncontrolled; elapsed times are observations, not
performance qualification.

The release example passes **118 direct checks** and **128 isolated-worker
checks**. It retains the original 52 direct frames plus all **82 isolated frames**
and their separately prepared direct comparisons:

| Captured case | Project range | Complete isolated frames |
| --- | --- | ---: |
| Retimed/repeated Original and Repeat gap | [20,63) | 43 |
| Last repeated Source, captured Freeze and Background | [121,128) | 7 |
| Odd authored canvas | [20,21) | 1 |
| Accepted Generated Hold | [0,30) | 30 |
| Fresh request after cancellation | [20,21) | 1 |

Every isolated frame matches the direct producer byte for byte and retains the
same exact output clock. The odd canvas remains 319×179 while output is 318×178.
The nonzero Original range retains 43 frames, a 43043-tick endpoint at time base
1/30000, and absolute audio boundaries [32032,100901).

The odd Original and every Generated frame also pass the existing independent
f64 complete-plane reference, within the unchanged one-code tolerance. That is
**93,396,906 compared values** for the isolated path, with none beyond tolerance.
The reference independently implements transfer, interpolation, output matrix
and chroma filtering; it shares the canvas geometry implementation. Both raw
paths and all references are retained for an independent archive audit.

The first Original progress report was 1 of 43 frames. Its callback committed a
new Source pose, undid it and redid it before the host admitted completion.
The worker retained the captured revision; a separately opened live revision
produced different pixels. The accepted Generated snapshot and history stayed
unchanged.

Cancellation was requested after the actual Generated worker reported 1 of 30
frames. It returned Cancelled without an admitted range. A new process then
successfully prepared the Original frame. This measures a real cooperative
cancellation path, not hard-timeout latency or every native hang condition.

Cargo JSON records select both exact executables. Worker and example SHA-256
values are unchanged before and after execution. A 360-second external timeout
encloses the example's 300-second cooperative deadline; the run finished in
8.64 seconds including its launcher.

## Checks and review

The full locked workspace passes **2,140 tests** across 157 result records,
with zero failures or ignored tests. Strict workspace/all-target Clippy and
formatting pass. The 34 additional tests cover the separate protocol, exact
contract/document binding, control shutdown, retained snapshots and independent
transport adapter, including five integration tests with 14 subprocess cases.

The real CLI rejects a changed document hash and an internally valid but wrong
canvas contract before GPU/output creation. Host fault fixtures exercise
malformed/truncated messages, stale attempts, unclean or post-terminal completion,
specific Failed diagnostics, truncated or hash-mismatched files, illegal pixel
codes and snapshot lifetime after workspace/package removal. Fixture-only
Python emits protocol faults and known bytes; it provides no rendering evidence.
The actual Metal run above supplies that evidence.

The macOS process-launch witness passes both its deliberately unguarded
descriptor-inheritance case and the production serialized-launch case. Group
cleanup is confirmed for every probe command. Existing process chaos tests also
pass after transport extraction. These cooperative launch/group guarantees are
not an OS sandbox and do not contain escaped process groups.

Independent reviews covered transport extraction, host/protocol admission,
child control and qualification assertions. They found and prompted fixes for:

- A specific child failure being replaced by its expected unsuccessful exit.
- Preflight/artifact-copy interruptions losing the public cancellation/deadline
  outcome.
- An interrupted control read being mistaken for proof that queued bytes had
  drained.

Later boundary and qualification reviews found no actionable defect. The
initial compile also caught a diagnostic type without Display; the host now
uses its string accessor. The failure log and earlier review findings remain
retained alongside the passing results.

All final checks, release artifacts and native probes share source inventory
`1c9362b007ffaa8738374a5605ab53044382640abf2af0d4a56d2dfb1c504e3e`.
Only documentation and archived evidence changed afterward. The
[evidence bundle](../../tools/media-qualification/evidence/2026-09-29-render-worker/README.md)
retains terminal journals, source inventories, binary identities, actual planes,
comparisons and reviews.

## Remaining work

The raw range has explicit 512 MiB/100,000-frame bounds. Product encoding must
consume these frames within the child instead of spooling full uncompressed
movies. Durable render jobs, restart recovery, bounded priority scheduling,
complete audio/effects, native H.264/AAC and the approved timing metadata,
independent emitted-file verification, atomic publication and the native
workflow remain required. HDR, full-size stress/performance and release matrices
are also unqualified.

No widget, keybinding, focus path or native window behavior changed. Unchanged
UI replay, physical keyboard/IME and VoiceOver checks were not rerun. This
increment supplies no evidence that the complete editor or export workflow is
usable or release-ready.
