# ProRes 422 source qualification, 2026-10-09

Implementation, the full gate, native sanitizers, release exports and packaged
verification pass. DP-02 and DP-16 remain partial. This record covers the exact
admitted grammar below, not every ProRes profile, container or source operation.

## Implementation

The existing pinned LGPL FFmpeg 8.0.3 software ProRes decoder handles
`apco`, `apcs`, `apcn` and `apch` (Proxy, LT, Standard and HQ). No dependency,
license, hardware-decoder or runtime helper change is needed. The header guard
admits closed QuickTime tables, empty `wide`, URL data handlers, consistent
full-frame `tapt`, limited-range `nclc`/`nclx`, and version-one AAC `wave`
wrappers with exact units and optional matching mono/stereo speaker tags.
Apple's measured 118-byte `glbl` ImageDescription must repeat the outer codec,
raster, color and field order. Other embedded descriptions fail explicitly.

`native/deadpan-source/src/prores.h` checks every packet before decoding:
frame length/signature/version, matching raster, ten-bit 422/no alpha, stable
field/color interpretation, container-consistent rate and aspect hints,
quantization matrix bounds, computed slice-table extents, picture/slice/plane
lengths and zero stuffing. Allocation callbacks and ordinary corruption,
deadline, packet, frame and I/O limits still apply. The packet guard allocates
nothing. The small sample-entry dimensions cannot hide a larger coded raster.

Ten-bit `yuv422p10le` uses the existing direct double-precision RGBA64 converter
and shared GPU path. It retains the previously documented integer-output
clipping of out-of-range RGB; it does not add an HDR interpretation. Source
qualification receipts admit only ProRes with this pixel layout and limited
range. The generic picture reader preserves sixteen-bit output, including
playback and export. Both interlaced orders use the existing BWDIF field-clock
and temporal seek-context contract. ProRes is intra-only, so each pair of
fields seeks with the preceding source frame's context.

## Fixtures and focused checks

`native/deadpan-source/tests/generate_prores_fixtures.py` authors twelve
96x64 ten-bit 422 pictures at 30000/1001, using neutral/color patches, low bits,
moving details and independently varying chroma rows. Nine software fixtures
cover four profiles, 4:3 sample aspect, VFR with two missing picture timestamps,
and two field orders, plus a 98x66 raster that exercises partial macroblocks.
The unchanged mono AAC from `fields-bff.mp4` ends at
sample 19219. A separate `--apple` mode creates the tenth fixture with Apple's
VideoToolbox HQ encoder; its bytes are host-dependent. Both regeneration checks
pass on this machine.

Development fixture generation uses Homebrew FFmpeg 9.0.1, not the product
libraries. The retained raw planar references come from that CLI; native
tests independently reconstruct every RGB pixel and additionally compare
authored neutral values and distinct field contents. One-, eight- and
sixteen-thread decodes, backward seeks, exact clocks and native AAC samples
are compared. Tests reject malformed metadata, oversized/contradictory packet
headers, slice bounds, alpha/4444/RAW, color changes and contradictory embedded
Apple metadata. MOV seeds join the existing container and isolated native
mutation campaigns. Host tests round-trip receipts, reject forged codec,
pixel-layout and range combinations, and preserve field seek anchors.

The first eight command-line create/render/verify cases passed: 120 emitted
pictures and eight zero-offset audio windows, with exact 19219-sample output
endpoints. The final nine-fixture matrix passes in the milestone gate; the
separate release and packaged runs below retain every report and movie.

## Real Apple comparison and development failures

An Apple VideoToolbox HQ file generated on this M5 Max/macOS 26.5.2 has SHA-256
`7191190d824bd44995132883b431c70107ba16c7c95d24729452c48156a37c46`.
It is retained as `prores-apple-hq.mov`. It exposed three legitimate details
absent from the software fixtures: a redundant `glbl` description, `0x30` in
the reserved upper nibble above the zero alpha field, and a frame-rate hint
that changes from unspecified to the correct 30000/1001 after frame one.
Admission now validates these without weakening actual geometry/color checks.

A separate AVFoundation read of the first Apple-encoded picture to BGRA8 was
compared with scalar 422 reconstruction. Co-sited horizontal chroma had mean
absolute error 0.0326063 code values and maximum one; centered chroma had mean
0.583171 and maximum 29. This independently exercises the chosen siting and
matrix on a picture with varying horizontal and vertical chroma. It is a
software color comparison, not physical-display qualification.
The read uses `apple_decode.swift`; `compare_apple.py` reproduces the retained
`apple-chroma-comparison.json` exactly from `apple-first.bgra` and the committed
raw reference. `harness-identity.json` retains their hashes.

