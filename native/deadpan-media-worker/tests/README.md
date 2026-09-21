# Native converter fixtures

These are small developer fixtures for the real `deadpan-media-worker` FFmpeg
adapter. They are H.264 lossless RGB MP4 files made with a developer FFmpeg,
not a runtime dependency. The worker itself links the pinned qualified FFmpeg
prefix supplied by `DEADPAN_FFMPEG_PREFIX`.

The RGB pattern is deterministic for `(frame, x, y)`:

```
r = (17*frame + 31*x + 7*y + 3) mod 256
g = (29*frame + 5*x + 47*y + 11) mod 256
b = (43*frame + 13*x + 19*y + 23) mod 256
```

The generated manifest records the fixture file SHA-256 and the SHA-256 of the
concatenated packed RGB frames. The tagged files use full-range GBR/sRGB/BT.709
metadata. `rgb1_24_no_tags.mp4` deliberately omits transfer and primaries;
`rgb1_24_corrupt.mp4` zeroes a 20-byte run inside the H.264 slice payload while
preserving the container, so the decoder must reject the picture.
The two-frame audio fixture has one AAC stream; the converter must discard it
and report one discarded audio stream.

Regenerate the fixtures with a developer FFmpeg:

```sh
python3 tests/generate_fixtures.py --ffmpeg /path/to/ffmpeg --output tests/fixtures
```

The script invokes only the developer-supplied encoder and writes only the
explicit output directory. It never selects or runs the runtime worker. After
regeneration, review the manifest and run the native integration test
with the qualified prefix:

```sh
DEADPAN_FFMPEG_PREFIX=/absolute/path/to/qualified/ffmpeg-8.0.3 \
  cargo test -p deadpan-media-worker --test canonicalize_real_media --locked
```
