# Role edits: J- and L-cuts and role-only deletes

Linked edits are the default (specification §6.3, §6.5 and §8.3). These
commands change one media role without moving the edit's time. Each is one
semantic instruction, so the native app, macros, `.` and the headless macro
path author the same transaction, and each is one Undo.

## J- and L-cuts

`:jcut 6f` and `:lcut 6f` (or `200ms`; 6 frames by default) act at the cut
under the Edit cursor, between two adjacent children of the current ordinary
Sequence. The instruction is `SplitEdit { kind: j | l, length }`
([split_edit.rs](../crates/deadpan-core/src/semantic/planner/split_edit.rs)).

A split edit is two qualified primitives in one Compound:

1. [`RollSources`](SOURCE_ROLL.md) moves the linked seam by the length: earlier
   for a J-cut, so the next beat's picture and sound start from its source
   handle; later for an L-cut, so the current beat continues from its handle.
   The Roll's own admission, exact limits, retained sample clocks, editorial
   edges and root-sound restoration apply unchanged.
2. A picture-only [cutaway](CUTAWAYS.md) over exactly the moved stretch shows
   the pictures that were there before: the last pictures of the outgoing beat
   (J) or the first pictures of the incoming beat (L). Its source span is
   computed from the rolled side's retained affine picture map, before the
   Roll, so the cutaway samples the same source pictures at the same project
   frames. The picture therefore still cuts at the cursor and the duration is
   unchanged; only the sound's cut moves.

Both beats must be qualified Source or unity Partition beats with natural-rate
pictures (a retimed picture refuses, because a cutaway plays at the natural
rate). If either side lacks the source handle, the whole edit refuses with the
largest available length; it is never silently shortened. An existing cutaway
over the stretch refuses. The cursor stays at the picture cut.

Evidence: `j_and_l_cuts_move_the_sound_cut_and_keep_every_picture_and_the_duration`
and `split_edits_refuse_without_a_seam_handles_or_whole_length`
([split_edit.rs](../crates/deadpan-core/tests/split_edit.rs)) compare every
Edit picture before and after, the exact durations and the Compound inverse.
The `split-edits` replay types both commands at a cut between two different
Original moments, checks the beat lengths and the plan's pictures, Undo, and
`.` at a second cut. The `j-cut` and `l-cut`
[preview/export fixtures](PREVIEW_EXPORT_VERIFICATION.md) place the Original's
click in the incoming (J) or outgoing (L) handle, so it is heard only because
of the split edit, and compare the export with the preview.

## Role-only deletes

`:delete role=audio` and `:delete role=video` act on the Edit Visual time
range, which must lie inside one direct child of the current Sequence.
`:select role=audio|video|linked` chooses the role a Visual `d` deletes (and
Visual `r` repeats); the status line shows AUDIO ROLE or VIDEO ROLE until
`role=linked`. The choice belongs to one project session and group in Your
edit: showing the Original, entering or leaving a group, opening Repeat or
Retime contents, or replacing the project returns it to linked with a message. The
instruction is `DeleteRole { role }`.

- **Audio**: the range becomes a mute range of that beat's
  [clip gain](AUDIO_GAIN.md), in the beat's own output frames (the same exact
  silence as `,m`). Picture, time and placed sounds are unchanged.
- **Video**: the range becomes a removed-picture cutaway on the beat's Source
  (through unity Partitions): `Cutaway.removed` makes the shared picture plan
  show the project background there. Its `asset` and `selection` record which
  Original pictures were removed, clamped to the measured video; nothing is
  shown from them. Sound, time and word/pause/shot projections are unchanged.
  A pause's picture is its whole content, so `role=video` on a Hold refuses
  with guidance to use `:lift`.

Neither role-only delete moves later content. A range across a cut, an empty
range, `role=linked` with `:delete` and an overlapping cutaway refuse.
Recording stores the instruction; `.` applies it to a new Visual range.

Evidence: `role_only_deletes_keep_time_and_the_other_role_inside_one_beat`
(core), `a_removed_picture_cutaway_shows_the_background_and_keeps_every_other_picture`
(plan), the `split-edits` replay (status line, both roles through `d`, the
refusal across a cut, `:delete role=video` once) and the `role-delete`
preview/export fixture (silenced click, background pictures, unchanged time).

## Audio-only and video-only repeats

`:repeat 3 role=audio` or `role=video` (2 plays by default) repeats one role
of the Visual time range inside one direct child, over the following frames of
that beat, without inserting time. After `:select role=audio|video`, Visual `r`
does the same with its count. The instruction is `RoleRepeat { role, plays,
trim }`
([role_repeat.rs](../crates/deadpan-core/src/semantic/planner/role_repeat.rs));
the range's parent and every ancestor must be ordinary Sequences, so Edit
frames are root frames.

- **Audio**: the first play is the beat's own sound. Every later play is a
  root [sound event](SOUND_EVENTS.md) of the same Original audio stream: the
  host Source's exact audio mapping (`Placement`, `Duration` or
  `SelectedPlacement`; a fitted mapping refuses) translated so that the range's
  first host frame sounds at the play's onset, with a selection of exactly the
  range's sound (before the independent audio offset, which the event keeps).
  The beat's own sound is muted under the later plays by one mute range. The
  events appear in Placed sounds ("Repeat 2 of 3 · …") and follow root-clock
  edits (ripple deletes, insertions, moves) through root sound routes; later
  Splits keep them. A copy captures the beat's mute but not root sounds, so
  copying, cutting or `:recipe-save` of any Edit range that meets a sound
  repeated from the Original is refused with the sound's name instead of
  producing a silent copy; delete the repeat in Placed sounds or Undo first.
  Placed catalog sounds (audio-only assets) are unaffected by this refusal.
  Store admission rechecks the Original's receipt.
- **Video**: one looping cutaway over the later plays shows the range's
  pictures, at their natural rate (a retimed picture refuses); the beat's sound
  continues.

Repeats that would pass the beat's end refuse with the missing frame count
unless `overflow=trim` cuts them at the end; `extend=hold` is refused with an
explanation, because a role repeat never adds picture time. Recording and `.`
use the instruction.

Evidence: `role_repeats_play_one_role_again_without_inserting_time` (sound
selections in the beat's clock, mapping phase, mute range, looped pictures,
overflow refusal and trim, and the Compound inverse),
`audio_repeats_follow_root_edits_and_refuse_captures_that_would_leave_them_behind`
(the muted beat's capture refuses, others do not; a ripple delete routes the
repeats and keeps the mute; a Split keeps both repeats), the `split-edits`
replay (Placed sounds, the refused `yy`, a later Split) and the `role-repeat`
preview/export fixture, where the click is heard again exactly 5 and 10 frames
later, and still after a later ripple delete moves everything 3 frames
earlier, in the export and preview.

## Remaining

An explicit `audio-shift` command that ripples sound independently of picture,
`extend=hold` and fit/hold policies for role repeats, inter-play holds for
audio-only repeats, copying a range together with its repeats (refused today),
split edits
across group boundaries or onto Holds, role scope for yank/repeat/paste, and a
removed picture that exposes an underlying attachment instead of the background.
