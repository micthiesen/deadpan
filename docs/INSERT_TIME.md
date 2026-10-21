# Pause insertion

`Command::InsertTime` adds an ordinary Hold at one project-frame boundary in a
single reversible transaction. Native `,h` inserts a half-second freeze with
silence; a count scales the duration, so `3,h` inserts 1.5 seconds. `:hold` accepts
exact frames, milliseconds, seconds and clock notation, rounds once with
ties-to-even, and displays the resolved frame count before submission. The
cursor stays at the inserted pause. Root Holds remain selected for parameter
editing; a pause inside nested Sequences keeps its visible root group selected.
Nested inspector navigation remains required. The Original and its protected
undo baseline remain intact.

## Authored timing

The command carries an exact boundary, Hold recipe, fresh Hold identity, bounded
Split identity pool and timing identity allocated by its new revision. It
captures the pre-edit sampling clocks, optionally performs a transparent Split,
composes resume phases for every shifted physical allocation, and inserts the
Hold. Intermediate structures never become history revisions. Marks follow the
existing Split and Insert transforms; source clocks and sequence-pinned marks
retain their own semantics.

Reanchor existing fragments as well as newly split ones. At 30000/1001 fps, a
two-frame Source split at frame 1 has a right-hand entry at sample 1602. Inserting
one frame at that seam must map new sample 3203 to old sample 1602, with unit
sample step. Recomputing from the new absolute position would resume at 1601.
The final extra allocation sample is zero after the retained envelope exhausts.
Subsequent pauses compose differences on the then-current clock. They do not
recapture the raw recipe or accumulate rounded durations.

Bindings retain sampling phase and historical placement. Current owned recipes
retain their meaningful extent, so lengthening a captured RoomTone Hold exposes
its new tail without resetting phase or moving captured Repeat origins. Current
authored crop constraints and source placement still constrain raw reads.
Creative fade geometry follows the current body on the retained clock.

Ordinary `SetHoldDuration` also captures affected physical owners on their old
root clocks when resizing moves output time. Shared Repeat definitions stay
compact, and existing partial bindings retain their phase. No-op, terminal and
dormant-definition changes avoid unnecessary captures. The separate
clocked-Retime path is unchanged. Ordinary Holds inside fixed affine Retime
windows also retain moved interior clocks while outer placements stay fixed;
processing Preserve/Shift ancestors remain outside that qualification. See the
[ordinary Hold-duration clock qualification](qualification/hold-duration-clocks-2026-10-20.md).
The [fixed-window qualification](qualification/fixed-retime-hold-clocks-2026-10-20.md)
records exact nested slopes, cropped support and retained allocation endpoints.

