# Pauses inside FollowSpeed speed changes, 2026-10-15

Scope: `Command::InsertTime` at a strict interior of a FollowSpeed Edit Retime,
the [contract](../INSERT_TIME.md#pauses-inside-followspeed-speed-changes), and
the admission change that lets retained audio bindings and lineage coexist
with affine derived clocks. This is core, plan, audio and store evidence on
one machine. It is not native workspace, playback-device, listening or
export qualification, and no requirement or gate is newly complete.

## Environment

- Apple M5 Max, arm64, macOS 26.5.2.
- Source tree at `2b8c1869` plus this change; pinned FFmpeg prefix
  `ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3`.

## Behaviour checked

Fixture: a 12-frame Source slowed to 24 project frames (child slope 1/2)
between two 2-frame Sources at 30000/1001 fps.

| Case | Result |
| --- | --- |
| Pause of 2 at frame 6 (child cut 2, span 1) | Ordinary recipes only: windows `[0,2)` and `[2,12)`, a container below the Retime, a 1-frame adapter over the 2-frame Hold; Retime `[0,13)` over 26 frames. |
| Pause of 3 at frame 5 (child cut 3/2, span 3/2) | Derived clocks: left `3/2`, right `[3/2,12)`, adapter `3/2` over `[0,3)`, Retime 27 over `[0,27/2)`; exact root 31; recipe root also 31. |
| Second pause of 2 at frame 22 | The right window is refined in place to `[3/2,17/2)` with a new `[17/2,12)` window; the adapter is ordinary; root 33. |
| Pause at a seam inside a grouped child | Only the adapter is created; exact Retime clock; root 29. |
| Outside the speed change after exact clocks exist, Preserve stages, a Tone pause, a marked project | Refused, the last in the target query. |

Every committed case round-trips through JSON and its exact inverse patch.

## Audio and pictures

- `pauses_inside_follow_speed_resume_the_old_samples_at_integral_and_fractional_cuts`
  decodes `pcm-mono-44100.wav` through the canonical resampler for a 4-frame
  Original slowed to 8 frames. For one- and two-frame pauses at frame 3 (child
  cut 3/2), raw and edge-faded reads keep every earlier sample, return exact
  suppressed silence over the pause and resume the old sample stream
  bit-identically after it, at several chunkings. A further pause after the
  first resumes on the composed clock.
- `retained_lattices_on_fractional_follow_speed_clocks_keep_live_pcm`: a
  captured lattice on fractional FollowSpeed clocks renders the same decoded
  PCM as the unbound document at chunk widths 1, 31 and 256.
- `pauses_inside_follow_speed_shift_later_pictures_without_changing_their_source_times`:
  after pauses of 3 and 1 frames, every project frame shows the old picture's
  exact source time, shifted by the pauses, or the pause's Background.

## Storage

`pauses_inside_follow_speed_commit_reopen_and_navigate_history_exactly` commits
two pauses through SQLite, reopening (and replaying history) after each, then
undoes both, redoes both and reopens read-only; nodes, clocks and bindings
match the edited snapshot and the undone revision matches the initial nodes.

## Review

An independent review found no arithmetic, identity-order or reanchor-set
defects. Its minor findings were fixed before commit: the marks and sounds
refusal moved into the target query; refined windows keep their pitch label on
both halves; a fractional Retime keeps an integral recipe at the ceiling of its
clock; retimed pauses are limited to silent freeze or black pictures; the root
uses the checked integral conversion; the refusal text names the restriction.

## Commands

- `cargo test -p deadpan-core -p deadpan-plan -p deadpan-store --locked`
- `cargo test -p deadpan-audio -p deadpan-playback -p deadpan-cli -p deadpan-app --locked --no-fail-fast`
- `cargo clippy` with `-D warnings` for core, plan, audio, store, CLI and app,
  all targets; `cargo fmt --check`.

Results:

- Core, plan and store: all tests pass, none failed.
- Audio, playback: all pass. CLI: `encoded_render` and `encoded_verification`
  fail independently of this change. Commit `1e8baec9` raised the encoder ABI
  to 4, while the protocol fixture and retained verification movies still
  claim ABI 3 ("encoded codec, clock, policy or allocation claims are
  inconsistent"). The protocol fixture now reports ABI 4 and `encoded_render`
  passes 10/10; the retained movies are being regenerated separately.
- App: 1,081 passed and 6 failed in the full parallel run. Three pass alone
  (load). The other three (`native_new_starts_with_the_full_original…`, both
  `backups::a_moved_…` tests) create a system bookmark for a linked Original
  and exceed their 20 s update wait: on this machine each macOS bookmark call
  currently takes 32–36 s, and `deadpan-filesystem`'s own bookmark tests take
  33–36 s each while passing. These tests do not reach pause insertion.
- Strict Clippy for core, plan, audio, store, CLI and app, all targets, and
  `cargo fmt --check` are clean after the review fixes.

## Export verification

Full release preview/export run on 2026-10-15 (`cargo test --release --locked
-p deadpan-cli --features synthetic-worker --test preview_export`, source at
`49b7f3d7` plus the two new fixtures): 33 of 33 tests passed in 606 s, 69 recipe
fixtures built and verified, none skipped. The `tape-slowed-pause` fixture
([section8.rs](../../crates/deadpan-cli/tests/preview_export/section8.rs)) slows
the base's frames 12–24 to 24 frames and inserts a 3-frame black pause at Edit
frame 17, half way through Original frame 26: 45 pictures, minimum luma PSNR
56.45 dB and chroma 53.44 dB, 28.1 dB minimum audio block SNR, audio offset
verified zero, every expected picture provenance matched.

## Not established

Native display of documents with derived clocks (the app refuses pauses that
would need them), ordinary commands on such documents, Preserve/Shift output
routes, Repeat or Partition ancestors, nested speed changes, marks and sounds,
AI pauses, playback through a device and export.
