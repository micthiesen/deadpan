# Compact audio reanchors

Core 21/database 27 extend [owned timing bindings](OWNED_AUDIO_BINDINGS.md) with
ordered `AudioReanchorStep` records. They preserve each physical occurrence's
resume coordinate when structural insertion shifts its allocation. [Repeat-gap
bindings](GAP_AUDIO_BINDINGS.md) extend this state in core 22/database 28.
The native [slice workflow](SLICE_PLACEMENT.md) uses these bindings for linked
Original insertion and replacement in ordinary Sequence scopes.

## Allocation and support

`FrozenAudioLayout::project_scoped_with_allocation` returns an affine projection
and an optional visible range in physical-local frames. It intersects every
ancestor Retime selection, including transparent Partitions. Empty or disjoint
allocation is `None`; a clamped endpoint is not an entry. The separate meaningful
support query retains hidden Partition context for filters and DSP.

Projection follows one indexed ancestor path using stable play identities and
compact Repeat layouts. It also supports actual gaps after a stable preceding
play, using the gap's own duration. Both physical queries reject crossing a
nonunity Preserve stage. Its output and selected input have separate scopes.

## Chronological intent

An `OwnedAudioBinding` retains its lattice, optional legacy resume, and optional
`reanchors` array. Empty arrays are omitted. Each step contains a placement
template and an optional positive exact window in that template's captured scope.

Resolve each step's lexical scope independently. A birth in a narrower definition
drops the enclosing window and constraints outside that definition, while keeping
intrinsic Partitions. A new outer wrapper selecting the same retained root keeps
that root's own window. Birth does not imply entry zero: a new play of
`Repeat(Partition(A,[1,3)))` enters A at one even if a cut old occurrence enters
at two. Intersect the retained allocation with the clock's selected input and
applicable window. An absent entry leaves the previous sampling map intact.

Begin with the legacy resolved anchor and local phase, or zero phase at the
lattice's meaningful start. For each present entry `e` and previous anchor `a`:

```text
phase = phase + (B_step(e) - B_step(a)) * step.local_frames_per_sample
anchor = e
```

Every boundary uses its own RootRoundEven or PointCeil grid. Earlier phase terms
remain the initial state; timing geometry cannot reconstruct their history.
Normal plan/audio consumers receive the existing resolved scalar anchor and
phase. No alternate PCM pipeline is introduced.

## Commands and persistence

Split and occurrence isolation remap live arguments in every step while keeping
historical aliases. Pruning retains step-only clocks. Allocation reservation and
changed-owner reporting include them. A supported root `InsertTime` appends after
existing steps; bindings without steps retain their earlier scalar phase-term
authoring behavior exactly, including during legacy replay.

Old terms and new steps share the 256-item limit. All templates participate in
aggregate collection, validation, query and wire-byte budgets. Typed and JSON
admission reject zero/reversed windows. A short read cannot bypass retained
Preserve preparation or source admission.

Database schemas 1 through 26 migrate through complete replay on a backed-up
copy. Core schemas 16 through 20 use a closed binding adapter rejecting
`reanchors`, including empty, null and escaped field names, in snapshots and
both directions of history patches. Older histories gain no invented steps.
Audio-context schema 2 is unchanged and cannot carry owned binding state.

## Ripple deletion

`DeleteRipple { node, timing }` removes a complete child of an ordinary Sequence
in one transaction. Capture the downstream owners on the original, undeleted
tree at the old deletion end, then remove the child. Later siblings at each
ordinary Sequence level retain their old sample entry. Existing sampling
lattices and earlier resumes remain intact; compact Repeat plays and gaps stay
compact, and a moved nonunity Preserve stage retains its complete input history.

The timing allocation must equal the new revision. Empty-child deletion and
deletion through project end do not capture a clock because no surviving time
moves. Marks follow their existing content/pinned/loss policies; the root sound
bus receives exactly one deletion transform and removed Hold allowances retire.
Root deletion, missing children and Repeat/Retime ancestry fail before mutation.
A whole Repeat or Retime may be removed as a direct ordinary Sequence child.

Native `dd`/`:delete` and the public CLI `delete` verb use this command. The CLI
allocates timing ordinal zero before both dry-run and commit; explicit
`delete_ripple` accepts the full typed identity. Historical core `Delete` keeps
its original reduction so saved patches replay exactly. Current core 34 and
database 43 need no new fields or migration. Frozen command adapters reject
the new tag. [Qualification](qualification/ripple-delete-2026-09-30.md) records
the decoded-PCM regression, durable history and integration checks.

### Selected ranges

