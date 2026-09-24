# Authored audio lineage qualification, 2026-09-23

This increment builds on `1eb043c941bc923473e282edf6b169e236244076`, whose
[CI run](https://github.com/micthiesen/deadpan/actions/runs/35945736930) passed.
Core 15/database 21 retain explicit audio copy relationships through structural
Split and occurrence isolation, reversible patches and migrated history.
[The contract](../AUDIO_LINEAGE.md) defines their lifecycle and limits.

## Verification

The full repository gate passed with **1,048 tests**, zero failed and zero
ignored. All 341 source and fixture hashes stayed unchanged through the run.
[Raw evidence](../../tools/audio-qualification/evidence/2026-09-23-lineage/summary.json)
retains command results, compressed logs, source hashes and the gate script.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Pass |
| `cargo test --workspace --locked` | 1,048 passed |
| `cargo build --workspace --locked` | Pass |
| `cargo run -p deadpan-cli -- doctor` | Pass; development foundation |
| `cargo run -p deadpan-app --locked -- --smoke-test` | Pass; Metal and shutdown |

All 54 migration tests and 18 persistence tests passed together in the workspace
gate. The eight saved ImageGen boards/prompts, five unchanged imported spec files
and changed Markdown links verified. Tests ran on macOS 26.5.2, Apple M5 Max,
Rust 1.97.1 with the pinned FFmpeg 8.0.3 compatible developer prefix.

## Review

Independent general, lineage/clock and migration reviews found no issues. The
review covered the complete dirty and untracked increment against `origin/main`
at the base revision above. Reviewers made no source changes. The dedicated
core lineage suite passed ten tests; the focused core/reference plan suites
passed 30 integration tests and two preflight unit tests.

## Behavior exercised

The dedicated core tests exercise whole-context Split, root Split and shallow
refinement; lineage-only changed IDs; guarded inverse patches; and transparent
picture, label, edge and Group/Ungroup changes. Audio mapping, offset and
Hold-duration changes detach affected raw contexts and ancestors. Move checks
both parent chains. Deletion and override replacement/clear/shrink prune owners
while surviving copies retain historical names. Nested billion-play occurrence
isolation keeps compact orders. Malformed, duplicate, dangling and oversized
records and exhausted identity pools fail atomically.

Frozen layouts retain bounded optional lineage, accept older layouts without it,
and reject malformed token records before materialization. Reference plan tests
exercise actual Split and refinement, copied Repeat source/gap domains and
stable plays. Foreign plans, moved placement, changed audio mappings and distinct
Preserve input clocks do not gain compatibility merely from related tokens.
Physical domain identity remains separate from copy provenance.

An authentic database-20 fixture was created with the preserved pre-change CLI.
It includes direct Split, undo/redo, Repeat occurrence copying, occurrence Split,
deletion and a pending redo. An authentic database-4 fixture from a preserved
older CLI exercises real occurrence copying, undo/redo and override clearing.
Migration compares old snapshots and complete projected transaction summaries,
retains newly calculated lineage in modern history, and verifies navigation and
reopen. Initial legacy snapshots gain no inferred relationships. Tests reject
lineage fields, including null and empty objects, in initial/later snapshots,
forward/inverse patches and commands across all 20 legacy database schemas while
retaining the original database and backup. Imported initial allocation names
remain reserved after every current owner is gone.

Fixture executables and SQL dumps have [recorded hashes](../../tools/audio-qualification/evidence/2026-09-23-lineage/fixture-provenance.json).
The database snapshots were captured through SQLite's backup API. During test development, a fixture
with empty occurrence paths was replaced because it did not actually copy a
tree. The older SQL dump loader also needed foreign keys disabled while loading
its table-order dump, matching the existing fixture loaders. Neither correction
changed production migration semantics.

## Remaining work

Lineage is authored provenance, not proof of equal PCM, a live decoder binding
or media admission. Retaining audio through inserted time still requires authored
live-to-frozen sample bindings, composed phase anchors, compact Repeat lifecycle,
explicit policy replacement and genuine seam envelopes before the atomic Hold
insertion command can be connected to the app. This increment does not complete
a product requirement or delivery gate.

No GUI behavior changed. Visual, focus/IME and keyboard QA are not repeated for
this backend increment; the saved single-Original ImageGen boards and previous
native interaction reviews remain the design evidence.
