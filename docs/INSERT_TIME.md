# Pause insertion

`Command::InsertTime` adds an ordinary Hold at one project-frame boundary in a
single reversible transaction. Native `,h` inserts a half-second freeze with
silence; a count scales the duration, so `3,h` inserts 1.5 seconds. `:hold` accepts
exact frames, milliseconds, seconds and clock notation, rounds once with
ties-to-even, and displays the resolved frame count before submission. The
inserted pause stays selected at its start for picture inspection and parameter
editing. The Original and its protected undo baseline remain intact.

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

The native service resolves the freeze from its immutable picture plan and
registered measured source index: the frame left of the cursor, or the first
frame at project start. It records that frame's original PTS and time base, not
a project-frame number or inferred cadence. Background/audio-only content uses
Background. Session and revision guards apply before preparation and commit.

## Persistence and refusals

Core 17/database 23 admit the new command. Database 22 replays through a frozen
core-16 adapter retaining its exact binding and command vocabulary. Old history
cannot acquire InsertTime commands or invented timing records. Migration uses a
consistent copy, preserves the backup and validates the complete chronology.

Zero native duration produces a message without submitting an edit or allocating
history. The core/headless command rejects zero with an explicit no-change
message; it never publishes an empty transaction. Negative time, stale context,
identity exhaustion and unsupported structures fail atomically.

## Remaining full-product work

Current insertion supports root boundaries and interiors of Source or ordinary
Background/Freeze Hold beats, including existing transparent fragments. Every
shifted root beat must be one of those forms. Nonempty Repeat gaps are rejected
even in the prefix because compact gap binding ownership is not implemented.
Nested/repeated/retimed/generated shifted structures require compact entry
dispatch and scope-aware insertion. Arbitrary insertion must inherit its enclosing
group's ownership. These are required follow-ups, not reduced V1 scope.

Active generation requests still require genuine host relevance observations;
the native service preserves the store's refusal while that resolver is absent.
It does not fabricate unresolved observations. Freezing accepted footage,
requesting/auditioning AI candidates, audio-policy controls, transport, listening
qualification and export remain separate required work. Generic structural
Insert remains available for its existing child-index semantics; it does not
implement this splice contract.