Earlier fixture generation tagged only the encoder context, leaving ProRes
frame color unspecified. The decoder correctly refused it; the generator now
tags the raw input frames too. Anamorphic MOV required a checked `tapt` parser.
An extended host test initially assumed the H.264 three-frame GOP length for
ProRes; its expected seek anchor now uses each one-frame ProRes GOP. Two test
compilation errors were corrected. Actual logs remain in
`/tmp/deadpan-prores-20261009`; no decoder failure was converted to success.

## Execution evidence

Evidence is retained under `/tmp/deadpan-prores-20261009`. `identity.json`
records base revision `07e1d71e` and the reviewed source/fixture hashes.
`checks.json` and its logs record the serial verification commands. The
sole-agent review covered integer/extent bounds, codec callback ownership,
metadata agreement, scalar color precision, field clocks and seek context,
forged receipts, AAC identity and the independent Apple counterexample.

The preserved pre-change PCM release CLI refuses Proxy, HQ and Apple HQ
registration without changing authored documents. Its SHA-256 is
`fae1c8b49b56720aadaf495f74a4fbbefe7fb43c81c49f4eb42632e3d63db2b9`.
`before/report.json` retains those failures and unchanged snapshots.

The final focused run passes all six native ProRes tests and all twenty host
source-qualification tests. The gate's expanded ProRes create/render/verify
test passes all nine cases, including the real Apple encoder.

On Apple M5 Max / macOS 26.5.2 (25F84), Rust 1.97.1 and the pinned FFmpeg
8.0.3/libopus 1.6.1 prefix, `cargo xtask gate` passes all 5,575 workspace tests
and 1,097 UI tests, strict workspace/UI Clippy, formatting and documentation
tests. Twelve existing workspace cases and two existing UI cases remain
skipped. No assertion failure or delayed-stdio warning occurred in this run.
All 157 native source tests pass under ASan/UBSan in
`/tmp/deadpan-prores-source-asan-20261009`. This instruments the native C
adapter and target C dependencies, not Rust, FFmpeg or libopus.

The release CLI imports and validates all ten fixtures through
`project create-original`, then renders and independently verifies each emitted
file: 144 pictures and ten signal windows, all at zero measured audio offset.
Each movie retains the exact 19219-sample audio endpoint. This includes the
98x66 partial-macroblock raster, both field orders and the Apple encoder.
`release/report.json` records the actual projects, revisions, movie hashes and
verification reports. The executed CLI SHA-256 is
`c97b5bea4016db5474f30ee9f9c31b7b23408950e82bb15c7a743e2dd8228853`.

The ad-hoc relocatable bundle and full `bundle-verify` pass, including the
native/AI runtime smoke checks, relocated dependencies and negative tamper or
missing-resource cases. From an unrelated directory with a fresh HOME and
PATH restricted to `/usr/bin:/bin`, its CLI imports and validates all ten
fixtures, then renders and independently verifies Apple HQ, TFF and the 98x66
raster: 48 pictures and three signal windows, all at zero measured offset,
with exact audio endpoints. `packaged/report.json` retains the projects,
revisions, movie hashes and reports. The executed CLI SHA-256 is
`6bf36c87716d3e1ef14ecf59b6b96befc269fbcf490ebfa105e50b0fc5cd5ebd`.

The reviewed production, test and fixture hashes remain unchanged throughout
qualification; `identity.json` records that final comparison. Formatting and
diff whitespace checks pass. These headless checks make no clean-machine,
physical-listening or GUI interaction claim.

## Remaining scope

ProRes 4444/XQ, alpha, RAW, HDR/log, odd-height interlace, MXF, QuickTime PCM and
broader metadata forms remain explicitly unqualified. This does not establish
VideoToolbox source decoding, physical display color, listening, native picker
interaction, clean/second-Mac behavior, or the full editing/export matrix.

Primary references: [Apple ProRes family](https://www.apple.com/final-cut-pro/docs/Apple_ProRes_White_Paper.pdf),
[RDD 36:2022 syntax and decoding](https://pub.smpte.org/pub/rdd36/rdd36-2022.pdf),
and the pinned FFmpeg `proresdec.c`, `proresenc_kostya.c`, and MOV demuxer sources.
