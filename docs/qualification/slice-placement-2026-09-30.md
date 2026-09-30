# Visual slice placement, 2026-09-30

## Scope

The native `:splice` workspace previews linked Original copies at ordinary
Sequence child seams. It shows decoded source endpoints beside the destination,
local In/Out refinement, exact destination controls, a provisional interval,
Before/Proposed comparison, and an explicit one-transaction commit. Escape
preserves the saved edit and copied range. See [the contract](../SLICE_PLACEMENT.md).

This is partial §9.7. Frame-interior placement, edited-slice copy/move,
replacement, picture/audio-only policies and Repeat/Retime occurrence targets
remain required. No DP requirement or release gate is closed by this increment.

## Verification

The locked workspace run passed 2,592 tests. The final UI-feature app run passed
358 tests after the picture-entry and focus regressions were added. Strict
workspace/UI Clippy, the diagnostic example's Clippy and formatting passed.
A real-media integration test
previews Original frames `[0,30)` at Edit boundary 30 and commits the retained
proposal. Independently reopened committed media matches the proposed decoded
pictures at both joins, their metadata/framing, and canonical stereo PCM.
All four tested audio halves contain nonzero samples, so silence cannot make
the equivalence check pass trivially. The unsaved revision is absent from SQLite
before the explicit commit.

The production UI replay passes 215 checks covering widget Tab/Shift+Tab
circuits, focused button activation, synthetic IME ownership, exact source endpoints and both joins,
stale preparation and destination revisions, cancellation, one commit and undo.
Paint checks inspect text clips and submitted image meshes at 960×640 and
1280×820; both final captures were visually inspected. The Original copy/paste
regression replay also passes its 16 checks. Simulated delivery verifies routing and cursor ownership; it does not
establish device performance or acoustic quality.

The shortcut audit compares the production router with the local Kestrel source:
104 state cases × 62 reserved chords, or 6,448 checks.

## Native observations

A separate disposable Slice QA app left the user's existing Deadpan window,
desktop and size untouched. Native accessibility exposed the slice controls
and endpoint labels. Keyboard range refinement, Before/Proposed comparison,
Tab/Enter and Escape cancellation were exercised. All 20 SQLite table counts
were unchanged after the two cancelled runs; the QA app was quit afterward.

The debug build did not sustain audition: pictures advanced approximately one
8,192-sample prefill per restart, and a later loop reported `Starved`. The first
pass overlapped workspace tests. At 48 kHz that prefill lasts 170.667 ms, or
5.115 frames at 30000/1001. The following isolated measurements establish that
unoptimized preparation cannot keep up even on the saved edit.

The read-only `qualify_audio_stream` example then read eight consecutive
8,192-sample blocks through two fresh persistent `OfflineAudioSession`s and one
retained-session repeat. With no competing builds, debug refills took
1,065.50–1,194.28 ms. Release refills took 21.21–31.12 ms. All PCM and limiter-gain
hashes matched within and across profiles. This establishes sufficient preparation
margin for this saved fixture in release; it does not measure device delivery
or larger projects and effects.

With builds and tests stopped, a separate release QA app sustained the proposed
join loop: observed pictures advanced from frame 4 to 18 and wrapped to 1 across
two 2.5-second intervals. Space changed Pause to Audition; after the final
in-flight picture, the image stayed fixed across a separate 1.2-second check.
Resume and a second pause passed, as did Space on the focused Audition/Pause
button. No `Starved` or other error appeared. This verifies visible native
delivery and controls for this fixture, without an acoustic claim.

Escape returned to the saved edit at boundary 0/120. Every row hash and count
in all 20 database tables matched consistent before/after SQLite backups.
The release QA app quit and released its writer lock. The user's existing
Cursor QA window remained running in its assigned desktop and size.

## Corrections and limits

Initial checks found an inherited horizontal layout collapsing endpoint
pictures. Explicit vertical child layouts fixed it. A first equivalence fixture
sampled silent intervals; the corrected joins deliberately cover retained
impulses and tails while preserving the nonzero assertions. Focused button
audition passed; a later replay expectation was corrected to restore the
destination picture after that extra audition.

Review also found that opening from active playback at an interior destination
could revoke an in-flight picture without requesting the captured saved frame.
Opening Place slice now explicitly requests that frame. Cancellation uses the
last issued proposal identity, even when a newer local refinement was not sent.

Hardware: Apple M5 Max, 128 GiB, macOS 26.5.2, Metal, Rust 1.97.1 and the pinned
LGPL FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`. Native observations are not
physical IME, VoiceOver, acoustic, HDR or release performance qualification.
Command reports retain the base commit, tracked diff digest and a source
manifest covering new files as well as tracked files.

[Retained reports and source manifests](../../tools/media-qualification/evidence/2026-09-30-slice-placement/)
include the initial failures alongside successful reruns.
