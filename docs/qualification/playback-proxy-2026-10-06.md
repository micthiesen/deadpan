# Playback through the preview proxy, resumable proxies and stress runs, 2026-10-06

DP-16 follow-up to [proxy-2026-10-05](proxy-2026-10-05.md) and
[pause-and-cold-seek-2026-10-06](pause-and-cold-seek-2026-10-06.md). This
record covers:

- [proxy pictures during playback](../PROXIES.md#playback-pictures), with
  incremental repositioning of the Original decoder between pictures;
- [resumable proxy builds](../PROXIES.md#resuming) by completed ranges;
- the [preview/export comparison](preview-export-2026-10-06.md) extended with
  accepted Generated Holds and larger media;
- stress runs: 10,000-beat playback, seek storms and jumpy playback through
  the real preview worker, and a proxy build, playback and export together;
- the legacy Accepted/Still reader decision;
- [decode-ahead at cuts](#decode-ahead-at-cuts-follow-up-2026-10-06), a
  later follow-up removing the 1080p60 cut-edit drops.

These are engineering measurements on one machine, not release
qualification. Acoustic synchronization and physical display latency are not
measured here (To verify (owner)).

## Environment and identity

| Item | Value |
| --- | --- |
| Hardware | Apple M5 Max (Mac17,7), 18 cores, 128 GiB, macOS 26.5.2 (25F84), AC power, no thermal or performance warning, Low Power Mode off |
| Toolchain | rustc 1.97.1, release, FFmpeg prefix `/private/tmp/deadpan-ui-ffmpeg/prefix` |
| Source | HEAD `8108f964` plus this change and other agents' concurrent uncommitted work in the shared checkout (AI variant retention and comparison, bridge colour, generated joins), tracked diff `23523e41…`. Only the database schema of that work is on these paths |
| Official runs | Playback: `cargo xtask perf --stages playback` (`deadpan-cli` `6cdb3b2c54bb`, `deadpan-media-worker` `5230105e17e4`, `perf` `c511953c5172`, tracked diff `fa444815…`), after the review fixes below. Stress: the earlier `cargo xtask perf --stages playback,stress` (`deadpan-cli` `60a6c0a4ce9c`, `perf` `79108a9aacb5`, tracked diff `23523e41…`), whose playback rows agree with the later run within 1–2 dropped pictures and 6 proxy pictures. Every stage started below the 3.0 load gate (1.6–3.0); none was flagged |
| Fixtures | The movies of the earlier seek and proxy records (x264 `veryfast`, keyframes 250 pictures apart, BT.709 limited): `gen-4k30` 3840×2160 30 fps 60 s, `gen-1080p60` 1920×1080 60 fps 120 s, imported with this build's `project create-original`. Derived: `-cuts` (`perf make-cuts --every 24 --plays 3 --seconds 40`: fragments of 24 pictures over the first 40 s, every second one a three-play Repeat; a backward jump needing a keyframe seek every 2.4 s), `-long` (`perf make-long`: 100 fragments of 6 pictures grouped, repeated 100 times and exploded into 10,001 Original Source beats, 61,200 and 66,600 frames), `large-10000` (10,000 Background Holds, `perf make-large`) |
| 4K proxy | Built by the run in a private cache in 46.5 s (51.2 s beside playback and export); 1920×1080 intra H.264 |

## Playback design in brief

Pictures still follow the device-reported heard clock with one in flight;
only which pixels serve a requested picture can change. Each Original
decoder now reports how it would reach a picture (`DecodePlan`: current,
forward, finishing a reposition, keyframe seek) and `frame` follows that
plan, so forward decoding past dropped pictures no longer seeks. The
playback policy (`deadpan_media::playback_pictures`) keeps moving averages of
the measured forward, per-ordinal seek and proxy costs. A picture is exact
when its predicted cost fits 70% of its period, otherwise the proxy picture
of the same ordinal serves it and the Original decoder repositions toward
the earliest picture it can reach before playback does, decoded one picture
at a time between requests. The native worker and `perf playback --pictures
adaptive` use the same policy and decoder API; the perf harness yields every
2 ms instead of at the next request.

## Playback, 30 s real-device audition

`--pictures original` never uses a proxy; `adaptive` is the native policy.
p50 / p95 / max where three values are given.

| Workload | Policy | Presented | Dropped | Exact / proxy | Decode + Metal ms | Presentation interval ms | Underruns |
| --- | --- | --- | --- | --- | --- | --- | --- |
| gen-1080p60 | original | 1,800 | **0** | 1,800 / 0 | 5.1 / 6.8 / 8.9 | 16.9 / 18.6 / 23.3 | 0 |
| gen-1080p60 | adaptive (no proxy needed) | 1,800 | **0** | 1,800 / 0 | 5.1 / 6.8 / 7.7 | 16.9 / 18.6 / 19.7 | 0 |
| gen-1080p60-cuts | original | 1,781 | **19** | 1,781 / 0 | 4.9 / 6.7 / 59.6 | 16.7 / 18.7 / 71.4 | 0 |
| gen-1080p60-cuts | adaptive (no proxy needed) | 1,781 | **20** | 1,781 / 0 | 5.0 / 6.7 / 62.3 | 16.6 / 18.7 / 70.6 | 0 |
| gen-4k30 | original | 900 | **0** | 900 / 0 | 15.5 / 15.9 / 20.6 | 33.2 / 35.7 / 36.9 | 0 |
| gen-4k30 | adaptive | 901 | **0** | 901 / 0 | 15.5 / 15.8 / 16.2 | 33.1 / 35.7 / 36.8 | 0 |
| gen-4k30-cuts | original | 882 | **19** | 882 / 0 | 15.6 / 16.9 / 149.5 | 33.1 / 36.1 / 167.2 | 0 |
| **gen-4k30-cuts** | **adaptive** | **900** | **0** | **813 / 87** | 15.5 / 16.0 / 48.9 | 33.2 / 36.2 / 71.4 | 0 |

- Every run covered its interval with no leading or trailing missed picture
  beyond the last (at most one trailing frame), no failure and no device
  fault. The xtask target "Playback sustained at source rate" PASSes for
  gen-1080p60, gen-4k30 (both policies) and gen-4k30-cuts adaptive, and
  FAILs for both 1080p60 cuts runs and 4K cuts without the proxy.
- **4K cuts with the proxy:** each jump in the 30 s showed proxy pictures
  for 207 / 272 / 272 ms (p50 / p95 / max over 15 runs measured to the next
  exact picture), at most 8 pictures; 14 of 15 repositions reached their
  target before playback (the other was overtaken and replaced). Proxy
  pictures decoded in 5.9 / 6.9 ms, exact ones in 8.6 / 8.9 ms (decode and
  conversion). The heard clock was never more than one frame ahead of a
  completed picture (versus four without the proxy). The earlier run
  measured 81 proxy pictures, 175 / 273 ms runs and 14 of 16 repositions.
- **Sequential 4K30** stays exact without a proxy: forward decoding fits the
  23.3 ms budget, so the policy never chose the proxy.
- **1080p60 cuts still drop** 19–20 pictures in 30 s, one or two per keyframe
  seek (decode plus Metal up to 60–62 ms against a 16.7 ms period). Proxies
  are built only above 1080p ([eligibility](../PROXIES.md#eligibility-and-recipe)),
  so the policy has no reduced tier there.
- **Stop:** the exact picture of the last playback frame took 15–16 ms (4K)
  and 4.5–6.8 ms (1080p) in the harness; in the app a displayed proxy picture is
  replaced through the stopped picture path (proxy then refinement after a
  seek).
- Peak footprint: 1.15–1.25 GB for 4K playback (both decoders), 0.55 GB at
  1080p60. Playback queues stayed bounded: at most 1 prepared PCM batch, at
  most 750 device packets (4 s), 8,192 prefill frames.
- Start latency (play to first device-reported content): 227–254 ms.

## Stress

| Workload | Presented | Dropped | Exact / proxy | Decode + Metal p95 / max ms | Underruns | Peak footprint |
| --- | --- | --- | --- | --- | --- | --- |
| large-10000 (10,000 Background Holds, 24 fps) | 720 | 0 | 720 Background | 1.4 / 1.7 | 0 | 0.53 GB |
| gen-1080p60-long (10,001 Source beats) | 1,801 | 0 | 1,801 / 0 | 6.8 / 12.0 | 0 | 0.74 GB |
| gen-4k30-long (10,001 Source beats, adaptive) | 900 | 0 | 900 / 0 | 16.4 / 27.1 | 0 | 1.43 GB |
| gen-1080p60-cuts beside an export | 1,786 | 15 | 1,786 / 0 | 5.3 / 52.8 | 0 | 0.58 GB |
| gen-4k30-cuts adaptive beside a fresh 4K proxy build and an export | 900 | 0 | 817 / 83 | 18.5 / 52.1 | 0 | 1.33 GB |

- The 10,000-beat projects open in 0.77–0.80 s (store, plan, verified Original
  snapshot and decoder) and start sounding in 245–269 ms. `make-long` commits
  100 splits, a group, a 100-play Repeat and its explosion in 0.79 s (the
  10,000-beat explosion commit 0.26 s).
- **Concurrent 4K:** the fresh proxy build (51.2 s, 153 MB footprint, worker
  excluded) and the public Render export (430 s, 0.14× real time,
  succeeded and verified) ran during the whole playback, which still dropped
  nothing; proxy runs lasted 204 / 274 ms and exact decodes rose from 8.7 to
  9.7 ms p50. The app itself pauses proxy builds during playback and renders.
  The concurrent 1080p60 export took 434 s (0.28× real time); its build was
  not needed.

### Seek storm and jumpy playback through the real preview worker

`worker::project_tests::stress_tests::seek_storm_and_jumpy_playback_stay_bounded`
(ignored; release; run on package copies with the run's proxy cache). For
10 s it submits a random stopped seek every 4 or 16 ms while taking replies,
then a final seek; then for 10 s it plays at the picture rate with one
request in flight, jumping to a random frame every 0.5 s.

| Package, interval | Storm replies (of submitted) | Reply latency p50 / p95 ms | Final seek: first / exact picture ms | Jumpy playback exact / proxy, dropped | Repositions reached | Queue high | Resident before → high |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 4K, proxy, 4 ms | 0 of 2,500 | (none) | 13 / 326 | 184 / 116, 0 | 20 of 20 | 2 | 684 → 1,115 MB |
| 4K, proxy, 16 ms | 625 of 625 (623 proxy) | 6.1 / 6.1 | 6 / 200 | 189 / 111, 0 | 17 of 17 | 1 | 674 → 763 MB |
| 1080p60, no proxy, 16 ms | 4 of 625 | 12.2 / 15.9 | 21 / 21 | 589 / 0, 11 | 0 | 2 | 156 → 341 MB |

- The preview mailbox never held more than one pending and one active
  request, and resident memory stayed bounded (the 4K growth is the 16-thread
  decoder's frame pool and the proxy decoder).
- When requests arrive faster than any picture decodes (4 ms, or 16 ms
  without a proxy at 1080p60), every request is superseded and cancelled
  before it completes, so the viewer keeps its previous picture until the
  input pauses; then the latest seek shows within 13–21 ms (the proxy at 4K)
  and its exact picture follows the full 150 ms scrubbing rest. Cancelled
  decodes reopen their decoder, and cancelled reopens log FFmpeg "moov atom
  not found" (11 lines at 4 ms); this is the existing cancellation contract,
  not a new failure.
- Jumpy 4K playback dropped nothing: about 38% of pictures came from the
  proxy, each jump followed by exact pictures after a reposition. At 1080p60
  without a proxy the jumps dropped 11 frames in 10 s.

## Resumable proxy builds

Proxy builds now record completed keyframe-aligned ranges (600 pictures, at
most 1,024 ranges) in a private per-Original journal and resume after a
cancellation, a killed worker or a killed host; partial state is never an
entry ([PROXIES](../PROXIES.md#resuming)). `crates/deadpan-cli/tests/proxy_resume.rs`
kills the worker (SIGKILL after a journaled range) and the host (the test
re-executes itself as a host and SIGKILLs it while its worker runs), then
resumes: only the missing ranges are encoded, every published proxy picture is
a keyframe at the Original's exact PTS and duration, and the partial state is
removed. Damaged ranges (flipped byte, torn tail, truncation, torn journal)
are encoded again; journals for other bytes, recipes, encoders, rasters or
range sizes are discarded. Measured on a generated 40 s 4K30 Original (1,200
pictures, load about 7.5): one range 29.9–31.0 s, two ranges 30.2–31.2 s, ten
ranges 31.9 s, cancelled after 600 pictures and resumed 20.6–25.5 s.

## Preview/export comparison

44 recipe fixtures pass ([record](preview-export-2026-10-06.md)), adding four
accepted schema-3 Generated Holds made by the synthetic worker through real
conditioning, bundle qualification, publication and acceptance
(generated-pause, -repeat, -reframe, -prefix: minimum luma PSNR 50.0–52.6 dB,
chroma 58.3 dB, audio offsets 0) and two long-GOP B-pyramid generated
Originals (`large-1080p` 195 frames, 38.3 dB; `large-2160p` 68 frames,
44.4 dB; audio offsets 0). A negative check flags exactly the 12 Generated
frames against the pre-acceptance revision. The HDR set was not extended: its
two recipes already cover the cut, Repeat, freeze and caption paths.

## Legacy Accepted and Still readers

Out of scope, by the owner's 2026-09-30 development-format decision (no users;
breaking changes authorized; obsolete formats refused,
[development formats](../DEVELOPMENT_FORMATS.md)). No V1 workflow creates
either: V1 has no still-image import (specification §16.3), and AI pauses are
accepted only as schema-3 `HoldVideo::Generated` through
`accept_generation_bundle`. Packages from the builds that produced legacy
accepted media are refused by schema before any reader runs. The generic
headless vocabulary (`add_asset` of an unqualified record, then `insert`)
can still author a `SourceVideo::Still` or `HoldVideo::Accepted` provider, and
every picture consumer refuses it explicitly rather than guessing: the
preview worker ("Still-image preview is not yet qualified", "Legacy
accepted-media preview has no qualified generated evidence") and
`ProjectPictureSession`, which export, render and verification use
(`StillUnsupported`, `AcceptedUnsupported`; `crates/deadpan-cli/src/picture/tests.rs`).
A reader would need media evidence these providers never record, so none is
implemented.

## Review

An independent review of the playback and harness changes found the decoder
plans, the reposition loop and the stop path correct, and these issues, all
fixed before the final playback run:

- The harness polled the engine after each picture only for its sample,
  discarding that update; a terminal phase arriving then would have been
  lost. It is now handled at the top of the loop. The stress run predates
  the fix; its device diagnostics (0 faults, 0 starved reports) and covered
  intervals show no terminal update was lost there.
- A pause requested the displayed proxy frame and then its own picture; it
  now requests only its own. The Original view's replacement checks the
  selected video.
- An in-group lead could overflow its integer conversion near a zero
  denominator; it is now bounded by the horizon before conversion.
- A picture whose decoder reopened recorded the reopen as seek cost; it is
  no longer recorded. Failed repositions are counted.
- Each playback variant now opens its own package copy.

Not changed: the reposition lead assumes unity speed (documented), and the
uninterruptible keyframe seek that starts a reposition is not separately
measured.

## Tests and checks

- `deadpan-media`: `playback_pictures` unit tests (10), and
  `tests/source_session.rs` `forward_continuation_and_repositioning_return_sequential_pictures`
  (CFR, B-pyramid, offset-start and VFR fixtures at one and eight threads,
  every picture compared with a sequential single-threaded decode); the
  remaining crate tests pass.
- `deadpan-app` `worker::` tests (36 passed, the stress test ignored),
  including `late_playback_pictures_use_the_proxy_until_the_original_repositions`.
- UI replays (`cargo xtask replays`): `proxy-seek`, `playback-feedback`,
  `original-playback`, `workspace`, `diagnostics`, `jobs`, `delayed-preview`
  and `rapid-input` PASS; after the review fixes `proxy-seek`,
  `playback-feedback`, `original-playback`, `workspace` and `diagnostics`
  PASS again. No replay shows playback proxy pictures: its fixture is below
  the proxy raster.
- `deadpan-cli`: `export_paths_never_reach_proxies`, the `picture` unit tests
  (36) and `golden_renders` (2) pass with the forward-continuation decoder.
- `xtask` tests (39) pass.
- Strict Clippy (`--all-targets -D warnings`) for `deadpan-media`,
  `deadpan-diagnostics`, `deadpan-cli`, `xtask` and `deadpan-app` with and
  without `ui-harness`.

## Decode-ahead at cuts (follow-up, 2026-10-06)

The 1080p60 cut edit above dropped 19–20 pictures per 30 s because each
Repeat restart needed a keyframe seek of the serving decoder and 1080p
Originals have no proxy. Edit playback now decodes ahead of discontinuities
at every raster ([decode-ahead](../PLAYBACK.md#audio-clock-and-pictures),
`deadpan_media::lookahead`): after each picture the plan is scanned up to
4 s ahead for the next picture that does not continue the Original (a
backward jump or a forward jump of more than eight pictures), a companion
decoder of the same Original (shared verified snapshot, receipt index and
background measurement; same codec threads) is positioned at its exact
picture on its own thread, and the two decoders swap roles when playback
reaches it. Only which decoder produces the exact picture changes.

| Item | Value |
| --- | --- |
| Source | HEAD `90b2905b` plus this change and another agent's concurrent uncommitted headless/live-endpoint work (deadpan-cli, app project service, store registers), tracked diff `c9ba512a…`; none of that work is on these paths |
| Run | `cargo xtask perf --stages playback` (`deadpan-cli` `8ff95bdba050`, `deadpan-media-worker` `fc0c03ffd9e1`, `perf` `8b7440d69492`). Every stage started below the 3.0 load gate (2.19–2.95); none was flagged. Hardware and toolchain as above |
| Fixtures | The same `gen-1080p60` and `gen-4k30` movies, imported again with this build (the earlier packages are schema 67, refused by the current store), and the same `-cuts` derivation (`make-cuts --every 24 --plays 3 --seconds 40`) |
| Comparison | `perf playback --lookahead off` on the same cut package: the previous behaviour |

| Workload | Policy | Look-ahead | Presented | Dropped | Exact / proxy | Serving seeks | Decode + Metal p50 / p95 / max ms | Interval p95 / max ms | Peak footprint |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| gen-1080p60 | original | on | 1,801 | 0 | 1,801 / 0 | 0 | 4.6 / 6.3 / 10.9 | 18.5 / 24.8 | 550 MB |
| gen-1080p60 | adaptive | on | 1,800 | 0 | 1,800 / 0 | 0 | 4.6 / 5.9 / 10.7 | 18.5 / 23.7 | 552 MB |
| gen-1080p60-cuts | original | off | 1,786 | **14** | 1,786 / 0 | 37 | 4.6 / 6.3 / 60.0 | 18.6 / 68.8 | 567 MB |
| **gen-1080p60-cuts** | original | **on** | 1,800 | **0** | 1,800 / 0 | 0 | 4.5 / 4.7 / 6.3 | 18.3 / 20.2 | 685 MB |
| **gen-1080p60-cuts** | adaptive | **on** | 1,801 | **0** | 1,801 / 0 | 0 | 4.5 / 4.7 / 6.3 | 18.2 / 19.7 | 697 MB |
| gen-4k30 | original | on | 900 | 0 | 900 / 0 | 0 | 15.8 / 16.2 / 20.4 | 35.5 / 39.1 | 1,172 MB |
| gen-4k30 | adaptive | on | 901 | 0 | 901 / 0 | 0 | 15.7 / 16.2 / 21.2 | 35.9 / 39.4 | 1,148 MB |
| gen-4k30-cuts | original | off | 878 | **22** | 878 / 0 | 18 | 15.7 / 26.2 / 175.6 | 36.2 / 187.7 | 1,343 MB |
| **gen-4k30-cuts** | original | **on** | 900 | **0** | 900 / 0 | 0 | 15.6 / 16.5 / 21.5 | 35.9 / 38.8 | 1,861 MB |
| **gen-4k30-cuts** | adaptive | **on** | 901 | **0** | 901 / 0 | 0 | 15.5 / 16.2 / 22.0 | 35.7 / 39.5 | 1,883 MB |

"Serving seeks" counts exact playback pictures that the serving decoder
reached by a keyframe seek (or more than eight forward pictures): cuts the
look-ahead did not cover. Peak footprint is the whole `perf` process
(`/usr/bin/time -l`).

- **Target met.** Both cut edits drop nothing with the look-ahead, in both
  policies, against 14 (1080p60) and 22 (4K) without it in the same run
  (the earlier run: 19–20 and 19). Decode plus Metal never exceeded 6.3 ms
  at 1080p60 and 22.0 ms at 4K30; the heard clock was never ahead of a
  completed picture (lag 0 at every completion). Audio: 0 underruns,
  0 faults, every interval covered, at most one trailing frame.
- **Every cut was served ahead.** 1080p60: 38 jobs started, 38 reached,
  37 swaps (adaptive: 39 started, one stopped by retargeting, 38 swaps);
  4K: 19 started, 19 reached, 18 swaps in both policies; one companion
  opened per run, no failure. A reached job without a swap positioned the
  companion for a cut the 30 s ended before.
- **The 4K cut edit no longer needs the proxy.** With the look-ahead every
  restart is exact (901 / 0 against 813 / 87 proxy pictures before); the
  proxy remains the fallback for a picture the companion did not reach.
- **Memory.** The companion decoder costs 118 MB at 1080p60 (567 →
  685 MB) and 518 MB at 4K30 (1,343 → 1,861 MB): a second 16-thread 4K
  decoder's frame pool. It exists only during edit playback with a
  discontinuity in the horizon, at most one per retained Original, and is
  closed after the first request that is not playback. Sequential
  playback opens none (the uncut rows equal the earlier record).
- Start latency 205–258 ms and picture session open 94–132 ms, as before.

Tests: `deadpan-media` `lookahead` unit tests (7: continuity rule, scan
reuse, incremental extension within its 1,024-frame bound, background
pass-over, errors, swap preference) and `tests/lookahead.rs` on real media
(`pyramid-bframes.mp4` and `cfr-bframes.mp4` at one and eight threads:
every picture of an edit with backward restarts, forward jumps and a jump
into the last keyframe group equals a sequential single-threaded decode, in
order, each cut served by the companion's current picture; retargeting,
cancel, release and drop while positioning are bounded). `deadpan-app`
`worker::project_tests::lookahead_tests`: a split and Repeated
`pyramid-bframes.mp4` played through the real preview worker returns every
picture in order, the restart (95 → 30) and its neighbours equal an
independent exact decode, the look-ahead swap counter advances; and only
the next playback picture keeps a running job (a stopped request, cancel,
clear and shutdown set its stop flag). All 38 `worker::` tests pass (the
stress test ignored), `deadpan-media`, `deadpan-diagnostics` and `xtask`
tests pass, strict Clippy (`--all-targets -D warnings`) passes for
`deadpan-media`, `deadpan-diagnostics`, `deadpan-cli`, `xtask` and
`deadpan-app` with and without `ui-harness`, and the `workspace`,
`playback-feedback`, `proxy-seek`, `original-playback` and `diagnostics`
replays pass (the `:diagnostics` panel shows the new Look-ahead row).

An independent review found no material defect and three minor issues,
fixed after the run above: a target whose positioning failed was retried
on every picture (now once per target); a stop arriving while the same
target was re-requested waited one picture before restarting; and the
harness scheduled the look-ahead (plan scan and job start) inside the
measured picture time and failed the run on a plan error. The measured
decode plus Metal times above therefore include the scan, a conservative
difference; the worker always scheduled after publishing its reply.
Focused tests and strict Clippy were rerun after the fixes; the perf run
was not.

Not covered by this follow-up: selection-loop seams and Original audition
are not looked ahead (a loop seam still seeks); crossing a Generated Hold
still closes the Original session; the companion uses the serving decoder's
thread count, so 4K memory could be lowered with fewer companion threads at
an unmeasured cost to forward decoding after a swap; the stress stage
(10,000-beat and concurrent proxy/export playback) and the seek-storm
worker stress test were not rerun; lower hardware tiers and memory pressure.

## Not covered

- Acoustic synchronization of pictures with sound and physical display
  latency (To verify (owner)): all picture timings end at Metal completion,
  without the window, compositor or scanout.
- 1080p60 edits with cuts dropped pictures in the runs above; the
  [decode-ahead follow-up](#decode-ahead-at-cuts-follow-up-2026-10-06)
  removes those drops.
- The native app's playback was exercised through the real preview worker
  and replays with simulated delivery, not a physical audition in a native
  window.
- Playback while a proxy is first opened: playback never opens one.
- Lower hardware tiers, memory pressure and page-cache-cold media.
