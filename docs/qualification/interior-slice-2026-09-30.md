# Interior slice placement, 2026-09-30

## Scope

Native `:splice` can preview and insert a linked Original range inside a direct
Source, ordinary Hold or supported transparent fragment of an ordinary Sequence.
The group and exact local boundary remain explicit. The core splits the target
and inserts the Source in one `SpliceSourceAt` command, preserving both retained
contexts and transforming placed sounds once. Preview creates no history;
Enter saves the exact proposal and one Undo removes the split and insertion.

Core schema 33 and database schema 42 remain unchanged because no document or
table field changed. Frozen command adapters reject the new tag. See
[the contract](../SLICE_PLACEMENT.md) and [headless command](../HEADLESS.md).

This extends the earlier [seam preview](slice-placement-2026-09-30.md). Edited
slice selection/move, replacement, picture/audio-only policies and Repeat/Retime
occurrence destinations remain required. No DP requirement or release gate is
closed by this increment.

## Automated verification

The locked workspace passes 2,609 tests, with none failed or ignored. The final
UI-feature app run also passes all 362 tests. Strict workspace and UI-feature
Clippy checks and workspace formatting pass. Focused
coverage includes direct and nested Source/Hold interiors, refinement of
existing fragments, marks and framing, exact identity/node budgets, old command
adapters, stable sound allowances and one sound ripple. Store and actor tests
cover source admission, stale targets, retained proposal IDs, exact retransmitted
commit receipts, failed SQLite cursor writes, reopen, Undo and Redo. An injected
commit failure leaves no preliminary Split revision.

Actual media tests compare independently decoded proposed and committed pictures
on both sides of both joins, including metadata and framing. Canonical PCM at
each join is nonzero and agrees exactly. The final 256 samples and limiter gains
match the preceding saved edit after its suffix shifts. Separate handwritten
NTSC phase oracles cover single-frame Source insertion, fragment refinement and
nonzero room-tone continuation; these would detect rounded sampling clocks.

The final production `place-slice` replay passes 267 checks. It exercises interior
preview and commit, both join pictures, exact Undo, counted boundary navigation,
raw native input batches, distinct empty-group slots, synthetic IME, complete
Tab circuits, stale picture/proposal replies and cancellation. The Kestrel audit
passes 104 router states × 62 reserved chords, or 6,448 cases.

Both final interior captures were checked against the picture-first design
contract at 960×640 and 1280×820. The decoded endpoint strip, destination picture,
exact boundary, local beat position, unsaved state and provisional interval are
visible. The final PNGs are byte-identical to the inspected passing captures.
The intermediate screenshot allowance was reached; semantic checks continued
and the named minimum/default checkpoints retained reserved capture capacity.

## Review corrections

The first production replay reproduced six failures from two keyboard defects:

- Clamped `h/l` changed a deliberately chosen slot to the first slot sharing
  its numeric boundary. Both leading and trailing empty Sequences reproduced
  the wrong insertion order. The fix retains the slot when the boundary does
  not move; commit and Undo checks verify the resulting child order.
- A native event batch could lose repeated count digits or its final motion.
  Raw `1,1,l` and `1,2,h` batches both stayed at boundary 30 instead of reaching
  41 and 18. The router now removes each recognized press once and processes
  the complete ordered batch. Tab, pointer transitions, IME and focused native
  buttons retain their input ownership.

A follow-up review proposed that an earlier `u` could leak into the editor when
cancelling. Both raw `u,Escape` at the heading and `u,Enter` on the focused Cancel
button passed before any cancellation change. Pinned egui replaces raw input
events on layout retries; the static review had assumed they survived. That
finding was dismissed and the regressions were retained. Independent core/audio
and store/actor reviews found no additional actionable defect.

An initial test-module declaration used the wrong relative module directory;
the explicit `moment/interior.rs` path corrected it before compilation. Debug
linking reports the toolchain's oversized `__eh_frame` warning. No suppression
or unrelated build setting was added.

## Native release verification

The optimized app was bundled separately as
`dev.thiesen.deadpan.interior-slice-qa` and opened a disposable retained replay
project. The user's existing window and desktop were untouched. Keyboard input
copied Original `[10,24)`, opened `:splice` at Edit boundary 30, inspected both
joins, committed once and undid once. Stable one-based display labels matched:

| Displayed Edit frame | Displayed Original frame | Fixture slate |
| --- | --- | --- |
| 30 | 30 | 029 |
| 31 | 11 | 010 |
| 44 | 24 | 023 |
| 45 | 31 | 030 |

Commit showed 134 frames and five root children; Undo restored 120 frames and
three children. SQLite backup snapshots confirm exactly one `splice_source_at`
edit at local boundary 30 followed by one Undo, with every authored field
restored except its fresh revision. Only history, revisions, cursor state and
redo changed; the other 16 tables were identical. Shutdown released the writer
lock. The first comparison script used the wrong JSON discriminator key;
changing `type` to the actual `command` key completed the command assertion.

Release playback visibly advanced and paused/resumed without a displayed
starvation error. A complete loop wrap was not conclusively captured in this
pass, and no acoustic claim is made. Native accessibility inspection listed
the slice controls. It does not establish VoiceOver acceptance or resolve the
earlier Render-overlay omission. One separately sampled AX-caption/screenshot
pair showed the preceding slate; a subsequent read matched. This was not a
coherent single-frame capture proving a presentation defect. The native report
retains that observation. Native screenshots were inspected inline; retained
PNG files come from the offscreen production replay.

## Evidence and limits

The source fixture is `cfr-bframes.mp4`, 30000/1001 fps, decoded through the pinned
LGPL FFmpeg prefix on Apple M5 Max, 128 GiB, macOS 26.5.2, Metal and Rust 1.97.1.
Report metadata records the base commit, diff digest and complete source-file
manifest. Actual PCM checks do not qualify acoustics or device delivery, and
offscreen input does not qualify physical IME or VoiceOver. The earlier measured
debug audio starvation remains a limitation; these checks do not establish the
full project-size, effects, HDR or performance matrix.

[Retained reports and captures](../../tools/media-qualification/evidence/2026-09-30-interior-slice/)
include failed and successful replays.
