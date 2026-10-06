# Preview proxies

A preview proxy is a rebuildable, reduced-raster, intra-only copy of the
Original's picture stream. The main viewer uses it for stopped seeks, so a
random jump decodes one small picture instead of a long-GOP preroll. When the
cursor rests, the exact Original picture replaces it. Specification §16.4
calls for this tier, and DP-16 tracks it. Proxies are never authoritative:
export, render, verification, AI conditioning, tracking, shot analysis and
thumbnails never read them.

## Eligibility and recipe

§16.4 asks for proxies only where they materially improve seeking.
`deadpan_media::proxy::proxy_plan` therefore chooses by raster alone:

- An Original with more than 1920×1080 pixels gets a proxy (`Raster`).
  Measured 4K long-GOP warm seek is 211 ms p95 without one.
- At or below 1080p, no Original gets one, whatever its keyframe spacing.
  Threaded 1080p seeks with keyframes 250 pictures apart already measure
  48 ms p95 ([seek record](qualification/seek-2026-10-05.md)).
- An Original whose per-picture durations differ from its presentation
  intervals is ineligible (`IrregularDurations`). MP4 stores sample durations
  as decode-timestamp distances, so without reordering a proxy could not
  reproduce them exactly. Variable frame rate itself is fine. The Original
  card says "no seek proxy" and its hover explains why.

Recipe version 1 (`PROXY_RECIPE_VERSION`):

| Property | Value |
| --- | --- |
| Raster | Uniform scale to fit 1920 on the long side and 1080 on the short side, in the picture's own orientation. Never upscaled. Each dimension is rounded to the nearest even integer, at least 2. 3840×2160 becomes 1920×1080. |
| Aspect and rotation | The Original's sample aspect ratio, written to the MP4 track and, through FFmpeg's `h264_metadata` filter, to the H.264 VUI, because VideoToolbox writes none and decoders compare them. The Original's rotation, as the exact display matrix the source adapter maps to its quarter turns. |
| Codec | VideoToolbox H.264 High in MP4 (`h264_videotoolbox`, software allowed). Every picture is an IDR (`gop_size` 1, no B pictures), constant quality 60. |
| Timing | Each packet carries the Original picture's exact PTS and duration in the Original's time base. DTS equals PTS. The track and movie timescales equal the Original's time-base denominator. |
| Color | The worker decodes the Original to full-range RGBA through its own qualified interpretation (matrix, range, transfer, primaries). It converts that RGBA to limited-range BT.709 4:2:0 with left-sited chroma and area scaling. The stream is tagged BT.709 matrix, limited range, and the Original's transfer and primaries. Decoding the proxy through the same adapter therefore yields RGBA in the Original's transfer and primaries, and the shared renderer treats both identically. |

H.264 in MP4 was chosen because the source adapter already admits it through
its closed container grammar, with the same bounded decoder and per-picture
index checks. A new codec would need a new admission grammar. The pinned LGPL
build has no libx264, FFV1 is lossless and therefore much larger, and MJPEG
would need a new admission path.

## Building

`deadpan_cli::proxy::build_proxy` runs on a background thread:

1. **Status.** It checks eligibility, a remembered failure and the cache. A
   published entry is rehashed only if its file state changed (see
   *Storage*); a damaged entry is removed and rebuilt.
2. **Space.** The cache must fit a planning estimate of 1/8 byte per proxy
   pixel and picture, about six times the measured rate. The cache budget is
   checked first, with a cleanup if needed. Both the cache volume and the
   temporary volume (for the Original snapshot) must keep 2 GiB free beyond
   their share.
3. **Snapshot.** It copies and verifies the Original into a private snapshot.
4. **Encoding.** It runs `deadpan-media-worker proxy JSON`. The worker is
   process-isolated through `deadpan_native_process::spawn`, receives only
   descriptors (stdin: Original snapshot, stdout: a private 0600 staging
   file, stderr: heartbeats and one bounded JSON reply), lowers its own QoS
   to utility and decodes the Original with four codec threads.
   - The request is versioned, bounded and validated on both sides.
   - The output cap is three quarters of a byte per proxy pixel and picture
     plus 16 MiB, half the raw 4:2:0 size.
   - The host enforces the deadline, cancellation, the output cap and clean
     group teardown, as for every other worker mode.
5. **Stall watch.** The worker writes a newline heartbeat on its control
   pipe after opening its decoder and after each accepted picture, at most
   every 250 ms. The host strips heartbeats before the reply. If neither
   output growth nor a heartbeat happens for `PROXY_STALL_TIMEOUT` (60 s,
   configurable through `ProxyEncodeOptions`), the host stops the worker's
   process group and reaps its leader. It reports `Stalled` with whether
   that teardown was confirmed.
