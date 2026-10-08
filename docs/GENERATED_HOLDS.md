# Authored generated Hold foundation

Core schema 6 retains the schema-5 generated Hold intent and exact retained sampling metadata.
It does not establish that a candidate has passed media validation or been auditioned
in the application. Generic project creation and command ingress
reject new generated artifacts with `GeneratedAcceptanceUnavailable`. The dedicated
[store acceptance API](GENERATION_ACCEPTANCE.md) binds a selected Ready receipt,
verified dependencies and an undoable edit. The native application remains a shell.

## Authored representation

`HoldVideo::Generated` contains an immutable `GeneratedArtifact` and a captured
`HoldFallback`. The fallback is Background or a source Freeze, never another
generated provider. The artifact names two immutable video-only assets: the sampled
master and the original native sequence. Each has a strict algorithm-tagged BLAKE3
object reference and exact byte length. A third object reference retains the
provenance manifest. Pure identity types live in `deadpan-core`; the storage module
re-exports them without changing their JSON wire format.

`BridgeSamplingMap` version 1 retains project/native rational rates, original output
count N, native sequence count M, and the explicit encoded-sRGB RGB8 linear half-up
interpolation policy. For output frame j, the exact native position is
`(j+1)*(M-1)/(N+1)`. The compact formula is the sampling map; seeking does not allocate
N entries. Both conditioning boundary frames are excluded from output sample positions.

Document validation requires the map's project rate to match the document, a
positive Hold duration no greater than N, exact N/M asset frame counts, matching
content identities, video-only masters, and a valid fallback. Native/container time
bases remain explicit. The Matroska millisecond clock does not replace the exact
sampling map or frame ordinal. The core cannot inspect manifest contents or prove
that media bytes implement their declared metadata.

## Commands and frame reuse

`AcceptGeneratedHold` atomically registers up to two referenced immutable asset
records and replaces the visual provider. It captures the existing Background or
Freeze fallback, or preserves the fallback from an already generated provider.
Legacy `Accepted` providers have no captured fallback and must first be changed to
an explicit Background/Freeze. Unrelated asset insertion and replacement of an
existing asset record are rejected. `RevertGeneratedHold` restores the captured
fallback. Both commands support occurrence isolation and exact inverse patches.
Neither changes Hold audio or downstream source coordinates.

