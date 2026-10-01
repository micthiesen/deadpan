# Native atomic move follow-up

Implementation plan for the next milestone. Core/media/persistence focused tests
pass; the backend full gate is running. This file is not implementation or
qualification evidence. The independent `native-design-review.md` beside it
corrects boundary-only reparenting and specifies bounded comparison semantics.

## User flow

- Copy a linked range from Your edit with v/motion/y, navigate, open :splice.
- Insert remains the default. Add an explicit Move choice with visible `m` key.
  Moving requires the register's source revision to equal the captured current
  destination revision. Historical copying remains available after edits/Undo;
  an old register cannot delete whatever now occupies its numeric source range.
- The source parent and destination parent remain explicit and may differ.
  Ordinary Sequence endpoints/scopes match the new core command's admission.
- Source In/Out refinement stays local to the draft and never alters the register.
- Moving and replacing are mutually exclusive. Choosing Move exits replacement
  and retains the existing insertion destination. Choosing Replace exits Move.
- Expose source removal and destination insertion positions simultaneously, with
  total duration unchanged. Exact no-op placement gives an explanatory message
  and no native commit, though generic core history may permit revision-only work.
- Escape restores entry view and selections with no authored changes. Enter
  commits the retained typed request once and selects the moved result's range.

## Source and proposal authority

Keep the existing copied neutral source view for endpoint pictures/refinement.
It supplies display authority only. MoveRange addresses current authored IDs via
source_revision/source_parent/range and a pre-edit explicit destination; it uses
no paste identity pool or historical media import exception.

Add a typed Copy/Move operation to the native proposal. Do not infer destructive
intent from Destination::Replace or from the presence of an edited register.
Reject Original+Move, historical source+Move and Replace+Move at service ingress,
even if the UI disables those choices. Prepare the source view before a rejected
destination, as in copy, so error feedback does not erase endpoint inspection.

Narrowly extend store-issued edited preview admission to accept MoveRange with
capture_revision equal to its validated current source revision. Continue through
the exact prepare_command transaction and sealed media view. Do not loosen strict
Original proposal admission. The same proposed snapshot/picture/PCM machinery can
then inspect the moved document with its final live ancestor treatments.

## Result selection

Move retains several existing child roots; do not wrap them in a new group merely
to satisfy Prepared.node's current one-root assumption. Separate the prepared
result's exact range/parent/first selected child from copy's imported group ID.
Resolve the final moved children inside the admitted proposal and verify their
combined contiguous duration against core preflight.inserted. Retain this result
with the request and in `CommittedEdit`, including after the draft closes.
Completion selects that exact Edit range only when the visible workspace matches
the successful receipt's project/session/revision, after scope reconciliation.
The generic once-only completion path owns this. Duplicates cannot reselect after
later navigation. Failed refresh preserves the old visible selection and
saved/Reopen guidance. Both `finish_splice_preview` and `receive_splice` currently
assume the first node spans the entire result; replace those assumptions with
validated forest metadata rather than manufacturing a group.

## Join inspection and comparison

Current copy/replace preview has one edited region; move has two separated sites.
Do not reuse Prepared.removed as if it shared the insertion start.

Retain explicit metadata:

- source_before: selected half-open range in the saved revision;
- removal_after: core removal_join in the proposed revision;
- destination_before: core pre-edit boundary;
- inserted_after: core inserted interval.

Provide a distinct keyboard-accessible removal-join control (`s` is a candidate)
and keep `f` for insertion inspection. Show the active join in the main picture
and playback controls. Refocus resets to that site's exact boundary without
changing either source selection or destination. Existing h/l/counts inspect
frames; destination d/j/k retain their existing placement behavior.

Before/Proposed comparison maps within the active site's corresponding windows:

- Removal compares saved [a,b) with proposed [removal_join,removal_join).
- Insertion compares saved [destination_before,destination_before) with the
  proposed inserted range.

Comparison preserves site-relative time. Prefix offsets are relative to the
site's start, suffix offsets to its end, and an empty seam explicitly maps to the
counterpart start. Interiors with empty counterparts clamp to that join. The
existing map_comparison requires equal starts and is insufficient for move.
Use checked arithmetic over complete absolute frame/sample boundaries, including
the possibly different rounded allocations at fractional frame rates. This is
not a universal same-source-sample map: root sounds and source voices have
different ownership. Preserve playing/paused state with existing transport rules.

Meaningful moves at source In or Out can change grouping without changing frame
order. For these boundary cases, use global identity frame/sample comparison and
show both scopes with "Move between groups; timing unchanged". Do not draw a
temporal gap or map the unchanged content into another beat.

Cap the context flank between the two affected sites at the other edit boundary,
as specified in the review's direction table. Keep affected intervals complete,
honestly label shortened context, and clamp sample mapping to actual counterpart
window allocations. Compare outside the paired window resets visibly to the
active join. A terminal join remains boundary N while its picture is N-1.
Switching active site changes only cursor/window/transport identity, not the
proposal identity. Only source/destination/operation edits rebuild the proposal.

Shift-Space loops the chosen site's bounded context. For insertion, this includes
both slice edges; for removal, it includes the joined surviving context. Labels
must say which site loops. The source and destination can be arbitrarily distant;
do not accidentally prepare a giant contiguous loop simply to cover both sites.
The user can inspect/audition each site without modifying the proposal.

## Layout and keys

Two local timeline strips can keep distant joins legible: a labelled removal row
and insertion row with independent explicitly labelled clocks. Do not squeeze a
whole long project between tiny marks on one unexplained scale. Endpoint strip,
main picture, current operation and exact durations stay visible at 960x640.
Avoid growing another permanent toolbar row if operation choices fit an existing
row; measure wrapped text before painting and preserve minimum picture tests.

Add keys only through navigation::splice::route_key, contextual help and the
production Kestrel audit fixture together. Test plain logical keys, native button
focus, repeat suppression, composition, modifier chords and pending counts.
Candidate keys are not approved until that compatibility check is performed.

## Decisive native follow-up checks

1. Same/cross-parent moves in both directions; three cuts inside one source;
   actual before/proposed/committed decoded frames at removal and insertion sites.
2. Each site's PCM window matches the committed edit, including NTSC boundary
   samples, live ancestor treatments, unchanged root sounds and Hold gates.
   Comparison separately checks its explicit site-relative mapping, including
   close/distant sites, window caps, terminal positions and boundary reparenting.
3. Copy vs Move vs Replace toggles keep source and destination intent; historical
   copy remains usable while stale move rejects; a refreshed capture enables it.
4. Own-interior destination and selected-descendant cycle reject with endpoints
   still visible. Exact no-op commits nothing through the native action.
5. Cancel changes no database cells; commit is one revision; Undo restores all
   authored data and stable identities. Moved range selection waits for refreshed
   workspace; saved failures and duplicate receipts preserve recovery guidance.
6. Late source/proposal/picture/PCM replies cannot retarget either join; entry
   state restores only in the same live revision/session.
7. Minimum/default Metal captures plus a unique isolated native bundle for the
   high-value keyboard path. Reserved user Cursor QA app/Space stays untouched.

No claim that this satisfies temporal Repeat/Retime/generated interiors,
role-only placement, cut-to-register, persistent registers or the full spec.
