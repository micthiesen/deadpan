# Exact Original moments: 2026-09-26

This increment implements the [Original-moment timing contract](../SOURCE_MOMENTS.md)
as groundwork for native range reuse. It adds measured VFR range candidates,
exact audio selections over a retained full-span mapping, shared audio rendering
and core-20/database-26 persistence. Native Visual selection, registers, named
moments and atomic range splicing remain open. No requirement or gate is complete.

The checkout is based on `c03a5edde5f28d27074745eb15711cb28b1f2e50`, with the
previous captured-framing increment also present. Changes remain local because
the session makes `.git` read-only. There is no new commit or push to report.

## Behavior exercised

- Measured CFR, offset and VFR media use original PTS boundaries, including the
  measured terminal boundary. Duration rounds upward once; absent A/V overlap
  does not remove the Original's audio metadata.
- Fractional 44.1/48 kHz selections retain full source phase, restrict filter taps
  and stop at their exact physical endpoint. Negative placement, independent
  offsets, zero-sample windows and windows outside the host are covered.
- FollowSpeed, Preserve, root/point transfer, captured clocks, Split and NTSC
  InsertTime preserve their distinct timing and endpoint semantics. Moving a
  selection after capture changes support while keeping the retained clock.
- Source anchors reject hidden audio boundaries. Ordinary commands and isolated
  occurrence edits round-trip and invert. Older audio contexts still authenticate
  against matching historical revisions after alias reuse.
- Closed legacy mapping grammars reject new vocabulary through snapshots,
  subtrees, commands, occurrence edits and both patch directions, including null
  fields and escaped names.

The actual core19/database25 producer binary has SHA-256
`849b0d0b1129f488e90dbeb391c6be9633769aa88a91dbc250bca08d5520d2e3`.
Its [fixture](../../crates/deadpan-store/tests/fixtures/v25-captured-audio-history.sql)
has SHA-256 `c9c0d2ee68021d5b83fb0fff61e83a4f6c8af08beb93a1ea5d65203baa3663d8`:
19 revisions and 11 history entries, captured Hold/gap framing, mapping and
occurrence commands, and pending redo. Migration compares every old snapshot and
transaction, operational rows and the unchanged pre-migration backup. The
[producer and provenance](../../crates/deadpan-store/tests/fixtures/v25-captured-audio-history.provenance.json)
record how the fixture was obtained.

## Review and focused verification

The check workflow used independent general, migration and exact-audio reviewers.
All three returned no findings. The main review additionally required closed
schema-1 audio-context admission, version-independent historical content matching,
and captured-context preflight in the frozen core19 snapshot reader.

Focused final checks passed: 21 legacy-ingress tests, 18 measured timing tests,
69 migration tests, three CLI audio-context tests, 22 plan tests, and 56 audio
stage tests. The stage count combines 55 passing cases in the full focused run
with the remaining case's successful rerun after a fixture correction. It is
not a claim that the initial run passed.

Retained failures were test-authoring errors: three old subtree fixtures omitted
their required `overrides` map; a point-grid expected value was off by one; a
Preserve oracle used the reciprocal speed; and a shared PCM test helper expected
the old revision after an edit. The corrected tests retain explicit identity and
PCM assertions. Clippy also required constructing a deliberately reversed-range
test input from explicit endpoints. Production behavior was not relaxed to make
these checks pass.

The [final workspace run](../../tools/media-qualification/evidence/2026-09-26-source-moments/gate-2/report.json)
passed formatting, all-target Clippy with warnings denied, the locked workspace
build and doctor. The complete test run used `--no-fail-fast`: **1,408 passed,
one failed, zero ignored**. The failure was
`directories_fifos_and_sockets_are_rejected_without_blocking` in
`deadpan-jobs/tests/artifact.rs:200`: the sandbox denied `UnixListener::bind`
with OS error 1 before the socket assertion. No test was skipped or weakened.
The overall gate remains failed; this is not a green workspace result.

All 433 source/configuration hashes match before and after the run. The workspace
run includes the worker example tests and both compile-fail doctests; they were
not counted a second time from focused runs. Doctor reports core 20/database 26
and keeps both new capabilities explicitly partial. The complete logs, earlier
failures, focused checks, reviewer record and checksums are in the
[evidence directory](../../tools/media-qualification/evidence/2026-09-26-source-moments/README.md).

## Environment and remaining acceptance

The checks run on macOS 26.5.2 (25F84), arm64, Rust 1.97.1, using the existing
pinned FFmpeg development prefix. A current hardware query was denied by the
sandbox; this record does not present earlier hardware metadata as a new reading.

This increment changes no GUI controls, startup or picture rendering. No new
Computer Use, Metal comparison or listening session was needed for this data/audio
foundation. A new built-in ImageGen [moment-reuse target](../design/boards/original-moment-reuse-v1.png)
was visually inspected for temporal selection, separate context/focus, explicit
paste destination and correct example arithmetic. Its prompt and byte identity
are retained in the design manifest; this is future-interface design, not coded
GUI acceptance. Previous captured-view Metal/native review
limitations remain recorded in [that increment's evidence](captured-framing-2026-09-26.md).
Full native selection/reuse, mastering, acoustic and preview/export acceptance
remain required.
