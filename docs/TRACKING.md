# Selected-target tracking

Deadpan tracks a user-selected region through the Original's pictures with
Apple Vision's object tracker and can save the result as an
[attention target](TARGETS.md). Tracking itself never edits the project; saving
is an ordinary reversible `SetTarget` edit. Section 11.3 of the
[specification](spec/DEADPAN_SPEC.md) is normative; this records the implemented
boundary. Requirement DP-11 remains Partial (see [requirements](REQUIREMENTS.md)). The
native app drives the same host code ([in the app](#in-the-app)).

## Pieces

| Piece | Responsibility |
| --- | --- |
| [`deadpan_analysis::tracking`](../crates/deadpan-analysis/src/tracking.rs) | Pure, serde-serializable `TrackedPath` / `TrackSample` / `TrackState`, `NormalizedRect` with rotation mapping, the versioned `deadpan-track-1` policy, shot-boundary range ends, all-or-nothing corrections and range-local re-tracking. No I/O. |
| [`deadpan_analysis::tracking` mapping](../crates/deadpan-analysis/src/tracking/target.rs) | `TrackedPath::to_target` and `retrack_target`: the exact mapping to a core `AttentionTarget`, compaction to its bounds, provenance. |
| [`deadpan_jobs::tracking`](../crates/deadpan-jobs/src/tracking.rs) | Versioned, strictly framed worker protocol and the `TrackingProtocol` adapter for the shared `SupervisedProcess`. |
| [`deadpan-track`](../native/deadpan-track/) | Process-isolated worker executable: pinned descriptor-only FFmpeg decoding through `deadpan-source`, Vision `VNTrackObjectRequest` through `objc2-vision`; with the argument `detect-faces`, `VNDetectFaceRectanglesRequest` on one picture ([face detection mode](#face-detection-mode)). |
| [`deadpan_jobs::faces`](../crates/deadpan-jobs/src/faces.rs) | The separately versioned face-detection protocol and its `FaceProtocol` adapter. |
| [`deadpan_cli::faces`](../crates/deadpan-cli/src/faces.rs) | Host face detection (picture resolution, verified copy, supervision, strict admission), `face_target` and the `detect-faces` command. |
| [`deadpan_cli::tracking`](../crates/deadpan-cli/src/tracking.rs) | Host attempt (range resolution against the qualified index, the asset's video span and stored shots; verified source copy; supervision; artifact snapshot; exact decoded/observation check; policy), saving and correcting targets, and the `track` / `track-correct` commands. |

## Coordinates and time

Regions are `x, y, width, height` normalized to the *displayed* picture with a
top-left origin, every edge in `[0, 1]`. The worker converts the region to coded
orientation (`coded_from_displayed`, undoing the stream's clockwise quarter
turns), then to Vision's lower-left origin, and converts Vision's boxes back the
same way, clipping any part outside the picture. Sample aspect ratio scales each
axis uniformly, so it does not change normalized coordinates; it does change
distances, so the policy measures them in the display aspect (below).
Deserialized regions are checked, not adjusted, so stored paths round-trip.

Every sample carries the source picture's exact PTS in the qualified stream's
time base. A path covers the half-open PTS range `[start_pts, end_pts)`. The
host picks pictures from the Original's qualified index: the first is the
picture displayed at `--from` (the last picture whose PTS is at or before it).
The range ends before `--to`, clamped to the end of the asset's measured video
span, so a saved target always lies inside its asset. Decoded pictures and
observations must match the indexed pictures exactly; anything else is a
protocol failure.

## Policy `deadpan-track-1`

Constants (all in `TrackPolicy::default()`):

| Constant | Value | Meaning |
| --- | --- | --- |
| `MIN_CONFIDENCE` | 0.3 | Lowest accepted Vision confidence. |
| `MAX_INTERPOLATED_GAP` | 6 pictures | Longest gap between confident neighbours that may be interpolated, counted in pictures whatever the stride. |
| `MAX_CENTER_STEP` | 0.1 per elapsed picture | Largest center movement, as a fraction of the displayed picture's longer side. |
| `MAX_CENTER_JUMP` | 0.35 | Cap on center movement however many pictures elapsed, same unit. |
| `MAX_AREA_RATIO` | 2.0 | Largest growth or shrink factor of the box area. |
| `MAX_TRACK_PICTURES` | 36,000 | Pictures one attempt may decode (25 minutes at 24 fps). |
| `MAX_TRACK_STRIDE` | 30 | Largest analysed-picture stride. |
| `MAX_TRACK_KEYFRAMES` | 1,024 | Keyframes per path. |

1. The selected region is a keyframe; its picture is `manual` and the
   tracker's observation there is ignored.
2. An observation is accepted when it has a region, its confidence is at
   least 0.3, and it is plausible against the last accepted or manual region:
   the center moves at most 0.1 of the displayed longer side per elapsed
   picture (so a stride scales the allowance), never more than 0.35, and the
   area changes by at most a factor of two. Distances scale normalized x by
   `min(aspect, 1)` and y by `min(1/aspect, 1)`, where `aspect` is the displayed
   width over height after sample aspect ratio and rotation. Accepted
   observations are `tracked` with Vision's confidence. A confident implausible
   jump is a loss, never a new subject.
3. Rejected observations between two accepted ones at most seven pictures
   apart (a gap of at most six pictures) are `interpolated`, linearly in x, y,
   width and height by picture ordinal. Their confidence is the lower of the
   two neighbours' (a manual neighbour counts as 1), never the rejected
   observation's. With a stride above 7 nothing can be interpolated.
4. A longer gap, or one no accepted observation closes, is `lost` at the last
   confident region, without confidence. Tracking then does not resume
   automatically: later observations are `lost` when they would still be
   rejected and `held` when Vision reports a confident, plausible box, both at
   the last confident region without confidence, until a manual keyframe.
5. `TrackedPath::correct` inserts or replaces a keyframe and returns the range
   it governs (to the next keyframe or the path's end); its samples hold the
   corrected position. `correct_and_retrack` also re-tracks exactly that range.
   Both are all or nothing: on any error the path is unchanged. The host's
   `correct` runs the worker over that range first and changes the path only
   when the run and the policy succeed, so a failed or cancelled correction
   never looks like a loss.

Range ends (`tracking_end`): the first stored shot boundary after the start
picture stops tracking by default (`stop.reason = "shot_boundary"`). Without a
stored shot analysis the command refuses rather than tracking through cuts;
`--through-shots` disables the stop explicitly. A range longer than the picture
bound stops with `picture_limit`. The shot rule is
[`deadpan-shots-1`](../crates/deadpan-analysis/src/shots.rs) and does not detect
dissolves.

## Saving as an attention target

`TrackedPath::to_target(label, asset, time_base, engine, budget)` produces a
core `AttentionTarget` over `[start_pts, end_pts)` of the asset:

* The first keyframe is `region`; later keyframes are `corrections`. Manual
  samples are not stored as samples.
* `tracked` and `interpolated` keep their state; `lost` and `held` both become
  core `lost`, which holds the last confident position.
* Regions become center and size in millionths of the displayed picture, each
  rounded to the nearest millionth with ties away from zero, centers clamped to
  `0..=1,000,000` and sizes to `1..=1,000,000`.
* Confidence becomes thousandths the same way, clamped to `0..=1000`; absent
  confidence (lost, held) stores 0.
* Compaction fits core bounds (4,096 samples per target and what the project's
  32,768-sample total leaves beside its other targets). Core `region_at`
  interpolates linearly between consecutive samples of the same moving state
  with no correction between them ([targets](TARGETS.md)), so inside such a run
  a sample is dropped when every center and size component lies within the
  tolerance of the straight line between the kept samples around it (core
  rounds that line to a millionth, adding at most one more). Inside a run of
  `lost` samples holding one region every sample after the first is dropped.
  The first and last sample of every run are kept, and one kept segment spans
  at most 256 samples. Tolerances 500, 1,000, 2,000, 4,000, 8,000 and 16,000
  millionths are tried in order; if none fits, saving fails with a limit
  error. Dropped samples' confidences are not retained. The tolerance used is
  reported.
* `provenance` records `rule` (core `TargetRule`, `deadpan-track-1`), `engine`
  (for example `Apple Vision VNTrackObjectRequest 2 accurate`, the latest run
  over any of the samples) and `stop` (core `TargetStop`: `range_end`,
  `shot_boundary` or `picture_limit`, why the first run ended where the span
  ends). Policy constants are fixed by the rule identity; the stride and
  runtime timings are reported by the command, not stored.

`retrack_target(target, segment, budget)` applies a correction to a saved
target: the segment must be tracked from the correction over exactly its
`correction_range`. Samples outside the range and other corrections are kept;
the correction is inserted (or replaces one at the same time). Provenance
takes the re-track's engine and keeps the stop reason, because the span is
unchanged. Where the old
target interpolated from its last sample before the correction into the
replaced range, a sample one tick before the correction ends that line, so
pictures before the correction keep their region to within one millionth.

Property tests check that saved targets reproduce every path sample through
core `region_at` within the reported tolerance plus one millionth, keyframes
exactly, and that a correction at any picture changes nothing before it beyond
that rounding and yields the corrected region throughout its range.

## Worker protocol

Protocol version 1. The host creates a fresh attempt workspace, copies the
Original's retained bytes to `input/source` while hashing them, and admits the
copy only when its length and SHA-256 equal the qualified index's content
identity. The store's verified snapshot exposes no descriptor, so this is one
full copy per attempt; an APFS clone would need a store API. For each run it
validates the request, attempt and cancellation identities before creating a
per-run output scope `output/<attempt>`, then sends one `Track` message: the
source artifact (hash and length), the expected stream (index, coded size, raw
time base, rotation), `start_pts`, exclusive `end_pts`, the exact number of
indexed pictures in the range, the stride, the region, the scope, a byte budget
(24 bytes per decoded picture plus 256 per analysed picture, at least 4,096 and
at most 10,080,000 bytes) and a timeout. Messages are bounded by the shared
256 KiB frame limit.

The worker opens the source below `input/` without following links, checks its
length and SHA-256 again in 1 MiB chunks (observing cancellation and the
deadline between them), opens it with the pinned `SourceDecoder` (container
admission, closed codec grammar, default decode limits), and refuses a
different stream, size, time base or rotation. It seeks to `start_pts`, decodes
metadata only up to it, requires a picture exactly there, then decodes exactly
the declared number of pictures before `end_pts` in strictly increasing PTS,
recording every one. Every `stride`-th picture is converted to RGBA, copied into
an owned BGRA `CVPixelBuffer` and given to one `VNTrackObjectRequest`
(revision reported by Vision, tracking level `accurate`) on one
`VNSequenceRequestHandler`; each result is fed back as the next input
observation, and the final analysed picture's request is marked as the last
frame before it runs. A Vision error or empty result records the picture without
a region; after 30 consecutive failures Vision is not asked again. The worker
writes a `RawTrack { decoded, observations }` JSON object to
`output/<attempt>/observations.json`, created exclusively, and reports
`Completed` with decoded/analysed counts and decode and Vision times.

`Completed` must name an artifact below the attempt's scope, within budget, with
exactly the requested counts. The host admits it only after clean process
teardown, a contained hashed snapshot, bounded parsing, and an exact match of
every decoded PTS and every observation PTS against its own index selection,
then applies the policy. Progress is a monotonic percentage. Cancellation sends
`Cancel` and waits for the worker to stop; the worker checks it between
pictures and inside each decoder call, and end of stdin also cancels. At the
deadline the host requests cancellation and drains the process to a stopped
receipt (bounded by 5 seconds) before the workspace is removed; unconfirmed
cleanup is reported as a worker failure. The environment is empty, stdout
carries only framed messages, and the supervisor keeps a bounded stderr tail.
All `unsafe` is in
[`native/deadpan-track/src/vision.rs`](../native/deadpan-track/src/vision.rs),
one documented block per framework call; the rest of the worker denies it.

## Face detection mode

Face proposals (specification §6.4 `target=face:2`, §7.6 numbered detected
regions) reuse this worker and its isolation, not its tracking policy. The host
launches `deadpan-track detect-faces` (the only accepted argument; any other
argument list exits 2) and speaks a separate protocol,
[`deadpan_jobs::faces`](../crates/deadpan-jobs/src/faces.rs) version 1, over the
same 256 KiB length-framed transport: one `DetectFaces` message names the
verified source copy below `input/` (copied exactly as for tracking, by the
shared `copy_verified_original`), the expected stream and the exact PTS of one
indexed picture, with a timeout of at most one hour; `Cancel` with the
attempt's token, or end of stdin, cancels. The worker re-verifies length and
SHA-256, opens the pinned decoder, seeks, decodes metadata up to the picture
(which must exist at exactly that PTS), converts only that picture to BGRA and
runs one `VNDetectFaceRectanglesRequest` on a fresh `VNImageRequestHandler`
(revision as Vision reports it: 3 on macOS 26). More than 64 observations is
an error, never a truncation. Boxes are mapped as tracking maps them (lower-left
to top-left origin, coded to displayed orientation, clipped to the picture,
discarded below the minimum extent), confidences clamped to `[0, 1]`, exact
duplicates dropped, and sorted by left edge, then top edge, then size. The
faces travel inline in `Completed { pts, faces, runtime, timings }`; there is
no artifact.

The host's `FaceProtocol` requires the attempt's identity and the requested
PTS; `admit_faces` requires at most 64 valid rectangles (the same checked
`NormalizedRect` deserialization), finite confidences in `[0, 1]` and a
strictly increasing order, so face numbers are deterministic. Every face source,
including the app's test seam, passes this admission. The picture is the one
displayed at the requested PTS (the last indexed picture at or before it),
inside the asset's measured video span; the receipt must be the one the head
binds. Detection opens the project read-only and never edits.

```sh
deadpan-cli detect-faces <project.deadpan> --at <pts> [--asset <id>]
```

prints the content identity, asset, head revision, stream, analysed PTS and
picture ordinal, runtime and timings, and `faces` numbered from 1 with region
and confidence. Errors use `FaceDetectionUnavailable`, `FaceDetectionCancelled`,
`FaceDetectionFailed` and `InvalidInput`. Saving a face is the app's
`target=face:N` command ([face proposals](FRAMING.md#face-proposals)); in the
app the service runs one face job at a time on its own bounded thread, beside
the tracking job, and close, open and shutdown cancel and drain it like
tracking.

Evidence: [`native/deadpan-track/tests/faces.rs`](../native/deadpan-track/tests/faces.rs)
runs the real worker on
[`two-drawn-faces.mkv`](../native/deadpan-track/tests/fixtures/two-drawn-faces.mkv)
(11,548 bytes, SHA-256
`563904e45b81ee8c80200e222fb26470089fbe62477f77072f17e8953de1e958`, produced by
[`generate_faces_fixture.py`](../native/deadpan-track/tests/generate_faces_fixture.py)
with Pillow and the development ffmpeg 9.0.1 CLI): four 480×270 FFV1 pictures,
two shaded cartoon heads in pictures 0–1 and only the background in 2–3. These
are **drawn**, not people. On Apple M5 Max, macOS 26.5.2, Vision found both heads
in picture 0, left (confidence 0.651) before right (0.804), with centers within a
few pixels of the drawn faces, and nothing in picture 2. With release binaries,
`detect-faces --asset clip` printed the same two faces at `--at 0` and `--at 30`
(the picture displayed at 30 ms is picture 0) and none at `--at 83` (picture 2),
in 62–82 ms per command including worker launch, 47–66 ms of it in Vision
(one request per process, so setup is included) and about 1 ms decoding. The test also covers face numbering
and out-of-range refusals, the target mapping, a picture before the first one,
cancellation before launch and an expired deadline. Protocol tests cover
request bounds, identity and picture binding, ordering, duplicates, confidence
bounds and the 64-face limit.

## Commands

```sh
deadpan-cli detect-shots <project.deadpan> [--asset <id>]
deadpan-cli track <project.deadpan> --from <pts> --to <pts> --region <x,y,w,h> \
  [--asset <id>] [--stride <n>] [--through-shots] [--save <target-id> [--label <text>]]
deadpan-cli track-correct <project.deadpan> --target <id> --at <pts> \
  --region <x,y,w,h> [--stride <n>]
```

`track` tracks with the project opened read-only and prints JSON: rule, content
identity, asset, stream (index, time base, rotation, display aspect), the
shot-analysis key that bounded the range, picture and analysed counts, the
Vision runtime report, elapsed/worker/decode/Vision times, and the `TrackedPath`.
With `--save` it maps the path (label defaults to `Tracked target`) and commits
`SetTarget` through [`live_project::dispatch_short`](LIVE_PROJECT.md): the
writer when the project is closed, or the open app's authenticated endpoint.
The expected revision is the head the range was resolved against, captured
before tracking with the asset's video span, and the asset's qualification
receipt must be the one that head binds; an edit made while tracking makes the
save fail with `RevisionConflict` instead of being overwritten. An existing
target id is refused (`InvalidInput`, checked before tracking starts and again
against the captured head) unless `--replace` is given, which overwrites the
target and its corrections. The output adds `saved` with the target id, stored
sample count, compaction tolerance, whether it replaced a target, and the
commit receipt. Undo reverts the save like any edit.

`track-correct` reads the saved target and the head together, requires the
range preparation to see the same head, requires `--at` to be an indexed picture
inside its span, tracks only the correction's range (to the next correction or
the span end, without a shot stop because the span already ends there), applies
`retrack_target` and commits `SetTarget` expecting that head, so a concurrent
edit is refused with `RevisionConflict`. It prints the corrected range, counts,
runtime, tolerance and receipt.

Options are `--name value` pairs, each at most once; `--label` and
`--replace` require `--save`. SIGINT or SIGTERM cancels the attempt and a second signal exits at
once. Invalid arguments report `InvalidInput`, a missing source, target or shot
analysis `TrackingUnavailable`, cancellation `TrackingCancelled` and other
tracking failures `TrackingFailed`; commit refusals keep their store or live
project codes. The worker must be installed beside the executable
(`target/<profile>/deadpan-track`; it is a default workspace member).

## In the app

The native project service runs tracking on one bounded job thread per
project, like AI pause generation, and never on the writer, the UI or audio
threads ([service](../crates/deadpan-app/src/project/service/targets.rs)).

- Commands: in Camera, `T` tracks the picked or followed target; `:track`
  tracks the target the selected beat follows, `:track ID` or `:track Label`
  names one, and a final `through-shots` crosses cuts explicitly.
  `:track-cancel` cancels. Without `through-shots` a stored shot analysis is
  required (the app detects shots automatically); without one the job reports
  that tracking cannot stop at cuts and saves nothing. `:track` refuses a target
  that already has samples or corrections: correct it instead.
- Each command captures session and head revision at entry. The job thread
  opens the package read-only, runs `prepare_tracking` (refusing a head that
  differs from the captured one), then `track` with the installed
  `deadpan-track` worker (`TrackingRuntime::beside_current_executable`; a
  missing worker is reported as unavailable). Progress is the worker's
  percentage. The writer then maps the path with `to_target` (keeping the
  target's label) and commits `SetTarget` expecting the captured revision, so
  any edit made while tracking refuses the save, as `track --save` does.
- Corrections: `c` in Camera on a tracked target opens its rectangle at the
  displayed picture; Enter runs a job over exactly `correction_range(at)`
  (no shot stop) and saves `retrack_target` expecting the entry head. Earlier
  pictures keep their positions. An untracked target's correction is saved
  directly without tracking.
- One job runs at a time; a second start is refused. Cancellation is explicit
  and cooperative. Closing, opening another project and shutdown cancel the job
  and wait until it has drained before releasing the project. Outcomes
  (saved positions, cancelled, failed with the reason, unavailable) appear in
  the inspector; the footer shows progress. Camera stays open while tracking and
  continues on the saved revision.

Test seam: with `test` or `ui-harness`, `targets::Backend::Scripted` replaces
only the Vision worker with deterministic confident observations moving at a
fixed speed. Range resolution against the stored shots, the verified copy, the
policy, compaction and the revision-guarded save stay real. Service tests cover
the shot requirement, stops at a stored cut, the entry-head guard, single-range
corrections, a second start, worker failure, cancellation and close draining.
The `targets` replay drives creation, picking, following, tracking and
correction through the production keys.

## Evidence

[`native/deadpan-track/tests/worker.rs`](../native/deadpan-track/tests/worker.rs)
runs the real worker under the supervisor on a deterministic **synthetic**
fixture,
[`moving-square-cut.mkv`](../native/deadpan-track/tests/fixtures/moving-square-cut.mkv)
(49,178 bytes, SHA-256
`de0f4c886b453d3a3f5b3ff8c1461a0ab3cb671a71884c06cd288f82d9b453d0`, produced by
[`generate_fixture.py`](../native/deadpan-track/tests/generate_fixture.py) with
the development-only ffmpeg 9.0.1 CLI; decoded only by the pinned 8.0.3
libraries). It is 48 pictures of 320×180 at 24 fps, limited-range BT.709 FFV1:
a textured square moves over a dark texture, passes behind a pillar (fully
hidden in pictures 20–22), and a hard cut at picture 30 shows a second square on
a bright texture. `deadpan-shots-1` finds exactly the cut at picture 30.

Observed on Apple M5 Max, macOS 26.5.2, Vision `VNTrackObjectRequest`
revision 2:

* Pictures 1–14 are `tracked` within 1 px of the true center. As the square
  enters the pillar Vision's confidence falls (0.45, then 0.30, 0.16, 0.09 …,
  0.011 once hidden) and never recovers after it re-emerges; the policy marks the
  rest `lost` at the last confident position. Picture 15 measured 0.298 in one
  run and at least 0.3 in another, so Vision output is not bit-reproducible
  across runs; tests assert only the robust properties.
* Tracking stops at the cut: 30 samples, `end_pts` 1250 ms, `stop`
  `shot_boundary` at picture 30.
* With `--through-shots` and stride 2, the path never moves to the second
  square: every sample after the cut stays `lost` at the held position, and the
  range ends at the asset's video end (1999 ms), not the requested `i64::MAX`.
* A manual keyframe on the second square at picture 32 re-tracks only
  `[picture 32, end)`; samples before it are unchanged and the re-tracked samples
  are `tracked` within 1 px of the moving square.
* Saving a stride-3 path commits `SetTarget`; reloaded from SQLite, the target's
  `region_at` reproduces every path sample within the compaction tolerance, an
  unanalysed picture between strided samples lies within 3 px of the true
  center (interpolated, not stepped), and a `Follow` framing layer over it
  resolves in `RenderPlan` to the target's center at frames 0, 5 and 10 and to
  its fallback after the span. `track-correct`-equivalent re-tracking from a
  correction at picture 24 changes only `[1000, 1250)` and reloads unchanged.
* [`crates/deadpan-cli/tests/tracking.rs`](../crates/deadpan-cli/tests/tracking.rs)
  saves policy-built paths over the registered fixture: a save against a head
  that changed during tracking fails with `RevisionConflict` and writes
  nothing; a save against the current head commits; an existing id is refused
  without `--replace` (also by the `track` command before tracking) and
  replaced with it; a correction keeps every earlier sample, records the new
  engine and keeps the stop reason, and is refused against a stale head; undo
  steps back through replacement, correction and save.
* Cancellation before launch and an expired deadline return typed errors
  without a path; ranges before the first picture and a missing shot analysis
  are refused.
* With release binaries, `track --save` stored 8 samples (stride 2, tolerance
  500 millionths), `track-correct --at 1000` re-tracked 6 pictures, and
  `project undo` reverted the correction.

Measured with release binaries (`target/release`), whole 30-picture range,
including worker launch:

| Input | Stride | Analysed | Command | Worker | Decode, hash, seek, convert | Vision incl. BGRA copy |
| --- | --- | --- | --- | --- | --- | --- |
| 320×180 fixture | 1 | 30 | 100 ms | 84 ms | 9 ms | 75 ms (2.5 ms/picture) |
| 320×180 fixture | 2 | 15 | 67 ms | 52 ms | 7 ms | 44 ms (2.9 ms/picture) |
| 1920×1080 nearest-neighbour upscale | 1 | 30 | 186 ms | 175 ms | 93 ms | 82 ms (2.7 ms/picture) |
| 1920×1080 nearest-neighbour upscale | 2 | 15 | 136 ms | 125 ms | 58 ms | 67 ms (4.5 ms/picture) |

Per-picture Vision cost includes its first-picture setup, so short or strided
runs read higher. A debug-profile test run measured about 17 ms per picture,
mostly the unoptimized BGRA copy. The upscale is a measurement input only
(16-slice FFV1, scratch, not committed).

## Remaining

* Real-person footage. Specification Section 25 requires it for face/person
  tracking quality; the synthetic square proves the pipeline, coordinate
  mapping, shot stop, loss marking, persistence and correction, not tracking
  quality on people, faces, motion blur, lighting changes or partial occlusion.
* Rotated and anamorphic sources are covered by pure mapping and aspect tests
  only; no rotated real-media tracking run exists yet.
* Saving while the app holds the project goes through the authenticated live
  endpoint; that path is the shared `dispatch_short` route and is not exercised
  by these tests.
* The app's real-worker path (`deadpan-track` beside the app binary) is not
  exercised by app tests or replay, which use the scripted seam. Incremental
  scheduling by visible cursor neighbourhood (Section 11.4) remains open.
* Each attempt copies the whole Original into its workspace.
* `deadpan-jobs` depends on `deadpan-analysis` for `NormalizedRect` in the
  protocol; moving the wire type was optional and not done.
* Point targets, face proposals in Camera's numbered picker, general (non-face)
  region proposals and stabilization consumers of a path remain open. Face
  detection quality on real people (lighting, profile, occlusion, small faces)
  is unmeasured: the fixture is drawn.
