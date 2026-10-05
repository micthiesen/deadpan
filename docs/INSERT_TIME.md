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

Core 24 adds existing root Sequence seams before composite suffixes: Sequence,
Repeat, Retime, owned gap branches and generated Hold owners can move together.
Each physical or default-gap owner receives one compact chronological reanchor
step in the pre-edit root window. Its placement is captured from the current
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

## Remaining full-product work

Current insertion supports existing root seams before arbitrary composite
suffixes and interiors of Source or ordinary Background/Freeze Hold beats when
followed by either physical fragments or composite structures, including inside
unretimed Sequence groups. Interior cuts beneath Repeat/Retime ancestors, fractional cuts,
outside-in occurrence isolation and selected Original-moment payloads remain
required. Arbitrary insertion must inherit its enclosing
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
