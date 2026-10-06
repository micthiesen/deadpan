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
| [`deadpan_analysis::shots`](../crates/deadpan-analysis/src/shots.rs) | Pure `PictureSignature` (32 × 18 cell means of integer Rec.601 YCbCr plus a 32-bin luma histogram), the 23-byte `PictureMeasure` (`deadpan-picture-signature-2`), the streaming `ShotMeasurer` with resumable `ShotProgress`, validated `ShotAnalysis`, hard cuts and the `deadpan-shots-2` rule with [gradual transitions](../crates/deadpan-analysis/src/shots/gradual.rs). |
| [`deadpan_media::picture_scan`](../crates/deadpan-media/src/picture_scan.rs) | Decodes every picture of a verified snapshot in order, from the first picture or from any indexed picture through an exact seek, and checks each against its qualified index ordinal. |
| [`deadpan_cli::shots`](../crates/deadpan-cli/src/shots.rs) | `prepare_shot_input`, `stored_shot_progress`, pipelined resumable `scan_shots`, `stored_shots`, the `detect-shots` and `shots` commands. |
| [`deadpan-store`](../crates/deadpan-store/src/shot_analysis.rs) | `shot_analysis` and the operational `shot_scan_progress` table (database schema 66). |
| [`preview::shots`](../crates/deadpan-app/src/preview/shots.rs) | Automatic background job, its checkpoints and the rail status. |

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

`scan_shots` hands each converted picture over a two-picture channel to a
`deadpan-shot-signatures` thread, which reduces it to its signature; a picture
of at least 2^20 pixels is reduced in up to four row bands on scoped threads
(integer sums, so the split never changes the result). The caller's thread
decodes the next picture meanwhile and feeds returned signatures, in order, to
a `ShotMeasurer`, which keeps the last 51 signatures and one 23-byte measure
per picture. Cancellation (an `AtomicBool`) and one deadline are checked before
every decode call and by the native decoder; `progress(measured, total)`
follows each measured picture. Decoding runs on the caller's thread: the CLI's
main thread or the app's dedicated `deadpan-shots` thread, never the UI or
project service. No unsafe code was added; decoding stays inside the existing
native adapter.

The decoder runs with `SHOT_DECODE_THREADS = 4` codec threads (FFmpeg frame
and slice threading). Threaded decoding returns the same pictures bit for bit:
the media tests compare RGBA hashes of whole and seeked scans at 1 and 4
threads on three H.264 fixtures, the worker's
[FFV1 test](../native/deadpan-media-worker/tests/ffv1_picture_scan.rs) does so
for a 30-picture 4×2 FFV1/Matroska file made by the real conversion worker
(the source fixtures hold only single-picture FFV1 files; the tiny raster
exercises frame threading, not slices), the CLI test compares analyses at 1
and 8 threads,
and the Caminandes 2 measures are byte-identical at 1, 4 and 8 threads (below).
The per-picture PTS and duration checks are unchanged. With frame threads,
FFmpeg logs "Late SEI is not implemented" for that file (28 times per scan);
SEI is metadata, and the identical measures show the pictures are unaffected.

### Resuming

A scan offers a checkpoint at most every `SHOT_CHECKPOINT_INTERVAL` (10 s;
`detect-shots` uses `CLI_CHECKPOINT_INTERVAL`, 60 s, below). A checkpoint is a
`ShotProgressTail`: only the measures changed since the previous checkpoint,
from the first picture whose measure or span changed (at most 25 pictures
before the previous checkpoint's end, since a picture completes the spans of
the 25 before it). Neither the scan nor the app copies the whole measure
vector per checkpoint; the store appends the tail to the saved progress. No
checkpoint is offered after the scan is cancelled or within
`CHECKPOINT_DEADLINE_MARGIN` (5 s) of its deadline. When the receiver refuses
a tail, the next checkpoint carries every measure and replaces the saved
progress. A later scan of the same key resumes it:
`ShotMeasurer::resume` asks for pictures again from 50 pictures before the
first unmeasured one (the largest comparison window is 51 pictures), so every
span still waiting for later pictures is recomputed from real signatures, and
`scan_pictures_from` seeks exactly to that picture: the decoder restarts at
the key picture the qualified index names for it (`seek_from`), decodes the
preroll without converting it, checks every preroll picture against its own
indexed PTS and duration, and continues exactly as a scan from picture 0
would. Decoding from the start and skipping signatures was rejected: the
decode is the larger cost. A resumed scan equals an uninterrupted one byte for
byte (the analysis and CLI tests resume from 11 and 7 stopping points, and
the Caminandes 2 run below was killed with `SIGKILL` and resumed). Progress of
a different picture count is ignored and the scan starts over.

