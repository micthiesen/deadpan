# Pause insertion qualification, 2026-09-24

This increment builds on `7277f29fe1ae96df5900d8871435f9961f697869` and adds
[atomic pause insertion](../INSERT_TIME.md), exact native duration entry and
current-duration audio support on retained clocks. Core 17/database 23 freeze
the prior core-16 command grammar. The base commit's
[CI run](https://github.com/micthiesen/deadpan/actions/runs/35966375560) passed.

## Timing, history and media evidence

Core and real decoded-audio tests exercise start/interior/end insertion, prior
Split seams, every shifted fragment, repeated pauses and nonzero fractional
resume. At 30000/1001 fps, the new sample 3203 resumes the old sample 1602 after
a one-frame seam insertion. Partitioned reads equal whole reads. Source and
sequence marks retain their distinct clocks; inverse patches restore fixtures.
Empty automatic projects acquire a timed basis with the first Hold. Unsupported
structure, zero, stale context and exhausted identities cannot publish an edit.

Lengthening a moved RoomTone Hold preserves its binding, phase and captured
Repeat origins while exposing its current tail and relocating its end fade.
Actual PCM matches an independent current-duration oracle. Prefix equality stops
before the old resampler support halo, where newly available input can legitimately
change output. A former old-support fade expectation was corrected: removing an
old enclosing crop exposes the current body, while a still-owned Edit crop
continues to restrict support. Tests assert both cases with exact PointCeil grids.

Native service tests use the qualified VFR fixture to verify the actual measured
freeze PTS, prefix/suffix picture identity, Original baseline, reopen and one-step
undo/redo. They do not substitute project-frame numbers for source timestamps.

The [compatibility archive](../../tools/audio-qualification/evidence/2026-09-24-insert-time/compatibility-evidence.tar.gz)
contains a DB22/core16 fixture created by the actual baseline's core/store APIs,
with no version-tag downgrade. The isolated baseline build used Rust 1.97.1 and
verified 2,127 tracked files against its Git archive. Its CLI validated the
fixture before migration. Current migration preserves all six revision documents,
both history requests/edits, state, pending redo and table definitions except
intended schema tags. The backup remains DB22 with an identical SQL dump.
Four raw/faded PCM windows match old output. This compatibility fixture is a
SilentHold, so those reads prove admission and compatibility, not nonzero DSP.
Separate decoded-source and RoomTone tests provide nonzero signal evidence.

## Native interaction and appearance

A native test project created from the qualified CFR fixture lives under the
system Documents/Deadpan folder and starts with its complete 120-frame Original.
At boundary 37, `,h` shows a pending-key hint and inserts one 15-frame pause;
the viewer displays burned-in source frame 036. `:hold 250ms` resolves visibly
to seven frames, and `3,h` inserts 45 frames. Undo/redo changes one insertion at
a time. Zero makes no history entry; Original-view insertion is refused.

The saved [single-Original design targets](../design/README.md) were compared
with the running app. The picture remains dominant, the Original stays pinned,
and muted panes, lavender selection and the yellow boundary distinguish context.
Visible keycaps explain pause insertion and navigation. Review moved the existing
duration action beside its value so it remains above the inspector fold.

Live accessibility activation exposed a crash: an inspector button focused the
command field after that frame's footer had already omitted it. Focus now waits
for the frame that emits the field. Headless UI tests enable AccessKit and assert
that the focused node exists in every emitted tree. The rebuilt native app's
duration button and exact-pause link both opened focused text fields; typing,
applying a 19-frame duration, undoing to 15 and cancelling worked. A final focus
check also corrected the originating pane: Escape now returns to the inspector,
and Return immediately reopens its duration field. The review
process exited cleanly. Physical IME/non-US hardware, VoiceOver and listening
were not qualified by these checks.

## Repository gate and review

The final gate passed on macOS 26.5.2 (25F84), Apple M5 Max, Rust 1.97.1,
using the selected pinned compatible LGPL FFmpeg prefix: formatting, locked
workspace/all-target Clippy with warnings denied, locked workspace tests/build,
CLI doctor and native Metal startup/shutdown. **1,204 tests passed, zero failed,
zero ignored.** All 376 source and fixture hashes remained unchanged throughout
the gate. No source edits followed it. The native executable tested for the final
focus fix has the same hash as the final checked build.

[Evidence](../../tools/audio-qualification/evidence/2026-09-24-insert-time/summary.json)
retains counts, [command results](../../tools/audio-qualification/evidence/2026-09-24-insert-time/report.json),
[source hashes](../../tools/audio-qualification/evidence/2026-09-24-insert-time/source-hashes.json),
six compressed logs, the gate script, compiler/host metadata and
[native observations](../../tools/audio-qualification/evidence/2026-09-24-insert-time/native-review.json).
Eight saved image/prompt records, five original spec archives and changed Markdown
links passed verification.

Independent review covers core timing, storage/migration and the full change.
The logical comma leader was moved ahead of generic modified-key rejection so
Shift/Option layouts can use it without enabling Command/Control combinations.
Current headless API documentation was updated for schema 17 and InsertTime.
A proposed missing final validation finding was dismissed because the public
command path's duration traversal already performs complete validation.

## Remaining scope

Insertion currently accepts root Source or ordinary Background/Freeze Hold
beats and transparent fragments; all shifted beats must satisfy that scope.
Nested/repeated/retimed/generated shifted structures, nonempty Repeat-gap
ownership and enclosing-group scope remain required. Active generation requests
still need genuine relevance observations, and accepted-footage freezing remains
unavailable. Audio-policy editing, arbitrary ranges, playback, AI workflow,
export and every open requirement/gate remain required. This is a tested editing
increment, not a completed product or a reduced V1 specification.
