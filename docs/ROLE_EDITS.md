# Role edits: J- and L-cuts and role-only deletes

Linked edits are the default. These
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

### Beside a pause or across a group boundary

When one side of the cut is a pause or a group, nothing can roll: a pause has
no media handle, and a Roll needs two literally adjacent Source beats. The
split edit then keeps every beat, picture and duration and moves only sound
(since 2026-11-10):

- The leading beat is the incoming side for a J-cut and the outgoing side for
  an L-cut; for a group it is the group's first (J) or last (L) nonempty beat,
  through ordinary Sequence groups. It must be a Source or unity Partition of
  the Original with linked sound and a natural-rate audio mapping. Its sound
  continues from its own Original handle across the cut as one root
  [sound event](SOUND_EVENTS.md) through that beat's exact audio mapping and
  independent offset, over exactly the split length. The event's seam side
  is Hard, and so is the leading beat's own edge at the cut where it would
  fade (an unmarked Split fragment side has no fade and is left alone), so
  the sound is sample-continuous there; the far end of the event fades as
  usual.
- The other side's beat at the cut (again through groups) gives way: a
  silent pause gets an explicit silence allowance for the event, so the pause
  stays silent except for the carried sound; a beat of the Original is muted
  over the stretch with one clip-gain mute range in its own frames.
- Everything happens in one Compound and one Undo, with the same
  `SplitEdit { kind, length }` instruction, so recording and `.` behave as
  for a Roll split edit. The cursor stays at the picture cut.

It refuses with a reason when the leading side is a pause ("a pause has no
sound of its own"), a Repeat or a speed change; when a pause on the other side
has its own sound; when the other side's beat or pause at the cut is shorter
than the split (the largest available length is reported); when the Original
handle runs out (likewise); when a leading beat or group carries gain or
effects, which the handle sound would not keep; inside a play scope; and
below a Repeat or speed change, where Edit frames are not root frames. Two
adjacent Source beats still roll exactly as above.

Evidence: `role_boundaries.rs` at 30000/1001 fps checks the exact heard Edit
frames, the Original frame at each event's first frame, Hard edges, the
allowance or mute range, unchanged nodes and duration, exact Compound Undo and
each refusal. The `split-edits` replay types `:lcut 6f` into and `:jcut 6f`
out of a `:hold 8f` pause and the refused pause-led J-cut. The `l-cut-pause`
and `j-cut-pause` preview/export fixtures place the Original's click only in
the carried handle, under a black pause, and compare the export with the
preview.

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
range in the current Sequence. A range across cuts acts on each direct child's
own part in one transaction and one Undo.
`:select role=audio|video|linked` chooses the role a Visual `d` deletes (and
Visual `r` repeats); the status line shows AUDIO ROLE or VIDEO ROLE until
`role=linked`. The choice belongs to one project session and group in Your
edit: showing the Original, entering or leaving a group, opening Repeat or
Retime contents, or replacing the project returns it to linked with a message. The
instruction is `DeleteRole { role }`.

- **Audio**: the range becomes a mute range of that beat's
  [clip gain](AUDIO_GAIN.md), in the beat's own output frames (the same exact
  silence as `,m`). Picture, time and placed sounds are unchanged.
- **Video**: the topmost picture goes. Where a cutaway covers the range, that
  part of the cutaway is trimmed so the beat's own picture shows again; a later
  piece of a `hold` or `gap` cutaway starts its selection correspondingly later
  so it shows exactly the same pictures, while a `loop` or `bounce` cutaway
  refuses a mid-cycle start (delete to its end instead). Elsewhere the range
  becomes a removed-picture cutaway on the beat's Source (through unity
  Partitions): `Cutaway.removed` makes the shared picture plan show the project
  background there. A second delete therefore reaches the background; a range
  already removed refuses. Its `asset` and `selection` record which
  Original pictures were removed, clamped to the measured video; nothing is
  shown from them. Sound, time and word/pause/shot projections are unchanged.
  A pause's picture is its whole content, so `role=video` on a Hold refuses
  with guidance to use `:lift`.

