# Preview proxies

A preview proxy is a rebuildable, reduced-raster, intra-only copy of the
Original's picture stream. The main viewer uses it for stopped seeks, so a
random jump decodes one small picture instead of a long-GOP preroll. When the
cursor rests, the exact Original picture replaces it. During playback it
serves the pictures whose exact decode would be late, such as the jump at a
cut or Repeat restart, while the Original decoder repositions ahead
([playback pictures](#playback-pictures)). Specification §16.4
calls for this tier, and DP-16 tracks it. Proxies are never authoritative:
export, render, verification, AI conditioning, tracking, shot analysis and
thumbnails never read them.

Fractional clean-aperture Originals keep their complete backing raster in the
proxy. Its own source metadata must declare no crop. After cache/receipt
verification, the private viewer reader scales the Original's exact clean
rectangle to the proxy dimensions before upload. The shared renderer then uses
the same clean-image proportions and source-percent coordinates for either tier.
This does not change authored media, source clocks or the cache recipe.

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
   pixel and picture, about six times the measured rate, so it also covers
   the completed ranges and the joined movie that exist together briefly.
   The cache budget, which counts partial build state, is checked first,
   with a cleanup if needed. Both the cache volume and the temporary volume
   (for the Original snapshot) must keep 2 GiB free beyond their share.
3. **Snapshot.** It copies and verifies the Original into a private snapshot,
   takes the encoder slot (step 6) and locks this Original's partial build
   state (see *Resuming*).
4. **Encoding by ranges.** `proxy_segments` divides the Original into
   contiguous ranges of pictures: a range ends at the first keyframe at or
   after `PROXY_SEGMENT_PICTURES` (600) pictures, so every range starts at a
   keyframe, and there are at most 1,024 ranges. For each range the journal
   does not already hold, the builder runs `deadpan-media-worker proxy
   JSON` (protocol 2). The worker is process-isolated through
   `deadpan_native_process::spawn`, receives only descriptors (stdin:
   Original snapshot, stdout: the partial state's private 0600 ranges file,
   stderr: heartbeats and one bounded JSON reply), lowers its own QoS to
   utility and decodes the Original with four codec threads.
   - The request is versioned, bounded and validated on both sides. It names
     the range's first ordinal, the keyframe PTS to seek to, and the exact
     PTS of its first picture and of its end, and the output offset, which
     must be the ranges file's current length.
   - The worker seeks to the keyframe, decodes metadata only until the
     range's first picture, and encodes exactly the range's pictures into a
     complete MP4 written at that offset. It refuses a range that does not
     start or end where planned.
   - Before replying, the worker reads its movie back through the MP4
     demuxer: every packet must be an intra picture at exactly the next
     expected time, and it reports the SHA-256 of the decoder configuration
     (`avcC`).
   - The output cap is three quarters of a byte per proxy pixel and picture
     of the range plus 16 MiB, half the raw 4:2:0 size.
   - The host enforces the deadline, cancellation, the output cap and clean
     group teardown, as for every other worker mode.
   - After a range succeeds, the host synchronizes the ranges file, hashes
     the range's bytes (BLAKE3) and records it in the journal.
5. **Stall watch.** The worker writes a newline heartbeat on its control
   pipe after opening its decoder and after each accepted picture, at most
   every 250 ms. The host strips heartbeats before the reply. If neither
   output growth nor a heartbeat happens for `PROXY_STALL_TIMEOUT` (60 s,
   configurable through `ProxyEncodeOptions`), the host stops the worker's
   process group and reaps its leader. It reports `Stalled` with whether
   that teardown was confirmed.
6. **Retry.** `encode_proxy_retrying` retries a range once, in a new process
   with the failed attempt's bytes truncated away and the remaining
   deadline, after either:
   - a stall whose teardown was confirmed, or
   - an `invalid_packet` refusal, where VideoToolbox emitted a packet outside
     the one-IDR-per-picture contract and the worker exited normally.

   A failed VideoToolbox session (`encoder_session_failed`) is retried the
   same way. An unconfirmed
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
   finishes. Between ranges the builder waits before starting the next one.
8. **Assembly.** `deadpan-media-worker proxy-assemble JSON` joins every
   range, in picture order, into one MP4 in a fresh private staging file,
   at packet level without re-encoding (stdin: the ranges file, read only).
   - The request lists each range's offset, length, picture count and exact
     start and end PTS; ranges must join without gap or overlap and cover
     every picture. It is bounded at 256 KiB.
   - Every packet must be an intra picture at exactly the next expected
     time of its range, and every range must carry byte-identical decoder
     configuration; otherwise the worker fails (`segment_mismatch` for a
     configuration).
   - The stream takes the recipe's interpretation exactly as a single
     encoding writes it: the Original's sample aspect, its display matrix,
     limited BT.709 tags with the Original's transfer and primaries, and
     track and movie timescales equal to the Original's time-base
     denominator. DTS equals PTS.
   - The worker reads the joined movie back and checks every picture, the
     whole span and the configuration again.

   Before assembly the builder compares the recorded configuration
   digests. If ranges differ, it encodes the minority again; if they still
   differ, it encodes every range again; a third disagreement fails the
   build.
9. **Verification.** `verify_proxy` reads the staged file in place, without
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
10. **Publication.** It publishes the movie and its sidecar atomically, then
   removes the partial build state.

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
budget, a worker stopped by SIGKILL or SIGTERM from outside) is remembered
for that Original and recipe, so reopening the project does not repeat a
doomed build. Remembering it also removes the partial build state.

### Resuming

Specification §16.4 asks that proxy generation can be cancelled and resumed
by completed ranges, and that partial files are never ready assets.

- **Journal.** The partial state's `ranges.json` (schema 1) names the
  recipe, worker protocol, encoder, the Original's BLAKE3, SHA-256, length
  and stream, the proxy raster, the picture count and the range target. Per
  completed range it records the first and end ordinals, the offset and
  length in the ranges file, the BLAKE3 of those bytes and the decoder
  configuration digest. It is replaced atomically: a temporary file is
  written and synchronized, renamed over the journal, and the directory is
  synchronized. A range is recorded only after its bytes are synchronized.
- **Resume.** A build first validates the journal. A journal that does not
  parse, or names other Original bytes, recipe, protocol, encoder, raster,
  picture count or range target, is discarded with all its ranges. Each
  recorded range must be one of the planned ranges, lie inside the ranges
  file, not overlap another, and rehash to its recorded BLAKE3 and length;
  any that fails is dropped. The ranges file is truncated to the end of the
  last surviving range, which removes an interrupted range's bytes, and the
  journal is rewritten. Only missing ranges are encoded. A torn or
  unrecorded range is therefore encoded again and never trusted.
- **What keeps ranges.** Cancellation (the Jobs panel, a new session, app
  exit), pause, space or budget refusal, a repeated stall or VideoToolbox
  failure, a worker stopped from outside, and a killed host all keep the
  recorded ranges. `:proxies retry` and the next automatic build resume
  them. Publication or a remembered failure (including the six-hour build
  deadline) removes them.
- **A killed host.** The partial state's lock is an `flock` on the ranges
  file's open file description, which each worker inherits as its output.
  A worker that outlives its host keeps the state locked, so a new build
  waits for it rather than writing beside it. The worker notices the closed
  control pipe at its next heartbeat (at most one picture plus 250 ms) and
  exits. A worker suspended for a pause gets SIGHUP and SIGCONT when its
  process group is orphaned.
- **Progress.** `BuildControl::progress` reports ranges and pictures
  planned, reused and encoded. The app shows them in the Jobs panel.
- **Cost.** On a 40 s 4K30 Original (1,200 pictures, keyframes every 60) on
  the shared development machine (load average about 7.5), release builds:
  one range 29.9 and 31.0 s, two ranges of 600 pictures 30.2 and 31.2 s, ten
  ranges of 120 pictures 31.9 s, so each range adds about 0.1 s including
  assembly. Cancelled after the first 600 pictures and resumed, the second
  build took 20.6 and 25.5 s instead of about 30 s. Snapshot, verification
  and publication are not resumable and take the same time in every build
  (`proxy_resume` `measure_range_overhead_and_resume`).

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
  until `:proxies retry`. Its completed ranges are kept.
- **Retry.** `:proxies retry` forgets a remembered failure and builds again,
  continuing from any completed ranges. The next opening of the project
  resumes a build that a new session or app exit interrupted.
- **Status.** The Original card shows "preparing seek proxy", "seek proxy
  paused", "seek proxy failed" or "no seek proxy", with the reason on hover.
  If no proxy exists, seeking simply uses the Original. Nothing waits for a
  proxy. The Jobs panel shows the share of pictures encoded and how many
  ranges an earlier build left.

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
  .partial/v1-<Original BLAKE3>-s<stream>/   # 0700, an unpublished build's ranges
    segments.bin                 # 0600 range movies; its flock marks the build
    ranges.json                  # 0600 journal of completed ranges
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
- **Partial build state.** Completed ranges live under `.partial`, keyed
  like entries, never under an entry name; lookup, opening and usage of
  entries never read them as a proxy. The builder creates and locks the
  directory under the shared cache lock, with `O_NOFOLLOW` and owner and
  mode checks; a symbolic link in its place is refused. While a build or
  any of its workers holds the ranges file's `flock`, no other build,
  removal or cleanup touches the state, in this or any other process.
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
  - partial build state of other recipe versions, of keys whose proxy is
    already published, and unused for 7 days (`DEFAULT_PARTIAL_GRACE`);
  - the least recently used entries and partial build states while the
    cache, counting both, exceeds its 64 GiB budget.

  The current Original's entry and partial build state are retained (its
  partial state goes when its proxy is published or its failure is
  remembered). An entry whose movie a reader holds (an exclusive
  non-blocking `flock` fails) is never removed, nor a partial state whose
  build holds its lock, in this or any other process. Explicit
  [cache cleanup](STORAGE.md) (`cache clean`, `:storage` then C) runs the
  same cleanup with a one-day unused, staging and partial grace; it removes
  entries and partial states but never reads proxy pictures.
