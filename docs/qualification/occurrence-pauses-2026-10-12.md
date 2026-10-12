# Pauses inside Repeat, 2026-10-12

Native and macro pauses can now insert integer-frame time into one concrete
Repeat play or gap, including nested Repeats. The Repeat keeps its stable play
identities and remains the live enclosing owner. Inserting one frame grows the
project by one frame, regardless of the play count.

`pause_insertion_target` supplies exact isolation, mark, Split, container and
gap identity requirements. `InsertOccurrenceTime` isolates shared branches
outside-in before capturing temporal placement. An implicit gap becomes an
owned Hold with its retained clock and permissions. Existing Sequence seams
keep their owner; Repeat seams choose the left branch, matching the frozen
picture immediately before the insertion point. A transparent Sequence contains
the pause where a Repeat previously owned a physical child directly.

Every later physical allocation retains its own pre-edit sample entry. Native
freeze capture matches the complete insertion-parent instance, including Repeat
ancestry. It captures lower composition only; the parent's framing, escalation
and ancestor operations remain live. AI insertion creates one durable preparation
for the allocated Hold in its selected branch. Ordinary `InsertTime` retains
its existing admission rules and wire form.

## Verification

Base `feee242c89cbe6f58fe22aa0e1c82edb965f8c55`; Apple M5 Max, 18 cores,
128 GiB, macOS 26.5.2 (25F84), locked dependencies and Rust 1.97.1.
The primary implemented and self-reviewed alone. No native window,
browser or audio device was used.

The core boundary matrix covers each integer interior in plain and nested
Repeats, including play seams, implicit gap interiors, existing owned branches,
forward/inverse patches and JSON round trips. Additional cases cover concrete
mark movement, copied owned marks, gap permissions, malformed identity pools,
checked timing exhaustion and `u32::MAX` plays without expansion. Macro replay
retains the pause cursor and AI intent. Store checks exercise a concrete AI
preparation through reopen, Undo and Redo with fresh intent.

Real 44.1 kHz WAV reads at 30000/1001 fps check every play and room-tone gap
allocation against independently rounded frame boundaries. Extra allocation
samples are zero after retained support ends; chunk sizes cannot change output.
Attached sounds cover fresh/saved clocks, dry audio and event/owner EQ, comparing
interiors away from intentional silence-gate fades. A separate root-sound/EQ
oracle checks continuous retained sample history through a permitted gap and
silence throughout the newly inserted time.

The native service test imports a real MP4, adds Source and Repeat camera
curves, freezes one play and verifies source PTS, captured lower layers, live
Repeat framing, unchanged other pictures and durable SQLite Undo/Redo/reopen.
This uses the headless service without opening a GUI or audio device.

The final combined core, render-plan, audio, playback and store run passed
2,899 tests with zero failed or ignored. Strict workspace all-target Clippy
with `deadpan-app/ui-harness` passed before the final root-bus fix; strict
all-target core/audio Clippy passed on the final source. All eight native pause
tests pass, including the existing Source, VFR, nested Sequence and black-pause
cases. The debug link retains the existing `__eh_frame` size warning.

The new public-CLI export fixture uses registered MP4/WAV media, copied attached
sound with event/owner EQ, two plays and a silent gap. A pause splits the first
play and another splits its gap. Four independently read pre-edit limited-PCM
windows retain their values within `1e-6`. Both debug and release MP4s pass all
15 expected pictures and 31 audio blocks, with zero measured offset and minimum
block SNR 15.9505 dB. No timing, picture or fidelity threshold was changed.
The final release app and CLI build and formatting check pass. Six independently
addressed 256-sample authored-PCM windows are bit-identical between the saved
debug CLI and final release CLI, including two wholly silent inserted windows.
A fresh final-release verification again passes every emitted picture and audio
window without changing the authored document or executable identity.

The [retained evidence](../../tools/audio-qualification/evidence/2026-10-12-occurrence-pauses/manifest.json)
includes source, fixture and executed-binary hashes; all command receipts and
logs, including failed runs; the source patch and self-review; both projects and
emitted movies; complete verification reports; and the PCM comparison samples.
The [test counts](../../tools/audio-qualification/evidence/2026-10-12-occurrence-pauses/test-counts.json)
and [source identity](../../tools/audio-qualification/evidence/2026-10-12-occurrence-pauses/identity.json)
bind this evidence to the tested implementation.

## Defects caught during verification

At an unsplit Repeat-child edge, adding a transparent container initially reused
the attached sound's pre-edit timing identity for a different structural layout.
The command refused with `sound clock identity names a different structural
layout`. The new placement graph now gets the next checked ordinal. The saved
and fresh attached-PCM regressions pass with that fix.

Materializing an implicit gap initially passed independent root sound events
to a physical-only binding capture, which correctly refused them. Events and
routes now detach only during that zero-time preparatory step and restore before
the actual insertion ripple. Core permissions and real root-sound PCM regressions
pass, including explicit silence in the new Hold.

One expanded test first failed to compile because `apply` had two glob-imported
meanings; it now explicitly calls `crate::apply`. The permission fixture initially
exceeded its owner's sound interval under `Reject`; the fixture was shortened
to fit. These corrections did not weaken validation or comparison thresholds.
The failed runs are retained with the passing results.

## Remaining scope

Authored Retime interiors and fractional insertion points still need exact
derived-clock authoring. Native temporal edits from the scoped inspector,
selected Original-moment payloads and freezing accepted footage remain open.
Root-owned attached sounds and changed complete processing scopes retain their
existing explicit refusals. Physical listening, native-window interaction and
performance budgets are not established by these checks. No DP requirement or
delivery gate is newly complete.
