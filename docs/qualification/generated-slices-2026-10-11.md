# Generated preview slice threading, 2026-10-11

Accepted Generated preview uses up to four FFV1 slice threads. The four real
accepted masters measure 6.2–6.5 ms sequential decode p95 with one thread and
about 2.0 ms with four. Eight through sixteen threads provide no useful gain.
All 5,220 pixel comparisons against single-threaded references pass. Five
ordinary 1080p60 app runs, a 4K30 run and both 30-lap loops show every picture.
A longer run exposes a separate terminal Original-picture handoff failure;
DP-16 remains partial.

## Problem and implementation

The [selection-loop qualification](generated-loops-2026-10-11.md) retained an
intermittent ordinary-playback failure: Edit 63 was skipped early in the first
Generated Hold. Its preceding two pictures spent 10.99/8.64 ms on the worker
and 20.54/18.59 ms from request to composition. An old-binary control and a
repeat of the fixed-loop binary passed, so that record did not establish a
regression caused by Original preparation. Single-threaded Generated decoding
left little margin for asynchronous UI submission at 60 fps.

The native adapter now requests only `FF_THREAD_SLICE` for FFV1 whenever a
caller selects multiple threads. This also enforces the existing FFV1
configuration parser's one-frame allocation assumption for arbitrary Originals;
the previous general setting allowed frame threading. FFmpeg 8.0.3 allocates
the configuration's slice contexts once and dispatches those slices through
its slice executor. Additional threads do not multiply the admitted FFV1
contexts or create a frame pipeline. Other codecs keep their existing policy.
`DecodeWork` exposes the actual codec thread count and active frame/slice modes.

`open_generated_picture_for_preview` applies the four-thread cap (or the lower
available CPU count) through the same complete accepted-media admission. It
keeps all six-object, asset, color, index, authority and cancellation checks.
Index measurement and offline `open_generated_picture` remain single-threaded.
The admission drops its measuring decoder before opening the serving decoder;
using the existing delayed `set_serving_threads` instead would temporarily keep
both live. The four-decoder playback bound is unchanged. Transport clocks,
picture scheduling, GPU work, proxies, acceptance and authored state do not
change.

## Decoder comparison

Release builds use Rust 1.97.1, the qualified FFmpeg 8.0.3 libraries and the
M5 Max / 128 GiB / macOS 26.5.2 reference Mac. No compiler or inference runs
alongside measurements. The inputs are the unchanged two-Hold 1080p60 and
4K30 fixtures from the previous qualification, with two real accepted
768×320 BGR0 masters each. These masters contain 30 or 15 sampled pictures;
the Original resolution does not enlarge Generated media.

`compare_ffv1_threads` captures a single-threaded RGBA/metadata reference,
then runs three passes at each of 1, 2, 4, 8, 12 and 16 threads. Every pass
compares all pictures and 50 deterministic random seeks. It verifies actual
slice-only configuration, stream metadata, PTS/duration, exact pixels and
unchanged input SHA-256. Timings include decoding and the owned pixel copy,
excluding comparison hashing. File caches are warm. The already decoded
opening picture is excluded from sequential p95.

| Master/report | One-thread sequential p95 ms | Four-thread sequential p95 ms | Four-thread maximum ms | Four-thread seek p95 ms |
| --- | ---: | ---: | ---: | ---: |
| `compare-1080p-0` | 6.193 | 1.964 | 1.997 | 1.969 |
| `compare-1080p-1` | 6.481 | 2.040 | 2.059 | 2.047 |
| `compare-4k-0` | 6.528 | 1.987 | 1.994 | 1.995 |
| `compare-4k-1` | 6.306 | 1.953 | 1.979 | 1.949 |

Native tests verify the actual threading mode at 2, 8 and 16 threads, exact
sequential/reverse-seek pixels and pre-call cancellation with no work or lost
position. All 183 native tests pass. An initial test incorrectly expected
pre-call cancellation to poison the decoder; the corrected assertion checks
the existing no-work cancellation and exact resume contract. Its failed log
is retained.