## Measurement

`deadpan-picture-signature-2` stores 23 bytes per picture:

| Bytes | Value |
| --- | --- |
| 3 | The `deadpan-picture-signature-1` change: cell and histogram change from the previous picture and the cell change from the picture two before. |
| 2 | Mean cell luma and the mean absolute deviation of cell luma from it (black pictures). |
| 9 × 2 | For each half-span `h` in 2, 3, 4, 6, 8, 12, 16, 20, 25: `across`, the cell change between the pictures `h` before and `h` after, and `residual`, half the mean absolute value of `2·middle − before − after` over all cell samples. Zero where the window leaves the pictures. |

A blend of two pictures lies on the line between them, so the middle of a
dissolve or fade has a small residual and a large `across`; a cut, a flash or
motion through textured content does not. Signature 1 was not enough: its
consecutive changes are rounded to whole steps (a 48-picture dissolve between
scenes that differ by 40 changes by under 1 per picture) and it has no
long-range comparison, so a dissolve and a pan of the same speed look alike.
The old rows are not reread; a scan replaces them.

## Rule

`ShotAnalysis::boundaries` applies `deadpan-shots-2`. Hard cuts
(`ShotAnalysis::cuts`) are exactly those of `deadpan-shots-1`: picture `k`
begins a shot when its cell change is at least 24 and its histogram change at
least 48 (of 255), its cell change is at least three times the mean of up to
eight neighbours on each side, both it and the next picture differ from the
picture two before them by a cell change of at least 24 (so a one-picture
flash that returns to its scene is not a cut), and at least six pictures have
passed since the previous cut.

Gradual transitions (`ShotAnalysis::transitions`) are reported as a kind
(`dissolve`, `fade_out`, `fade_in`), the half-open blended pictures and one
boundary picture. A picture `k` with half-span `h` is a candidate middle when
`across ≥ 24`, `3 · residual ≤ across`, `across` is at least twice the
`across` of the same-sized windows just before and after (so steady motion is
not a transition), no consecutive cell change inside exceeds `across / 2` and
no cut lies inside, and no wider window at `k` has more than 10/9 of its
change (the window holds the whole blend). Candidates are accepted by
increasing half-span, then residual, when neither their window nor their
final blended range (after length estimation, clamping and the fade
adjustments below) overlaps an accepted one: a fade's range reaches past its
window to the black picture and could otherwise overlap a neighbouring
transition. The length
`n` comes from the widest smaller window inside the blend, where
`across(h') / plateau = 2h' / (n + 1)`, clamped to 3..=48 pictures; a
candidate whose `n` would fit the window two half-spans smaller is refused.
A picture is black at mean luma ≤ 6 and spread ≤ 4. With black pictures within
`2h` after the middle only, it is a fade out ending at the first black
picture, which is its boundary (black is its own shot); the mirror case is a
fade in whose first picture is the boundary; otherwise a dissolve whose
boundary is its middle picture. Boundaries of both kinds are merged; a gradual
boundary closer than six pictures to another boundary, or to picture 0, is
dropped. Picture 0 always begins the first shot. The measures are stored, so
the rule can be rerun exactly on any later read.

## Storage

`shot_analysis` is keyed by Original content identity, qualified video stream
and signature version. It stores the picture count and the measures as a BLOB
of 23 bytes per picture (`encode_measures`). Saving replaces the row for its
key, removes rows of the same pictures under an older signature version,
never creates a revision or Undo step, and is limited to 16 rows per
project. Every read takes the expected picture count (the Original's qualified
index length), requires the stored count and BLOB length to agree with it and
revalidates through `ShotAnalysis::new`. `stored_shots` reads only the current
signature version and skips an unreadable or mismatched row, so the app scans
again.

Database schema 66 adds the operational `shot_scan_progress` table under the
same key: picture count, next picture and the measures so far (a checkpoint's
spans that reach past its measured pictures are zero, and reads check that).
It is written outside history. `append_shot_scan_progress` takes a tail: one
from picture 0 replaces the row; any other must join a saved row of the same
picture count whose next picture reaches the tail's start, and SQLite joins
the stored prefix with the tail (`substr(measures, …) || tail`) without the
caller copying it; a tail that does not join is refused.
`save_shot_scan_progress` replaces the row whole.

