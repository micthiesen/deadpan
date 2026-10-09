# Fractional MP4 clean apertures, 2026-10-09

DP-02 / specification §4.3. Fractional `clap` rectangles now remain exact geometry
from source admission through retained qualifications and the shared picture
renderer. This extends the [integral-aperture qualification](clean-aperture-2026-10-09.md).
The complete source-policy and source/creative-operation matrix remains open.

## Interpretation and consumers

The decoder retains the full bounded RGBA backing raster for fractional edges.
The field interpretation follows Apple's signed rational
[width denominator](https://developer.apple.com/documentation/quicktime-file-format/clean_aperture/aperturewidth_d)
and [horizontal offset denominator](https://developer.apple.com/documentation/quicktime-file-format/clean_aperture/horizoff_d)
definitions, with positive denominators and exact raster containment required.
It neither rounds the rectangle nor resamples encoded HDR values. Integral
rectangles retain the existing in-place compaction. Source PTS, durations, audio
inventory and allocation admission remain unchanged. Raw 4:2:0 reads explicitly
refuse a fractional aperture before consuming a picture; normal source consumers
use RGBA and its retained geometry.

The optional exact `[left, top, width, height]` is validated in the source receipt,
render frame, worker request and model-input evidence. Missing fields retain the
ordinary full-raster behavior and serialization. Existing core 48 / SQLite 76,
source qualification and worker protocol versions are unchanged. Previously
admitted sources produce the same receipt bytes. New fractional sources have
distinct content and qualification identities; indexed reopening compares the
complete retained interpretation against the decoder.

Project basis uses clean extents before SAR and rotation and keeps the existing
nearest-even rounding. Source-percent Camera/target coordinates refer to the
clean image. Only sampling UVs translate into backing-raster coordinates.
Preview, export and thumbnails use this geometry. Transfer decoding, primary
conversion, HLG OOTF and per-source SDR tone mapping precede canonical filtering.
The unchanged SDR byte-identity regression protects ordinary sources.

Vision, shot signatures and AI conditioning need a bounded integral image.
`deadpan_media::analysis_picture` bilinearly samples the complete clean rectangle
into its nearest positive integral raster, without changing PTS. It checks
caller cancellation/deadline per row. Vision and shots retain their existing
encoded-RGB analysis interpretation. Conditioning converts the source codes to
sRGB before sampling and retains the exact aperture in its measured evidence;
join comparisons share that conversion. Existing HDR/rotation model-input
refusals remain explicit. These analysis images never feed canonical rendering.

Proxy encoding and verification retain the full backing raster. After validating
the proxy's own metadata, its private viewer reader scales the Original aperture
to that raster with exact ratios. The sidecar does not pretend the MP4 encodes a
crop. Native preview source threads and allocation budgets remain unchanged.

## Fixtures and independent oracles

The source generator adds a `clap` box to SHA-256-pinned files, changes only
enclosing sizes and chunk offsets and verifies the identical compressed payload.

| Fixture | Backing | Exact clean rectangle | SHA-256 |
| --- | --- | --- | --- |
| `aperture-fractional.mp4` | 320×180 H.264 SDR | `[13.25,9.25,299.5,159.5]` | `7fd3e7faf2d65d9ca1b462c1d58933ec1fc41c93ecc49c0e91f4289999c96a92` |
| `aperture-fractional-hdr.mp4` | 64×36 HEVC PQ | `[6.25,2.25,47.5,27.5]` | `aad1e389bb8ebce04e0fc5ec1588ad33374bd72e39ac534b6b61a88c3e288f78` |
| `aperture-fractional-uhd.mp4` | 3840×2160 H.264 SDR | `[519.75,279.75,2800.5,1600.5]` | `7bda4e5784735f93b6065ca155530d614d01dd5e52f674ff5cd085bf18ed2fbd` |
| `moving-square-aperture.mp4` | 320×180 H.264 SDR | `[20.25,10.25,279.5,159.5]` | `624e5aa2ff782664cf30efeddec8e50b6a213548a7f8f512ab9f9184948dcdf6` |
| `faces-aperture.mp4` | 480×270 H.264 SDR | `[0.25,0.25,319.5,269.5]` | `b954b19c49e8e0ee99bba9e07708c93d7d09501cdc944a35f94c6fe445a9f0ab` |

The Vision generator converts existing synthetic FFV1 fixtures with development
FFmpeg 9.0.1/libx264 CRF 12, then uses the same byte-level aperture generator.
These GPL development tools are not linked into Deadpan; the runtime decoder
remains pinned LGPL FFmpeg 8.0.3. Both generators support `--check`.

Independent tests compare all 120 SDR and eight HDR decoded pictures against
their unchanged backing originals, including eight-thread seeking. Renderer
tests calculate expected UVs directly for all four rotations, while actual
Metal PQ/HLG sampling is compared with the f64 color reference. Separate
affine RGB ramps prove clean-pixel sampling, cancellation and matching
conditioning/join values. Receipt tests cover exact round-trip, reopen and
malformed/overflowing rectangles. Canvas and proxy tests cover exact scaling.

Real Vision tracking follows the independently authored moving square with less
than eight source pixels of center error before occlusion; all subsequent
unoccluded samples are tracked. Shot detection finds the original cut at picture
30. The face test verifies two faces in the encoded uncropped baseline and only
the retained face after the fractional crop, with clean normalized coordinates.

## Failures retained during development

An initial compile used i128 arguments for `ExactRatio::compare_integer`, whose
API takes i64. The bounded dimension conversions were corrected before the
integration build passed.

Strict Clippy identified enlarged tracking/face request enum variants after the
exact metadata was added. Those two request variants now box their stream
metadata; the serialized protocol is identical. The final protocol and complete
Vision worker reruns cover this adjustment.

Two tighter face crops produced no Vision detection. A controlled comparison
also found none with an integral crop of the same region, while the full encoded
baseline detected both faces. The final wider integral and fractional crops
both detect the retained face. No detector threshold or runtime algorithm was
changed to force that result. The diagnostic source and output are retained at
`/tmp/deadpan-fractional-face-diagnostic-20261009.rs` and
`/tmp/deadpan-fractional-aperture-face-diagnostic-20261009.log`.

## Verification

Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1, pinned FFmpeg 8.0.3.
Implementation and separate diff review were performed by the sole working agent.
The initial source/media/render gate passed 308 tests, including its doc test.
The complete workspace run with `deadpan-app/ui-harness` and locked dependencies
passed 5,546 tests, including two doc tests and 1,097 app/UI tests. Its 11 existing
opt-in measurements, real-model/network tests and release-only export matrices
remain skipped in this debug gate. The debug app linker reported its existing
`__eh_frame` size warning; no test failed.

That workspace build preceded the final review refinements. Rebuilt focused
checks pass for analysis sampling (1), worker protocols (84), model stream
validation (3), conditioning (7), joins (8), shots (4), the complete Vision worker
suite (25), and the new real UHD proxy (1). Strict all-target workspace Clippy
passes with and without `deadpan-app/ui-harness`; formatting and both fixture
generators' `--check` pass.

The VideoToolbox proxy retains all eight Original PTS/durations while encoding
the full 3840×2160 backing raster to 1920×1080. Presentation bounds become exactly
`[259.875,139.875,1400.25,800.25]`. Independent verification reports mean absolute
pixel difference 0.581 codes and signed RGB biases `[0.079,0.014,0.141]` codes,
below the existing 3.0 mean and 1.5 per-channel bounds.

The Python worker's strict measured-stream reader accepts and bounds-checks the
optional aperture, including the same checked-add limits as Rust. All 124 Python
worker tests pass. The AI runtime cache key hashes every worker adapter source,
including this reader, so the next bundle assembly cannot reuse the old adapter.

The release CLI creates a one-Original project from the fractional SDR fixture
and renders its immutable revision through the public command. Independent
`verify-export` passes all 120 pictures and four audio windows at 300×160,
30000/1001 fps and exactly 192,192 presented audio samples. All three signal
windows measure zero offset. Minimum luma/chroma PSNR is 43.897/60.373 dB;
minimum audio-block SNR is 29.535 dB and maximum block-level difference is
0.044 dB. Exact source-picture provenance agrees at every output position.

The project, movie, verification report, complete logs, command inventory,
source map and binary pins are retained under
`/tmp/deadpan-fractional-final-20261009`. The source map records the base commit
and every changed implementation, test and fixture file. SHA-256 pins:

| Evidence | SHA-256 |
| --- | --- |
| Source map | `76bdaccb8dae00279b5be109a7bd97a5469c4e548f26b93ed2f489aa5a411274` |
| Executed release CLI and its render worker | `04f39ecb0156dbadd4e4af37a713b36d16b06ccf37728711d8a271c72796ea9f` |
| Executed release Render integration test | `36508cf79eadbb0687b03ad8f3f1f26beab7263c95015a5875389c286dce0547` |
| Executed debug media worker | `6eb37ffd029b60f116e16ded11810f28ca5c1a337bcb51938d4e08c48254a098` |
| Executed debug Vision worker | `a3d442fc57537738fda7003f6cae216d72ef214bb2d56497db9efa6ba912dc4e` |

No live native UI, physical display, release bundle or new real-model inference
is claimed by this change. DP-02 remains partial for the broader source-policy
and source/creative-operation matrix.