All 45 ordinary worker tests and seven real-media Generated tests pass. The new
app regression compares threaded preview with the independent single-threaded
offline reader, forward and backward through both accepted masters, and checks
their actual threading modes, stream metadata and complete indexes. It passes
again against the 4K fixture. Strict workspace/all-target Clippy with
`ui-harness` and rustfmt pass.
All 183 native tests also pass with the native C adapters and target C
dependencies instrumented by ASan/UBSan. Rust and the separately built FFmpeg
libraries are not instrumented.

## Application replay

The predetermined series retains every result: five ordinary 1080p60 runs,
one ordinary 4K30 run, both 30-lap loops and the existing 30.5-second 1080p60
fixture. These use the release app, real device delivery, 5% monitor gain,
proxies disabled and complete offscreen Metal submissions at 1280×820 points,
1× scale, with a requested 120 Hz cadence. Each opens a private portable copy;
no compiler or inference runs alongside playback.

| Run | Presented / expected | Composition p95 / max ms | Whole-replay peak MiB |
| --- | ---: | ---: | ---: |
| Five ordinary 1080p60 runs | 180 / 180 each | 14.432–14.681 / 15.355–22.206 | 768.0–781.8 |
| Ordinary 4K30 | 90 / 90 | 27.458 / 42.615 | 1890.5 |
| 1080p60, 30 laps | 1831 / 1831 | 12.868 / 14.216 | 875.6 |
| 4K30, 30 laps | 931 / 931 | 13.037 / 34.698 | 2143.7 |
| Long 1080p60 | **1829 / 1830** | 13.978 / 21.018 | 860.9 |

The eight successful runs present all 3,752 pictures. In the five ordinary
1080p runs, Edit 61/62 now spend 3.618–3.919 / 3.011–3.345 ms on the worker,
versus 10.995/8.644 ms in the retained failed control. All Generated pictures
are presented in every run. Full-playthrough Generated worker p95 still ranges
from 5.8 to 7.3 ms in the short ordinary fixtures; the standalone 2 ms result
must not be substituted for whole-app latency. The two loops measure Generated
worker p95 of 2.919/3.289 ms. Sampled clock lag is at most one frame in all nine
runs; this is not a physical/acoustic measurement.

No run reports an underrun, device fault or proxy picture. The callback silence
counter is 1,408 frames in each short ordinary run, 1,344 in the long run and
zero in the loops. That counter includes nonplaying/terminal callback fill;
it is distinct from the starvation counter. It is retained without claiming
that aggregate silence proves acoustic timing.

The long run misses only Edit 1829, its final Original picture. Its playback
request arrives with 236 samples (4.917 ms) left before the endpoint; decode
takes 5.781 ms. The reply is received after 10.283 ms and immediately superseded
by the End handler's stopped request. That stopped request does display the
correct final picture, but it does not satisfy during-playback coverage. The
failure remains open; this qualification does not convert it to a pass or
establish that slice threading caused it. `terminal.json` retains the exact
clock, events and final stopped identity. No repeated long run was used to
select a passing result.

Executed app SHA-256:
`e7ff8d67bfb4a004eb5c00f63e23725886141f5edd94e7e2bc9183d7abe6034f`.

## Scope

The native comparisons establish unchanged pixels and timing semantics for
these admitted FFV1 streams. They do not qualify larger Generated masters,
physical display latency, acoustic synchronization, native-window presentation,
other hardware, the remaining format/color matrix or the full effects/mastering
pipeline. No additional agent was used.

The [evidence manifest](../../tools/media-qualification/evidence/2026-10-11-generated-slices/manifest.json)
retains source/executable identities, all commands and results, source snapshots,
fixture pins, complete decoder reports and application traces. Scratch media,
binaries and sanitizer outputs remain under `/tmp/deadpan-generated-slices-20261011`.
