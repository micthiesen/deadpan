# HDR picture color and encoder pixels

`deadpan-render` carries PQ and HLG sources through the shared picture pass,
selects an SDR or HDR output branch, tone-maps where that branch requires it,
and converts the composed working picture into Rec.2100 10-bit encoder planes.
This is a library boundary. The automatic HDR/SDR decision (specification
Section 22.5), source admission, encoding, metadata muxing and app labelling
belong to their owners. The [SDR boundary](SDR_ENCODER_PIXELS.md) is unchanged.

## Scale and transfers

The working picture remains linear Rec.2020 D65 in `Rgba16Float`, with working
1.0 equal to `HDR_REFERENCE_WHITE_NITS` (203 cd/m², BT.2408 graphics white).
SDR sources, white caption fill and the black Background are therefore already
placed at HDR reference white. Binary16 holds values far above 10000 cd/m²
(working 49.26).

`Transfer::Pq` decodes with the SMPTE ST 2084 EOTF, divided by 203.
`Transfer::Hlg` applies the BT.2100 inverse OETF per channel, converts scene
light to Rec.2020, then applies the whole-RGB OOTF for the nominal display:
`Fd = 1000 · Ys^0.2 · Es` with Rec.2020 luminance `Ys`, divided by 203. Scene
luminance at or below zero maps to zero. HLG 75% gives about 203 cd/m².

The HLG constant `c` differs intentionally between implementations. The f64
CPU reference derives `c = 0.5 − a·ln(4a)` = 0.5599107295. The WGSL shader and
the independent `qualify_hdr_export` reference use BT.2100's published rounded
0.55991073, which is 4.7e-10 higher. That is below one f32 ulp (6e-8) at this
value, so the shader is unaffected. The qualification reference uses the
published constant so it shares no derivation with production code.

Public f64 references: `pq_eotf`, `pq_inverse_eotf`, `pq_code_to_nits` (a
limited-range 10-bit code), `hlg_oetf`, `hlg_inverse_oetf`, `hlg_ootf`,
`hlg_inverse_ootf`, plus `HLG_NOMINAL_PEAK_NITS` (1000), `HLG_SYSTEM_GAMMA`
(1.2) and `PQ_PEAK_NITS` (10000). Unit tests check the published anchors:
PQ 0.5081 ≈ 100, 0.58069 ≈ 203 and 0.7518 ≈ 1000 cd/m², and HLG 0.75 ≈ 203.

## RGBA64 sources

`Rgba8Frame::new_rgba16(metadata, bytes)` admits packed little-endian RGBA64
(code / 65535, straight alpha). Rows are a multiple of eight bytes and at least
`width * 8`. The total, including padding, is at most `MAX_FRAME16_BYTES`
(128 MiB), separate from the 64 MiB RGBA8 bound. `sample_depth()` reports
`SampleDepth::Eight` or `Sixteen`. Existing `Rgba8Frame::new` callers are
unchanged.

The renderer uploads RGBA64 bytes unchanged into a core `Rgba16Uint` texture.
The `interpret16` entry point normalizes each integer code in f32, with about
2⁻²⁴ relative error. That is far below a quarter of a 10-bit step. No
binary16 source rounding or optional wgpu feature is involved.

RGBA64 sampling snaps a bilinear fraction within 1/1024 texel of a source
texel to that texel. f32 coordinate error is about 1e-5 at 100 pixels and up
to 1e-3 at 8K. Without the snap, an unscaled black PQ texel adjoining a 10000
cd/m² texel measured 13 code values too high. The CPU reference applies the same snap.
RGBA8 sampling is unchanged.

## Output branches

`PictureRenderer::set_color_pipeline(ColorPipeline)` selects the branch for
later submissions. The default is `ColorPipeline::default()`, SDR output with
a 1000 cd/m² tone-map source peak. `ColorPipeline { output: OutputColor, tone_map: ToneMap }`.

- `OutputColor::Sdr`: SDR sources are bit-identical to the previous renderer.
  PQ/HLG sources are tone-mapped per source texel in the interpret pass,
  before premultiplication, resampling and compositing. The composite then
  uses the existing Rec.709 clip and sRGB display encode.
