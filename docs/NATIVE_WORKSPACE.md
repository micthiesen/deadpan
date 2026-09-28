# Native project workspace

The application now connects the existing storage, measured import and picture
plan boundaries to a native project workflow. New chooses one video, creates a
`.deadpan` package in Documents/Deadpan and initializes the complete original
timeline automatically. Your edit evolves through reversible changes; Original
stays pinned for browsing and deliberate reuse. A separate sound catalog admits
external audio without changing picture or duration. Generic/legacy packages
retain their broader register/insert workflow. See [the profile contract](SINGLE_ORIGINAL.md).
It also navigates ordinary [Sequence groups](GROUP_NAVIGATION.md), splits their
direct child beats at the cursor, inserts silent freezes through
[Insert Time](INSERT_TIME.md), wraps or updates Repeats, deletes beats and changes
an existing selected Hold's duration. [Pause qualification](qualification/insert-time-2026-09-24.md)
records exact input, measured frame selection and native focus review.
[Structural speed editing](RETIME_EDITING.md) adds exact `:retime` and
`:wrap-retime` with an explicit preserve/tape pitch choice, a resolved-duration
preview and one reversible command. Existing Retime parameters remain editable.
[Original and edit audition](PLAYBACK.md) adds Space Play/Pause and Shift+Space
selection loops with adjustable context, canonical pre-master audio and
device-clock pictures. Full mastering, range operators,
generated-provider rendering and export remain open.

The footer measures its current content before the frame is presented. An egui
layout retry resolves changed wrapping or a command/Camera mode transition in
the same frame, with at most three passes. Native text consumes its final input
before command closure; service, playback and dialog updates are consumed once
per outer frame. Repeat continuation and picture scheduling wait for the final
pass, and discarded geometry does not resize the GPU target. Pending Camera
entry waits for a later repaint if the picture only becomes ready in a sizing
retry. Newly focused command/search fields initialize their Escape filter in a
same-frame retry, preserving text even when Escape is the next native input.
Keycaps and their labels reserve their complete measured width together,
so narrow layouts wrap the pair instead of clipping the last shortcuts.
See the [footer qualification](qualification/footer-layout-2026-09-27.md).

## Project and media ownership

`project::ProjectService` owns one writable `ProjectStore` on its service thread.
The UI uses a bounded command mailbox and takes immutable `Arc<Workspace>` updates
without blocking on a mutex. Each workspace contains the committed document,
compiled picture plan, qualified source catalog and history availability.
Failed create/open operations preserve the prior project. Supported older schemas
upgrade through the store's consistent-backup, validated migration on the service
thread. Only a successfully opened and validated candidate replaces the current
session; failed migration retains the old active project and import. Generic
projects stay generic and the original database backup remains available.
Reopening the same
package reuses its writer session. Closing or switching cancels the old import
and revokes its original handle before releasing the writer. Quit waits for an admitted project command to finish;
background preparation does not block shutdown. New commands are rejected once
shutdown starts.

A separate persistent import thread performs original retention, private
snapshot creation, selected-stream decoding and receipt preparation. There is
one active preparation and one cached complete-file token. Original preparation retains
its measured available audio; failure to qualify selected audio fails the import.
Sound import selects the first actual audio stream through bounded container
admission, including an audio track after a video track. Explicit stream selection
remains available in the backend. The complete stream picker,
format matrix, relink dialog and security-scoped bookmarks are not implemented.
Linked media therefore must remain reachable at its recorded path.

Initialization commits the qualified Original, measured presentation basis and
full-source beat with a durable baseline in one transaction. Undo cannot cross
that baseline; deleting every current beat does not unlock another video. An
incomplete package stays Awaiting Source and can retry. New resolves the system
Documents directory on the service thread, never cwd. Initial picker cancellation
creates no package. Bounded names and exclusive creation prevent overwrite.

