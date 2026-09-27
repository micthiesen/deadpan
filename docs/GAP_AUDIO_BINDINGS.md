# Authored Repeat-gap audio bindings

Core 22/database 28 extend [owned audio timing](OWNED_AUDIO_BINDINGS.md) to a
Repeat's configured gap recipe. The optional `gap_bindings` map is keyed by the
owning Repeat, independently of node-output bindings. It uses the same retained
timing records, phase expressions, [reanchor steps](AUDIO_REANCHORS.md), bounded
admission and PCM consumer. No second audio renderer or mutable media body is
introduced.

This is timing infrastructure for massaging the one Original. General atomic
moment splice, movement/raw-recipe lifecycle and the native Visual/register
workflow remain open. Existing ImageGen boards remain the UI targets.

## Identity and clocks

`AudioReferenceClock.recipe` distinguishes `node` from `repeat_gap`; the default
node value is omitted. A gap template has a separate `gap_after` argument, either
live on its current Repeat owner or captured to a historical preceding play.
Outer Repeat occurrences remain in `InstancePath.repeats`. The gap's own Repeat
is never inserted into that path as though the gap were a child play.

A live gap survives only if its preceding stable play had a positive gap in the
retained layout. A new play or a formerly final play gaining a gap is a birth.
Overriding the preceding child does not remove its following gap. Reorder retains
historical placement for surviving gaps. Capture includes configured positive
gaps in one-play Repeats before any gap renders.

Resolve outer definition births first. A surviving inner gap can retain its
placement within a new outer definition. Own-gap birth instead selects
`GapDefinitionPointCeil`: local zero, unit scale, configured gap duration and no
invented preceding play. Direct gap-definition inspection excludes its live
own-gap argument explicitly.

Clock scopes carry coordinate-domain identity. `NodeOutput(repeat)` and
`RepeatGap(repeat)` share an owner ID but have different coordinates. A reanchor
window in the Repeat's full timeline must not clip a newly born gap definition.
Visible allocation still clips every relevant Partition; meaningful support
remains separate, and a hidden gap has no new entry.

## Ownership and current policy

Capture visits every owned default and override subtree once, retaining one
shared pre-edit layout. Split and occurrence isolation copy gap owners and remap
live arguments, while historical aliases remain fixed. Allocation reservation,
changed-owner reporting, aggregate limits and timing pruning visit both maps.

Positive gap-duration changes retain the clock and resume map. An existing
nonnegative resume anchor may exceed a shortened current gap's duration; the
current raw support still limits all reads. Removing the configured gap prunes
its binding. Re-adding it stays unbound until a new capture and cannot revive
the removed intent.

Current gap duration, source media, Silence, RoomTone or Tail policy and authored
edge choices govern evaluation. Retained layouts supply timing only. Original
receipt/byte verification and preparation budgets remain active. A silent gap
with no input-grid sample still suppresses its output interval through Preserve.
Unsupported Tail processing continues to fail before source I/O.

## Shared evaluation

Both root and point walkers intercept bindings for dynamically reached gaps and
seeded gap domains before returning a leaf. `AudioBound` retains gap identity and
current duration. Its raw operand uses the `RepeatGap` definition, bypassing only
that gap's binding. A node-output bypass cannot disable a gap recipe implicitly.

Signed root/point placements preserve their grids. A bound definition pins its
retained reference anchor to the consuming grid's allocated anchor and advances
by the exact current-to-retained rate. It does not acquire the fractional start
phase of an unbound recipe merely because its placement origin is fractional.
RoomTone and Preserve caches retain definition/occurrence identity and re-admit
source dependencies under the same request budget.

Core 22 introduced capture of gap-bearing unaffected prefixes. Core 24 adds
[pause insertion at root seams](INSERT_TIME.md) before composite suffixes, using
compact current placement steps for physical and default-gap owners. Arbitrary
interior splice must still resolve nested scope, all reanchors, marks, identities
and generation relevance in one atomic command.

## History and verification

Database schemas 1 through 29 migrate through full chronological replay on a
backed-up copy. Frozen core 21 admits existing reanchors but rejects gap maps,
recipe discriminators, own-gap arguments and gap-definition clocks. Older
binding adapters also freeze their nested clock/template vocabulary. Empty,
null and escaped new fields are rejected in old snapshots and both patch
directions. Older projects gain no invented gap bindings.

Core 23/database 29 additionally retain [independent gap branches](REPEAT_GAP_BRANCHES.md).
The frozen core-22 boundary preserves default-gap binding intent while rejecting
that newer branch vocabulary and detached Hold references to gap clocks.

An actual preserved core-21/database-27 CLI supplies migration history from an
explicitly seeded initial reanchor document. Its later commands, undo, redo and
pending redo are produced by that binary; provenance distinguishes initial
fixture intent from command-generated history.

[Qualification](qualification/gap-bindings-2026-09-26.md) records passing focused
plan/PCM/CLI checks, migration tests and three independent reviews
with no remaining findings. The full workspace run retains its sandbox socket
failure and playback timeouts; all 11 playback cases passed on an unchanged
exact-binary retry. Concurrent app changes are tracked separately. No DP
requirement or release gate is complete.
