# Automatic HDR output, end to end (2026-10-05)

Summary record for the DP-17 HDR branch described in
[HDR output](../HDR_OUTPUT.md). Host: Apple M5 Max (Mac17,7), macOS 26.5.2
(25F84), Metal, pinned LGPL FFmpeg 8.0.3 prefix
`/private/tmp/deadpan-ui-ffmpeg/prefix`. Source: `af4677a3` plus the
uncommitted HDR working tree of 2026-10-05. Each component record retains its
exact commands, fixtures and numbers:

| Stage | Record | Result |
|---|---|---|
| Source admission and 16-bit decode | [hdr-source-admission](hdr-source-admission-2026-10-05.md) | HEVC Main10 `hvc1` and H.264 High10 PQ/HLG admitted; `hev1`, 10-bit SDR, BT.709-primaries PQ, changed mastering and Dolby Vision NALs refused; RGBA64 within 0.427/65535 of an independent reference |
| Shared picture path and Rec.2100 pixels | [hdr-pixels](hdr-pixels-2026-10-05.md) | 344,520 actual Metal 10-bit codes: PQ exact, HLG within one code; SDR path bit-identical |
| Native HEVC Main10 encoder | [hevc-main10-encoding](hevc-main10-encoding-2026-10-05.md) | hardware and OS software PQ/HLG succeed; tags, `mdcv`/`clli`; VideoToolbox HLG Dolby Vision RPUs (NAL 62) stripped before muxing (46 of 46 pictures) |
| Finished-file verifier | [hdr-verification](hdr-verification-2026-10-05.md) | HEVC/colr/mdcv/clli/IDR/fresh-GOP checks; one-sided decoded content-light bounds; OS software PQ `topleft` chroma rejected |
| Automatic admission and durable Render | [hdr-automatic-admission](hdr-automatic-admission-2026-10-05.md) | `AutomaticHdrV1` selects hardware HEVC with B frames at 320x180, 1080p and 2160p60; checkpoint retry, reconcile and cold retry pass; SDR decisions byte-identical |
| Preview versus export | [hdr-preview-export](hdr-preview-export-2026-10-05.md) | PQ 59.92 dB / HLG 60.38 dB minimum luma PSNR in 10-bit code values over 42-frame recipes; zero audio offset; SDR fallback and SDR Original render H.264 |

## Findings

- VideoToolbox attaches a Dolby Vision 8.4 RPU to every HLG picture. Deadpan
  strips NAL types 62/63 in the encoder; source admission keeps refusing them,
  so verified HLG output is plain HLG.
- OS software HEVC PQ declares `topleft` chroma location for left-sited input;
  the verifier rejects it. It is never an eligible automatic fallback.
- VideoToolbox ignores `AllowOpenGop`; fresh-GOP verification remains the proof
  of independent GOPs.
- Exact per-pixel MaxCLL/MaxFALL cannot be recovered from decoded 4:2:0
  pictures, so the verifier bounds the declared values from below only.

## Not established

HDR display or EDR presentation (preview is a labelled tone-mapped SDR
simulation), real camera/phone HDR files (QuickTime variants are outside the
closed MP4 grammar), Dolby Vision and HDR10+ sources, long or 4K HDR content
through the full workflow, perceptual review of the BT.2408-style tone map, other
hosts, AI-pause conditioning from HDR Originals (refused) and release packaging.
