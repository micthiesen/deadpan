# Exact Original analysis windows, 2026-10-25

This is preparation for local word refinement, not completed DP-10 refinement.
The existing full-transcription preparation shares the same canonical path.
No model, proposed word bounds, authored edit or correction is accepted by this
change. Core 58 and SQLite 87 remain current.

## Behavior

`prepare_original_audio_window` accepts a half-open interval on the complete
Original's 16 kHz analysis grid. Its distinct result retains `analysis_origin`
in Original source samples, `analysis_offset` in 16 kHz samples, complete extent,
source interval and content/stream identity. It never rounds the window's first
fractional source coordinate into the existing integer `AnalysisInput.origin`.
The canonical resampler renders global output coordinates, including its halo,
so a local window is bit-identical to the corresponding full-analysis crop.

Empty, reversed, outside and longer-than-thirty-second requests refuse using
the retained qualification receipt before snapshotting, decoding or allocating
PCM. Output allocation and source buffers are bounded. Retaining
`OriginalAnalysisSession` reuses its reader for later windows.

**Initial session opening still decodes the entire source into AudioSession's
private disk PCM cache. This is not selected-range decoding.** Snapshot adoption
avoids the previous second source copy and respects the qualified speaker layout.
Source verification, opening, reads and resampling retain cancellation and one
absolute deadline. Adoption now carries those controls through its debug-only
SHA-256 recheck, checking between chunks rather than only after the whole hash.

## Actual verification

Apple M5 Max, Mac17,7, 128 GiB, macOS 26.5.2, Rust 1.97.1; locked dependencies,
qualified FFmpeg 8.0.3 libraries. Development optimization level 2. Scratch is
`/tmp/deadpan-word-refinement-2026-10-25`.

- The first actual CLI transcription suite compiles and records three passes,
  one failure: the test-authored negative-PTS PCM MOV is correctly refused by
  the container codec allowlist. A separate Opus composition-offset candidate
  also correctly refuses because its synchronous-packet contract is unmet.
  Neither result weakens media admission.
- The replacement mutates the repository's real AAC MP4: signed audio ctts
  offset −481, edit media time 1024 to 543, chunk offsets +24, unchanged encoded
  packets/durations and 1024-sample priming. Actual CLI registration qualifies
  audio interval `[-481,191711)` at 48 kHz. Independent FFmpeg 9.0.2 sees first
  decoded PTS −481 and final padding 320. Its timestamp warnings remain in the
  captured logs. The fixture SHA-256 is
  `0a740db7739109ca28e77d27fca989b70de9b200621bdd0c0393666ab187e97a`.
- `analysis-window-tests-second` passes all four CLI transcription tests in
  27.489 seconds including compilation, 3.06 seconds of test execution. They
  compare complete/window PCM bits at 44.1/48 kHz, zero/positive/negative origins,
  fractional phases, block and terminal edges; assert qualified source clocks,
  unchanged authored snapshots/history; reject invalid/cancelled/expired work;
  and prove a retained reader still serves after source storage is unavailable.
- `snapshot-adoption-unit-first` passes the regression that interrupts a real
  debug hash after its first chunk, for both cancellation and deadline.
- Strict CLI/Media/Store all-target Clippy first catches an intentionally
  reversed range literal in the negative test. Expressing that same invalid
  range through its fields preserves the test without a lint suppression.
  `analysis-window-strict-second` passes in 3.163 seconds. Targeted formatting
  and the complete diff check pass. No fresh broad application/release run is
  claimed for this preparation change.
- Independent review finds the debug-hash cancellation gap; follow-up review
  confirms its fix preserves identity validation and typed errors, with no
  further finding. The real-media tests pass before the purely syntactic
  reversed-range change; the final all-target lint compiles the final tests.

Actual runner receipts retain command, scoped source before/after hashes,
streams, exit and duration. Final source snapshots are preserved as text under
scratch source-second/source-third, with 82 inputs; the initial failed test
receipt has 80 input hashes and its compiled test executable retained, but its
complete earlier source bodies were not frozen before subsequent edits. Do not
claim a reconstructible earlier source snapshot from those hashes alone.

The [published evidence](../../tools/model-qualification/evidence/2026-10-25-analysis-windows/manifest.json)
retains final source text, all six actual test/lint/format runner receipts and
streams, the original failed outcomes and independent negative-fixture
construction/registration/decode logs. Its 182 files occupy 2,112,959 bytes;
manifest SHA-256 is
`6551431b41ea30a091396d66f72db8f7fa46be5399a7af8cfec2a9d020c9846a`.
Every file is reread and matched by size/SHA-256 with exact tree membership.
This collection excludes the separate model experiments and compiled test
executable, with their scope and retention limits explicit in the manifest.

## Remaining refinement work

The [model qualification plan](../../tools/model-qualification/word-refinement/plan.md)
records rejected DTW refinement and the phoneme-model investigation. Backend
context/ambiguity safety, native inference/features, approved model/runtime
distribution, immutable proposals, pinned correction acceptance, app/CLI flows
and actual end-to-end verification remain required. Initial whole-stream cache
preparation and cursor-priority incremental analysis remain separate open work.
