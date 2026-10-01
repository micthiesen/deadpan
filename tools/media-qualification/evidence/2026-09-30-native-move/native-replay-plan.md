# Native Move replay witnesses

Implementation planning only; none of these new native checks has run.

## One Source, three cuts

Use the existing qualified 120-frame original fixture. Copy current Edit
`[20,30)`, navigate to60, open `:splice`, select Move. It must preview the exact
order `[0,20),[30,60),[20,30),[60,120)`, total120. Insertion result is `[50,60)`;
removal join is20. Three cuts share one original owner and need no artificial
group wrapper.

- Copied endpoint ordinals:20 and29, exclusive Out30.
- Removal proposed frame20 is Original30; saved frame20 is Original20.
- Insertion proposed frame49 is Original59, frame50 is Original20, frame59 is
  Original29, and frame60 is Original60.
- Commit adds one history row and selects the full `[50,60)` interval. Undo
  restores all authored data, with the required fresh revision.
- Cancel across endpoint/site/comparison changes leaves every database cell
  unchanged and preserves the accepted copied register.

Reverse witness: copy `[70,80)`, insert before10. Proposed order is
`[0,10),[70,80),[10,70),[80,120)`. Inserted interval is `[10,20)` and final removal
join80. Compare/audition each site independently.

## Authority and operation choice

- Destination25 for source `[20,30)` rejects own-interior removal. Source endpoint
  inspection remains available. Destinations20 and30 in the same parent explain
  the exact no-op and cannot commit, without splitting or allocating clocks.
- Copy a source, commit another edit or Undo, then open placement. Historical
  Copy remains usable; Move rejects the old source revision. A new current yank
  re-enables Move. An old source revision paired with a fresh destination must
  still reject at service ingress.
- Capture an independent replacement `[60,70)`, navigate to90, then open
  placement. Switching Replace to Move must use the retained insertion point90,
  not reinterpret replacement In60 as a new destination. Move and Replace remain
  exclusive. Switching back to Copy leaves the copied register intact.
- Original+Move and Replace+Move fail at service ingress as well as being
  unavailable in controls. Source/destination edits advance proposal identity;
  switching the active join does not.

## Scope and unchanged timing

Seed ordinary groups through a validated fixture, then use production Enter/
Backspace navigation to copy in one group and place in another. Result scope,
parent label and full range selection must agree with the committed document.
Keep empty donor groups. Do not flatten whole Repeat/Preserve units.

For boundary reparenting, use root `[A(a:20),B(b:100)]`, then move a into B slot0
at old20. Every global provider frame remains in the same order while B's live
framing/gain now applies to a. Compare identical global frames/samples, display
both group scopes and timing unchanged, and avoid a fake removed-time gap.

## Comparison, transport and asynchronous state

- Both directions, distant sites, one-frame separation and overlapping default
  context. Cap the between-sites flank at the other join, label shortened context
  and never prepare the whole distance merely to audition a local join.
- Exact checked absolute sample boundaries at30000/1001. Comparison is explicitly
  site-relative time; canonical PCM in each view must equal its own committed
  result. Do not claim all voices share one physical source-sample map.
- Keep playing/paused state while toggling Before/Proposed; exact empty seams map
  to counterpart starts. An inspection cursor outside the paired context resets
  visibly to the selected join. Terminal boundaryN and displayed pictureN-1 stay
  distinct.
- A delayed successful receipt closes the draft but cannot consume the old
  visible range. Select the entire result only after matching session/project/
  revision and scope reconciliation. Duplicate receipt after navigation cannot
  reselect. Retain saved/Reopen guidance after failed refresh or coalesced copy
  feedback. Test session changes and stale replies separately.
- Switching sites may retain the last submitted picture while a new one loads.
  Its displayed caption/identity must remain tied to that accepted picture.

## Visual and native verification

Run real router/pointer/Tab and synthetic IME cases, update help/keycaps and live
Kestrel audit together. Inspect minimum960x640/default1280x820 Metal captures:
source endpoints, both labelled local timelines, current operation, main picture,
units and focused control remain readable. Existing Copy/Replace layouts and
Original/delete workflows retain their regression checks. Run separate release
performance mode with unchanged target definitions.

Use only a uniquely named temporary QA bundle/private project for native keys
and SQLite backups. Never inspect/select/move/resize/close the user's reserved
`dev.thiesen.deadpan.cursor-qa` app or switch its Space.
