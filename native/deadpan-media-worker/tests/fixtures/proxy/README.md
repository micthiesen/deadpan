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
