# Explode, duplicate and multi-play occurrence edits

Specification §5.2 requires `explode` to convert a Repeat into an ordinary
Sequence while preserving every current picture, sound, timing and override,
and requires copies to have new authored IDs, share immutable media and carry
no hidden live link. §5.4 requires a duplicate to carry its owned attachments.
This document records the core contract, its evidence and its limits.

## Explode

`Command::Explode { node, identities, timing }` converts one Repeat in place.
The Repeat keeps its identity, label, framing, treatments, edges, cutaways,
captions and beat sounds, and becomes the enclosing Sequence. Its effects were
already evaluated over the whole repeated passage, which a Sequence of the same
duration evaluates identically.

Each play becomes an owned child at its unchanged absolute position, in
current play order:

- An existing play override stays as that play's subtree.
- The first play without an override keeps the authored default definition.
- Every other default play receives a fresh copy through the same transparent
  clone as occurrence isolation. Copies share immutable media and audio
  lineage, never authored nodes.
- A positive default gap after a non-final play becomes an explicit Hold, as
  `IsolateGap` builds it, with the Repeat's gap edge policies.
- An explicit gap branch stays. The Repeat's gap boundary policies used to
  apply to a positive branch, so a Hard gap policy becomes Hard on the branch
  root's matching edge. Automatic stays automatic, and an empty branch (which
  had no gap extent) is unchanged.
- A final play's dormant gap branch rendered nothing and is removed with its
  owned content. If every play was overridden, the unused default definition
  is removed the same way. Marks owned there follow their loss policy.
