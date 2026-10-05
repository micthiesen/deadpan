# HDR source admission, 2026-10-05

Scope: `native/deadpan-source` admission and decode of PQ/HLG HEVC Main10 and
H.264 High10 MP4 sources, the `deadpan-media` qualification wire, basis color
policy and proxy refusal (DP-17 source side). Host: Apple Silicon (M5 Max),
macOS 26 (Darwin 25.5.0), Rust 1.97.1, pinned LGPL FFmpeg 8.0.3 prefix at
`/private/tmp/deadpan-ui-ffmpeg/prefix` (hevc software decoder included).
Contract: the coordinator's HDR design (linear Rec.2020, 203 cd/m2 reference)
and [source admission](../SOURCE_ADMISSION.md#hdr-sources).

## Fixtures

Produced by `native/deadpan-source/tests/generate_hdr_fixtures.py` with
Homebrew ffmpeg 9.0.1 (libx265 4.3+1-e9b8812, x264 core 165 r3222 b35605a, native
AAC; GPL development producers, never linked) and
`tests/generate_hdr_static_metadata.c` built against the pinned 8.0.3 prefix,
which adds stream `mdcv`/`clli` through movenc. Two consecutive runs produced
identical bytes.

```sh
export DEADPAN_FFMPEG_PREFIX=/private/tmp/deadpan-ui-ffmpeg/prefix
python3 native/deadpan-source/tests/generate_hdr_fixtures.py
```

| File | Bytes | SHA-256 | Content |
| --- | ---: | --- | --- |
| `hevc-pq.mp4` | 4885 | `d59a3ca17af6f88748c87aa72659e700971f4da42012020b1bb2ea82d062d7b0` | 64x36, 24 fps, 8 lossless Main10 frames, IDR every 4, PQ, SEI + `mdcv`/`clli` |
| `hevc-hlg.mp4` | 4392 | `9b67005fd098faccd4c75a67f09df192bd2b979dbb9ec8fc432bccf279196c22` | same layout, HLG, no static metadata |
| `h264-high10-pq.mp4` | 2121 | `3e771b6752cb3bc58d7fda4420c47b21b23d790f03cf0df65ba02cd1929ea260` | same content, H.264 High10 qp 1, PQ |
| `hevc-ten-bit-sdr.mp4` | 2347 | `275dd75ccd6c2d9229f0a179e1c5c337b84e1faaeedf5d49266b7032e30d8d97` | refusal: ten-bit BT.709 |
| `hevc-pq-bt709.mp4` | 2347 | `b4269d4c8a10352c6670b8e25eed408dbbcc0ab7040c576d4300da56792675de` | refusal: PQ with BT.709 primaries/matrix |
| `hevc-pq-mastering-change.mp4` | 3886 | `8c7842178b2ea60c9c01dc4c7f0a80efc756c4a0929b470f24b63f6a915cee31` | refusal: second IDR segment SEI peak 4000 cd/m2 |
| `hdr-pq-av.mp4` | 11255 | `c46833fc6a20670c915fdf9f74c49bb52c7d9774b2379641484c851605505b5a` | 320x180, 30 fps, 60 frames, CRF 18, 2 B frames, IDR every 30, PQ + static, AAC |
| `hdr-hlg-av.mp4` | 10990 | `2e4e15aff2415fd3bd5d8b64f01583b867fc7b24be213fd55fbe064b160c1e07` | same, HLG, no static metadata, AAC |

Small fixtures: rows 0..23 hold four 16-pixel patches (Y, Cb, Cr) = (64, 512,
512), (P, 512, 512), (400, 450, 600), (940, 512, 512) with P = 573 (PQ, 203.7
cd/m2) or 721 (HLG 75%); rows 24..35 hold Y = 64 + ((12x + 37f) mod 877).
AV fixtures: rows 0..89 hold five 64-pixel patches Y = 64, P, 400 (Cb 450, Cr
600), Q, 940 with (P, Q) = (573, 723) for PQ (203.7 and 1004 cd/m2) and (721,
502) for HLG; below, a moving gradient and a 16x16 Y=940 box at x = 5f mod 304,
y 120..135; frame 30 alone has a 32x32 Y=940 marker at x 288..319, y 148..179.
Audio is 48 kHz stereo AAC-LC, silent except a 10 ms 1 kHz sine (amplitude 0.5)
from sample 48000. PQ static metadata: primaries R (0.68, 0.32), G (0.265,
0.69), B (0.15, 0.06), white (0.3127, 0.329), 1000 / 0.0001 cd/m2, MaxCLL 1000,
MaxFALL 400.

## Results

```sh
export DEADPAN_FFMPEG_PREFIX=/private/tmp/deadpan-ui-ffmpeg/prefix
export CARGO_TARGET_DIR=/private/tmp/deadpan-hdr-target-source
cargo test -p deadpan-source --locked
cargo test -p deadpan-media --locked
cargo test -p deadpan-media-worker --locked
cargo test -p deadpan-cli --locked --lib proxy
cargo test -p deadpan-cli --locked --test encoded_verification
cargo clippy -p deadpan-source -p deadpan-media -p deadpan-media-worker --all-targets --locked -- -D warnings
```

All passed (deadpan-source: 52 unit, 9 `hdr_decode`, plus the existing
integration suites; deadpan-media all suites; media worker 33; cli proxy 7 and
encoded verification 22). Clippy reported no warnings.

Measured:

- Lossless HEVC PQ and HLG: every ten-bit plane of all 8 frames equals the
  generator's code values; picture types I P P P I P P P; decoder profile 2.
- Sixteen-bit RGBA against an independent f64 BT.2020 NCL expectation on flat
  patch interiors: maximum error 0.427/65535 (double precision, one rounding).
  The H.264 qp-1 fixture stays within two ten-bit codes. Eight-bit RGBA equals
  the sixteen-bit result's high byte within one code.
- libswscale 8.0.3 ten-bit to RGBA64 with explicit BT.2020 details returned
  (34494, 0, 35118) for the (400, 450, 600) patch whose expected value is
  (34628, 22205, 16605); its eight-bit path returned the correct (135, 86, 65).
  The sixteen-bit output therefore uses Deadpan's own conversion.
- Refusals with exact codes: ten-bit SDR HEVC `unsupported_depth`; PQ with
  BT.709 primaries `unsupported_primaries`; 8-bit 4:4:4 FFV1 PQ (`hdr-pq.mkv`)
  `unsupported_primaries` (was `unsupported_transfer`); ten-bit FFV1 SDR
  `unsupported_depth`; SDR FFV1 with stream content light `unsupported_hdr`;
  SEI mastering change at the third picture `stream_changed`; `hev1`
  `unsupported_codec`; differing in-band PPS/VPS `stream_changed`; NAL type 62
  `unsupported_hdr`; NAL type 41, layer 1 or temporal ID 0 `unsupported_codec`.
  A byte-identical in-band PPS repetition decodes all frames exactly.
- HEVC fresh-keyframe restart and `open_at_keyframe` at the second IDR return
  the exact lossless planes; restart at a P picture fails `invalid_keyframe`.
- AV fixtures decode 60 pictures each, the frame-30 marker is present only in
  frame 30, and a seek through B frames returns identical planes.
- SDR qualification JSON keeps the exact pre-HDR color object
  `{"range":...,"matrix":...,"transfer":...,"primaries":...}` with no new keys;
  PQ receipts round-trip mastering/content light and derive
  `HdrRec2020Pq`/`HdrRec2020Hlg` basis policies.

## Review fixes, 2026-10-05

Fixtures added by `generate_hdr_fixtures.py review` (two consecutive runs
produced identical bytes):

| File | Bytes | SHA-256 | Content |
| --- | ---: | --- | --- |
| `hevc-pq-open-gop.mp4` | 7151 | `23aa153c3a2f34244f0b665c8a30c935a749d477f48fccdd2398be86fc08538a` | 24 lossless PQ frames, x265 open-gop, keyint 8, 3 B frames: IDR, then CRA at pictures 8 and 16 each followed by RASL NAL types 9, 8, 8 |
| `hevc-pq-invalid-static.mp4` | 2440 | `59db05cb145c28c2dda52435434f3b5ef4ed5094fe0cf4ec81af36784635ce23` | 2 lossless PQ frames; SEI mastering peak 10 cd/m2, MaxCLL 100, MaxFALL 400 |

- HEVC SPS geometry: `hvcc` parses every SPS through its conformance window
  and checks the coded size against `max_dimension`/`max_pixels`. The PQ
  fixture codes 64x40 and crops to 64x36; with `max_pixels` 2304 its 64x36
  sample entry passes but the SPS fails `resource_limit`. A same-length forged
  SPS declaring 8320x64 behind the unchanged sample entry fails
  `resource_limit` in container preflight. Synthetic SPS units (0 to 6
  sub-layers, conformance windows, emulation-prevention bytes) parse exactly;
  8200 per side and 8192x8200 fail `resource_limit`; zero size, a window
  covering the picture, 7 sub-layers and truncation fail `invalid_input`;
  4:2:2 fails `unsupported_codec`.
- Static metadata: one rule set in `deadpan_core` (the former encoder rule).
  `deadpan-source` and `deadpan-encode` delegate to it; both crates' tests
  replay `deadpan_core::mastering_rule_cases`/`content_light_rule_cases` (17
  and 7 cases) and match core exactly. The invalid-static fixture opens with
  both values absent and `ignored_static` set for each, decodes both frames
  exactly, and qualifies as `"ignored_static":{"mastering":true,
  "content_light":true}`; stored evidence with a note beside a value, invalid
  stored values or a note on SDR is refused. The SDR wire is unchanged.
  `mdcv` values are no longer container grammar. Change detection compares
  FFmpeg's raw rationals, so `hevc-pq-mastering-change.mp4` still fails
  `stream_changed`.
- Open GOP: with the RASL guard disabled, a fresh restart or opening at either
  CRA decoded to the end without its RASL pictures and no error. With the
  guard, both fail `invalid_keyframe`; fresh opening at the IDR and an
  ordinary seek to the CRA return the exact lossless pictures.
- RGBA64 conversion: per-frame luma and horizontal chroma-tap tables, direct
  plane reads, unchanged per-pixel double expressions. Output is
  byte-identical to the previous conversion on 1920x1080 and 3840x2160 PQ
  x265 frames (testsrc2, 10-bit 4:2:0, left siting) and the existing 0.427/
  65535 accuracy test passes. `copy_current_rgba16` per call, release build,
  median of 9: 15.7 to 7.1 ms at 1080p and 61.4 to 28.6 ms at 4K on a quiet
  machine; interleaved under load (load average about 10) 16.1-19.4 to
  9.6-9.7 ms and 66.4-74.2 to 37.7-39.8 ms. A reciprocal-multiply variant
  measured no further gain and was not adopted.

```sh
export DEADPAN_FFMPEG_PREFIX=/private/tmp/deadpan-ui-ffmpeg/prefix
export CARGO_TARGET_DIR=/private/tmp/deadpan-hdr-target-source
cargo test -p deadpan-source --locked
cargo test -p deadpan-encode --locked
cargo test -p deadpan-media --locked
cargo test -p deadpan-media-worker --locked
cargo test -p deadpan-core --lib --locked -- hdr:: output_color
cargo test -p deadpan-jobs --lib --locked render::admission
cargo test -p deadpan-cli --locked --lib proxy
cargo clippy -p deadpan-source -p deadpan-media --all-targets --locked -- -D warnings
```

## Not established

No Dolby Vision, HDR10+, HDR Vivid, ICC or ambient fixture was constructed;
their refusal paths are the existing side-data checks plus the NAL type 62/63
guard. Real camera/phone HDR files (usually QuickTime `qt  ` with extra
metadata) remain outside the closed MP4 grammar. HLG/PQ display, tone mapping,
encoding and finished-file verification belong to the renderer, encoder and
verifier work. Sanitizer runs were not repeated for this change.
