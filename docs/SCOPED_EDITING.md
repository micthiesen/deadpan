# Editing Repeat contents

`Enter` opens a selected Repeat or Retime. Inside it, `j/k` selects children,
`Enter` descends and `Backspace` returns to the parent. On an existing Hold with
This play at every Repeat level, `Enter` opens its exact duration instead.
Enter also opens Hold duration inside fixed Edit Retime windows without Repeat
ancestry, with All plays explicit at every enclosing Repeat, or in the narrow
This play case of explicit Repeat bodies containing one fixed, unclocked Edit
Retime. Derived pause clocks retain their timing guard.
Pause insertion follows the explicit choice at each enclosing Repeat. With This
play at every Repeat level, `:retime` changes the selected beat's speed in that
play only: a first change isolates the play and wraps the beat in its own
Retime, and later changes update that Retime in place. All plays refuses it.
`s`, `dd`, `rr`, `:repeat N` and `:repeat [N] gap=` change the selected beat
in This play only (below). Other temporal edits still use the ordinary Sequence timeline.

Choose `:scope all` to edit a Repeat's shared definition or `:scope play N` to
edit one stable play by its current one-based number. These choices apply at the
nearest displayed Repeat level. Nested Repeat choices remain separate. All
plays preserves existing overrides. Browsing never creates an override.

`:scope plays 2-3` (or a list such as `:scope plays 1,3` or `1,3-5`, at least
two distinct plays) selects several stable plays of the nearest displayed Repeat
together. The viewer browses the first; the inspector shows "Plays 2, 3 edited
together" and the breadcrumb `[plays 2, 3/N]`. Gain, Camera and pause-audio
changes then commit one `EditScopedMany` transaction: the same beat in every
selected play, located structurally in each play's owned branch or the shared
definition, with only those plays isolated and one Undo. A gain change carries
over as the same trim step or ranged step, so each play keeps its own recipe; a
play whose gain recipe differs in another way, or whose owned contents differ
in structure, refuses with a message instead of being overwritten. Camera and
pause-audio values are set as chosen in each play. Nested contents entered
below the multi-play level keep the choice. A deeper `:scope all`,
`:scope play N` or `]r`/`[r` changes that inner branch while retaining the outer
selected set. A scope change at the selected level ends its set; a new
`:scope plays` replaces the previous selection axis. The choice survives its
own commit, because the Repeat keeps its identity and stable play IDs. An exact
set retained from a commit ends on scope navigation, since row positions cannot
recreate its mapped counterparts.

Freeze and black pause insertion also apply to every selected play, as one
Compound and one Undo. Each counterpart uses the same local boundary in its
selected owner and captures its own picture and framing. The resulting Holds
remain selected together even when their groups have different child durations
and the new Holds occupy different child slots. Further insertion can use that
retained set. Browsing another row or scope ends this exact Hold selection.

`:gain +3dB range=4-10` adds a constant step over exactly those local frames of
the selected beat, keeping its trim, other ranges and saturation. Inside Repeat
contents it is a partial-range edit within the selected play or plays. It is
not recorded in macros.

`]r` and `[r` step the nearest displayed Repeat through All plays, then play 1
to N, clamped at both ends; a count steps further (`3[r` returns to All plays
from play 3). On a selected Repeat beat in Your edit, `]r` opens it at play 1
and `[r` at its last play; with a count, `3]r` opens play 3 and `2[r` the
second-to-last play, clamped to the Repeat's plays. Inside a play, `j`/`k` reach that play's owned gap
branch when it has one, and `Enter` descends into nested groups and Repeats,
whose plays `]r` then steps independently. `Backspace` keeps each outer play
choice. The inspector's Previous/Next buttons and the footer teach the keys.
Stepping is read-only, like `:scope`, and Macros do not record it.