6. **Retry.** `encode_proxy_retrying` retries once, in a new process with a
   fresh staging file and the remaining deadline, after either:
   - a stall whose teardown was confirmed, or
   - an `invalid_packet` refusal, where VideoToolbox emitted a packet outside
     the one-IDR-per-picture contract and the worker exited normally.

   A failed VideoToolbox session (`encoder_session_failed`) is retried the
   same way. The failed attempt's staging is removed. An unconfirmed
   teardown is never retried, so two encoders never write at once, and a
   second failure is reported. Such failures are machine conditions, not
   remembered against the Original.

   Under concurrent sessions, VideoToolbox's encoder service sometimes never
   answers a synchronous `VTCompressionSessionCompleteFrames`. Its other
   failures are refusing new sessions and emitting out-of-contract packets
   ([evidence](qualification/proxy-2026-10-05.md#failures-and-robustness)).
   The worker therefore reports a failure, or exits after a complete
   output, without tearing the session down. The builder holds a per-user
   encoder slot (`Proxies/.encoder.lock`), so Deadpan runs at most one proxy
   session at a time.
7. **Pause.** While the build's pause flag is set, the host suspends the
   worker's process group (SIGSTOP) and resumes it after (SIGCONT). Paused
   time never counts as a stall; the deadline still runs. Verification waits
   between its steps, but a step already started (the index measurement)
   finishes.
8. **Verification.** `verify_proxy` reads the staged file in place, without
   copying it:
   - It hashes the file (SHA-256 and BLAKE3) and measures its complete index
     with one codec thread.
   - It checks that every proxy picture is a keyframe with the same exact
     rational PTS and duration as the Original picture of the same ordinal,
     that the terminal endpoints are equal and that the stream has the
     planned raster, `yuv420p` and the planned color tags.
   - It decodes sixteen evenly spaced pictures (first and last included) from
     both the proxy and the Original and compares 16×9 block averages of
     their RGB values.
9. **Publication.** It publishes the movie and its sidecar atomically.

**Fidelity.** The limits are three, all in 8-bit code values:

- mean absolute block difference at most 3.0;
- largest block difference at most 16.0;
- mean signed difference of each of R, G and B (proxy minus Original) at
  most 1.5. This catches a tint or level shift that averages out of absolute
  errors.

The sidecar records the result, together with the most colorful sample and
its Original's block luma and chroma spread, showing how much color the
samples covered. These checks catch wrong matrices, swapped channels, range
errors, wrong pictures and gross scaling errors; they do not qualify
compression quality. Measured values are in the
[qualification record](qualification/proxy-2026-10-05.md). A tiny,
hard-edged full-range RGB picture cannot survive 4:2:0 subsampling and is
refused rather than served with wrong color.

A failure that is neither a cancellation nor a machine condition (space,
budget) is remembered for that Original and recipe, so reopening the project
does not repeat a doomed build.

### In the app

`preview/proxies.rs` starts one build per project session, after the Original
is ready, including for an existing project when it opens. It first removes
stale cache entries. A new session or app exit cancels it, and cancellation
stops the worker. The build respects the user and the machine:

- **Setting.** `:proxies off` disables automatic proxies and `:proxies on`
  enables them. The setting is remembered per user in
  `~/Library/Application Support/Deadpan/proxies.json`.
- **Pause.** A monitor thread suspends the worker while the edit plays, a
  render runs, an AI model (generation or transcription) runs, or tracking or
  face detection runs ([job coordinator](JOBS.md) yield rules). It also
  suspends it while `pmset` reports battery power, Low Power Mode or thermal
  pressure, rechecked every 30 s.
- **Cancel.** The Jobs panel (`:jobs`) cancels a build; it is not rebuilt
  until `:proxies retry`.
- **Retry.** `:proxies retry` forgets a remembered failure and builds again.
- **Status.** The Original card shows "preparing seek proxy", "seek proxy
  paused", "seek proxy failed" or "no seek proxy", with the reason on hover.
  If no proxy exists, seeking simply uses the Original. Nothing waits for a
  proxy.

## Storage

Proxies live in the per-user cache that specification §20.1 reserves for
global rebuildable data: `~/Library/Caches/Deadpan/Proxies`
(`deadpan_cli::proxy::cache`). Packages and package copies that share an
Original share its proxy. No project, SQLite row, revision or history entry
refers to a proxy.

```text
~/Library/Caches/Deadpan/Proxies/
  .lock                          # flock: shared for reads, exclusive for changes
  v1-<Original BLAKE3>-s<stream>/
    proxy.mp4  proxy.json        # 0444 movie and sidecar index
    used                         # its modification time is the last use
    verified.json                # hash verified for one exact file state
  .staging/<uuid>/               # 0700, a build in progress
  .trash/<uuid>/                 # replaced or evicted entries
  .failures/v1-<…>.json          # remembered failed builds
```

- **Key and sidecar.** The key is the recipe version, the Original object's
  BLAKE3 content address and the stream index. The sidecar
  (`ProxySidecar`, schema 2) records:
  - schema, recipe, encoder and reason;
  - the Original's BLAKE3, SHA-256, length and stream;
  - the proxy file's SHA-256, BLAKE3 and length;
  - the proxy's measured stream interpretation and complete index;
  - the fidelity samples.
- **Descriptors.** Every operation is relative to one root directory
  descriptor, with `O_NOFOLLOW` on each component. The root must be owned by
  the user and not writable by others. Removal unlinks relative to its
  parent and never follows a symbolic link.
- **Publication.** Publication syncs both files, makes them read-only, syncs
  the staging directory, then takes the exclusive cache lock and checks
  that the root path still names the opened directory. It renames the
  staging directory into place with `RENAME_EXCL`, or atomically swaps it
  with an existing entry (`RENAME_SWAP`) and moves the old one to the trash.
  It then syncs the root. A partial file is never visible under an entry
  name, and dropping an unpublished staging directory deletes it.
- **Reading.** A reader opens the published movie in place and takes a
  shared `flock` on its descriptor for as long as it reads it. The bytes are
  hashed against the sidecar once per exact file state (device, inode,
  length, modification time), recorded in `verified.json`. Later openings,
  in any process, only compare that state, so reopening a project does not
  rehash its proxy. Opening also marks the entry used. A malformed entry is
  `Damaged`, and readers treat it as absent.
- **Cleanup and eviction.** `cleanup` runs before each build, under the
  exclusive lock. It removes:
  - staging older than 24 hours;
  - trash no longer read;
  - entries of other recipe versions;
  - entries unused for 30 days;
  - the least recently used entries while the cache exceeds its 64 GiB
    budget.

  The current Original's entry is retained. An entry whose movie a reader
  holds (an exclusive non-blocking `flock` fails) is never removed, in this
  or any other process. Explicit [cache cleanup](STORAGE.md) (`cache clean`,
  `:storage` then C) runs the same cleanup with a one-day unused and staging
  grace; it removes entries but never reads proxy pictures.
- **Safety.** Deleting the directory at any time loses only time.

## Preview reader

The only proxy picture reader is the app's private `worker/proxy.rs`. The
main viewer's `PreviewWorker::new` may serve a stopped committed picture
(`Work::Project` without a transport generation) from the proxy:

- **Opening.** A request never waits for a proxy to open, because the first
  opening of a new entry hashes it once. When a request finds a current entry
  but no open reader, the worker serves the Original and remembers the proxy.
  While idle after a stopped interactive request, never between playback
  pictures, it opens the proxy: lookup, sidecar validation against the
  receipt's index and interpretation, the published file through its shared
  lock, and a one-thread decoder checked picture by picture against the
  sidecar index. The next request cancels that opening. No bytes are
  copied.
- **Failures.** A proxy failure drops the proxy until its cache entry
  changes, and the worker decodes the Original instead. It is never shown as
  a picture error.
- **Steps.** A single step in either direction from the Original decoder's
  current picture is decoded exactly, without a proxy picture first: forward
  needs no seek, and a backward step must not flash proxy pixels.
- **Refinement.** After publishing a proxy picture, the worker waits for the
  cursor to rest. A request that followed the previous interactive request
  within `REFINE_DELAY` (150 ms) is scrubbing (held keys, a drag) and waits
  the full 150 ms; repeated keys arrive faster, so scrubbing never starts an
  exact decode it would abandon. An isolated request (a jump, a click, a
  key's first press) waits only `ISOLATED_REFINE_DELAY` (30 ms), which
  coalesces an immediate follow-up; a key's first repeat comes later than an
  exact 4K seek takes. The rest also lasts until the viewer has taken the
  proxy reply, so the exact picture follows it rather than replacing an
  undelivered one. If no newer request arrives, the worker decodes the
  exact Original picture for the same ticket, retrying once with a reopened
  decoder, and publishes it as a second reply. A newer request cancels the
  refinement through the same flag, and a stale refinement can never be
  published. If the refinement still fails, the proxy picture stays
  displayed with the picture error.

`Picture::tier` is `Original` or `Proxy`. A proxy picture is a distinct
presentation identity:

- `DisplayedPicture` keeps its tier, so the refined picture renders even
  though its request is unchanged.
- `stable_sequence_ticket` and `stable_proposed_ticket` require the
  `Original` tier, so a proxy picture is never a Camera, Slip or Trim target.
- Camera target drawing requires an Original-tier picture, its overlay waits
  while a proxy is displayed, and `Presentation::refining` lets Camera entry
  wait for the exact picture instead of failing.
- The viewer paints a small "Proxy" chip in the picture's top-right corner,
  and the accessible picture label ends in "· proxy preview", only while
  proxy pixels are displayed.

Playback (`ticket.transport` set), proposals, edited slices, copied views, AI
candidates and the card thumbnail worker (`PreviewWorker::named`) always
decode the Original.

## Never reaching export

- **Types and visibility.**
  - Proxy pictures exist only inside the app's private `worker/proxy.rs`,
    as a `PreviewFrame` whose only conversion is to the preview upload
    format there.
  - The media crate defines the contract, verification and sidecar but
    serves no proxy pictures.
  - `Workspace` holds no proxy cache. Only the main preview worker and the
    background job hold one.
- **Callers.** `ProjectPictureSession`, encoded render, export verification,
  generation conditioning, tracking and shot analysis obtain pictures only
  through store snapshots of Originals or generated objects.
- **Source check.** `export_paths_never_reach_proxies` scans every crate's
  `src` and fails if any file outside the proxy modules, the app's job and
  preview reader and their tests names `proxy::`, `ProxyCache`,
  `ProxySidecar`, `open_proxy_file`, `from_verified_file` or
  `PreviewFrame`.

## Evidence

| Test | What it shows |
| --- | --- |
| `native/deadpan-media-worker/tests/proxy_real_media.rs` (10 tests) | **Timing:** the real worker encodes CFR, offset-start, B-pyramid, VFR and `frame_mbs_only_flag` 0 Originals, and every proxy picture has the Original's exact PTS and duration and is intra. **Interpretation:** 4K is downscaled to 1920×1080 by policy, and SAR 4:3 with a BT.601 matrix, Display P3 with sRGB, and a rotated Original keep their interpretation with per-channel bias under 1.5; a 4×2 RGB picture is refused by fidelity. **Rejections:** sidecars for other bytes, indexes, recipes or a tinted bias are refused; at or below 1080p nothing gets a proxy. **Stall and retry:** a stub worker hangs then delegates (two spawns, output only from the second run, the first staging removed, no surviving process); a second stall is reported, not looped; an `invalid_packet` refusal is retried once and two are reported. **Control:** a paused build is suspended without a stall; cancellation; a wrong plan or picture count is refused. |
| `crates/deadpan-media` conversion tests | Stall detection with confirmed teardown; a suspended group is resumed, not reported as stalled. |
| `crates/deadpan-cli/src/proxy/cache_tests.rs` (6 tests) | Staging is invisible until publication; replacement swaps atomically while a reader keeps the old movie; hashing happens once per file state and changed bytes are damaged; symbolic links are never followed; cleanup handles grace, retention, readers in use, budget, staging and stale recipes; failures are remembered across handles until published or forgotten; a replaced cache directory is never written. |
| `crates/deadpan-app/src/worker/proxy_tests.rs` | The first seek is exact and opens the proxy while idle. Later, a seek shows the proxy and then the bit-exact Original picture for the same ticket. Forward and backward steps from a refined picture are exact at once, with nothing to refine. A newer seek cancels refinement, and a seek that follows another within 150 ms waits the full rest before refining. Playback and thumbnails stay exact. A damaged entry falls back silently and a rebuilt one is used. |
| `crates/deadpan-app/src/presentation/tests.rs` | The proxy tier is a distinct identity, and a failed refinement keeps the proxy displayed with its error. |
| `proxy-seek` replay | The job decides "not needed" for the small fixture; `:proxies off` and `on` change the setting without an edit; the Proxy chip is painted and accessible while proxy pixels show; Camera's gate refuses them; the exact picture replaces them with the chip gone. |
| `perf seek`, `perf proxy-build` | Build, opening, cold, warm and refined seeks, and playback and edit latency during a build; see [the qualification record](qualification/proxy-2026-10-05.md). |

## Limits

- Proxy generation is not resumable by completed ranges (§16.4): a cancelled
  build restarts. Partial files are never treated as ready.
- One proxy recipe and one picture stream per Original. Compatibility
  projects with several videos get no proxies.
- Playback does not use proxies; 4K30 playback already meets its budget
  sequentially.
- Power and thermal state come from `pmset` text, read every 30 s while a
  build runs. A state that `pmset` does not report is not detected.
- A `pmset`-less or HOME-less environment, or an unsafe cache directory,
  disables proxies with an "unavailable" state.
- A future HDR or 10-bit interpretation needs a new recipe: the source adapter
  admits only eight-bit SDR today.
