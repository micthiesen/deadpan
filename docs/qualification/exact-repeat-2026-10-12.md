# Exact Repeat prefixes, 2026-10-12

`ExactRepeatLayout` indexes fractional owner clocks without rounding or expanding
plays. Stable iteration IDs, independent play and gap overrides, reordered
allocation runs, both seam biases and comparison budgets use the same indexing
algorithm as ordinary `RepeatLayout`. This supplies the compact Repeat
prerequisite for [derived splice clocks](../STRUCTURAL_SPLICE_DESIGN.md#derived-owner-clocks).

The shared implementation has two private numeric domains. Existing documents
retain `FrameDuration` lengths, i64 prefixes and checked i128 intermediates.
The exact interface uses reduced `ExactRatio` lengths and prefixes with checked
arithmetic. It does not allocate an exact-duration map for ordinary documents
or change their public API. No schema, command, authored duration or audio
recipe changes in this increment.

Exact identity lookup returns separate outcomes for a missing identity and
arithmetic exhaustion. A reduced total can fit even when an intermediate
prefix cannot; lookup returns that failure instead of panicking or choosing a
neighboring play. A final gap retains and validates its owned branch while
contributing no time. Single-play lookup does not compute that dormant gap's
period, which could overflow despite the entire active play being representable.

## Verification

Base `b4a47e72472b0c913efcc767bfb6140995e670a9`; Rust 1.97.1, locked
dependencies, Apple M5 Max, 128 GiB and macOS 26.5.2 (25F84).
The primary implemented and self-reviewed alone.

Six new tests cover an independent expanded integer-tick oracle over 128
generated fractional layouts, every half-tick and both biases; exact/integer
API parity; billion-play compactness; negative and missing extents; retired
owners; huge dormant final gaps; and checked compile/lookup exhaustion.
The billion-play fixture stores three segments and uses at most two segment
comparisons for its selected seam. Six existing gap-layout tests also pass,
including final-gap revival, sparse reorder and expanded-reference boundaries.

The combined core, plan, audio, playback and store run passes all 2,908 tests,
with zero failures or ignored tests. This includes the new exact-index cases,
production integer picture/audio queries, retained Preserve and pause PCM,
adversarial commands, durable Undo/Redo and history validation. Strict all-target
Clippy for those five crates and formatting pass. The focused initial twelve
tests overlap this total and are not added to it.

The [evidence](../../tools/media-qualification/evidence/2026-10-12-exact-repeat/metadata.json)
retains commands, exits, test counts, compressed full logs, source hashes and
the exact-layout test executable identities.

## Remaining integration

This numerical index does not persist effective clocks or admit Retime pause
insertion. Exact Sequence/Retime extents, retained selections, mark transforms,
the owned Preserve input/output route and their ordinary render/audio consumers
remain required. Existing Retime insertion guards stay in place. No GUI,
physical audio or performance target is newly qualified, and no DP requirement
or release gate is newly complete.
