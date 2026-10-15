# Pauses inside pitch-preserving speed changes, 2026-10-15

Scope: `InsertTime` strictly inside a Preserve or Shift Edit Retime through the
ordinary split path, as described in
[the contract](../INSERT_TIME.md#pauses-inside-pitch-preserving-speed-changes).
Core and decoded-PCM evidence on one machine; no requirement or gate is newly
complete.

## Environment

Apple M5 Max, arm64, macOS 26.5.2; source tree at `cb053cf9` plus this change;
pinned FFmpeg prefix `ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3`.

## Behaviour checked

Core (`crates/deadpan-core/tests/insert_time/retimed.rs`), a 12-frame Source
under a Preserve speed change of 24 frames:

- A 3-frame pause at project frame 5 splits the stage at output frame 3: the
  left window shows `[0,3)` of the original stage, the right window `[3,24)` of
  a complete copy with the same recipe; the Hold sits between them; total 31;
  no derived clocks.
- With static framing on the stage, the target reports `lift = slow`, picture
  capture stops below it, and a new group holds both halves and the pause with
  the stage's framing; the stage and its copy lose it.
- An inner group with an animated framing envelope refuses with "moving
  framing". `pause_insertion_target` reports `Ordinary` for these stages; a too
  small identity pool still fails atomically.

Audio (`a_pause_inside_a_pitch_preserving_speed_change_resumes_its_processed_output`):
decoded `pcm-mono-44100.wav` at 30000/1001 fps through the canonical Preserve
processor, a 4-frame Original stretched to 8 frames. A 2-frame pause at frame 3
keeps every earlier sample, returns suppressed silence over frames 3–4 and
resumes the old processed stream bit-identically from the old frame-3 boundary,
in raw and edge-faded reads, both plain and with a −6 dB gain lifted to the
enclosing group.

## Not established

Native app interaction for these pauses (the shared target query and command
are used), playback through a device, export, and processing cost: each pause
adds one full copy of the stage's processing.
