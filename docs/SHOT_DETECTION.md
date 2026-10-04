# Shot detection

Deadpan measures the visual change between consecutive Original pictures and
proposes shot boundaries from it. Section 11 of the
[specification](spec/DEADPAN_SPEC.md) is normative: "Shot detection proposes
source-frame boundaries from visual change metrics. Neither analysis changes
the project frame rate or inserts cuts by itself." This records the implemented
boundary. Like [speech activity](SPEECH_ACTIVITY.md) and
[transcripts](TRANSCRIPTION.md), shots are an annotation: detecting, saving or
loading them never edits the project.

## Pieces

| Piece | Responsibility |
| --- | --- |
| [`deadpan_analysis::shots`](../crates/deadpan-analysis/src/shots.rs) | Pure `PictureSignature` (32 × 18 cell means of integer Rec.601 YCbCr plus a 32-bin luma histogram) and its three-byte `change`; validated `ShotAnalysis` and the `deadpan-shots-1` boundary rule. |
| [`deadpan_media::picture_scan`](../crates/deadpan-media/src/picture_scan.rs) | Decodes every picture of a verified snapshot in order and checks each against its qualified index ordinal. |
| [`deadpan_cli::shots`](../crates/deadpan-cli/src/shots.rs) | `prepare_shot_input`, streaming `scan_shots`, `stored_shots`, the `detect-shots` and `shots` commands. |
| [`deadpan-store`](../crates/deadpan-store/src/shot_analysis.rs) | Database schema 61 `shot_analysis` table. |
| [`preview::shots`](../crates/deadpan-app/src/preview/shots.rs) | Automatic background job and the rail status. |

## Scan

`prepare_shot_input` resolves the ready Original's qualification receipt (or a
registered `--asset`), snapshots its retained bytes through the store and
copies them into a private SHA-256-verified `VerifiedSourceInput`, the same
path audio analysis and preview use. The store can be closed afterwards.

`scan_pictures` opens a fresh persistent native decoder over that snapshot,
requires the decoder's stream index, raster and time base to match the
qualified interpretation, and converts each picture to packed RGBA8 with the
existing explicit matrix/range conversion. Picture `k` must carry exactly the
PTS and reported duration of index ordinal `k`, and decoding must end exactly
after the last indexed picture. A different PTS, a missing picture or an extra
picture fails with the ordinal and both values; nothing is skipped, inserted
or guessed. Bytes other than the qualified content are refused before
decoding.

`scan_shots` reduces each picture to its signature and pushes
`previous.change(&next, before)` (`[0, 0, 0]` for picture 0), keeping only the
two previous signatures and the three-byte changes: memory is two signatures
plus three bytes per picture. Cancellation (an `AtomicBool`) and one deadline are checked before
every decode call and by the native decoder; `progress(done, total)` follows
each picture. Decoding runs on the caller's thread: the CLI's main thread or
the app's dedicated `deadpan-shots` thread, never the UI or project service.
No unsafe code was added; decoding stays inside the existing native adapter.

## Rule

`ShotAnalysis::boundaries` applies `deadpan-shots-1`: picture `k` begins a shot
when its cell change is at least 24 and its histogram change at least 48 (of
255), its cell change is at least three times the mean of up to eight
neighbours on each side, both it and the next picture differ from the picture
two before them by a cell change of at least 24 (so a one-picture flash that
returns to its scene is not a cut), and at least six pictures have passed
since the previous boundary. Picture 0 always begins the first shot. Stored changes are
the measurement (`deadpan-picture-signature-1`), so the rule can be rerun
exactly on any later read.

## Storage

Database schema 61 adds `shot_analysis`, keyed by Original content identity,
qualified video stream and signature version. It stores the picture count and
the changes as a BLOB of three bytes per picture. Saving replaces the row for its
key, removes rows of the same pictures under an older signature version,
never creates a revision or Undo step, and is limited to 16 rows per
project. Every read takes the expected picture count (the Original's qualified
index length), requires the stored count and BLOB length to agree with it and
revalidates through `ShotAnalysis::new`. `stored_shots` reads only the current
signature version and skips an unreadable or mismatched row, so the app scans
again. Schema 59, 60 and 61 packages are upgraded in place to the current
schema 62 by their next writer (see [development formats](DEVELOPMENT_FORMATS.md)).

## Commands

`detect-shots <project> [--asset <id>]` scans with a read-only store, then
takes the writer only to save. It prints the key, rule, picture count, the
boundary picture ordinals, the shot count, `elapsed_ms`, `signature_elapsed_ms`
(time inside `PictureSignature::from_rgba`) and `pictures_per_second`.
`shots <project> [--asset <id>]` prints the stored analysis's boundaries under
the rule: picture ordinal, PTS, exact seconds (`pts · time_base` as a rational),
display seconds and both changes. Without an analysis it prints
`"analysis": null`. Failures report `ShotDetectionUnavailable`,
`ShotDetectionCancelled` or `ShotDetectionFailed`.

