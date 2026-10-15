# Editing projects with exact speed-change clocks, 2026-10-15

Scope: ordinary commands, host queries and the native workspace on documents
that contain derived clocks, as described in
[the contract](../INSERT_TIME.md#editing-documents-with-exact-clocks), plus
pauses inside split slowed beats. Core, plan, audio, store and app service
evidence on one machine; no requirement or gate is newly complete.

## Environment

Apple M5 Max, arm64, macOS 26.5.2; source tree at `ac5dd61a` plus this change;
pinned FFmpeg prefix `ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3`.

## Behaviour checked

Core (`crates/deadpan-core/tests/insert_time/retimed.rs`), on a 12-frame Source
slowed to 24 frames with a fractional pause already inside it:

- Pauses at root seams before and after the speed change use the ordinary
  path, move it whole and keep its clocks; the target query reports
  `Ordinary` there.
- `SetHoldDuration` on a Hold outside the speed change works; on the pause
  inside it, the command refuses with "inside a speed change".
- `Split` of the speed change at frame 10 keeps the exact total and copies its
  clocked content (twice as many clocks); a later pause inside the right copy
  goes through its transparent window, which grows with the pause.
- `Group` keeps the clocks and gives the group the exact extent 29;
  `DeleteRipple` of the speed change removes its clocks.
- Every committed case round-trips through JSON and its exact inverse patch.

Audio (`a_pause_in_a_split_slowed_beat_and_at_root_seams_keeps_every_other_sample`,
decoded `pcm-mono-44100.wav` at 30000/1001 fps): a pause inside the copied half
of a split slowed beat keeps every earlier sample, inserts suppressed silence
and resumes the old stream bit-identically; on a document with exact clocks, a
pause at the project start resumes each owner on its own rounded boundary
(old 6406 maps to new 9610, not to a constant offset).

App service (`native_pause_inside_a_tape_speed_change_keeps_exact_clocks_and_one_undo`):
on the real `cfr-bframes.mp4` project, slowing the Original to half speed with
tape pitch and inserting a 3-frame black pause at frame 31 (child frame 15.5)
commits, refreshes the workspace with exact clocks, shows black exactly over
frames 31–33 and the old pictures three frames later, undoes in one step, and a
freeze pause at the same point captures a Freeze picture.
`derived_clocks_open_in_the_native_workspace_without_authored_changes` opens a
clocked package without writing.

## Commands

Focused runs of the tests above; full `cargo test` for core, plan, store, audio,
playback and app with `--no-fail-fast`; strict Clippy for core, plan, audio,
store, CLI and app, all targets; `cargo fmt --check`. Results are listed in
the commit message's validation note and the handoff.

## Not established

Native editing inside a clocked speed change (scoped inspector timing and Trim
inspection report refusals), Preserve/Shift output routes, Repeat ancestors,
marks and sounds on clocked documents, playback through a device and export of
a clocked project.
