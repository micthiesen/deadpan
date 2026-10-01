# Atomic linked range moves

The core/headless command is implemented and verified. The native Move control
remains open. Section 9.7 of the specification still requires its visible
removal/insertion preview and complete keyboard flow. See
[qualification](qualification/atomic-move-2026-09-30.md) for evidence and limits.

`Command::MoveRange` relocates current authored content in one reversible edit.
It preserves the identities of whole moved units and does not import a copied
slice. The selected range, source parent and destination all address the same
pre-edit revision.

## Coordinates and scope

The command carries:

- `source_revision`, which must match the current and expected revision;
- `source_parent` and a nonempty global half-open Edit `range`;
- `MoveRangeDestination::Seam { parent, index }`, where `index` is the original
  destination child slot, or `Interior { parent, target, at }`, with a strict local
  offset in the original named direct child;
- one `SplitIdentities` pool and an `AudioTimingId` allocated under the new revision.

Both parents and their paths to the root must be ordinary Sequences. The source
and destination may share a parent, be in sibling groups, or have an ancestor
relationship, provided the destination parent survives the move. Endpoint cuts
admit Source, ordinary Hold and supported unity Partition chains. Complete
intervening groups, Repeats, Retime stages and accepted Generated Holds move as
owned units. Partial temporal occurrences remain outside this boundary.

For source `[a,b)`, length `L=b-a`, and original destination boundary `d`, the
inserted start is `d` if `d<=a`, otherwise `d-L` if `d>=b`. Total project duration
is unchanged. A destination strictly inside `[a,b)` or inside a moved subtree
fails. Boundary destinations can be meaningful reparenting operations even when
their global times coincide.

`ProjectDocument::range_move` is a read-only preflight. It reports the exact final
inserted interval, original destination, final removal join, required Split IDs,
timing slots and whether the operation is an exact no-op. The existing whole-node
`Command::Move` keeps its historical index-after-removal semantics.

## Structure and identities

Plan source In/Out and destination cuts jointly against the original tree. A
single Source may require three distinct cuts; the first creates a retained
context and later cuts refine its windows. Shared boundaries are cut once. The
temporary node limit includes every required context copy.

Destination seams retain their exact slot among co-located empty children. Empty
source children strictly inside the range travel with it; empty children at In
or Out remain in their parent. A zero-length range does not select empty groups.
Preserve the source parent even if it becomes empty.

Whole moved nodes, marks, Repeat play identities, sparse overrides, accepted
artifacts and existing timing/lineage identities retain their names. Only the
necessary endpoint contexts receive fresh node IDs. The complete supplied Split
pool is validated, including unused tails. Do not use the paste importer, create
a new enclosing group, or expand a Repeat to move it.

An exact no-op is resolved before Split or audio capture. It changes no authored
structure or clocks. Generic command history can still record the request's new
revision; native controls should explain the no-op without committing it.

## Picture, audio and ownership

Capture original sample lattices before transparent endpoint cuts. The split
tree still has its original duration. From that tree, capture one shared placement
layout and append at most one reanchor to each affected physical owner. Moved and
displaced owners use their own original windows. Stop physical traversal at
nonunity Preserve outputs, retaining their complete processing context.

Build the final child order directly. An intermediate delete-only document can
lose retained samples at fractional frame boundaries, so it must not become the
basis for later capture or validation. Preserve old entry phase and provider
support while allocating each final interval as `B(end)-B(start)`. A moved or
displaced interval may gain or lose one allocated sample at fractional rates;
real retained support determines its PCM, and exhausted support stays silent.

Moved groups retain their own framing and gain. Source and destination ancestors
remain live on their final owner clocks. Moving contents out of a group does not
copy that group's treatment, and entering a new group applies its treatment once.
Captured Hold geometry remains independent of live ancestor framing.

### Root sounds stay in the root clock

All currently supported `SoundEvent` recipes are root-owned. MoveRange leaves
that owner, its clock and its duration unchanged, so `sounds` and `sound_routes`
remain unchanged. A sound overlapping the moved interval does not become owned
by its children. Existing routed sample labels and envelope phase remain intact.
Do not apply a ripple deletion/insertion journal to an internal move.

Transparent Split remaps existing Hold/Repeat-gap allowances where necessary.
After relocation, current Hold policy and those retained grants apply at the
issuers' final positions. New overlap creates no new grant. These live gates can
change the audible mix without changing the root sound's recipe or route.
See the [sound ownership contract](SOUND_EVENTS.md#structural-edit-contract).

## Marks and history

Marks retain logical identities, boundary bias and loss policy. Split may create
physical bindings, but the move does not copy logical marks. Child-local anchors
follow their retained host. A parent-local mark whose content leaves its host
uses its explicit OutsideHost policy. Absolute Sequence pins remain absolute;
unresolved intent is not retargeted to newly adjacent material.

The store commits one request, reversible patch, revision and history cursor
change in one transaction. Dry-run writes nothing. Undo/Redo restore authored
state with fresh revisions. A register captured before any later edit or Undo
can still be copied; it cannot authorize a move against that new revision.
Accepted generated media stays accepted without reviving its generation request.

## Remaining work

Native Move selection, source-removal and destination-insertion comparison,
audition, stale draft handling and final result selection remain required.
Repeat/Retime occurrence interiors, role-only operations, cut-to-register,
persistent/named registers and the full editing language also remain open.
