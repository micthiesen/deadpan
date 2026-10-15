# Tracking evidence: rotated and anamorphic Originals, live-endpoint saves

Date: 2026-10-15. Machine: Apple M5 Max, macOS 26.5.2 (25F84). Base revision
`20dad2b5` plus this change. Vision reported `VNTrackObjectRequest` revision 2,
level `accurate`. Requirement DP-11 ([tracking](../TRACKING.md)); this record
covers agent-verifiable gaps only. Tracking quality on real people remains an
owner item.

## Fixtures

[`generate_geometry_fixtures.py`](../../native/deadpan-track/tests/generate_geometry_fixtures.py)
(development ffmpeg 9.0.1 CLI with FFV1 and libx264; `--check` reproduces every
byte) draws one synthetic scene: 24 pictures at 24 fps of a bright 36 px square
with a dark cross moving (+5, +1.5) px per picture from (40, 40) over a static
textured background, in a square-pixel 320×180 frame before rotation. Deadpan
decodes the files only through the pinned FFmpeg 8.0.3 libraries.

| File | Coded | SAR | Display matrix | Upright | Bytes | SHA-256 |
| --- | --- | --- | --- | --- | --- | --- |
| `square-rotated-90.mp4` (H.264) | 320×180 | 1:1 | 90° clockwise | 9:16 | 5,656 | `07a53a1af3f261631dac3f214584091bc3e20a4d2f7735b047b06b544fcc8e7b` |
| `square-anamorphic.mkv` (FFV1) | 240×180 | 4:3 | none | 16:9 | 22,651 | `64546dac9a65263c3fcb210c7631377873f38a43d042c3a6bffd3f8464deaf7f` |
| `square-rotated-270-anamorphic.mp4` (H.264) | 240×180 | 4:3 | 270° clockwise | 9:16 | 5,775 | `f92bce038e3f5f688c4fdd51045de24cf8e5646d624d604baeb86f07e2aa86d8` |

The truth is FFmpeg's own presentation, independent of Deadpan's mapping: the
generator decodes each file with FFmpeg's automatic rotation, scales it by its
SAR to square pixels, and records the centroid and extent of pixels brighter
than the background per picture, normalized to the upright picture, in
`square-geometry-truth.json` (SHA-256
`7c2da2ac60b3499a3e1e9305598ed7b5ba8dd57f8612052f778736020e5203e1`).

## Rotated and anamorphic tracking

[`native/deadpan-track/tests/geometry.rs`](../../native/deadpan-track/tests/geometry.rs)
registers each fixture in a project, runs the real `deadpan-track` worker under
the shared supervisor through `track_project` from picture 0 with the truth's
box, saves the path with `save_tracked`, reloads it and checks it:

- The host reports the declared quarter turns, and the prepared and saved
  `display_aspect` is the exact upright aspect (9:16, 16:9, 9:16), equal to the
  decoded picture's `FrameMetadata::display_aspect`.
- Every sample after the selection is `tracked`. Its centre is within 3 upright
  pixels of the truth, and each extent is within 25% of the truth's. Because of
  the quarter turn, the width in normalized coordinates is the larger extent on
  rotated sources.
- The reloaded target's `region_at` at every picture lies within the same
  tolerance of the truth.
- Camera and follow mapping use the actual decoded metadata (rotation, SAR,
  aperture) on the project canvas, 1080×1920, 1920×1080 and 1080×1080. At every
  picture `PictureGeometry::source_to_canvas` puts the saved centre exactly
  (within 1e-9) where an independent Fit computed from the upright size alone
  puts it, so on the true square. `deadpan_plan::follow_pose` centres exactly
  `source_to_input(0, centre)`.

Worst centre error in upright pixels, identical in two consecutive runs:

| Fixture | Path | Saved target | Stored samples |
| --- | --- | --- | --- |
| rotated 90° | 0.60 | 0.60 | 23 |
| anamorphic 4:3 | 1.23 | 1.23 | 23 |
| rotated 270° + anamorphic | 1.05 | 1.05 | 23 |

### Defect found and fixed: Vision saw squeezed pixels

