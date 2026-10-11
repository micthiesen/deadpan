# Proxy interpretation fixtures

Short H.264 clips for `tests/proxy_real_media.rs`, made with the
development-only Homebrew FFmpeg 9.0.1 and libx264 (GPL, not linked by
Deadpan) from `testsrc2`, with `-x264-params keyint=4:scenecut=0:bframes=2`
and color tags set by `setparams`. The pinned FFmpeg 8.0.3 decoder admits
them in the tests.

| File | Picture | Tags | Purpose |
| --- | --- | --- | --- |
| `uhd-bt709.mp4` | 3840×2160, 8 pictures at 24 fps | BT.709 limited | Downscale path to 1920×1080, chosen by policy |
| `anamorphic-bt601.mp4` | 1440×1080, SAR 4:3, 6 at 25 fps | BT.601 (SMPTE 170M) matrix, BT.709 primaries and transfer | Non-square pixels and a non-BT.709 matrix |
| `p3-srgb.mp4` | 640×360, 6 at 30 fps | Display P3 primaries, sRGB transfer, BT.709 matrix | Non-BT.709 primaries and transfer |
| `rotated-bt709.mp4` | 320×180, 6 at 24 fps, display rotation 90 | BT.709 limited | Rotation (`-display_rotation 90 -c copy` of a plain encode) |

SHA-256:

```text
a04f59c16b66eaac02605093f5565efb0edaa762acdb0d8877c1afb5a891524b  uhd-bt709.mp4
df550d5c16c6e1abba189c1e7c5280c6c56b2e107c6ead1e0b131f7717f8222c  anamorphic-bt601.mp4
24add7db9720283c700a412dfbd2d150d5bb5aeac8c9863f81ba44f150941dc8  p3-srgb.mp4
54da8a94ce64acb1bccd48c84feacbec9dd0cee4ff51c1653aa1a48e09b266d5  rotated-bt709.mp4
```

## Recorded retry success

`cfr-retry-success.mp4` and its exact `cfr-retry-success.json` worker reply
isolate the stalled-worker lifecycle test from a second, unrelated VideoToolbox
failure after its deliberate first stall. The test still decodes and verifies
all 120 picture timings and sampled pixels against the Original, checks the
second staging file byte for byte, and verifies removal and process teardown.
Other real-media tests continue to encode through the actual worker.

This is actual output from `deadpan-media-worker proxy`, generated on the
M5 Max with the qualified FFmpeg 8.0.3 libraries on 2026-10-11: 320×180,
120 intra pictures at exact PTS `n * 1001` in time base `1/30000`, quality 60,
BT.709 and SAR 1:1. The worker reads the existing owned
`deadpan-source/tests/fixtures/cfr-bframes.mp4`; no external footage is used.
The regeneration command, request, reply and executable identity are retained
with the proxy-order qualification evidence.

- Original SHA-256: `5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.
- Worker SHA-256: `ef87a341fd6d2d9ac1d8c78488ddc98cc7d4396c1f14d804b377937e9b1146c7`.
- Proxy SHA-256: `0120663d6db5e06f8e995e3ca70de0a8e8eaf033736d13a45900230605a5f6a0`.
