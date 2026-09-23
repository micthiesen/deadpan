# Logical marks and physical fragments

Core schema 13 keeps one logical `MarkId` with bounded physical bindings. This
supports retaining a named mark through [structural Split](STRUCTURAL_SPLIT.md).
Native mark editing is still open; admitting bindings
does not prove that a splice has produced the correct lineage.

A mark retains its primary `owner`, `boundary` and `state`, plus an optional
`fragments` array. Each fragment holds an owner, coordinate and state. The label,
insertion bias and loss policy belong to the logical mark and apply to every
binding. Empty fragments are omitted from JSON. The limits are 1,024 bindings
per mark and 100,000 across the document, including primary bindings. Existing
node, depth, identity and JSON byte limits also apply.

## Ownership and lifecycle

Ownership tracks an authored node's lifetime, independently of where a boundary
is hosted or whether that owner currently contributes visible output. For
example, an A-owned Local mark at B remains valid while A exists, even when a
retained partition hides A's contribution at that time. Local bindings hidden
by a crop stay bound; Source bindings retain the original asset clock without
requiring a current timeline use. These are the existing mark semantics.

Each structural edit transforms every binding atomically with the tree. With
`DeleteOwned`, a lost binding is removed; the logical mark disappears only when
none remain. The first surviving binding becomes primary. With `KeepUnresolved`,
a lost binding retains its last coordinate and typed reason. A surviving bound
fragment remains usable even if the primary is unresolved. No unresolved
binding automatically reattaches. Exact duplicate physical records collapse
deterministically during lifecycle transforms.

`SetMark` replaces the entire logical mark with one explicitly bound boundary.
`DeleteMark` removes all its bindings. During real occurrence isolation, owned
Local/Source bindings are collected under one fresh logical mark for that copy.
Bound Local hosts inside the copied tree are remapped; outside hosts and all
unresolved coordinates retain their meaning. Concrete occurrence bindings
relocate once, and sequence-pinned events stay once. Neither operation enumerates
Repeat plays. Identity or binding exhaustion rejects the entire transaction.

Pure Split must retain the existing logical MarkId while building the correct
physical bindings. It must map owner and coordinate host independently, preserve
external references and allocate no fake mark copy merely to keep both sides.
Those construction rules remain work for [structural splices](STRUCTURAL_SPLICE_DESIGN.md).

## Exact named resolution

Named queries inspect bound bindings against one immutable revision. Local
bindings below Repeat still need explicit complete occurrence scope, and Source
bindings always require an actual Source occurrence. A matching implicit
candidate cannot make another candidate's required scope disappear. Explicit
Local scope selects that physical host. Fully scoped Occurrence and Sequence
bindings do not accept added occurrence scope; select their physical boundary
through an ordinary point query when a logical mark is ambiguous.

Hidden bindings are excluded from a result. Equal exact project coordinates
produce one boundary carrying all matching binding ordinals in `mark.bindings`.
Ordinal zero is the primary binding; subsequent ordinals refer to `fragments`
at index one less. These ordinals belong to the returned revision and must not
be reused after an edit. The result's `target` is the first matching physical
target, not an exclusive choice of attachment ownership. Ordinary point queries
omit the `mark` metadata.

Distinct exact coordinates return `MarkAmbiguous`, even when they round to the
same project frame. Quantization occurs once, after exact resolution. A mark
with no bound bindings returns `MarkUnresolved`; a bound mark with no visible
matching candidate reports the relevant scope or mapping failure.

At an internal Partition start, left bias selects the other side; at its internal
end, right bias selects the other side. Apply this visibility rule before
deduplicating exact coordinates. External context endpoints remain legal, and
ordinary authored Retime crops retain their previous inclusive endpoint behavior.
Stored binding validation remains independent of this query visibility test.

## Persistence and evidence

Database schema 19 introduced core 13; current database 20 stores core 14 and
replays database 19 through its frozen multi-binding grammar. Schemas 1 through 18 replay every revision and
forward/inverse patch through strict frozen adapters. Core schemas 3 through 12
use a shared frozen mark grammar that rejects `fragments`, including `[]` and
`null`. Old marks gain one binding without JSON growth. The core-12 adapter
retains Partition purpose, and earlier adapters continue to reject it.

[Lifecycle tests](../crates/deadpan-core/tests/mark_fragments.rs) cover partial
loss, promotion, unresolved state, copy subsets, independent ownership,
serialization, inverse history, replacement and bounds. [Resolution tests](../crates/deadpan-core/tests/mark_resolution.rs)
cover exact ambiguity, seam bias, original clocks, explicit scope and compact
billion-play queries. [Legacy tests](../crates/deadpan-core/tests/legacy_marks.rs)
exercise strict wire rejection and the 100,000-mark compatibility boundary.
[Store tests](../crates/deadpan-store/tests/migration.rs) use authentic schema-18
history and verify full replay, retained backups, pending redo and failed
promotion. [Persistence tests](../crates/deadpan-store/tests/persistence.rs)
exercise modern fragment loss through commit, reopen, undo and redo.
