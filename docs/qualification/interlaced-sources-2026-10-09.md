# Interlaced source pictures, 2026-10-09

DP-02 / DP-16, specification §§16.3 and 22.2. Interlaced Originals now have a
deterministic progressive presentation before shared preview, analysis and
export. This qualifies the measured subset below. Other codecs, repeated-field
telecine and the complete source/creative-operation matrix remain open.

## Recipe and exact clocks

The descriptor-only source decoder uses the pinned LGPL FFmpeg 8.0.3
[BWDIF filter](https://ffmpeg.org/ffmpeg-filters.html#bwdif), with
`mode=send_field:parity=auto:deint=interlaced`. Source planes retain their
format, color metadata, SAR and orientation; automatic filter conversion is
disabled. Color conversion follows deinterlacing. Planes smaller than 3x4,
unannounced progressive-to-interlaced transitions, repeated-field telecine and
unrepresentable timestamps fail explicitly.

A three-entry queue retains original coded-picture clocks. Only bounded
ordinals enter FFmpeg's internal timestamp arithmetic. Each interlaced pair
bisects the measured interval to the next coded picture. The last pair requires
its own positive decoded duration. Video uses exact half-source ticks, including
odd intervals and negative PTS; audio clocks and the common origin are unchanged.
Progressive pictures in an admitted interlaced stream pass through once.

Source receipts retain `bwdif_fields` as this fixed recipe. The preceding GOP
is the index's temporal seek anchor; native seeks without an index find that
context themselves. Skipping non-reference preroll is disabled for field input.
Missing temporal anchors are refused when a receipt is read. Existing progressive
receipt bytes, core format 48 and SQLite 76 remain unchanged.

A single coded picture has no temporal neighbors. BWDIF's ordinary EOF behavior
both duplicates its internal timestamp and can weave moving fields. Deadpan
uses the measured field order and duration, and spatial bob for this case:
preserve current-field rows, average the nearest same-field rows for missing
lines, and replicate the nearest row at an edge. No other field enters that
interpolation. Both field orders and 8/10-bit planes have independent tests.

Encoded exports, proxy movies and generated candidate masters must remain
progressive. Their validators explicitly refuse this source-only interpretation;
deinterlacing cannot conceal an invalid output.

## Retained fixtures

The [fixture manifest](../../native/deadpan-source/tests/fixtures/manifest.json)
contains every file's byte length and SHA-256. Development FFmpeg 9.0.1/libx264
encodes independently authored 96x64 moving bars. It is not linked by Deadpan.
Each original field moves the bar two pixels; native tests check every interior
row against that field's authored position, including synthesized rows.

| Fixture | Coded pictures / presented fields | Field cadence |
| --- | --- | --- |
| `fields-tff.mp4` | 12 / 24, top first | 50 fps |
| `fields-bff.mp4` | 12 / 24, bottom first | 60000/1001 fps |
| `fields-tff-bframes.mp4` | 12 / 24, two B frames | 50 fps |
| `fields-single.mp4`, `fields-single-bff.mp4` | 1 / 2 each | 50 fps |
| `fields-100.mp4` | 12 / 24 | 100 fps; automatic output 50 fps |
| `fields-variable.mp4` | 12 / 24 | Odd variable intervals |

The variable fixture changes only video timing and required MP4 offsets over the
hash-pinned TFF file. Every compressed video and audio byte stays identical.
Coded intervals are `[2401,4801,2401,2401,7201,2401,2401,4801,2401,2401,2401,6001]`
at 1/60000 s. Its final 6001-tick picture proves that the preceding interval is
not extrapolated. Both generators reproduce checked-in bytes with `--check`.

Sequential decoding and every backward field seek are byte-identical with
1, 8 and 16 threads. Real source receipts preserve exact audio endpoints and
50/59.94 fps basis selection. Output field counts obey native limits and a
pre-call cancellation preserves the pending second field.

## Small output padding found during Render

The initial public Render failed its size budget at 96x64. A retained production
encoder probe proved that this Mac's hardware H.264 stream has a 192x96 coded
raster: SPS `pic_width_in_mbs_minus1=11`, `pic_height_in_map_units_minus1=5`,
right/bottom crop offsets 48/16, and visible size 96x64. The software encoder
does not need that padding. Shared output-decoder bounds now allow this measured
minimum before ordinary block alignment. Exact visible-raster, complete picture,
color, timing and audio checks remain mandatory. No fallback rule was relaxed.
The 50 fps hardware B-frame probe still reports its existing timestamp-order
failure and the authorized automatic path uses hardware without B frames.

The initial single-picture timestamp and row-position failures and small-raster
Render failures are retained in `/tmp/deadpan-interlace-*-20261009.log`.
The actual small encoder probes and SPS trace are in
`/tmp/deadpan-small-probes-20261009` and
`/tmp/deadpan-small-hardware-headers-20261009.log`.

## Verification

Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1, pinned FFmpeg 8.0.3.
Implementation and a separate diff review were performed by the sole working
agent. The direct native adapter harness passes ASan/UBSan for 8/10-bit TFF/BFF single fields,
mixed progressive/interlaced pictures, negative and odd clocks, terminal
durations, overflows and sentinel refusals. FFmpeg libraries themselves are
not instrumented by that harness. The complete source suite also passes all
120 tests with its native C boundary instrumented, including payload mutation
of every new MP4 fixture in isolated child processes. Rust and the separately
built FFmpeg libraries are uninstrumented.

Both debug and release public Render matrices pass 62 output pictures and four audio windows:
24 at 50 fps, 24 at 60000/1001 fps, two single-picture fields and 12 frames from
100-field input. Every signal window has measured offset zero. Expected audio
sample counts are 23040, 19219, 1920 and 11520 respectively. Picture provenance
also verifies exact Original field selection, including alternate fields at
100-to-50 fps. Evidence: `/tmp/deadpan-interlace-render-padding-20261009.log` and
`/tmp/deadpan-interlace-render-release-20261009-retry1.log`. Release packages,
movies and full comparison reports remain in
`/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmp1soUFY`.

The workspace gate passed 5,555 tests, with 12 opt-in tests ignored. After the
final review guards, the source/media/models/encoder packages passed 499 tests
and the CLI library passed 459. The small-raster encoder measurement was run
explicitly and passed, retaining the expected hardware B-frame refusal. Both
workspace Clippy configurations (with and without `deadpan-app/ui-harness`)
passed all targets with `-D warnings`; formatting passed. A stale C runtime-role
constant caused the first final rebuild to fail and was corrected before these
checks. Its failed log is retained.

The ad-hoc signed release bundle passes relocation, app smoke, CLI/app doctor,
real Render and damaged-helper refusals with a fresh home and scrubbed
environment. Both executables observe the actual mapped `libavfilter.11.dylib`
inside `Contents/Frameworks`. A separate packaged-CLI run creates an interlaced
Original, renders and independently verifies all 24 progressive pictures at
50 fps plus 23,040 audio samples with zero measured offset. Its environment
contains only a private `HOME` and `PATH=/usr/bin:/bin`; its working directory
contains no development tools. The packaged CLI SHA-256 is
`41ad5b873c42d820f42aeb74abebff345d46a9b24ace448eeb5e7ca291e0a25f`.
This proves relocation on this Mac. No native window judgment or physical
motion/display assessment is claimed.

Consolidated evidence is in `/tmp/deadpan-interlace-final-20261009`: test logs,
sanitizer reports, release projects/movies/comparison reports, the packaged
project/export, command records and `identity.json`. The identity record pins
every changed file against base commit `70f2415e`, the staged binary patch and
the actual executed development, release, sanitizer and packaged binaries.
The bundle remains at `/tmp/deadpan-fields-bundle-20261009/Deadpan.app`.
