# Editing Repeat contents

`Enter` opens a selected Repeat or Retime. Inside it, `j/k` selects children,
`Enter` descends and `Backspace` returns to the parent. On an existing Hold with
This play at every Repeat level, `Enter` opens its exact duration instead.
Enter also opens Hold duration inside fixed Edit Retime windows without Repeat
ancestry, with All plays explicit at every enclosing Repeat, or in the narrow
This play case of explicit Repeat bodies containing one fixed, unclocked Edit
Retime. Derived pause clocks retain their timing guard.
Pause insertion follows the explicit choice at each enclosing Repeat. Other
temporal edits still use the ordinary Sequence timeline.

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
below the multi-play level keep the choice; `:scope all`, `:scope play N` and
`]r`/`[r` end it. The choice survives its own commit, because the Repeat keeps
its identity and stable play IDs.

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
Mixed owned-gap Hold resizing, Repeat/Retime timing combinations, copy/paste
and general macros inside these contents remain required work;
their native actions refuse without editing the retained outer beat.

Freeze/black pauses, existing Hold duration and sound, and whole-owner Zoom and Creep with
Keep or OriginalCenter intent support recording and dot when This play is explicit at every enclosing
Repeat. Saved programs retain length/picture or framing intent; invocation
captures the current play and maps its authoring and presentation addresses
through isolation. A pause continues from its actual new Hold, keeping the
ordinary cursor and enclosing selection. A duration setter continues at the
resized Hold's exact start, even when shrinking past the old interior cursor.
Hold sound changes retain the absolute cursor and preserve picture and timing.
Only InsertPause, SetHoldDuration, SetHoldAudio, SetRoomTone, Zoom and nested
named calls are admitted. Scope navigation cancels recording and
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
while live counterparts may ripple. Implicit default gaps, Retime/Partition
ancestry and placed sounds remain unavailable. A single all-Play dormant gap
retains the existing live-boundary refusal.
See [selected duration semantics](SEMANTIC_MACROS.md#selected-plays-hold-length).

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

The This play and mixed fixed-Retime paths above require literal body choices
and one fixed unclocked Edit Retime below all enclosing Repeats. They do not broaden
selected-play or other Retime duration paths. See the
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

Implicit default gaps have no authored node, so neither `j` nor `]r` selects
them; they need their own recipe controls. Multi-play selection is chosen only
by command; the inspector shows it but has no pointer control for it. This boundary does not implement occurrence-local cuts, replacement, Repeat
count changes, Retime changes, implicit gap recipe editing, semantic recording
inside occurrences beyond freeze/black pauses and whole-owner Zoom intent, or complete definition previews. Those remain part of the
normative specification. No full-product requirement or gate is complete.
