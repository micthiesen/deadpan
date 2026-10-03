# Exact sibling selections

`SliceCaptureSelection::Children { first, last }` selects a nonempty inclusive
span of direct children in an ordinary Sequence. Identity and sibling order
determine membership. A time interval is only a derived display coordinate.
This preserves empty children at either endpoint, where a time range cannot
identify the intended structures.

`ProjectDocument::sequence_children` validates the parent and its ordinary
Sequence ancestry, resolves both endpoint IDs, and returns the first slot,
exclusive end slot and exact global Edit range. Equal IDs select one child.
Missing, foreign or reversed endpoints refuse. A nonempty span may have zero
duration; a group with no children has no span to select.

## Capture and placement

Capture retains the whole subtree of every selected child, in order, including
empty endpoints and interiors. The unselected parent contributes no framing,
treatments or owned marks. Whole-child capture of that parent remains the way
to retain its own authored context. Independent root sounds stay outside both
capture forms.

Serialized Children captures require matching first/last parts, distinct whole
roots and exact contiguous output windows. A zero-duration capture contains
only Sequence nodes with no physical audio bindings. Store admission recaptures
the exact selector at its immutable revision. Removing or reordering an interior
empty child may preserve the displayed range, but cannot preserve that proof.

The existing paste path imports the forest under a fresh neutral wrapper,
renames its owned identities and retains its sampled context. A zero-duration
forest uses an explicit sibling slot; it does not allocate timing, schedule
source pictures, or audition nonexistent audio. Its native copy and placement
labels say empty contents.

## Editing

- `GroupSelection` groups the exact span, including empty endpoint children,
  without changing rendered time. It adds no endpoint splits.
- `RepeatSelection` places the exact span under its neutral body Sequence.
  It retains all selected empty children and refuses a zero-duration body.
- `DeleteChildren { parent, first, last, timing }` removes the exact span in
  one reversible transaction. It captures surviving suffix entries before
  removal and transforms the independent root sound bus once. Zero-duration
  and terminal deletion allocate no unnecessary suffix clock.
- Store and Compound cut admission require the Children capture and
  DeleteChildren command to name the same parent and endpoint IDs. A
  DeleteRange with identical picture time is insufficient because it can leave
  selected empty children behind.

Current Range and Child behavior and serialization remain unchanged. No
document or database schema constant changes. This development build adds a
closed command and selector variant; older binaries need not accept them.

## Remaining text-object integration

This is the shared structural boundary required by specification §7.3, not a
completed keyboard text-object workflow. `ig` must resolve exact contents;
`ag` must retain the selected or containing group's own context. Visual state,
macros and dot must retain object intent and correct scope transitions instead
of reducing it to endpoints.

`ib` and `ab` require an additional attachment distinction. Current inline beat
effects remain part of the beat in either case; marks are owned boundaries,
and current sounds belong to the independent root bus. Full `ab` support needs
the specified beat-owned temporal attachment model and its editing lifecycle.
Root sounds must never be collected solely because their time overlaps a beat.
Analysis-based objects and occurrence editing also remain required. No new
keyboard object is advertised by this foundation.
