# Reversed pauses, tails, bleeps and lifts

Ping-pong hold, Reverse hiccup, Hanging
tail, Bleep, Are We Done?, Lift, tails and
reverse are all Hold providers: inserted time with independent picture and sound policies.
Selected effect sends feed these allocated pauses through the shared mixer.

## Model

| Provider | Meaning |
| --- | --- |
| `HoldVideo::Reverse { asset, span, origin }` | Plays a measured Original picture span backwards at the project rate from an exact source-clock origin. Hold-local frame `k` samples `origin - (k + 1/2) × ticks_per_project_frame`. An omitted origin defaults to `span.end`; outside the selected span, its adjacent endpoint picture holds. |
| `HoldVideo::Play { asset, span, origin }` | Plays forward with the same endpoint policy, sampling `origin + (k + 1/2) × ticks_per_project_frame`. An omitted origin defaults to `span.start`. |
| `HoldAudio::Reverse { source }` | Plays the source span backwards from the Hold's start at its natural rate, then digital silence. The span is at most 1,048,576 samples at 48 kHz (about 21.8 s), the one-block limit, checked at document validation. |
| `HoldVideo::Rewind { bounce }` | Rewinds the edit before the occurrence, live: Hold-local frame `k` shows the evaluated picture of root frame `S - 1 - k` (`S - 2 - k` with `bounce`), `S` the occurrence's first root frame, with the framing of every owner the Hold does not share, captured on the canvas exactly as a freeze captures it. Black before the project start. |
| `HoldAudio::Rewind { bounce }` | The processed Original sound heard over the Hold's own length just before the occurrence (ending one frame earlier with `bounce`), played backwards from the Hold's start: output sample `n` is heard sample `len - 1 - n`. The window is at most 1,048,576 samples, checked at validation. |
| `HoldAudio::Tail { maximum, effect, input }` | A live reference: the wet output of `effect` fed over the two seconds before this occurrence, ringing from the Hold's start for `maximum` frames, then exact silence. `effect` is `reverb` (default) or `delay`; `input` is `original` (default) or `sends`. It stores no source. |

## Selected sends

`:tail-send reverb -6dB` or `:tail-send delay -6dB` sends the selected beat's
processed output into later matching `:tail ... input=sends` pauses. `off`
removes one send. `:sound-tail-send` applies to the selected placed sound.
The two levels are independent, exact millidecibels within -96 to +24 dB.
They do not change the dry signal, add time or create a continuous wet return.
The inspector shows current levels; command entry captures the target and revision.
Whole-beat sends support dot and macros, and a selected Repeat play is isolated.

`TailSends` belongs to `AudioTreatments`, `SoundEvent` or `BeatSound`. A beat
tap follows its own ordered treatments and balance, before its parent; a group
tap includes its processed children and attachments. An event tap follows its
gain and edges, before owner treatments. Explicit sends at multiple levels add.
Owners shared with the destination pause apply their treatments once on the
tail output, at its clock. Their send levels still select the input contribution.

The contribution mixer evaluates the complete permitted input graph. Selecting
a send cannot remove a peer that drives shared saturation or change EQ history.
Input Hold permissions, exact sound clocks and source admission remain active.
Other tail returns are silent while preparing any tail, preventing feedback.
Sends and input choices survive ordinary recipe copies, saved contexts and Undo.
Core schema 50, store schema 78 and frozen audio context schema 11 carry them;
obsolete development packages are refused without writes.

## Provider constraints

Why a Hold provider rather than a Retime direction: reversing a Retime would
make every frozen-context and Split rule that stops at
a nonunity Preserve treat a second opaque stage kind, and composite children
(groups, Repeats) would need whole-subtree reversal. A reversed Hold is a leaf
whose sound is one self-contained recipe, like room tone, so the existing
split machinery stays untouched, and the picture is an ordinary
`Picture::Source` that every decoder, cache and export path already admits.
The captured providers above cover one continuous Original passage at its
natural speed.

## Composite and speed-changed passages

Any other passage (cuts, pauses, groups, framing, Repeats, speed changes
and ramps of either pitch policy) becomes a `Rewind` Hold, a live structural
reference to the frames just before the pause, the model hanging tails
already use. The choice, against capturing:

