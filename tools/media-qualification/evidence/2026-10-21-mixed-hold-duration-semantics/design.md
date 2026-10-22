# Mixed Default/Play Hold duration: proposed next slice

Status: source analysis only. No implementation or runtime claims.
Current shared-duration source remains frozen.

## Recommendation

Add one temporal scoped-duration wrapper that preserves an explicit
ScopedNodeTarget. Do not reconstruct an all-Play target from a representative,
add duration to value-only ScopedNodeEdit, or flatten the Repeat.

Initial mixed family: one target, at least one Default and one Play step,
ordinary Sequence/Repeat ancestry without Retime/Partition. Keep current ordinary,
all-Default and all-Play commands/admission unchanged. Reuse their existing saved
SetHoldDuration { length } intent. A later increment may unify lowering after
equivalence is proved; unification is not needed for this feature.

There is no unresolved product decision. Default/Play authoring meaning is
already normative in spec 5.2. Resource limits and unsupported temporal clocks
must continue to fail truthfully.

## Existing exact machinery

- scoped_edit.rs: ScopedNodeTarget::validate and matches_instance.
- scoped_edit/isolation.rs:
  temporal_requirements(document,target) -> ScopedEditRequirements;
  isolate_temporal(document,target,identities,EditContext)
    -> (ProjectDocument, mapped ScopedNodeTarget, Vec<IsolationMap>).
  Only a Play step whose effective body is still the literal default clones.
  Default steps never clone; existing owned bodies/gaps stay owned.
  Nodes are consumed in structural preorder. Local/source marks consume their
  exact current ID-order pool, with additional copies for nested clones;
  occurrence marks relocate instead. Freshness includes retained audio IDs.
- scoped_edit/proof.rs: ScopedIsolationRecord::from_execution, bind_prepared;
  derive_scoped_isolation replays production; ValidatedScopedIsolation maps
  explicit Default/Play prefixes without aliasing other authoring scopes.
- occurrence_edit.rs::clone_nodes already clones authored nodes, sparse nested
  overrides, audio lineage/bindings and retained sound aliases without moving
  root time.
- insert_time/hold_duration.rs::apply is the correct duration implementation
  AFTER isolation: it computes actual aggregate root change, captures old
  physical allocations compactly and calls the ordinary reducer, lineage and
  mark transforms. It retains unbound/partial/bound Source, RoomTone, Repeat-gap
  and owned-branch clocks. It retains generated fallback and Tail clamping.
- command.rs::apply_with_isolation collects actual executed isolation steps.
- insert_time/occurrence.rs::prepare is an existing pre-lifecycle resolution
  pattern: isolate structurally first, recurse on a mapped concrete command,
  then produce one net transaction against the original input.
- scoped_edit/hold_duration.rs::prepare_occurrence_hold_duration is an output
  shape exemplar, but its all-Play conversion and single-occurrence preparation
  cannot be used for mixed authority.
- occurrence_edit/hold_duration.rs::prepare captures one root suffix beginning
  at one selected Hold end. That is not the shared mixed algorithm.

## Proposed public boundary, for root/API agreement

Suggested command name, not yet agreed:
Command::SetScopedHoldDuration {
    target: ScopedNodeTarget,
    duration: FrameDuration,
    identities: OccurrenceIdentities,
}

The command needs authoring authority, not a sampled presentation. A concrete
presentation is invocation/continuation evidence, separately supplied to planning.
This avoids making dormant authoring depend on an invented iteration.

ProjectDocument::scoped_hold_duration_requirements(
    &self, target: &ScopedNodeTarget, duration: FrameDuration
) -> Result<ScopedEditRequirements, EditError>

Check target/Hold, positive changed duration, supported ancestry and command sound
policy BEFORE querying isolation requirements or requesting identities.

prepare_scoped_hold_duration(
    document: &ProjectDocument,
    request: &CommandRequest,
    presentation: Option<&InstancePath>,
) -> Result<PreparedScopedHoldDuration, EditError>

