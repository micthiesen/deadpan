# Ordinary Hold resize on processing-input clocks

2026-10-20 proposal only. Repository frozen; no Cargo, runtime witness or implementation executed by this scout. Builds on /tmp/deadpan-retime-hold-clocks-2026-10-20/processing-next.md.

## Scope and invariant

Support ordinary, initially unclocked SetHoldDuration whose enclosing Preserve/Shift output/window stays fixed. Transport each affected physical recipe on the clock it actually consumed before the edit. Keep each processing stage's timed recipe, selected input window, output extent and position fixed. Child clocks move within those windows; unrelated outer allocations stay put. Do not use the dedicated inside_clocked duration path, which intentionally expands stage extents/windows and has separate admission limits.

Existing AudioTimingId, FrozenAudioLayout, AudioPlacementTemplate, AudioClockRoot::PreserveInputPointCeil and AudioReanchorStep suffice. No new serialized primitive or provider resolver.

## Precise narrow capture API

In `audio_binding_lifecycle.rs`, replace the private boolean `collect_root_placements` with an internal selector concept:

    enum PlacementCapture<'a> {
        None,
        Root,
        RecipeClocks(&'a BTreeSet<NodeId>),
    }

This is private execution configuration, never persisted. Existing callers map false→None / true→Root, preserving their behavior exactly. Add one crate-private entry:

    capture_for_hold_duration_edit(
        document: &ProjectDocument,
        timing: AudioTimingId,
        affected: &BTreeSet<NodeId>,
    ) -> Result<CompositeInsertionCapture, DocumentError>

It uses `capture_with_placements(... retain_timing=true,
PlacementCapture::RecipeClocks(affected), scope=Some(LayoutScope {
    aliases: empty, placement_owners: Some(affected)
}))`.

The existing internal result container already holds state, optional phase-only old layout and node/gap placement maps. Its name is insertion-specific but its representation is suitable; no public rename is necessary.

`Capture::capture_recipe` decides whether to collect a CURRENT placement separately from whether to install a fresh lattice:

- None: no movement placements.
- Root: current existing rule, only `clock==ProjectRootRoundEven`.
- RecipeClocks(affected): requested physical owner, on its current `self.clock`, including ProjectRootRoundEven and PreserveInputPointCeil.

Capture::walk already assigns clock roots correctly. A processing output itself is captured on its enclosing clock BEFORE descending. Descendant recipes switch to that stage's input PointCeil root, with outside Repeat arguments/births reset and restored on exit. A nested processing output therefore belongs to the outer input grid; its own descendants belong to the nested input grid. Store exactly those existing placement templates, without conversion to root coordinates.

Selecting only affected CURRENT placements avoids charging extra movement records for every unrelated input recipe. Existing unbound lattice inventory stays global, as it already is for ordinary capture. State budgets and wire/work limits stay unchanged.

## Preparation/reanchor API

Add a narrow internal helper in `insert_time/composite.rs` (or private to hold_duration.rs if sharing is not useful):

    prepare_hold_duration_owners(
        document: &ProjectDocument,
        affected: &BTreeSet<NodeId>,
        old_root_total: i64,
        timing: &AudioTimingId,
    ) -> Result<ProjectDocument, EditError>

It mirrors prepare_owners' established lifecycle using the NEW capture function: install old state/table, account existing entries, append, prune, rescope_timing and validate_for on the PRE-EDIT document. The caller installs only the returned binding state onto its unpublished reduced candidate, then existing lineage/marks/final validation runs.

Reuse `append_anchored_steps` with AllocationEntry. Split captured node/gap maps by template.reference.root:

- ProjectRootRoundEven: existing full old root window `[0,old_root_total)`.
- PreserveInputPointCeil: `window=None`. The saved template already brands the clock. `AudioReanchorStep::allocation_entry` calls clock_scope, projects in that stage's CHILD scope, and applies that scope's selected input support. A ROOT-frame window must never be passed here.

This requires no change to persisted step evaluation. PointCeil scope uses grid_origin equal to the stage's captured selected input start and selected support[start,end). Nested affine Retime crops are applied by FrozenAudioLayout's visible-allocation projection inside that scope. Repeat arguments remain compact and domain-specific.

A common private finalization helper can avoid duplicating prepare_owners' entry accounting/prune/rescope/validate sequence, but do not alter old insertion collectors/call-site policy as part of this slice.

Retain current `with_scoped_timing_captures(false)` around any physical preparation that shares ordinal0 with attached-sound journal restoration. The timing identity must still identify the same complete old layout. Do not filter journal capture or allowances with the physical selector.

## Duration-specific owner collection

Extend only the duration-specific collector; existing insertion/occurrence collectors retain their stricter contracts.

- Keep Sequence later-sibling collection and Repeat whole-interior promotion while local duration ripple is live.
- At the FIRST Retime ancestor, stop propagation of movement into outer siblings/plays, regardless of affine/processing policy: raw SetHoldDuration changes neither this Retime's output extent nor selected input window.
- Keep pending INTERIOR owners. Their nearest physical clock is supplied by the capture walk, not inferred by the collector.
- Continue inspecting full ancestry for correctness/limit accounting and dormant default proof. Processing ancestors no longer cause an early None once all-clock capture is qualified.
- Expanding pending roots keeps `repeat_interior_owners`' existing stop BELOW a processing sibling: only that sibling's OUTPUT moved, not its intrinsic input positions. A processing ancestor containing the changed Hold is not itself a moved output and must not be promoted just to reach its descendants.
- Preserve the validated all-overridden proof (`child==default && overrides.len()==plays`) anywhere on the path, including above the first fixed window. Return no capture for a dormant branch; no expanded play iteration.
- Preserve the current silent-Hold/default-gap physical filter and every older skipped binding. Sources remain conservative; a processing sibling remains an opaque conservative physical output.