- `OutputColor::Hdr(HdrTransfer::Pq | Hlg)`: no source is tone-mapped, so the
  working texture keeps values above 1.0 for encoder readback. Only the
  display pass tone-maps the composite for the SDR preview texture, then applies
  the same clip and sRGB encode.

`ColorPipeline::preview_label(hdr_source)` returns the viewing condition:
`"HDR PQ output, tone-mapped SDR preview"`, `"HDR HLG output, tone-mapped SDR
preview"`, `"SDR output (HDR source tone-mapped)"` or `"SDR output"`.

### Tone map

`ToneMap::new(source_peak_nits)` accepts 203 to 10000 cd/m². Otherwise it
returns `RenderError::ToneMapPeak`. The curve places HDR reference white at or
just below SDR reference white (BT.2408 style) instead of the BT.2390 EETF's 203 cd/m² target
peak, which dimmed 203 cd/m² white to about 159 cd/m² at a 1000 cd/m² source
peak. In working light `x` with source peak `P = peak / 203`:

- `x ≤ k`, with `ToneMap::KNEE` `k = 0.9` (182.7 cd/m²): unchanged.
- Above the knee: `y = (x − k)/(1 − k)`, `Y = (P − k)/(1 − k)` and
  `f(x) = k + (1 − k) · y(1 + y/Y²)/(1 + y)`, an extended-Reinhard shoulder.
  `y` is clamped to `Y`, so inputs at or above the source peak map to 1.0.

The shoulder meets the identity with slope 1 at the knee. Its slope
`(1 + (2y + y²)/Y²)/(1 + y)²` is positive and never above 1 for `P ≥ 1`, so the
curve is continuous, strictly increasing above the knee and never brightens.
At a 203 cd/m² peak, `Y = 1` and the curve is the identity with a clip at 1.0.
Working 1.0 maps to `(1 + k)/2 + (1 − k)/(2Y²)`:

| Source peak (cd/m²) | 203 | 250 | 400 | 1000 | 4000 | 10000 |
| --- | --- | --- | --- | --- | --- | --- |
| Working 1.0 maps to | 1.0 | 0.95455 | 0.95044 | 0.95003 | 0.95000 | 0.95000 |
| sRGB display code | 255 | 250 | 249 | 249 | 249 | 249 |

At the default 1000 cd/m² peak, 300, 500 and 1000 cd/m² map to working 0.986,
0.995 and 1.0 (codes 253, 254 and 255). Values below 182.7 cd/m² are exact.
This is the deliberate trade-off: placing diffuse white near SDR white leaves
only the top 10% of working light (sRGB codes 243 to 255) for highlights.
A lower knee would spread highlights further but dims reference white below
0.95. With this shoulder, white approaches `(1 + k)/2` = 0.95 from above as
the peak rises, and 1.0 as the peak falls to 203 cd/m².

The curve is applied to `max(R, G, B)` without clamping, and signed RGB is
scaled by the resulting ratio, which preserves hue. A maximum at or below the
knee, including a nonpositive one, is unchanged. Shader uniforms carry `k`,
`1 − k`, `Y` and `1/Y²`.

SDR graphics (sRGB white, caption fill) stay at working 1.0. In SDR output, an
HDR source's 203 cd/m² white shows as code 249 (1000 cd/m² peak) beside
captions and SDR footage at 255. In the HDR-output preview, the composite is
tone-mapped as a whole, so HDR white and graphics both show 249; the encoded
HDR file keeps both at 203 cd/m². The remaining gap is six codes: graphics
preview at 249 under HDR output but export at 255 under SDR output.

CPU references: `ToneMap::map_working`, `ToneMap::map_nits`,
`tone_map_highlights(rgb, tone_map)`,
`source_to_working_with(pipeline, rgb, color)`, `working_to_display_with(pipeline, rgb)`,
`reference_working_with_geometry` (f64 working composite) and
`reference_pixel_with_pipeline` (SDR preview code). Unit tests check exact
identity below the knee, slope 1 just above it, monotonicity, a slope that
never exceeds identity, the bound at 1.0, exact 1.0 at the source peak, the
tabulated reference-white placement and hue-preserving ratios. On Metal,
`hdr_render` renders a PQ 203 cd/m² frame and an sRGB white frame through both
branches: SDR export gives working 0.950 (code 249) and 1.0 (255); the HDR
preview gives 249 for both, each within one code of the CPU reference. The
`qualify_hdr_export` example records the placement at five peaks against an
independent closed form `k + (1 − k)·y(Y² + y)/(Y²(1 + y))`, with zero
difference.