`:isolate-gap` (since 2026-11-08), with This play chosen on a Repeat, gives
that play its own copy of the default gap after it (core `IsolateGap`: same
timing and sound clocks, one Undo). `j`/`k` then reach the gap inside the play,
where duration, room tone, freeze or AI pictures change only it. For a Repeat
nested inside others, every enclosing Repeat must also have This play chosen;
the service then sends one `EditOccurrence { edit: IsolateGap }`, which copies
each chosen enclosing play outside-in (sized by
`ProjectDocument::occurrence_gap_requirements`) before isolating the gap, so
other occurrences keep the shared default. An All plays choice at any enclosing
Repeat refuses with a hint. Evidence: the `scoped-plays` replay check
":isolate-gap in play 2 gives only that play its own gap pause" and
`project::tests::isolate_gap::nested_isolate_gap_copies_only_the_chosen_outer_play`.

## The shared gap after a play

A Repeat's default gap is an implicit Hold recipe: it plays after every play
without its own gap pause and has no authored node. With This play chosen at
every Repeat level (since 2026-11-09), the play's rows end with a row for it,
such as "Gap after play 2 · 6f · shared gap", whenever that gap renders: the
play is not the last, the Repeat has a positive default gap and the play has
no gap pause of its own. `j`/`k` reach it like any row. It is not offered for
All plays (`:repeat gap=` and a selected Repeat's `:room-tone` edit the shared
recipe) or for several selected plays.

The row's inspector names the shared length and sound. Its edits change only
that play's gap, isolating it in the same transaction:

- `Enter` (Change length…) opens `:hold-duration` with the shared length.
- `:room-tone` opens the room-tone sheet for the gap; Apply sets it.
- `:hold-silence` (Use silence) makes it silent.
- `:isolate-gap` (Own copy only) isolates it unchanged.

The first three commit one semantic `SetPlayGap { change }` instruction
(`Length` or `Audio`). The planner adds one Compound leaf `IsolateGap` (or,
below enclosing plays, `EditOccurrence { IsolateGap }` sized by
`occurrence_gap_requirements`) and then the ordinary one-play pause length or
pause sound leaf on the new Hold, so retained timing follows the existing
isolated-gap path: the copy binds the old default-gap clock and the length
change reanchors later owners on their pre-edit entries. One Undo restores the
shared gap. The inspector then selects the new gap pause with the cursor at
its first frame. Macros record `SetPlayGap` and dot repeats it after the play
chosen at invocation.

Refusals name their reason without saving: an unchanged length or sound, the
last play, a play that already owns its gap ("select it with j") and a Repeat
without a gap. While the row is selected, every other edit (gain, Camera,
pauses, structure, AI pictures) refuses with a hint to select the play's
contents with `k`; the row addresses the play's contents only as the
authority for these gap edits. A render preview of an unapplied gap
room-tone draft refuses, since it cannot isolate the gap.

The row has its own registry context, Shared gap row
(`Contexts::SHARED_GAP`), which Help marks as here while it is selected
(since 2026-11-09). Only actions registered for it say "Works here" in the
Keys sheet, and the footer teaches only those: Enter (change length),
`:room-tone`, `:hold-silence`, `:isolate-gap`, `k`/`j`, dot, undo, play
stepping, the parent, macros and `:scope`. Each footer hint names its
registry action and appears only where that action applies, so gain, mute,
Camera, punch-in, split, delete and repeat are never taught on the row. Dot
after any edit other than a gap or pause length or sound reads "repeat
unavailable".

