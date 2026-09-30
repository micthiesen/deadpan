# Committed encoder pictures, 2026-09-29

The [encoder picture host](../EXPORT_PICTURES.md) now produces owned SDR I420
from one committed project revision and half-open range. It preserves the
authored canvas, applies the shared even-raster rule only at output, and assigns
exact rational output timestamps independently of source PTS. It retains both
absolute project sample boundaries for future audio integration. One completed
frame may remain held; cancellation and deadline failures publish no result.

This is implemented encoder input. It creates no encoded file, final-render
process, audio mix, publication or native Render control. DP-16 and DP-17 remain
partial/open, and no delivery gate closes.

## Actual Metal result

Base commit: `d97268fe056dd8fafd8a03cdf7fbfa38ee79ba65`.
Host: Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1, wgpu 30.0.1
and pinned LGPL FFmpeg 8.0.3. Power, thermal state, other host work and OS caches
were uncontrolled. This run is a correctness check, with no performance claim.

The release `qualify_project_picture` example passes all **118 checks** and
retains **52 actual I420 frames**. Cargo's JSON artifact record selects the exact
executable; its SHA-256 is unchanged before and after execution. An external
240-second timeout encloses the example's 180-second cooperative deadline.

The actual path exercises:

- Original frame selection through a 2× Retime and two-play Repeat, captured
  freezes, and legal opaque-black Background/Repeat-gap frames.
- Exact `30000/1001` timing, `1/30000` output time base, 1001-tick frames and
  an exclusive 128128-tick endpoint for 128 frames.
- A nonzero `[20,63)` capture with output PTS starting at zero and independently
  derived absolute audio boundaries `[32032,100901)`.
- An unchanged captured revision through concurrent writer edits, undo, redo
  and writer close. A newly captured changed revision produces changed pixels;
  its captured Hold retains the prior view.
- Rejection at the exclusive range end, pre-cancelled and expired requests,
  a held-result rejection, and successful preparation after that result drops.
- A committed `319×179` canvas mapped to `318×178` output with signed relative
  aspect error `70/28391`. Full-plane comparison uses the original canvas and
  explicit authored framing values; the document remains unchanged.
- All 30 sampled frames of the previously accepted, relocated Generated
  fixture, including complete artifact identity, sampled ordinal, source PTS,
  captured framing and unchanged history. No model or worker input is needed.

The Generated fixture comes from actual FFV1 conversion and explicit store
acceptance of the committed tiny RGB test source. Its retained expected RGBA
is computed independently in the bundle test. It is a compatibility project,
not the native single-Original or AI acceptance workflow.

## Complete-plane comparison

The example-only f64 reference implements source transfer decoding, linear
bilinear sampling, the Rec.709 output transfer/matrix and left-sited chroma
filtering separately from the production working-matrix, half-float readback
and I420 conversion. It shares `PictureGeometry`; this is not independent
verification of that geometry implementation.

Thirty Generated frames plus the odd-canvas Original compare **93,396,906
individual code values** across Y, Cb and Cr. None exceeds the fixed one-code
tolerance for the GPU's half-float path. Per-plane results are:

| Plane | Values compared | Values differing | Largest absolute error |
| --- | ---: | ---: | ---: |
| Y | 62,264,604 | 1,277,174 | 1 |
| Cb | 15,566,151 | 92,753 | 1 |
| Cr | 15,566,151 | 145,727 | 1 |

The odd-canvas comparison differs at only 94 luma values, each by one code;
both chroma planes match exactly. All actual planes and all 31 references are
retained, with hashes and per-frame error measurements. The comparison does
not re-decode an encoded output or establish preview/export codec equivalence.

## Checks and review

The locked full workspace passes **2,106 tests** across 156 result records,
with zero failures or ignored tests. Twelve new contract/ownership tests cover
exact clocks, both origin-based audio boundaries, odd/tiny rasters, shared byte
limits, HDR/range rejection, rational/endpoint overflow, prepared-picture identity,
retained-result release and cancellation/deadline checks. Final formatting passes.

Strict workspace/all-target Clippy passes. Its first run found that the current
SHA-256 result type lacks direct hexadecimal formatting in the new reference
report. The example now uses its existing byte-formatting pattern. The failed
compiler log remains retained; no production behavior changed for that fix.

Independent reviews found no actionable defect in exact clocks/geometry,
resource ownership/cancellation, or the qualification oracle and assertions.
Reviewers performed source review only; the parent session owns the recorded
build, tests and Metal run. The complete journals and source inventories are in
[the evidence bundle](../../tools/media-qualification/evidence/2026-09-29-export-pictures/README.md).
Corrected lint, release build, Metal execution, workspace tests and final
formatting share source inventory `66cc8615665f9cb3a47656aeb9cefb1c4724eb116b155535af44128121d69949`.
Only documentation and retained evidence were added afterward. Every recorded
compiler, test and native command has a terminal status.

## Remaining work

The checks use synthetic SDR sources and cooperative in-process preparation.
They do not establish hard interruption of SQLite, decoder or GPU calls,
full-resolution memory/throughput, physical display behavior, HDR, legacy
Accepted/Still providers, complete effects, mastered audio or emitted MP4s.
No app widget, keyboard binding, focus path or native window behavior changes,
so unchanged UI replay and physical keyboard/IME/VoiceOver checks were not rerun.

Next is supervised final-render process isolation using this real producer,
followed by the full shared picture/audio graphs, qualified encoding and the
approved AAC timing metadata policy, emitted-file verification, atomic
publication and one-action native Render. Model management, candidate audition
and acceptance, remaining editing/analysis, recovery, performance, signed
distribution and all other requirements remain in scope.
