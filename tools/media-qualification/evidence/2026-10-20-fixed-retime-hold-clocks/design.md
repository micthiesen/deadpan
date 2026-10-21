# Fixed-window FollowSpeed Hold-duration capture design

2026-10-20, read-only. No repository writes, Cargo or PCM execution. Current release remains frozen. This is a bounded proposed design, not passing evidence.

## Answer

`insert_time::composite::prepare_owners` already has the coordinate machinery required for physical owners below ordinary, unclocked FollowSpeed Retimes, including nonzero and fixed input windows. It does not assume a common integer suffix translation. Its owner set is a selector for reanchor steps, and its window is in historical ROOT frames. The missing piece is duration-specific affected-owner collection and triggering, not a new clock format.

Qualify this first for ancestry containing only affine stages (`!pitch.processes(mapping.duration()==duration)`) and no derived clocks. A processing Preserve/Shift ancestor is a separate clock boundary; existing capture intentionally does not collect project-root movement placements for its descendants. Do not silently claim that path is handled merely because binding capture succeeds.

## Exact existing APIs and proof

1. `composite::prepare_owners(document, affected: &BTreeSet<NodeId>, at: ProjectFrame, total: i64, timing: &AudioTimingId)` captures BEFORE mutation. Use `[0,old_root_total)` as existing ordinary Hold-duration path does; do not pass child-local cut/duration as root coordinates.
2. It calls `audio_binding_lifecycle::capture_for_composite_insertion_scoped`, obtaining state plus current node/gap `AudioPlacementTemplate`s independently from historical lattice/resume bindings. Existing lattice/resume expressions are retained; initially unbound physical recipes acquire the OLD lattice.
3. `Capture::walk` keeps `AudioClockRoot::ProjectRootRoundEven` through FollowSpeed/unity affine Retimes. `capture_recipe` records root placements for those owners even if already bound. Repeat templates use live/captured iteration arguments and births, not expanded plays.
4. FrozenAudioLayout captures Retime recipe geometry and derives exact projection. `audio_reference.rs::project_scoped_inner` uses `factor = extent/(child_end-child_start)`, `origin=(origin-child_start)*factor`, `scale*=factor`. Visible-allocation projection clips support through every Retime input window, including Partitions. Thus a sibling shift delta in child frames becomes delta times the product of enclosing output/input slopes, with exact rational origins.
5. `prepare_owners` appends an AllocationEntry `AudioReanchorStep` against the OLD layout for each affected physical node/gap. `AudioReanchorStep::allocation_entry` obtains the captured visible entry, intersects the old root window, and keeps old local coordinates. No old visible allocation means no new anchor. Existing chronological resume/phase expressions compose rather than reset.
6. `deadpan-plan::audio_bound::bound_at` resolves the old lattice plus resume; anchors it at the CURRENT live local boundary, and uses current raw recipe/support. It calculates current-boundary rounding independently. This transports old phase through a changed placement while respecting the current Retime crop. Old/new sample interval lengths need not agree, and no common rounded suffix delta is assumed.
7. `reanchor_step_is_inert` can omit a provably zero-contribution non-Repeat step; bindings still capture the old lattice. Repeat steps are retained conservatively.

Therefore the prepared bindings can be installed on the already reduced candidate just as in current `hold_duration::apply`, provided (a) the physical policy and affine ancestor maps are unchanged, and (b) owners are correctly chosen. The Hold duration changes, but a following owner's raw recipe and every affine stage's duration/mapping/pitch remain unchanged.

## Bounded affected-owner collection

Do not loosen `occurrence_owners_after` for all callers: occurrence pause/insertion has different ancestry requirements and already rejects Retime mutation intentionally. Use a private duration-specific collector (or an explicit internal mode used ONLY by ordinary SetHoldDuration).

Preflight the target's complete ancestor path once:

- Existing dedicated `inside_clocked` dispatch remains first and unchanged.
- For the new fixed-window branch, verify no processing Retime/Shift stage appears ANYWHERE above the Hold, including above the nearest affine Retime. Stopping collection at the nearest stage alone would miss an enclosing Preserve clock boundary.
- Build/retain a bounded parent index if useful; never enumerate Repeat plays. Preserve all current command validation/no-op/provider fallback/Tail clamp behavior.

Collect with `child = target`, pending disjoint subtree roots, walking outward:

