# HDR preview-versus-export verification, 2026-10-05

Scope: the [preview/export harness](../PREVIEW_EXPORT_VERIFICATION.md) extended to
the DP-17 HDR branch: ten-bit PQ/HLG references from the shared picture path,
HEVC Main10 container/color checks, CTA-861.3 light cross-check, the automatic
branch truth table and the SDR fallback. This is fixture evidence on one
machine. It is not HDR display, EDR presentation, release or listening
qualification.

## Environment

- Apple M5 Max, arm64, macOS 26.5.2 (25F84); Metal backend; hardware HEVC
  Main10 (`hevc_videotoolbox`) with B frames requested.
- Source tree at commit `af4677a3` plus the uncommitted HDR branch work of
  several concurrent owners (render, encoder, admission, finished-file
  verification). Results apply to that working tree.
- Pinned FFmpeg prefix `/private/tmp/deadpan-ui-ffmpeg/prefix`.

```sh
export DEADPAN_FFMPEG_PREFIX=/private/tmp/deadpan-ui-ffmpeg/prefix
export CARGO_TARGET_DIR=/private/tmp/deadpan-hdr-target-exportverify
cargo test --locked -p deadpan-cli --lib export_verification
cargo test --locked -p deadpan-cli --test preview_export_hdr
DEADPAN_PREVIEW_EXPORT_HDR_RESULTS=/path/new.json \
  cargo test --release --locked -p deadpan-cli --test preview_export_hdr -- --nocapture --test-threads 1
cargo clippy --locked -p deadpan-cli --lib --bins --test preview_export_hdr --test preview_export -- -D warnings
```

## Fixtures

`hdr-pq-av.mp4` and `hdr-hlg-av.mp4` ([source admission record](hdr-source-admission-2026-10-05.md)):
320x180, 30 fps, 60 frames, five 64-pixel patches (Y 64, P, 400, Q, 940 with
(P, Q) = (573, 723) PQ or (721, 502) HLG), a moving gradient and box, a
frame-30 marker, AAC with a 1 kHz click at Original sample 48,000; the PQ file
declares `mdcv` (R 0.68/0.32, G 0.265/0.69, B 0.15/0.06, D65, 1000/0.0001
cd/m²) and `clli` 1000/400. The grain fixtures `hdr-pq-grain-av.mp4` and
`hdr-hlg-grain-av.mp4` (`generate_hdr_fixtures.py grain`, 232,446 and
232,013 bytes, hashed in the fixture manifest) have the same size, rate,
length, audio and PQ boxes, with real-like footage: a sky gradient with
chroma, a soft highlight disc (PQ 723 / HLG 860), a panning texture with
2-pixel fence edges, a moving dark subject, a graphics-white sign, and
clumped film grain (2x2-correlated, sigma 10 luma / 5 chroma ten-bit codes)
over everything, encoded with x265 CRF 18 `-tune grain`. Each recipe ([test](../../crates/deadpan-cli/tests/preview_export_hdr.rs))
is a single-Original project edited through the headless `command` API: keep
Original [10, 40); two total plays of Edit [15, 21) (Original 25..31, marker
and click); a 6-frame silent freeze at Edit 6 holding Original 15; a white
caption on Edit [0, 4). 42 frames. Twelve output frames have hand-derived
Original provenance, and every one matched.

## Results

| Run | Path | Frames | Min luma PSNR | Min chroma PSNR | Max thumbnail MAD | Audio | Light |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| hdr-pq | encoded worker | 42 | 59.92 dB | 66.64 dB | 0.059 | offset 0, block SNR >= 61.7 dB | `clli` 9998/2082 = reference |
| hdr-hlg | encoded worker | 42 | 60.38 dB | 67.87 dB | 0.053 | offset 0, block SNR >= 61.7 dB | no `mdcv`/`clli` |
| hdr-pq | public Render (AutomaticHdrV1, published) | 42 | 59.92 dB | 66.64 dB | 0.059 | offset 0, block SNR >= 61.7 dB | `clli` 9998/2082 = reference |
| hdr-hlg | public Render (published) | 42 | 60.38 dB | 67.87 dB | 0.053 | offset 0, block SNR >= 61.7 dB | no `mdcv`/`clli` |
| hdr-pq-grain | encoded worker and public Render | 42 | 44.51 dB | 52.42 dB | 0.200 | offset 0, block SNR >= 61.7 dB | `clli` 3137/95 = reference |
| hdr-hlg-grain | encoded worker and public Render | 42 | 43.55 dB | 51.25 dB | 0.248 | offset 0, block SNR >= 61.7 dB | no `mdcv`/`clli` |
| PQ + SDR video (fallback) | public Render, H.264 | 60 | 47.2–51.7 dB | 59.2–59.6 dB | 0.095 | offset 0, block SNR 64.0 dB | n/a (SDR) |
| SDR Original | public Render, H.264 | 6 | passed | passed | passed | passed | n/a |

Largest 4x4-cell luma mean difference (`local_luma_error`, ten-bit codes)
of the correct encodes: 3.9 (hdr-pq), 4.3 (hdr-hlg), 8.4 (hdr-pq-grain) and
12.6 (hdr-hlg-grain), identical on both paths. The grain movies are
355,813 and 356,466 bytes.