PreparedScopedHoldDuration {
    transaction: EditTransaction,
    document: ProjectDocument,
    target: ScopedNodeTarget,
    presentation: Option<InstancePath>,
    boundary: Option<ResolvedBoundary>,
    isolation: ScopedIsolationRecord, // private, accessor + validated mapping
}

This method must execute the actual production wrapper with apply_with_isolation,
bind its exact before/request/after record, map the supplied target and optional
presentation, then resolve the final Hold local-zero AnchorIndex boundary.
Use actual mapping steps, not fabricated clone maps or old-cursor sampling.
Expose map_target/map_instance like existing prepared results, revision-bound.

Semantic invocation should add a distinct duration-only context or generalize
the dedicated shared-duration context only by an explicit agreed API change.
Do not broaden framing::validate_scoped_context, which protects every other
instruction. Retain the inspector root, ordinary parent/root selection, optional
presentation and authoring branches. Permit SetHoldDuration/Call only.

## Production sequencing and clocks

1. Validate original authority and exact pool lengths/freshness. A no-op refuses
   before cloning or allocation.
2. Call isolate_temporal against the old structure; retain every returned
   IsolationMap::proof in execution order. Isolation itself preserves time.
3. Lower to plain Command::SetHoldDuration on the mapped target node.
4. Enter the established ordinary command lifecycle on the isolated document.
   This invokes insert_time::hold_duration::apply exactly once for physical
   capture/resize. Do not also call occurrence_edit duration preparation.
5. Preserve the complete old timing layout at leaf revision ordinal0 when a
   physical capture/sound journal requires it. Isolation introduces no new timing
   allocation, so there is no ordinal collision or second capture.
6. Return the net transaction against the original input and the actual proof.
   Compound replay and derive_scoped_isolation must observe the same steps.

The current occurrence pause preprocessor is one model. A new duration
preprocessor should be narrow, not a general command-rewrite framework. Do not
skip the wrapper when pools are empty: an already-owned mixed target still needs
its authoritative raw-duration lowering and proof output.

Initial sound admission should preserve current repeated-duration refusal for
external root/beat sounds unless a separate complete transport is implemented.
Do not silently treat the wrapper as a value edit or claim the shared raw setter's
attached-sound qualification automatically covers clone-plus-edit. Existing
all-Default setter routing must retain its current sound behavior.

## Authoring meaning and resource examples

Use outer3, inner2, body [Source2, Hold2, Source2], independently owned inner
play1 [Source2, Hold4, Source2], independently owned outer play2
[Source2, Hold7, Source2], lead3/tail3. Initial root duration45.

Outer Play1 / inner Default:
- Isolate outer play1 if unowned. Clone its entire effective body (inner Repeat,
  literal Body4 nodes, owned inner Body4 nodes): 9 node IDs in this fixture.
- Keep mapped inner branch Default. Its one unoverridden inner play grows.
- Hold2->3 changes root by1, not2 or3; outer play0 stays on old literal Hold2.
- Existing inner-owned Hold4 is copied structurally but remains unchanged.

Outer Default / inner Play0:
- Outer Default stays shared. Isolate inner play0 only: Body Sequence plus three
  physical children, 4 node IDs in this fixture.
- New inner override is reused by both live outer default plays.
- Hold2->3 changes root by2, not1 or3; independent outer play2 remains unchanged.
- Literal Hold2 remains authored, now dormant within the inner default.

Counts above exclude marks. Exact public requirements, not these constants,
authoritatively determine pools. A selected Play that already owns its branch
needs zero new IDs at that level. Pool size depends on authored copied subtrees,
never rendered Repeat count.

## Dormant mixed scopes

No-Retime body-path liveness is a conjunction:
- Default body step: at least one current nonoverridden body iteration.
- Play body step: the named stable iteration resolves to that exact effective
  body (validated by target.validate); its body is live locally.
Any dormant Default ancestor suppresses the entire descendant target, including
a later Play. No play enumeration or arbitrary representative is necessary.

