# Native marks and jump history

Save a position with `m` followed by a letter; return with `'` followed by that
letter. Uppercase and lowercase are separate, giving 52 project marks. A pending
prefix shows the valid next keys and has no timeout. Escape cancels. Counts,
held second keys and conflicting prefixes cannot write or jump. Native text
entry, composition and global shortcuts retain their input.

`:mark a`, `:jump a` and `:unmark a` use the same revision-bound service commands.
`:marks` and the visible Marks button open the saved list. The list shows Original
or Your edit, saved/unresolved state, Jump and Remove actions. Tab moves through
the native controls; Enter activates a focused button. Save captures the position
where the list opened. Escape returns to the editor. `Ctrl O` and `Ctrl I` work
in both the workspace and list; native text keeps those keys outside the list.

## Identity and clocks

The shared authored mark map owns each letter at `native-mark-{letter}` with an
exact single-letter label. Selection uses that ID. The label confirms intentional
use of this namespace: an existing different label rejects Set, Jump and Delete
without overwriting the mark. A headless command may deliberately author the same
ID and label for parity. Copies retain their fresh mark IDs and do not steal
keyboard letters. Other named marks remain available through the typed API.

Original marks retain the registered asset and its exact measured video PTS,
including the measured terminal boundary. They do not select an arbitrary edited
occurrence or add a timeline beat. Native capture and resolution validate the
admitted source qualification and complete picture index. Normal asynchronous
picture admission still checks the media; the project writer does not decode it.

Edit marks retain a concrete host occurrence with exact local coordinates.
Capture requires an ordinary Sequence scope. At an interior seam it picks the
right-hand child; at the final boundary it picks the final child with left bias.
An explicitly selected empty child disambiguates equal-time siblings. Empty
groups retain their identity rather than an invented picture. Existing core
transforms follow Split, Move and Repeat wrapping. Lost content remains explicitly
unresolved until an authored change, including Undo, restores it.

Jump resolution retains exact fractions through Retime. The cursor displays the
ties-to-even project frame and reports the exact fraction when they differ. The
accessible ordinary Sequence ancestry determines the displayed group and selected
child. An inaccessible occurrence is inspected through its enclosing composite.
Missing, unresolved or ambiguous marks fail without moving either cursor.

## Saved metadata and asynchronous replies

Set and Remove each save one atomic undoable transaction. They preserve the
current pane, both clocks, selected beat and active Visual range. A successful
mark-only revision can rebase transient selections and navigation entries because
it changed no editorial content. The independent saved receipt survives later
queries and failures. A saved write followed by failed preview refresh reports
the durable result and asks the user to reopen; it never silently retries a write.

The service checks session, project, revision and a never-reused request ticket.
An identical successful retransmission returns its receipt; an altered payload or
stale ticket rejects. Prefix and command entry capture their position, including
missing-target errors. A later cursor, selection, pane or revision change cannot
supply a different capture. A delayed jump reply cannot steal subsequent
navigation. Generic selection-changing edit receipts are cleared on every mark
intent.

## Jump trail and current limits

`Ctrl O` returns to the departure of a successful mark jump; `Ctrl I` moves
forward. `:jump-back` and `:jump-forward` are equivalent. Each branch is bounded
to 128 entries. A new successful jump discards its forward branch. A failed jump
does not consume history. Entries retain exact positions, so reassigning a
letter cannot rewrite the trail. Changing pane alone preserves a retained exact
fractional position.

The trail is local to the open session. Edit entries require their captured
revision and expire after unrelated authored changes, including ordinary Undo
or Redo. Native mark-only saves explicitly rebase them. Original entries survive
edits while their asset qualification and measured boundary remain valid.
Expiration removes unavailable entries from both branches so they cannot block
older valid positions. Saved marks are the durable way to follow edited content.

This does not add structural-edit rebasing for transient history, history for
every motion, persistent named registers, motion/text-object operators or native
navigation inside Repeat/Retime occurrence scopes. DP-05 and DP-20 remain partial.
