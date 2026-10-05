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

## Section 8 increment (same day)

Seven more fixtures, run with the same command on the same machine after an
independent review (3 passed in 34.1 s; the earlier nine are unchanged and
still pass):

| Fixture | Construction | Frames |
| --- | --- | --- |
| one-more-time | `:gag one-more-time plays=3 gap=12f shorten=6f` saved and run as macro `m` through headless `macro` (native planner, gap-picture resolver and store admission): Repeat of Edit [12, 24), freeze gaps of 12 then 6 frames holding Original 35 | 72 |
| repeat-gaps-steps | the recorded `:repeat 3 gap=200ms,120ms gain-step=3dB zoom-step=0.08` (`SetRepeat` with plays, gaps and escalation) on Edit [12, 24) through the same macro path; gaps round once to 6 and 4 frames | 64 |
| nothing-happens | `:gag nothing-happens register=r tone=12f silence=12f` at Edit 15 through the macro path, register `r` holding the Original moment [28, 31) as a native copy stores it; room tone from `copied_moment_audio`, then silence | 54 |
| audio-lag | `SetSourceAudioMapping` offset +2,400 samples on the Source under Edit [10, 20) | 30 |
| bed-drop | the sound-event sound cut at Edit 12 by `SoundEvent::cut_at` (the native `:sound-cut` construction): selected placement [0, 2) frames, Hard end | 30 |
| mute-range | mute range [25, 35) on the whole, unshortened Original | 120 |
| off-center | static pose 1.5x centered at (0.35, 0.40) on the base beat | 30 |

Besides picture provenance, these fixtures assert independently derived sound
on the limited bus before export (`inspect-audio --limited`, 256-sample
windows, loud above 0.5 peak, quiet below 0.01):

- One More Time and the gap/step Repeat: each play's click sounds 9,562
  samples after that play's own rounded start (B(12), B(36), B(54) and B(12),
  B(30), B(46)), and the gaps between plays are silent.
- Nothing Happens: the moment hears source samples [44,845, 49,649); its
  4,804-sample loop repeats the click every 4,708 samples from the Hold origin
  24,024 (27,179, 31,887, 36,595, 41,303), the silence Hold is quiet, and the
  Original's click moves 24 frames later.
- Audio lag moves the click exactly 2,400 samples; the bed drop stops at
  19,219.2 instead of about 24,000; the mute range silences the click at
  48,000 while the click at 191,992 still sounds.

| Fixture | Pictures | Min Y PSNR dB | Min Cb/Cr PSNR dB | Max thumbnail MAD | Min neighbor margin dB | Audio windows (signal) | Gated blocks | Min block SNR dB | Max block level dB | Offset status | Render s | Verify s | Passed |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| one-more-time | 72 | 62.2 | 58.3 | 0.019 | 0.0 | 2 (2) | 3 | 38.2 | 0.04 | verified_zero ×2 | 1.56 | 0.69 | yes |
| repeat-gaps-steps | 64 | 53.4 | 58.3 | 0.036 | 0.0 | 2 (2) | 3 | 30.4 | 0.06 | verified_zero ×2 | 1.50 | 0.75 | yes |
| nothing-happens | 54 | 62.2 | 58.3 | 0.019 | 0.0 | 2 (2) | 5 | 39.7 | 0.02 | verified_zero ×2 | 1.32 | 0.58 | yes |
| audio-lag | 30 | 62.2 | 57.7 | 0.019 | 37.6 | 1 (1) | 1 | 55.6 | 0.00 | verified_zero | 1.08 | 0.32 | yes |
| bed-drop | 30 | 62.2 | 57.7 | 0.019 | 37.6 | 1 (1) | 8 | 8.0 | 0.14 | verified_zero | 1.13 | 0.39 | yes |
| mute-range | 120 | 59.8 | 57.6 | 0.031 | 35.5 | 4 (2) | 3 | 3.2 | 0.52 | verified_zero, not_applicable ×2, verified_zero | 1.48 | 0.86 | yes |
| off-center | 30 | 54.8 | 60.8 | 0.039 | 33.7 | 1 (1) | 1 | 55.2 | 0.01 | verified_zero | 1.09 | 0.34 | yes |

No preview/export mismatch was found. The mute-range windows around the muted
click carry no signal and pass the silent-window gate (no decoded sample above
-50 dBFS); its 3.2 dB minimum block SNR is the whole-Original opening transient
already measured above. The review also found that a gap change moved later
plays without retaining their clocks: at 30000/1001 a play moved from frame 8
to 9 by a second gap edit landed at sample 14,415 instead of B(9) = 14,414.
`SetRepeatGaps` now reanchors every interior entry after the first play, and
`ntsc_gap_changes_move_each_play_and_the_suffix_as_identical_samples`
(deadpan-audio, decoded PCM) checks identical samples per play across three
successive gap edits and their inverses. The `recipes` native replay drives
these operations through the production router (75 checks).

## Not covered

Accepted Generated Holds (no headless accepted fixture exists without the
development model runtime), tails, gain envelopes,
per-play overrides, native key paths, odd canvases, nonzero export ranges,
HDR, longer or higher-resolution media, and physical display/listening.
