# Proposed old native baseline

Source-only proposal. Do not claim a run. Existing test helpers are in
crates/deadpan-app/src/preview/scoped/model/tests/mixed_duration.rs and imported
from the scoped model test parent.

## Smallest witness

Strengthen the existing
mixed_duration_hints_refuse_owned_gaps_without_changing_all_play_gap_support
fixture without changing production:

root Sequence [outer]
outer Repeat: inner, two stable plays, no default gap
inner Repeat: body Hold2, two stable plays, default gap Hold1
inner.gap_overrides[first actual inner iteration] = owned-gap Hold3

Use actual IterationOrder IDs from document, not nominal ordinal constructors
for any newly inserted public fixture. This existing helper's freshly authored
IDs are known source data and are sufficient for the minimal unit witness.

Positive prefix:
- validate document and exact root duration14.
- owned gap Node is independent of inner default gap recipe1.
- State root outer cursor2; enter outer Default, choose inner Play1 (display
  ordinal1, actual position0), select owned-gap. Confirm selected node,
  target.validate, mixed Default/Play branches and concrete presentation.
- independently resolve the displayed owned-gap start2/end5.
- Build a separate all-Play target with outer Play0 / inner Play0 and same gap.
  Confirm matches_instance and public occurrence_hold_duration_requirements for
  duration4. Expected exact pools:3 nodes,0 marks, since outer shared body is
  cloned as inner Repeat+body Hold+owned-gap Hold; inner gap edge is already owned.
- Public prepare_occurrence_hold_duration / apply_with_result succeeds in an
  unpublished copy. Root14->15; only outerPlay0 gap grows; final primary start2;
  outerPlay1 remains3; default gap remains1. Assert inverse and JSON.
- Assert ordinary source document/workspace unchanged by that control.
- All-Play State uses outer Play1, enter inner, inner Play1, owned gap selection:
  existing hold_parameter_in returns Some("hold-duration 3f").
- Generic mixed semantic_framing still refuses (not permission being widened).

Only then EXPECT mixed State.hold_parameter_in Some("hold-duration 3f").
Current body-only cached path returns None. That is intended old failure. Do not
also expect a mixed core success in this old witness: that gate fails earlier
and would mask the final native hint failure.

After baseline pin, semantic success for outer Default / inner Play gap duration4
must root14->16 and preserve current representative start2. Command exact pools
0/0. Duration-only context accepts saved intent SetHoldDuration, no new schema.

## Selected follow-up, if wanted in same baseline

Use inner3 with body Hold2, default gap1, owned gap after position0 Hold3,
after position1 Hold4, dormant final branch Hold5; outer2 no gap. Root26.
Primary is gap after position1 with visible start7; select gap after position0
as counterpart under equal outer Default prefix and distinct inner Plays.
Requested2 means gap1 shrink2 plus gap0 shrink1, each used twice:26->20.
Final primary starts6 after earlier gap changed. Primary-first leaf order must
recompute final continuation. Public positive control can be sequential
all-Play occurrence edits on independently isolated outerPlay0; expected root23
and still untouched outerPlay1. Do not use Compound scalar mixed controls as
old prefix, because current mixed command intentionally refuses gaps.

Native counterpart path must be confirmed from current also_targets slot
projection: select corresponding gap row, not body row. If owned gap subtrees
differ structurally, fixture must use same gap-root Hold shape and not infer
counterparts via shared NodeId. A selected scalar public all-Play control plus
native selected authority bound checks is sufficient; no fake default-gap node.

## Additional direction

outer Play / inner Default can be tested with inner Repeat living inside the
outer's owned gap subtree. An immediate outer Play gap edge cannot become
Default. This is a separate structural direction, not the same two-level
body-first fixture with branches swapped.
