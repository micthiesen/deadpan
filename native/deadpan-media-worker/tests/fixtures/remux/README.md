# Remux fixtures

Single-stream fragmented MP4 files resembling separately delivered DASH
picture and sound, split by stream copy from
`native/deadpan-source/tests/fixtures/cfr-bframes.mp4`
(SHA-256 `5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`,
120 H.264 packets and 189 AAC packets) with the development-only
`ffmpeg version 9.0.1` CLI:

```sh
ffmpeg -i cfr-bframes.mp4 -map 0:v:0 -c copy \
  -movflags frag_keyframe+empty_moov+default_base_moof video-fragmented.mp4
ffmpeg -i cfr-bframes.mp4 -map 0:a:0 -c copy \
  -movflags frag_keyframe+empty_moov+default_base_moof audio-fragmented.m4a
```

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `video-fragmented.mp4` | 32290 | `582a2fe0c990b5fccbf379e2a39e39ed1f18ed12db067e90d25a80b1ac7fda8f` |
| `audio-fragmented.m4a` | 6265 | `6a2af519bb3142148d5c1f4e27821ffc0364eb1294f6c71575223306cb29057c` |

The runtime worker reassembles them with the pinned FFmpeg 8.0.3 libraries;
the developer CLI is never a runtime dependency.