Progress is retained for one scan at a time: saving progress deletes every
other key's progress. In a single-Original project with a ready Original,
saving any shot analysis or progress also deletes progress of any other
content, and progress for other content is refused (the stale rows are still
removed). Generic projects keep progress of any registered source scanned
with `--asset`. The Original of a single-Original project never changes
(relinking requires identical content), so this removes only rows left by
earlier development work or a generic `--asset` scan. Saving a key's final
analysis also deletes that content and stream's progress in the same
transaction. Both tables always exist in schema 66, so there are no
missing-table branches. A read checks the picture
count, the next picture against the BLOB and `ShotProgress::new`; an
unreadable row is skipped and the scan starts over. Earlier development
packages are refused (see [development formats](DEVELOPMENT_FORMATS.md)).

## Commands

`detect-shots <project> [--asset <id>] [--decode-threads <1-16>]` scans with a
read-only store, resuming a saved checkpoint, and takes the writer briefly for
each checkpoint and to save, so the project stays openable elsewhere while it
scans. A writable open revalidates the package, so the command checkpoints
every `CLI_CHECKPOINT_INTERVAL` (60 s) and appends only the tail. It checks
cancellation and the deadline again after opening the writer and skips the
write when either applies; SQLite's 250 ms busy timeout bounds a contended
write. Checkpoints are best effort: a project open elsewhere keeps the previous
checkpoint, and the next one replaces it whole. It prints the key, rule, picture count, the boundaries, `cuts`,
`transitions` (`kind`, `first`, `end`, `boundary`), the shot count,
`resumed_from`, `decoded_pictures`, `checkpoints_saved`/`checkpoints_unsaved`,
`decode_threads`, `elapsed_ms`, `signature_elapsed_ms` (time inside
`PictureSignature::from_rgba` on the signature thread, overlapping decoding)
and `pictures_per_second` (decoded pictures over elapsed time).
`--decode-threads` exists for measurement. `shots <project> [--asset <id>]`
prints the stored analysis's boundaries under the rule: picture ordinal, kind
(`cut` or the transition kind), PTS, exact seconds (`pts · time_base` as a
rational), display seconds and the consecutive changes, then the transitions
with their first picture's time. Without an analysis it prints
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

The scan reads the package's checkpoint before it starts and resumes it. Its
checkpoint tails reach the UI as events; the waiting tail is submitted as
`ProjectRequest::SaveShotProgress` on the annotation lane when the lane and the
user-command slot are free (a newer tail merges into a waiting one with
`ShotProgressTail::merge`). The service appends it only for the session's
ready Original, stream, signature and picture count, and ignores failures:
progress is rebuildable. After a failed append the scan's later tails no
longer join and are refused too; the finished analysis is unaffected, and a
later scan resumes from the last saved progress. At most about
10 s of scanning is lost by quitting.

The Original card's detail line carries the status without adding rail
height: `595 frames · shots 42%` while scanning, then `595 frames · 6 shots`.
A failure restores `595 decoded video frames` and adds the message and Try
again below the REUSE heading.

## Real media, 2026-10-04

This records `deadpan-shots-1` and the single-threaded scan of that date;
the 2026-10-05 section below supersedes its throughput.

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

## Real edited footage, 2026-10-05

[Qualification record](qualification/shots-2026-10-05.md). "Caminandes 2: Gran
Dillama" (Blender Institute, 2013, CC-BY 3.0), the official Blender upload
`Z4C82eyhwgU`, created with `project create-from-url` (1920 × 1080 H.264, 24
fps, 3,504 pictures, 146 s). Ground truth was marked by hand from contact
sheets of every eighth picture and per-picture strips around every candidate
(Homebrew FFmpeg scene scores and luma, for inspection only): 24 hard cuts, a
17-picture fade in from black (46–62), a 14-picture night-to-day dissolve with
unchanged framing (2450–2463) and a two-picture flash blend from the llama to
the Earth (2117–2118). Matches allow ±2 pictures.

| Rule | Cuts found | Cut precision | Cut recall | Transitions found | Transition precision | Transition recall |
| --- | --- | --- | --- | --- | --- | --- |
| `deadpan-shots-1` (HEAD `04d38fd3`) | 18 | 18/18 | 18/24 | none | – | 0/3 |
| `deadpan-shots-2` as first written | 18 (identical) | 18/18 | 18/24 | 3 | 2/3 | 2/3 |
| `deadpan-shots-2` | 18 (identical) | 18/18 | 18/24 | 2 | 2/2 | 2/3 |

The first version accepted a 51-picture window at 2301 (`across` 42, residual
14) over a fast camera tilt into a darkening sky in one continuous shot. Its
own length estimate (10 pictures) would have fit a much smaller window, which
had failed the linearity and peak tests; the rule now refuses a candidate
whose estimated length fits the window two half-spans smaller. The fade in was
reported as 46..61 (true 46..63, boundary 46 exact) and the dissolve as
2452..2459 (true 2450..2464, boundary 2455, true middle 2456.5).

