# Performance qualification, 2026-10-04

Reproducible Section 25 measurements (`cargo xtask perf`, see
[PERFORMANCE.md](../PERFORMANCE.md)) on real and generated media. The UI
replays pass, as do real-device playback and warm seek on the small 640×360
fixture. Seek at 1080p and 4K, cold admission, and edits on real or large
projects fail. These are engineering measurements on one machine, not release
qualification.

The authoritative numbers come from run 3 below. Runs 1 and 2 are retained
only where they add evidence, and each is labeled with its binary identity.

## Environment and identity

| Item | Value |
| --- | --- |
| Hardware | Apple M5 Max (Mac17,7), 18 cores, 128 GiB |
| OS | macOS 26.5.2 (25F84) |
| Power | AC, `powermode 0` (Low Power Mode off). `pmset -g therm` recorded no thermal or performance warning before or after. |
| Toolchain | rustc 1.97.1, release profile, pinned FFmpeg prefix `/private/tmp/deadpan-ui-ffmpeg/prefix` |
| Cache | OS file cache warm. Nothing was purged, because that needs root. |
| Display | Offscreen Metal only: no window, compositor or scanout |

| Run | Source | Binaries (SHA-256 prefix) | Used for |
| --- | --- | --- | --- |
| 3 (2026-10-04 23:33 to 2026-10-05 00:16) | Shared checkout: HEAD `f5201ec6` plus 230 dirty paths (tracked diff SHA-256 `1e45aa46…7787`). This includes this task and the other agent's concurrent core/app work, among it the busy-path fix noted below. | `deadpan-cli` `b3fad4c1…9b4d`, `deadpan-media-worker` `40e05292…61b9`, `perf` `b1e7194c…c236`; UI harness `deadpan-app` `1afdf089…e1d4`, worker `0f48ef58…730a` | Every table unless marked otherwise |
| 1 | HEAD `f5201ec6` plus this task's patch only, from an exported tree | `deadpan-cli` `e836c343…ddb59`, `perf` `4895eba6…f5afb`; UI app unhashed | Release UI replay failure diagnosis; profile and history-open measurements |
| 2 | Same export, corrected pause request | `perf` `efc25aeb…d3d2` | Superseded by run 3; the numbers agree within a few percent |

The run-3 stages waited for a 1-minute load average of 4.0 or below
(`--max-load 4`). No stage was flagged; stages started at load 1.7–3.96. The
other agent compiled intermittently throughout (load reached 9.0 at the end).
The five edit stages started at load 3.5–3.8. The structural edit costs below
match run 2 within a few percent, so they are not load-dominated. Rerunning
them on an idle machine is still owed.

## Fixtures

| Name | Content |
| --- | --- |
| `caminandes` | Copy of the real Caminandes 2 project: 1920×1080 H.264 at 24 fps, 3,507 frames (146 s), AAC; keyframes up to 128 frames apart |
| `interview` | Copy of `~/Documents/Deadpan/interview.deadpan`: 640×360 test pattern at 30 fps, 595 frames, keyframe interval 250 |
| `gen-1080p60` | `testsrc2` plus sine: 1920×1080 at 60 fps, 120 s, x264 `veryfast`, GOP 250, BT.709 limited; generated and imported in run 1 |
| `gen-4k30` | Same recipe, 3840×2160 at 30 fps, 60 s |
| `large-10000` | 10,000 root Background/Silence Holds of 12 frames each; no media |

The suite copies each package while holding a shared lock on its
`.writer.lock`. The originals were never opened writable.

## Results against Section 25.2 (run 3)

PASS and FAIL require a successful stage, a quiet machine and enough samples:
at least 100 warm seeks, 20 committed edits of each kind, or 40 UI samples.
Everything else is INFO.