- Pictures could be captured as an explicit list (one decoded picture and
  one captured framing per frame) and still be exact. Sound could not be
  captured as safely. A frozen block of the rendered mix would have to be
  rendered while the command is planned (media work inside macros, dot and
  the headless path) and published as a verified generated object; a frozen
  audio context cannot yet capture the binding state nearly every edited
  project has. Capturing only the pictures would let them disagree with a
  live sound after a later edit.
- A live reference stores no media, needs no store admission or cleanup
  reference, survives Undo/Redo and portable copies unchanged, and keeps
  picture and sound consistent with each other: both always reverse what
  now precedes the pause. Editing the passage changes its reversal, as it
  changes a tail. The single natural-rate passage keeps its captured form
  (and its existing evidence), since its source samples are exact without
  any rendering.

Rules, shared by the picture plan and `StageAudio`:

- Pictures are evaluated through the shared picture plan: the root walk that
  reaches a `Rewind` Hold at local `k` walks the root again at the mirrored
  frame's center and returns that picture identity unchanged (Original
  point, Freeze, Background, accepted footage). Framing scopes below the
  deepest owner shared with the Hold, the evaluated picture's own captured
  context and a Repeat-gap identity clip are captured with
  `CapturedFraming::capture`, so shared groups frame the picture once, live
  on the Hold. A Split fragment keeps the original occurrence's start.
- Sound reads the same root window a tail with `input=original` would read
  (`read_heard_window`): time mapping, Preserve and ramp processing, edges,
  Hold gates and the gain, mute and treatments of owners the pause does not
  share. Shared owners apply once, on the output, at the pause. Placed sounds
  are excluded: they keep playing on their own clock. The window is the
  exact round-even samples of the passage's frames, reserved against the
  request's prepared-frame and residency budgets; the reversed block is
  cached per occurrence like every Hold effect.
- Rewinds never chain: inside another rewind's or a tail's window a rewind
  shows black and is silent, and the host refuses a passage that holds one
  (or a tail). A read without an edit position (a definition, a retained
  clock or a point-grid read) is black and silent. A `Rewind` Hold inside a
  nonunity Retime is an invalid document, refused at authoring ("put the
  cursor outside the Retime"), and a Repeat's default gap cannot be one. It
  captures no `picture_context`.
- Frozen audio context schema 13 carries `rewind` (no input, like a tail);
  schema 12 and older reject it.

Validation requires the picture span inside the asset's video and the audio
span inside its audio (and a reversed span within the one-block limit); a
Tail's maximum is positive and at most the Hold. Explicit picture origins retain
fractional source ticks independently of the containing decoded-picture span;
validation checks the full Hold's source-clock arithmetic for overflow. Core
format 48 and database schema 76 store these origins. Reverse,
ping-pong and bleep timing are covered through actual exports.
`SetHoldDuration` shortens a Tail's maximum with a shorter Hold. Pause
insertion admits Reverse pictures like freezes; splitting a reversed Hold keeps
its full recipe behind Partitions. Generated media cannot be accepted onto a
reversed Hold. Frozen audio contexts retain `reverse`, `tone` and the tail
effect (`ReferenceAudibility`), and the frozen-JSON preflight admits the
`effect` field. These need context schema 7: a schema-6-or-older context
containing a reversed, tone or tail Hold is rejected (the project has no users,
so older contexts are not upgraded). Store admission checks the source receipt,
Original binding and exact sample endpoints for `SetHoldAudio` and for
`InsertTime` pauses carrying Reverse audio; tails and tones read no stored
media and need none.

## Rendering

The audio plan carries `AudioContent::Reverse { source, duration }` and
`AudioContent::Tail { maximum, effect, input, duration }` with the full intrinsic
Hold duration. `StageAudio` prepares each as one canonical cached block keyed
by definition, occurrence path, preceding play and content
(`PreparedKey::Hold`). The cache belongs to one immutable plan, so any edit
(which compiles a new plan) prepares again.

- A reversed Hold reads its exact source span at 48 kHz once and reverses it
  in sample order (output `n` is input `len - 1 - n`,
  [`reverse`](../crates/deadpan-audio/src/hold_effects.rs)).
- A tail is a live reference evaluated on the plan at render time. On the root
  grid the occurrence's start is the root sample where its own (possibly
  retained) sampling map is at local zero, so a Split fragment or a moved
  Repeat gap keeps its ring's phase; a start at or before root zero hears
  nothing. The tail reads the root window `[max(0, start - 96000), start)`
  through the root readers (time mapping, edge fades, Hold gates). Original
  input without EQ uses 256-sample blocks; EQ and selected sends use the
  bounded shared graph, whose media readers still request bounded blocks.
  The window is reserved against the request's
  prepared-frame and residency budgets, and every nested block spends the same
  plan-work, source-check, depth, deadline and cancellation budgets; source
  dependencies are retained with the cached block for revalidation. `original`
  excludes placed sounds; `sends` includes only explicitly sent contributions.
  Another tail inside that window counts as silent,
  so tails never chain or feed back. Changing, re-gaining, muting or replacing
  what comes before a tail changes the tail. Consequently audio blocks over a
  tail depend on the sources heard up to two seconds before it, and the tail is
  prepared again whenever that preceding window changes (any edit compiles a
  new plan).
