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
  worker also calls, rendered by the shared `PictureRenderer::render_composed`
  with the same framing layers and captured context. The app worker's own
  session caching and admission code is not exercised. The harness reads the
  composed linear Rec.2020 working target back and converts it at the
  [SDR encoder pixel boundary](SDR_ENCODER_PIXELS.md) into limited Rec.709 I420,
  the like-for-like reference for a decoded H.264 picture (the display texture
  applies the sRGB display transform and is not a comparable code space).
- **This reference shares the picture session with the encoder host.** A
  planning or picture-session bug produces the same wrong picture on both
  sides and passes the PSNR gates. The picture comparison therefore proves
  encode, frame order, timing, color tagging and mux fidelity. The independent
  check of recipe semantics is the per-frame provenance (Original asset,
  source frame and PTS; Generated frame; Background) asserted against
  expectations derived by hand from each recipe in the fixtures.
- **Audio** comes from `OfflineAudioSession`, the limited canonical bus that
  audition also plays (`LimitedAudio`, before monitor gain), read on the
  absolute project sample grid `[B(start), B(end))`. It is likewise the bus the
  encoder consumed.

## Container, color and timing checks

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
  1,024-sample lag). Decoded frames that do not continue the previous frame are
  reported as `audio.gaps`; window samples the file does not present are
  counted as `uncovered_samples` and fail `decoded_audio_gap`. The presented
  end must equal the edit's declared duration and the committed sample count.

## Picture gates

Each selected output frame `n` is compared with reference `n`, and its luma
with references `n-1` and `n+1`. Defaults (`Thresholds::default()`):

| Check | Default | Rationale |
| --- | --- | --- |
| Luma PSNR | >= 32 dB | Measured 53.1–62.2 dB minimum per fixture; a different picture of the moving fixture measures 15–27 dB. |
| Chroma PSNR (Cb and Cr) | >= 32 dB | Measured 53.5–60.1 dB. |
| 8x8 luma thumbnail mean absolute difference | <= 4.0 codes | Gross mismatch gate for wrong framing, zoom or content. Measured <= 0.065; the 1.35x reframe negative fails it. |
| Neighbor margin | reference `n±1` must not beat `n` by > 0.5 dB | `frame_index_mismatch`. The own frame beat its best neighbor by 28–38 dB on moving fixtures (`min_neighbor_margin_db`). |
| Black frame | decoded mean luma <= 20 while reference mean > 32 | `unexpected_black_frame`. |

On static content (freezes, Background, repeated stills) neighbors are
identical, the margin is 0 dB and an off-by-one frame is invisible in pixels.
There it is guarded only by the exact PTS-to-ordinal mapping and the frame
count. Max and mean absolute errors per plane are reported but not gated.

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
sample exceeds -50 dBFS.

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
  offset +1,024, priming cross-check still agrees); an altered copy whose video
  edit starts one frame late (a leading picture discarded, every later picture
  on the previous ordinal, the last ordinal missing); and the original movie
  against the shorter pre-pause revision (range, frame count and audio edit
  duration all reported).
- `every_recipe_export_matches_its_committed_preview` exports all nine recipes.
  Debug renders cost about four seconds per output second, so it is ignored in
  debug builds. The release-mode gate is
  `cargo test --release --locked -p deadpan-cli --test preview_export` (about
  19 s). `DEADPAN_PREVIEW_EXPORT_RESULTS=<new.json>` records the table, and
  `DEADPAN_PREVIEW_EXPORT_KEEP=1` keeps packages and movies.

Measured results are in the
[qualification record](qualification/preview-export-2026-10-04.md). The harness
does not cover HDR, native key paths (fixtures use the headless command API that
the keys resolve to), accepted Generated Holds (no headless fixture exists
without the model runtime), tails, or physical playback/listening; device audio
and display remain separate qualification.
