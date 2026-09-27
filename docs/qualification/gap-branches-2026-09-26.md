# Editable Repeat gap branches: 2026-09-26

This increment adds sparse independently owned gap branches in core 23/database
29. `IsolateGap` materializes one rendered default gap as an ordinary Hold while
preserving its duration, marks, captured picture context, audio edges and exact
audio timing. `SetGapOverride` replaces a gap with fresh content;
`ClearGapOverride` restores the current default. See
[the contract](../REPEAT_GAP_BRANCHES.md).

This is groundwork for inserting an Original moment or extra time inside a
selected gap. General project-boundary splice, native Visual selection/registers,
gap controls and complete media acceptance remain open. No DP requirement or
release gate changes status.

## Behavior

- Sparse branches follow their preceding stable play identity. Reorder and growth
  preserve them; retiring a play removes its branch with reversible history. A
  final play retains a dormant branch without rendering a trailing gap. Empty
  branches suppress one gap independently of the shared default.
- Compact layout, marks, anchors and current picture/audio evaluation share the
  same branch geometry. Outer occurrence copies retain owned gap descendants.
  Dormant branches retain their definition and affine timing but have no current
  allocation and cannot independently qualify as an effective generation target.
- Materialized Holds read their current raw recipe. Retained audio bindings may
  refer to the old gap clock, with the selected own-gap argument closed to a
  stable play. Outer arguments remain live. Newly born gaps keep canonical
  definition clocks and discard old enclosing reanchor windows where required.
- Frozen audio contexts use schema 3. Earlier context and core schemas reject
  the new layout fields, including empty and null fields. The core-22 adapter
  also preserves its old owner/reference-kind constraint.
- Database schemas 1 through 28 replay into schema 29 on a consistent backed-up
  copy. Two fixtures were produced with the retained core-22/database-28 CLI,
  SHA-256 `d572c2b6e19f0d31e04912bc375b2deac7536225c9755381a508270d9414a475`.
  The first has 11 revisions and six history edits. The binding-bearing fixture
  has six revisions and two history edits; only its initial binding snapshot was
  explicitly seeded and validated. All its later edits and undo/redo came from
  that binary. Provenance records this distinction.

## Verification

The locked full-workspace run passed 1,534 tests, failed two and ignored none.
Formatting, all-target Clippy with warnings denied, the workspace build and CLI
doctor passed. The optional `ui-harness` all-target Clippy check also passed,
followed by 180 unit and two headless integration tests. All 482 source/config
paths stayed unchanged throughout that complete run.

One failure was the stale CLI doctor assertion expecting core 22/database 28;
it has been updated to 23/29 and the correct migration range. Its focused
post-correction suite passed all 16 cases, along with formatting and CLI
all-target Clippy. Only that test file changed after the full run; the follow-up
retains matching final source hashes. Production sources did not change for
this correction.

The other failure was `directories_fifos_and_sockets_are_rejected_without_blocking`
at `crates/deadpan-jobs/tests/artifact.rs:200`. The sandbox denied
`UnixListener::bind` with `PermissionDenied`, OS error 1, before the socket
rejection assertion was reached. That check remains failed, and the overall
gate remains failed. Nothing was skipped or suppressed. All 11 playback tests
passed in this run; earlier timeout evidence remains unchanged in its original
qualification records.

The ten decoded-PCM gap tests include five materialization cases: retained NTSC
phase and live current recipe, independently calculated start fades and Hard
edges, distinct outer-play clocks and definition births, canonical newly born
gaps, and actual Preserve output with irregular/fresh reads. Core tests exercise
sparse and billion-play geometry, ownership, marks, copying and inverse patches.
Plan tests cover current picture/audio/retained-context traversal. All 80 store
migration cases passed, including both actual old-binary fixtures and the closed
legacy command/layout grammar.

A separate current-CLI harness passed 43 invocations, producing 11 revisions and
seven history entries through public commands. It exercised isolation, Hold
resizing, dormant/revived branches, clearing and durable undo/redo across process
reopenings. It used Background/Silence without seeded snapshots or rewritten
history. Its binary SHA-256 is
`1c0e22b21da81133b10ccfb3fa7a06f4a0b9ea0e0d16e3020b78a2c21c0c489a`.
An initial run also passed; the final run corrects its reporting-only command
counter, which had omitted the final snapshot call.

Three independent reviews covered general behavior, audio/timing and history.
One history-test finding was fixed and re-reviewed: the rejection fixture had
an obsolete identity field. Each of the three new commands and its occurrence
form now parses successfully under the current schema before being rejected by
the old adapter. Audio review independently confirmed the handwritten fade and
fractional-phase oracles. No review findings remain. A separate review of the
completed UI harness's production hooks found no input, display-identity or
feature-disabled regression.

Initial Clippy runs found missing empty gap maps in existing test literals, a
Boolean assertion style issue and an over-wide internal helper signature. These
were corrected without suppressions. [Retained evidence](../../tools/media-qualification/evidence/2026-09-26-gap-branches/README.md)
contains the failed runs, complete gate, follow-up, source hashes, CLI traces,
old-binary producer logs and review outcomes. The pinned native prefix was
`/tmp/deadpan-ui-ffmpeg/prefix` on arm64 macOS 26.5.2, build 25F84, Rust 1.97.1.
No device, listening, physical display or encoded-export claim is made here.

## UI harness integration

The finished concurrent UI harness is preserved in this checkout and included in
the optional-feature validation. Its previous real-Metal visual and release
performance results remain in [its qualification record](ui-feedback-2026-09-26.md).
No new live GUI or physical-display result is claimed for these core changes.
The retained workspace screenshot was compared with the enlarged ImageGen target;
its shallower viewer and denser controls are recorded in
[the interaction review](../INTERACTION_REVIEW.md). All ten generated design
boards and their exact prompts remain in the repository.

The session cannot write Git metadata. Changes, including the finished harness,
remain pending the next authorized commit and push. The complete checkpoint is
`/tmp/deadpan-gap-branches-20260926/checkpoint`. This limitation does not change
the project's completion goal.