- Sequence while ripple is live: append siblings strictly AFTER child. Earlier siblings and the current Hold do not move in this concrete parent use.
- Repeat while ripple is live: replace pending with that entire Repeat root, as the existing collector does. Raw shared Hold edits can change default/owned branch duration and thereby move later plays, gaps and later appearances of the edited Hold. Whole-Repeat physical collection + compact arguments conservatively covers them, including direct Repeat→Repeat ancestry.
- First Retime: its raw output duration and selected input window are fixed by SetHoldDuration. Keep pending interior roots, but STOP propagating the ripple. Do not append this Retime ancestor as a moved output, outer Sequence siblings or outer Repeat interiors. Their output allocations did not move.
- Root reached without Retime: existing root-ripple behavior, unchanged.

Expand pending roots with existing `repeat_interior_owners`. This descends Sequence/Repeat/default/owned gap branches and affine Retimes, but stops BELOW a processing Preserve/Shift stage. A processing stage that is a FOLLOWING SIBLING is legitimately moved as one opaque physical output, so keep that owner, not its intrinsic children. A processing stage on the TARGET ANCESTOR PATH is a different case and is excluded from the initial new qualification above.

Retain current physical filtering: Source conservative, audible Hold/default gap conservative, affine Retime/Sequence themselves contribute no binding. Keep older silent bindings intact. Do not unconditionally collect outer siblings just because they appear later in root order.

Trigger preparation from nonempty affected physical owners after a real duration change, even if root totals match. Avoid changing no-op/terminal/no-live-effect behavior. Fixed-window crops can hide all affected output; keeping compact dormant clocks conservatively is valid but needs an explicit no-live-output test and bounded-state assertion rather than inventing a per-play visibility walk.

Complexity stays proportional to owned graph + compact Repeat segments + existing binding terms. Parent depth is bounded, sets deduplicate physical owners, no play-count expansion. Existing work/term/wire budgets remain unchanged.

## Preserve and unchanged sibling details

- Unchanged outer siblings must receive no new reanchor steps. Capture may still create their first lattice because the existing capture primitive inventories ALL unbound physical recipes; that freezes their current phase without moving it. Distinguish binding creation from movement/reanchoring when asserting metadata.
- A following Preserve sibling inside the edited affine window moves as one existing processing output. Its captured output gets a root reanchor; its descendants retain their PreserveInputPointCeil lattices and receive no root movement steps. This is the same supported opaque-suffix behavior as ordinary insertion.
- A Preserve ancestor containing the resized Hold changes its processing OPERAND. `capture_recipe` switches descendant clock root to PreserveInputPointCeil and excludes those descendants from root placements. Current prepare_owners cannot by itself promise the needed input-edit semantics. Do not reset the stage output binding, graft a root reanchor onto point-clock descendants, or call unchanged output 'preserved PCM'. Keep this as a separate explicit scope decision/refusal, not hidden support.
- Any attached sound capture must retain the complete old timing layout when physical bindings and sound journal use the same allocation/ordinal0. Preserve the current `with_scoped_timing_captures(false)` guard wherever a physical capture is performed. Do not filter sound journal capture or allowances with the physical owner filter.

## Required proving tests before accepting design

1. Minimized fixed-window witness in scout.md: lead3 + FollowSpeed unity Retime4 over [Hold2,Source2], Hold2→3. OldSource starts root5/B8008, new root6/B9610, root duration7 unchanged. Independent first256 samples at literal source3100; new sample phase must retain that oracle, not reset to3100+147/400. Compare full surviving1601samples without alignment, plus exact inverse and chunk independence.
2. Shrink with sufficiently long child suffix that mapping remains valid, producing exposure/clipping separately from surviving support. Out-of-range mapping shrink must still refuse atomically.
3. Nonzero selected Retime window and genuine nonunity FollowSpeed slope. Compute old/new physical origins and project each sample boundary exactly; never multiply rounded duration or assume a common rounded delta.
4. Nested affine stages, Repeat below fixed stage (default+owned) and Repeat above fixed stage. Later internal plays/gaps move; outer sibling/next stage unchanged. Compare old clocks/bound PCM and bound state size for a huge compact Repeat count.
5. Following opaque Preserve sibling: moved output only; retained input bindings unchanged, existing independent canonical-processing oracle. Processing ancestor must retain explicit unsupported behavior unless separately implemented/qualified.
6. Existing partially bound Source/RoomTone/gap resumes, zero-local/cropped old entries, full history inverse; no double capture in EditOccurrence or inside_clocked path.

No new primitive/schema/public API is needed for the first affine fixed-window branch. The algorithm is a proposal until the minimized old-code witness and these coordinate checks run.
