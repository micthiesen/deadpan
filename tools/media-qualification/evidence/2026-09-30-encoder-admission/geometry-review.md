# Small-raster native failures

Parent execution observed the same explicit Output failure at 14x16, 16x16 and
64x64, all at 30/1: `source decode resource_limit: source dimensions exceed
configured bounds`. Each run first rejected hardware B-frame timestamp order,
then failed hardware without B-frames. Both failures remain in each report;
cleanup was confirmed and the Output failure did not permit another encoder.
No failed movie was admitted or retained as a successful probe.

Independent read-only source review by the native-generator agent found:

- `verification/inspect.rs` and `admission/content.rs` already budget dimensions
  rounded up to H.264 macroblocks. The two smallest cases therefore allow 256
  coded pixels, rather than just the visible 224 pixels of 14x16.
- The pinned FFmpeg build uses 16-byte stride alignment, covered by that budget.
- Native geometry checks can reject container, decoder-context, coded or
  pre-crop frame dimensions. Normal backing-buffer alignment does not change
  AVFrame width/height. Exact crop interpretation remains required.
- The diagnostic does not identify which dimensions were rejected. These
  results alone cannot prove a platform encoder minimum or justify a wider
  decoder allocation. Capture actual dimensions/limits or inspect the failed
  movie's SPS before changing the bound.

No product source or admission threshold was changed after these observations.
The successful final matrix covers 320x180 at 30000/1001, 640x360 at 60/1,
1920x1080 at 30000/1001 and 640x360 at 4/1. Small-raster support remains open.

The first evidence script also used an incorrect assertion that the MP4 header
allocation was below 1MiB. The existing native formula includes a fixed 1MiB
overhead: 1024 packets * 128 + 1048576 = 1179648 bytes. The corrected evidence
check asserts that exact value. Previously successful probe encodes were reused;
the independent AVFoundation decodes and event checks ran on their retained bytes.
