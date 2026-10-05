# Preview-versus-export verification, 2026-10-04

Scope: the [preview/export harness](../PREVIEW_EXPORT_VERIFICATION.md) applied to
nine Section 8 recipe fixtures exported through public headless `render` and
compared with their committed revisions by `verify-export`. This is fixture
evidence for the automatic SDR path on one machine. It is not release, HDR,
device playback or listening qualification.

## Environment

- Apple M5 Max, arm64, macOS 26.5.2; Metal backend; automatic encoder policy
  `AutomaticSdrV1` (hardware H.264 selected by the live probe).
- Source tree at commit `0dda7a8f` plus the uncommitted harness changes.
- Pinned FFmpeg prefix `/private/tmp/deadpan-ui-ffmpeg/prefix`.
- Command: `DEADPAN_PREVIEW_EXPORT_RESULTS=… cargo test --release --locked -p deadpan-cli --test preview_export -- --nocapture`
  (3 passed in 18.8 s). The debug default-gate test
  `black_pause_export_matches_preview_and_later_edits_are_reported` passed in
  about 42 s including its four negatives; a debug run (before those fixes) of the full suite took 225 s.

## Fixtures

Every fixture is a one-Original project from `cfr-bframes.mp4` (320x180,
30000/1001, burnt-in frame counter, moving square, stereo clicks), shortened
to Original frames [12, 42) and edited only through the headless `command` API
([recipes](../../crates/deadpan-cli/tests/preview_export/recipes.rs)). Each
fixture also lists output frames whose expected Original frame or Background
was derived from the recipe semantics; the exported pictures' reported
provenance matched all of them, and the uncompressed plan matched them in the
separate smoke test.

| Fixture | Construction | Frames |
| --- | --- | --- |
| repeat-with-gap | `WrapRepeat` 3 plays, 6-frame silent Background gap | 66 |
| freeze-hold | `InsertTime` 15-frame Freeze (native `,h` provider and captured view), silence | 45 |
| black-pause | `InsertTime` 12-frame Background, silence | 42 |
| retime-half | `WrapRetime` 12 → 24 frames, pitch Preserve | 42 |
| framing | static 1.35x zoom; smoothstep creep 1x → 1.35x; follow of a moving target at 1.35x | 30 |
| cutaway | `SetCutaways` Original 90..96, Hold fit, over Edit [10, 20) | 30 |
| escalating-repeat | `WrapRepeat` 3 plays + `SetRepeatEscalation` +3 dB, +0.08 zoom | 54 |
| gain-trim | `SetAudioTreatments` whole-beat +6 dB trim | 30 |
| sound-event | catalog stereo WAV (declared FL/FR mask) placed with `SetSound` at Edit frame 10 | 30 |

## Results (optimized build, every frame and every audio window)

Second run, after review fixes (per-block audio gates, multi-segment alignment
with a periodicity test, edit/color checks). All three tests passed in 18.7 s.

| Fixture | Pictures | Min Y PSNR dB | Min Cb/Cr PSNR dB | Max thumbnail MAD | Min neighbor margin dB | Audio windows (signal) | Gated blocks | Min block SNR dB | Max block level dB | Offset status | Render s | Verify s | Passed |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| repeat-with-gap | 66 | 54.9 | 57.8 | 0.040 | 0.0 | 2 (2) | 3 | 31.4 | 0.05 | verified_zero ×2 | 1.42 | 0.63 | yes |
| freeze-hold | 45 | 62.2 | 57.8 | 0.019 | 0.0 | 2 (1) | 1 | 65.2 | 0.00 | not_applicable, verified_zero | 1.12 | 0.37 | yes |
| black-pause | 42 | 55.2 | 53.5 | 0.065 | 0.0 | 1 (1) | 1 | 40.3 | 0.03 | verified_zero | 1.11 | 0.33 | yes |
| retime-half | 42 | 62.2 | 58.3 | 0.019 | 0.0 | 1 (1) | 1 | 31.8 | 0.04 | verified_zero | 1.53 | 0.72 | yes |
| framing | 30 | 53.1 | 60.1 | 0.034 | 30.3 | 1 (1) | 1 | 55.2 | 0.01 | verified_zero | 1.16 | 0.35 | yes |
| cutaway | 30 | 62.2 | 58.3 | 0.019 | 0.0 | 1 (1) | 1 | 55.2 | 0.01 | verified_zero | 1.10 | 0.34 | yes |
| escalating-repeat | 54 | 53.3 | 58.3 | 0.035 | 28.0 | 2 (2) | 3 | 40.5 | 0.03 | verified_zero ×2 | 1.40 | 0.68 | yes |
| gain-trim | 30 | 62.2 | 57.7 | 0.019 | 37.6 | 1 (1) | 1 | 55.6 | 0.01 | verified_zero | 1.20 | 0.36 | yes |
| sound-event | 30 | 62.2 | 57.7 | 0.019 | 37.6 | 1 (1) | 19 | 7.9 | 0.68 | verified_zero | 1.21 | 0.37 | yes |

