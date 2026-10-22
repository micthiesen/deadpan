# Ordinary inspected Hold-duration authority

Status: agreed public API; repository production unchanged pending root's native baseline pin.

## Contract

Add SemanticContext.hold_duration: Option<SemanticHoldDurationContext>.

Public SemanticHoldDurationContext derives Debug, Clone, PartialEq, Eq, Serialize,
Deserialize, with deny_unknown_fields:
- root: NodeId
- target: NodeId
- presentation: InstancePath

Public HoldDurationVisibility derives Debug, Clone, PartialEq, Eq:
- exact: ExactFrameRange
- frames: FrameRange

ProjectDocument::ordinary_hold_duration_visibility(
    &self,
    parent: &NodeId,
    authority: &SemanticHoldDurationContext,
) -> Result<HoldDurationVisibility, EditError>

The pure query validates the document and ordinary Sequence parent. The inspector
root must be its direct child and an actual fixed unclocked Edit Retime. Target
must equal presentation.node and be a Hold. presentation.repeats must be empty.
Every ancestor from the target to inspector root must be a Sequence or fixed
unclocked Edit Retime. Ordinary parent and its ancestors remain ordinary Sequence
scopes. A clocked containing stage, Repeat, Partition, wrong owner/root, missing
node, hidden target, or empty picture-sampled span refuses SelectionUnavailable.
Pitch policy alone is not a refusal; Preserve/Shift use the same picture geometry.
Existing reducer/provider/sound policy failures remain authoritative at execution.

The semantic validator additionally requires no scoped, Original or Visual
authority and selected_child == Some(root). Cursor need only satisfy the existing
ordinary scope bounds, including an invocation interior to the Hold. It is not
sampled to rediscover or replace target identity.

## Exact bounded geometry

Build a parent index once from the validated document's compact graph, without
recursive traversal or expanded Repeat plays. Obtain current exact node extents
once. Ascend from Hold interval [0, authored Hold duration):
- Sequence parent: add the exact sum of preceding sibling extents.
- Edit Retime parent: intersect with its fixed selected input [mapping.start,
  mapping.end), then map x to (x - mapping.start) * output_duration /
  mapping.duration. Preserve the empty-intersection outcome.
- Reject any other traversed ancestor.
Continue through root's ordinary Sequence ancestry to absolute project space.
Bound all traversal and checked rational arithmetic by existing node/depth limits.
No FrozenAudioLayout projection: its meaningful/allocation APIs intentionally
reject crossing nonunity Preserve processing, whereas this query is picture
geometry and must admit those stages.

Picture centers f + 1/2 own half-open exact intervals. Derive sampled endpoints
independently as ceil(exact_endpoint - 1/2), clamp to [0, document_frames].
A center tied at interval In is included; a center tied at Out is excluded.
Reject start >= end, rather than fabricating a nearby presentation.

## Planner integration

A focused planner/hold_duration.rs contains query/types or validation helper and
the duration-only semantic implementation. Keep framing::validate_scoped_context
and existing This-play/Retime refusal unchanged.

Initial validate_context checks the new authority before any allocator call.
execute permits only SetHoldDuration and Call for this authority. Expanded named
and counted calls therefore remain duration-only; unsupported later instructions
refuse the entire immutable plan without committing or writing registers.

set_hold_duration branches to the new authority before direct-child selection.
Resolve requested length against staged rate; reject same duration before allocation.
Request ParameterEdit, reserve its fresh revision and author ordinary
Command::SetHoldDuration { node: authority.target, duration }.
After commit_leaf updates the staged document, recompute visibility for the same
identity. Reject if now invisible or unsampled. Continue at frames.start, preserve
context.parent, context.selected_child (the inspector root), presentation and
authority, then validate the complete context. Trace identifies actual Hold with
its actual parent and current sampled visible range, never intrinsic duration
added to a root frame.

Ordinary plain SetHoldDuration without this authority and the existing scoped
EditOccurrence duration path retain their behavior. No new command, saved intent,
schema, identities, or occurrence isolation resource.

## Focused tests proposed

1. Grow/shrink under FollowSpeed, Preserve and Shift with exact fractional In:
   root lead4 + stage4 mapping1..8 over before3/Hold3/after3 + tail.
   Old visible [36/7,48/7), sampled [5,7), invocation6; shrink1 gives
   [36/7,40/7), sampled [5,6). Preserve stage recipe and outer neighbors;
   ParameterEdit only; exact request replay/inverse/JSON.
2. Crop starts inside Hold: stage4 mapping4..8; old Hold3..6 visible [4,6).
   Resize to1 makes intersection empty and must fail atomically after staged apply.
3. Unsampled initial Hold: stage1 mapping0..10, target input1..2; [1/10,1/5)
   contains no project picture center, reject before allocation.
4. Half-center ties: input Hold1 at [1,2) under stage1 mapping0..2 admits
   picture0 from [.5,1); target [0,1) maps [0,.5) and is unsampled.
5. Nested fixed windows and Sequence groups: compare exact composed clipped
   interval, first visible sampled continuation, arbitrary interior cursor.
6. Counted nested named duration programs (alternating lengths), staged query
   recomputation, late identical-length no-op and late Zoom/pause/navigation refusal;
   original document/bank stay exact and no request is published.
7. Malformed authority: simultaneous scoped/Original/Visual, wrong parent/root,
   target/presentation mismatch, Repeat ancestry, Partition, derived-clock stage,
   missing target; refuse before allocation.
8. Existing ordinary and This-play duration tests continue unchanged.

## Ownership / exclusions

Core semantic model/planner, new pure visibility query, core tests after root go.
Root updates external CLI SemanticContext constructors/protocol fields; app worker
owns native capture, receipt, recording/dot and inspector gates. No normal-only
bypass, no broad framing authority, no Repeat-under-Retime support, no new sound
lifecycle or generated-provider behavior.