## In the app

Shot detection needs no model. When a single-Original project's Original is
ready and the workspace has no stored analysis, `reconcile_shots` starts one
background scan for the session. The result is submitted to the project
service (`ProjectRequest::SaveShotAnalysis`), which saves it only for the
session's ready Original, its qualified video stream, the current signature
version and the qualified picture count, and publishes
`Workspace::shot_analysis` (`OriginalShots { key, analysis }`). The analysis
is loaded when a project opens and carried unchanged across edits. A new
project session cancels the previous scan; quitting cancels and joins scans
for up to 3 s, like transcription.

The Original card's detail line carries the status without adding rail
height: `595 frames · shots 42%` while scanning, then `595 frames · 6 shots`.
A failure restores `595 decoded video frames` and adds the message and Try
again below the REUSE heading.

## Real media, 2026-10-04

Apple M5 Max, release build. Synthesized clips were made with Homebrew FFmpeg
from `lavfi` sources concatenated at known pictures, H.264 1920 × 1080 at 24 fps,
tagged Rec.709, registered video-only in scratch projects.

| Clip | True cuts | Detected | Notes |
| --- | --- | --- | --- |
| Six hard cuts between testsrc2, mandelbrot, SMPTE bars, life, cellauto and hue-shifted testsrc2 (336 pictures) | 72, 120, 168, 240, 288 | 72, 120, 168, 240, 288 | Exact. Cell changes 32–69, histogram 130–255. |
| testsrc2 → 12-picture cross-dissolve → mandelbrot with a one-picture white flash → hard cut to bars (180 pictures) | dissolve 60–72, cut 132 | 132 | The dissolve is missed (cell change about 6 per picture). An earlier two-value measurement proposed the flash at 100 as a cut; comparing each picture with the one two before now rejects a flash that returns to its scene. |
| Copy of `interview.deadpan` (640 × 360, 30 fps, 595 pictures, one static shot) | none | none | |
| Replay fixture `cfr-bframes.mp4` (320 × 180, 120 pictures) | none | none | `shots` replay scenario. |

Throughput: the 1080p clip scanned at 69 pictures/s (4.84 s for 336 pictures,
repeatable to 1 %), 2.9 × real time at 24 fps; the 360p interview at 1,040
pictures/s. Of the 14.4 ms per 1080p picture, decoding alone takes about
8.7 ms (a metadata-only full decode of the same file, measured with the
`inspect_source` example, took 2.94 s), signatures 3.6 ms (1.20 s total) and
RGBA conversion plus its 8 MB allocation the remaining 2.1 ms. RGBA conversion
is therefore not the bottleneck: the native decoder runs single-threaded
(`thread_count = 1`). It offers no cheaper exact picture path for this use:
its I420 copy admits only 8-bit limited-range Rec.709 4:2:0 for export and
would require a new signature version. The synthesized clip is high-entropy
(29 Mbit/s); ordinary footage decodes faster. At this rate an hour of 1080p24
takes about 21 minutes in the background.

## Shots as motions and objects

`]s` and `[s` move to the start of the next or previous shot occurrence, in
Original and Your edit, and `iS`/`aS` select the shot occurrence at the Edit
cursor; both compose after `d`, `y` and `r` and record in macros. Detected
boundaries are hard cuts with no transition frames, so `aS` equals `iS`.

Shots reach the Edit clock by the same projection as words and pauses
([`deadpan_cli::speech`](../crates/deadpan-cli/src/speech.rs)): a frame
presenting an Original picture belongs to that picture's shot, and consecutive
frames of one shot form an occurrence while their pictures do not go back. A
cut inside a shot keeps one occurrence, a replayed shot starts another, and
freezes, generated pictures and gaps belong to no shot.

## Remaining

Gradual transitions (dissolves, fades) are not detected, a flash longer than
one picture is still proposed as a cut, cuts between scenes with similar luma
histograms are missed, and the six-picture spacing is a picture count rather
than a duration (250 ms at 24 fps, 50 ms at 120 fps); the rule has not been
qualified on real edited footage. The scan copies the Original into its own
verified snapshot and decodes at normal priority beside preview and audio;
reusing the preview's snapshot and lowering the scan's priority remain open.
A failed scan is not retried automatically when the project is reopened in
the same app run (Try again does), but a new app run tries again. Decoding is single-threaded and the
signature is computed on the same thread; no pipelining. Shot display in the
timeline and progress persistence across app restarts (an interrupted scan
starts over) remain open.