Missed cuts: 347, 441, 671 and 1136 are cuts between similar desert shots with
cell changes of 20–22 (histogram 55–91), below the cut rule's 24; 2982 is a
credit page change over the same blurred background (cell 4, histogram 16);
3427 is small license text appearing on black (cell 0, histogram 2). The
two-picture blend at 2117 passes through a white flash (residuals 30–45) and
is shorter than the three-picture minimum. Not proposed and correct: a fence
post sweeping past the lens (488–504), whip pans and camera pull-backs (976,
1440), lightning and explosion flashes (1200, 2233–2300, 2653) and the
continuous tilt at 2296–2306. Counterfactual, not adopted: a cut cell
threshold of 20 (or 16, or 12) finds 22 of 24 cuts with no false cut here.

Throughput, Apple M5 Max, release builds, three interleaved rounds while other
agents compiled (load averages 4–9 on 18 cores); medians:

| Clip | HEAD (1 thread, signature inline) | 1 thread | 4 threads (default) | 8 threads |
| --- | --- | --- | --- | --- |
| Caminandes 2, 1080p24, 3,504 pictures | 124.7 pictures/s (28.1 s) | 247.8 (14.1 s) | 447.8 (7.8 s) | 458.6 (7.6 s) |
| Synthesized cuts, 1080p24, 336 pictures, 29 Mbit/s | 70.1 (4.8 s) | 93.7 (3.6 s) | 237.6 (1.4 s) | 317.7 (1.1 s) |
| Interview, 360p30, 595 pictures | 1,076.7 (0.55 s) | 1,822.3 (0.33 s) | 2,780.2 (0.21 s) | 2,777.8 (0.21 s) |

Signature time per 1080p picture fell from about 3.8 ms to about 0.85 ms (row
bands on up to four threads, per-cell accumulators, no per-pixel clamps that
cannot apply); it now overlaps decoding. Four decode threads are 1.8× (cartoon)
to 2.5× (high-entropy clip) faster than one; eight add 2–34 % more and take a
larger share of the machine, so four stays the default. An hour of 1080p24
takes about 3.2 minutes for footage like Caminandes 2 and about 6 minutes for
the high-entropy clip, against about 12 and 21 minutes before. The synthesized
clips still give the 2026-10-04 results: the five cuts exact, and the soft clip
now reports its 12-picture dissolve as 61..72 (boundary 66) besides the cut at
132, with the returning flash rejected.

## Shots as motions and objects

`]s` and `[s` move to the start of the next or previous shot occurrence, in
Original and Your edit, and `iS`/`aS` select the shot occurrence at the Edit
cursor; both compose after `d`, `y` and `r` and record in macros. A gradual
transition contributes one boundary, so the motions and objects see it like a
cut: half of a dissolve's blended pictures belong to each shot, a fade out's
to the shot it leaves and a fade in's to the shot it enters. `aS` still equals
`iS`; including a transition's blended pictures in `aS` is not implemented.

Shots reach the Edit clock by the same projection as words and pauses
([`deadpan_cli::speech`](../crates/deadpan-cli/src/speech.rs)): a frame
presenting an Original picture belongs to that picture's shot, and consecutive
frames of one shot form an occurrence while their pictures do not go back. A
cut inside a shot keeps one occurrence, a replayed shot starts another, and
freezes, generated pictures and gaps belong to no shot.

## Remaining

Cuts whose cell change stays below 24 although the histogram changes strongly
are missed (four of 24 on Caminandes 2; a threshold of 20 would find them
there, but one film is not enough evidence to change the cut rule), as are
cuts with almost no global change (a credit page over the same background,
small text on black). Blends shorter than three pictures, a flash longer than
one picture, lighting changes and fast tilts that happen to be linear can be
misread; transition extents are estimates (Caminandes 2's 14-picture dissolve
was reported as 7 pictures, with the boundary one picture from the true
middle). The gradual rule is qualified on one animated short and synthesized
clips only, with three true transitions. The six-picture spacing is a picture
count rather than a duration (250 ms at 24 fps, 50 ms at 120 fps). `aS` does
not include transition pictures. The scan copies the Original into its own
verified snapshot and decodes at normal priority beside preview and audio;
reusing the preview's snapshot and lowering the scan's priority remain open.
A failed scan is not retried automatically when the project is reopened in
the same app run (Try again does), but a new app run tries again. Shot display
in the timeline remains open.
