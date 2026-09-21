# Source registration qualification, 2026-09-21

This slice implements live selected-stream qualification, durable measured source
evidence and atomic authored registration with optional full-source insertion.
The developer CLI exposes that path. It uses the existing explicit presentation
basis; automatic first-primary/provisional basis state and native import remain
open. [Source registration](../SOURCE_REGISTRATION.md) defines the boundary.

The run used Apple M5 Max, 128 GiB, macOS 26.5.2, Rust 1.97.1 and pinned LGPL
FFmpeg 8.0.3 at `/tmp/deadpan-media-compatible-xyhilms4/prefix`.
The [source manifest](../../tools/media-qualification/evidence/2026-09-21-source-registration/source-manifest.json)
records the base revision and exact implementation and fixture hashes.
The prior timing commit `ed5c8eccebb8bb77451284f95b99b4ed33c49c93` passed
[CI run 35641118891](https://github.com/micthiesen/deadpan/actions/runs/35641118891).

## Actual media and persistence

Qualification tests open real CFR, VFR, offset A/V, rotated/anamorphic and
audio-only fixtures through actual source sessions. Round-tripped snapshots
retain original clocks, frame identities, audio coverage, stream interpretation
and the exact common origin. Negative translated origin cases retain alignment.
Strict wire tests reject malformed origins, versions, stream identities, metadata
and unsupported fields. A compile-fail test confirms that persisted JSON cannot
construct the live admission token.

Store integration tests retain actual originals, create live tokens and commit
registration/insertion. The offset fixture retains picture start `640/1001`
frames, audio samples `[95072,288288)` and a 121-frame beat at 30000/1001 fps.
No missing priming evidence is inferred. Tests cover:

- Read-only preview alongside the writer, registration without insertion,
  current-asset deduplication, insertion, fresh undo/redo revisions and reopening.
- Moving the complete package and decoding its managed original again.
- Reusing an asset ID after undo while retaining distinct receipts and exact
  indexes for both historical branches. Deleting an abandoned branch's receipt
  makes validation fail even when the current head is intact.
- An injected SQLite history-write failure that rolls back receipt, asset, node,
  revision and cursor together. The retained original remains available and the
  same uncommitted revision can be retried.
- Missing bytes after decode, mismatched original identity, cancellation, stale
  revision, read-only writes, generic qualified-asset ingress and corrupt evidence.

CLI subprocess tests cover CFR, offset A/V and 44.1 kHz audio-only media,
explicit stream selection, no audio-failure fallback, writer coexistence,
stale-before-offline rejection, deduplication and undo/redo. Protocol tests reject
unsupported, missing or malformed versions before opening the project or media.
A project with current
generation requests rejects commit without genuine relevance context while
allowing preview. The CLI does not fabricate a context plan.

## Historical project preservation

The [fixture generator](../../tools/media-qualification/evidence/2026-09-21-source-registration/fixture/generate.py)
archives and compiles `ed5c8eccebb8bb77451284f95b99b4ed33c49c93`, whose doctor
reports core schema 8/database schema 13. Its old store API migrates the schema-12
seed and writes signed picture/audio placements, direct and occurrence edits,
marks, a branch and pending redo. Two independent SQL dumps match byte for byte.

The [manifest](../../tools/media-qualification/evidence/2026-09-21-source-registration/fixture/manifest.json)
records 46 revisions, 25 history edits and one pending redo, alongside retained
original and generation records. Migration to database 14/core 9 compares all
historical snapshots, requests and forward/inverse patches under frozen core-8
meaning. Old assets remain unqualified; operational rows and backups survive.
Corrupt old documents/commands/patches and preexisting modern qualification tables
fail before promotion. Core wire tests also reject new receipt fields, including
null, and ImportSource commands in older schemas.

## Verification scope

The repository gate passed formatting, workspace Clippy with warnings denied,
**546 Rust tests** with zero failures or ignored tests, workspace build and doctor.
The [gate report](../../tools/media-qualification/evidence/2026-09-21-source-registration/gate/report.json)
records the exact commands and output. [Review dispositions](../../tools/media-qualification/evidence/2026-09-21-source-registration/review.json)
record independent general, admission/history and migration reviews.

General review found that the new CLI request lacked the protocol field used by
the other JSON command envelopes. The request now requires `protocol: 1` and
rejects other versions before opening the project. A subprocess regression test
covers future/zero versions, missing/null/string versions, commit and preview,
a held writer lock and an absent original. The reviewer checked the follow-up
and confirmed the finding resolved. Admission/history review found no
concrete defects; its separate targeted test attempt also lacked the FFmpeg prefix.

Migration review found no code defects and identified stale current-schema
references in the timing and audio-mapping guides. Those references now name
database 14/core 9; the related picture-mapping guide was updated too. Its
separate store test attempt lacked the qualified FFmpeg prefix, so it did not
run. The full repository gate above used the qualified prefix and includes the
34-test migration suite.

The first store test run failed six cases because the fixture helper passed a
path containing `..`, correctly rejected by original retention. The helper now
canonicalizes repository fixture paths. The test-only managed object path also
now includes the actual `blake3-` filename prefix. The corrected six-test run
passes. Intermediate logs are retained in the evidence directory. A CLI test
compile caught an incorrect test accessor; its fixture setup also needed legacy
SQL loading with foreign keys disabled and the actual media directories. These
test setup failures were corrected before the final gate.

No native adapter, startup, focus or control changed. ASan/UBSan, native lifecycle,
GUI aesthetics, IME, accessibility and keyboard navigation were not repeated for
this host/persistence slice. Existing [preview evidence](source-preview-2026-09-21.md)
retains its limited visual and keyboard scope. The Python qualification harnesses
were not rerun locally. Actual media decoding is exercised in Rust integration
and subprocess tests; this does not establish listening, playback or export.

## Remaining work

Automatic project basis state/adoption, native import/retry/relink controls,
legacy asset requalification, still-image import, bookmark resolution and the
full format/color matrix remain open. Receipt/index size bounds are defensive
limits, not measured large-media capacity. Media scheduling, audio output,
source-aware generation context and shared playback/export remain required.
All full-product requirements and gates remain open or partial.
