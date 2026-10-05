# HEVC Main10 HDR encoding qualification

This record covers the `native/deadpan-encode` HDR path only: synthetic
`HdrEncoderProbe` pictures and audio markers through `EncoderSession`, then
independent inspection of the emitted files. It does not qualify the renderer's
HDR pixels, a host verifier, publication, physical HDR display behavior,
achieved bitrate on real content or DP-17 as a whole.

## Environment

- Apple M5 Max, macOS 26.5.2 (25F84), source base `af4677a3` plus the
  uncommitted HDR encoder changes from this increment.
- Pinned LGPL FFmpeg 8.0.3 prefix `/private/tmp/deadpan-ui-ffmpeg/prefix`
  (libavcodec 62.11.103, libavformat 62.3.103, libavutil 60.8.103) for encoding
  and `ffprobe`.
- Homebrew `ffmpeg` 9.0.1 used only as an external decoder for the PSNR
  comparison below. It is not part of the product or the pinned runtime.
- Release example binary `qualify_hdr` SHA-256
  `ea0d31a4df7d8c06988e4a2a8f6eb9ae262d1ab5cd27b8fb6c7d3758a3936bc9`.

## Commands

```sh
export DEADPAN_FFMPEG_PREFIX=/private/tmp/deadpan-ui-ffmpeg/prefix
export CARGO_TARGET_DIR=/private/tmp/deadpan-hdr-target-encode
cargo test -p deadpan-encode --locked
cargo clippy -p deadpan-encode --all-targets --locked -- -D warnings
cargo build --release -p deadpan-encode --locked --example qualify_hdr
$CARGO_TARGET_DIR/release/examples/qualify_hdr OUT/1080 1920 1080 30 1
$CARGO_TARGET_DIR/release/examples/qualify_hdr OUT/2160 3840 2160 60000 1001
# per file: decode to raw and compare frame-by-frame with the written reference
ffmpeg -nostdin -v error -i FILE.mp4 -f rawvideo -pix_fmt yuv420p10le dec.yuv
psnr REFERENCE-input.yuv dec.yuv WIDTH HEIGHT   # small C tool, peak 1023
```

The example writes, per transfer, the exact planar 10-bit probe input
(`*-input.yuv`, byte-identical to raw `yuv420p10le`) and, per transfer x mode x
B-frame request, an MP4 plus a JSON report with the encoder report, queried
`VideoCodecInfo` and `ffprobe` stream/packet observations. The FFmpeg `psnr`
filter was not used for the recorded numbers: with a rawvideo reference it
paired frames incorrectly and reported ~15-19 dB even though sampled decoded
values matched the input exactly; the frame-indexed C comparison is recorded.

## Results

All 16 attempts succeeded (PQ/HLG x hardware/software x B None/TargetTwo at
1920x1080 30/1 with 46 frames, and 3840x2160 60000/1001 with 91 frames).
Hardware and OS software HEVC Main10 are both present on this machine.

| File (1080p30) | Bytes | ffprobe chroma | Reordered packets | PTS<DTS | ms |
| --- | ---: | --- | ---: | ---: | ---: |
| pq hardware none | 49,422 | left | 0 | 0 | 313 |
| pq hardware targettwo | 47,840 | left | 31 | 0 | 245 |
| pq software none | 39,894 | **topleft** | 0 | 0 | 1,912 |
| pq software targettwo | 38,423 | **topleft** | 30 | 0 | 1,975 |
| hlg hardware none | 75,097 | left | 0 | 0 | 354 |
| hlg hardware targettwo | 71,677 | left | 31 | 0 | 331 |
| hlg software none | 63,696 | left | 0 | 0 | 1,990 |
| hlg software targettwo | 62,034 | left | 30 | 0 | 2,075 |

| File (2160p59.94) | Bytes | ffprobe chroma | Reordered packets | PTS<DTS | ms |
| --- | ---: | --- | ---: | ---: | ---: |
| pq hardware none | 165,395 | left | 0 | 0 | 1,560 |
| pq hardware targettwo | 164,844 | left | 65 | 0 | 1,514 |
| pq software none | 83,468 | **topleft** | 0 | 0 | 10,044 |
| pq software targettwo | 81,733 | **topleft** | 63 | 0 | 11,860 |
| hlg hardware none | 236,885 | left | 0 | 0 | 1,830 |
| hlg hardware targettwo | 228,391 | left | 65 | 0 | 1,834 |
| hlg software none | 155,156 | left | 0 | 0 | 10,948 |
| hlg software targettwo | 141,223 | left | 63 | 0 | 12,133 |

