# Native project workspace

The application now connects the existing storage, measured import and picture
plan boundaries to a native project workflow. It creates and opens `.deadpan`
packages, registers managed or linked originals, inserts an entire source by
explicit command, saves undo/redo, and inspects exact source or sequence frames.
Playback, range operators, generated-provider rendering and export remain open.

## Project and media ownership

`project::ProjectService` owns one writable `ProjectStore` on its service thread.
The UI uses a bounded command mailbox and takes immutable `Arc<Workspace>` updates
without blocking on a mutex. Each workspace contains the committed document,
compiled picture plan, qualified source catalog and history availability.
Failed create/open operations preserve the prior project. Reopening the same
package reuses its writer session. Closing or switching cancels the old import
and revokes its original handle before releasing the writer. Quit waits for an admitted project command to finish;
background preparation does not block shutdown. New commands are rejected once
shutdown starts.

A separate persistent import thread performs original retention, private
snapshot creation, selected-stream decoding and receipt preparation. There is
one active preparation and one cached complete-file token. Video import retains
its measured available audio; failure to qualify selected audio fails the import.
The current audio-file option selects stream zero. The complete stream picker,
format matrix, relink dialog and security-scoped bookmarks are not implemented.
Linked media therefore must remain reachable at its recorded path.

Import registers the source without changing the sequence. Explicit insertion
uses the caller's current revision, parent and insertion index, and the shared
store transaction path. The first primary video insertion may select measured
presentation cadence and geometry under the existing automatic-basis policy.
Undo restores that policy along with the source beat.

A cached source can be inserted while another import prepares. An uncached source
requires the import lane; its captured revision and target are preserved through
preparation, and subsequent edits cause stale rejection rather than retargeting.
Undo/redo remain available during preparation. Active-generation edits that need
unimplemented source-context resolution fail explicitly through the store's
relevance guard. They are not admitted with invented context hashes.

Every successful insertion reports its actual committed revision and node ID.
The completion marker survives coalesced background progress, registration and
stale worker replies until the next user command. UI selection follows this
marker, not a progress label or an assumed index. Errors survive unrelated
background progress. User assets and accepted objects retain their existing
storage policies; cancellation never treats originals as disposable caches.

## Picture and keyboard behavior

The preview worker has its own bounded latest-request mailbox. Each project
request carries an immutable workspace and explicit Source or Sequence cursor.
Opening a source checks the original content, selected stream, interpretation and
measured frame index against its receipt. A retained decoder is reused only
within the same session/asset/receipt, with catalog validation repeated when the
immutable catalog entry changes. Clearing or closing cancels pending work and
releases the decoder on its owning worker, without a UI-thread join.

Sequence inspection uses `RenderPlan::picture` and the exact measured source
index, including source span endpoint policy, retiming, repeats and freeze Holds.
Blank/background plans display deterministic black. Empty sequence frame zero
has an explicit empty state. Unsupported still/generated providers report an
error rather than silently substituting footage. Project canvas aspect and the
shared SDR renderer determine displayed geometry. No audio is played.

The cursor is a boundary in `[0, duration]`; the frame to its right is displayed,
with the final preceding frame shown at the end. Sources appear on the left,
sequence beats below the picture, and mode/context/boundary status beneath them.
Source and beat widgets are virtualized; their derived lists are rebuilt when
the document or search changes, not every redraw. Keyboard selection reveals the
selected row. The current strip presents root beats, not a complete nested editor.

Normal bindings include counts, `h/l`, `j/k`, `gg/G`, native arrows/Home/End,
`u`/Ctrl-R and native Command shortcuts. Prefixes do not time out. Logical
punctuation and digit keys accept layout modifiers; physical positions are not
used. Text editing retains native shortcuts, and composition suppresses editing
bindings. Real pane focus targets intercept Tab and arrows; text Escape is
handled after the text widget processes same-frame input. The partial binding
vocabulary is not the full declarative operator/visual/camera/trim grammar.

`⌘N` creates, `⌘O` opens, `⌘I` imports, and `⌘Return` inserts after the selected
root beat or at sequence end. `/` searches; `:` opens command entry with
`insert`, `undo`, `redo`, `new`, `open`, `import`, `source`, `sequence`, `help`.
Native panels are constructed on the main app thread and polled through a retained
future/waker. One panel may be open at a time, and an active import disables another import
chooser. Create always adds the `.deadpan` suffix while preserving an authored
name or earlier suffix. Import captures its project session
before opening the panel and rejects a result for a different session.

[Qualification](qualification/native-workspace-2026-09-21.md) records the actual
service, decoder, keyboard/focus and native interaction checks and their limits.
