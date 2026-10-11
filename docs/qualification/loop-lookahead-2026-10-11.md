# Selection-loop picture decode-ahead, 2026-10-11

Original and Edit selection loops now prepare their return picture before the
audible window wraps. This extends the existing cut/Repeat look-ahead without
changing the device clock, requested picture, source receipt, output generation
or exact-pixel checks. DP-16 remains partial.

## Implementation

The transport captures the loop's exact sample window, including audition
context, and maps its four-second horizon through the same clock as picture
presentation. Original uses measured PTS boundaries, including VFR and audio
lead/tail; it never substitutes the project frame rate. Partial-frame context
includes the pictures containing the first and final included samples.
The last included Original picture also prepares the seam when its first
request arrives, even if a long VFR duration, audio tail or outward frame
enclosure puts the seam beyond four seconds. Picture coalescing otherwise
issues no later request while that picture holds.

`DiscontinuityScan::next_loop` follows picture order inside that window, then a
virtual continuation through its start. An internal cut takes priority over a
later seam. It examines at most one lap ahead, retains the existing per-call
scan bound, skips background pictures and does not visit excluded edit frames.
A loop that holds one picture requires no companion. The existing bounded
companion decodes the next discontinuity and swaps with the serving decoder
when that saves work. Software remains the normal decoder; this does not
enable the optional VideoToolbox path.

Native requests carry the hint only for the matching captured workspace,
revision, picture domain and output generation. Scanner reuse is bound to
those identities and the loop window. New generations, stopped requests,
cancel, clear and shutdown stop old work; a queued proposal cannot let an older
committed request restart it. Crossing a Generated Hold still closes the
Original session and remains a separate performance gap.

## Correctness and checks

- **160 media tests pass**, including cut priority, wrapping, horizon limits,
  changed windows, background at a seam, excluded-frame rejection and a bounded
  one-picture loop.
- **19 transport tests pass**, including loop sample/lap handling and resume,
  partial-frame context, measured VFR at a different project rate and an
  Original with nonzero A/V origin.
  The long-hold regression uses the measured VFR receipt with a deliberately
  slow project rate: the last picture has more than five seconds of outward
  enclosure left, and the test proves the seam is scheduled before coalescing
  suppresses later requests. This test fails without the final-picture rule.
- **41 preview-worker tests pass.** The new real-decoder case runs three laps
  through both Original and Edit, checks all 180 returned ordinals and exact
  pixels at six loop starts against an independent decode, and observes the
  prepared decoder serving both wraps in each domain. The existing Repeat-cut,
  proxy, cancellation, interpretation and proposal tests also pass.
- One existing opt-in large-package seek-storm test was not run. It is distinct
  from the loop measurements below. The focused loop filter additionally
  passes six app tests, overlapping the transport suite.
- The initial test build rejected a signed/unsigned lap assertion in the new
  test. It was corrected before the passing runs; the failed log is retained.
  A later formatting check required wrapping one test assertion, also fixed.

Review was performed by the sole implementing agent, as requested. No live
GUI, physical display latency or listening check is claimed. The existing
owner verification list retains the physical/acoustic checks.

## Real-device measurements

The updated `perf playback` accepts `--loop-end EXCLUDED_FRAME`. It preserves a
monotonic lap/picture count when the content coordinate wraps, counts skipped
pictures across seams and retains every seam's requested frame, delivery sample
and decode-plus-Metal cost. The benchmark uses the shared decoder, scanner and
device engine with one picture in flight, without a native window.

The fixtures are the deterministic long-GOP H.264 1080p60 and 4K30 clips from
the [hardware-decoder qualification](hardware-decode-2026-10-11.md), imported
through `project create-original`. Each comparison auditions the same 2.4-second
Edit range for 30 seconds: `[197,341)` at 60 fps and `[107,179)` at 30 fps.
`--pictures original` excludes proxy assistance; `--lookahead off` supplies the
negative control. Monitor gain is 0.05. Runs start below load 4 without a
concurrent build or other qualification task.

Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), AC power and no reported thermal
or performance warning. Rust 1.97.1 release and the pinned LGPL FFmpeg 8.0.3
prefix; exact dylib pins and linkage are retained. Initial load was 2.14–2.89.

| Source | Look-ahead | Presented | Dropped | Slowest loop return | All-picture p95 / max | Peak footprint |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 1080p60 | off | 1,783 | 17 | 55.392 ms | 6.799 / 55.392 ms | 546 MB |
| 1080p60 | on | 1,801 | **0** | **6.723 ms** | 6.768 / 7.610 ms | 650 MB |
| 4K30 | off | 889 | 12 | 85.060 ms | 16.069 / 85.060 ms | 1,233 MB |
| 4K30 | on | 900 | **0** | **15.825 ms** | 16.101 / 21.351 ms | 1,775 MB |

All four runs cover the requested interval and twelve loop returns, with zero
audio underruns, device faults or silent-padding frames. Both enabled runs
serve all twelve returns through a decoder swap, with one companion opened,
thirteen targets reached and no failed positioning. The additional peak
footprint is approximately 104 MB at 1080p and 542 MB at 4K. These measurements
qualify the shared Edit-loop path; Original's clock/window mapping and native
worker swaps are established by the tests above.

Executed benchmark SHA-256:
`0866e0807446d1749293a341a4cb387315de39dbfb11a30f441ab69cbe2397ba`.
The retained importer is the previously qualified `8aa000be…` CLI. The
benchmark was built from `458da78e` plus the source hashes in its identity
record. After measurement, review added only the Original final-picture hint
and its regression test; the measured scanner and benchmark source remain
unchanged. Both versions of those two app files and the final source hashes
are retained. Final strict workspace/all-target Clippy and rustfmt pass.

The [evidence manifest](../../tools/media-qualification/evidence/2026-10-11-loop-lookahead/manifest.json)
retains commands, exit codes, complete test logs, every seam measurement,
`time -l` memory, before/after load and power state, source/fixture/library
hashes, scripts and the initial failed checks. Binary and synthetic movies
remain in `/tmp/deadpan-loop-lookahead-20261011` and the preceding hardware
qualification scratch directory; reproducible fixture recipes remain in that
qualification's evidence. No requirement is closed by this follow-up.
