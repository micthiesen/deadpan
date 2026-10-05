# Shared HDR picture and Rec.2100 encoder pixels

On actual Metal, RGBA64 PQ and HLG sources pass through the production
`render_composed` path under the HDR output branch. They then convert to tight
Rec.2100 `yuv420p10le` planes. The fixtures produce 344,520 10-bit codes.
Every PQ code matches an independent f64 reference exactly. HLG codes differ
by at most one, at one near-tie neutral patch. All 22 original checks pass; after the 2026-10-05 review the example also runs five tone-map anchor checks (27 total), each matching an independent formula exactly. The
existing SDR path remains bit-identical. This qualifies a library pixel
boundary and these synthetic fixtures. It does not qualify encoding, an
emitted file or product export. DP-17 remains open.

Contract: [HDR pixels](../HDR_PIXELS.md).

## Environment

- MacBook Pro, Apple M5 Max (integrated GPU, wgpu Metal backend), 128 GB,
  macOS 26.5.2 (25F84).
- Rust 1.97.1. Working tree based on `af4677a3` with uncommitted HDR changes
  in `crates/deadpan-render`.
- Debug build, `CARGO_TARGET_DIR=/private/tmp/deadpan-hdr-target-render`.

## Command

```sh
export DEADPAN_FFMPEG_PREFIX=/private/tmp/deadpan-ui-ffmpeg/prefix
export CARGO_TARGET_DIR=/private/tmp/deadpan-hdr-target-render
cargo run -p deadpan-render --locked --example qualify_hdr_export -- \
  REPORT.json NEW_FIXTURE_DIRECTORY
```

The report and fixtures were written to the session scratch directory. They
are not retained in the repository. The run took 0.64 s.

## Fixtures and reference

Each transfer has two cases. The 320×180 case has 16 bytes of padding per
input row. The 318×180 case also has an odd chroma width of 159 and 16 bytes of
padding per working-readback row. Its padding is poisoned with nonfinite half
floats to check that conversion ignores it. Every case uses identity geometry,
BT.2020 primaries and opaque RGBA64 signal codes `round(E'·65535)`:

- Neutral patches at 0, 0.005, 100, 203, 1000, 4000 and 10000 cd/m². HLG
  signals saturate at its 1000 cd/m² peak.
- Full-signal R, G and B, and R, G and B at the neutral 100 cd/m² signal.
- One-column red/blue alternation, one-row red/green alternation and a
  two-axis four-color pattern, all at signal 0.75.

The reference starts from the known signal codes. It uses published ST 2084
and BT.2100 constants (HLG `c = 0.55991073`) and its own clip, inverse
transfer, NCL matrix, left-sited chroma filter and quantizer. It does not call
production color functions or decode working half floats. The predeclared
tolerance is one code, for f32 GPU transfer arithmetic and binary16 working
storage. The tolerance was not changed after measurement.

## Results

| Case | Codes | Y max / exact | Cb max / exact | Cr max / exact |
| --- | --- | --- | --- | --- |
| PQ 320×180 | 86,400 | 0 / 57,600 | 0 / 14,400 | 0 / 14,400 |
| PQ 318×180 | 85,860 | 0 / 57,240 | 0 / 14,310 | 0 / 14,310 |
| HLG 320×180 | 86,400 | 1 / 53,460 | 0 / 14,400 | 1 / 12,800 |
| HLG 318×180 | 85,860 | 1 / 53,100 | 0 / 14,310 | 1 / 12,720 |

All HLG one-code differences come from the neutral 100 cd/m² patch. Its f64
reference luma is 615.545, so it quantizes to 616. GPU light is about 3e-4
lower, within f32 HLG decode error and one binary16 step, and quantizes to 615.
The differing chroma samples come from the same columns.

Frame light statistics are within 2.3e-4 relative of the reference:

| Case | max cd/m² (actual / reference) | mean cd/m² (actual / reference) |
| --- | --- | --- |
| PQ 320 | 9997.75 / 10000 | 2253.006 / 2253.405 |
| PQ 318 | 9997.75 / 10000 | 2253.533 / 2253.933 |
| HLG 320 | 999.934 / 1000 | 357.002 / 357.078 |
| HLG 318 | 999.934 / 1000 | 356.904 / 356.980 |

The 10000 cd/m² maximum is 2.25 cd/m² low. It is the nearest binary16 to
working 49.26, whose step is 1/32.

Output SHA-256 values (PQ outputs are byte-identical to their references):

- PQ 320: `b3a3e9642659258183d046e7a7914085b110700b27b8767ffdcfd578c4261217`.
- PQ 318: `1dfa955f5621f986cd839cfa16057cfe349882e105c85ec1777a3831430f1e64`.
- HLG 320: `11f986f8e8597aa240fa48bb43715bce94c7d65d049d3be9a37da054bc3af95c`.
- HLG 318: `5453dd162578b4ad80278d8c7afb0ac2061a888a800fe989684eedc333a79628`.

The first run, without the RGBA64 texel snap, failed the PQ 320 case. Row 89
is black and borders 10000 cd/m² primaries. f32 bilinear coordinate error
leaked about 1e-5 of the neighbor into it, so it measured Y 77 instead of 64.
The 2,738 Y codes outside tolerance had a maximum error of 26. The snap
described in the contract fixed this. The run was then repeated unchanged.

## Other evidence

- `cargo test -p deadpan-render --locked` passes 47 unit tests, the caption
  composite, `hdr_render` (2 tests) and `sdr_bit_identity`.
- `hdr_render` renders 48×16 RGBA64 PQ/HLG ramps, saturated rows and partial
  alpha in five branch/tone-map combinations. The maximum working error is
  9.6e-4 relative. Display codes and P10 luma codes from GPU working are
  within one of the reference. SDR output tone-maps PQ highlights to
  working ≤ 1.001.
- `sdr_bit_identity` pins SHA-256 values for working and display bytes. They
  were captured from the renderer before this change: three RGBA8 SDR transfer
  and primary combinations with resampling and rotation, plus one captioned
  composite. They are unchanged after the change.
- `qualify_sdr_export` still passes 22/22 checks with zero code difference in
  all 172,260 I420 codes.
- `cargo clippy -p deadpan-render --all-targets --locked -- -D warnings` is
  clean.

## Not qualified

These results do not cover YUV source decoding or real HDR media; HEVC Main10
encoding, P010 packing or emitted-file metadata; scaled HDR resampling beyond
the snap tolerance; physical HDR display behavior or perceptual tone-map
quality; or release builds and other GPUs.