Sound import registers audio without changing the sequence or stealing the
Original selection and cursor. The focused catalog supports separate selection,
`j/k`, Space play/pause/resume and Shift+Space whole-sound loops, with its own
elapsed/total clock and visible key hints. [Sound audition](PLAYBACK.md#sound-catalog-audition)
uses canonical playback and measured sample endpoints without moving either
editor cursor or requesting picture changes. Explicit [root sound placement](SOUND_EVENTS.md#native-root-placement)
uses `,s` or `:sound-place` to place the selected catalog sound's complete measured
span at the retained Your edit cursor. It leaves picture duration unchanged and
rejects overflow instead of trimming or stretching the sound. The existing shared
audio bus mixes placed events before its single limiter. Legacy import also
retains registration-only semantics.
Explicit whole-original reuse (or generic source insertion)
uses the caller's current revision, parent and insertion index, and the shared
store transaction path. The first primary video insertion may select measured
presentation cadence and geometry under the existing automatic-basis policy.
Generic insertion undo restores that policy along with the source beat. A V1
baseline is fixed after initialization and retains its Original identity.

A cached source can be inserted while another import prepares. An uncached source
requires the import lane; its captured revision and target are preserved through
preparation, and subsequent edits cause stale rejection rather than retargeting.
Undo/redo remain available during preparation. Active-generation edits that need
unimplemented source-context resolution fail explicitly through the store's
relevance guard. They are not admitted with invented context hashes.

Beat edits capture the writer session, revision, cursor and Sequence scope. The service resolves
the typed intent against that immutable document, allocates identities and commits
through the same core/store command path. Only direct children of the active
Sequence may be targeted. Repeat/Retime descendants require occurrence-aware
selection and remain unavailable in native navigation. Repeat
setters preserve an existing gap; explicit wrapping creates a new Repeat even
when the selected beat is already one. Deletion selects the next sibling, otherwise
the previous sibling, otherwise explicitly clears selection.

Rapid explicit wraps (`rr` / counted `rr`) retain up to sixteen waiting intents
in the UI, in addition to one submitted edit. Each creates a separate nested
Repeat and undo step. The next request is constructed only after consuming the
matching committed wrapper, with its fresh revision, selected node and scope.
An idle writer alone never authorizes continuation. The mailbox stays bounded
to one command and its normal revision guards remain mandatory.

The notice shows the waiting count and Escape hint. Escape, another resolved
action, navigation, pointer context change, a modal, window blur or close cancels
waiting wraps and reports their count; the submitted edit may finish. Overflow
is explicitly counted. Errors and mismatching completions discard waiting work.
The next partial operator/count survives the chain's own matching completions.
Repeat setters and unrelated edits are not queued or silently retargeted.
These session-local intents are not persisted recovery work.

Every successful insertion or beat edit reports its actual committed revision,
captured scope and resulting selected node, including an explicit empty selection.
The completion marker survives coalesced background progress, registration and
stale worker replies until the next user command. UI selection follows this
marker, not a progress label or an assumed index. Errors survive unrelated
background progress. User assets and accepted objects retain their existing
storage policies; cancellation never treats originals as disposable caches.
Registration updates the available source without switching the active viewer
away from Sequence, including after history commands clear a completion marker.

Placed sounds have a separate pane and event selection. `:sounds` or pane
navigation focuses the list, `j/k` selects an event, and `h/l` moves an unrouted
event by exact project frames. Counts compose on its durable mapping without
accumulating rounded sample durations. Enter opens exact position entry in whole,
nonnegative 48 kHz samples. `+` / `-` changes the event by 3 dB per step;
`:sound-gain -3.125` sets exact gain within -96 through +24 dB. Soft/Hard controls
and `:sound-edges soft|hard` change both endpoints, and `dd` or `:sound-delete`
removes the selected event. These controls never change monitor or master gain.
Ordinary undo/redo applies to each committed sound edit.

Sound parameter entry captures the selected event, project session and revision.
A stale or missing target fails instead of following an asynchronous completion
to another sound. The list keeps catalog audition and the selected structural
beat distinct. Gain and edge edits preserve routed events' chronological cuts;
absolute moves and frame nudges reject routed events explicitly. Silent pauses
suppress sound and the inspector describes that policy without offering an
unimplemented allowance control. Nested owners, Repeat/Retime sound transforms,
custom silence allowances, effects and export remain open. See the
[native sound placement qualification record](qualification/native-sound-placement-2026-09-27.md)
for verification scope and limits.

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
shared SDR renderer determine displayed geometry. Audition uses the same picture
path and a separate canonical audio preparation worker. Ordinary stopped
inspection remains silent. Explicit Original playback follows its measured PTS
and full A/V union, without moving the edit cursor or changing project history.

Presentation keeps the requested position, accepted decoded picture and displayed
picture separate. Each identity retains the project session/revision and explicit
Source or Sequence coordinate. An accepted picture waiting for the GPU remains
available when a newer request starts; a late reply for an older request is still
rejected. The caption below the viewer and its accessibility name describe the
submitted picture, while the bottom status bar describes the requested boundary
and any pending update. Identical original pixels at two sequence positions have
different presentation identities. This is submission ordering, not a measured
physical display timestamp.

A replacement render target is promoted only after successful submission, keeping
the prior picture and caption intact on allocation/render failure. Current decode
failure clears the old image and says it is unavailable. Picture errors are scoped
to presentation; a later successful decode clears an earlier render failure
without erasing unrelated project/action errors. Render failure stops automatic
retries until a fresh request or decode arrives. Backgrounds explicitly replace
textures, and an empty sequence has no invented source frame. The
[presentation qualification](qualification/preview-presentation-2026-09-21.md)
records regression, native appearance and keyboard evidence.

The cursor is a boundary in `[0, duration]`; the frame to its right is displayed,
with the final preceding frame shown at the end. One Original and the separate
sound catalog appear on the left, editable beat cards below the picture, and
mode/context/focus/boundary plus contextual keycaps beneath them. Your edit and
Original label the internal Sequence and Source contexts. Original/edit duration
comparisons use project frames; Original browsing counts measured video frames.
These counts can differ for VFR or offset media and are labeled separately.
Source and beat widgets are virtualized; their derived lists are rebuilt when
the document or search changes, not every redraw. Keyboard selection reveals the
selected row. Sequence cursor motion selects the current-depth beat to its right, or the
last beat at the final boundary. Compact cards have explicit half-open frame
boundaries and a local yellow cursor; equal widths do not imply equal durations.
Enter opens a selected ordinary Sequence group; Backspace returns to its parent.
Breadcrumbs show the current depth, and the footer separates the group-relative
position from the absolute edit cursor. Empty groups remain navigable. A conditional
inspector describes the selected child and offers real command-based parameters.
See [group navigation](GROUP_NAVIGATION.md) for scope and asynchronous completion.

Normal bindings include counts, `h/l`, `j/k`, `gg/G`, native arrows/Home/End,
`u`/Ctrl-R and native Command shortcuts. Prefixes do not time out. Logical
punctuation and digit keys accept layout modifiers; physical positions are not
used. Text editing retains native shortcuts, and composition suppresses editing
bindings. Real pane focus targets intercept Tab and arrows; text Escape is
handled after the text widget processes same-frame input. The partial binding
vocabulary is not the full declarative operator/visual/camera/trim grammar.
Clicking away cancels command entry after widget processing. Visible command
mode suppresses normal edit keys even if focus has already moved in that frame.
Pointer-button input batches defer shortcut and command submission routing until
widgets resolve their focus changes. Text still reaches the widgets; ordinary
key-only navigation and hovering are unaffected.

`⌘N` chooses a new Original, `⌘O` opens an existing project, and `⌘I` adds sound
in a Ready V1 project or chooses the Original for an incomplete project. Legacy
projects retain generic import. `,i` reuses the whole Original after the
selected child or at the current group's end. `/` searches; `?` opens keyboard help;
`:` opens command entry with
`insert`, `split`, `undo`, `redo`, `new`, `open`, `import`, `source`, `sequence`, `enter`, `parent`, `help`,
`sound-place`, `sounds`, `sound-at`, `sound-gain`, `sound-edges`, `sound-delete`.
In Original, `v` starts/finishes a half-open temporal range, `h/l` adjusts its
head, and `y` copies it without history. The inspector displays In, excluded Out,
original ordinal count and measured duration; the bar uses measured PTS. Return
to Your edit and use `p`/`P` after/before its selected direct child. An empty group
accepts paste at its start. `:select`, `:yank`, `:paste` and `:paste-before` expose
the same actions. The session-local copy survives selection cancellation and
ordinary edits, but not session or source receipt changes. Named/persistent
registers and Visual replacement remain open. See [Original moments](SOURCE_MOMENTS.md).

Native panels are constructed on the main app thread and polled through a retained
future/waker. One panel may be open at a time, and an active import disables another import
chooser. Import captures its project session and revision before opening the
panel and rejects a result for a different session or stale authored context.
The File menu and contextual footer show only the current action's meaning.

The Keys window keeps its scrolling instructions visible above the reference.
`j/k`, arrows, Page Up/Down and Home/End scroll from its measured position,
including after wheel input. Help owns keys and text until Escape; a command
that follows Escape in the same input batch reaches command entry in order.
Opening help mid-batch immediately transfers ownership to it. Native dialogs
and popup menus retain priority, including their Escape and IME handling.

In Sequence context, `rr` wraps the selected child in two total plays;
`3rr` makes three total plays and `1rr` retains one. `dd` deletes that child.
Operator prefixes remain pending without a timer. Unsupported deletion counts,
zero/overflow counts and conflicting post-operator counts fail explicitly.
Held-key autorepeat cannot complete an edit operator. Changing context, pane or
selection cancels the pending operator. Source context remains non-destructive
and teaches browsing, returning to Your edit and explicit reuse instead.

`s` and `:split` cut inside the selected child at the current project-frame
boundary. Both linked roles retain their original timing and full processing
context. The right fragment is selected from the committed structure, keeping
the cursor at the cut. The footer, inspector button and help teach the binding;
the inspector button is disabled at existing boundaries. Counted Split and extra
command arguments fail explicitly. Repeated cuts refine sibling fragments rather
than building a deeper tree. See [Split](STRUCTURAL_SPLIT.md).

Command entry accepts `:repeat N`, `:wrap-repeat N`, `:delete`, and
`:hold-duration Nf`. Repeat updates an existing selected Repeat or wraps another
beat; wrap-repeat always wraps. Hold duration requires a selected existing Hold
and an explicit positive integer frame unit. Arguments and unsupported options
are rejected, not silently ignored. This command subset does not yet implement
the specification's full typed-unit, selector and completion grammar.

`,h` inserts a 0.5-second frozen silent pause at the cursor. Counts multiply that
duration: `3,h` inserts 1.5 seconds. `:hold 12f`, `:hold 250ms`, `:hold 1.5s` and
`:hold 01:02.500` use exact time input and show the resolved frame count. Seconds
round once with ties-to-even; zero creates no history. The inspector and footer
teach the shortcut, and Choose pause duration opens the command entry. The new
cursor stays at the pause's committed start, even if navigation occurred while
the command was pending. A direct child Hold stays selected; Inspector Enter
edits its duration. A deeper pause selects its visible enclosing group; Enter
opens that group so the Hold can be selected and edited.
The service freezes a measured original frame and its existing cropped view,
then commits one atomic splice. The new Hold inherits its actual Sequence
parent and all ancestor framing once, retaining only lower crops. Camera can
adjust a selected pause independently;
its inspector explains that Reset keeps the captured view.
See [pause insertion](INSERT_TIME.md) for resume semantics and supported scope.

The pause command supports root Source and ordinary Hold interiors and their
fragments, plus existing root seams before composite suffixes, including Repeats
and their gap branches. It also supports these cuts inside unretimed Sequence
groups without flattening them. Insertion beneath Repeat or Retime ancestors,
Repeat gaps, occurrence navigation, range edits, beat gain controls, semantic
dot-repeat and macros remain open. Unsupported interior boundaries fail without
an edit. Existing child-index Insert
retains its distinct semantics. [Splice design prerequisites](STRUCTURAL_SPLICE_DESIGN.md)
record the wider scope still required.

`,f` opens a temporary [Camera draft](FRAMING.md) on the selected direct child in
Your edit. The stopped picture previews source-relative `h/j/k/l` movement,
counted zoom steps and numbered center/corner targets. Numeric fields use canvas
percentages, preserve native text editing and expose validation before Apply.
Enter applies one edit and keeps the cursor; Escape restores the entry framing.
Ordinary Camera changes move the complete existing curve. Reset replaces it;
`,z` creates a static 1.35× punch and `,c` creates a whole-beat smoothstep creep.
The normal inspector distinguishes static framing from whole-beat motion.
Saved regions, tracking and native occurrence targets remain open.
[Captured views](CAPTURED_FRAMING.md) retain existing framing when a native pause
is inserted. Still and accepted-generated providers remain unsupported for this
native capture path.

Use the [UI feedback loop](UI_FEEDBACK.md) for current keyboard, mouse, layout and
picture review. Replay production inputs through the real application, inspect
the visual artifacts and run separate release timing when responsiveness can
change. The [interaction review](INTERACTION_REVIEW.md) records personal shortcut
compatibility and remaining improvements. Offscreen checks do not establish
native dialogs, IME, VoiceOver, global shortcut delivery or physical presentation.

[Qualification](qualification/native-workspace-2026-09-21.md) records the actual
service, decoder, keyboard/focus and native interaction checks and their limits.
The [root editing qualification](qualification/native-editing-2026-09-23.md)
records Repeat, Delete, Hold-duration, completion-selection and focus evidence.
The [single-Original qualification](qualification/single-original-2026-09-23.md)
records the revised creation flow, protected baseline, separate sound catalog,
design comparison and current verification limits.
