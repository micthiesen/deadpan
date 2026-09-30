# Automatic encoder probe qualification

This qualifies the [deterministic admission boundary](../AUTOMATIC_ENCODER_ADMISSION.md).
It does not qualify durable automatic Render jobs, public controls, arbitrary
project effects or the release hardware/OS matrix.

## Boundary and retained evidence

The host derives actual-raster/rate probe inputs and gives every attempt a fresh
identity. Only an exact admitted native capability/timing failure permits the
next mode. Protocol faults, malformed tails, other failing exits and cleanup
uncertainty cannot authorize a fallback. The selected file passes complete MP4,
picture/GOP and ordinary/manual audio inspection, followed by expected-content
checks and private byte admission after supervised teardown.

Host: Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1 and pinned LGPL
FFmpeg 8.0.3. The reports retain helper hashes, kernel identity and observed codec
versions. The initial 320x180, 30000/1001 run reproduced hardware B-frame PTS 1001
before DTS 2002, then selected hardware without B-frames. All six stereo events
landed on their exact samples. Per-plane maximum errors were 4, 7 and 5 code
values. No content threshold was changed after the run.

The final probe packet cap was then reduced from the general encoder allowance
to 1024. The initial report remains evidence of the earlier run, not the final
packet-bound execution.

## Final results

The [retained evidence](../../tools/media-qualification/evidence/2026-09-30-encoder-admission/README.md)
contains the commands, source inventories, complete reports, exact synthetic
movies, independent AVFoundation PCM, review notes and a rehashed manifest.
The final product source inventory is
`f8b70d4ff7a0673e537a4f1f8ea8f9781333749cdbbca02aed195638d64baa9b`.

- Locked workspace tests: 2,384 passed in 170 result groups; zero failed or ignored.
- Strict workspace Clippy across all targets, formatting and the helper build passed.
- Native Metal initialization, window startup and shutdown passed.
- Four final native cases passed complete verification and expected-content checks.
  All 190 pictures were also decoded through fresh GOP sessions. Independent
  AVFoundation decoding placed all 24 signed stereo events at their exact samples,
  with no alignment or sample tolerance.

| Raster | Rate | Frames | Audio samples | Maximum Y/Cb/Cr error |
|---|---|---:|---:|---|
| 320x180 | 30000/1001 | 46 | 73,674 | 4 / 7 / 5 |
| 640x360 | 60/1 | 91 | 72,800 | 8 / 11 / 15 |
| 1920x1080 | 30000/1001 | 46 | 73,674 | 9 / 9 / 13 |
| 640x360 | 4/1 | 7 | 84,000 | 8 / 7 / 7 |

All four selected hardware without B-frames. The first three retained an exact
hardware B-frame timestamp-order rejection. The 4 fps case began without B-frames
because its GOP target was two. The final 1024-packet cap bounded the native MP4
header allocation to 1,179,648 bytes, including its fixed 1 MiB overhead.

The final helper SHA-256 was
`1cdbe0ddd00b5d58fb72c24f0fdbe8328f5b129c4d458a675347ac91ed587a49`.
The host record also hashes the seven development-prefix libraries for reproduction;
these records do not add product runtime-binding semantics.

## Small-raster failures

Actual 14x16, 16x16 and 64x64 probes at 30/1 all failed with
`source decode resource_limit: source dimensions exceed configured bounds`.
They retained the preceding B-frame timing rejection, stopped on the Output
failure and confirmed cleanup. No failed movie became an admitted candidate.

Independent source inspection found that both decoder callers already budget
macroblock-rounded geometry, and the pinned FFmpeg build uses 16-byte stride
alignment. The diagnostic does not name the rejected dimensions or stage.
These observations cannot establish an encoder minimum or justify widening the
bound. Capture actual dimensions and limits, or inspect the failed movie's SPS,
before changing admission. These three rasters remain unqualified. The
[geometry review](../../tools/media-qualification/evidence/2026-09-30-encoder-admission/geometry-review.md)
retains the inspection and observed failures.

## Review and corrected failures

Independent review found that a post-probe cancellation/deadline check could
replace an unconfirmed cleanup error. Removing that replacement preserves the
error; control is rechecked before any permitted next probe. Review also caught
a cancellation call placed in the exit branch instead of the regressed-progress
branch. It now runs on regression, and the hostile fixture must receive the exact
cancel identity/token and leave a receipt before it can finish. That regression
test passes. A final control check precedes issuing the owned qualification.

The first CLI compile failed on unsupported SHA-256 LowerHex formatting. Explicit
byte formatting fixes it; corrected focused tests and the initial native run
pass. One preliminary Clippy queue was deliberately interrupted to apply the
reviewed cancellation correction. Its incomplete log is retained and is not a
passing check. The final source review has no remaining actionable findings.

The first native evidence script incorrectly expected the MP4 header allocation
to be below 1 MiB. Its corrected assertion includes the existing fixed overhead.
The initial summary remains retained. Successful probe files were reused for
the independent audio checks; product source and content thresholds did not
change after the full workspace gate.

## Limits

Normal builds were used. Sanitizers, physical listening, optional UI replay,
painted interaction and GUI performance were not rerun. UI source did not change.
Native startup/shutdown was smoke-tested; the CLI probe runs exercised the new
private worker dispatch.
Loaded-library byte fingerprints and a runtime-bound project consumer remain
open. The unchanged legacy verifier's absent-B error remains fatal. No dynamic
encoder setting, current-runtime probe report or synthetic fixture proves full
project rendering, complete mastering, HDR or release acceptance.
