# Performance measurement

[Specification Section 25](spec/DEADPAN_SPEC.md#25-performance-requirements-and-instrumentation)
sets engineering targets. This page maps each target to the measurement that
evaluates it, describes the reproducible suite, and lists the diagnostics the
code exposes. Results live in dated qualification records; the current one is
[performance-2026-10-04](qualification/performance-2026-10-04.md), with seek
superseded by [seek-2026-10-05](qualification/seek-2026-10-05.md) and preview
proxy seeks in [proxy-2026-10-05](qualification/proxy-2026-10-05.md), and edit
latency, storage and open time superseded by
[timing-storage-2026-10-05](qualification/timing-storage-2026-10-05.md) and then
[incremental-commit-2026-10-05](qualification/incremental-commit-2026-10-05.md). A target
without a measured workload is open, never passed.

## Run the suite

Prepare the pinned FFmpeg prefix ([Development](DEVELOPMENT.md)), then:

```sh
export DEADPAN_FFMPEG_PREFIX=/path/to/prefix
cargo xtask perf --output /tmp/deadpan-perf-NEW \
  --fixture caminandes=/path/to/caminandes.deadpan \
  --fixture interview=$HOME/Documents/Deadpan/interview.deadpan \
  --generate --ui
```

- Builds the release `deadpan-cli`, `deadpan-media-worker` and the
  `deadpan-cli` example `perf` once and copies them to `OUTPUT/bin`, so a
  concurrent rebuild cannot replace a running binary. `summary.json` records
  their SHA-256 digests, the Git revision and every dirty path.
- Every `--fixture` package is copied with `cp -Rp` into `OUTPUT/fixtures`
  while the suite holds a shared lock on its `.writer.lock`. A package that
  the app or CLI holds open for writing is refused rather than copied
  mid-transaction. Stages that write (edit, playback, export) use a further
  private copy under `OUTPUT/work`. The named packages are never opened
  writable.
- `--generate` makes two long H.264 High/AAC fixtures with the `ffmpeg` on
  PATH (x264 `veryfast`, default long GOP, explicit BT.709 limited range):
  1920×1080 at 60 fps for 120 s and 3840×2160 at 30 fps for 60 s, then imports
  each with `project create-original`. The exact arguments are in the summary.
- A 10,000-beat package of root Background/Silence Holds (the `large-project`
  replay's shape) is always created with `perf make-large`.
- `--ui` also builds the ui-harness app in release and runs its
  `rapid-input`, `edit-latency` and `large-project` scenarios in
  `--mode performance` (see [UI feedback](UI_FEEDBACK.md#performance-interpretation)).
- The seek stage also passes `--proxy-cache OUTPUT/work/proxies-NAME
  --worker BIN` to `perf seek`, so the [preview proxy](PROXIES.md) is built
  and verified in a private cache root, never the user's. It then measures
  proxy work as the native viewer performs it:
  - opening: the first opening, which hashes the file once, and later
    openings;
  - cold: a new reader to the first picture;
  - warm: decode plus Metal completion;
  - refinement: a proxy seek, the 150 ms rest, then the exact Original
    picture from a warm, verified preview session.

  Originals that need no proxy (at or below 1080p) report
  `"status": "not_needed"`. `perf proxy-build PACKAGE --proxy-cache DIR
  --worker BIN` runs one build alone, for measuring its effect on concurrent
  playback and edits.
- `--quick` shortens fixtures and sample counts for a smoke run; every target
  row is then INFO. `--stages` selects a subset of
  `doctor,scale,seek,edit,playback,export,ui`. `--audition-seconds` sets the
  real-device audition length (default 30).
- Before each seek, edit, playback, export and UI stage the suite waits up to
  10 minutes for the 1-minute load average to fall to `--max-load` (default
  3.0). A stage that still starts above it is flagged, and its targets are
  INFO.

Each stage runs under `/usr/bin/time -l`; its JSON, log and resource line
(wall, user, system, maximum RSS including reaped children, and the measured
process's peak memory footprint) are kept in the output directory.
`summary.json` records every binary's SHA-256, including the UI harness
binaries, the Git revision, dirty paths and a hash of the tracked diff. It
ends with a `targets` table. A row is PASS or FAIL only when its stage
succeeded, the machine was quiet and it has enough samples: at least 100 warm
seeks, 20 committed edits of that kind, or 40 UI samples. Otherwise it is INFO
with the reason. Playback rows also require the audition to cover the
requested interval (or the rest of a shorter project), with no dropped,
leading or trailing pictures. It records machine load and `pmset` power/thermal state before and after,
but it does not purge the OS file cache (that needs root); the reports say
which samples ran with a warm file cache.

Close other heavy work first, and record what else was running. Compare only
like workloads and the same summary schema.

## Target inventory

| Section 25 target | Measurement | Notes |
| --- | --- | --- |
| Key event to command-state update, p95 < 8 ms | ui `rapid-input` `warm_navigation_input_cpu_ms` | Whole egui input frame CPU: an upper bound on the state update. Small replay fixture. |
| Cached ordinary edit to visible preview, p95 < 50 ms | ui `edit-latency` `cached_repeat_input_to_picture_complete_ms`; `perf edit` split, Repeat wrap and Undo | The replay measures input through real commit, decode and offscreen Metal completion on its small fixture. `perf edit` measures the store commit plus workspace refresh (the cached validated head and its plan compiled in that validation scope, as the native workspace does) on real and 10,000-beat packages, without the picture. It then reopens the edited package writable and read-only and reports `reopen_*` times and `reopen_validation`. |
| Warm seek within an indexed source, p95 < 80 ms | `perf seek` warm random seeks, Original and preview proxy; ui `rapid-input` navigation to picture completion | `perf seek` uses the committed project picture boundary with progressive (preview) admission, the persistent threaded decoder and the shared Metal pipeline, on real long-GOP media at full canvas. Warm samples start after the background index measurement has verified; `measuring_seek` samples random seeks while it still runs. `decode_threads` records the serving codec thread count. `proxy.warm_seek` measures the same random seeks through the verified [preview proxy](PROXIES.md), which the native viewer shows first for a stopped seek. `proxy.refinement.refined_ms` is the latency until the exact Original picture replaces it, including the 150 ms rest. |
| Cold long-GOP seek, < 300 ms | `perf seek` cold samples (10 per admission mode) | Session-cold but page-cache-warm, so INFO: each sample is a new picture session (store, plan, verified private snapshot, decoder, first picture), but the OS file cache is not purged. `cold.progressive` is the preview path (receipt-verified pictures while the complete measurement runs; `verified_ms` is when it finishes); `cold.complete` is the export path, which measures the whole index before its first picture. Retaining the previous picture while loading is a UI property not measured here. |
| Playback 1080p60 and 4K30 | `perf playback` on the generated fixtures; `perf seek` frame stepping | Real device audio; pictures follow the heard clock, newest frame wins, skipped frames count as dropped. Decode and Metal completion only: no window, compositor or display scanout. |
| Audio: no callback underruns | `perf playback` `Engine::diagnostics()` | Counts starved and faulted device reports. The full editing/inference stress suite is not yet defined. |
| Hold insertion fallback visible < 100 ms | ui `edit-latency` `hold_fallback_input_to_picture_complete_ms`; `perf edit` insert pause | The CLI stage commits the native `,h` freeze through the store. |
| 10,000-beat navigation without a whole-document scan | ui `large-project` navigation CPU; `perf scale` frame lookup at 100 to 50,000 beats; code audit | Lookups use the compiled plan's binary search; per-revision work (validation, plan compile, rows) scales with size and is measured separately. |
| Idle: event-driven UI | Not measured by this suite | The replay harness's repaint wait (see [UI feedback](UI_FEEDBACK.md#repaint-waits-and-worker-timing)) is evidence of event-driven repaint requests, not an idle CPU measurement. |
| AI generation | Cited from the [AI pause run](AI_HOLDS.md#measured-run-2026-10-04) | Not rerun here: a single run of minutes and about 21 GiB footprint. |
| Export speed by workload | `perf` export stage through `deadpan-cli render` | Wall time per stage from timestamped events, encoding frames per second, and `real_time_factor` (content seconds from the committed frame rate ÷ wall), including encoder admission, full verification and publication. |
| Memory | `/usr/bin/time -l` around every stage | Peak RSS and footprint per process. Not a memory-pressure test. |

## Diagnostics

| Section 25.3 counter | Where | State |
| --- | --- | --- |
| Render-plan compile time | `deadpan-cli doctor --project PACKAGE` (`single_sample_ms.plan_compile`, with snapshot load, validation and anchor index; one cold sample each); `perf scale` | Implemented. |
| History validation on open | `ProjectStore::open_validation()` and `validate_report`; `doctor --project` `history_validation`; `perf edit` `reopen_validation`: revisions, those proved by the [history receipt](TIMING_STORAGE.md#verified-history-receipts) and those recomputed | Implemented. `project validate` recomputes all; `--quick` reports what opening checks. |
| Audio underruns and device faults | `deadpan_playback::Engine::diagnostics()`: activated generations, device reports, starved reports, faults, silent padding frames and maximum callback render cost | Implemented as cumulative atomics; observations only. Not yet shown in the native app. |
| Dropped video frames | `perf playback` (pictures skipped while following the heard clock) | Benchmark only. The native preview coalesces superseded requests without a counter. |
| GPU submission and completion | `perf seek`/`perf playback`; ui-harness `ui_composition_*` and picture timings | Harness timings are feature-gated. |
| Preview proxy | `perf seek` `proxy` (build time, bytes, raster, fidelity and bias, opening, seek and refinement latency); `perf proxy-build`; the sidecar's `fidelity`; the app's proxy job state on the Original card and in the `proxy-seek` replay | No live counter of proxy versus Original pictures in the native app. |
| Index measurement | `SourceSession::measurement()`, `ProjectPictureSession::source_measurement()`: Measuring, Verified, Mismatch or Interrupted for the retained Original; `DecodeWork::decoded_pictures` counts pictures the codec decoded | Not shown in the native app. A mismatch surfaces as the picture error; the preview worker retries an interruption once. |
| Cache hit rate | `ProjectPictureSession::stats()`: cold decoder admissions and their time, retained-decoder reuses, decoded and Background pictures. `perf seek` reports them as `cold.<mode>.session_stats` and `warm_session_stats`, with the decoder reuse rate. | Picture sessions only. PCM, limiter and thumbnail caches expose occupancy at most. |
| Decode queue depth | None | The picture path is single-flight by design; the playback preparation queue is bounded but not instrumented. |
| File I/O | None | Open. |
| Model memory | Pack manifests declare memory; the AI pause run measured RSS and footprint externally | Open as a live counter. |
| Active decoders, encoders, preview resolution, model pack, fallback path | `doctor` runtime section (FFmpeg images, workers, model root) and `doctor --project` (per-source codec, raster, keyframe spacing, preview canvas, quality tier) | The CLI reports stored and installed facts; it cannot see a running app's live sessions. |

All counters are process-local and bounded; none enters authored state,
history or the project database.