Every file: `ffprobe` reports `hevc`, `hvc1`, profile `Main 10`, `yuv420p10le`,
`bt2020` primaries, `bt2020nc`, `tv` range and `smpte2084` or `arib-std-b67`.
`has_b_frames` is 0 without and 2 with the B-frame request. Every video packet
needed its duration from the contract (46 or 91), as on the SDR path. The two
runs of the matrix produced identical file sizes. Repeated `hevc_videotoolbox`
log: "This device does not support the AllowOpenGop option. Value ignored." The
closed-GOP request is therefore not enforced by VideoToolbox; GOP independence
needs emitted-file verification.

Box bytes from `pq-3840x2160-hardware-targettwo.mp4` (hex, box header first):

- `hvcC`: `...6876634301 02 ...` general_profile_idc 2 (Main10); the tests also
  check chroma_format_idc 1 and 10-bit luma/chroma.
- `colr`: `636f6c72 6e636c78 0009 0010 0009 00` (nclx 9/16/9, full_range 0).
  HLG files carry transfer `0012` (18).
- `clli`: `0000000c 636c6c69 03e8 00cb` (MaxCLL 1000, MaxFALL 203).
- `mdcv`: `00000020 6d646376 2134 9baa 1996 08fc 8a48 3908 3d13 4042 00989680
  00000032`: G(8500,39850), B(6550,2300), R(35400,14600), white (15635,16450),
  max 10,000,000 and min 50 in 1/10000 cd/m², exactly the contract values in
  the box's G,B,R order. HLG files contain neither box.
- `elst` present in every video track (`use_editlist=1`).

Decoded planes versus the exact input (frame-indexed, 10-bit peak 1023):

| File | Y dB (max abs) | Cb dB | Cr dB | min frame Y dB |
| --- | --- | --- | --- | --- |
| 1080 pq hardware none | 63.41 (43) | 69.51 | 75.65 | 60.39 |
| 1080 pq hardware targettwo | 63.40 (45) | 70.30 | 75.84 | 60.82 |
| 1080 hlg hardware none | 63.46 (52) | 70.13 | 82.93 | 60.85 |
| 1080 hlg hardware targettwo | 63.38 (52) | 69.71 | 82.68 | 60.85 |
| 1080 pq software none | 69.75 (30) | 91.93 | 89.64 | 68.92 |
| 1080 hlg software none | 68.00 (28) | 80.82 | 85.54 | 66.34 |
| 2160 pq hardware none | 67.55 (25) | 79.08 | 86.82 | 66.33 |
| 2160 pq hardware targettwo | 67.45 (24) | 78.91 | 87.07 | 66.33 |
| 2160 hlg hardware none | 66.58 (19) | 76.49 | 90.11 | 65.90 |
| 2160 hlg hardware targettwo | 66.55 (24) | 77.32 | 91.66 | 65.79 |
| 2160 pq software none | 72.04 (27) | 99.51 | 91.93 | 70.55 |
| 2160 hlg software none | 69.31 (24) | 87.61 | 95.67 | 68.64 |

Frame counts matched exactly and sampled pixels (black, reference white,
colored patches, ordinal strip) decoded to their input codes, so the P010
packing, plane order and frame order are correct. Software B-frame rows were
also measured (66.40-71.59 dB Y). The media, JSON and comparison outputs were
kept in session scratch space only and are not retained in the repository;
rerun the commands above to reproduce them.

## Findings

1. Hardware PQ/HLG HEVC Main10 with and without the B-frame request emits
   correctly tagged files with the expected static metadata. Unlike hardware
   H.264 on this machine, HEVC B-frame reordering produced no PTS<DTS packet;
   `video_timestamp_order` rejection remains unchanged and would still apply.
2. OS software HEVC Main10 exists. Software HLG matched hardware declarations.
   Software PQ declared `topleft` chroma location (type 2) in its VUI although
   the input and request are left-sited. A host verifier must reject that
   declaration, or software PQ needs a separate qualified decision; this
   increment does not parse the SPS VUI inside the encoder.
3. A coordinator experiment with Homebrew FFmpeg found codec-context color
   fields alone left VT transfer/primaries unknown; the native path tags every
   AVFrame as well as the context, and all files above carry complete tags.
4. Achieved bitrates (0.2-1.2 Mbit/s against 10/85 Mbit/s targets) reflect
   the flat synthetic probe, not rate-control qualification.
5. Static metadata exists only as `mdcv`/`clli` container boxes. The HEVC
   bitstream carries no mastering/content-light SEI.

Untested here: real rendered HDR content, rate control under realistic detail,
B-frame picture-timing equality on the decoded timeline (packet order only),
audio marker positions in HDR files (the audio path is unchanged from SDR),
AVFoundation/QuickTime playback and physical display behavior.