`DeleteRange { parent, range, identities, timing }` removes a nonempty global
half-open interval in an explicit ordinary Sequence. Its endpoint admission
uses the recursive unity Partition path shared with edited-slice placement;
splitting retains complete owner contexts without reserving an inserted Source.
Historical Source replacement, standalone Split and InsertTime admission remain
unchanged. The structural `range_deletion` query reports the required Split IDs;
it does not waive the separate audio-work or final-document validation limits.

Capture original sampling lattices before splitting either endpoint. When a
suffix also moves, a second timing identity retains its entries on the split,
undeleted tree before removing time. Both identities use the new revision; the
second ordinal is the supplied ordinal plus one. A deletion with no endpoint
split uses the supplied identity for its suffix alone. An aligned terminal
deletion captures no clock. Prefix fragments retain their original mapping.
The root sound bus receives one deletion transform, and Split transports Hold
permissions and marks before their usual removal policies apply.

Source, ordinary Hold and nested unity Partition fragments may be partial
endpoints. Framing and audio treatments on retained windows keep their owner
contexts. Complete intervening composites are retained structurally until
removal. Empty groups at either endpoint survive; those strictly inside the
range are removed. A Sequence under Repeat/Retime and partial Repeat, general
Retime or Generated Hold endpoints still require occurrence editing. One inverse
restores the complete authored state.
The operation adds no persisted document fields or schema version.

## Atomic range moves

`MoveRange` jointly splits source and destination endpoints, then captures the
moved and displaced physical owners on one unchanged-time layout. Each affected
owner receives one reanchor before the final child order is installed. Old entry
phase and complete provider support remain intact; final sample allocation uses
the new absolute boundaries. Whole units preserve their identities, while live
source/destination Sequence treatments follow final ownership. Root-owned sounds
retain their unchanged clock and routes. See [atomic moves](ATOMIC_MOVES.md).

## Remaining work

Current steps represent Source, Hold, Repeat-gap and opaque Preserve output resumes.
Movement through temporal occurrences, the remaining raw-recipe lifecycle,
role-only deletion, temporal occurrence deletion and the full Visual/register
workflow remain open.
Native paste and partial-range deletion must not use separate Split/Edit commits.

### Gap ownership design record

The following design is now implemented by [authored gap bindings](GAP_AUDIO_BINDINGS.md).
It records the transition from core 21/database 27; current verification is
tracked with that implementation.

The next implementation should add an optional `gap_bindings` map keyed by the
owning Repeat, sharing typed owner helpers and the same resolver/PCM pipeline.
A recipe discriminator on the reference clock separates a node from its gap.
Keep the own-gap preceding-play argument separate from the outer Repeat path.
Add an explicit gap-definition PointCeil scope for configured gaps with no
occurrence, including one-play Repeats.

Gap survival depends on the historical preceding play having a positive gap.
A former final play gaining a gap is a birth even if its play identity survived.
Child overrides do not remove ordinary following gaps. Resolve outer births
first, then the own-gap birth as the innermost scope; an outer birth can retain
a surviving inner gap's placement inside that outer definition.

Before adding this vocabulary, give clock scopes typed coordinate-domain
identity. A whole Repeat output and its gap recipe may share a NodeId while
using different coordinates. The current node-root equality is valid only for
the existing node scopes; it must not retain a Repeat-output window on gap birth.
Canonical gap projection uses local zero, unit scale and gap duration, without
inventing a preceding play or querying the whole Repeat as a gap definition.

Capture configured gaps before they render. Copy/remap their owners and live
arguments with Split/isolation, retain bindings across play resize/reorder, and
prune them when the gap is removed. Re-adding a gap must not resurrect removed
intent. Both root and point walkers must intercept dynamic gaps and seeded
gap domains/definitions. Bypass exactly the current physical binding; preserve
gap recipe identity through policies, prepared caches and source admission.

This will require another strict schema boundary. Freeze nested clock/template
vocabulary in existing legacy bindings, including phase and reanchor placements;
the present shallow old-binding check deliberately relies on that nested grammar
remaining unchanged. Reject all new gap fields in old snapshots and patches,
including empty/null and escaped names, and give old histories empty gap maps.

Required next evidence includes interrupted and later full RoomTone gaps at
NTSC, former-final/new/reordered/overridden plays, one-play capture followed by
growth, nested births, same-ID distinct-clock windows, duration/policy changes,
direct definitions and signed physical domains, Preserve zero-point silence,
hidden gap re-exposure, durable history and bounded billion-play resolution.
This subsection preserves the original design requirements.

[Qualification](qualification/audio-reanchors-2026-09-26.md) records this
increment's tests, migration evidence, reviews and limitations.
