# Selected-play allocation preflight, 2026-10-12

`scoped_many_requirements` no longer rejects valid projects whose node, mark or
historical audio identities collide with its temporary staging names. Native
`:scope plays` uses this query before committing gain, Camera and pause-audio
changes. The committed command and caller-owned identity rules are unchanged.

The query now searches past occupied names in its private document copy. Its
node inventory is shared with actual isolation admission and includes live
nodes, live lineage origins, retained timing nodes and retained lineage origins.
Marks and all pending target names also reserve their names. Candidate work is
bounded by occupied/targeted names plus requested identities; zero-identity
stages skip the inventory. No temporary
name or staged revision leaves the query, and no project name is reserved.

## Evidence

Base `6d23060b1c606da4e8230e564eef737c608851af`; Apple M5 Max, 128 GiB,
macOS 26.5.2 (25F84), Rust 1.97.1, locked dependencies. The primary agent
implemented and self-reviewed alone.
[Retained evidence](../../tools/media-qualification/evidence/2026-10-12-scoped-preflight/metadata.json)
records source/binary hashes, commands, results and compressed logs, including
both failing regression runs.

Five regressions first passed the actual edit with caller-owned identities,
then failed the allocation query with `IdentityConflict` on the unchanged
production code. They independently cover live nodes, marks, live lineage,
retained timing nodes and retained timing lineage. Seven positive cases also
cover dense collisions across successive stages and `u32::MAX` Repeat plays.
Each compares exact node/mark counts, selected and untouched plays, unchanged
duration, the inverse, repeated query results and the input's serialized bytes.
A repeated target becomes a no-op after the earlier staged edit. Two further
review regressions reproduced false admission of missing node and Repeat names:
an earlier temporary clone happened to supply those exact names. Reserving all
pending target names now retains the original rejection. All nine regressions
pass on the final source.

`cargo test --locked -p deadpan-core` passed all 1,073 tests, zero failed or
ignored. Self-review then added the zero-identity fast path and the missing-target
guard; focused checks cover that final source rather than repeating the full core suite.
`cargo test --locked -p deadpan-core -p deadpan-app scoped` passed 80 tests
on the final source, zero failed or ignored: 34 normal app tests and 46 core
tests. These overlap the full core total and are not added to it.

Strict workspace all-target Clippy with `deadpan-app/ui-harness` passed before
the missing-target guard. Strict core all-target Clippy passes on the final
source; `cargo fmt --all -- --check` also passes. The debug native link retains the existing warning
that `__eh_frame` exceeds 16 MiB; it did not fail the build or tests.

The native regression creates an actual SQLite package containing the colliding
names, edits both inner plays in only one outer play, and checks one revision /
one Undo, unchanged other plays and unrelated beats, duration, Undo, Redo and
reopen. It opens no native window or audio device.

## Scope

This fixes allocation for existing scoped value edits. It does not implement
pause insertion inside Repeat or Retime, change audio processing, or qualify
GUI appearance, physical output or performance targets. Those requirements
remain open. No DP requirement or release gate is newly complete.