Before this change the worker gave Vision the coded raster. On both 4:3
fixtures the tracker repeated the previous box for 1–4 pictures and then
caught up. The anamorphic FFV1 path reached 16.7 px of error (picture 11).
On the rotated anamorphic H.264 file the stale box became implausible at
picture 13, after which the path was interpolated and then lost, reaching
24.4 px of error. Wherever the box was updated, the mapping was correct
(within about 1 px).

Two square-pixel FFV1 controls of the same scene used the unchanged worker.
At the same 240-pixel coded width (240×136) the worst error was 1.12 px; at
320×180 it was 0.99 px. Coded size and per-pixel motion are therefore not the
cause; the squeezed appearance is.

The fix is `deadpan_media::analysis_picture::square_pixel_picture`. It resamples
the clean picture bilinearly to square pixels by enlarging the narrower axis.
Where that would exceed the 8,192-pixel or 16.7-megapixel analysis bounds, it
reduces the wider axis instead. The worker uses it for tracking and for
`detect-faces`. Normalized coordinates still refer to the whole clean coded
picture, so the protocol, the rotation mapping and the stored paths are
unchanged. Square-pixel sources take the previous path without a copy. Landmark
inspection for AI Holds still uses the clean coded picture.

## Saving through the open project's live endpoint

`track_save_commits_through_the_open_projects_live_endpoint` (same file)
holds the writer in a `ProjectStore` with a bound `deadpan_cli::host::Endpoint`.
A service-thread stand-in answers with the shared `execute_short`, as
`crates/deadpan-cli/tests/live_project.rs` does. The test runs the real worker
on the rotated fixture beside the held writer:

- `save_tracked` reaches the owner through `dispatch_short`. One remote
  `Execute` commits the target. The receipt has `committed: true`, and its
  `committed_revision` matches both the owner's head and
  `outcome.revision_id`. The target keeps its label and the 9:16 aspect, and a
  reader sees the same document.
- The owner reports a failed workspace refresh. The commit stands and the
  receipt carries `host_refresh_error`. The client does not repeat the request.
- Saving the same run again expects the pre-save head. The owner refuses it
  with `RevisionConflict`, and the document is unchanged.
- A fresh run saved with `--replace` semantics commits remotely, and its receipt
  has no refresh error.
- After the endpoint is dropped while the writer is still held, a save is
  refused (`HostOwnerChanged`: the discovery record names a revoked owner). No
  second writer is opened and nothing is written.
- After the owner closes, `project undo` steps back through the replacement
  and then the first remote save.

## Incremental scheduling (not implemented)

Specification §11.4 asks for analysis by source ranges, starting with the
visible cursor neighbourhood. This does not fit cleanly in
`deadpan-cli`/`deadpan-analysis` without app changes:

- Vision's tracker state lives in one worker process. Splitting a range into
  chunks re-seeds Vision from a tracked box, not a manual one. That changes
  results and accumulates drift. The policy (`TrackedPath::track`) also treats
  every seed as a manual keyframe. A chunked run needs a continuation seed that
  is neither manual nor a correction, carries the previous chunk's
  lost/held state, and has an `extend` operation proven equal to the
  single-run policy on its own observations.
- Tracking only runs forward from the selected picture. A neighbourhood that
  lies before the selection needs reverse-order decoding in the worker
  protocol (seek per GOP, decode, reverse).
- Progressive results need an operational, in-memory partial path shown in
  Camera, then one `SetTarget` at the end. Saving each chunk would add one
  undo step per chunk and make later chunks stale against the head.
- Prioritizing by cursor belongs to the app's job coordinator (`JobKind::rules`)
  and the preview's cursor, both in `deadpan-app`.

## Not covered

- Real-person footage (owner item, specification Section 25).
- Rotated or anamorphic face detection on real faces. Drawn faces on an
  anamorphic source were not run either; the face path shares the fixed
  picture preparation.
- The app's real-worker path. App tests and replay still use the scripted
  Vision seam.
- The release-mode cost of square-pixel resampling for large anamorphic
  sources. It is one bilinear pass per analysed picture and was not measured.
