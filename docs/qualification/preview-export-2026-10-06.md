# Preview-versus-export verification: Generated Holds and large media, 2026-10-06

Scope: the [preview/export harness](../PREVIEW_EXPORT_VERIFICATION.md) extended
with four accepted Generated Hold fixtures and two larger generated Originals
(1920x1080 and 3840x2160). Each fixture is exported through public headless
`render` and compared with its committed revision by `verify-export`. This is
fixture evidence for the automatic SDR path on one machine. It is not release,
real-model, HDR, device playback or listening qualification.

## Environment and source identity

- Apple M5 Max, 128 GiB, arm64, macOS 26.5.2 (25F84); Metal backend; automatic
  encoder policy `AutomaticSdrV1`. Rust 1.97.1.
- Pinned FFmpeg prefix `/private/tmp/deadpan-ui-ffmpeg/prefix` for Deadpan.
  Test-only tools: Homebrew `ffmpeg` 9.0.1 with `libx264`/`libx264rgb`
  (generates the large Originals and encodes the synthetic worker's
  footage; Deadpan never links it) and a release `deadpan-media-worker`
  beside the tested `deadpan-cli`.
- Git HEAD `8108f964065d4b35d5e2e2e3d5a5bfd6ecc66bc0`, with a dirty shared tree.
  This change touches only `crates/deadpan-cli/tests/preview_export.rs`,
  `crates/deadpan-cli/tests/preview_export/recipes.rs`, the new
  `crates/deadpan-cli/tests/preview_export/generated.rs` and these docs.
  Other agents had uncommitted changes in progress and the build included them:
  AI variant retention and bridge colour (`crates/deadpan-store`
  generation/retention/storage, `crates/deadpan-models`,
  `crates/deadpan-cli/src/generation/**`, `crates/deadpan-app/**`),
  resumable proxies (`crates/deadpan-cli/src/proxy/**`,
  `crates/deadpan-media` proxy, `native/deadpan-media-worker` proxy and
  converter) and perf examples. The exact dirty path list was captured
  before the run. These results apply to that tree, not to HEAD alone.
- Command: `DEADPAN_PREVIEW_EXPORT_RESULTS=<new.json> cargo test --release --locked -p deadpan-cli --features synthetic-worker --test preview_export -- --nocapture`.
  All 3 tests passed in 212.6 s of test time (333.5 s wall, including the
  incremental release build). No fixture was skipped; 44 of 44 passed.
- Strict Clippy: `cargo clippy --locked -p deadpan-cli --all-targets -- -D warnings`
  passes, both without and with `--features synthetic-worker`.
- Debug plan test (`every_recipe_builds_a_valid_package_whose_plan_matches_its_expectations`,
  which runs in the default gate) on the six new fixtures plus `black-pause`:
  38.1 s.

## New fixtures

Generated Hold fixtures ([generated.rs](../../crates/deadpan-cli/tests/preview_export/generated.rs))
start from the `black-pause` recipe: `cfr-bframes.mp4` shortened to Original
[12, 42), with a 12-frame silent Background Hold `black` at Edit 15, between
Original 26 and 27. The test fills the Hold with the synthetic worker
(`deadpan_cli::generation::attempt::synthetic`, a seeded linear blend of the
two boundary pictures with a coloured band) and accepts the result. Only the
model is replaced. Conditioning, the durable attempt, bundle qualification,
generated-object publication and `accept_generation_bundle` all run
unchanged. The accepted Hold is a `HoldVideo::Generated` (schema 3, six
retained objects). The sampled master has one picture per Hold frame, so
Hold-local frame `k` must report `provenance.kind = generated` with
`source_frame = k`. The plan smoke test checks the same expectations against
the compiled plan's `accepted.frame`.

| Fixture | Construction after acceptance | Frames | Hand-derived provenance |
| --- | --- | --- | --- |
| generated-pause | none | 42 | 0..15 Original 12..27; 15..27 sampled 0..12; 27..42 Original 27..42 |
| generated-repeat | `WrapRepeat` 2 plays with a 4-frame silent Background gap | 58 | 15..27 and 31..43 sampled 0..12; 27..31 Background; 43..58 Original 27..42 |
| generated-reframe | `SetFraming` static centered 1.35x on the Hold | 42 | as generated-pause |
| generated-prefix | `SetHoldDuration` 12 → 7 (reuses the accepted prefix) | 37 | 15..22 sampled 0..7; 22..37 Original 27..42 |

