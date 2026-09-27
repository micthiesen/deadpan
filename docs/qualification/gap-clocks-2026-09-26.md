# Repeat-gap audio clocks: 2026-09-26

This increment adds a canonical [Repeat-gap definition](../AUDIO_DEFINITIONS.md),
explicit root and PointCeil placements, a bounded support projection for actual
gap occurrences, and read-only `--repeat-gap` headless inspection. These are
necessary operands for general range splicing. Authored gap bindings, compact
entry dispatch, atomic range reuse and its native interface remain open.
No DP requirement or gate is complete.

The local checkout remains based on
`c03a5edde5f28d27074745eb15711cb28b1f2e50`, with captured-framing and Original-moment
changes already present. Git metadata is read-only in this session; there is no
commit or push for these changes. Core 20/database 26 and audio-context schema 2
remain unchanged by this increment.

## Contracts exercised

- A configured gap can be read even when a one-play Repeat has no actual gap or
  all plays use overrides. Only the gap recipe contributes audio; no child or
  invented preceding-play identity enters the definition.
- Intrinsic duration is the gap duration, independent of Repeat output duration.
  Root round-even and PointCeil grids preserve their distinct signed coordinates,
  exact support, source phase and finite endpoint behavior.
- 44.1 kHz RoomTone uses an independent exact phase and sampling oracle, including
  fractional loop length, fresh seeks and different read partitions. Current
  recipe changes affect PCM while the explicit placement remains unchanged.
- Silence retains policy on a support containing no point and on an expanded
  placement with output points. Tail processing remains explicitly unsupported
  before source reads; no unsupported effect is silently omitted.
- Cache admission rechecks dependencies; cancellation and work limits return no
  partial PCM. Real linked-original corruption fails before sample exposure.
- Actual gap support uses the stable preceding play and its gap duration,
  including sparse unequal overrides, moved plays, nested repeats and billion-play
  bounds. Edit crops constrain support; transparent Partitions retain hidden
  context. Support projection rejects crossing an opaque Preserve clock.
- Current and historical CLI reads preserve the snapshot, history cursor and
  qualification counts. Configured gaps with no occurrence do not evade media
  qualification or historical receipt resolution.

## Verification

Focused checks passed: 34 core tests, 34 plan tests, 29 audio tests and 13
headless audio-inspection tests. The audio result combines the existing
definition/domain suites with the corrected six-test gap suite. The first run
failed one new fixture because its helper tried to construct a zero-duration
Hold. The corrected assertion verifies rejection at document admission before
plan construction; production validation was not relaxed.

Independent general and exact-audio reviews returned no findings. The review
covered admission, compact lookup, signed clocks, current policies, cache identity
and the headless inspection boundary.

The final locked workspace run passed 1,430 tests with one failure and zero
ignored tests. The failure was
`deadpan-jobs/tests/artifact.rs:200`: creating the Unix socket fixture returned
`PermissionDenied`, OS error 1, before its rejection check could run. The full
test command used `--no-fail-fast`; no test was suppressed. This keeps the overall
gate failed. Formatting, all-target Clippy with warnings denied, the workspace
build and CLI doctor passed independently.

All 437 source/configuration files had identical hashes before and after the
run. The host was arm64 macOS 26.5.2, build 25F84, with Rust 1.97.1 and the retained
qualified FFmpeg development prefix. The [evidence directory](../../tools/media-qualification/evidence/2026-09-26-gap-clocks/README.md)
retains commands, complete logs, source hashes, the corrected fixture's initial
failure, review scopes and a file-integrity manifest.

## Limits and next work

No GUI, native lifecycle, GPU, device or listening path changes in this increment.
Its tests are headless. The [moment-reuse ImageGen target](../design/boards/original-moment-reuse-v1.png)
remains the design reference for the later native workflow; it is not implemented
UI evidence. Previous Metal and native review restrictions remain documented in
the captured-framing qualification.

Next, authored gap bindings must distinguish surviving actual gaps from genuinely
new gaps. A former final play gaining a gap has no old gap clock, even if its play
identity survived. Compact entry dispatch must separately preserve a cut first
occurrence and later full occurrences. One suffix offset or one local resume for
every play is insufficient. General splice must commit the inserted moment,
boundary partitions, clocks and marks in one reversible transaction.
The [proposed compact resume dispatch](../STRUCTURAL_SPLICE_DESIGN.md#proposed-compact-resume-dispatch)
records retained allocation-entry queries with explicit hidden occurrences and
lexical birth rules. It is a design for the next implementation, not evidence
that those persisted bindings or the splice command already exist.
