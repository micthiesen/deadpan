# Preparing Generated playback entry, 2026-10-11

The main preview worker now prepares the next accepted Generated Hold before
playback reaches it. This follows the [decoder reuse change](generated-playback-cache-2026-10-11.md),
which preserves the Original and its companion across the Hold. DP-16 remains
partial pending full-resolution playback measurements and its other obligations.

## Implementation

After publishing an Original picture, the worker scans the immutable Edit plan
within the four-second horizon or captured selection-loop window. It checks at
most 1,024 new frames per call and caches examined frames. An interior Generated
entry takes priority over a later loop seam. Wrapped scanning stays inside the
half-open selection and visits at most one lap. The scan is bound to project
session, revision, output generation and loop bounds.

One background job uses the ordinary strict Generated admission: all retained
objects are verified, the sampled master is measured, and its first requested
picture is decoded. The job owns the sole Generated decoder until the cache
takes it. A different parked Generated decoder is released before starting a
replacement. The bound remains one Original, its one companion, and one
Generated decoder, including an admission still in flight.

At entry, the complete media key must match. The foreground reader rechecks
handle liveness and its normal media authority. If playback catches an unfinished
admission, it waits for that same job while observing foreground cancellation.
A failed preparation is retried through the ordinary reader on actual demand,
which reports any failure. Stopped requests, cancellation, clear, shutdown and
generation changes stop preparation. Teardown stays on the worker outside the
mailbox lock.

## Verification

The [previous qualification's accepted fixture](generated-playback-cache-2026-10-11.md#verification)
is reused through portable copies. It has a real registered Original, actual
conditioning, durable attempts, host bundle qualification and explicit
acceptance; only generation itself uses the synthetic worker.

- **Four real-media tests pass.** The new entry test waits for preparation,
  adopts the already admitted decoder before calling the picture reader, proves
  its first picture has `DecodePlan::Current`, and compares the entire returned
  pixel buffer with an independent cold read. The other new test rejects both
  a cancelled completed preparation and one from another project session, then
  verifies recovery through ordinary admission.
- The retained loop tests still verify all 90 pictures against the independent
  reader, three Generated entries, both prepared Original returns, repeated
  decoder reuse and stopped cleanup. The actual preview-worker test now
  exercises background Generated preparation too.
- **44 ordinary preview-worker tests pass**, including three new scanner tests
  for cut priority, excluded frames, wrapping, bounded incremental scanning,
  horizon changes and backward navigation. Mailbox tests also verify that a
  stopped request cancels Generated preparation.
- **60 media unit tests**, strict workspace/all-target Clippy and rustfmt pass.
  The ordinary worker run skips four fixture-dependent tests, all run explicitly
  above, and the existing large-package seek-storm test, which was not rerun.
- The first Clippy run rejected a complex scope tuple. A named `ScanScope`
  replaced it; the failed log and passing follow-up are retained.

The sole implementing agent reviewed the full change, bounds, cancellation,
media authority and teardown, as requested. No native window, device playback,
GPU timing or listening check was performed for this change.

## Small-fixture observations

On the M5 Max / 128 GiB / macOS 26.5.2 reference Mac, using release Rust 1.97.1
and the qualified FFmpeg 8.0.3 prefix, prepared entry took **0.220 ms** in the
first passing run and **0.169 ms** in the separate resource run. In the same
runs, the direct test that intentionally omits preparation spent 19.743 ms
and 15.908 ms on first Generated admission. These observations use a 320×180
Original with small synthetic Generated media and an explicit wait for the
preparation to finish; they establish the avoided admission work, not a
full-resolution realtime deadline.

The resource run passes all four tests in 1.60 seconds, with 101.9 MB maximum
RSS and 77.0 MB peak footprint. Those are whole-test values, including fixture
copies and reference readers. Actual full-resolution playback through this
worker remains owed. The CLI `perf playback` example uses a separate picture
reader and cannot establish this app-owned preparation path by itself.

The [evidence manifest](../../tools/media-qualification/evidence/2026-10-11-generated-prefetch/manifest.json)
retains source and executable hashes, commands, logs, review, resource use and
fixture/library pins. The copied executable is in
`/tmp/deadpan-generated-prefetch-20261011`; the accepted fixture remains in
`/tmp/deadpan-generated-lookahead-20261011/conditioned.deadpan`.

Executed release-test SHA-256:
`2dd204945b3a7a11ce641627ae5217c1563e87ea8c827bbfc6ab5a24523e5a3f`.
