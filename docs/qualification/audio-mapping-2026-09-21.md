# Independent audio mapping qualification, 2026-09-21

This change represents source audio duration independently of picture duration.
It adds an exact mapping constructor, reversible node/occurrence commands,
revision-aware anchor resolution, core schema 6 and database schema 11.
[The contract](../SOURCE_AUDIO_MAPPING.md) documents units and unavailable
out-of-host boundaries. All product requirements remain open or partial.

## Behavior exercised

- A one-second selected audio span under a 60-frame picture maps to 30 frames at
  30 fps. A 24000-sample offset places it at `[15,45)`. The old explicit
  `fit_beat` mode still maps it across 60 frames.
- Original negative timestamps, equivalent source clocks, fractional project
  rates, repeat occurrences and nested retiming retain exact rational positions.
  Property tests compare source samples against an independent integer formula.
- Negative offsets and positive delayed tails reject boundaries outside the host.
  Source-coordinate marks stay fixed. One-play edits isolate only that play and
  retain the picture, beat duration, other plays and exact inverse transaction.
- Invalid/nonpositive/overflowing durations, missing audio, missing mapping
  fields and unsupported JSON fields fail. A test exposed serde's tagged unit
  variant accepting extra fields; the explicit empty-struct wire rejects them.
- The actual CLI exercises preview, commit, reopened exact boundary queries,
  stale revision rejection, undo and redo. At 30000/1001 fps, a one-second audio
  endpoint delayed by ten frames is exactly 40010/1001 project frames.

## Migration evidence

The [schema-10 fixture](../../crates/deadpan-store/tests/fixtures/v10-history.sql)
was generated with the previous implementation
`82f76aaf86e24d4fb3aec822ada0a638d19ddf25`, not by relabeling a current document.
It retains 14 revisions, eight edits, two Sources with offsets -137 and 2401,
two marks, pending redo, generated-media admission and a real managed PCM
original record. The archived binary validated it. Reproduction matched the SQL
byte for byte; [provenance](../../tools/media-qualification/evidence/2026-09-21-audio-mapping/fixture/manifest.json)
includes source, binary, original, harness and fixture hashes.

The 27 migration tests preserve all old authored semantics, revision identities,
history/cursor/redo, original records and generation metadata. Every old document
and forward/inverse edit is compared using its frozen vocabulary. Four additional
legacy unit tests cover source projection and new-field/command/patch rejection
through all five old core adapters. Corruption tests retain the unchanged
original database and consistent pre-migration backup.

After migration, a new explicit audio mapping commits, undoes, redoes and reopens
with the original records intact. Invalid original metadata is rejected, but
metadata validation alone does not establish that original bytes are present or
match a claimed length; verified original snapshots remain the byte-level check.

## Verification scope

All 440 Rust tests passed, with no ignored tests. Formatting, workspace Clippy
with warnings denied, workspace build and headless doctor also passed on Apple
M5 Max / 128 GiB / macOS 26.5.2 with Rust 1.97.1. Doctor reports database schema
11 and document schema 6.

The [repository gate](../../tools/media-qualification/evidence/2026-09-21-audio-mapping/gate/report.json)
records formatting, workspace Clippy, tests, build and headless doctor. The
[source manifest](../../tools/media-qualification/evidence/2026-09-21-audio-mapping/source-manifest.json)
binds the tested base and changed files. Independent general, timing and
migration review dispositions are retained alongside the logs.

No native adapter, startup or GUI behavior changed. Sanitizer, Python, listening,
device, aesthetics, keyboard, IME and accessibility checks were not repeated for
this pure authored-state change. The previous source-audio sanitizer and CI
results remain limited to [that implementation](source-audio-2026-09-21.md).
Audio render plans, resampling, output and authored import are still required.
