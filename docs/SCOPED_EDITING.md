# Editing Repeat contents

`Enter` opens a selected Repeat or Retime. Inside it, `j/k` selects children,
`Enter` descends and `Backspace` returns to the parent. The ordinary Sequence
timeline remains the authority for temporal editing.

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

Gain, Camera and Hold audio use the captured scope. Changing a shared node in
This play isolates only the necessary selected ancestors, then applies the
value in the same reversible transaction. An unchanged value creates no
override or history entry. Timing edits, copy/paste and macros inside these
contents remain required work; their native actions currently refuse without
editing the retained outer beat.

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

A dormant or unsampled definition can still receive a typed gain value or Hold
audio edit. Its inspector reports the missing picture and retains the last
displayed image. Camera and comparison audition require a visible representative.
A bounded representative search does not prove that every other play is
unsampled; the UI distinguishes that case from a proven dormant default.

## Atomic edits and asynchronous ownership

`scoped_edit_requirements` returns exact fresh node/mark needs and recognizes
unchanged values. `EditScoped` accepts gain, framing, endpoint policy, Hold
audio and rename value edits. Preparation returns the remapped authoring target
and can map an independently captured presentation. The host allocates
identities, validates media and commits through the same store command path.

Selective isolation preserves sparse overrides, retained clocks, concrete and
definition marks, and per-play sound permissions. Default ancestors are
wildcards only for actual uses of their shared definition. Duration and source
coordinates do not change in this command family.

Native requests capture session, project, revision, outer Sequence scope,
inspected root, authoring target, optional presentation and root cursor. The
service retains the mapped receipt before refreshing the workspace. The UI
consumes it only at its exact visible revision and cannot use a late receipt to
reclaim a different play, cursor or focused view. Undo, Redo and unrelated head
changes close stale nested navigation.

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
inside occurrences or complete definition previews. Those remain part of the
normative specification. No full-product requirement or gate is complete.