- Gain convention: an owner whose gain also applies to the tail itself (a
  shared group, Repeat or the root) applies once, to the tail's output at the
  tail's own position; the tail's input carries only the gain, mute and
  envelopes of owners it does not share (the beats before it). A -6 dB group
  around speech and its tail lowers the tail by 6 dB, not 12, and a group
  envelope is evaluated at the tail's time, not the speech's. In the
  edge-faded and time-mapped stages, which apply no owner gain, the tail's
  input still carries the non-shared gains because the cached tail block is
  shared by every stage.
- A tail rings only on the root grid. Read on a point grid (an audio
  definition, a retained clock, a policy query, a tape or mix) it has no edit
  position and is silent; such reads never fail because of a tail. A tail
  cannot sit inside a speed change: a document with a tail Hold or tail gap
  under a nonunity Retime of either pitch policy is invalid, so wrapping a
  tail in a Retime, changing a unity Retime around one to another speed, or
  giving a pause inside a Retime a tail is refused at authoring time with
  that reason and the fix ("put the pause just after the Retime instead,
  where it rings the sped-up sound at normal speed"). Transparent Partitions
  and unity Retimes are allowed. Decided 2026-11-10:
  the ring is defined at the edit's rate, but inside a speed change every
  sample is read on the stage's input grid, which has no edit position, and
  is then resampled (tape) or stretched (Preserve) by the one shared DSP
  path. An output-rate ring there would need prepared audio that bypasses
  the stage it sits in, which neither stage kind has exactly (pre-stretching
  the ring by the inverse rate is an approximation, not the same samples).
  A tail just after the Retime hears the processed, sped-up sound and rings
  it at normal speed, which is the intended effect.
- Loop seams (2026-11-10): every occurrence hears what the edit clock heard
  just before it. A tail opening a Repeat play hears, in the first play, what
  precedes the Repeat, and in later plays the end of the previous play and
  its gap; a tail as the Repeat's gap hears the end of the play before it.
  Other tails in the window (the previous play's own tail, a gap tail) are
  silent. `a_tail_at_a_loop_seam_or_after_a_gap_hears_the_edit_just_before_it`
  checks all three rings against the reverb of the authored bus over exactly
  those windows.
- The tail runs the fixed effect over that input in `f64` and keeps what rings
  afterwards ([`render_tail`](../crates/deadpan-audio/src/hold_effects.rs)):
  the reverb is a per-channel parallel-comb (8) and series-all-pass (4)
  network with a 25-sample stereo spread, feedback 0.84 and damping 0.2
  (its impulse response falls 60 dB in 1.09 s, measured by a unit test that
  keeps the documented "about 1.1 s" true); the delay repeats every 300 ms at half level. The
  ring fades linearly to exact zero over its last 50 ms (shorter for short
  rings); the rest of the Hold is digital silence.

A read that starts mid-Hold samples the same block, so preview, seeking and
export hear identical samples. Shared edge fades, Hold gates, gain and the
limiter apply afterwards as for any Hold sound. Algorithm identities:
`deadpan-reverse-sample-order-v1`, `deadpan-tail-reverb-stereo-comb8-allpass4-v1`,
`deadpan-tail-delay-300ms-half-v1`. The source-stage inspector
(`SequenceAudio`, `inspect-audio` without a stage flag) reads source PCM before
Hold effects and still refuses reversed, tone and tail Holds, as it refuses room
tone, with a message pointing to the processed stages (`--time-mapped`,
`--edge-faded`, `--authored-bus`, `--limited`), which render them.

A reversed 44.1 kHz source is resampled forward to 48 kHz first, then
reversed sample by sample. When the span's exact 48 kHz extent is fractional
the resampler origin is shifted by that fraction, so output sample `n` hears
the source exactly `n + 1` mix samples before the span's end (origin-based
boundaries; a test checks every point of a 221-sample 44.1 kHz span).

Reversing needs the sound heard over the frames at its natural rate: the
host refuses a passage whose audio mapping is stretched or squeezed or that
sits under a speed stage, since reversing it would replay something else.

## Bleeps

A bleep keeps the pictures and replaces the sound. `HoldVideo::Play { asset,
span, origin }` plays an Original span forward at the project rate (the mirror of
`Reverse`, holding its last picture past the end), and `HoldAudio::Tone {
frequency_hz, level }` synthesizes a sine for the whole Hold: 20 Hz to 20 kHz at
or below full scale, 2 ms linear ramps to exact zero at both ends, and the phase
of sample `n` computed as `n * f mod 48000` in integers so nothing accumulates
([`tone`](../crates/deadpan-audio/src/hold_effects.rs), `deadpan-tone-sine-exact-phase-2ms-ramps-v1`).
A tone reads no media, has no frozen-context input and needs no store
admission. `,b` or `:bleep [880Hz] [level=-6dB]` (default 1 kHz at -10 dB)
resolves the Visual range's pictures before cutting it into the register, then
inserts the Play/Tone Hold of exactly the cut length at the join, as one
compound and one Undo (`SemanticInstruction::Bleep`). The range must show
Original pictures; a group object refuses.

A range across cuts, or reaching into an unframed group (since 2026-11-10), is
still one Hold and one tone, so the tone never restarts or dips at a cut. The
host splits the range into runs whose pictures advance exactly one project
frame of the Original's clock per frame: the first run plays as the Hold's
`Play` picture, and each later run becomes a picture-only
[cutaway](CUTAWAYS.md) on the new Hold over exactly its frames, whose
selection starts half a frame before the run's first picture point, so every
frame center samples the same Original time it showed before. Contiguous Split
fragments form one run. Because a cutaway carries none of the old framing, a
range across cuts must show every picture unframed at every level (no beat or
group framing and no captured framing); a framed part, a pause, a still or
generated picture in the range, more than 16 cuts, or a run starting before
the Original's first picture refuses with a reason. Evidence:
`a_bleep_across_cuts_puts_later_passages_on_its_pause_as_cutaways` (one Hold,
its cutaway, exact Compound Undo, a cutaway past the pause refusing), the
`split-edits` replay (`,b` over a cut keeps every picture and the length) and
the `bleep-across-cut` preview/export fixture.

## Lift

`:lift` cuts the Visual range into the selected register as `d` does and puts
back a silent black (Background) Hold of exactly its length at the join, so
everything after it keeps its time (`SemanticInstruction::Lift`, one compound).

## Commands

The host resolves each pause from the staged document
([`deadpan_cli::pause`](../crates/deadpan-cli/src/pause.rs)), shared by the
native app, macros and the headless semantic path:

- `:reverse 8f` (default 8f) inserts at the Edit cursor a pause playing the
  frames before it backwards. When every picture is the same Original leaf at
  its natural rate (exact source steps) and its sound is one natural-rate
  passage, the pause captures them as below; otherwise it is a live
  `Rewind` (above). The selected decoded-picture span runs
  from the first picture's PTS to the end of the last, while a separate exact
  origin preserves the requested project passage's end. The sound heard over
  those frames, as exact inward-rounded source samples, is reversed (silence stays silence; a sound
  that is not one continuous passage refuses). At most 20 s.
- `:ping-pong 12f` (default 12f) omits the last project-frame sample before the
  cursor: the pause is one project frame shorter and its source-clock origin
  moves back exactly one project frame. A longer VFR picture may still appear
  at neighboring project samples; its complete decoded interval is not removed.
- `:tail [D] [effect=reverb|delay]`: on a selected pause its sound becomes the
  live tail of the two seconds heard just before it, ringing for `D` (at most
  the pause; the whole pause when omitted). Only the sound changes, so this
  works on any pause, including accepted generated footage or stills. With no
  pause selected a freeze pause of `D` is inserted at the cursor carrying that
  tail. `,t` opens the command with the length ready to change: the selected
  pause's length, or 1s.
- `:gag are-we-done [register=r] [pause=1.5s]`: a freeze pause whose reverb
  tail rings for the whole pause, a reaction cutaway from the register's
  Original moment over it, grouped and pinned like every recipe
  ([gags](GAGS.md)).

Semantic instructions `InsertReverse { length, bounce }`, `Tail { length,
effect }`, `Bleep`, `Lift` and `SetCutaway { register, fit }` record in macros (`,t`, `:tail`,
`:reverse`, `:ping-pong` and whole-beat `:cutaway` record instead of refusing).
The inspector shows `Reversed` (or `Reversed passage`) picture and sound, and `Reverb tail 15 f` with
a `Tail of: Live 2 s before` row.

## Tests and evidence

- Core: planner sites, bounce length, refusals, tail on a selected Hold versus
  a new pause (including a pause whose picture cannot be resolved), Are We
  Done? as one group with an exact inverse, the reversed-span bound, and
  schema-6 contexts refusing each new variant.
- Plan: reversed pictures frame by frame and the first-picture hold.
- Audio: reversal order and block crossing, delay echo positions and levels,
  reverb RT60, decay to exact silence, determinism; through `StageAudio` the
  reversed Hold, the exact fractional 44.1 kHz reversal, both tail effects
  against the reference block over the preceding authored bus, a -6 dB trim
  scaling and a mute silencing the tail, a pause at the start staying silent,
  a tail not feeding the next one, shared-group trim, envelope and mute
  applying once, a Repeat with pause and gap tails split and re-read with identical PCM and the Original after them unchanged,
  point-grid definition, gap and tape reads that are silent rather than
  failing, the source-stage message, and a mid-Hold read. Core refuses
  wrapping a tail in a Retime, speeding up a unity Retime around one and
  giving a pause inside a Retime a tail.
- Store: InsertTime with Reverse sound from an unqualified asset is refused
  without a write; admitted samples commit; a tail needs no admission.
- CLI: the heard sound must be at its natural rate (a squeezed mapping
  refuses).
- Replay `hold-effects`: `:reverse 8f`, `:ping-pong 12f` (same start, earlier
  end, one frame shorter), `,t` proposing `tail 1s effect=reverb` and inserting
  a tail pause, `,h` then `,t` switched to `delay` on the selected pause with
  timing unchanged, `:lift` and `,b` over a Visual range with total time
  unchanged, the inspector rows and Undo; Kestrel audit with `,t` and `,b`.
- Bleep and lift: planner tests for the pre-cut picture site, the refilled
  length and the wire defaults; the forward Play picture plan; the tone through
  `StageAudio` without reading media; frozen contexts of reversed and tone
  Holds render the live samples.
- Rewind: plan tests (`picture_plan/rewind.rs`) check every picture and
  captured framing of a passage with a cut, a freeze pause, a framed group
  and a 2x Retime mirrored exactly, bounce, black before the start, no
  nesting and invalidity inside a speed change; `StageAudio` tests check the
  reversed block equals the forward authored bus over the same passage
  (two cuts, a -6 dB group, a pause and a 2x Preserve Retime) reversed sample
  for sample, with and without bounce, in several read sizes, and that a
  tail after a rewind hears it as silence. The CLI resolver test covers the
  fallback and the nesting refusals. The `hold-effects` replay cuts two
  frames with `v l d`, runs `:reverse 8f` across the cut (every picture
  mirrored, the inspector row), repeats it with `.` and undoes each step.
  Export fixtures `reverse-across-cut` (pictures across the cut, the click
  back at 38,485) and `reverse-ramp` (a pitch-kept continuous ramp
  rewound).
- Export (release, [verification](PREVIEW_EXPORT_VERIFICATION.md)):
  `reverse-hiccup` (the reversed click lands 3,250 samples into the pause),
  `ping-pong`, `hanging-tail` (reverb, 29 compared blocks), `tail-echo` (echo at
  43,181), `are-we-done`, `bleep` (pictures unchanged, the click replaced by
  the tone) and `lift`.

A fourth pause sound, a user-chosen `custom` sound (once or looped, with
fades), is documented with room tone ([custom pause sounds](ROOM_TONE_AUDIO.md#custom-pause-sounds)).

## Remaining

A tail inside a speed change (refused, see above), a bleep mixed over the sound instead of replacing it, bleeping framed
pictures across a cut, choosing the tail's source range by hand, a captured
(edit-independent) reversal of composite passages, reversing placed sounds,
a passage longer than 20 s, a reverse inside a speed change, and listening
qualification of the effects. Selected sends from beats and placed sounds
into a tail are implemented ([Selected sends](#selected-sends)).
