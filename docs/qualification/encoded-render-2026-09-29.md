# Committed SDR encoding qualification

The [encoded render worker](../ENCODED_RENDER.md) now produces real H.264/AAC
MP4 candidates from one committed project revision and range. Picture and audio
readers independently bind the complete document before native work. The host
admits private candidate bytes only after clean worker teardown and independent
hashing. Production file verification and publication remain required.

## Actual files and independent observations

Measured on Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1 and the
pinned LGPL-only FFmpeg 8.0.3 build. The retained build and command records bind
the executed worker/example, native libraries, headers, compiler and sources.

Seven completed candidates contain 304 pictures and 390,695 authored stereo
sample frames:

| Case | Pictures | Audio samples | Captured behavior |
| --- | ---: | ---: | --- |
| Structural | 108 | 172,973 | Original, Repeat, Hold and background through live edit/undo/redo |
| Nonzero range | 1 | 1,601 | `[1,2)` at 30000/1001 retains absolute sample rounding phase |
| Explicit software | 43 | 68,869 | Requested two B-frames, measured one consecutive B-frame |
| Odd canvas | 1 | 1,602 | Authored 319x179 canvas maps to legal 318x178 output |
| Markers | 120 | 96,000 | 60 fps visible ordinals and independently known audio events |
| Accepted Generated | 30 | 48,048 | Retained schema-3 master at 1920x1080, without its model |
| After cancellation | 1 | 1,602 | A new attempt completes after cancellation and byte exhaustion |

All cases except the explicit software attempt use hardware without B-frames.
The earlier hardware B-frame rejection remains retained in
[native encoder qualification](native-encoding-2026-09-29.md); this integration
does not change that policy or add automatic fallback.

Both normal and ASan/UBSan independent readers pass all seven files:

- Every decoded picture has its exact CFR PTS, duration, endpoint, raster and
  SDR interpretation. All 912 complete planes, containing 116,984,106 codes,
  meet the fixed lossy bounds: maximum 48 codes, mean absolute error 1.5 and
  mean squared error 16 per plane. Observed maxima are 43, 0.823421 and 1.070087.
  The marker movie also retains all visible ordinals 0 through 119.
- Twenty-one actual GOP boundaries each start a fresh decoder. All 855 decoded
  suffix pictures match the linear pass's hash, PTS and duration.
- Ordinary FFmpeg, manual-skip FFmpeg and AVFoundation decode every authored
  sample at its observed absolute PTS. Fixed comparison bounds are maximum
  error 0.25 and RMS error 0.02. Observed maxima are 0.105701 and 0.000981.
  Completely silent references remain exactly silent. No audio alignment,
  gain adjustment, event-based crop or packet dropping is used.
- The marker's three events at samples 100, 48,000 and 95,800 remain exact in
  both channels of canonical input and all three readers. All 18 encoded-reader
  event coordinates have zero sample error. Exact movie/track clocks, zero
  presented starts, normal-rate edit lists, measured priming/reordering and
  fast-start order pass separately. Audio and picture endpoints preserve their
  distinct exact durations for nonzero-origin ranges.

The actual Metal run also passes the existing 118 direct and 128 raw-worker
checks plus 17 encoded checks. Cancellation after real partial progress and
native byte-budget exhaustion return no candidate; a subsequent attempt succeeds.

The sanitizer run instruments the independent C and Objective-C readers, with
leak detection disabled. It does not instrument this child, Rust or the separately
built FFmpeg libraries. The unchanged encoder C adapter has its own retained
normal/sanitized qualification. Neither result proves driver allocation bounds.

## Review, checks and retained failures

Independent reviews cover protocol/wiring, child, host and qualification
predicates. Review fixed conflicting SAR admission, unbounded fixture-input
reads and a progress-fault test that could fail merely for missing completion.
Missing codec/frame SAR can use an explicit square stream SAR; any present
contradictory declaration fails. Progress-fault fixtures now continue to a valid
completion, so the host must reject the earlier fault before artifact access.

The locked workspace passes 2,182 tests, with zero failed or ignored tests.
The strengthened seven integration tests pass separately. Strict workspace and
all-target Clippy, formatting and all 123 Python tests pass. Source inventories
identify the tested implementation and subsequent fixture-only correction.

Retained initial failures include two test-only Clippy allocation findings, an
overstrict codec-SAR oracle rejecting missing metadata despite valid stream SAR,
and a missing `diagnostic` field in the result aggregator. All were corrected
before final admission. Original reports and logs remain available alongside
the passing observations.

## Evidence and remaining work

[Retained evidence](../../tools/media-qualification/evidence/2026-09-29-encoded-render/)
includes actual MP4 candidates, complete reference/decoded planes and PCM,
SQLite backup snapshots of fixture packages, initial and final reader reports,
command journals, binary/source identities and review dispositions. The archive
manifest and audit verify every retained member's size and SHA-256.

This is bounded development-fixture evidence. The plane/PCM tolerances are not
a qualified acceptance policy for arbitrary user content. Independent isolated
production verification, complete source/provenance inventory, durable jobs and
recovery, destination-side partial-file publication and native Render remain
open. Full mastering/effects, HDR, acoustic listening, mid-native drain/fast-start
cancellation, disk-full/short-write faults, 4K/8K performance and release hardware
and OS coverage also remain unqualified. No GUI change required a visual replay.
All DP requirements and Gates A through G remain open or partial.