PSNR is measured in ten-bit PQ/HLG code values (peak 1023) for HDR and in
eight-bit codes (peak 255) for SDR. The HDR hardware encodes measured
identically across reruns and between the worker and public Render paths
(same movie sizes, 39,458 and 42,173 bytes). The full release run of
`preview_export_hdr` (6 tests, four HDR fixtures) passed: the public Render
test in 8.4 s and the other five in 11.3 s after a 2 min build. MaxCLL 9998 cd/m² is the fixture's Y 940
content (PQ 10,000 cd/m², clipped at coding). Both HDR movies: `hvc1`,
hvcC profile 2, `colr` 9/16/9 or 9/18/9 limited, decoder profile 2, left chroma,
PQ `mdcv` equal to the source volume carried by the branch decision
(`tone_map_peak_nits` 1000, reason `hdr_sources`).

Negatives on the same HDR movies:

| Negative | Fixture | Luma PSNR | Local error (codes) | Flags |
| --- | --- | ---: | ---: | --- |
| missing caption, frames 0–3 | hdr-pq | 38.60–39.18 dB | 219.5–236.9 | `local_structure_mismatch` |
| | hdr-hlg | 37.24–37.50 dB | 181.8–200.3 | `local_structure_mismatch` |
| | hdr-pq-grain | 37.05–37.58 dB | 157.6–234.8 | `local_structure_mismatch` |
| | hdr-hlg-grain | 34.30–34.59 dB | 240.8–325.0 | `local_structure_mismatch` |
| 1.35x reframe, frames 37 and 40 | hdr-pq / hdr-hlg | 11.70–13.10 dB | 827.4–868.3 | luma, chroma, gross, local |
| | hdr-pq-grain / hdr-hlg-grain | 16.60–18.67 dB | 422.9–562.7 | luma, gross, local |

Frames 4 and 20 of the missing-caption run and frames 2 and 24 of the reframe
run pass with no flags.

## Picture gate calibration

An earlier 40 dB HDR luma gate failed the missing caption (38.6–39.2 dB) by
about 1 dB and was calibrated only on flat synthetic patches. The grain
fixtures show why whole-picture PSNR cannot carry that distinction: correct
grainy encodes measure 43.6 dB, only 4–9 dB above missing-caption pictures
(34.3–39.2 dB), and heavier real grain would close that gap. The gates are
now:

- HDR luma PSNR >= 30 dB, a gross-mismatch gate: 13.6 dB below the lowest
  correct encode (43.55 dB, hdr-hlg-grain) and 11.3 dB above the highest
  gross negative (18.67 dB, the grain reframe).
- HDR chroma PSNR >= 40 dB, unchanged: 11.2 dB below the lowest correct
  encode (51.25 dB).
- HDR local structure: the largest difference of 4x4-cell luma means must not
  exceed 40 ten-bit codes. Correct encodes reach 12.6 codes (3.2x below);
  the missing caption measures at least 157.6 codes (3.9x above). It fails
  exactly the four captioned frames on every fixture, independently of their
  whole-picture PSNR. A unit test fails a 16x8 missing graphic whose picture
  still measures above 30 dB while uniform +-22-code grain passes.
- Eight-bit SDR pictures report `local_luma_error` without a gate; their
  thresholds and qualification are unchanged.

Branch and reference checks (default gate, debug, about 37 s):

- Truth table on real committed documents: SDR Original → SDR (`sdr_sources`);
  PQ → PQ with the source mastering volume; HLG → HLG with none. Pure
  decisions: PQ basis with only SDR video → SDR (`no_hdr_source`); HLG basis
  with a PQ source → SDR (`transfer_mismatch`); SDR basis with a PQ source →
  SDR (`hdr_source_in_sdr_basis`); PQ plus an accepted Hold → SDR
  (`generated_pictures`, tone-map peak 1000, no mastering); a Background Hold
  keeps PQ; an HLG source's declared mastering is never retained.
- HDR references: the source patches survive within one code (64, 573/721,
  400, 723/502, 940), and the caption fill is the most frequent changed value,
  exactly PQ 573 / HLG 721 (BT.2408 203 cd/m²); no pixel is raised above it.
- SDR fallback (generic project, PQ primary plus a registered SDR video):
  basis PQ, decision SDR `sdr_source_mixed`; tone-mapped references peak at
  Y 235, the 203.7 cd/m² patch at Y 210 and the 1004 cd/m² patch at Y 235.

## Findings during the run

- VideoToolbox HLG output carried one Dolby Vision RPU NAL unit (type 62) per
  picture (ffprobe: profile 8.4 style RPU side data); the qualified source
  decoder refused the movie (`unsupported_hdr`). The encoder owner now strips
  NAL 62/63 before muxing; the HLG row above is after that change. PQ output
  carried no NAL 62.
- The silent-window peak gate failed an unedited Original whose click starts at
  sample 48,000: the reference itself peaks at -48.4 dBFS (source AAC pre-echo)
  and the export at -49.8 dBFS. The gate now also requires the decoded peak to
  exceed the reference peak by 3 dB; exact-zero references are unchanged.

## Not established

Public Render first failed while the concurrent admission/verification work
was integrating (a finished-file verifier `clli` mismatch on the probe, then a
report schema without `content_light`); the rows above are from the first
tree in which it completed. HDR display or EDR
presentation, real camera HDR sources (the grain fixtures are synthetic
real-like footage, not camera captures), software HEVC output (which declares
top-left chroma and must be rejected), long or 4K HDR content and listening
remain unqualified.