Shortening a generated Hold retains the original artifact and sampling map. Its
visible interval is the sampled master's prefix `[0, duration)`. Re-extending up to
N reuses those retained frames. Extending beyond N restores the captured fallback
in the same duration edit. The store atomically records a replacement preparation
with the accepted artifact, original generation controls and exact authoring
scope. The app prepares and generates the replacement through its bounded AI
worker; the core remains independent of jobs. See
[replacement lifecycle and recovery](AI_HOLDS.md#lengthening-accepted-pauses).

Changed raw source boundaries also restore the captured fallback and create
replacement work, atomically with the source edit. The store derives the final
provider set after the complete base command, including Compound commands and
dependent neighboring Holds. The core applies the derived
`WithBoundaryReplacements` envelope as one reversible patch; it performs no
media or model work. Hosts cannot supply an unverified replacement list. See
[changed source boundaries](AI_HOLDS.md#changed-source-boundaries).

The picture plan requests sampled-master frame ordinals and keeps compiled revisions
immutable. It never resamples the original map after a duration change. Reversion
does not delete asset records or files; inverse history retains the full provider.
History-aware media reference accounting keeps accepted objects until no
authored or retained history reference needs them; see [Storage](STORAGE.md).

## Storage and migration

Database schema 72 stores core document schema 46, qualified bundle receipts,
immutable accepted-origin receipts and durable automatic-intent history. Older
unused development packages are refused before admission without modification,
under the owner's breaking-format authorization for this session. Accepted
origins are proven through their original qualified acceptance; raw generated
snapshots cannot stand in for that proof. Historical media contracts retain
their declared interpretation without invented evidence.

Generic store ingress examines the resulting authored providers, including Repeat
gaps and override-owned nodes. It can retain or copy an already present artifact,
resize it, revert it, and navigate existing history. It cannot introduce a new
artifact through a command or initial project snapshot. Direct `SetHoldProvider`
also rejects Generated in core; acceptance has its own typed operation. Read-only
history inspection does not depend on a model or reopen worker output paths.

## Verification and remaining integration

Core tests cover atomic asset/provider changes, inverse restoration, occurrence
isolation, map validation, resize reuse, fallback, and strict legacy vocabulary.
Picture-plan tests compare every reused frame and the first following source frame.
Store tests check generic ingress rejection and resize/revert/undo/redo across reopen
using an explicitly synthetic preexisting authored fixture, not playable media.
Migration tests use a genuine schema-6 database produced by commit
`5100432be7b95600d88179254624dd9d78096714`, including pending redo, nine requests,
ten attempts, evicted and selected receipts, failures, cancellation and interrupted
states. Corrupt history and operational metadata must preserve the original and backup.
Review caught the legacy asset-hash vocabulary gap. A regression that adds an
otherwise valid BLAKE3 asset to every old snapshot reproduced erroneous promotion
before the fix; old-schema asset parsers and projections now reject it.

On 2026-09-21, the full repository gate passed on Apple Silicon macOS with pinned
Rust 1.97.1: formatting, workspace Clippy with warnings denied, 268 Rust tests
(none failed or ignored), workspace build, and `deadpan-cli doctor` reporting
database schema 7 and core schema 5. The audio, model, and FFV1 report suites passed
20, 47, and 5 Python tests. An obsolete migration test expectation of schema 6
was corrected before the full passing run. Native startup, GUI, inference and
actual media-rendering checks were not repeated for these pure semantics.

Candidate-to-master conversion, provenance binding and selected-Ready/relevance
checks now support explicit durable store acceptance. Audition, exact source/color
context, application acceptance, model-independent rendering and portable copy
remain open. [Generated-object storage](GENERATED_MEDIA.md) supplies byte ownership;
[the acceptance contract](GENERATION_ACCEPTANCE.md) describes the integrated boundary.

## Source joins

An accepted bridge meets the Original with hard cuts at exact frames; nothing is
crossfaded, because a picture crossfade would alter original frames. For a pause of
N frames inserted at Edit boundary f, frame f-1 is the unchanged left picture L,
frames f..f+N-1 show sampled-master frames 0..N-1 (cropped back to the recorded
`content_aspect` by `picture::fill_canvas_aspect`), and frame f+N is the unchanged
right picture R that was at f before insertion. Sampled frame j is the half-up
linear interpolation of the native sequence at `(j+1)*(M-1)/(N+1)`, so neither
conditioning endpoint (native frames 0 and M-1) is ever presented.

`deadpan_cli::generation::joins::measure_request_joins(package, generated, origin,
hold, receipt, cancelled)` is the §12.5 endpoint-discontinuity heuristic. It decodes
the committed pictures at f-1 and f+N of the request's origin revision through
`ProjectPictureSession` (what the viewer shows there, before editorial framing),
and the Ready receipt's sampled-master frames 0 and N-1 through a verified
generated-object snapshot (`open_candidate_master`), cropped exactly as
presentation crops them (`fill_canvas_aspect`). Both are compared in one space,
`joins::comparison_region(canvas, native)`: the canvas aspect inside the native
raster, which is the size of a presented generated frame. Each boundary picture is
fitted whole into that region the way conditioning fits it, then cropped like a
generated frame; authored black stays black. Each join reports the mean absolute
RGB difference on the 0..255 scale, the largest channel difference and a class:
Smooth below 6, Noticeable below 20, otherwise Jump. The thresholds are
uncalibrated against viewers. The measurement is advisory: it never accepts,
rejects, selects or reorders a variant, and Smooth does not prove an invisible
cut. Editorial framing and captions are not composed, and eight-bit, unrotated
pictures only are measured. `generate-hold` reports it per Ready attempt as
`ready.joins = {entry, exit, region, advisory: true}` (each join
`{mean_abs_diff, max_abs_diff, class}`), or `{error}` when it cannot be measured;
a measurement failure never changes the attempt's outcome.

`crates/deadpan-cli/tests/generated_joins.rs` checks these joins on real media
through production code with the synthetic worker (only the model is replaced):
the `freeze_hold` recipe (`cfr-bframes.mp4`, 15-frame Freeze at f = 15) filled by
two Ready variants, the first accepted, undone back to the Freeze fallback, then
the second accepted. Through `ProjectPictureSession` each accepted revision shows
frame f-1 byte-identical to L and frame f+N byte-identical to R as decoded at the
pre-insertion revision, every pause frame byte-identical to the cropped sampled
frame, the exact sampling positions and interpolation against the decoded native
master, native endpoints equal to the retained conditioning pictures, and first
and last generated frames differing from both endpoints. The Freeze and undone
revisions show L throughout the pause. `measure_request_joins` equals
`measure_pictures` on the same decoded pictures, and on these real pictures a
master whose end frames are the retained conditioning pictures measures exactly 0
(Smooth, confirming the shared comparison space reproduces conditioning's
placement) while their inverse is a Jump. Real-model join quality is not
measured here. These tests skip without `ffmpeg` (libx264rgb) or a built
`deadpan-media-worker`, except under `DEADPAN_REQUIRE_SYNTHETIC_WORKER=1`, which
`cargo xtask gate` sets, where a missing tool fails them.

## Speech preservation

Hold pictures and Hold audio are independent: acceptance, reversion to the
fallback and switching variants change only the visual provider, so they change
no audio sample. Insertion itself splits the Original audio at B(f) and resumes
it at B(f+N) without regeneration or stretching.

The same test file checks this on two real Originals: `cfr-bframes.mp4` (silent
but for one click after the pause) and a generated 320x180 Original whose 48 kHz
stereo AAC is a continuous aperiodic amplitude-modulated chirp, so both seams cut
through sound. On both, the edge-faded bus (`ProjectAudioSession::read_edge_faded`)
and the limited bus that audition and export share (`OfflineAudioSession`) of the
accepted, undone and second-accepted revisions are bit-identical to the Freeze
revision over the whole Edit. Against the pre-insertion revision, the Freeze
revision is bit-identical before B(f) and, shifted by exactly B(f+N)-B(f)
samples, from B(f+N) to the end, excluding only the 96-sample seam fade windows
(where samples are never louder than the original); the pause itself is silent.
The proposal the app auditions before acceptance (`Snapshot::proposed_generated`)
is admitted against the committed base, and differs from that base only in the
revision ID, the Hold's `video` and the two added video-only master assets; the
Hold's duration and audio policy and every other beat, sound and asset are equal.
Its PCM is read inside `deadpan-playback`, whose `Sources` provider is private,
so that read is covered by the existing playback unit test rather than this one.
These checks are structural (bit equality and an exact sample shift), not a
listening test.