Core 24 adds existing root Sequence seams before composite suffixes: Sequence,
Repeat, Retime, owned gap branches and generated Hold owners can move together.
Each physical or default-gap owner receives one compact chronological reanchor
step in the pre-edit root window, unless that step is provably inert
([timing storage](TIMING_STORAGE.md#inert-reanchor-steps)). Its placement is captured from the current
document even when the owner's lattice and prior steps are older. A partial
Repeat fragment and later complete plays therefore resume at their own visible
entries. Hidden allocations remain absent; newly born gaps keep their canonical
birth dispatch. No command expands the play count.

Movement stops at the first nonunity Preserve output. Its intrinsic preparation
history stays on the retained input clock and is not shifted a second time.
The old reducer remains in use for suffixes it already admitted, preserving
their exact transaction output. Only the newly admitted composite path uses
the generalized placement capture.

Core 25 also admits a root Source, ordinary Hold or supported transparent
fragment interior before a composite suffix. The sampling lattice is captured
before the internal Split; the current placement graph is captured afterward,
with a second consecutive checked timing ordinal. Copied physical aliases belong
to the second graph, while their inherited lattices still name the first. At
30000/1001 fps, inserting one frame inside a two-frame Source resumes its right
part from old sample 1602 at new sample 3203. The following Repeat resumes old
sample 3203 at new sample 4805. Each entry needs its own rounding; a common suffix
sample offset would change the audio. Existing reducer paths remain unchanged.

Core 26 also descends through strict interiors of unretimed Sequence groups.
`ProjectDocument::insert_time_target` supplies the same parent, child slot and
exact Split identity count to the native host and the command. An existing seam
stays at its Sequence level; a strict interior descends until that seam or a
supported physical Source/Hold fragment. The new Hold is a child of that actual
Sequence. Every ancestor remains live, and later siblings at every enclosing
Sequence level receive their own current-clock reanchor. Repeat suffixes remain
compact, including a billion-play suffix, and later Preserve inputs stay on
their intrinsic clocks. This does not admit a Repeat or Retime ancestor.

`Command::InsertOccurrenceTime` now adds a pause at an integer boundary inside
one concrete Repeat play or gap. The shared `pause_insertion_target` query
returns the exact isolation, Split, container and gap identity requirements.
Isolation runs outside-in before temporal capture and preserves stable play
identities. An implicit gap becomes an owned Hold, retaining its sound clock and
permissions. A direct Repeat child receives a transparent Sequence when needed;
the Repeat remains the live enclosing owner. No play-count expansion occurs.
Existing Sequence seams keep their level; a Repeat seam chooses its left branch,
matching the picture sampled immediately before the insertion boundary.

The scoped inspector supplies `within: InstancePath` to name one concrete
occurrence. `pause_insertion_within` starts there and admits both its edges,
so insertion at the beginning of play two cannot fall into play one. The
command remaps the address through its own isolation before resolving the
insertion again. Native requests retain the session, revision, scope and cursor;
the saved receipt selects the new Hold within the same stable play. Retime
ancestors are refused for this occurrence path.

Explicit `target: ScopedNodeTarget` authority keeps each ancestor's Default or
Play choice independently of the concrete `within` presentation. The command
rejects competing legacy `all_plays: true` authority. The first Default ancestor
is the shared edit root, qualified by any concrete Play prefix. Only Default
levels multiply inserted time, excluding overridden uses; Play levels contribute
one. An outer three-play Default with an inner two-play Play adds three pause
lengths; outer Play with inner Default adds two. Root sound ripple occurs at the
shared root's old concrete end. Every moved owner keeps its own sample clock.

`pause_insertion_scoped` returns exact fresh identity needs for that authority.
`prepare_occurrence_time` returns the actual mapped inserted Hold, its concrete
presentation and exact final start, plus revision-bound address mapping methods.
It resolves the new Hold's local zero; looking for it at the old root cursor
would fail when earlier default occurrences have already grown.

Native `:scope plays` prepares one bounded Compound for distinct plays at one
Repeat level under the same prefix. Each selected owner gets
its own qualified freeze or black recipe at the retained local boundary. Later
addresses and earlier inserted Holds map through each actual isolation stage;
root positions are resolved again from local anchors. The receipt retains every
mapped Hold, including differing child slots, with one durable history step and
one Undo. Shared and multiple-play AI insertion remains unavailable.

With This play explicit at every Repeat level, native freeze/black insertion
uses `InsertPause { length, black }` semantic intent for recording and dot.
Each invocation resolves its own exact boundary; the saved program retains no
node, play or sampled picture. `PauseSite::ScopedBoundary` carries the concrete
presentation to qualified freeze capture. Black bypasses that provider.
The planner uses `prepare_occurrence_time`'s actual Hold continuation for a
following pause, Zoom or named call, while preserving the ordinary cursor and
enclosing selection. A late refusal rolls back the complete run.

An existing Hold in one explicit Repeat play also accepts
`SetHoldDuration { length }` semantic intent through `:hold-duration Nf`.
`occurrence_hold_duration_requirements` queries exact temporal isolation pools;
`prepare_occurrence_hold_duration` retains the actual execution proof, mapped
Hold and concrete presentation. Its local-zero anchor supplies the final
cursor. Resizing does not sample the old cursor to guess a new target, insert
another pause or change the ordinary enclosing selection. Mixed/multiple
scope choices, Retime ancestry and external sound events remain refused for
this resize path.

Repeated Hold resizing captures still-unbound downstream audio after occurrence
isolation and before changing duration. The old concrete Hold end bounds the
suffix reanchor, including later ancestor plays and gaps. Existing lattices
retain their phase; previously unbound Sources keep their pre-edit origins.
Rounding each new root position into a fresh Source clock would change samples
at fractional frame/sample rates, even though the picture timing was correct.

Mixed choices and multiple selected plays retain their direct native path;
they cannot supply semantic invocation authority. Scoped AI, reverse/tail
recording, Retime ancestors and Visual selection remain unsupported on this
path. See [one-play pause intent](SEMANTIC_MACROS.md#one-play-pause-intent).

With `all_plays: true` the same command inserts into the Repeat's shared
definition instead (specification §5.2: default edits affect every iteration),
so every default play grows and overridden plays keep their own branches. It
admits default plays at every Repeat level with ordinary Sequences around and
between the Repeats (a nested definition grows in every play of every enclosing
Repeat), a cut inside the innermost definition (never a gap) shown through any
default occurrence, and no AI preparation
(generation needs one effective occurrence). Nothing is isolated: the command
captures unbound lattices under its timing identity, Splits the definition
leaf in place (wrapping a direct Repeat child in a Sequence when needed),
reanchors every owner inside the outermost Repeat from the earliest default
occurrence's cut to that Repeat's end on its own pre-edit clock, then the suffix after the Repeat,
exactly as Repeat gap changes do. Decoded 44.1 kHz Source and room-tone gap PCM
keeps every play, gap and the tail sample-exact on the NTSC grid
(`an_all_plays_pause_keeps_every_play_and_gap_on_its_own_44100_clock`, and for
nested Repeats entered through the first or last occurrence,
`an_all_plays_pause_in_nested_repeats_keeps_every_occurrence_on_its_own_clock`); core
tests cover the structure, exact inverse, an isolated play left unchanged and
refusals. The scoped inspector's All plays scope (`:scope all`) inserts through
it (`scoped-plays` replay).

The command captures every later play, gap and enclosing Sequence sibling on
its own pre-edit sampling clock. The inserted time grows only the selected
occurrence. Native pauses and semantic macro pauses share the target query;
AI insertion records one durable preparation for the newly allocated Hold in
that concrete branch. The ordinary `InsertTime` command retains its previous
admission rules. Authored Retime interiors still require exact derived clocks.

The native service resolves the freeze from its immutable picture plan and
registered measured source index: the frame left of the cursor, or the first
frame at project start. It records that frame's original PTS and time base, not
a project-frame number or inferred cadence. Background/audio-only content uses
Background. Session and revision guards apply before preparation and commit.

`:hold 12f video=black` (black-frame punctuation, specification §8.2) uses the
same boundary, Split identities, sample reanchors and history as a freeze, but
authors `HoldVideo::Background` with no sampled picture or captured framing.
`audio=silence` is accepted and is the only choice at insertion; room tone is
chosen afterwards. Macros record it as `InsertPause { length, black: true }`,
which samples no picture (the field is omitted for freeze pauses). Covered by
`black_pause_inserts_background_picture_and_silence_as_one_undo`,
`a_black_pause_needs_no_picture_resolver_and_keeps_its_wire_form` and the
`zoom` replay.

[Captured framing](CAPTURED_FRAMING.md) retains the spatial composition entering
the chosen insertion Sequence as well as that exact source frame. Descendant camera curves
become their sampled static poses, with every intermediate crop preserved. The
parent and all its ancestors remain live on the inserted Hold and apply once. The
pause's own framing remains independently editable through core commands and
native Camera at any navigable ordinary Sequence depth. Resetting Hold framing keeps its
captured crop. Repeating a pause at the same canvas does not accumulate redundant
capture stages. No source media or rendered preview pixels are duplicated.

## Persistence and refusals

Core 17/database 23 admit the new command. Database 22 replays through a frozen
core-16 adapter retaining its exact binding and command vocabulary. Old history
cannot acquire InsertTime commands or invented timing records. Migration uses a
consistent copy, preserves the backup and validates the complete chronology.

Core 24/database 30 mark the broader contextual admission. Database schemas
through 29 validate the pre-edit document against the old Source/Hold suffix
restriction before applying a replayed command. Closed JSON vocabulary and exact
patch comparison alone cannot reject a forged old history that claims a formerly
unsupported command. Frozen core 23 retains its gap branches and detached gap
clocks; migration adds no invented edits. A preserved core-23 CLI supplies a
genuine database-29 history with edits, undo, redo and pending redo.

Core 25/database 31 freeze the next contextual boundary. Database 30 replays
through frozen core 24, admitting its existing root seams while rejecting an
interior cut before a composite suffix. Earlier databases retain their stricter
physical-suffix check. An authentic database-30 fixture retains composite edits,
abandoned history and pending redo; matching forged interior histories are
rejected before promotion.

Zero native duration produces a message without submitting an edit or allocating
history. The core/headless command rejects zero with an explicit no-change
message; it never publishes an empty transaction. Negative time, stale context,
identity exhaustion and unsupported structures fail atomically.

Core 26/database 32 freeze the nested Sequence admission boundary. Database 31
uses the closed core-25 adapter, retaining root physical interiors and refusing
nested Sequence interiors or seams. The genuine old-binary fixture preserves
26 revisions, 12 commands and pending redo. Matching modern transactions forged
as old history are rejected before promotion; their original and backup survive.

## Pauses inside FollowSpeed speed changes

`InsertTime` at a strict interior of a FollowSpeed Edit Retime, reached through
ordinary Sequences from the root, keeps that Retime as the live owner of the
pause ([qualification](qualification/retimed-pauses-2026-10-15.md)). Its slope
`s` (child frames per output frame) is unchanged: a cut at output frame `k`
lands at child coordinate `start + k*s`, and a pause of `h` project frames
occupies `h*s` child frames. Neither value is rounded. A physical leaf is cut
by transparent left/right windows over the original and a copy (an untreated
window is refined in place), a single beat below the Retime gains a container
Sequence, and a non-unity stage wraps the authored Hold in a FollowSpeed
adapter mapping its `h` frames onto `h*s` child frames. The Hold therefore keeps
its requested project duration, and later speed edits retime it with its group.
Integral results keep ordinary recipes; fractional windows become
[derived clocks](STRUCTURAL_SPLICE_DESIGN.md#derived-owner-clocks). Every value
stays exact through later pauses in the same speed change.

Sampling lattices are captured before the copy, and each moved owner, inside
or after the speed change, receives one chronological reanchor step in the
pre-edit root window, exactly as at root interiors. At 30000/1001 fps, a pause
inside a 2x-slow FollowSpeed beat keeps every earlier sample, adds exact
suppressed silence, and resumes the old sample stream bit-identically after it,
at whole and fractional child cuts and after a second pause.

`pause_insertion_target` returns `PauseInsertionTarget::Retimed` with the live
picture parent, the identity count and `needs_exact_clocks(frames)`. The
semantic planner, CLI and native `,h`/black pauses use it. Transparent windows
above the speed change (a split slowed beat) pass their unity selection through
and grow with the pause. Nested FollowSpeed stages and transparent windows
between them also retain their live ownership. Planning maps the cut through
each stage's exact child window and multiplies its slope into the insertion-site
clock. A pause of `h` project frames grows each stage's output by `h` times its
enclosing slope, and its child window by `h` times its cumulative input slope.
Intermediate fractions remain exact even when the net speed is unity. The Hold
keeps its authored `h` frames through one adapter at the insertion site. Later
siblings at every enclosing Sequence keep their individual pre-edit sample
entries; one rounded sample offset for the whole suffix would be incorrect.
Further pauses may enter the resulting windows or the first pause's adapter.
See [nested-speed qualification](qualification/nested-speed-pauses-2026-10-18.md).

Repeat ancestors, nested pitch-processing stages, AI pauses, pauses other than
a silent freeze or black picture, marks and
placed sounds still refuse, the latter two already in the target query.

## Pauses inside pitch-preserving speed changes

A Preserve or Shift speed change processes its whole input as one DSP history,
so the pause cannot enter its child. Instead the ordinary path splits the stage
at the pause's output frame: `Split` keeps the left half on the original stage
and windows a complete copy for the right half, each with its full processing
context, and the Hold goes between them
([qualification](qualification/preserve-pauses-2026-10-15.md)). Every value is
an integral output frame, so no derived clocks appear, and marks, sounds and
ordinary commands keep their usual transforms. The right copy's retained clock
resumes its processed output at the old cut, exactly as a split Source does.
When the stage has framing, gain or edges, a new group takes them around both
halves and the pause, so they apply once and stay live over it; the stage and
its copy lose them, and picture capture stops below the stage
(`InsertTimeTarget::lift`). A treated stage below a transparent window and a
group inside the stage with moving framing refuse: the pause sits beside the
stage's copies, so neither could stay live over it. Each pause adds one more
full processing copy, and a later speed change applies to each half separately.

## Editing documents with exact clocks

A fractional Retime keeps an integral recipe at the ceiling of its exact clock,
so outside every clocked speed change integral recipes equal exact extents
([qualification](qualification/clocked-editing-2026-10-15.md)). Ordinary
commands therefore run on such documents inside a sealed command scope
(`derived_clock::with_sealed`), where integral duration queries return those
recipes after checking that equality. After each command leaf,
`sealed_preserved` refuses any result that changed content or timing inside a
clocked speed change, unless the leaf was a retimed pause: sealed content may
stay unchanged, be removed, or be copied exactly, and the owning Retime may
change only attributes outside its timing. Copies carry their derived clocks
(`clone_nodes`; an owned Preserve input refuses), and removed Retimes drop
theirs. Pauses at root seams, Splits of the speed change, Hold edits elsewhere,
deletion, grouping and Undo/Redo work. Inside the speed change,
`SetHoldDuration` takes an exact path: the authored difference propagates
through every enclosing adapter, Sequence, speed change and window at its own
slope (clocks that become whole return to ordinary recipes), and later owners
only translate, which leaves their retained sample clocks unchanged. A Hold
only partly visible through a window, or under a Repeat, refuses. Edits that
change only labels, framing, gain, edges, attachments or a Hold's picture and
sound providers work inside it, since the check compares timing structure only;
other timing edits inside it refuse with a message naming it. The native workspace opens these projects; its
remaining integral consumers (scoped inspector timing, Trim inspection) report
the refusal rather than guessing.

## Remaining full-product work

Current insertion supports existing root seams before arbitrary composite
suffixes and interiors of Source or ordinary Background/Freeze Hold beats when
followed by either physical fragments or composite structures, including inside
unretimed Sequence groups, FollowSpeed speed changes and concrete Repeat plays or
gaps. Interior cuts beneath Preserve or Shift Retime ancestors and selected
Original-moment payloads remain required. Native scoped-inspector temporal intent also remains guarded;
the new path addresses the concrete cursor from ordinary Sequence navigation.
Arbitrary insertion must inherit its enclosing
group's ownership. These are required follow-ups, not reduced V1 scope.

Active generation requests still require genuine host relevance observations;
the native service preserves the store's refusal while that resolver is absent.
It does not fabricate unresolved observations. Freezing accepted footage,
requesting/auditioning AI candidates, audio-policy controls, transport, listening
qualification and export remain separate required work. Generic structural
Insert remains available for its existing child-index semantics; it does not
implement this splice contract.

[Composite insertion qualification](qualification/composite-insertion-2026-09-26.md)
records command, PCM, picture and migration tests, native service checks, and the
blocked Metal replay separately.

[Interior insertion qualification](qualification/interior-insertion-2026-09-26.md)
records the separate pre/post-Split clock checks, native VFR service, authentic
database-30 migration, public headless history and the blocked keyboard replay.

[Nested Sequence qualification](qualification/nested-sequence-2026-09-26.md)
records actual parent ownership, ancestor suffix clocks, native capture and
completion, database-31 replay, CLI history and the new keyboard scenario.
