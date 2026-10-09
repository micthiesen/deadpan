# Pixel-aligned MP4 clean apertures, 2026-10-09

DP-02 / specification §4.3. MP4 Originals with a pixel-aligned `clap` rectangle
now import and render their clean picture. Previously the container admission
grammar rejected the box. This is a source-policy increment; DP-02 remains
partial, including fractional aperture sampling and the complete VFR and
creative-operation matrix.

## Interpretation and bounds

The eight signed 32-bit fields retain their rational meaning. The rectangle's
left edge is `(sample_width - aperture_width) / 2 + horizontal_offset`, with
the corresponding vertical expression. This follows Apple's
[clean-aperture field definitions](https://developer.apple.com/documentation/quicktime-file-format/clean_aperture/apertureheight_d).
The parser uses bounded i128 arithmetic, requires positive extents and
denominators, and rejects any rectangle extending outside the declared raster.
Duplicate or incorrectly sized boxes fail admission. Integral edges are exact;
the decoder does not use FFmpeg's floating-point crop rounding. A half-pixel
center offset is valid when it resolves to an integral rectangle. Fractional
edges return `unsupported_transform`, retaining an explicit implementation gap.

The native adapter keeps decoding and converting the full codec-visible raster,
then compacts the selected RGBA8/RGBA64 rows in that same owned buffer. Cropping
after color conversion preserves chroma interpolation at odd origins. No unsafe
code, extra picture allocation, resampling or timestamp transformation was added.
The clean dimensions reach source qualification, the automatic project basis,
preview and export. Original SAR and rotation remain independent. Input and
codec allocation limits still apply to the full raster, including codec padding.

Raw I420 and ten-bit planar reads support an even chroma-aligned rectangle.
Other rectangles refuse that raw-plane API before consuming a picture; the
ordinary RGBA source path still supports odd origins and odd visible dimensions.
Source indexes keep every original PTS/duration. Audio declarations and encoded
payload are unchanged. Project format 47 and database schema 75 are unchanged.

## Fixtures and independent comparisons

The committed generator adds one `clap` box to each SHA-256-pinned Original.
It adjusts only enclosing sizes and chunk offsets, and verifies identical
compressed payload. It performs no encode or media subprocess operation.

| Fixture | Original raster | Clean rectangle `[left,top,width,height]` | SHA-256 |
| --- | --- | --- | --- |
| `aperture.mp4` | 320×180 SDR H.264 | `[13,9,300,160]` | `9ad6958800a899000645cc4af8d5fd472ac3f62b0556d36807a8b04f009a7a81` |
| `aperture-hdr.mp4` | 64×36 PQ HEVC | `[6,2,48,28]` | `55c40f0ef970692cdaa203334cf0f3c12777dec1fd0dc03de29bd70951bee548` |

`native/deadpan-source/tests/clean_aperture.rs` independently selects rows from
the corresponding uncropped Original, without calling the new crop helper.
It compares every byte of 120 SDR pictures at both RGBA depths and every HDR
picture's 16-bit RGBA and raw ten-bit planes. Source metadata, PTS and audio
inventory remain equal apart from visible dimensions. Eight-thread reverse
seeks, fresh HDR keyframe opening/restart and raw SDR plane copies pass.
Malformed fields and out-of-raster rectangles fail; a fractional edge fails
explicitly. A smaller clean rectangle cannot bypass the full-raster input limit.

## Local results

Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1, pinned LGPL FFmpeg 8.0.3.
The only working agent performed implementation and a separate diff review.

- `cargo test --locked -p deadpan-source`: 115 tests passed, none ignored.
- Strict workspace Clippy, all targets with `deadpan-app/ui-harness`: passed.
- Fixture generator `--check`, workspace rustfmt and `git diff --check`: passed.
- Real CLI project creation, retained-source reopen, all 120 plan positions,
  Render and `verify-export`: passed. The file is 300×160 at 30000/1001 fps,
  exactly 120 pictures and 192,192 presented audio samples. All four audio
  windows pass; all three signal windows measure zero offset. Every picture's
  Original provenance matches the independently expected ordinal.

The Render test is
`clean_aperture_original_renders_at_its_visible_size_without_av_changes` in
`crates/deadpan-cli/tests/preview_export.rs`; it retains its package, movie and
full report when `DEADPAN_PREVIEW_EXPORT_KEEP=1` is set. Initial Render passed
in 42.02 seconds and the final-source run in 42.15 seconds. The initial lint invocation named the nonexistent feature
`deadpan-app/ui`; it ran no lint checks. The corrected feature above passed.

Logs are `/tmp/deadpan-aperture-native-final-20261009.log`,
`/tmp/deadpan-aperture-clippy-20261009.log` and
`/tmp/deadpan-aperture-render-final-20261009.log`. No live native UI,
fractional-aperture sampling, additional source-format matrix or new release
bundle was qualified by this change.

Final-source evidence is retained at `/tmp/deadpan-aperture-final-20261009`,
including the complete project, MP4, verification report and `pins.json`.
The source map pins changed Rust, generator, fixture and manifest bytes over
base `d5665e1320041cac27615e99b7d90e676521c9c8`. Its SHA-256 is
`1d200c1ace8015c031c5527d7a3eec67e97ceb169cb54f51eb7dc6ec625d24eb`.

| Executed or emitted artifact | SHA-256 |
| --- | --- |
| Debug CLI | `467ff588f56de09b66c0fc62e8887c42009d69a27162c43e3e6eafc7ea050fc3` |
| Render integration test | `64982c56eb5b4fbdf7414194dba929583437fb661ebd2ea3376f8cbba34458fc` |
| Native aperture test | `ea810b3a2a7525d0906f12db1e558a63c5b6288b890a139b2f1f6f62c1ff8416` |
| Published MP4 | `e1150d5a160ab074cd0bf97c7d49bc5ce5449c97ca430c5d09f399bb4db504d6` |
| Full verification report | `042fb4af424ea6cd82a8c6e80f4f4961762847c8da8e60304c0b20769b5555ee` |
