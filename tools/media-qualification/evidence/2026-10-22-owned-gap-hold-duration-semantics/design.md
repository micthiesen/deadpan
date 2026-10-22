# Owned-gap Hold duration, next bounded increment

Source inspection only, 2026-10-21. No Cargo, repository edits or execution
claim. Current selected-duration milestone remains frozen.

## Recommended contract

Extend the existing mixed and selected duration authorities to admit ordinary
Hold nodes inside explicitly owned gap subtrees. Reuse the existing commands,
runtime contexts, exact isolation pools and executed-proof mappings. No new wire
type, instruction, implicit default-gap NodeId, or value-only temporal edit.

A Repeat edge has exactly three relevant structural meanings:
1. Default follows only the literal child. It is dormant iff every current body
   play is overridden.
2. Play follows that stable iteration's effective body.
3. Play follows that same iteration's separately owned gap root.

For case 3, the owned root must equal gap_overrides[repeat][iteration]. The
iteration must be current. It is live iff position(iteration)+1 < len. The
position is looked up in compact IterationOrder, never inferred from allocation
ordinal. A final-play branch is a valid dormant definition, not a project
occurrence. Body overrides do not suppress that play's owned gap, and the shared
default-gap duration or absence does not determine an owned gap's duration.

The owned-gap edge can be anywhere in the ancestry. Thus:
- outer Default / inner Play can reach the inner Repeat's owned gap.
- outer Play / inner Default can reach an inner Repeat body located inside the
  outer Play's owned gap subtree.
- outer Play / middle Default / inner Play can combine both.
Default cannot denote an owned gap at the Repeat that owns that gap.

Accumulate dormancy as OR of all fully overridden Default-body edges and final
owned-gap edges. Keep the existing Sequence/Repeat-only temporal admission,
positive Hold duration, exact target ownership, no placed sounds, no
Retime/Partition, no Visual and duration-only Call expansion. Empty Sequences
and zero-length gap roots are not duration targets; a positive Hold within an
otherwise ordinary gap subtree is not such an empty definition.

No product decision is needed: spec §4.4 says no trailing gap, §5.2 distinguishes
default definition from sparse occurrences, §5.3 forbids silent anchor
retargeting, §7.4 requires atomic semantic macros, and §8/8.1 require editable
Hold parameter adjustment. REPEAT_GAP_BRANCHES.md already defines dormant final
gap definitions and explicit ownership.

## Exact existing APIs and blockers

Core:
- scoped_edit.rs::RepeatEditBranch and ScopedNodeTarget::{validate,matches_instance}
- scoped_edit/isolation.rs::validate_target already recognizes an effective body
  OR separately owned gap for Play. It isolates only an unowned body child.
- temporal_requirements / isolate_temporal reuse that validator. Outer body
  isolation copies nested gap_overrides plus nodes, clocks and local marks.
  An owned-gap edge itself requires no new clone.
- scoped_edit/mixed_hold_duration.rs::mixed_body_dormant is the scalar blocker:
  Play compares only effective body and explicitly rejects owned-gap ancestry.
- semantic/planner/selected_hold_duration.rs::body_dormant repeats that body-only
  rule with a shared parent index. Extend its edge proof without falling back to
  per-target full graph scans. uniform_level still requires distinct Plays of
  one Repeat under exactly equal captured preceding authoring prefix.
- occurrence.rs::InstancePath::validate intentionally accepts owned final gaps.
  This is OWNERSHIP validity, not liveness. AnchorIndex rejects a dormant gap.
- scoped_edit/hold_duration.rs::occurrence_hold_duration_requirements and
  occurrence_edit.rs::apply_captured already recognize owned gap ancestors.
  apply_captured skips isolation of the gap's own Play edge, but still isolates
  outer shared bodies.
- Command::SetScopedHoldDuration -> actual isolate_temporal ->
  ordinary Command::SetHoldDuration lifecycle remains correct architecture.
  Never use EditScopedMany or convert mixed presentation to all-Play.
- prepare_scoped_hold_duration must positively recognize final-gap dormancy:
  None presentation accepted only when dormant; actual proof maps authority;
  final boundary exists only for a live representative.
- selected planner keeps Compound leaf-by-leaf staging; no-op members validated
  then skipped; actual proof remaps ALL remaining targets and primary
  presentation; primary boundary recomputed after the whole batch.

Native:
- preview/scoped/model.rs::hold_duration_available explicitly rejects any
  gap_roots ancestor in its Default-containing route.
- selected_duration_target_path_supported contains the analogous selected body
  gate (read current source on implementation).
- project/scoped.rs::{semantic_mixed_hold_duration,
  semantic_selected_hold_duration} delegate liveness to the core and independently
  validate rendered primary first/last picture.
- Existing model/tests/mixed_duration.rs::
  mixed_duration_hints_refuse_owned_gaps_without_changing_all_play_gap_support
  contains the smallest truthful old native baseline and all-Play hint control.
- Existing projection reasons distinguish dormant default versus dormant gap.
  Availability should accept only a positively matching new gap-dormancy reason,
  never any missing presentation. Capture revalidates full authority.

