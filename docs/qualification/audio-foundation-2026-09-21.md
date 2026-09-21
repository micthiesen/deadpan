# Structural audio plans and native DSP boundary

This milestone adds [exact structural audio queries](../AUDIO_PLAN.md) and a
bounded [Rust/C++ canonical stretch adapter](../AUDIO_DSP.md). It does not add
native playback or complete DP-09, DP-16, Gate A or Gate B.

Evidence is retained in
[`tools/audio-qualification/evidence/2026-09-21-foundation`](../../tools/audio-qualification/evidence/2026-09-21-foundation/).
The source manifest identifies the working source above base revision
`151ad21ec1f1568fb0d3bf47cbab10e114ab8f42`. Hardware was an Apple M5 Max,
128 GiB memory, macOS 26.5.2 (25F84), Rust 1.97.1 and Apple Clang 21.0.0.
The native sanitizer build sets a macOS 15 deployment target; this run does not
establish macOS 15 runtime coverage.

## Structural planning

`RenderPlan` now retains source audio placement, signed offset, Hold policy and
Retime pitch policy alongside picture mapping. Bounded audio queries partition
an explicit 48 kHz sample interval through Sequences, compact Repeats, sparse
overrides, gaps and nested Retimes. They retain exact original source coordinates
and leaf transforms while rounding absolute project endpoints only once.

Eleven plan tests include two fixed-seed, 96-case property tests against
independently expanded integer-boundary references. Coverage includes long NTSC
sequences without accumulated rounding, both half-sample tie parities, zero-sample
leaves, signed placement clipping, mixed nested pitch policies, distinct Hold
policies, billion-play sparse overrides, partition-invariant metadata, empty and
invalid queries, traversal/span budgets and explicit arithmetic overflow.
The headless CLI test inspects audio against a real SQLite package while a writer
is open, verifies repeat/gap boundaries and typed errors, and checks the authored
snapshot remains unchanged.

Timing review found that Repeat lookup originally charged comparisons after
performing its binary search. The fix adds `RepeatLayout::locate_bounded`, checking
the allowance before each comparison, and passes the remaining shared query
budget into it. A core regression tests exact, one-short and zero allowances on
a billion-play sparse layout; the audio suite checks shared-budget failure.
The timing reviewer confirmed the fix.

Independent general and focused FFI reviews found no further issues. The FFI
review checked retained input through destruction, moves, thread confinement,
exception containment and output publication. Actual worker lifecycle remains
unexercised because the production audio preparation service is not connected.

## Actual DSP and boundary checks

The adapter uses the unchanged canonical header from the earlier
[measured prototype](audio-canonical-2026-09-20.md), now included directly by the
qualification harness. Exact vendored Stretch/Linear headers, forwarding headers,
pins and complete MIT notices are checked by source hashes. Cargo adds only a
workspace crate using existing locked registry dependencies.

Six Rust tests exercise the native engine, owned source lifetime after moves,
irregular consumer partitions, nonmonotonic seeks through fresh canonical
replay, cancellation/resume, output suffix ownership, invalid input/recipe
bounds, and vendor integrity. All **50 historical PCM output SHA-256 values
match**: 15 mixed tone/chirp/impulse/dynamics cases, 30 short impulses and five
unity edge-length impulses. Expectations come from the previously committed
qualification report, not the new wrapper.

The initial Rust reconstruction of the mixed synthetic input produced a different
input hash, so the test correctly stopped before comparing DSP output. Possible
compiler/libm expression differences were not diagnosed further. The final test
retains the original 1,537,536-byte synthetic PCM, verifies its historical input
hash and decodes those exact bytes. No output expectation was changed. Fixture
provenance and both input hashes are in the
[fixture record](../../native/deadpan-dsp/tests/fixtures/README.md).

A standalone ASan/UBSan probe instruments the actual new C++ adapter and pinned
DSP. It passed create/read/destroy, null and invalid arguments, repeated lifetimes,
output guards, unchanged suffixes, EOF, empty reads, nonfinite/over-limit input,
and admitted rate/pitch extremes without sanitizer diagnostics. The retained
JSON records exact commands, source and binary hashes, host/compiler and linkage.
This does not instrument Rust itself or establish callback deadlines.

The existing canonical C++ harness also compiled through the forwarding header
with warnings denied and the vendored includes. Twenty Python measurement
regression tests passed. The full original performance/listening qualification
was not rerun; its measurements remain historical evidence.

## Repository gate and remaining work

After the review fix, the exact repository gate passed: rustfmt, warnings-denied
workspace Clippy, **648 Rust tests with zero failures or ignored tests**, locked
workspace build, and headless diagnostics. The initial 647-test gate is retained
separately. An ad hoc CLI check initially omitted `DEADPAN_FFMPEG_PREFIX`; rerunning
with the qualified FFmpeg 8.0.3 prefix passed. Both complete gates used that prefix.
An unrestricted Git whitespace check reports the unchanged vendored headers'
trailing whitespace and raw test-log endings. Those bytes were retained for
provenance; the authored-source whitespace check passed with those paths excluded.

No UI, native startup or lifecycle changed, so GUI testing and the native smoke
test were not repeated. [Native workspace visual and keyboard evidence](native-workspace-2026-09-21.md)
remains separate. No device audio or listening claim is made.

Still required: exact plan/source-phase binding to PCM preparation, resampling
and explicit speaker-layout conversion, long-clip preparation, fractional and
mixed pitch stages, reverse/variable rate, edge fades, gain/mixing/effects,
room-tone loops and tails, true-peak limiting, prepared-cache lifecycle, native
device clocks and route handling, listening, playback/export equivalence, and
the remaining product specification. The wrapper's bounded input and recipe
range must not be mistaken for full timeline rendering support.