Neither role-only delete moves later content. An empty range, `role=linked`
with `:delete`, an overlapping cutaway and, for video, a pause in the range
refuse.
Recording stores the instruction; `.` applies it to a new Visual range.

Evidence: `role_only_deletes_keep_time_and_the_other_role_within_and_across_beats`
(core, including both roles across a cut), `a_removed_picture_cutaway_shows_the_background_and_keeps_every_other_picture`
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
  A sound range may span several beats (since 2026-11-07): each beat's own
  stretch of the range plays again as its own root sound event, exactly as
  `:audio-shift` builds them, translated by whole multiples of the range, so a
  play keeps every cut inside the range. The later plays run over the range's
  last beat, whose sound is muted under them; the overflow policies below apply
  to that beat. Silent pauses inside the range repeat as silence; a pause with
  its own sound refuses, as does a range ending in a pause, a range whose sound
  is already muted (its repeats would sound it again) or a range outside the
  group. Contiguous Split fragments of one Source become one event per play,
  so no edge fade dips at a pure Split seam; `:audio-shift` merges them the
  same way. Other beat treatments (gain, envelopes) are not copied onto the
  repeated events. Evidence:
  `an_audio_repeat_spans_cuts_and_plays_each_beats_sound_again`.
- **Video**: one looping cutaway over the later plays shows the range's
  pictures, at their natural rate (a retimed picture refuses); the beat's sound
  continues. A range across cuts places one natural-rate cutaway per beat
  stretch and play on the range's last beat (contiguous Split fragments merge),
  so each play shows the same pictures in order. A pause or group in the range,
  or a cutaway already covering part of it (its repeats would show the beat
  beneath), refuses. Evidence:
  `a_video_repeat_spans_cuts_and_shows_each_beats_pictures_again`.

Repeats that would pass the beat's end refuse with the missing frame count
unless `overflow=trim` cuts them at the end or, for sound, `extend=hold` holds
the beat's last picture for exactly the overflow: one silent freeze pause is
inserted after the beat (its later content moves on its own sample clocks, as
for any pause), the repeats run on into it, and each repeat that reaches it
gets an explicit silence allowance for that pause, so it is heard while the
picture holds. Only this policy adds time; a video-only repeat refuses it, and
choosing both policies refuses. Recording and `.` use the instruction.

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

## Audio shift

`:audio-shift +5f` (or `-3f`) slides all sound from the Edit cursor to the end
of its group by whole frames, across cuts, while every picture stays. The
instruction is `AudioShift { frames, trim, extend }`
([audio_shift.rs](../crates/deadpan-core/src/semantic/planner/audio_shift.rs)).
Each beat's own sound over the affected frames becomes a mute range, and plays
again as a root sound event of the same Original audio through that beat's
exact mapping, moved by the shift, so the slide is continuous across cuts and
follows later root-clock edits. A forward shift leaves silence at the cursor; a
backward shift also mutes the frames it slides over and refuses before the
group's start. Sound pushed past the group's end refuses with its count unless
`overflow=trim` cuts it there or `extend=hold` holds the last picture for the
shift and permits the sound over that pause. A pause with its own sound in the
stretch, a group, a fitted mapping or a Visual selection refuses. The owner chose
this continuous behavior on 2026-11-07.

Evidence: `an_audio_shift_slides_sound_across_cuts_and_keeps_every_picture`
(exact root frames and Original phase of each moved sound, mutes, unchanged
pictures, the extension and its allowance, and four refusals) and the
`split-edits` replay (refusal, a shift across the cut, Undo). The `audio-shift`
preview/export fixture (focused release run, 2026-11-08) shifts sound across a
real cut by 5 frames with `overflow=trim`: the click is silent at its old
sample and heard exactly 8,008 samples later, with every picture unchanged and
zero-offset audio.

## Remaining

Inter-play holds for audio-only repeats, copying a range together with its
repeats (refused today), split edits led by a pause or reaching into a pause
with its own sound, split edits below Repeats or speed changes, and role scope
for yank/repeat/paste.