### Shader uniform

The uniform remains a flat array of `vec4<f32>` values, written without
unsafe casts. It grows from ten to twelve values (192 bytes):

- `hdr`: x selects the interpret tone map, y the display tone map, and z the
  HLG OOTF.
- `tone`: knee, headroom `1 − knee`, normalized peak `Y` and `1/Y²`.

Transfer codes 3 (PQ) and 4 (HLG) extend `interpretation.y`. A second bind
group layout serves `interpret16`, with the uniform at binding 1 and a `Uint`
texture at binding 2. Black Background submissions now write the display rows
and branch flags that their display pass reads.

## Rec.2100 encoder pixels

`Rec2100Yuv420P10Frame::from_working(&WorkingRgba16Frame, HdrTransfer)`
returns the planes and `FrameLight`. It applies the SDR converter's admission
rules: even dimensions, finite active channels, alpha exactly 1.0, and padding
never read. Signed working values remain until the explicit output clip, after
the Rec.2020 primaries stage, which is the identity here:

- PQ: display light `working · 203` is clipped per channel to [0, 10000] cd/m²,
  followed by the inverse EOTF.
- HLG: display light is clipped to [0, 1000] cd/m², followed by the inverse
  OOTF on Rec.2020 luminance. Scene light is clipped to [0, 1] per channel
  before the OETF. Saturated HLG primaries cannot reach the full display peak.
  For example, full-signal blue displays at about 569 cd/m².

The BT.2020 NCL matrix uses Kr 0.2627 and Kb 0.0593. Quantization is 10-bit
limited range: Y is `64 + 876·Y'` clipped to 64..940, and Cb/Cr are
`512 + 896·C` clipped to 64..960. Chroma uses the SDR left-sited filter:
horizontal `[1,2,1]/4` on even columns with edge clamping, then vertical
`[1,1]/2`. It is filtered before one nearest, ties-upward quantization.

The output is tight `yuv420p10le`: all of Y, then Cb, then Cr, as
little-endian u16 samples with codes in the low ten bits. Accessors are
`bytes`, `y_plane`, `cb_plane`, `cr_plane`, `y_stride_bytes` (`2·width`),
`chroma_stride_bytes` (`width`), `sample_count`, `code(index)`, `transfer`,
and `policy` (`Yuv420P10Policy::Rec2100PqLimitedLeft` or
`Rec2100HlgLimitedLeft`). The encoder adapter packs P010 itself.

`FrameLight { max_nits, mean_nits }` uses the same clipped display light:
the frame maximum and the frame mean of per-pixel `max(R, G, B)` in cd/m².
The host derives MaxCLL and MaxFALL as maxima over frames and supplies them
only for PQ.

Verification helpers are `working_to_rec2100_nonlinear(rgb, transfer)` for
the per-pixel R'G'B' reference, and `rec2100_p10_to_working(transfer, [y, cb, cr])`
for inverse matrix and EOTF/OOTF back to working.

## Precision

On actual Metal:

- Decoding RGBA64 PQ/HLG, storing binary16 working values and reading them
  back gives at most 9.6e-4 relative error against the f64 reference, with a
  1e-3 absolute floor.
- The SDR preview codes are within one code of the reference in both branches,
  including the tone map.
- In the qualification fixtures, every PQ P10 code matches the independent
  reference exactly. HLG differs by at most one code, at a near-tie of 615.55.

See the [qualification record](qualification/hdr-pixels-2026-10-05.md).

## Limits

The renderer does not decode YUV sources. The source owner converts 10-bit
YUV to RGBA64 and chooses admission. There is no ICC or physical HDR display
path; the preview is always an SDR texture. HLG output assumes the nominal
1000 cd/m² display and no black lift. The renderer does not choose a dynamic
tone-map peak from mastering metadata; the caller passes `ToneMap`. Captions
and Background are unchanged in working light. They are tone-mapped only in
the HDR-output preview. This boundary does not verify encoding or the emitted
file.
