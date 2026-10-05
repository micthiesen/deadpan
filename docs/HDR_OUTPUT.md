# Automatic HDR output

Specification 22.5 requires one automatic action to produce HDR only when the
picture-bearing sources are HDR, the committed effects path preserves it and a
qualified encoder supports the profile and metadata; otherwise SDR with real
tone mapping. Preview uses the same branch decision as export. This document
records Deadpan's implementation of that branch end to end. Component details
live in [source admission](SOURCE_ADMISSION.md), [HDR pixels](HDR_PIXELS.md)
and [native encoding](NATIVE_ENCODING.md).

## Working light and BT.2408 placement

The shared working space stays linear Rec.2020 D65 in `Rgba16Float`. Working
1.0 is 203 cd/m², the BT.2408 HDR reference (graphics) white, and equally SDR
reference white. SDR sources, captions (fill 1.0) and authored Background
(black) therefore need no special casing in an HDR composite: an SDR caption
is placed at 203 cd/m² in a PQ or HLG output.

- PQ sources: working = PQ EOTF(E′) / 203.
- HLG sources: BT.2100 inverse OETF to scene light, then the BT.2100 OOTF for a
  1,000 cd/m² nominal display (γ = 1.2, Rec.2020 luminance), divided by 203.
  A 75 % HLG signal therefore lands at ≈203 cd/m², as BT.2408 specifies. This
  display-referred choice is fixed; Deadpan does not expose another HLG peak.

## Branch decision

`deadpan_cli::picture::decide_output_color` derives one decision per committed
revision from the document and each registered asset's qualified receipt:

| Condition | Output |
|---|---|
| SDR basis, SDR sources | SDR (unchanged path) |
| HDR basis (PQ or HLG) and every registered/picture source has that transfer, no stills or accepted/generated footage | HDR with that transfer |
| HDR source present together with SDR video, a still, accepted or generated Hold footage, or another transfer | SDR, HDR sources tone-mapped |
| SDR basis but HDR source (legacy mixed project) | SDR, HDR sources tone-mapped |

The project basis records the Original's qualified transfer at initialization
(`HdrRec2020Pq`/`HdrRec2020Hlg`, otherwise `SdrRec709`); an SDR Original can
never produce HDR, and no tag is changed without the pixel transform. No
source is upscaled. Every registered video asset counts, used or not, so the
branch does not depend on which interval is rendered or previewed. The
decision records its reason, the HDR source peak used for tone mapping and,
for PQ, the single consistent source mastering volume.

The pure rule is `deadpan_core::decide_output_color`; `ProjectStore::output_color`
applies it to the revision's stored receipts. Export, the native app (the
project service computes it when building each committed workspace, so an
HDR asset that is registered but not loaded still counts) and durable render
admission all use that one store function. Admission re-derives the branch and
mastering volume from the receipts and refuses a stored decision whose color
policy or `mdcv` volume differs. The project-wide audit still checks only the
basis rule (no HDR from an SDR basis, no transfer change).

Tone-map peak: a valid mastering volume's peak wins; MaxCLL is per-programme
metadata that stale tags and edits invalidate, so a MaxCLL below the mastering
peak never lowers it. Without a mastering volume a MaxCLL of at least
400 cd/m² is used, otherwise the 1,000 cd/m² default. Any ignored MaxCLL is
recorded as `ignored_content_light` in the decision. HLG always uses its
nominal 1,000 cd/m² display peak.

## Tone mapping and preview

The renderer's `ColorPipeline` carries the branch:

- **SDR output** tone-maps each HDR source frame in the interpret pass, before
  compositing, on max(R, G, B) with RGB scaled by the ratio. The curve keeps
  light up to working 0.9 unchanged and compresses the rest, up to the
  declared source peak, into (0.9, 1.0] with a smooth shoulder. HDR reference
  white (203 cd/m²) lands at working 0.95 or slightly above (0.950, sRGB
  code 249, at the default 1000 cd/m² peak), next to SDR graphics at 1.0
  (255). See
  [the tone map](HDR_PIXELS.md#tone-map). SDR frames take the unchanged clip
  path, so SDR projects are bit-identical.
- **HDR output** keeps above-reference working values. The SDR preview display
  pass tone-maps the composite with the same curve, so HDR reference white and
  SDR graphics both preview at code 249 for a 1000 cd/m² peak.

The preview is therefore a tone-mapped SDR simulation of the HDR output on
every display, labelled with the viewing condition. Native EDR presentation
through an extended-range Metal layer is not implemented: egui/eframe owns the
surface format and the current display transform encodes sRGB once. This is a
documented limit, not a claim of HDR monitoring.

## Encoding and metadata

HDR outputs use the [native encoder](NATIVE_ENCODING.md) HEVC Main10 path:
VideoToolbox `hevc_videotoolbox`, P010 input packed from the planar 10-bit
boundary, `hvc1`, BT.2020 primaries, BT.2020 non-constant matrix, PQ or HLG
transfer, limited range, left chroma, in both the codec context and every
frame so the VUI and `colr` box agree. AAC audio is unchanged. Video bitrate is
the SDR v1 class interpolation × 1.25 (`(sdr × 5 + 2) / 4`), the HDR policy v1.

PQ outputs carry an `mdcv` box from the single consistent source mastering
volume, when the source declared one, and a `clli` box whose MaxCLL/MaxFALL
are measured from the emitted pictures (CTA-861.3 over the clipped linear
light used for coding). Source MaxCLL/MaxFALL are never copied. HLG outputs
carry neither box. VideoToolbox through FFmpeg 8.0.3 inserts no mastering or
content-light SEI, so the in-stream SEI path is absent; YouTube and Apple
players read the MP4 boxes.

## Verification

The finished-file verifier checks the HEVC Main10 sample description, `colr`,
`mdcv`/`clli` against the contract and recomputes MaxCLL/MaxFALL from the
decoded 10-bit pictures. `verify-export` compares decoded 10-bit planes with
the committed picture path converted at the same HDR boundary; PSNR is
measured in 10-bit PQ or HLG code values. See the
[qualification record](qualification/hdr-output-2026-10-05.md).
