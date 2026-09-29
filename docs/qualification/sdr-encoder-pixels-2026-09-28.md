# Shared SDR encoder pixels

The shared renderer now produces bounded, owned Rec.709 encoder planes from
its composed linear working target. On actual Metal, all 172,260 I420 codes in
two synthetic fixtures exactly match an independent reference. Hardware H.264
encode/decode retains their declared interpretation and timing, with maximum
error three codes. The same pair passes ASan/UBSan. This qualifies a library
pixel boundary and these fixtures; DP-16 remains partial and DP-17 remains open.

See the [pixel contract](../SDR_ENCODER_PIXELS.md),
[harness instructions](../../tools/media-qualification/compatible/README.md#shared-renderer-encoder-planes)
and [retained evidence](../../tools/media-qualification/evidence/2026-09-28-sdr-pixels/README.md).
The synthetic files, complete decoded planes, command logs, source inventories,
failures and exact Cargo artifact identity are retained. No user media is used.

## Measured picture path

The Rust example sends known linear Rec.709 RGBA8 through the production
`render_composed` path with identity geometry, then reads its linear Rec.2020
`Rgba16Float` working texture. The new CPU conversion transforms signed working
values to linear Rec.709 before the SDR clip, applies the BT.709 OETF/matrix,
and filters left-sited chroma before quantizing tight limited-range I420.
It never reads the sRGB display texture or reconstructs composition in FFmpeg.

The independent f64 reference starts from known input pixels and does not call
the production color transform or decode its working half floats. Broad color
patches, near-knee neutrals, one-column/one-row alternation and two-axis chroma
patterns cover the complete image. The 318-pixel case has 16 padding bytes per
working row; poisoning that padding with nonfinite half patterns leaves output
unchanged. Source PTS is deliberately negative and separate from output time.

All 22 Metal checks pass, including two output sizes, exact tight plane lengths,
single-flight rejection, cancellation before admission and during work, terminal
ticket rejection, expired deadlines, dropped-ticket recovery and padding.
The predeclared renderer tolerance is one code for GPU/half-float rounding;
measured error is zero in every Y, Cb and Cr code for both sizes.

The native probe consumes and hashes the actual renderer bytes, not the
reference. Each emitted file is one 30 fps, video-only frame encoded by hardware
VideoToolbox H.264 High at the harness's 2 Mbit/s setting. A fresh FFmpeg decoder
drains completely. Both files retain exact geometry, 8-bit 4:2:0, progressive
picture, explicit square pixels, BT.709 primaries/transfer/matrix, limited range,
left chroma, PTS zero and duration 1/30 second. Fast-start is present and edit
lists are structurally absent. The observed unknown frame SAR `[0,1]` resolves
through explicit stream `[1,1]`; malformed ratios cannot use that fallback.

Every decoded code is compared against the actual input. Bounds were fixed
before measurement at maximum 12 codes and mean absolute error 2 codes per
plane; neither bound was widened.

| Fixture | Y maximum / mean | Cb maximum / mean | Cr maximum / mean |
| --- | --- | --- | --- |
| 320×180 | 3 / 0.21479 | 2 / 0.03750 | 2 / 0.04750 |
| 318×180 | 3 / 0.22570 | 2 / 0.03760 | 2 / 0.05101 |

Normal and sanitized runs pass all 20 media checks and their separate source,
library, input and final-file admission checks. No process or sanitizer fault
occurs. Instrumentation covers the C probe, not FFmpeg, Rust or Apple frameworks;
macOS leak detection is disabled. These small debug-build measurements are not
playback/export throughput or physical-display qualification.

## Verification and review

Base commit: `31e28647df6b1c5e088bca0d073336df0aef74d3`.
Host: Apple M5 Max, 128 GiB, macOS 26.5.2, Apple clang 21.0.0 and SDK 26.5.
Rust 1.97.1, wgpu 30.0.1, pinned LGPL FFmpeg 8.0.3, macOS 15 native deployment
target. The existing successful build receipt is re-admitted; FFmpeg is not
rebuilt. The receipt SHA-256 is
`baf7437c0cf4a61c94d75efd92db50b5ed3a96b2fa8f1e891284bdcf7c3d78c7`.

- Locked full workspace tests pass: 2,070 tests, zero failures or ignored tests,
  155 target result records including documentation, in 1,353.83 seconds.
- Strict full-target workspace Clippy and formatting checks pass. The first
  Clippy invocation found array-literal indexing inside `json!` needed
  parentheses; its failed log is retained. It finished before the one-line fix.
- All 101 Python tests pass in 6.56 seconds including the outer runner. Review
  caught malformed `[0,0]` SAR being treated as unknown; the fix reuses the strict
  existing oracle and covers malformed/Boolean fields and valid stream fallback.
  The independent reviewer confirmed that correction.
- Cargo's unchanged workspace artifact inventory finishes in 0.85 seconds. Its
  exact example executable is hashed before/after execution; the actual Metal
  run passes in 3.20 seconds. No stale binary glob or separate feature graph is
  used, and no passing workspace tests are rerun.
- The first native compile fails because POSIX declarations hide Darwin's
  `O_NOFOLLOW`. The exact source and compiler output are retained. Enabling
  `_DARWIN_C_SOURCE` before includes follows the installed SDK guard and keeps
  the no-follow check. Independent review confirms the fix. Corrected native
  qualification passes in 4.95 seconds; ASan/UBSan passes in 6.89 seconds.
- The existing 120-frame, 60 fps default-edit-list encoder dispatch control
  passes every required check in 4.69 seconds. Of its 105 recorded checks, only
  diagnostic H.264 packet-byte equality differs because mux framing changes;
  required AAC packet equality and decoded picture/timing checks pass.

The renderer, example and C/Python consumer had independent review. Only the
parent executed compilers/tests/GPU/media. Rust and Cargo source identities are
unchanged from successful lint through the workspace gate, artifact inventory,
Metal execution and formatting check. C/Python authoring was independent of the
Rust gate; native source inventories stay unchanged within each measurement.
No preview widget, keybinding or shader changed, so the previously recorded UI
and visual evidence was not repeated for this backend addition.

## Remaining export work

These video-only fixtures did not decide the AAC edit-list policy. The
[FFmpeg timing failure](encoder-timing-2026-09-28.md) and independent
[AVFoundation result](native-audio-2026-09-28.md) remain unchanged. The user
separately approved the §22.3 timing-metadata revision on 2026-09-28; that
decision retains complete emitted-file verification and does not extend these
video-only fixtures' scope.

A fixed-revision project picture session must admit historical receipts and
immutable original/generated bytes, resolve exact plan frames and carry the
shared framing/captured context into this boundary. Accepted generated media
and Still providers need qualified readers; unsupported providers must fail
explicitly. Final-render process isolation, complete audio/mastering and mux
policy, native video decoding, closed GOP independence, real footage/full-size
color and quality, HDR, output verification, atomic publication and the native
one-action workflow remain required. No product requirement or gate is closed
by these fixtures.