## Timing and audio requirements

Reuse insert_time/hold_duration.rs::apply once AFTER isolation. For live changes,
occurrence_owners_after promotes the enclosing Repeat and captures compact
physical node and configured default-gap owners throughout all later
occurrences; it also covers directly nested Repeats. Every affected surviving
Source, RoomTone Hold, opaque processing sibling output and default gap keeps
its pre-edit physical clock. Existing binding, partial binding, birth and
AllocationEntry semantics must be preserved. Silence filtering is unchanged.
Shared outer Default multiplies only actual surviving default uses, excluding
owned outer body overrides. No expanded play loop.

The materialized gap Hold's Node binding can retain a lattice whose physical
history is a Repeat-gap recipe. gap_override::materialize closes its own
preceding iteration argument; outer Repeat arguments remain live. A duration
resize must retain that binding and phase, not rematerialize, clear it, or
replace it with a fresh default-gap clock. Distinguish this from a fresh
SetGapOverride subtree, which starts with its own current raw recipe.

Dormant final-gap resize changes the definition/provider fallback/Tail clamp
but not output extent, cursor, displayed picture or clocks. An outer unowned
Play may STILL need isolation to preserve authoring meaning before a dormant
edit. Existing ordinary lifecycle root-total-equal path should leave bindings
unchanged when no Retime ancestry exists; prove it in a regression.

A batch can contain live and dormant gaps and a no-op member. A dormant primary
keeps the ordinary cursor even if other live selected members ripple; all
target identities still map by actual Compound proof. No fabricated final-gap
boundary. The specification's stable preceding identity must survive reorder,
count growth, Undo/Redo and JSON.

No special root/beat sound allowance is proposed. Current wrappers explicitly
refuse these temporal cases through sound_events::validate_command, including
otherwise no-op unsupported members. Existing native all-Play support remains
unchanged for a live owned gap.

## Minimal implementation surface

Expected production changes:
- scoped_edit/mixed_hold_duration.rs: generalize body liveness to owned branch
  liveness; retain mixed-shape requirement and exact temporal machinery.
- semantic/planner/selected_hold_duration.rs: same edge proof using existing
  shared parents; generalize comments/error messages from body-only.
- scoped_edit/hold_duration.rs ONLY if executable tests expose all-Play dormant
  preparation in the selected route (see risk below).
- app preview/scoped/model.rs and current target-path helper: cached branch and
  nonfinal position proof, no graph scan on draw.
- app project/scoped.rs / service receipt only if dormant projection enum/reason
  handling needs it; no weakening of semantic_framing.
- focused core/native/PCM/store tests and docs.

Concrete follow-up correction from current source review:
Selected planner uses EditOccurrence for all-Play targets without invoking the
public prepared occurrence helper, BUT occurrence_edit/hold_duration.rs::prepare
also resolves the old Hold-end Anchor unconditionally for a changed target.
Therefore changed dormant all-Play members currently fail inside the reducer
before the final selected continuation query. Valid ownership alone is not
enough. Include that narrow reducer helper in core ownership: after retaining
existing ancestry admission, use the same positive dormant-path proof and skip
physical suffix capture for a dormant definition. Continue ordinary recipe,
provider, Tail, mark and lineage processing. Never fabricate a boundary, skip
unsupported ancestry validation or route all-Play through the mixed command.
The standalone prepare_occurrence_hold_duration still requires a live returned
boundary; its native scalar behavior can remain live-only. Mixed/selected
dormant semantics need no new standalone all-Play API.

## Qualification matrix, bounded

1. Scalar outer Default / inner Play gap: two live uses grow/shrink, preserved
   outer override, exact zero-pool edit at gap edge; stable representative.
2. Scalar outer Play gap containing inner Default: isolate outer body only when
   needed, exact sparse pool; owned gap and nested Hold alias map correctly.
3. Selected two/three gaps under same Repeat, distinct explicit preceding IDs;
   different old lengths to one duration, order [last,first,middle], final primary
   boundary rechecked; same original inner gap definition across selected outer
   Plays must not deduplicate by NodeId.
4. Live gap plus final dormant gap; final primary None, no fabricated Anchor;
   reordered stable final identity changes liveness; dormant default above live
   inner gap; outer isolation still preserves dormant authoring meaning.
5. Materialized gap Node binding born from default-gap lattice, partly bound
   suffix, 44.1k natural audio / NTSC; old gap entry phase retained after current
   duration and moved later Source/gap entries retain old samples. Compare
   analytical authored ranges without event alignment; chunk independence.
6. All-owned no extra allocation, local + occurrence mark mapping/loss, accepted
   generated Hold shrink versus extension fallback, store durable history and
   pending generation mapping.
7. No-op subset/full; stale scope, retired IDs, wrong gap root, default-gap fake
   target, omitted ancestry, sound/Retime/Partition member; late failure after
   valid earlier leaf leaves doc/bank/history exact. Million-play sparse test
   covers graph-bound timing and no per-play expansion.

Existing source evidence is architecture, not an executed proof of the new
family. Retain old executable baseline first, then core/PCM/durable/native
verification and final real-media preview/export.