Evidence: core `semantic::planner::tests::scoped_framing::scoped_play_gap`
(nested and top-level isolation, bound clocks retained, exact Undo,
refusals), service `project::tests::scoped_structure::a_shared_gap_length_in_one_play_becomes_its_own_pause_in_one_undo`
and `project::tests::scoped_structure::play_gap_replay` (a saved Macro
recorded in play 2 runs in play 3, and dot repeats the change in plays 3 and
1, each changing only that play's gap with one Undo per run), and the
`scoped-plays` replay checks from "j in play 2 reaches the shared gap after it
as its own row" through "One Undo restores the shared gap exactly", including
the gap row footer, `.` on play 1's gap row, the refused `:hold-silence` on
the silent shared gap and `:room-tone` on play 1's gap row.

## Splitting, deleting and repeating a beat in This play

With This play chosen at every enclosing Repeat (since 2026-11-08), `s` or
`:split`, `dd` or `:delete`, and `rr`, `3rr`, `:wrap-repeat N` or `:repeat N`
(on a beat that is not itself a Repeat) change the selected beat in that one
occurrence. The service sends one `EditOccurrence`, sized by
`ProjectDocument::occurrence_structure_requirements`, that copies each chosen
play outside-in and then applies `Split`, `DeleteRipple` or `RepeatChild` to the
copy, so other plays keep the shared definition. One Undo restores everything.

- `s` splits at the Edit cursor. A play whose whole contents are one beat gains
  a group holding both fragments. The inspector selects the right fragment.
- `dd` removes the beat from that play and the rest of the play closes up.
  Since 2026-11-09 it is a cut: the beat as it plays there is saved to the
  register first (see below), in the same transaction. `:delete` still removes
  without copying. The inspector selects the following beat, else the
  preceding one, else the emptied group, with the cursor at the old start.
- `rr` wraps the beat in a new Repeat in that play; the inspector selects it.
- `:repeat N` on a selected Repeat (since 2026-11-09) sends `SetRepeatPlays`
  and changes that Repeat's total plays in this occurrence only, as the root
  setter does: surviving plays keep their stable identities and gaps, removed
  plays end, and new plays take the edit's revision allocation. The inspector
  keeps the recounted Repeat selected with the cursor at its start. A count
  equal to the current one refuses ("already plays N times in this play")
  without isolating or saving anything.
- `:repeat [N] gap=... [gap-step=...]` on a selected Repeat (since 2026-11-09)
  sends `SetRepeatGaps` and changes that Repeat's plays and complete gap set in
  this occurrence only, exactly as the root `:repeat` does: the count first,
  then `gap=` sets the default gap (`gap=0` removes the gaps) and each later
  gap of a `gap-step` ladder becomes a fresh independent Hold after its play,
  while unnamed gap branches end. Every gap holds the last picture of the
  Repeat's first play in this occurrence, captured through
  `PauseSite::RepeatGap` with only the composition below the Repeat. Later
  plays and gaps inside the Repeat keep their own pre-edit sample entries as
  well. The same plays and gaps refuse ("already has these plays and gaps in
  this play") without saving; a plain beat refuses with "wrap a beat first".
  `gain-step`, `zoom-step` and `speed-step` still refuse inside Repeat
  contents; set them from the parent.

Deletion, Repeat, count and gap changes keep timing exact: every physical owner
after the change
(the rest of the play, later plays and gaps of each enclosing Repeat, and
everything after the outermost one) keeps its own pre-edit sample entry, and a
wrapped beat's first play keeps its old phase. Shared owners keep their earlier
occurrences unchanged. Core tests check the retained entries at 30000/1001 fps,
where a one-frame move is not a whole number of samples.

These keys refuse, with the earlier structural message, for All plays or
several selected plays at any level. Deletion, Repeat, count and gap changes also
refuse a play's whole contents (there is no group to close up; split it or press Enter first),
a beat inside a speed change or crop in the play, and documents with placed
sounds or beat sounds ("remove the sound events before editing time").

These keys commit through semantic Apply, so a recording macro records them and
dot repeats them. The saved intent is `Split`, `Cut` of the selected beat
(`dd`), `DeleteBeat` (`:delete`, without a register), `Repeat` on the selected beat, `SetRepeatPlays` or `SetRepeat` with
plays and gaps only; it names no beat, node or play. Each invocation resolves
the beat and the explicit play chain chosen then and authors the same single
`EditOccurrence` the native key does, with the same continuation. A macro
recorded in play 2 therefore edits play 3 when run there, and leaves play 2 as
recorded. Without This play at every level the instruction refuses ("choose
This play") and saves nothing. Evidence: the `scoped-plays`
replay checks "s inside play 2 splits only that play and selects the right
fragment", "rr inside play 2 repeats the right fragment there only", "dd inside
play 2 deletes the left fragment there only", ":repeat 3 on the Repeat inside
play 2 sets its plays there only", ":repeat 4 gap=6f on the Repeat inside play
2 sets its plays and gaps there only", the five Undos, "Recording s and dd
inside play 2 edits that play only and saves only Split and the beat cut" and
"@a in play 3 splits and deletes at the new play's cursor and leaves play 2 as
recorded";
`project::tests::scoped_structure` and core
`insert_time::occurrence_structure` cover nested plays, retained clocks and
refusals.

## Copying and pasting a beat in This play

Since 2026-11-09, with This play chosen at every enclosing Repeat, `yy` copies
the selected beat as it plays in that occurrence, `dd` cuts it, and `p`/`P`
paste a register after or before it in that play only. A `"x` prefix names the
register as at the parent.

- `yy` captures `CapturedEditSlice::capture_occurrence`: the play's effective
  contents (its own override branch, or the shared default) and every physical
  owner's clock over that occurrence's project interval, closed for each
  enclosing play as Explode closes them, so the copy carries no Repeat
  reference. The slice records the explicit plays (`plays`) and recaptures
  from them at store admission. Copying isolates nothing and saves no
  revision; the bank write is the only change.
- `dd` is one Compound: that capture, then the one-play `DeleteRipple`.
- `p`/`P` send one `EditOccurrence` `PasteSlice` (an edited copy) or
  `PasteSource` (an Original moment). It isolates the chosen plays outside-in
  and then places the content exactly as the root seam paste does
  (`SpliceSlice`/`SpliceSource`), at the seam after or before the beat in that
  occurrence: every later owner (the rest of the play, later plays and gaps of
  each enclosing Repeat, and everything after) keeps its own pre-edit sample
  entry, and the pasted Source starts unbound on the project grid. Other plays
  keep the shared definition. The inspector selects the pasted beat with the
  cursor at its start. One Undo removes it.
- A copy made in a play also pastes at the parent (`p` outside the contents
  and macros). Place slice cannot narrow it to a shorter range.

Refusals: a play's whole contents (split it first, or press Enter), a beat
inside a speed change or crop in that play, documents with placed or beat
sounds, an empty register, a copied node set, an empty copied group, Visual
ranges and motions (`yw`, `d3l`), and All plays or several selected plays.
Macros record `Yank`/`Cut` of the selected beat and `Paste`; a recorded paste
resolves the beat and play at each run. Dot repeats the cut in the play chosen
then; like the parent paste, `p` is not dot-repeatable. Evidence: core
`insert_time::occurrence_structure` (copy, slice paste and Original paste at
30000/1001 fps keeping the tail's and later plays' retained entries, exact
Undo, refusals), the planner `scoped_structure` tests (register equals the
occurrence capture, cut Compound, refusals), `project::tests::scoped_structure::copy_paste`
(service copy, paste, cut, Undo, parent and native parent paste of a play copy)
and the `scoped-plays` replay checks "yy inside play 2 copies that play's right
part into the default register without an edit", "p on play 3's whole contents
refuses readably without an edit", "p inside play 3 pastes play 2's copy after
its right part there only and selects it" and "Three Undos remove the paste and
both splits; the copy remains".

## Framing a range in This play

Choose This play at every enclosing Repeat, select a visible child, then use
`v`, frame motions and `v` to retain a half-open Time selection on the root Edit
clock. Frame motions and End admit the exclusive Out boundary, including the
whole play. At an internal seam the viewer shows the picture to its right.
Finish with `v`, then `h` to return inside before framing; the finished range
stays fixed. Moving its active head keeps the same owner and concrete play.
Changing owner, play, scope or authored revision clears it; the first Escape
clears the range before another Escape leaves the inspector.

`:zoom`, `:creep` and their comma keys use that selection only when its complete
root interval lies inside the selected occurrence. The canonical plan maps both
boundaries through enclosing Repeat positions, Retime transforms and crops into
the owner's exact output clock. A partially visible fractional boundary refuses;
seeking a sampled picture there does not prove the whole frame interval belongs
to that occurrence. The stopped cursor must also show the selected owner.
All plays, mixed or several-play choices, a plain nested Retime without a Play,
empty ranges and ranges crossing an occurrence boundary refuse atomically.

Keep-center and explicit `target=center` work when the existing static baseline
permits the ranged edit. On a fixed Follow, only implicit Keep-center creep is
available: it retains the live target and both scale bases, and uses the selected
owner's exact output clock. On a supported static baseline, a newly chosen saved
target is sampled at root Edit In for Smash, or Out minus one frame for Creep.
The fixed center uses that actual provider picture, captured geometry and inner
framing, independently of the stopped cursor. Even a full-owner Time range stays
a fixed sample, rather than becoming Follow. Creep holds its arrival pose after
Out. A missing, clipped or out-of-span target refuses without changing anything.
To use a detected face or region, clear the range, save it explicitly in Camera,
leave Camera and select the range again with the saved target's id or label.
Direct `face:N`/`region:N` scoped range commands still refuse. Sampled-target
recording remains unavailable; existing whole-owner framing behavior is unchanged.

The retained scoped Time selection is for framing and read-only audition. Other
owner parameters, including gain, Hold duration, pause insertion and Camera,
refuse until Escape clears it; independent placed-sound controls keep their
captured sound target. Object, mark, transcript and multi-range selections do
not become scoped framing authority. The inspector describes the saved recipe
with Follows, Scale before, Creep, Window and Framing clock rows, including
retained duration and offset when present.

Slice placement and structural replacement require returning to the parent,
even without a Time range. Their pointer controls and command routes use the
same scope rule; a destination captured before entering contents also refuses.

Finish `v` / motion / `v` before starting `q`. Record only the resulting Zoom
intent or a named framing call. Scoped navigation and selection gestures remain
unrecordable; each named or dot invocation needs a fresh selected range and
explicit play choices, and derives its own window from that invocation.
See [framing](FRAMING.md#zoom-and-creep-commands),
[macro authority](SEMANTIC_MACROS.md#one-play-zoom-and-creep) and the
[scoped range qualification record](qualification/scoped-framing-ranges-2026-10-25.md).

## Scope-aware parameters

Gain, Camera, Hold audio and [AI pictures](AI_HOLDS.md#repeat-and-retime-scopes) use the captured scope. Changing a shared node in
This play isolates only the necessary selected ancestors, then applies the
value in the same reversible transaction. An unchanged value creates no
override or history entry. `,h`, `,a` and `:hold` insert a silent freeze, AI
fallback or black pause at the retained Edit cursor when every enclosing Repeat
names This play. Both edges belong to the selected occurrence, including the
start of a later play. One transaction isolates the necessary branches and
inserts time; the inspector keeps the new Hold selected. AI insertion saves its
durable preparation with the fallback and still requires explicit acceptance.

All plays inserts the pause into the shared definition, so default occurrences
grow and overridden plays keep their branches. Choices stay independent at each
level: outer All plays plus inner This play grows that inner play in every
default outer use; outer This play plus inner All plays grows every default
inner use only in that outer play. Several selected plays vary one Repeat level
under the same prefix. Each owner retains the concrete descendant reached by
its own local cut, even when different timings reach different inner plays.
AI pauses still need
one concrete play at every level; Retime ancestors refuse this occurrence path
([pause insertion](INSERT_TIME.md)). Freeze
capture also refuses a boundary whose preceding picture does not pass through
the selected insertion parent; black pauses need no sampled picture.
Mixed owned-gap Hold resizing, Repeat/Retime timing combinations, other
timing edits (Trim, Slip, motions and ranges), range copies and general macros
inside these contents remain required work; their native actions refuse
without editing the retained outer beat. Split, beat deletion, Repeat wraps and
Repeat count and gap changes in This play are described above.

Freeze/black pauses, existing Hold duration and sound, and whole-owner or
supported ranged Zoom and Creep with Keep or OriginalCenter intent support
recording and dot when This play is explicit at every enclosing Repeat. Saved
programs retain length/picture or framing intent; invocation
captures the current play and maps its authoring and presentation addresses
through isolation. A pause continues from its actual new Hold, keeping the
ordinary cursor and enclosing selection. A duration setter continues at the
resized Hold's exact start, even when shrinking past the old interior cursor.
Hold sound changes retain the absolute cursor and preserve picture and timing.
Without Visual selection, InsertPause, SetHoldDuration, SetHoldAudio,
SetRoomTone, whole-owner audio setters, Zoom and nested named calls retain their
existing admission. With the scoped Time range retained, only Zoom and named
framing calls are admitted. Scope navigation cancels recording and
permanently revokes any queued macro's inspector continuation. See
[one-play framing macros](SEMANTIC_MACROS.md#one-play-zoom-and-creep).
See [one-play pause intent](SEMANTIC_MACROS.md#one-play-pause-intent) for pause
recording, dot and unsupported-scope behavior.
See [one-play Hold sound](SEMANTIC_MACROS.md#one-play-hold-sound) for exact
prepared samples, Silence, RoomTone and unchanged direct scope workflows.

`:hold-duration Nf` resizes the selected Hold in one explicit play at every
level when its ancestry uses only Repeat and Sequence nodes. The inspector's
Change duration control and Enter prefill its current length. A no-op refuses
without history, isolation or cursor movement. Root or attached sound events
remain unavailable for this timing edit. Accepted generated-picture
shortening retains its provider; extension beyond accepted coverage saves the
deterministic fallback and durable preparation for a new explicit candidate
acceptance. The [Hold duration qualification](qualification/scoped-hold-duration-2026-10-20.md)
retains the nested-play, real-PCM, history and native verification.

Explicit This play choices at every enclosing Repeat also support this duration
edit through exactly one fixed, unclocked Edit Retime below the innermost Repeat.
The target path may otherwise contain Sequence ancestry, but no other Retime,
Partition or owned gap. Each edit isolates its selected branches outside-in and
retains the Retime's source crop and output extent. Visibility composes crop and scale
with the selected stable play; the cursor continues at the first sampled
visible Hold picture, which can differ from the Hold's intrinsic start.
Hidden or unsampled Holds, derived pause clocks, root or attached sound events,
and selected-multiple-play contexts refuse. Mixed choices use the separate path
below. Repeat depth follows the
existing document bound. See the [nested This play fixed-Retime qualification](qualification/nested-fixed-retime-hold-duration-2026-10-22.md).

All plays at every enclosing Repeat has a separate duration-only path. It
changes the literal shared Hold once and preserves every independent play
override. A visible default continues at the new start of the same captured
stable play. A default whose body uses are all overridden remains editable
without inventing a presentation; its cursor and rendered duration stay fixed.
The core proves dormancy from complete body-override coverage, independently
of gap overrides. Recording and named calls accept duration setters only.
Fixed unclocked Edit Retimes below the inspected Repeat are supported: each
window keeps its output length, and continuation uses the first sampled visible
Hold picture after every crop. Nested Repeat offsets follow current stable
plays, including independent body and gap overrides. Hidden or unsampled Holds
refuse. Partition, derived clocks and fixed Retimes above the inspector root
retain their guards. Mixed and several-play paths keep their separate limits.
See [shared duration semantics](SEMANTIC_MACROS.md#all-plays-hold-length).

Mixed All plays/This play choices have their own duration-only context.
An explicit This play branch is isolated only when it still uses the default;
All plays choices stay shared. Already owned branches consume no new node IDs.
The actual isolation proof maps both the authored target and its independent
visible representative. A proven dormant definition may still need isolation
to retain its selected Play meaning, while output and cursor stay fixed.
Recording, calls and direct-edit dot retain those choices. An explicit owned
gap follows its preceding stable Play, including beneath an outer Default or
above an inner Default body. The current final Play's gap is a dormant
definition. Mixed body choices also support one fixed, unclocked Edit Retime
below every enclosing Repeat, with Sequence ancestry otherwise. Literal All
plays choices remain shared; the independent representative follows the actual
isolation proof. Each setter preserves the crop and output extent and continues
at the first sampled visible Hold picture. A cropped intrinsic start is not a
continuation frame. Hidden or unsampled Holds refuse before publication.
Owned-gap Retime paths, additional Retimes, Retimes above or between Repeats,
Partition, derived clocks and placed sounds remain unavailable on this mixed
path; several targets use the selected path below. See [mixed duration semantics](SEMANTIC_MACROS.md#mixed-all-plays-and-this-play-hold-length)
and the [ordinary mixed qualification](qualification/mixed-hold-duration-semantics-2026-10-21.md)
and [fixed-Retime qualification](qualification/mixed-fixed-retime-hold-duration-2026-10-22.md).

Several selected Holds in bodies or explicit owned gaps use a separate
duration-only context. The selection
must vary explicit Plays of one Repeat under an identical earlier prefix;
later All plays/This play choices remain intact. Enter prefills the primary
Hold's length even when the selected recipes differ. Each changed member uses
its own staged temporal edit, and all members commit as one Compound with one
Undo. Actual isolation proofs retain the complete mapped selection; the cursor
resolves the primary Hold only after every resize. A zero total duration change
can still change several recipes. Valid no-op members consume no identities;
an entirely unchanged selection makes no edit. Browsing another row revokes the
mapped selection; choose a new play set explicitly before another batch or dot.
A final owned gap can remain selected alongside live gaps; changing its recipe
does not add a trailing gap. A dormant primary preserves the ordinary cursor
while live counterparts may ripple. Implicit default gaps, other Retime ancestry,
Partition and placed sounds remain unavailable. A single all-Play dormant gap
retains the existing live-boundary refusal.
See [selected duration semantics](SEMANTIC_MACROS.md#selected-plays-hold-length).

Selected bodies can cross one fixed unclocked Edit Retime below every enclosing
Repeat, retaining either selected axis and the other level's literal All plays
or This play choice. Every member, including a no-op, must have sampled visible
geometry or a proven dormant Default definition before any allocation. Crops,
pitch and fixed output remain unchanged; the final primary cursor uses the
cropped visible interval. See the [selected fixed-window qualification](qualification/selected-fixed-retime-hold-duration-2026-10-22.md).

An ordinary Hold inside fixed Edit Retime windows, without any Repeat ancestry,
has a separate duration-only path through the same control, Enter and command.
It supports recording, named duration calls and dot, revalidating the actual
Hold and every crop before each setter. Its cursor continues at the first
sampled visible picture, which may differ from the Hold's intrinsic start.
The fixed output duration and independent output sound clocks stay unchanged.
Visual ranges, derived pause clocks and Holds with no sampled visible picture
refuse. Recording admits length setters and named duration calls; other edits
must wait until recording is saved or cancelled. See the
[native fixed-window qualification](qualification/native-fixed-retime-hold-2026-10-21.md).

The This play, mixed and selected fixed-Retime paths above require literal body
choices and one fixed unclocked Edit Retime below all enclosing Repeats. Other
Retime duration paths retain their guards. See the
[nested This play fixed-Retime qualification](qualification/nested-fixed-retime-hold-duration-2026-10-22.md).

The [This play pause qualification](qualification/scoped-pauses-2026-10-12.md)
records exact-boundary, PCM, durable-history and offscreen keyboard verification,
including the compact inspector correction and retained failed runs.

The [mixed and selected-play qualification](qualification/mixed-repeat-pauses-2026-10-20.md)
adds complete per-level scope, exact mapped target sets, decoded PCM, real
release exports and compact keyboard/pointer replay evidence.

## Authoring and presentation

`ScopedNodeTarget` names a node and every Repeat ancestor, with an explicit
Default or stable Play branch for each. Default follows the literal authored
child even when all current plays override it. It never invents an iteration.
An owned gap belongs to its stable play even when its final position suppresses
the gap's output. Implicit default gaps need their own future recipe controls.

The displayed picture has a separate concrete `InstancePath`. Navigation keeps
exact affine transforms and ancestor clips through Retime. Only root frames
whose centers actually sample the selected occurrence are offered for seeking.
The model caches its structural index, rows and selected projection. Play
counts stay compact; it does not create one widget or node per play.

A dormant or unsampled definition can still receive a typed gain value, Hold
audio edit or generated-picture acceptance. A proven dormant All plays Hold
also accepts duration changes. AI generation requires both local
definition boundaries. Its inspector reports the missing picture and retains the last
displayed image. Camera and comparison audition require a visible representative.
A bounded representative search does not prove that every other play is
unsampled; the UI distinguishes that case from a proven dormant default.

## Atomic edits and asynchronous ownership

`scoped_edit_requirements` returns exact fresh node/mark needs and recognizes
unchanged values. `EditScoped` accepts gain, framing, endpoint policy, Hold
audio, rename and `AcceptGeneratedHold` edits. Preparation returns the remapped authoring target
and can map an independently captured presentation. The host allocates
identities, validates media and commits through the same store command path.

`scoped_many_requirements` stages the selected edits in order so later targets
see earlier isolation and unchanged values need no new identities. Its private
temporary names avoid existing nodes, marks, retained audio aliases and every
pending target name. A missing target cannot become valid by naming a temporary
clone; no legal authored name is reserved for the query. Only the caller's real identity
pools enter the committed edit. The [preflight regression record](qualification/scoped-preflight-2026-10-12.md)
covers name collisions, nested remapping and native durable Undo/Redo.

Selective isolation preserves sparse overrides, retained clocks, concrete and
definition marks, and per-play sound permissions. Default ancestors are
wildcards only for actual uses of their shared definition. Duration and source
coordinates do not change in the value-edit command family. Scoped pause
insertion uses `InsertOccurrenceTime` with explicit `target: ScopedNodeTarget`
authority and a separate `within: InstancePath` presentation;
its query checks the captured boundary and remaps that instance through
outside-in isolation before the time insertion. Later allocations retain their
pre-edit sampling clocks.

Native requests capture session, project, revision, outer Sequence scope,
inspected root, authoring target, optional presentation and root cursor. The
service retains the mapped receipt before refreshing the workspace. The UI
consumes it only at its exact visible revision and cannot use a late receipt to
reclaim a different play, cursor or focused view. Undo, Redo and unrelated head
changes close stale nested navigation.
Navigation permanently revokes a pending edit's continuation, including moving
away and back to equal coordinates before its receipt arrives. The durable edit
and refreshed workspace remain valid.

Saving or deleting a mark preserves the current play, child, cursor and view.
Only the typed mark receipt's exact old-to-new revision transition permits that
rebase. Navigation made while the mark save is pending remains current; an old
receipt cannot revive navigation across another edit. A successful mark jump
explicitly leaves the scoped inspector.

Gain proposals and Commit preview and render preserve the same captured scope.
The unchanged initial Gain proposal gets a private preview revision without
isolation or a durable write. Camera compares the exact captured picture layer;
its preview cannot silently frame a different occurrence.

See [qualification and retained failures](qualification/scoped-plays-2026-10-03.md)
for test, rendered and native evidence, source provenance and remaining limits.

## Remaining work

Shared gap rows exist only with This play at every level; a shared gap
inside several selected plays, and its Camera, gain, picture provider or AI
pictures before isolation, remain unavailable. Multi-play selection is chosen only
by command; the inspector shows it but has no pointer control for it. This boundary does not implement occurrence-local cuts, replacement, Repeat
count changes, All-plays Retime changes, general semantic
recording of scoped navigation or arbitrary occurrence edits, or complete
definition previews. Those remain part of the
normative specification. No full-product requirement or gate is complete.
