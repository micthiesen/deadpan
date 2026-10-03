# Editing Repeat contents

`Enter` opens a selected Repeat or Retime. Inside it, `j/k` selects children,
`Enter` descends and `Backspace` returns to the parent. The ordinary Sequence
timeline remains the authority for temporal editing.

Choose `:scope all` to edit a Repeat's shared definition or `:scope play N` to
edit one stable play by its current one-based number. These choices apply at the
nearest displayed Repeat level. Nested Repeat choices remain separate. All
plays preserves existing overrides. Browsing never creates an override.

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

This boundary does not implement occurrence-local cuts, replacement, Repeat
count changes, Retime changes, implicit gap recipe editing, semantic recording
inside occurrences or complete definition previews. Those remain part of the
normative specification. No full-product requirement or gate is complete.