- **Safety.** Deleting the directory at any time loses only time.

## Preview reader

The only proxy picture reader is the app's private `worker/proxy.rs`. The
main viewer's `PreviewWorker::new` may serve a committed picture
(`Work::Project`) from the proxy: a stopped one first and refined after a
rest, as below, and a playback one when the Original cannot deliver it in
time ([playback pictures](#playback-pictures)):

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

Proposals, edited slices, copied views, AI candidates and the card
thumbnail worker (`PreviewWorker::named`) always decode the Original.

### Playback pictures

During playback (`ticket.transport` set) the audio device's reported clock
chooses each picture, one in flight, the newest heard frame winning, exactly
as without a proxy ([audition](PLAYBACK.md#audio-clock-and-pictures)). Only
the pixels of a requested picture can differ: the proxy picture has the same
Original ordinal, PTS and duration and is a distinct presentation identity.
Timing, audio, the heard clock and the requested frames never depend on it.

- **Plan.** `SourceSession::decode_plan` says how the retained Original
  decoder would reach the picture: its current picture, forward decoding
  (`Forward`, every picture decoded; chosen for the next picture or when it
  passes at most half the ordinals of a seek), finishing an unfinished
  reposition (`Resume`) or a keyframe `Seek` (non-reference preroll skipped).
  `frame` follows the same plan, so forward decoding past dropped pictures no
  longer seeks.
- **Choice.** `deadpan_media::playback_pictures::PlaybackPictures` keeps
  per-decoder moving averages of the measured cost of a forward picture
  (decode and conversion), of a seek or reposition per ordinal (only steps
  of at least four ordinals; shorter ones measure a keyframe picture, not
  preroll) and of a proxy picture. A picture is exact when its predicted
  cost fits 70% of the picture period (`EXACT_BUDGET`; the rest is upload,
  composition and the viewer's frame), otherwise the proxy serves it.
  Unmeasured, a forward step is tried exactly and a seek of more than one
  ordinal is not: only long-GOP Originals above 1080p have a proxy. Without
  an open proxy every picture is exact (and may be late, as before). A
  picture whose decoder must first reopen (after a failure or cancellation)
  is not recorded, since the reopen dominates its cost.
- **Repositioning.** After a proxy picture the worker aims the Original
  decoder at the earliest picture it can reach before playback does: with
  the time left between proxy pictures (80% of it, `REPOSITION_SHARE`) and
  the measured preroll cost, a lead `L` with
  `(behind + L) × preroll ≤ L × useful time per ordinal`, or the next
  keyframe when that comes sooner, within four seconds
  (`MAX_REPOSITION_LEAD`). Until preroll is measured it is assumed to cost a
  quarter of a forward picture per ordinal. A decoder already standing or
  moving ahead is kept when playback reaches it no later. No target exists
  when measured forward decoding itself exceeds the budget: then the
  Original cannot keep up even sequentially and the proxy serves the rest
  of the playback. Between requests the worker decodes the reposition one
  picture at a time (`begin_reposition`, `advance_reposition`), checking the
  mailbox before each picture: a new request waits at most one preroll
  picture, and a decode is never cancelled midway (that would reopen the
  decoder). Starting a reposition (the keyframe seek and flush) is one more
  uninterruptible call. The lead assumes playback advances one Original
  picture per Original picture duration (unity speed); a Retime or Hold
  changes how soon playback reaches the target, not which pixels a
  picture shows. Every decoded picture is checked against the receipt index, as
  for a seek. When playback reaches the target the picture is exact again.
- **Stop.** Playback never refines its own proxy pictures. When the
  transport stops (pause, end, a command or inspector stop) and the
  displayed picture is a proxy picture of the current view (and, for the
  Original, of the selected video), the viewer requests that frame's
  stopped picture, which is exact or, after a seek, the proxy refined to the
  exact picture after the rest. A pause requests its own stopped picture
  instead. The proxy chip and the "· proxy preview"
  label show while proxy pixels are on screen during playback too.
- **Look-ahead first.** Before the tier is chosen, a companion decoder
  already positioned at a cut serves it exactly
  ([decode-ahead](PLAYBACK.md#audio-clock-and-pictures)); the proxy serves
  only pictures the companion did not reach in time.
- **Opening.** Playback never opens a proxy; one opened for an earlier
  stopped seek is used. A project opened straight into playback plays
  exact pictures until a stopped seek opens the proxy.
- **Counters.** `deadpan_diagnostics::PLAYBACK_PICTURES` counts playback
  pictures requested, frames skipped (dropped) by the transport, pictures
  served exact and from the proxy, and repositions started, reached and
  failed (a failure leaves the decoder to reopen on its next picture); the
  `:diagnostics` panel shows them under PLAYBACK.

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
| `native/deadpan-media-worker/tests/proxy_real_media.rs` (12 tests) | **Timing:** the real worker encodes CFR, offset-start, B-pyramid, VFR and `frame_mbs_only_flag` 0 Originals, and every proxy picture has the Original's exact PTS and duration and is intra. **Interpretation:** 4K is downscaled to 1920×1080 by policy, and SAR 4:3 with a BT.601 matrix, Display P3 with sRGB, and a rotated Original keep their interpretation with per-channel bias under 1.5; a 4×2 RGB picture is refused by fidelity. **Rejections:** sidecars for other bytes, indexes, recipes or a tinted bias are refused; at or below 1080p nothing gets a proxy. **Stall and retry:** a stub worker hangs then delegates (two spawns, output only from the second run, the first staging removed, no surviving process); a second stall is reported, not looped; an `invalid_packet` refusal is retried once and two are reported. **Control:** a paused build is suspended without a stall; cancellation; a wrong plan or picture count is refused. **Ranges:** CFR, offset-start, VFR and B-pyramid Originals, and rotated, anamorphic BT.601 and Display P3 ones, encoded in two to four keyframe-aligned ranges stored out of picture order in one file, joined by the worker and verified like a single encoding, with every picture intra at its exact time and the interpretation kept; an assembly of swapped or truncated ranges is refused. |
| `crates/deadpan-media` conversion tests | Stall detection with confirmed teardown; a suspended group is resumed, not reported as stalled. |
| `crates/deadpan-cli/src/proxy/cache_tests.rs` (9 tests) | Staging is invisible until publication; replacement swaps atomically while a reader keeps the old movie; hashing happens once per file state and changed bytes are damaged; symbolic links are never followed; cleanup handles grace, retention, readers in use, budget, staging and stale recipes; failures are remembered across handles until published or forgotten; a replaced cache directory is never written. **Partial state:** it is private (0700/0600), never an entry and counted in usage; the journal is replaced atomically; a second build waits and cancels; a duplicate of the ranges descriptor, as a worker holds it, keeps the lock after the build's handle is gone; cleanup removes other recipes', published keys', aged and over-budget partial states, keeps the retained and held ones, and removes a symbolic link without following it. |
| `crates/deadpan-cli/tests/proxy_resume.rs` (3 tests and a host helper; an ignored measurement) | Through `build_subject` and the real worker, eight 15-picture ranges of `cfr-bframes.mp4`. **Killed worker:** a worker SIGKILLed after a journaled range ends the build as `worker_terminated`, a machine condition: nothing remembered or published, ranges kept; the next build reuses exactly the journaled ranges, encodes only the rest, publishes a proxy whose every picture is a keyframe at the Original's exact PTS and duration, and removes the partial state. **Killed host:** the test re-executes itself as a host building into the same private cache and SIGKILLs it while a worker runs after a journaled range; the orphaned worker exits by itself; a build in the test process reuses the journaled ranges, encodes the rest, verifies and removes the partial state. **Damage and identity:** cancellation keeps ranges and cleanup retains them; a flipped byte inside a range, a torn tail, truncation into the last range and a torn journal are dropped and encoded again; a journal naming other Original bytes or length, another recipe, encoder or raster, or another range target reuses nothing; a remembered failure removes the partial state; a final build from nothing verifies. |
| `crates/deadpan-app/src/worker/proxy_tests.rs` | The first seek is exact and opens the proxy while idle. Later, a seek shows the proxy and then the bit-exact Original picture for the same ticket. Forward and backward steps from a refined picture are exact at once, with nothing to refine. A newer seek cancels refinement, and a seek that follows another within 150 ms waits the full rest before refining. Playback pictures whose exact decode fits the budget and thumbnails stay exact. A damaged entry falls back silently and a rebuilt one is used. **Playback:** with seeks measured slower than a picture period (a test-only cost scale), the next picture is exact, a jump is served from the proxy with the Original's PTS and is never refined, the decoder repositions to the next keyframe meanwhile, the picture there is exact and bit-identical to an independent decode, and the presentation reports the displayed proxy view a stopping transport replaces. |
| `crates/deadpan-media/tests/source_session.rs` (`forward_continuation_and_repositioning_return_sequential_pictures`) | With one and eight codec threads on CFR, B-pyramid, offset-start and VFR fixtures: forward continuation past skipped pictures, a reposition advanced one picture per call, finishing an unfinished reposition through a later request and a seek before an unfinished target all return pictures identical to a sequential single-threaded decode; advancing without a reposition is refused. |
| `deadpan_media::playback_pictures` unit tests (10) | Exact versus proxy choices against the budget, unmeasured defaults, an Original that cannot keep up sequentially, reposition targets (in-group lead, sooner keyframe, slow seeks, lead limit, unmeasured forward or preroll), short seeks not setting the preroll cost, and keeping a decoder close ahead but repositioning one a Repeat restart left behind. |
| `perf playback --pictures adaptive`, `worker::project_tests::stress_tests` | Real-device playback with the policy on plain, cut and 10,000-beat projects, beside a proxy build and an export, and a seek storm and jumpy playback through the real preview worker; see [the qualification record](qualification/playback-proxy-2026-10-06.md). |
| `crates/deadpan-app/src/presentation/tests.rs` | The proxy tier is a distinct identity, and a failed refinement keeps the proxy displayed with its error. |
| `proxy-seek` replay | The job decides "not needed" for the small fixture; `:proxies off` and `on` change the setting without an edit; the Proxy chip is painted and accessible while proxy pixels show; Camera's gate refuses them; the exact picture replaces them with the chip gone. |
| `perf seek`, `perf proxy-build` | Build, opening, cold, warm and refined seeks, and playback and edit latency during a build; see [the qualification record](qualification/proxy-2026-10-05.md). |

## Limits

- Resumption is by whole ranges: an interrupted range is encoded again from
  its start. An Original whose only keyframe is its first picture has one
  range, so its build is not resumable. The Original snapshot,
  verification and publication are repeated by every build.
- Ranges are reused only by a build with the same range target, recipe,
  worker protocol and encoder name. An operating-system update that changes
  VideoToolbox's decoder configuration makes old ranges disagree; they are
  then encoded again.
- A worker that outlives a killed host while hung in VideoToolbox keeps
  the partial state locked; the next build waits for it (cancellably) and
  does not start beside it.
- One proxy recipe and one picture stream per Original. Compatibility
  projects with several videos get no proxies.
- Playback chooses tiers from costs measured on this machine and this
  decoder; the first pictures after opening use defaults. Picture choice is
  per request: a proxy picture is shown whole, never mixed with exact
  regions. The budget is a fixed share of the picture period, not a measured
  display deadline, and the window, compositor and scanout are not part of
  any measurement.
- Power and thermal state come from `pmset` text, read every 30 s while a
  build runs. A state that `pmset` does not report is not detected.
- A `pmset`-less or HOME-less environment, or an unsafe cache directory,
  disables proxies with an "unavailable" state.
- A future HDR or 10-bit interpretation needs a new recipe: the source adapter
  admits only eight-bit SDR today.
