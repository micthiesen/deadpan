# Preview-versus-export verification

`deadpan_cli::export_verification` compares an emitted movie with the committed
revision it was rendered from. It answers the Gate D question that
[finished-file verification](FINISHED_FILE_VERIFICATION.md) deliberately leaves
open: does the exported content match what the shared preview path shows and
plays? It is a diagnostic. It edits nothing, grants no publication authority and
never replaces the structural verifier that Render runs before publication.

```sh
deadpan-cli verify-export /path/edit.deadpan --movie /path/Exports/edit.mp4 \
  [--revision REVISION_ID] [--frames 0,12,40 | --every 5] \
  [--samples 0:48000,96000:120000 | --no-audio] [--report /path/new-report.json]
```

`--revision` defaults to the current committed revision; pass the render's
`captured_revision` when the project has changed since. The complete committed
range is compared, as public Render exports it. The JSON report goes to stdout
(and to a new `--report` file). The command exits zero only when every check
passes; otherwise stderr carries `ExportVerificationMismatch` after the full
report. Unusable inputs fail with `ExportVerificationMovie`,
`ExportVerificationTiming`, `ExportVerificationReference` or
`ExportVerificationRenderer`. SIGINT and SIGTERM cancel cooperatively
(`ExportVerificationCancelled`). macOS only: references need the Metal renderer.

## References and what they prove

- **Pictures** come from `ProjectPictureSession`: the committed `RenderPlan`
  sample, `select_source_frame`, and the shared `source_to_render_frame`,
  `fill_canvas_aspect` and `render_layers` helpers that the native preview
  worker also calls, rendered by the shared `PictureRenderer::render_composed_captioned`
  with the same framing layers and captured context. The app worker's own
  session caching and admission code is not exercised. The harness reads the
  composed linear Rec.2020 working target back and converts it at the
  [SDR encoder pixel boundary](SDR_ENCODER_PIXELS.md) into limited Rec.709 I420,
  the like-for-like reference for a decoded H.264 picture (the display texture
  applies the sRGB display transform and is not a comparable code space).
