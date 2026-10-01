# Retained audio clocks across local-origin changes

Core schema 37/database 46 retain an exact translation between a physical
owner's current local coordinates and each captured audio clock. This is a
prerequisite for extending a Source before its previous local zero. It adds no
Trim command or native mode.

## Coordinate contract

`AudioPlacementTemplate.reference_local_offset` is an exact signed frame ratio.
It defaults to zero and is omitted when zero:

```text
historical_local = current_local + reference_local_offset
```

For a historical placement with origin `o`, scale `s` and offset `d`, resolution
returns origin `o + s*d` and translates its retained local support by `-d`.
The sample grid, scale, captured duration, Repeat identities and frozen layout
remain unchanged. `ResolvedAudioPlacement.local_duration` is the historical
recipe duration; it does not imply that the translated domain starts at zero.

The existing sample-boundary calculation therefore evaluates the same historical
point after a local translation. It does not reconstruct timing from current
picture coordinates or round the translation to an audio sample.

## Rebase operation

`OwnedAudioBinding::rebase_local(prefix)` returns a complete cloned binding for
`new_local = old_local + prefix`. A positive prefix moves existing material later
inside the new physical domain. Negative values support the inverse translation.

| Retained field | Change |
| --- | --- |
| Every placement's `reference_local_offset` | Subtract `prefix` |
| Resume `local_boundary` | Add `prefix` |
| Phase term `from_local` and `to_local` | Add `prefix` |
| Phase constant | Retain |
| Chronological reanchor window | Retain in its captured enclosing clock |
| Frozen layout, grid and identities | Retain |

The operation checks term/serialization bounds and exact arithmetic. A failure
leaves the input untouched. It does not mutate a document, validate a new Source
recipe or allocate a revision. The eventual editing command must capture an
unbound owner's clock before changing its geometry and commit all affected
state atomically.

Chronological reanchors independently project their frozen layout. Their entry
point is historical-local after captured crop/window constraints. Subtract the
step's offset before using that entry in current-local resume evaluation.
Translating the captured window instead would change which old material it names.

## Format and lifecycle

Current documents, patches and copied binding state retain the offset. Newly
captured templates start at zero. Existing identity-renaming paths copy the
numeric translation while changing only live names.

Supported historical readers keep their closed placement vocabulary. They
reject the new field even when explicitly zero, null or escaped in JSON,
including nested phase/reanchor templates. Projection into a historical form
requires zero offsets. Existing dormant-support and older gap/reanchor guards
still apply.

Database schemas 39 through 45 are unused development formats and are refused
without writable acquisition, migration or backup creation. Existing adapters
for schemas 1 through 38 remain. `FrozenAudioContext` stays at schema 5 because
it cannot carry owned binding state.

## Verification boundary

Core regressions cover signed composition and inversion, checked overflow,
per-play fractional sample allocation, chronological reanchors, PointCeil
support and historical wire rejection. Plan and decoded-PCM regressions model a
physical prefix behind a unity Partition so the selected output stays fixed.
They compare exact sample phase, block partitions and inverse restoration.
The storage regression preserves nonzero offsets and exact frozen layouts
through create/reopen, serialized history and fresh-revision Undo/Redo.
See [qualification](qualification/source-origins-2026-10-01.md).

Framing and audio-treatment owner clocks still need explicit preservation before
a public Source-prefix or Trim command can use this operation. Re-expressing
audio sampling alone does not preserve an existing camera path or gain envelope.