Audio expectations for the limited bus: a quiet window inside the Hold
(sample 30,000, plus 45,000 in the Repeat's second play and gap), and the
click 19,219.2 (12 frames), 44,844.8 (28 frames) or 11,211.2 (7 frames)
samples after its base position 28,781. That puts it in windows 47,795,
73,500 and 39,900.

The large Originals are generated in the test, not committed
(`large_media` in [recipes.rs](../../crates/deadpan-cli/tests/preview_export/recipes.rs)).
The source is `testsrc2` at 30000/1001 (timescale 30000), encoded as
long-GOP H.264 High: GOP 150, keyint_min 150, no scene cuts, 3 B frames with
pyramid references, 3 references, CRF 20, one encoder thread, BT.709 limited
range with left chroma siting. Audio is 48 kHz stereo AAC at 192 kb/s. Every
half second from 0.2 s it carries a 100 ms linear chirp from 300 to 1500 Hz
(L 0.7, R -0.6) and is otherwise silent. The chirps are aperiodic, so
alignment can verify a zero offset, and every cut lies at least 0.2 s from
one. In this run the 1080p source was 6,513,943 bytes (SHA-256
`4d8ddda7…0ce11c`) and the 2160p source 7,943,235 bytes (`5b40297e…a8160`).
These hashes are reported in each fixture's notes and are not asserted.

| Fixture | Construction | Frames | Hand-derived provenance |
| --- | --- | --- | --- |
| large-1080p | 1920x1080, 180 frames; delete [150, 180); `,h` freeze 15 frames at Edit 60; split 105/135 and `WrapRepeat` 2 plays; static 1.35x zoom on [0, 60) | 195 | 0..60 Original 0..60; 60..75 Original 59; 75..105 Original 60..90; 105..135 and 135..165 Original 90..120; 165..195 Original 120..150 |
| large-2160p | 3840x2160, 60 frames; `,h` freeze 8 frames at Edit 30; static 1.35x zoom on [0, 30) | 68 | 0..30 Original 0..30; 30..38 Original 29; 38..68 Original 30..60 |

Large-fixture audio windows: chirps are loud and the freeze pause and the
gaps between chirps are quiet. They include the same chirp in both Repeat
plays (Edit samples 177,724 and 225,772).

## Results (optimized build, every frame and every audio window)

| Fixture | Pictures | Min Y PSNR dB | Min Cb/Cr PSNR dB | Max thumbnail MAD | Min neighbor margin dB | Audio windows (signal) | Gated blocks | Min block SNR dB | Max block level dB | Offset status | Render s | Verify s | Passed |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| generated-pause | 42 | 52.5 | 58.3 | 0.063 | 9.8 | 1 (1) | 1 | 40.3 | 0.03 | verified_zero | 1.29 | 0.43 | yes |
| generated-repeat | 58 | 52.5 | 58.3 | 0.069 | 0.0 | 2 (1) | 1 | 55.7 | 0.00 | not_applicable, verified_zero | 1.37 | 0.45 | yes |
| generated-reframe | 42 | 50.0 | 58.3 | 0.072 | 8.9 | 1 (1) | 1 | 40.3 | 0.03 | verified_zero | 1.22 | 0.39 | yes |
| generated-prefix | 37 | 52.6 | 58.3 | 0.057 | 9.8 | 1 (1) | 1 | 55.0 | 0.00 | verified_zero | 1.23 | 0.38 | yes |
| large-1080p | 195 | 38.3 | 40.1 | 0.167 | 0.0 | 7 (7) | 129 | 20.3 | 0.18 | verified_zero ×7 | 15.34 | 18.88 | yes |
| large-2160p | 68 | 44.4 | 44.6 | 0.068 | 0.0 | 2 (2) | 42 | 38.6 | 0.03 | verified_zero ×2 | 26.70 | 21.32 | yes |

Every movie declared AAC priming of 1,024 samples and had no failures. The
38 existing fixtures all passed again: minimum luma 53.1 dB, and 91.3 s of
render plus verification together. All 44 fixtures took 180.3 s of render
plus verification. The two large fixtures account for 82.2 s of that, and
the 2160p render is the largest single cost (26.7 s for 68 frames).

The large test patterns lower the margins without approaching any gate:

- 1080p luma is 38.3 dB, 6.3 dB above the 32 dB gate.
- 1080p chroma is 40.1 dB, 8.1 dB above the gate. `testsrc2` has saturated
  colour edges at full resolution.
- The lowest block SNR is 20.3 dB, against a 1.5 dB gate.

Neighbor margins of 0 dB come from freezes and repeated stills. On the
Generated Holds, the blend between adjacent sampled frames still separates
neighbors by 8.9–9.8 dB.

## Negative check

The `generated-pause` movie was also verified against `generated-pause-r03`,
the pre-acceptance revision whose Hold is Background, at frames 14, 15, 20, 26
and 27 without audio. The run failed. Frames 15, 20 and 26 were flagged and
frames 14 and 27 were not. This confirms the comparison distinguishes the
generated pictures from the fallback they replaced. The existing negatives
(black-pause reframe/trim, AAC priming shift, one-frame video shift,
wrong-range revision, delayed caption) passed unchanged.

## Not covered

- Real model output. The synthetic footage is a deterministic blend. It
  exercises acceptance, retained-object admission, the shared cold reader,
  sampled timing and framing, but not model content.
- A Generated Hold in an HDR project. Accepted footage forces the SDR branch;
  no fixture exercises that fallback with generated pictures. HDR expansion
  was skipped: the existing HDR recipes already include a cut, a two-play
  Repeat, a freeze Hold and a caption, and another HDR recipe would add no
  new path at useful cost.
- Camera footage, long programs and throughput. The large fixtures are
  synthetic test patterns of at most 6 s. They prove geometry, long-GOP
  B-frame decode/encode and timing at 1080p and 2160p, not real-content
  compression behavior or long-duration performance.
- Generated fixtures skip, with a reported `skipped` row, when the binary is
  built without `--features synthetic-worker` or when the development
  `ffmpeg` or `deadpan-media-worker` is missing. CI's debug gate builds them
  only when those tools are present.
- Native key paths, physical display, device audio and listening, as before.
