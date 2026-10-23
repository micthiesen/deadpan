# Performance measurement

[Real inference interference](qualification/inference-interference-2026-10-17.md)
qualifies paired 30-second ordinary-source loops at 1080p60 and 4K30 with
app-owned Extension inference, actual device audio and offscreen Metal:
complete picture coverage and no underruns, faults or padding. Its seek/edit
samples are smoke checks; broader effects, rendering and latency distributions
remain unqualified by that workload.

[Specification Section 25](spec/DEADPAN_SPEC.md#25-performance-requirements-and-instrumentation)
sets engineering targets. This page maps each target to the measurement that
evaluates it, describes the reproducible suite, and lists the diagnostics the
code exposes. Results live in dated qualification records; the current one is
[performance-2026-10-04](qualification/performance-2026-10-04.md), with seek
superseded by [seek-2026-10-05](qualification/seek-2026-10-05.md) and preview
proxy seeks in [proxy-2026-10-05](qualification/proxy-2026-10-05.md), and edit
latency, storage and open time superseded by
[timing-storage-2026-10-05](qualification/timing-storage-2026-10-05.md) and then
[incremental-commit-2026-10-05](qualification/incremental-commit-2026-10-05.md) and then
[command-work-2026-10-05](qualification/command-work-2026-10-05.md), which also
records idle CPU. [pause-and-cold-seek-2026-10-06](qualification/pause-and-cold-seek-2026-10-06.md)
supersedes edit latency on 10,000 beats and the cold, warm 4K and
refinement seek rows;
[store-and-seek-2026-10-15](qualification/store-and-seek-2026-10-15.md)
moves the 64th-revision keyframe out of the edit (10,000-beat pause p95
104.9 to 75.5 ms at load 3.25 and 2.9) and adds page-cache-cold seeks
from a freshly attached disk image;
[edit-latency-2026-10-15](qualification/edit-latency-2026-10-15.md)
supersedes 10,000-beat edit latency (pause p95 71.4 / 74.3 to 40.3 /
44.7 ms, split, wrap and Undo p95 about 45 to about 20 ms, gated at load
2.8 to 3.0) by no longer rehashing the history on each writer edit and by
sharing document maps and identities; and [playback-proxy-2026-10-06](qualification/playback-proxy-2026-10-06.md)
the playback rows (with and without proxy pictures, cut edits) and adds the
stress runs. [Selection-loop decode-ahead](qualification/loop-lookahead-2026-10-11.md)
adds 30-second real-device comparisons at 1080p60 and 4K30: zero dropped
pictures with loop preparation, versus 17 and 12 without it. Run a bounded
loop with `perf playback PACKAGE --start-frame FIRST --loop-end EXCLUDED
--seconds 30 --pictures original --lookahead on` and repeat with `off`.
The report counts monotonic progress across wraps and records each seam.
[Generated playback caching](qualification/generated-playback-cache-2026-10-11.md)
retains Original decode-ahead across Generated Holds and reuses the Generated
decoder on repeated entry. Its bounded fixture checks verify handoff and pixels;
[Cold Generated preparation](qualification/generated-prefetch-2026-10-11.md)
now admits and decodes the upcoming Hold before entry; its small-fixture
prepared entry takes 0.17–0.22 ms. The [real app playback qualification](qualification/generated-app-playback-2026-10-11.md)
then exercises real accepted LTX pictures and full-resolution 1080p60/4K30
Originals through the app worker, real device audio and complete offscreen Metal
UI. Every picture is presented, with zero skips, proxies, underruns or faults.
The record separates worker time from complete UI composition and documents
the timing bookkeeping correction. The [closely spaced provider qualification](qualification/generated-transitions-2026-10-11.md)
adds a second real accepted Hold separated by one Original picture: both
1080p60 runs show all 180 pictures, and second entry falls from 190 ms to
0.15–0.16 ms. The four-decoder bound includes one upcoming Generated admission;
whole-replay footprint is about 768 MiB, not a worst-case decoder measurement.
[Generated selection loops](qualification/generated-loops-2026-10-11.md)
add 30 laps at 1080p60 and 4K30 with every picture shown. Preparing the missing
Original before first entry reduces the 4K request from 64 ms to 8.7 ms.
A separate nonloop control drops one early Generated picture.
[FFV1 slice threading](qualification/generated-slices-2026-10-11.md) reduces
standalone decode p95 from 6.2–6.5 ms to about 2 ms. Five ordinary 1080p60 runs,
one 4K30 run and both loops present every picture. A longer run misses its final
Original picture at the End-to-stopped handoff; terminal presentation stays open.
A target without a measured workload is open, never passed.

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
  - refinement: a proxy seek, the worker's 30 ms rest for an isolated
    request (`ISOLATED_REFINE_DELAY`; scrubbing keeps 150 ms), then the
    exact Original picture from a warm, verified preview session.

  Originals that need no proxy (at or below 1080p) report
  `"status": "not_needed"`. `perf proxy-build PACKAGE --proxy-cache DIR
  --worker BIN` runs one build alone, for measuring its effect on concurrent
  playback and edits.
- `--stages cold-seek` (opt-in) measures page-cache-cold preview seeks
  without root. Each media fixture is copied once onto a new APFS disk
  image (`hdiutil create`), which is then attached afresh (and detached
  afterwards) around each of 10 samples (3 with `--quick`). A new mount has
  no cached pages of its files, and the disk-image driver reads its backing
  file without caching it (checked with `mincore` in the
  [2026-10-15 record](qualification/store-and-seek-2026-10-15.md)), so each
  run's first session reads the database and Original from storage. `perf
  cold-seek PACKAGE --basis-from OTHER_COPY --repeat 1` sizes the Metal
  target from another copy, so nothing reads the package before the
  measured session opens, and repeats the seek once page-cache-warm in the
  same process. Reads pass through the disk-image driver (about 4.4 GB/s
  sequential on the 2026-10-15 machine); a direct read from the internal
  SSD's own volume is not measured, because purging its cache needs root.
- `perf edit` writes the keyframe a commit left due
  (`ProjectStore::write_due_keyframe`, see [deferred
  keyframes](TIMING_STORAGE.md#deferred-keyframes)) after the edit's refresh,
  as an idle host does, and reports it as `deferred_keyframe_ms`, outside the
  edit's `total_ms`.
- `perf edit PACKAGE --kinds split,pause,wrap` measures only the named edit
  kinds (Undo follows each wrap), for attributing one path under a sampling
  profiler such as `sample`. `perf seek PACKAGE --admissions progressive`
  skips the slow complete (export) admission in the same way.
- The playback stage plays each media fixture four ways for
  `--audition-seconds`: as imported and as a `perf make-cuts` copy (fragments
  of 24 pictures, every second one a three-play Repeat, so a backward jump
  needing a keyframe seek about every 2.4 s), each with `--pictures original`
  (never a proxy) and `--pictures adaptive` (the native worker's
  [playback tier choice](PROXIES.md#playback-pictures), with the proxy built
  into the seek stage's private cache if missing). Result keys are
  `playback/NAME`, `NAME+proxy`, `NAME-cuts` and `NAME-cuts+proxy`.
- `--stages stress` (opt-in, not part of the default set) adds: playback of
  the 10,000-Hold package; per media fixture, playback of a `perf make-long`
  copy (100 fragments of 6 pictures grouped, repeated 100 times and
  exploded: 10,000 Original Source beats); and per media fixture a fresh
  proxy build, adaptive playback of a cuts copy and a public Render export
  started together (the app itself pauses proxy builds during playback and
  renders; this does not). The seek storm runs through the real preview
  worker as an ignored app test (`worker::project_tests::stress_tests`;
  see its module documentation).
- `--quick` shortens fixtures and sample counts for a smoke run; every target
  row is then INFO. `--stages` selects a subset of
  `doctor,scale,seek,cold-seek,edit,playback,export,stress,ui`. `--audition-seconds` sets the
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
leading or trailing pictures. It records machine load and `pmset` power/thermal state before and after.
It never purges the OS file cache (that needs root): the `seek` stage's
cold samples are session-cold but page-cache-warm, and only the opt-in
`cold-seek` stage reads from storage, through a freshly attached disk
image. A `cold-seek` row needs 10 samples and its slowest must be below
300 ms; the load gate applies to every sample.

Close other heavy work first, and record what else was running. Compare only
like workloads and the same summary schema.

The [VideoToolbox comparison](qualification/hardware-decode-2026-10-11.md)
uses `cargo run --locked --release -p deadpan-media --example compare_decode --
ABSOLUTE_MOVIE ABSOLUTE_NEW_REPORT_JSON` to compare complete software and
hardware pixels plus 100 random seeks per path. This isolates decoder work;
it does not replace the shared Metal, playback or native UI stages above.
Hardware decoding stays opt-in because the measured long-GOP seeks were slower.

## Target inventory

The [first-pause profile](qualification/first-pause-profile-2026-10-23.md) and
[compact frozen timing qualification](qualification/compact-frozen-edges-2026-10-23.md)
separate the fresh 10,000-Hold binding cost from later edits. Qualified compact
timing measures first store-and-refresh medians 130.636 → 114.691 ms across five
quiet pairs, before picture display. A later
[binding/capture prototype](qualification/first-pause-binding-prototypes-2026-10-23.md)
measures 111.576 → 91.629 ms in a separate five-pair batch, with incomplete
standalone decoding. The [qualified Core 58 implementation](qualification/compact-audio-bindings-2026-10-23.md)
adds that reader correction and measures first medians 114.587 → 93.633 ms in
five quiet current-format pairs, with second medians 32.896 → 28.906 ms. These
short batches exclude picture display and establish neither a percentile nor
the complete visible-Hold target.

The [large first-pause replay](qualification/large-first-pause-harness-2026-10-23.md)
adds `large-first-pause` with an explicit fresh `perf make-large --beats 10000`
package. It opens a private portable copy, settles the initial Background,
routes `hold 12f` at frame 17,933, and requires one committed insertion plus
matching new-revision requested/decoded/submitted/composed picture identity.
Its final release diagnostic measures 393.681375 ms to commit and 396.208834 ms
through offscreen Background composition. Each invocation has one sample;
forty independent fresh invocations are required before a UI percentile claim.
These measurements include native semantic-command/service work absent from
the direct 93.633 ms store-and-refresh median. They do not attribute the delay
to the store, establish either edit target, or measure a physical display,
source decoding or PCM. The subsequent
[isolated app-service profile](qualification/ui-first-pause-profile-2026-10-23.md)
uses three rotated fresh-process triplets. Uninstrumented input-to-composition
median is 388.289833 ms; the instrumented service has median semantic planning
108.376500 ms, preview/runtime-bank preparation 119.775334 ms, Compound commit
130.474125 ms and refresh 4.128667 ms. Inclusive totals and disjoint children
are kept separate. All nine replays and provenance guards pass. This is a
diagnostic, with uncalibrated timer overhead and no queue-wait measurement,
percentile or target PASS. It motivates sharing Compound preparation inside
one transaction, while retaining fallible runtime preparation before writes.

The [qualified shared preparation](qualification/compound-runtime-preparation-2026-10-23.md)
implements that transaction-owned callback. Five quiet fresh-package pairs
measure input-to-commit medians 390.680250 → 273.119542 ms and input through
offscreen Background composition 393.261916 → 275.697709 ms. Every run passes
its seven replay checks and source/binary/authored-input guards, with an empty
WAL. Both executables use Core 58 /SQLite 87 and the same fixture and measured
command. This reduces the latter median by 117.564207 ms, about 29.9%, while
the 100 ms fallback and 50 ms ordinary-edit targets remain open. Five samples
per executable are diagnostic; they do not establish a formal percentile,
physical display latency, source decoding or PCM performance.

The subsequent [prepared-Compound profile](qualification/prepared-compound-profile-2026-10-23.md)
passes three rotated control/off/on triplets. Diagnostic medians locate
108.336666 ms in semantic planning and 117.677375 ms in Store preparation;
history writing is 7.352000 ms, durable commit 6.503167 ms and refresh
4.204375 ms. Nested totals are not added to their children. Queue wait and
instrumentation overhead remain unqualified; the fixture's empty bank does
not establish a runtime-copy preparation bound. These results motivate
examining planning/validation rather than weakening durability, with no
additional production gain or percentile/target PASS.

| Section 25 target | Measurement | Notes |
| --- | --- | --- |
| Key event to command-state update, p95 < 8 ms | ui `rapid-input` `warm_navigation_input_cpu_ms` | Whole egui input frame CPU: an upper bound on the state update. Small replay fixture. |
| Cached ordinary edit to visible preview, p95 < 50 ms | ui `edit-latency` `cached_repeat_input_to_picture_complete_ms`; `perf edit` split, Repeat wrap and Undo | The replay measures input through real commit, decode and offscreen Metal completion on its small fixture. `perf edit` measures the store commit plus workspace refresh (the cached validated head and its plan compiled in that validation scope, as the native workspace does) on real and 10,000-beat packages, without the picture. A keyframe the edit left due is written afterwards and reported separately (`deferred_keyframe_ms`, informational row). It then reopens the edited package writable and read-only and reports `reopen_*` times and `reopen_validation`. |
| Warm seek within an indexed source, p95 < 80 ms | `perf seek` warm random seeks, Original and preview proxy; ui `rapid-input` navigation to picture completion | `perf seek` uses the committed project picture boundary with progressive (preview) admission, the persistent threaded decoder and the shared Metal pipeline, on real long-GOP media at full canvas. Warm samples start after the background index measurement has verified; `measuring_seek` samples random seeks while it still runs. `decode_threads` records the default serving codec thread count and `decode_threads_for_canvas` the count for a stream of the canvas raster (more above 1080p). `proxy.warm_seek` measures the same random seeks through the verified [preview proxy](PROXIES.md), which the native viewer shows first for a stopped seek. `proxy.refinement.refined_ms` is the latency until the exact Original picture replaces it, including the worker's rest before refining an isolated request (30 ms; 150 ms while the cursor keeps moving). |
| Cold long-GOP seek, < 300 ms | `cold-seek` stage: `perf cold-seek` on a freshly attached disk image (10 samples); `perf seek` cold samples (10 per admission mode) | `cold-seek` is page-cache-cold (preview session from a new mount to the first picture, decode plus Metal completion) and PASSes or FAILs on its slowest sample. The `seek` stage's cold samples are session-cold but page-cache-warm, so INFO: each sample is a new picture session (store, plan, verified private snapshot, decoder, first picture), but the OS file cache is not purged. `cold.progressive` is the preview path (receipt-verified pictures while the complete measurement runs; `verified_ms` is when it finishes); `cold.complete` is the export path, which measures the whole index before its first picture. Retaining the previous picture while loading is a UI property not measured here. |
| Playback 1080p60 and 4K30 | `perf playback` on the generated fixtures and their cuts copies, with and without the proxy tier; `perf seek` frame stepping | Real device audio; pictures follow the heard clock, newest frame wins, skipped frames count as dropped. Pictures come from a progressively admitted interactive decoder, as in the native viewer (records before 2026-10-06 used a completely admitted `ProjectPictureSession`). Reports per-tier decode cost, presentation intervals, lag at completion, proxy runs until exact pictures resume and the exact picture after the stop. Decode and Metal completion only: no window, compositor or display scanout. |
| Audio: no callback underruns | `perf playback` `Engine::diagnostics()` | Counts starved and faulted device reports. The full editing/inference stress suite is not yet defined. |
| Hold insertion fallback visible < 100 ms | ui `edit-latency` `hold_fallback_input_to_picture_complete_ms`; `perf edit` insert pause | The CLI stage commits the native `,h` freeze through the store. |
| 10,000-beat navigation without a whole-document scan | ui `large-project` navigation CPU; `perf scale` frame lookup at 100 to 50,000 beats; code audit | Lookups use the compiled plan's binary search; per-revision work (validation, plan compile, rows) scales with size and is measured separately. |
| Idle: event-driven UI | Release app opened with `--project`, settled 30 s, then CPU time from `ps` over 60 s, wakeups from `top` and footprint from `footprint` ([record](qualification/command-work-2026-10-05.md#idle-and-memory)) | Manual, not part of `cargo xtask perf`. A visible window only; occluded, background and display power are not measured. |
| AI generation | Cited from the [AI pause run](AI_HOLDS.md#measured-run-2026-10-04) | Not rerun here: a single run of minutes and about 21 GiB footprint. |
| Export speed by workload | `perf` export stage through `deadpan-cli render` | Wall time per stage from timestamped events, encoding frames per second, and `real_time_factor` (content seconds from the committed frame rate ÷ wall), including encoder admission, full verification and publication. |
| Memory | `/usr/bin/time -l` around every stage; the idle app's footprint | Peak RSS and footprint per process. Not a memory-pressure test: `memory_pressure -S` needs root, and no allocation-pressure run exists. |

## Diagnostics

| Section 25.3 counter | Where | State |
| --- | --- | --- |
| Render-plan compile time | `deadpan-cli doctor --project PACKAGE` (`single_sample_ms.plan_compile`, with snapshot load, validation and anchor index; one cold sample each); `perf scale` | Implemented. |
| History validation on open | `ProjectStore::open_validation()` and `validate_report`; `doctor --project` `history_validation`; `perf edit` `reopen_validation`: revisions, those proved by the [history receipt](TIMING_STORAGE.md#verified-history-receipts) and those recomputed | Implemented. `project validate` recomputes all; `--quick` reports what opening checks. |
| Audio underruns and device faults | `deadpan_playback::Engine::diagnostics()`: activated generations, device reports, starved reports, faults, silent padding frames and maximum callback render cost; the native `:diagnostics` panel (PLAYBACK) | Implemented as cumulative atomics; observations only. |
| Dropped video frames | `perf playback` (pictures skipped while following the heard clock); `deadpan_diagnostics::PLAYBACK_PICTURES` in the native app: playback pictures requested and frames skipped by the transport, pictures served exact or from the proxy, repositions started and reached; `:diagnostics` (PLAYBACK) | Cumulative per process. A loop wrap is not counted as a drop. |
| GPU submission and completion | `deadpan_diagnostics::GPU`, recorded at the shared picture pipeline's submit (`deadpan-render`): submissions, completions, last, maximum and p50/p95 over a fixed 128-sample ring of `queue.submit` to the `on_submitted_work_done` callback; `:diagnostics` (GPU) and `doctor --project` (`diagnostics.gpu_submissions`, zero there); `perf seek`/`perf playback`; ui-harness `ui_composition_*` and picture timings | The latency ends when the owner next polls the device, so it includes polling delay. egui's own frame submission and the readback copy are not counted: eframe 0.36 performs the frame's `queue.submit` and presentation internally and exposes no hook around them, so counting it would mean replacing eframe's wgpu painter. Harness timings remain feature-gated. |
| Preview proxy | `perf seek` `proxy` (build time, bytes, raster, fidelity and bias, opening, seek and refinement latency); `perf proxy-build`; `perf playback --pictures adaptive` tiers; the sidecar's `fidelity`; the app's proxy job state on the Original card and in the `proxy-seek` replay; playback proxy and exact picture counts in `:diagnostics` | Stopped proxy pictures are not counted. |
| Index measurement | `SourceSession::measurement()`, `ProjectPictureSession::source_measurement()`: Measuring, Verified, Mismatch or Interrupted for the retained Original; `DecodeWork::decoded_pictures` counts pictures the codec decoded | Not shown in the native app. A mismatch surfaces as the picture error; the preview worker retries an interruption once. |
| Cache hit rate | `ProjectPictureSession::stats()`: cold decoder admissions and their time, retained-decoder reuses, decoded and Background pictures. `perf seek` reports them as `cold.<mode>.session_stats` and `warm_session_stats`, with the decoder reuse rate. `deadpan_diagnostics::CACHES`: hits, misses, evictions and resident entries/bytes (current and high) for the decoded-PCM source caches (CLI inspection and playback preparation, each bounded at 16 sessions and 1 GiB), limiter tiles (entries only) and limiter input blocks (entries and sample bytes); `:diagnostics` (PCM CACHES) and `doctor --project` (`diagnostics.pcm_caches`). Card thumbnail textures (hits, misses, evictions, resident entries) and the per-user preview-proxy cache (hits, misses, cleanup evictions); `:diagnostics` (PICTURE CACHES) and `doctor --project` (`diagnostics.picture_caches`) | The native app's preview worker uses `SourceSession` directly, not `ProjectPictureSession`, so its panel has no picture-session row. A hit counts only a revalidated reuse. A thumbnail lookup is one card's demand when it appears or its picture's key changes (an edit), not every painted frame; a hit means that exact picture is still retained. A proxy lookup is `cached_proxy` (status checks, builds and preview opens); its residency is a shared on-disk cache and is not tracked. |
| Decode queue depth | `deadpan_diagnostics::QUEUES`, current and high-water: the main viewer's and the card thumbnail service's source-preview slots (pending plus in progress, including idle proxy preparation; 0 to 2 each), prepared replies waiting for playback control workers (up to 175 PCM batches plus one terminal failure per engine), their stereo frame count (up to 1,433,600 per engine, excluding active batches/device queues/caches), packets in the device queue as seen by its producer (`Feed::queued_packets`, sampled each control tick, never from the callback) and the frames queued before the latest activation, the shared Trim/splice endpoint and Edit-junction picture worker's slots (pending plus in progress, 0 to 2), and waveform analysis jobs (gain-definition and Edit waveforms share one superseding slot per playback engine: queued plus running, 0 to 2); `:diagnostics` (DECODE QUEUES) and `doctor --project` (`diagnostics.queue_depths`, idle there) | The picture and waveform paths are single-flight by design, so a depth above 1 means a replaced request was still running. |
| File I/O | `deadpan_diagnostics::IO`, cumulative operations and bytes per path: the store's history rows (revision documents and stored patches read or inserted, and each edit's request and patch, counted after SQLite accepts the row; logical payload bytes), Media/Originals and Media/Generated object copies, verification reads and private snapshot writes, `deadpan-media` source snapshot copies, native video and audio decoder descriptor reads (each added once per decoder call from its cumulative AVIO count, including the opening guard), private decoded/prepared PCM cache files and proxy hashing reads; `:diagnostics` (FILE I/O) and `doctor --project` (`diagnostics.file_io`). `deadpan_diagnostics::SQLITE_PAGES`: SQLite page-cache hits, misses (pages read from the database or WAL), pages written and cache spills, read and reset per connection through `sqlite3_db_status` (the narrow `deadpan-sqlite-status` adapter) whenever the store reads or writes a history row and when a `ProjectStore` closes; `:diagnostics` (FILE I/O, SQLite pages) and `doctor --project` (`diagnostics.sqlite_pages`) | Not kernel I/O. Row byte counts still cover only history rows; page counts cover every table of a store connection but only once it publishes, so short-lived backup, checkpoint, migration and inspection connections that never reach a history row are not counted. Render/export output and model files are not counted. One operation is one buffered call. |
| Model memory | Pack manifests declare memory. `deadpan_diagnostics::WORKERS`: every `SupervisedProcess` samples its live, unreaped worker's physical footprint every 500 ms during `poll` (`deadpan_native_process::owned_child_memory`, `proc_pid_rusage` `RUSAGE_INFO_V2`), summed with current and peak, live count, samples and refused samples; transcription, AI pause generation, face and tracking workers count as model workers, render, encode, verification and probe workers as others; `:diagnostics` (MEMORY) and `doctor --project` (`diagnostics.model_workers`, none there). `deadpan_diagnostics::PROCESS_MEMORY`: this process's own physical footprint and resident size (`deadpan_native_process::own_memory`, the same `proc_pid_rusage` call), current and peak, sampled when diagnostics are read (the panel twice a second, `doctor --project` and diagnostic export once); `:diagnostics` (MEMORY, App process) and `doctor --project` (`diagnostics.process_memory`) | Sampling follows the host's poll cadence, so a short peak between samples is missed. The app footprint includes Metal allocations the system attributes to the process but does not break them out: wgpu 30's Metal backend supplies neither an allocator report nor memory counters, and a separate figure would need new unsafe Metal calls outside a qualified adapter. |
| Active decoders, encoders, preview resolution, model pack, fallback path | `doctor` runtime section (FFmpeg images, workers, model root) and `doctor --project` (per-source codec, raster, keyframe spacing, preview canvas, quality tier) | The CLI reports stored and installed facts; it cannot see a running app's live sessions. |

All counters are process-local and bounded; none enters authored state,
history or the project database. `deadpan-diagnostics` holds them as fixed
statics of relaxed atomics plus one 128-entry latency ring: recording never
allocates or locks, and the audio device callback records nothing. (Reading
SQLite's page counters takes that connection's own mutex inside SQLite, on
the store's thread; the recording into the statics stays lock-free.) The native
`:diagnostics` panel (Escape closes it) samples them, and this process's
footprint, twice a second without
pausing playback, alongside the playback engine's counters and the number of UI
outer UI updates (which include the panel's own refresh, so their rate is
not an idle measurement); each row is exposed to assistive technology as "label: value".
The `diagnostics` replay opens it through real keys, checks the accessible rows,
GPU latency, SQLite page and app-footprint sampling, resampling and keyboard
ownership, and closes it. A CLI process
reports only its own work: `doctor --project` adds `diagnostics` after its probes
(revision reads and their SQLite pages, the CLI's own footprint, no writes,
no GPU or worker activity).
