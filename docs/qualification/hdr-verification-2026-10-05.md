# HDR finished-file verification, 2026-10-05

Scope: `deadpan_cli::encoded_render::verification::inspect` for HEVC Main10
PQ/HLG candidates (DP-17 verification side). It covers real files emitted by
`deadpan_encode::EncoderSession` from synthetic content. It does not cover a
project render, publication, physical HDR display behavior, or DP-17 as a
whole. See [finished-file verification](../FINISHED_FILE_VERIFICATION.md#hdr-pq-and-hlg-files).

Host: Apple M5 Max, macOS 26.5.2 (Darwin 25.5.0), Rust 1.97.1, pinned LGPL
FFmpeg 8.0.3 prefix `/private/tmp/deadpan-ui-ffmpeg/prefix`. Source base
`af4677a3` plus the uncommitted HDR branch working tree of this date.

## Commands

```sh
export DEADPAN_FFMPEG_PREFIX=/private/tmp/deadpan-ui-ffmpeg/prefix
export CARGO_TARGET_DIR=/private/tmp/deadpan-hdr-target-verifier
cargo test -p deadpan-cli --lib --locked encoded_render::verification
cargo test -p deadpan-cli --locked --test encoded_verification
cargo clippy -p deadpan-cli --all-targets --locked -- -D warnings
# measurement (prints one line per file)
cargo test -p deadpan-cli --lib --locked --release hdr_light_measurement -- --ignored --nocapture
# optional: keep the emitted MP4s for external inspection
DEADPAN_HDR_VERIFY_KEEP=/some/dir cargo test ...
```

The tests live in
`crates/deadpan-cli/src/encoded_render/verification/inspect/hdr_tests.rs`.
They build an `EncodedRenderContract` whose `native_contract()` equals the
`HdrEncoderProbe` contract, encode through `EncoderSession`, hash the file into
an `EncodedManifest` and run the complete `inspect`.

## Results

- PQ and HLG, hardware, B-frames None and TargetTwo (640x360, 46 frames):
  pass, with fresh-GOP frames equal to the frame count. A reorder delay is
  observed only with the B-frame request.
- Container mutations each rejected: missing `clli`, missing or changed
  `mdcv`, `colr` transfer/primaries/full range, Main profile, 8-bit chroma,
  declared MaxFALL > MaxCLL. A PQ file checked under an HLG contract is also rejected.
- Byte-patched files with rebound hashes rejected: `colr` transfer 16 to 18,
  `mdcv` maximum luminance, `clli` 100/50 for 1000 cd/m² pictures, `clli`
  renamed to `free`.
- Declared content light of half MaxCLL, half MaxFALL, or 0/0 is rejected. The
  fixed probe declaration (1000/203) and 10000/10000 are admitted, because
  overstatement cannot be disproved.
- OS software PQ: rejected with `chroma_location: TopLeft` at 640x360, 1080p
  and 2160p. OS software HLG passes.
- VideoToolbox HLG (hardware and software) attached a Dolby Vision 8.4 RPU
  (HEVC NAL 62, `ffprobe` "Dolby Vision RPU Data") to every picture. Source
  admission refused it ("unqualified Dolby Vision HEVC NAL units"). The encoder
  now strips NAL 62/63 before muxing. The HLG test helper still proves
  rejection whenever an RPU is present, then verifies the same pictures with
  each RPU overwritten in place by filler data (NAL 38).
- HEVC with B-frames requested: VideoToolbox used a three-picture B pyramid
  (presentation order 0, 4, 2, 1, 3, ...) with a two-frame reorder delay
  (`elst` media time 2 frames). Every inter picture decodes as a B slice, so
  consecutive B slice types (three) exceed the requested two. The verifier
  bounds the reorder delay instead (see the verification document).
- SDR `encoded_verification` integration tests: 22 passed, unchanged.

## Content light measurement

Each 46-frame 30 fps PQ file was encoded at 1920x1080 and 3840x2160, in
hardware and software, with and without B-frames:

- **Probe**: the probe's exact pictures. Host light comes from those codes
  with each chroma sample covering its 2x2 cell.
- **Stress**: renderer `Rec2100Yuv420P10Frame::from_working` pictures.
  - Content: 1-pixel 0/1000 lines, 4000 cd/m² 2x2 highlights (one moving),
    off-grid single 4000 cd/m² red pixels, saturated primaries/secondaries at
    1000 cd/m² every 24 px, colored noise, a ramp and a soft gradient.
  - Host light is the renderer's `FrameLight`.
- **Edges**: Stress without the highlights, so MaxCLL is 1000 cd/m².

Values are shown as decoded bound minus declaration, in PQ code values.
Positive means the bound is above the declaration. Declared values are the
host values rounded up to whole cd/m².

| Content | Declared | Per-pixel max, nearest / bilinear (cd/m²) | Site max | Site p99.99 | Site p99.9 | Site p99 (used) | Site mean (MaxFALL) |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| Probe hw 1080p | 1005/166 | 1305 / 7973 | +17.7 | +5.7 | +0.1 | -0.2 | -0.3 |
| Probe hw 2160p | 1005/166 | 1124 / 7524 | +7.7 | +5.6 | +0.2 | -0.2 | -0.4 |
| Probe sw (both rasters) | 1005/166 | 1025 / 7378 | +1.0 | +0.4 | -0.2 | -0.2 | -0.3 |
| Stress (all) | 4000/529 | 7044-10000 / 10000 | -0.2 to +32.3 | -114 to -64 | -121 to -86 | -128 to -122 | -26 to -25 |
| Edges hw 1080p | 1000/526 | 6779-6802 / 10000 | +143.5 | +89.5 | +55.5 | +20.5 | -25.5 |
| Edges hw 2160p | 1000/526 | 7832-10000 / 10000 | +123.0 | +30.5 | +23.2 | +10.5 | -25.2 |
| Edges sw (both rasters) | 1000/526 | 6977-8436 / 10000 | +118.1 | +49.9 | +18.6 | +6.0 | -25.0 |

Conclusions:

- A per-pixel maximum of any 4:2:0 reconstruction cannot be required to match
  `clli`. Even the convex site maximum exceeds the true MaxCLL by up to 143.5
  codes, because lossy chroma coding overshoots at dense saturated edges.
- The p99 site bound exceeded the declaration by at most 20.5 codes. The
  tolerance of 32 codes keeps a 1.5x margin.
- The site mean never exceeded the true MaxFALL. The 8-code tolerance covers
  quantization and coding bias.
- With p99, MaxCLL carried by sparse highlights is only bounded loosely: the
  Stress bound sits 122-128 codes below 4000 cd/m².

Each row reports the worst value over its hardware/software and B-frame
variants.

### Review follow-up (same date)

- The left-sited filter's replicated left tap gives pixel column 0 total
  weight 3/8 and the last column 1/8 (interior 1/4). The MaxCLL argument holds
  (taps still sum to one), but the earlier claim that the site mean cannot
  exceed the frame mean was false at the edges: a 4x2 picture with a
  300 cd/m² column 0 and 100 cd/m² elsewhere gave an unweighted site mean
  about 10 % above the host mean. The meter now weights chroma column 0 sites
  by 2/3 (test `a_brighter_left_edge_does_not_raise_the_mean_above_the_host`).
- The measurement was rerun with that weighting and gave the same rounded
  table values: p99 at most +20.5 codes (Edges hw 1080p), site mean -0.3
  (probe) and -24.9 to -26.0 codes (Stress/Edges). Software rows still fail
  verification only for top-left chroma siting.
- Per-pixel decoded maximum as the MaxCLL bound, converted to codes: nearest
  chroma +24.9 (probe hw 1080p), +10.7 (probe hw 2160p), +1.4 (probe sw),
  +181.5 to +217.4 (Edges); bilinear 10,000 cd/m² on every Stress/Edges file.
  Rejected in favour of the p99 site bound.
- The decoded MaxFALL bound may exceed the decoded MaxCLL percentile (sparse
  highlights: two of 400 sites lit gives p99 = 0 and mean 2.5 cd/m²). The
  verifier and the jobs mirror now require FALL <= CLL only for the declared
  values.
- The packet scan rejects HEVC NAL types 62/63 in every packet (rewritten
  opening IDR, inter and final packets tested); HLG movies carrying `clli` or
  `mdcv` are rejected by the container check. Raw measurement lines were kept in session scratch space only;
rerun the ignored test to reproduce them.