- **HDR branch.** The revision's automatic branch comes from
  `ProjectPictureSession::color_decision()` (see [automatic HDR output](HDR_OUTPUT.md)),
  the same decision preview and the encoder host use. When the captured
  `ExportPictureContract::color_policy()` is PQ or HLG, the reference is
  `ExportPixels::Hdr`: the same composed working target converted at the
  [Rec.2100 ten-bit boundary](HDR_PIXELS.md#rec2100-encoder-pixels), and the movie
  is decoded with `next_yuv420p10`. Planes are compared in ten-bit code values,
  which for PQ and HLG are transfer-encoded signal values, so a code error
  weighs the same perceptual step wherever it occurs. An HDR project whose
  branch falls back to SDR (an SDR video, still or accepted footage present)
  is compared exactly like an SDR project; its HDR sources are tone-mapped
  into the eight-bit reference.
- **This reference shares the picture session with the encoder host.** A
  planning or picture-session bug produces the same wrong picture on both
  sides and passes the PSNR gates. The picture comparison checks encode
  fidelity, content order where references are distinguishable, exact PTS
  order, color tagging and mux fidelity. It cannot identify which nearly
  identical source frame was encoded after lossy compression. The independent
  check of recipe semantics is the reference provenance (Original asset,
  source frame and PTS; Generated frame; Background) asserted against
  expectations derived by hand from each recipe in the fixtures. That
  provenance records the intended reference, not a source identity recovered
  from decoded pixels. Sound recipes add the same kind of independent check:
  hand-derived 256-sample
  windows of the limited bus that must be loud (peak above 0.5) or quiet
  (below 0.01), for example the click moved by an audio lag, silenced by a
  mute range or repeated by a room-tone loop.
- **Audio** comes from `OfflineAudioSession`, the limited canonical bus that
  audition also plays (`LimitedAudio`, before monitor gain), read on the
  absolute project sample grid `[B(start), B(end))`. It is likewise the bus the
  encoder consumed.

## Container, color and timing checks

SDR checks are unchanged. For an HDR branch the movie must have an `hvc1`
sample entry whose `hvcC` is Main10 (profile 2), ten-bit luma and chroma and
4:2:0; `colr` 9/16/9 (PQ) or 9/18/9 (HLG), limited range; a decoder
interpretation of limited BT.2020 NCL with the branch transfer and codec
`hevc`; decoder profile 2; and left-sited chroma on every picture. PQ movies
must carry `clli` and an `mdcv` equal to the contract's retained mastering
volume (or none when the contract has none); HLG movies carry neither. When
every output frame was rendered (the automatic selection up to 600 frames),
MaxCLL/MaxFALL are recomputed from the references' clipped linear light
(`FrameLight`, rounded up to whole cd/m² as the encoder host does) and must
equal `clli` (`hdr_light`). A movie of the wrong dynamic range (eight-bit SDR
for an HDR branch, or the reverse) is reported as a color failure without
picture comparison. A file that the qualified source decoder refuses (for
example in-band Dolby Vision NAL units) is unusable input
(`ExportVerificationMovie`).

- Each track must have exactly one non-empty edit (`media_time >= 0`; an empty
  edit, `-1`, fails). The video edit must present exactly `frame_count × D / N`
  seconds; the audio edit exactly `B(end) - B(start)` samples.
- Container `colr` must be primaries/transfer/matrix 1/1/1, limited range; the
  decoder must interpret limited Rec.709, and every decoded picture must report
  left-sited chroma.
- Pictures are decoded continuously. Each PTS must map exactly onto an output
  ordinal (`pts × tb × N / D` an integer). Off-grid, duplicate, out-of-order,
  out-of-range and missing ordinals are reported as `picture_anomalies` and
  failures, not aborts. A picture is compared at the ordinal its own PTS claims,
  so a shifted stream also fails content gates.
- Audio uses Manual AAC decode, so physical priming stays visible, and each
  sample is placed at its own PTS. The first decoded PTS must equal minus the
  audio edit's `media_time`. That check is partly self-referential: the decoder
  applies the same edit, so a wrong edit moves both. The content alignment
  below is the real timing guarantee (a movie whose edit no longer skips the
  priming passes the priming cross-check and fails alignment with a measured
  2,048-sample lag with encoder ABI 2). Decoded frames that do not continue the previous frame are
  reported as `audio.gaps`; window samples the file does not present are
  counted as `uncovered_samples` and fail `decoded_audio_gap`. The presented
  end must equal the edit's declared duration and the committed sample count.

## Picture gates

Each selected output frame `n` is compared with reference `n`, and its luma
with references `n-1` and `n+1`. Defaults (`Thresholds::default()`):

| Check | Default | Rationale |
| --- | --- | --- |
| Luma PSNR (SDR, peak 255) | >= 32 dB | Measured 53.1–62.2 dB minimum per small fixture, 38.3 dB (1080p) and 44.4 dB (2160p) on the large test patterns; a different picture of the moving fixture measures 15–27 dB. |
| Chroma PSNR (SDR, Cb and Cr) | >= 32 dB | Measured 53.5–60.1 dB; 40.1 dB (1080p) and 44.6 dB (2160p). |
| Luma PSNR (HDR, ten-bit PQ/HLG codes, peak 1023) | >= 30 dB | Gross mismatch only. Correct encodes measured 59.9 dB (PQ) and 60.4 dB (HLG) on the clean recipes and 44.5 dB (PQ) and 43.6 dB (HLG) on the grain recipes; the 1.35x reframe negatives measured 11.7–18.7 dB. About 13.6 dB below the lowest correct encode and 11.3 dB above the highest gross negative. Small wrong regions are the local gate's job: a missing caption measured 34.3–39.2 dB and cannot be separated from grainy correct encodes by whole-picture PSNR. |
| Chroma PSNR (HDR) | >= 40 dB | Measured 66.6 dB (PQ) and 67.9 dB (HLG) clean, 52.4 dB and 51.2 dB on grain (11.2 dB margin). |
| HDR local structure: largest 4x4-cell luma mean difference | <= 40 ten-bit codes (`local_structure_mismatch`) | Averaging 4x4 cells removes most grain and coding noise but not a missing graphic. Correct encodes measured at most 3.9 (PQ) and 4.3 (HLG) codes clean and 8.4 and 12.6 codes on grain (3.2x below the gate); the missing caption measured 157.6–325.0 codes (3.9x above) and the reframe 422.9–868.3. A missing element that covers one full cell with more than 40 codes of contrast fails whatever the picture size; a two-pixel stroke split across cells needs about 160. SDR pictures report `local_luma_error` (eight-bit codes) without a gate. |
| 8x8 luma thumbnail mean absolute difference | <= 4.0 eight-bit-equivalent codes | Gross mismatch gate for wrong framing, zoom or content. Ten-bit codes are divided by four (limited-range ten-bit codes are exactly four times their eight-bit counterparts). Measured <= 0.065 SDR and <= 0.059 HDR; the 1.35x reframe negatives fail it in both. |
| Neighbor margin | a distinguishable reference `n±1` must not beat `n` by > 0.5 dB | `frame_index_mismatch`. Reference-only separation must exceed twice the declared luma error bound below. The own frame beat its best neighbor by 28–38 dB on moving fixtures (`min_neighbor_margin_db`). |
| Black frame | decoded mean luma <= 20 while reference mean > 32 (eight-bit-equivalent codes) | `unexpected_black_frame`. |

**Neighbor observability.** The minimum luma PSNR declares a per-picture RMS
error bound `epsilon = peak * 10^(-min_luma_psnr_db / 20)`: 6.4053 eight-bit
codes for SDR or 32.3501 ten-bit codes for HDR. A pair is `observable` only
when the RMS distance between its two reference luma planes exceeds
`2 * epsilon`. At or below that distance the two error balls overlap, so the
comparison is `unobservable`: compression within the declared fidelity can
produce the same decoded luma from either source. This decision uses only
references and the existing declared threshold. A poor decoded picture
cannot enlarge its own error allowance.

The 0.5 dB preference gate is unchanged for observable pairs. For
unobservable pairs it reports the raw neighbor PSNR without claiming an
index mismatch. This adds no exact-identity guarantee beyond the luma gate:
by the triangle inequality, an observable neighbor cannot be closer if the
own luma passes its error bound. An index mismatch is therefore a diagnostic
for an already failing luma comparison. All pixel, structure, color, PTS,
frame-count and audio gates still apply; unobservable does not mean that
content timing or source identity was verified. The rule is the same for
Original, Generated and Background references.

A known-order compression witness demonstrates the old rule's false
positive: encode three 64x32 pictures with neutral chroma, flat luma 100,
a 99/101 checkerboard, then flat luma 160. A libx264 CRF 23 encode removes
the middle picture's one-code texture. Its own PSNR is 48.13 dB, but the
previous reference now matches exactly. The unit regression models that
bounded texture loss without a codec dependency; a distinguishable shifted
square remains a failure in both SDR and HDR.

On static content (freezes, Background, repeated stills) neighbors are
identical and always unobservable. Exact PTS-to-ordinal mapping and frame
count still verify the presented timeline, but cannot expose a substitution
of identical pictures. Max and mean absolute errors per plane are reported
but not gated.

## Audio gates

Windows default to consecutive one-second windows over the whole output; a
remainder shorter than half a window joins the previous one. Long outputs (more
than 600 windows) use an even stride of 600 windows that includes the last.

**Per-block content.** Every 10 ms block (480 samples) is classified by its
reference level:

| Block class | Gate | Rationale |
| --- | --- | --- |
| Reference >= -60 dBFS | SNR >= 1.5 dB (`audio_block_snr`) and level difference <= 3 dB (`audio_level`) | Correct encodes measured >= 3.2 dB worst case: a low-level opening transient after second-generation AAC on the whole-Original baseline. Sound-event blocks measured >= 7.9 dB, click blocks 31–65 dB. Wrong content is at most 0 dB by construction: missing content is exactly 0 dB, a different program at the same level about -3 dB. The synthetic negative (100 ms of a different program at the same level inside a 1 s window) fails all eleven affected blocks while whole-window SNR stays above 6 dB, which is why the gate is per block. |
| Reference < -70 dBFS | decoded block RMS <= -60 dBFS (`unexpected_sound_in_silence`) | Silent holds, gaps and pauses must stay silent. Correct encodes measured <= -65.8 dBFS in silent blocks next to transients. |
| Between | not gated per block | Included in whole-window SNR, which is reported but not gated. |

A silent window (reference RMS < -70 dBFS) additionally fails if any decoded
sample exceeds -50 dBFS and the decoded peak also exceeds the reference peak by
more than the 3 dB level tolerance. The second condition admits sound the
reference itself carries: an unedited HDR Original whose click starts exactly at
sample 48,000 has source AAC pre-echo at -48.4 dBFS inside the otherwise silent
first window, which the export reproduced at -49.8 dBFS. Exact-zero references
(silent holds, gaps) keep the absolute gate.

**Alignment.** Up to eight nonoverlapping 4,096-sample reference segments per
window (the loudest above -70 dBFS) are each cross-correlated against the
decoded track over every lag in [-2048, 2048], which exceeds both documented AAC
failures (1,024 samples late, 1,088 early). Correlation is stereo (both
channels in one inner product); a mono sum cancelled the opposite-polarity
fixture click (L=0.749, R=-0.649) and once produced a spurious 191-sample lag.
Decoded samples outside the file's presentation read as silence rather than
skipping lags, and decoded priming before output zero is retained as context,
so an early shift at the very start of the output is observable.

Each segment is classified:

- `verified_zero`: a unique maximum at lag 0.
- `offset`: the strongest maximum is elsewhere. Any nonzero lag fails,
  including a few samples on smooth low-frequency content.
- `periodic`: another local maximum at least 32 lags from the peak lies within
  1e-3 of it, with lag 0 among the equal maxima. Such content (an exact tone)
  cannot show a shift that is a multiple of its period. Ambiguity is decided
  only from separated maxima, never from the width of the main lobe.
- `uncorrelated`: peak correlation below 0.5.

The window's `offset_status` is `offset` if any segment measured one,
`verified_zero` if at least one segment verified zero, `unobservable` if every
segment is periodic, `uncorrelated` otherwise, and `not_applicable` for silent
windows. `measured_offset_samples` is present only for `verified_zero` (0) and
`offset` (the measured lag); an unobservable window reports no offset and fails
`audio_offset_unobservable`. An export whose signal is purely periodic therefore
fails rather than passing as offset-verified. Nothing is shifted, trimmed or
event-aligned to pass.

## Report

Report schema version 3 adds per-picture `previous_index` and `next_index`.
Each is null when that reference neighbor does not exist, or contains:

- `status`: `observable` or `unobservable` at the declared luma fidelity.
- `reference_luma_rms`: the reference-to-reference RMS separation in codes.
- `max_error_luma_rms`: the per-reference bound `epsilon`; separation must
  exceed twice this value for `observable`.

`summary.index_observable_comparisons` and
`summary.index_unobservable_comparisons` count directed neighbor comparisons,
so checking both adjacent frames counts that pair twice.
`summary.pictures_without_index_neighbors` counts frames with neither
neighbor. These are coverage counts, not counts of verified source
identities. `previous_luma_psnr_db`, `next_luma_psnr_db` and
`summary.min_neighbor_margin_db` retain their raw values, including
unobservable comparisons; a negative margin can therefore coexist with a
passing report.

Version 2 introduced per-picture `local_luma_error` and
`summary.max_local_luma_error`, `output_color` (output policy, reason,
`hdr_sources`, `tone_map_peak_nits` and the PQ mastering volume),
`picture_bits` (8 or 10), the HDR container observations in `movie.color`
(`sample_entry`, `hevc_profile_idc`, `decoder_profile`, `mastering`,
`content_light`; omitted for SDR) and `hdr_light` (PQ only). Plane
`max_abs_error` and `mean_abs_error` are in codes of the compared depth.

## Memory and cancellation

Decoded audio is streamed: a window's buffer (plus alignment margins) exists
only while the decoder is inside it and is compared and dropped as soon as the
decoder passes its end. Overlapping explicit windows may retain at most 8 Mi
samples together. Picture references keep at most three frames. Selection
bounds: automatic pictures stride to at most 600 frames; explicit windows are
limited to 600 windows of at most 480,000 samples.

## Fixtures and scope

[Recipe fixtures](../crates/deadpan-cli/tests/preview_export/recipes.rs) build
real one-Original projects from `cfr-bframes.mp4` (burnt-in frame counter,
moving square, stereo clicks) through the headless `command` API, then export
them with public `render` and verify them with `verify-export`:

- `black_pause_export_matches_preview_and_later_edits_are_reported` runs in the
  default gate (about 42 s debug, 2 s optimized). After the verified export it
  checks four negatives: a same-duration 1.35x reframe and -12 dB trim
  committed afterwards (`gross_structural_mismatch`, `audio_level`, offset still
  0); an altered copy whose audio edit no longer skips priming (measured
  offset +2,048 with encoder ABI 2, priming cross-check still agrees); an altered copy whose video
  edit starts one frame late (a leading picture discarded, every later picture
  on the previous ordinal, the last ordinal missing); and the original movie
  against the shorter pre-pause revision (range, frame count and audio edit
  duration all reported).
- [HDR fixtures](../crates/deadpan-cli/tests/preview_export_hdr.rs) build
  one-Original projects from `hdr-pq-av.mp4` and `hdr-hlg-av.mp4` (320x180,
  30 fps, 60 frames, AAC, a frame-30 marker and a 1 kHz click at Original
  sample 48,000) and from the grain fixtures `hdr-pq-grain-av.mp4` and
  `hdr-hlg-grain-av.mp4` (the same size and audio; real-like footage with
  edges, highlights, motion and clumped film-grain noise, generated by
  `generate_hdr_fixtures.py grain`) with a cut, a two-play Repeat, a freeze
  Hold and a caption (42 frames). `hdr_worker_exports_match_their_committed_previews` encodes them
  through the real encoded render worker (hardware HEVC Main10, B frames
  requested) and verifies tags, provenance, clli, PSNR, local structure and
  zero audio offset, plus missing-caption (`local_structure_mismatch` on
  exactly the four captioned frames) and 1.35x reframe negatives.
  `hdr_public_render_matches_its_committed_preview` does the same through
  public Render (automatic HDR encoder admission, finished-file HEVC
  verification and publication). Both are ignored in debug builds:
  `cargo test --release --locked -p deadpan-cli --test preview_export_hdr`.
  The default-gate tests check the branch truth table on real committed
  documents, BT.2408 caption placement in the HDR references (fill at PQ code
  573 / HLG code 721) with the source patches preserved, an SDR Original
  rendering SDR H.264, and the SDR fallback of a PQ project with a registered
  SDR video (tone-mapped references never exceed Y 235; its public Render
  verifies as SDR). `DEADPAN_PREVIEW_EXPORT_HDR_RESULTS=<new.json>` records the
  HDR table. `deadpan_cli::export_verification::reference_pictures` renders
  single references for such checks.
- [VFR fixtures](../crates/deadpan-cli/tests/preview_export/vfr.rs) build eleven
  projects from both retained variable-duration Originals: complete A/V
  endpoints, freeze, Repeat gaps, nested Retime/Repeat, Preserve and Tape speeds,
  reverse, ping-pong, cutaway bounce and bleep. An independent authored PTS table
  and manually flattened recipe clocks check every plan position and exported
  reference provenance. The optimized matrix runs with
  `cargo test --release --locked -p deadpan-cli --test preview_export vfr_recipe_exports`;
  `DEADPAN_PREVIEW_EXPORT_KEEP=1` retains packages, movies, per-fixture reports
  and `results.json`. The plan matrix also runs in the default gate. All eleven
  exports, 867 pictures and 28 audio windows pass in the
  [2026-10-09 qualification](qualification/vfr-recipes-2026-10-09.md).
- `every_recipe_export_matches_its_committed_preview` exports every recipe
  fixture (44: 38 on `cfr-bframes.mp4`, four accepted Generated Holds and two
  larger generated Originals, below).
  Debug renders cost about four seconds per output second, so it is ignored in
  debug builds. The release-mode gate is
  `cargo test --release --locked -p deadpan-cli --features synthetic-worker --test preview_export`
  (about 213 s on an M5 Max, of which the two large fixtures take about 82 s
  of render and verification). It needs a development `ffmpeg` with
  `libx264` (`DEADPAN_BRIDGE_FFMPEG`, else `/opt/homebrew/bin/ffmpeg`) and a
  current `deadpan-media-worker` and `deadpan-track` executables beside the
  tested `deadpan-cli` (`cargo build --release --locked -p deadpan-media-worker
  -p deadpan-track`, or `DEADPAN_MEDIA_WORKER` with its sibling tracker).
  Rebuild both after worker-protocol changes; a present stale helper is not
  sufficient. A fixture whose tool or feature is missing is
  reported `SKIPPED` on stderr and as a `skipped` row in the results file,
  never silently dropped; record results only from a run with none skipped.
  `DEADPAN_PREVIEW_EXPORT_RESULTS=<new.json>` records the table, and
  `DEADPAN_PREVIEW_EXPORT_KEEP=1` keeps packages and movies.
- [Generated Hold fixtures](../crates/deadpan-cli/tests/preview_export/generated.rs)
  (`--features synthetic-worker`) fill `black_pause`'s 12-frame Hold with the
  synthetic worker and accept it: real conditioning from the committed
  boundary pictures, the durable attempt lifecycle, `deadpan-media-worker`
  bundle qualification, generated-object publication and
  `accept_generation_bundle`; only the model is replaced. The result is an
  ordinary schema-3 Generated Hold with all six retained objects, and the
  export reads its sampled master through the shared cold reader.
  `generated-pause` (the accepted Hold in place), `generated-repeat` (two
  plays of a local Sequence with one-frame Original endpoints and a 4-frame
  Background gap, authored before Default generation), `generated-reframe` (static
  1.35x live Hold framing) and `generated-prefix` (shortened to 7 frames,
  reusing the accepted sampled prefix) expect Hold-local frame `k` to show
  sampled frame `k` (`provenance.kind = generated`, `source_frame = k`) in
  every play. The `generated-pause` movie is also verified against the
  pre-acceptance Background revision and must fail on exactly the 12 Hold
  frames.
- Larger media: `large-1080p` and `large-2160p` generate their Original in
  the test (deterministic `testsrc2` with 48 kHz stereo AAC aperiodic chirps,
  long-GOP H.264 High with GOP 150, three B frames with pyramid references,
  BT.709 limited range, left chroma; generator in
  [recipes](../crates/deadpan-cli/tests/preview_export/recipes.rs)), so
  nothing large is committed. `large-1080p` is 1920x1080, 180 Original
  frames, cut to 150 with a 15-frame freeze pause, a two-play Repeat and a
  1.35x zoom (195 output frames); `large-2160p` is 3840x2160, 60 frames with
  an 8-frame freeze pause and a 1.35x zoom (68 output frames).
  `DEADPAN_PREVIEW_EXPORT_ONLY=saturation,j-cut` builds only the named
  fixtures for a focused rerun, prints how many it skipped, refuses unknown
  names and refuses to run when `CI` is set; record results from a full run.

Measured results are in the
[qualification record](qualification/preview-export-2026-10-04.md), the
[Generated Hold and large-media record](qualification/preview-export-2026-10-06.md) and, for HDR,
the [HDR record](qualification/hdr-preview-export-2026-10-05.md). The harness
does not cover HDR display or EDR presentation (the comparison is in encoded
code values), native key paths (fixtures use the headless command API that
the keys resolve to), real model output (Generated Hold fixtures use the
synthetic worker's blended footage; the model is replaced, the acceptance and
picture paths are not), Generated Holds in HDR projects (accepted footage
forces the SDR branch, which no fixture exercises), or physical
playback/listening; device audio and display remain separate qualification.
The large fixtures are synthetic test patterns of at most six seconds: they
exercise 1080p and 2160p geometry, long-GOP B-frame decode and encode, but not
camera footage, long programs or decode/encode throughput limits. Captions are drawn by the shared
picture session on both sides; the `delayed-caption` fixture additionally
verifies its movie against a later caption-free revision and requires exactly
the captioned frame to be flagged.
