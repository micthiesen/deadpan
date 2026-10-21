# Editing Repeat contents

`Enter` opens a selected Repeat or Retime. Inside it, `j/k` selects children,
`Enter` descends and `Backspace` returns to the parent. On an existing Hold with
This play at every Repeat level, `Enter` opens its exact duration instead.
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
the selected insertion parent; black pauses need no sampled picture. Other
timing edits beyond one-play Hold duration, copy/paste and general macros inside
these contents remain required work;
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
level. The inspector's Change duration control and Enter prefill its current
length. A no-op refuses without history, isolation or cursor movement.
All plays, mixed choices, multiple plays, Retime ancestry and root or attached
sound events remain unavailable for this timing edit. Accepted generated
picture shortening retains its provider; extension beyond accepted coverage
saves the deterministic fallback and durable preparation for a new explicit
candidate acceptance. The [Hold duration qualification](qualification/scoped-hold-duration-2026-10-20.md)
retains the nested-play, real-PCM, history and native verification.

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
audio edit or generated-picture acceptance. AI generation requires both local
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