No-op duration stays outside preparation. Empty physical suffix stays outside preparation. Invalid shorter children still fail selected-window admission; never shrink/rewrite the Retime window to admit them.

## Processing OUTPUT bindings

Preferred rule: retain pre-existing ancestor output bindings unchanged. Do not treat this as SetRetime and blanket-remove them.

Reason: the processing recipe (duration/rate/pitch/selection), output grid and position are unchanged; only current input content changes. `audio_bound.rs::raw_domain` builds a definition from the LIVE plan, bypassing only the output's own binding, while descendants retain their bindings. `intrinsic_stage` likewise uses the current owned stage. Historical timing tables do not contain PCM/old recipes to substitute as media. `StageAudio::prepare_stage` keys preparation by descriptor including current revision, so a changed operand cannot reuse a prior revision's prepared samples. `audio_lineage::reconcile_shared` invalidates changed raw-content ancestry normally.

Thus preserve an old moved output's saved phase/resume while rebuilding DSP from corrected current input. This differs from SetRetime, which changes the processing recipe and explicitly removes its own output binding. Do not weaken that existing path.

Global inventory may add an initial lattice for a previously unbound ancestor output; with unchanged output geometry that freezes the current clock without reanchoring it. Tests must distinguish new lattice capture from output movement.

Required proof before accepting this rule: old output with fractional resume (use retime_edits.rs::bound_document pattern), changed inner Hold, independent corrected input+canonical DSP, then apply the unchanged output map. Assert old output binding exactly equal and no output reanchor. If a real witness disproves equivalence, design a specific output-map transform rather than discarding history reflexively.

## Fresh versus partially/historically bound input

Fresh input Source/RoomTone/gap gets old PointCeil lattice and an allocation-entry reanchor if needed. Simple unbound local0 may compact as inert; the old lattice alone fixes entry phase.

Existing input bindings retain lattice, resume and chronological reanchors. Append a step from the NEW capture's pre-edit current placement, not from the old lattice. This is essential after a prior move/split/crop, where the current visible entry differs from an older resume. A hidden old allocation appends no effective anchor; newly visible content uses retained history and CURRENT finite support, not fabricated old PCM or an assumed whole-old-prefix equality.

PointCeil selected start may be nonzero. Independent sample labels must use ceil((frame-selected_start)/frames_per_sample), not root RoundEven or a rounded duration. `clock_scope` already implements that branding and support.

Nested Preserve/Shift inputs stay distinct clocks. A moved nested processor OUTPUT on an outer input grid is captured/reanchored there; its intrinsic input recipes are not root-moved. Never flatten the entire processing subtree into the outer clock.

## Minimal failure/proof matrix

1. Direct unbound Preserve: Body[lead3,Hold2,Source4], mapping0..8/output4, natural44.1k Source3100. Input old8008→new9610 on Hold2→3; independent first256 physical input samples must retain phase3100, not3100+147/400. Root duration unchanged.
2. Unity-rate Shift+3, mapping0..8/output8: same input witness, actual processor invoked. Run both grow and valid shrink/exposure with explicitly current finite input support.
3. Processing ABOVE affine window: inner output6/map0..8, processor output3/map0..6. Old Source input15/4→6006, new9/2→PointCeil7208, step49/40; wrong live phase+49/50. This catches root-grid substitution and nearest-ancestor-only checks.
4. Existing input binding + partial capture, nonzero selected start, prior clipped/resumed Source; second resize. Assert old lattice/resume-prefix preserved and current-entry step correctly composes. Include a prebound/moved processor output with fractional resume as above.
5. Repeat default/owned/gap cases inside processing input and outside fixed stage, following processing sibling on outer input clock, and one nested processor. Outside outputs/gaps do not move; appropriate input/output clock ownership remains explicit. Huge compact counts + >256 dormant default resizes retain bounded graph/state.
6. No-op and invalid fixed window refuse/skip as before; full exact inverse/JSON, store preview/commit/reopen/Undo/Redo, genuine late Compound refusal and exact sound-journal timing identity. Reuse existing generated fallback/Tail/inside_clocked checks rather than adding unrelated behavior.

Use input_signal→typed one-run AudioSignalTape→StageAudio::read_tape for the first physical witness. Full-stage PCM should compare independently reconstructed CURRENT finite input processed with the qualified exact rate/pitch recipe; changing a Hold legitimately changes DSP neighborhoods, so no aligned old-output-prefix oracle. Keep full crop/filter and suppression semantics explicit.

## Deferred distinct work

Do not silently route documents already using derived clocks/PreserveInputProjection through this mode. Their dedicated dispatcher and owned projection completeness are separate semantics. Likewise general speed-stage updates to an exact-clock owner remain a different DP-07 gap. This proposal does not justify adding new refusals or claiming those cases implemented.
