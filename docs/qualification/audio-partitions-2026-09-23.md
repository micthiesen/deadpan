# Transparent audio partitions, 2026-09-23

This change implements the [retained audio-context layer](../AUDIO_PARTITIONS.md)
needed by structural splices. It follows `9af4a292`. Core schema 12 and database
schema 18 distinguish transparent unity partitions from ordinary authored crops.
It does not implement the complete Split command, inserted-Hold resume anchors,
mark fragment scope or native editing controls. All product requirements remain
open or partial.

## Behavioral evidence

The [core tests](../../crates/deadpan-core/tests/partitions.rs) reject nonunity
partitions and explicit partition edges atomically, preserve purpose through
serialized reversible patches, and keep ordinary Retime defaults omitted. All
11 legacy core adapters reject purpose in documents, every supported subtree
request, occurrence edits and both patch directions, including `null` and
`"edit"`. Modern partitions cannot project into legacy crop vocabulary.

The [plan tests](../../crates/deadpan-plan/tests/audio_plan.rs) distinguish actual
allocation, retained sampling support and complete envelope ranges. They cover
an NTSC split boundary, signed standalone fragment envelopes, meaningful outer
crops and bounded seeking through a billion-play Repeat. [Signal tests](../../crates/deadpan-plan/tests/audio_signal.rs)
retain the distinction between root ties-to-even allocation and ceil point-grid
storage. [Picture tests](../../crates/deadpan-plan/tests/picture_plan.rs) compare
exact source positions and selected VFR frame identities through a fractional
Retime across both retained partitions, including out-of-order queries.

Actual decoded fixture PCM is compared using the shared resampler and native DSP:

| Case | Evidence |
| --- | --- |
| Authored crop versus transparent selection | The old 8,197-sample crop fixture still admits only its selected discrete sample. Transparent partitions retain the complete source filter context and reproduce the unpartitioned render. |
| Short envelope | Splitting a two-sample clip into two one-sample partitions retains gains `[0.5, 0.5]`; a standalone suffix retains the original envelope offset. |
| NTSC and 44.1 kHz | Signed source placement and a separate audio offset retain identical sample values and original fades through both readers. Queries cross the rounded boundary in different block sizes and orders. |
| Mixed pitch stages | Both Preserve/FollowSpeed nesting orders retain complete preparation history, including when a partition is inside a Preserve input. |
| Room tone | Retained Holds and a split inside a Repeat gap reproduce the original loop and crossfade phase, including suffix-first preparation. |

These checks live in [SequenceAudio tests](../../crates/deadpan-audio/tests/sequence.rs)
and [StageAudio tests](../../crates/deadpan-audio/tests/stages.rs). They compare
PCM, not only timing metadata. No listening or device playback qualification is
claimed.

## Migration and history

The [schema-17 fixture](../../crates/deadpan-store/tests/fixtures/v17-history.sql)
contains 21 revisions and 12 history entries from the previous native app and
CLI: a measured Original baseline, source reuse, Repeat edits, an authored crop,
a Hard edge, branches, undo and pending redo. It was captured through SQLite's
backup API using the `9af4a292` CLI binary with SHA-256
`f33679efec86390b1a6dd1d71e46a6b772500db38a6c04ece0b70dc7a232a990`.
The SQL fixture's SHA-256 is
`6dd5bde14b0bcad2e001569188ca084d36c0d3103c5e06e40ea53721519e74a6`.
Original media bytes are omitted; the repository CFR fixture supplied the
qualification evidence. No personal media or local paths are included.

[Migration tests](../../crates/deadpan-store/tests/migration.rs) replay every
snapshot and forward/inverse patch, preserving revision IDs, redo, operational
rows, source qualifications, workflow profile and protected Original floor.
The old authored crop remains Edit purpose. Tampered snapshots, requests and
patches, and missing profile infrastructure fail without promotion and retain
a matching pre-migration backup. The authentic schema-16 fixture stays generic.
[Persistence tests](../../crates/deadpan-store/tests/persistence.rs) additionally
verify a newly authored partition across commit, reopen, undo and redo, with
invalid timing rejected before any new database revision.

## Review and verification

The complete repository gate passed on Apple M5 Max: format, workspace Clippy
with warnings denied, all 899 tests (zero failed or ignored), workspace build,
doctor and the native Metal renderer smoke check. Source and fixture SHA-256
hashes were unchanged throughout the gate. [Command logs, durations and source
hashes](../../tools/media-qualification/evidence/2026-09-23-audio-partitions/summary.json)
are retained alongside this report. Doctor reports core 12/database 18 and the
project's remaining capabilities as partial or unimplemented.

The main agent reviewed the worker's plan/audio implementation and new PCM tests.
The worker independently reviewed the main agent's core validation, all frozen
legacy adapters, migration routing, profile preservation and tests. No confirmed
correctness or security finding remained. Review corrected a stale migration
comment that incorrectly implied schema-16/17 edge policies would default.
The initial full Clippy run caught a boolean assertion style in a new migration
test; it was corrected before rerunning the repository gate.
The first full test run also found that the native Open test's expected-value
setup parsed its schema-11 fixture through current document ingress. It now
uses the frozen legacy adapter; production migration behavior was unchanged.

Verification uses deterministic headless tests. No live GUI session is needed
for this change, which adds no interface controls; the prior
[single-Original aesthetic and keyboard review](single-original-2026-09-23.md)
remains the applicable native evidence. Full Split and subsequent movement of
its fragments still need the remaining contracts in the
[splice design](../STRUCTURAL_SPLICE_DESIGN.md).
