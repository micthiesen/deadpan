# HDR automatic admission and durable Render (2026-10-05)

Scope: `AutomaticHdrV1` encoder admission, durable decision retention and public
Render of HDR Originals. This record does not qualify HDR viewing, mastering,
effects, other hosts or release packaging.

Host: Mac17,7 (Apple M5 Max), macOS 26.5.2 (25F84). Source: `af4677a3` plus the
uncommitted HDR branch working tree of 2026-10-05, including the coordinator's
HEVC NAL 62/63 stripping and the finished-file verifier's HEVC/HDR checks.
Release `deadpan-cli` SHA-256
`8d146aef39378fbc7d011b8fe8a589e180530e73db2478764c88ace7008c50f9`
(`CARGO_TARGET_DIR=/private/tmp/deadpan-hdr-target-admission`, FFmpeg prefix
`/private/tmp/deadpan-ui-ffmpeg/prefix`).

## Supervised admission

`qualify_encoder_admission WORKER W H N D REPORT {sdr|pq|hlg}`, reports in
`/tmp/deadpan-hdr-admission-runs3/`. All nine runs passed.

| Output | Raster/rate | Rejected | Selected | Max 10/8-bit error Y,Cb,Cr | Video bitrate |
|---|---|---|---|---|---|
| SDR | 320x180 30000/1001 | HW TargetTwo PTS<DTS | HW None | 4, 7, 5 | 1,500,000 |
| PQ | 320x180 30000/1001 | none | HW TargetTwo | 72, 36, 20 | 1,875,000 |
| HLG | 320x180 30000/1001 | none | HW TargetTwo | 73, 36, 16 | 1,875,000 |
| SDR | 1920x1080 30000/1001 | HW TargetTwo PTS<DTS | HW None | 9, 9, 13 | 8,000,000 |
| PQ | 1920x1080 30000/1001 | none | HW TargetTwo | 45, 45, 16 | 10,000,000 |
| HLG | 1920x1080 30000/1001 | none | HW TargetTwo | 52, 65, 12 | 10,000,000 |
| SDR | 3840x2160 60/1 | HW TargetTwo PTS<DTS | HW None | 8, 3, 3 | 68,000,000 |
| PQ | 3840x2160 60/1 | none | HW TargetTwo | 24, 42, 10 | 85,000,000 |
| HLG | 3840x2160 60/1 | none | HW TargetTwo | 24, 49, 8 | 85,000,000 |

HEVC probes reported `video_profile` 2 and a reorder delay (`maximum_b_run`) of 2.
PQ probes declared clli 1005/168, 1005/166 and 1005/169; the verifier's decoded
lower bounds were 1003.401/166.664, 1003.401/165.461 and 1003.401/168.781 cd/m².

## Content error envelope

The ignored in-process test
`encoded_render::admission::hardware_tests::measure_hdr_probe_content_on_this_host`
(release, `--ignored --nocapture`) encoded every PQ/HLG/SDR x hardware/software x
None/TargetTwo combination at 320x180 and 1920x1080, then ran the content
oracle without the finished-file verifier. All 16 HDR combinations passed. The
worst 10-bit values were: maximum 73 (Y, hardware HLG 320x180), worst-frame MAE
0.640 and MSE 12.219 (Y, hardware PQ 320x180). The schema-2 limits are 192, 6.000
and 256.000. SDR hardware TargetTwo failed with exact PTS<DTS as before.

Before the coordinator's NAL fix, every HLG output failed source admission
because VideoToolbox attached Dolby Vision 8.4 RPU NAL units (type 62). The
encoder now strips them; the decoder still rejects 62/63.

The first supervised PQ run declared the fixed 1000/203 clli. The earlier
verifier meter, which compared against a bilinear per-pixel maximum, reported
7501.8 cd/m² and rejected it. The bilinear meter of the exact input codes was
already 7256.6 cd/m², so the hard 2x2 chroma edges, not coding, caused that
value. The final probe declares the exact per-pixel input light (1004.19 cd/m²
peak). The current verifier checks a one-sided chroma-site lower bound.

## Durable and public Render

`deadpan-cli project create-original` on
`native/deadpan-source/tests/fixtures/hdr-{pq,hlg}-av.mp4`, then
`deadpan-cli render --output`: both published (`/tmp/deadpan-hdr-render-e2e/out`).
Provenance shows `encoder_selection`, intent and decision `automatic_hdr_v1`,
output `hdr_rec2020_{pq,hlg}`, PQ output mastering from the source volume and
movie clli 9998/2082. ffprobe reports HEVC Main 10, yuv420p10le, smpte2084,
bt2020, left chroma, mdcv and clli side data.

`qualify_automatic_render` on fresh copies passed for PQ and HLG: the initial
encode, checkpoint retry without requalification, reconciliation and cold retry
with fresh qualification (`/tmp/deadpan-hdr-render-e2e/durable-{pq,hlg}-out/report.json`).
An SDR fixture (`cfr-bframes.mp4`) published under `automatic_sdr_v1` with no
`color_policy` or `mastering_display` field in its decision.