| Target | Workload | Measured (n) | Result |
| --- | --- | --- | --- |
| Key event to command state, p95 < 8 ms | UI `rapid-input`, release | p95 0.21 ms (120) | PASS |
| Warm seek, UI navigation to picture, p95 < 80 ms | UI `rapid-input`, small fixture | p95 1.53 ms (120) | PASS |
| Cached ordinary edit to visible preview, p95 < 50 ms | UI `edit-latency`, small fixture | p95 6.24 ms (40) | PASS |
| Same, real projects, headless commit plus refresh | `perf edit`, 30 each of split, wrap and Undo per project | p95 54–75 ms. The first tenth of commits has p50 4–5 ms. | FAIL |
| Same, 10,000 beats | `perf edit` on `large-10000` | p95 1.59–3.83 s (30 each) | FAIL |
| Hold fallback visible < 100 ms | UI `edit-latency` | p95 7.22 ms (40) | PASS |
| Hold insertion, real projects, headless | `perf edit` pauses | p95 100–109 ms (30 each, 0 refused) | FAIL |
| Hold insertion, 10,000 beats | `perf edit` pauses | 4 committed, 26 refused (`audio binding work exhausted`) | INFO: too few samples. The refusals are a defect. |
| Warm seek, p95 < 80 ms, real media | `perf seek`: 200 random non-adjacent frames, decode plus Metal completion | interview 71.4 ms PASS. Caminandes 265 ms, 1080p60 410 ms, 4K30 1,862 ms. | PASS 640×360; FAIL 1080p and 4K |
| Cold seek < 300 ms | `perf seek`: 10 new picture sessions | interview max 236 ms; Caminandes 6.9 s; 1080p60 15.9 s; 4K30 15.8 s | INFO: session-cold, page-cache-warm |
| Playback 1080p60 and 4K30 | `perf playback`: 30 s real-device audition, pictures following the heard clock | Every fixture covered its interval with 0 dropped, leading or trailing pictures. Decode plus Metal p95: 9.9 ms (1080p24), 7.2 ms (1080p60), 23.7 ms (4K30, max 29.1 against a 33.3 ms period). | PASS, without window presentation |
| Audio: no callback underruns | Same auditions | 0 starved and 0 faults in 10,311 device reports. Maximum callback render cost 20.5 µs. | PASS for idle audition only. The editing/inference stress suite is undefined. |
| 10,000-beat navigation without a whole-document scan | UI `large-project`; `perf scale` | Navigation CPU p95 0.45 ms (160). Frame lookup p95: 0.21 µs (100 beats), 0.63 µs (10,000), 1.08 µs (50,000). | PASS |
| Idle event-driven UI | none | | Open |
| AI generation | Cited from the [AI pause run](../AI_HOLDS.md#measured-run-2026-10-04) (debug, 640×360) | 103.3 s for a 1 s Hold; worker footprint about 21 GiB | Fails the provisional 2 s draft target; not rerun |
| Export by workload | `deadpan-cli render` | See [export](#export) | INFO; no universal target |

## Seek and frame stepping (run 3)

| Fixture | Warm p50 / p95 / max (ms, n=200) | Seeks ≥ 80 ms | Metal completion p95 | Step p50 / p95 | Sustained step |
| --- | --- | --- | --- | --- | --- |
| interview 640×360 | 31.6 / 71.4 / 74.1 | 0 | 1.0 ms | 0.90 / 0.99 ms | 1,108 fps |
| Caminandes 1080p24 | 107 / 265 / 348 | 128 | 2.2 ms | 6.07 / 10.0 ms | 148 fps |
| gen-1080p60 | 214 / 410 / 433 | 171 | 2.3 ms | 5.57 / 5.88 ms | 179 fps |
| gen-4k30 | 835 / 1,862 / 1,975 | 190 | 11.7 ms | 22.6 / 23.9 ms | 44 fps |

Decode preroll from the preceding keyframe dominates. The decoder cache works
as intended: each warm session reports one cold admission and 441 reuses
(reuse rate 0.998, `ProjectPictureSession::stats()`).

### Cold admission decodes the whole Original

A cold picture session copies the Original into a private snapshot while
checking SHA-256, then builds a fresh index by decoding every frame
(`SourceDecoder::next_metadata`). Only then does it compare that index with
the stored qualification.

- The session stats attribute 6.5 s (Caminandes), 12.3 s (1080p60) and
  13.7–17.4 s (4K30) to that admission.
- Store and plan open in under 20 ms.
- The native preview worker uses the same `SourceSession::open_verified` path
  once per project session, so opening a project pays this cost.

[PROJECT_PICTURES](../PROJECT_PICTURES.md) requires a "complete freshly
measured index", so this is a design decision rather than a local bug.
Options: packet-level verification against the stored index, retaining
verified sessions across reopen, or progress reporting.

## Playback (run 3)

| Fixture | Start to first heard | Heard / coverable | End | Pictures presented / expected | Dropped, leading, trailing | Picture p95 / max | Peak RSS / footprint |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Caminandes | 358 ms | 30.0 / 30.0 s | limit | 721 / 720.2 | 0, 0, 0 | 9.9 / 21.1 ms | 103 / 412 MiB |
| gen-1080p60 | 356 ms | 30.0 / 30.0 s | limit | 1,800 / 1,800.0 | 0, 0, 0 | 7.2 / 8.6 ms | 104 / 413 MiB |
| gen-4k30 | 358 ms | 30.0 / 30.0 s | limit | 901 / 900.7 | 0, 0, 0 | 23.7 / 29.1 ms | 238 / 650 MiB |
| interview | 130 ms | 19.8 / 19.8 s | Ended | 595 / 595.0 | 0, 0, 0 | 3.5 / 4.7 ms | 59 / 339 MiB |

- Start latency runs from `Engine::play` to the first update inside a
  device-reported content interval.
- The interview's 1,344 silent padding frames come after its natural end.
- Monitor gain was 0.05.
- The picture thread is a single-flight, newest-wins headless consumer, not
  the native transport UI.

## Edit latency (run 3)

Each cycle commits a split, a 12-frame native `,h` freeze, a three-play Repeat
wrap and an Undo of that wrap. Each edit is followed by the native refresh
(head snapshot and plan compile). Columns give p50 / p95 over 30 commits and
the p50 of the first and last tenth of commits, in commit order.

| Package | Split | Pause | Wrap | Undo | Refused | Final document / database |
| --- | --- | --- | --- | --- | --- | --- |
| Caminandes | 28.0 / 75.2 ms (first 5.1 → last 75.2) | 38.5 / 109.4 (11.0 → 109.4) | 18.1 / 58.2 (5.2 → 58.2) | 19.2 / 60.3 (3.8 → 60.3) | 0 | 2.30 MB / 198 MB |
| interview | 26.1 / 71.8 | 32.7 / 100.3 (6.0 → 100.3) | 17.7 / 53.9 | 18.7 / 57.6 | 0 | 2.24 MB / 194 MB |
| gen-1080p60 | 21.1 / 71.4 | 41.4 / 107.5 | 20.9 / 54.7 | 17.1 / 57.0 | 0 | 2.24 MB / 192 MB |
| gen-4k30 | 20.0 / 72.2 | 37.6 / 101.3 | 20.8 / 55.4 | 17.1 / 56.3 | 0 | 2.26 MB / 193 MB |
| large-10000 | 1,379 / 1,737 ms | 1,151 / 2,119 ms (4 committed) | 1,051 / 3,834 ms | 1,020 / 1,589 ms | 26 pauses | 33.2 MB / 4.82 GB |

On the real projects, cost climbs steadily in commit order. Without pauses
(run 1, `e836c343`), every small-project edit took 4–7 ms and 10,000-beat
edits took 91–114 ms at p95.

### Pause history grows every later edit

Each committed pause retains a frozen audio timing layout of the whole
structure at that moment (`audio_bindings.timings[].layout.nodes`).

- In a Caminandes copy after 15 cycles, 258 kB of the 351 kB document was 15
  such layouts, growing from 2.3 kB to 32 kB each.
- Every revision stores the full document JSON, and every commit parses,
  validates and serializes it. Storage and edit cost therefore grow as
  O(pauses × beats).
- On 10,000 beats, four pauses produced a 33 MB document and a 4.8 GB
  database. The fifth pause hit `audio binding work exhausted`.

Section 25.3 asks for storage roughly proportional to authored structure, so
this is the largest algorithmic budget miss. It lives in the core audio clock
design, which is under concurrent development, so it is listed rather than
changed here. [Compact timing storage](../TIMING_STORAGE.md) has since
removed this growth; see the
[2026-10-05 record](timing-storage-2026-10-05.md).

### Where a 10,000-beat commit spends its time (run 1 profile)

A `sample` profile of `ProjectStore::commit` on `large-10000` before any
pause (about 85 ms per commit):

| Stage | Share of commit time |
| --- | --- |
| Duration and validation passes over every node | 36.5% |
| Head snapshot JSON parse | 23% |
| SQLite insert of the 3.3 MB document, including `json_valid` | 19% |
| `to_json` | 13% |

The refresh after the commit adds 25 ms. Opening that package read-only took
208 ms when fresh and 6.27 s after 93 revisions (301 MB of revision
documents), because opening validates the full history.

### In-memory scale (`perf scale`, run 3, p50 of 5; lookup p95 of 2,000)

| Beats | Validate | Plan compile | Anchor index | InsertTime target | `from_json` | Frame lookup p95 | JSON |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 100 Holds | 0.04 ms | 0.07 ms | 0.06 ms | 0.04 ms | 0.12 ms | 0.21 µs | 0.03 MB |
| 1,000 Holds | 0.46 ms | 0.86 ms | 0.76 ms | 0.51 ms | 1.34 ms | 0.25 µs | 0.31 MB |
| 10,000 Holds | 5.34 ms | 10.3 ms | 9.23 ms | 6.02 ms | 17.8 ms | 0.63 µs | 3.15 MB |
| 50,000 Holds | 34.9 ms | 65.3 ms | 56.7 ms | 37.7 ms | 82.2 ms | 1.08 µs | 15.8 MB |
| 10,000 Sources | 9.03 ms | 16.2 ms | 11.8 ms | 9.13 ms | 45.5 ms | 0.83 µs | 22.4 MB |

Per-revision work is linear in beats. Per-request picture lookup is
logarithmic.

## Export (run 3)

| Fixture | Content | Wall | Real-time factor | Encoding | Verification | Encode rate | Peak RSS with children |
| --- | --- | --- | --- | --- | --- | --- | --- |
| interview 640×360 | 19.8 s | 9.0 s | 2.21× | 7.7 s | 0.7 s | 80.3 fps | 72 MiB |
| Caminandes 1080p24 | 146.1 s | 293.7 s | 0.50× | 262.7 s | 29.8 s | 13.7 fps | 180 MiB |
| gen-1080p60 | 120.0 s | 444.9 s | 0.27× | 398.3 s | 45.0 s | 18.7 fps | 187 MiB |
| gen-4k30 | 60.0 s | 443.8 s | 0.135× | 383.2 s | 57.7 s | 4.9 fps | 469 MiB |

All four exports were published and verified on the hardware encoder. User
CPU roughly equals wall time (278 of 294 s for Caminandes): the encoding
stage keeps about one core busy, because decode, Metal composition, I420
readback and audio preparation run serially.

## Memory (run 3, `/usr/bin/time -l`)

| Process | Peak RSS / footprint |
| --- | --- |
| Seek | 47–101 MiB RSS at 640×360 to 1080p; 244 MiB at 4K. Footprint (including the Metal device) 333–660 MiB. |
| 10,000-beat edit with pauses | 2.36 / 1.99 GiB |
| Scale up to 50,000 beats | 926 / 513 MiB |
| UI replays | 171–459 / 450–637 MiB |
| Exports | 72–469 MiB RSS including reaped workers. Footprint covers only the CLI process. |

No memory-pressure or lower-tier machine was measured.

## Release UI replay history

- Run 1 (exported tree): `rapid-input` and `large-project` failed in release
  before their measurement loops. One frame after the prelude settled, an
  automatic analysis save (`SaveShotAnalysis`) took the service's single
  user-command slot, so `begin_dialog` ignored Cmd+O silently and rapid wraps
  were refused.
- A concurrent change in the shared checkout moves analysis saves to a
  separate lane and reports file-action refusals. With it, run 3's hashed
  harness binary passes all three scenarios.
- An earlier same-day unhashed note quoted 0.41 ms key-to-state p95. Run 3 is
  the hashed replacement for it.

## Changes from this work

- The native cursor's beat lookup (`preview/selection.rs::at_boundary`) is a
  binary search over the ordered rows of the current scope. A randomized test
  compares it with the linear definition, including zero-length rows and rows
  offset to a nested scope's absolute start.
- `deadpan_playback::Engine::diagnostics()` provides atomic delivery counters,
  asserted by the starvation test.
- `ProjectPictureSession::stats()` counts decoder admissions and reuses.
  `perf seek` reports them.
- `deadpan-cli doctor --project` (macOS/Linux) reports one cold sample of
  each per-revision stage, plus per-source codec, raster and keyframe
  spacing. Its test proves it leaves package files unchanged apart from
  SQLite's WAL-index read marks.
- `cargo xtask perf` with the `perf` example, gated as described in
  [PERFORMANCE.md](../PERFORMANCE.md).

## Remaining gaps

- Proxy or intra-frame preview for 1080p and 4K long-GOP warm seeks.
- Whole-Original decode at cold admission.
- Pause timing-layout growth; full-document storage per revision; validation
  of every revision on open; pause refusal at 10,000 beats.
- Remaining linear UI scans:
  - per motion: NodeId lookups in `after_refresh` and `split_boundary`;
  - per frame: the status line, inspector and selected-group lookups;
  - nested scopes: the `document_path` prefix sum;
  - word motions: `SpeechTimeline::units` builds a vector of every word.
- Missing counters: decode queue depth, file I/O, PCM/limiter cache hits, live
  model memory, and a native diagnostics panel.
- Unmeasured: idle-machine edit rerun, physical display latency, idle CPU,
  memory pressure, lower-memory tiers, an editing/inference audio stress
  suite, release-build AI generation, and page-cache-cold seeks.