Every movie had one non-empty video edit (media time 0) presenting exactly the
committed frames and one audio edit (media time 1,024) presenting exactly
`B(end) - B(start)` samples, limited Rec.709 container and decoder color with
left chroma, the committed frame count on the exact output grid, no picture
anomalies, no audio gaps, and a first Manual AAC PTS of -1024. A neighbor margin
of 0 dB comes from static runs (holds, gaps, repeated stills) whose neighbors
are identical; there an off-by-one is guarded only by exact PTS mapping.

The whole-Original baseline export (120 frames, before shortening) measured
59.8 dB minimum luma PSNR, a 35.5 dB minimum neighbor margin, verified zero
offsets on its three signal windows and a 3.2 dB minimum block SNR on a
low-level opening transient (second-generation AAC); with sound-event blocks at
7.9 dB this set the 1.5 dB block gate between correct encodes and the 0 dB
(missing) / about -3 dB (different same-level program) wrong-content bound.

## Negative checks

- Default-gate test, after the verified black-pause export:
  - a same-duration static 1.35x reframe and -12 dB trim committed on the
    post-pause beat: frames 10 and 20 still pass, frames 29 and 35 fail
    `gross_structural_mismatch`, window [40000, 64000) fails `audio_level` with
    offset still verified zero;
  - an altered copy whose audio edit media time is 0 instead of 1,024: the
    priming cross-check agrees (first PTS 0, the self-referential case) and the
    alignment reports `offset` with a measured lag of +1,024;
  - an altered copy whose video edit starts one frame (1,001 ticks) late: the
    decoder discards the leading picture, 41 of 42 pictures decode, the final
    ordinal is reported missing and every compared picture fails
    `frame_index_mismatch` and PSNR;
  - the original movie against the pre-pause revision (30 frames): pictures
    beyond the committed range and the audio edit duration are reported.
- A +6 dB increase is not a usable negative for this fixture: the shared
  limiter clamps the near-full-scale click (reference peak -1.7 dBFS against
  -2.5 dBFS). This is correct limiter behavior, not a mismatch.
- Unit tests: 1,024 late, 1,088 early, ±1 sample and an early shift at the very
  start of the output are measured exactly and fail; small shifts (3, -5, 17)
  of smooth lowpassed noise fail as `offset`, not ambiguity; a period-64 tone is
  `unobservable` (never offset 0) both unshifted and shifted by 1,024, a
  period-100 tone shifted 1,088 fails as `offset`, and a short burst makes the
  tone observable in both directions; 100 ms of a different same-level program
  fails eleven blocks while window SNR stays above 6 dB; level loss, sound in
  silence, decoded gaps, frame shift, black frame and reframe are reported.

## Findings

- No preview/export content mismatch was found for these recipes.
- Harness defect found and fixed during qualification: a mono-sum alignment
  cancelled the opposite-polarity click (L=0.749, R=-0.649) and reported a
  spurious 191-sample lag. Alignment now uses a stereo inner product.
- A catalog sound registered from a plain PCM WAVE header (unspecified speaker
  layout) is admitted by `register-source` and `SetSound`, but both the
  audition bus (`inspect-audio --limited`: `AudioLayoutUnsupported`) and Render
  (`render_audio_failed: source channel layout has no supported explicit stereo
  interpretation`) refuse it. Preview and export agree, and the refusal follows
  the no-channel-count-guess rule, but the failure surfaces only at playback or
  export. The fixture uses a WAVE_FORMAT_EXTENSIBLE header with an explicit
  FL/FR mask.

## Not covered

Accepted Generated Holds (no headless accepted fixture exists without the
development model runtime), room tone, tails, gain envelopes and mutes,
per-play overrides, native key paths, odd canvases, nonzero export ranges,
HDR, longer or higher-resolution media, and physical display/listening.