- `RepeatEscalation` becomes explicit: each play whose escalation is not the
  identity is wrapped with its following gap in a new Sequence carrying the
  play's static centered pose and a constant clip-gain trim. A play and its
  gap shared that escalation, and the wrapper is applied in the same position
  of the framing stack and gain owner chain (inside the Repeat's own effects).

Captions count plays of their innermost enclosing Repeat. A caption whose
innermost Repeat was the exploded one is kept without a reveal in plays at or
after its reveal position and removed from earlier plays.

Marks and permissions follow their concrete play. Occurrence-anchored marks
are rebound to the copy that renders them and lose the exploded Repeat step.
Owned Local and Source marks in the default definition stay with the play that
keeps it; each copied play receives one fresh logical mark, as isolation does.
Root sound permissions addressed to a play's Hold move to that play's Hold, a
default-gap permission moves to the materialized gap Hold, and every address
loses the exploded Repeat step. Root sound recipes and routes are unchanged
because no root time changes.

### Retained audio clocks

Explode never recomputes a clock. A placement that names the exploded Repeat
is closed for its concrete play:

- If the play survives the placement's birth clause (or there is none), the
  live Repeat argument becomes `Captured` for that play and the clause is
  removed. This is exactly the clock the play resolved before.
- If the play was born after the capture, the placement's root becomes the
  clause's `DefinitionPointCeil` root, with outer clauses and arguments
  outside that scope removed and inner clauses kept, so an inner birth still
  wins exactly where it did before.
- A reanchor window authored in the enclosing captured scope was already
  discarded for a born play and is dropped. The one case a single root cannot
  express (an inner birth selecting the old captured root) refuses explicitly.

Only the exploded Repeat's own gap clock is captured, and only when a default
gap is materialized; other unbound owners keep their implicit clocks, which
explode leaves at the same positions.

`ProjectDocument::explode_requirements(node)` returns the exact node and mark
counts. Nodes are consumed per play in play order: the copied definition in
structural preorder, then a materialized gap, then an escalation group. The
command's `timing.allocation` must equal the new revision when a gap is
materialized. Explode refuses a non-Repeat target, an insufficient or reused
identity pool, document limits, and a Repeat inside a retained beat-sound
journal scope. History is one reversible transaction with an exact inverse.

The native `:explode` and semantic `SemanticInstruction::Explode` target the
selected direct-child Repeat, refuse every Visual selection, keep the cursor
and selection, and record and dot-repeat like Ungroup.

## Duplicate

`Command::Duplicate { parent, selection, identities, split_identities, timing }`
copies one exact direct child, inclusive sibling span or nonempty global range
of an ordinary Sequence immediately after itself. It captures the current
revision with `CapturedEditSlice::capture_selection` and resolves to the same
`SpliceSlice` (at a seam) or `SpliceSliceAt` (when a range ends inside a beat)
that paste uses, so marks, beat sounds, sparse overrides, retained clocks,
allowances and the single root sound transform follow the established copy
rules in [edited slices](EDITED_SLICES.md). The scratch capture timing never
persists. The copy enters under paste's neutral "Copied contents" Sequence.

`ProjectDocument::duplicate_requirements(parent, selection, timing)` returns
the paste pools and any Split identities. The stored history keeps the
Duplicate request; replay resolves it against the same revision. Root sounds
and Hold permissions remain outside structural ownership as for paste.
`SemanticInstruction::Duplicate { selector }` duplicates the selected beat or
Visual range and selects the copy.

## Multi-play and partial-range occurrence edits

`Command::EditScopedMany { edits, identities }` applies value edits to up to
1,024 explicit `ScopedTargetEdit { target, edit }` entries in one transaction.
Each target uses the [scoped editing](SCOPED_EDITING.md) Default/Play address
and its own value, so a relative change keeps each play's existing recipe.
Targets apply in order; a later target follows the identities an earlier
isolation gave its shared ancestors. Targets whose value is already current
are skipped with an empty pool; if every target is unchanged the command
refuses. `ProjectDocument::scoped_many_requirements(&edits)` returns the exact
per-target pools, staged in the same order. Remapping follows identical
Default/Play prefixes only: a later target reaching an already isolated node
through a different outer branch must name that node's current identity, and
otherwise refuses explicitly.

A partial range inside one play is a ranged value: `ClipGain::adjust_range`
over the target node's local frames isolates only that play and attenuates only
that range. Structural cuts inside one occurrence remain the existing
`EditOccurrence` Split. Hold-audio values in a multi-target edit receive the
same store admission as a single scoped Hold-audio edit.

## Evidence

- [`crates/deadpan-core/tests/explode.rs`](../crates/deadpan-core/tests/explode.rs):
  structure, overrides, gaps, dormant branches, captions, marks, escalation
  groups, refusals, exact undo, and resolved retained clocks equal for every
  play of a RepeatSelection Repeat; Duplicate of a child, span, interior and
  seam range with fresh mark identities and no scratch clock.
- [`crates/deadpan-core/tests/sound_allowances.rs`](../crates/deadpan-core/tests/sound_allowances.rs):
  nested explode moves concrete and default-gap permissions.
- [`crates/deadpan-core/tests/scoped_many.rs`](../crates/deadpan-core/tests/scoped_many.rs):
  plays 2-3 only, nested staging through an isolated outer play, partial-range
  gain inside one play, and unchanged-target handling.
- [`crates/deadpan-audio/tests/composite_insert/explode.rs`](../crates/deadpan-audio/tests/composite_insert/explode.rs):
  decoded 44.1 kHz PCM at 30000/1001 is bit-identical before and after
  explode through the raw time-mapped, edge-faded and limited stages for a
  Repeat with an override, nested Preserve, room-tone gaps and gain
  escalation; for born RepeatSelection plays after a pause, default and
  explicit room-tone gaps; for a beat-owned sound copied into every play with
  gain escalation and a root sound over the plays and gaps; and for nested
  Repeats exploded inner-first and outer-first.
- [`crates/deadpan-cli/tests/golden_renders.rs`](../crates/deadpan-cli/tests/golden_renders.rs):
  exploded Repeat-with-gap, escalation and nested-override packages hash
  exactly like the committed goldens of the original Repeats (every Metal I420
  frame and limited-bus PCM block).
- `cargo test --release --locked -p deadpan-cli --test preview_export`:
  `exploded-repeat`, `exploded-escalation` and `duplicated-range` recipes build
  through the CLI and verify plan and exported picture provenance.

## Limits

- Explode materializes every play. Plays are bounded by the document node
  limit; it never runs on a compact Repeat larger than that.
- Explode refuses inside a retained beat-sound journal scope.
- Explode converts one authored Repeat. A copy of it already isolated inside
  another Repeat's play override is a separate authored Repeat and stays one.
- Partial-range structural edits inside several plays at once (one cut applied
  to plays 2-3) remain separate EditOccurrence commands.
- Fixed while testing: binding implicit clocks (as `IsolateGap` then did for
  the whole project, and as InsertTime or RepeatSelection still do for shifted
  or wrapped owners) made the limited bus refuse a gained, nonunity Preserve
  inside a Repeat with "nonzero Original outside meaningful owner-clock
  support". The authored-gain owner walk mapped a play's first RoundEven output
  sample, which starts a fraction before the play's exact origin, onto the
  Preserve input's PointCeil grid just before point 0 and called it outside.
  The walk now attributes that overlapping sample to the first input point, as
  the current clock does, and `IsolateGap` binds only its own gap clock.
  Regression: `crates/deadpan-audio/tests/composite_insert/bound_preserve.rs`.
