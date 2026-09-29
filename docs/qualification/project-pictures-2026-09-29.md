# Committed project picture preparation

Deadpan now prepares exact original pictures from an explicitly captured
revision, using historical qualified receipts and private verified media. The
native preview shares its decoded-frame, index and framing adapters. Actual
Metal checks retain identical committed pixels while another writer edits,
undoes and redoes. This implements a picture-preparation boundary, not a
complete export worker. DP-16 remains partial and DP-17 remains open.

See the [picture contract](../PROJECT_PICTURES.md),
[SDR encoder planes](../SDR_ENCODER_PIXELS.md) and
[retained evidence](../../tools/media-qualification/evidence/2026-09-29-project-pictures/README.md).
No source project format, native control, layout or keybinding changes.

## Measured behavior

The real-media tests capture a committed document and plan beside a live writer.
Cuts preserve exact source ordinals; repeats, retiming, freezes and captured
geometry retain separate project/source clocks. Deletion, undo, redo and writer
close cannot change the session's document. A retained private input survives
linked-path deletion or replacement, while a cold read rejects missing or
changed originals. Foreign receipts, missing qualifications, unsupported
providers, HDR, invalid ranges, oversized rasters and cancellation fail
explicitly. Odd committed geometry remains unchanged.

The Metal example creates a synthetic qualified 320×180, 30000/1001 project:
120 source frames retimed to 60, played twice with a two-frame black gap,
followed by a three-frame captured freeze and three black frames. It asserts
the exact 128-frame duration and manually specified source ordinals at the
structural boundaries. All 31 checks pass over 18 retained output frames.

Repeated frame 20 and frame 82 are byte-identical. The captured Hold at frames
122–124 matches the framed original ordinal 41 deliberately selected for it;
this is not a claim that ordinal 41 is the immediately preceding image. Changing
the live Source pose changes its newly captured output but leaves the Hold's
captured geometry intact. The original session's frame 20 stays byte-identical
after live edit, undo and redo. Every black output code is exactly Y=16,
Cb=Cr=128. Foreign renderer targets, cancelled preparation and the excluded
range endpoint are rejected.

The example runs actual Metal, then reads the composed linear working target
through the existing SDR I420 boundary. It retains complete planes, SHA-256
identities, source ordinals/PTS, captured context, project/revision/frame IDs,
canvas and rate. Output time is never inferred from source PTS. No CPU picture
substitute, encoded movie, audio or user media is used in this experiment.

## Verification and review

Base commit: `2e26e4c21bf1f0da7beac1a65b3d0e1a57ab64bf`.
Host: Apple M5 Max, 128 GiB, macOS 26.5.2. Rust 1.97.1, wgpu 30.0.1 and the
existing pinned LGPL FFmpeg 8.0.3 prefix. Its build receipt SHA-256 remains
`baf7437c0cf4a61c94d75efd92db50b5ed3a96b2fa8f1e891284bdcf7c3d78c7`.

- The full locked workspace run finishes all 155 target result records,
  including doctests, in 346.07 seconds: 2,076 passed, two failed, zero ignored.
  Both failures are new test-fixture mistakes. All eight picture module tests
  pass in the final scoped continuation, covering those failures; the other
  completed results are preserved. The initial full invocation remains a
  failed observation, not a claimed green rerun.
- The rotation fixture's decoder reports three clockwise quarter-turns; the
  test initially expected one. The Blank-source fixture initially supplied
  neither picture nor audio. Its first correction retained the copied explicit
  video mapping, also invalid without picture. The final fixture retains
  qualified audio, sets Blank/FitBeat and an Independent link, then proves no
  video decoder is retained. Initial and intermediate test sources and failed
  logs are saved. Only `picture/tests.rs` changes after the full gate.
- Strict full-target workspace Clippy passes after the final test correction.
  The earlier SHA-256 digest formatting compile error is retained; its
  correction uses the existing explicit byte-to-hex pattern. No dependency
  version, production assertion or tolerance was changed to pass checks.
- Optional `ui-harness` coverage passes: 300 app tests and two headless
  integration tests, zero failed or ignored, in 34.30 seconds including build.
  Its strict all-target Clippy passes in 29.45 seconds. Base-app coverage is
  retained separately in the full workspace run. Final formatting also passes.
- Cargo's locked workspace JSON inventory identifies the exact example binary.
  Its hash is checked before/after execution, with a 120-second outer timeout
  around the whole-run 60-second cooperative deadline. The actual Metal work
  passes in 1.529 seconds, 1.695 seconds including its outer command. These
  tiny debug-build timings are not throughput qualification.
- The first launcher used its outer journal's report filename, so exclusive
  creation failed before media work. Separate inner and outer report paths
  fix that orchestration; the unchanged compiled example then passes. Both
  invocations and scripts remain retained.

Independent review covers receipt/revision identity, exact source clocks,
one-decoder ownership, odd geometry, framing extraction and black rendering.
It caught the final deadline gap and confirmed the correction. Runtime checks
supplied the fixture invariants missed during review. Only the parent ran
compilers, tests or Metal, serially; source inventories bind each result.

No widget, input route or shader changed. This adapter extraction does not
require new interface boards or another aesthetics, keyboard, painted-replay
or release-performance measurement. Existing evidence for those paths remains
scoped to its recorded sources; this experiment adds no native accessibility
or physical-display claim.

## Remaining product work

Accepted generated media and Still providers remain explicit errors; generated
object existence is not a decoded picture contract. Accepted media needs
bounded durable provenance readback, verified canonical master decoding and
shared native/fixed-revision consumers. Final-render process isolation, automatic
codec geometry, complete audio/mastering and muxing, native video decoding,
closed GOPs, HDR, full-resolution quality/performance, output verification,
atomic publication and the native one-action workflow remain required.

The user approved §22.3 timing metadata on 2026-09-28, applied on 2026-09-29.
Edit lists may represent encoder delay, padding and frame reordering, with
explicit stream-start/sync and full emitted-file verification. The
[failed FFmpeg result](encoder-timing-2026-09-28.md) and
[AVFoundation comparison](native-audio-2026-09-28.md) remain unchanged. Neither
that approval nor these picture fixtures closes a requirement or delivery gate.
