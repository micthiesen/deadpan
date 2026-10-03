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
- `ReplaceSliceChildren` and `ReplaceSourceChildren` replace the exact span in
  one command without endpoint splits. They capture surviving suffix samples
  before removal and transform root sounds once. Empty old or new contents
  use the corresponding insertion, deletion or no-time-change operation.
  Removed owners still trigger mark loss policy when both sides are empty.

Current Range and Child behavior and serialization remain unchanged. No
document or database schema constant changes. This development build adds a
closed command and selector variant; older binaries need not accept them.

## Group object workflow

Native, headless and semantic editing share this group object contract:

| Keys | Target |
| --- | --- |
| `yig` / `dig` / `rig` | Copy, cut or repeat group contents |
| `yag` / `dag` / `rag` | Copy, cut or repeat the whole group |
| `v`, then `ig` / `ag` | Select contents or the whole group as a Visual object |
| Visual `y` / `d` / `r` | Apply to that exact object |
| Visual `p` / `P` | Replace that object using the chosen register |
| `:splice`, then `r` | Preview replacement of the captured object |

An explicitly selected direct ordinary Sequence wins. Otherwise the containing
nonroot Sequence supplies the group. A selected Source, Hold, Repeat or Retime
does not suppress this fallback. Root is never an implicit group object, and
the cursor never guesses an object from equal timestamps.

`ig` resolves to the exact inclusive child forest. It excludes the group's own
framing, treatments and owned marks. `ag` selects that group as a whole child,
including its authored context. Independent root sounds remain separate.

Object Visual state stores the exact group identity and kind at its surrounding
revision. It derives membership and geometry from that document. Selection moves
the cursor to the object's end while preserving the explicit selected beat.
`v` finishes and retains the object. Moving an extending object changes it into
a time range anchored at the object's start. Moving a finished object leaves
its identity fixed. The visible selection cue distinguishes these states.

A group with no children is a valid empty `ig` Visual target. Copy, cut and group
refuse because there is no forest; paste inserts at its child slot zero.
A group containing only empty children has a real forest that can be copied,
cut, grouped or replaced. Repeat requires positive duration in both cases.

Direct object copy preserves navigation, cursor and selected beat. Visual copy
finishes its selection. Mutations clear Visual state:

- `ig` from inside stays inside the surviving group and selects the direct
  result or cut join. From outside it keeps the surviving group selected.
- `ag` continues in the outer parent. Cut selects the literal next sibling,
  otherwise the previous sibling, otherwise nothing, including equal-time
  empty siblings. Replacement, grouping and Repeat select the new direct result.

Macros record unresolved `TextObject` selectors or `SelectObject` instructions.
Each instruction and counted call resolves against the preceding staged context.
Dot resolves the saved selector against the new context; a current Visual object
overrides it. Each register capture records its unique timing identity, effective
parent, scope bounds and staged path labels. A capture's path comes from its
actual staged document, independently of the invocation and final navigation.

The CLI uses strict tagged Visual values: `{"type":"time",...}` and
`{"type":"object","selection":{"kind":{"type":"inner_group"},
"group":"group-id"},"extending":true}`. Supplied group identities are checked
against the surrounding revision and ordinary navigation scope. Historical
media admission still recaptures stored slices. No schema migration is provided
for the earlier development-only Visual envelope.

## Remaining objects

`ib` and `ab` require an additional attachment distinction. Current inline beat
effects remain part of the beat in either case; marks are owned boundaries,
and current sounds belong to the independent root bus. Full `ab` support needs
the specified beat-owned temporal attachment model and its editing lifecycle.
Root sounds must never be collected solely because their time overlaps a beat.
Analysis-based objects and occurrence editing also remain required.