Outer Play / dormant inner Default still needs outer isolation if that Play is
unowned: otherwise a definition-only edit would mutate other outer plays'
authored defaults. It creates the proper scoped authoring change but no output
time. Outer dormant Default / inner Play may likewise need inner isolation even
though the entire default family currently renders nowhere.

None presentation is accepted only with positive liveness proof; Some must match
a live actual instance. After isolation map the target, then prove dormancy again.
Dormant continuation preserves cursor and rendered root duration.

Owned gaps require an explicit decision about feature scope, not guessed body
liveness. Default steps cannot traverse owned gaps. A Play step can name one:
it renders only if its stable preceding play is nonfinal and the actual owned
gap subtree has positive extent. Its own extent, not default gap duration, is
authoritative. This is compactly provable with current IterationOrder/RepeatLayout.
Recommendation: include already-owned gaps with the exact active/final rules,
matching existing temporal target ownership, and add final-gap dormancy tests.
This needs no materialization or new gap recipe. An implicit default gap has no
Hold NodeId to resize and remains outside this operation. If implementation is
split by engineering scope, body-only admission must explicitly reject owned-gap
ancestry rather than accidentally treating final gaps as live.

## Store/preparation integration is required

A new wrapper must be recognized by:
- generation_scope/transitions.rs::may_isolate, so requests/preparations follow
  the actual command proof in forward and inverse history.
- generation_preparations/transitions.rs::duration_node and has_birth.
- leaf_births currently maps only EditOccurrence's synthesized all-Play address.
  Map the new command's supplied mixed target through actual proof instead.
- Other duration/provider/exhaustive command classifiers and frozen history
  vocabulary as required by the existing command format policy.

An accepted generated Hold extending beyond accepted output must create the
fallback/preparation for the mapped isolated Hold, not the original shared leaf.
Shortening within accepted support preserves the accepted provider. A pending
request must follow the mixed authoring address and remain revision-bound.
Do not infer this from wrapper recognition alone.

## Verification proposal

First retain an old native unchanged-API Hold-parameter regression with a raw
positive prefix as root did for the prior slices. New wrapper types cannot be
used to compile an old-code baseline.

Core:
- Independent exact root-delta examples above, both first/last live
  representatives, grow/shrink, existing-owned and initially-shared Play paths.
- Exact node/mark pools, too few/many/reused IDs, no-op before allocation.
- Map target, sibling targets and instance paths forward/backward; Default
  counterparts outside a selected Play must not remap, while an inner Play
  under Default must map in every concrete default outer presentation.
- Literal defaults, owned body/gap branches, marks and inverse/JSON exact.
- Dormant Default before/after a Play; correct required isolation despite no
  output; preserve entire clocks and cursor.
- Nested counted/named duration calls requery after prior isolation/captures;
  later no-op/unsupported instruction fails atomically.
- Million plays with sparse overrides; metadata/pools proportional to graph.
- Keep Retime/Partition and unsupported sound policies explicit.

PCM independent oracle:
- Natural 44.1k sources at nonzero source offset and NTSC project rate, RoomTone
  Hold plus default/owned gaps, existing captured fragments and partial bindings.
- Assert each surviving allocation's old literal source phase at its NEW exact
  origin. Shared duration changes produce multiple cuts, not one global shift.
- Compare Source after each affected Hold and later untouched/owned branches;
  later resized Hold occurrences retain their own old entry phase.
- Include direct Repeat->Repeat ancestry, first/last representative, growth and
  shortening, bounded block/chunk independence. No event/sample alignment.
- Unchanged overrides retain recipes but may shift in root time; compare their
  transported phase, not equality of absolute placement.

Store/native:
- Accepted generated shortening/extension mapping and fallback/preparation birth,
  preview/commit/dry-run/reopen/Undo/Redo, late compound failure with no writes.
- Native exact captured authority, root selection/cursor, stale and delayed
  command entry, actual picture-provider presentation, recording/Call/dot.
