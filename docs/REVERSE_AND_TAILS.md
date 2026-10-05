# Reversed pauses, tails, bleeps and lifts

Specification §8.1 "Ping-pong hold" and "Reverse hiccup", §8.3 "Hanging
tail" and "Bleep", §8.4 "Are We Done?", §6.3 "Lift", §10.4 tails and §10.5
reverse. All are Hold providers: inserted time with independent picture and sound policies
(§3). Neither adds a node kind, a send bus or a separate preview algorithm.

## Model

| Provider | Meaning |
| --- | --- |
| `HoldVideo::Reverse { asset, span }` | Plays a measured Original picture span backwards at the project rate from its end. Hold-local frame `k` shows the picture at `span.end - (k + 1/2)` project frames, an exact rational source point; past the span's start the first selected picture holds (adjacent-hold endpoint policy). |
| `HoldAudio::Reverse { source }` | Plays the source span backwards from the Hold's start at its natural rate, then digital silence. The span is at most 1,048,576 samples at 48 kHz (about 21.8 s), the one-block limit, checked at document validation. |
| `HoldAudio::Tail { maximum, effect }` | A live reference: the wet output of `effect` fed with the processed Original sound heard over the two seconds before this occurrence, ringing from the Hold's start for `maximum` frames, then exact silence. `effect` is `reverb` (default, omitted on the wire) or `delay`. It stores no source. |

Why a Hold provider rather than a Retime direction: reversing a Retime would
make every audio binding, reanchor, frozen-context and Split rule that stops at
a nonunity Preserve treat a second opaque stage kind, and composite children
(groups, Repeats) would need whole-subtree reversal. A reversed Hold is a leaf
whose sound is one self-contained recipe, like room tone, so the existing
binding and reanchor machinery stays untouched, and the picture is an ordinary
`Picture::Source` that every decoder, cache and export path already admits.
The cost is scope: only one continuous Original passage at its natural speed
can be reversed, not a framed group or a Repeat.

Validation requires the picture span inside the asset's video and the audio
span inside its audio (and a reversed span within the one-block limit); a
Tail's maximum is positive and at most the Hold.
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
`AudioContent::Tail { maximum, effect, duration }` with the full intrinsic
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
  through the ordinary root reader (time mapping, edge fades, Hold gates) in
  256-sample blocks, the block size every root, resampling, edge and bound
  reader is built for. The window is reserved against the request's
  prepared-frame and residency budgets, and every nested block spends the same
  plan-work, source-check, depth, deadline and cancellation budgets; source
  dependencies are retained with the cached block for revalidation. Placed
  sounds are not fed in, and another tail inside that window counts as silent,
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
  giving a pause inside a Retime a tail is refused at authoring time
  (otherwise the ring would be stretched or pitch-shifted). Transparent
  Partitions and unity Retimes are allowed.
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
span }` plays an Original span forward at the project rate (the mirror of
`Reverse`, holding its last picture past the end), and `HoldAudio::Tone {
frequency_hz, level }` synthesizes a sine for the whole Hold: 20 Hz to 20 kHz at
or below full scale, 2 ms linear ramps to exact zero at both ends, and the phase
of sample `n` computed as `n * f mod 48000` in integers so nothing accumulates
([`tone`](../crates/deadpan-audio/src/hold_effects.rs), `deadpan-tone-sine-exact-phase-2ms-ramps-v1`).
A tone reads no media, has no frozen-context input and needs no store
admission. `,b` or `:bleep [880Hz] [level=-6dB]` (default 1 kHz at -10 dB)
resolves the Visual range's pictures before cutting it into the register, then
inserts the Play/Tone Hold of exactly the cut length at the join, as one
compound and one Undo (`SemanticInstruction::Bleep`). The range must be one
continuous natural-rate Original passage; a group object refuses.

## Lift

`:lift` cuts the Visual range into the selected register as `d` does and puts
back a silent black (Background) Hold of exactly its length at the join, so
everything after it keeps its time (`SemanticInstruction::Lift`, one compound).

## Commands

The host resolves each pause from the staged document
([`deadpan_cli::pause`](../crates/deadpan-cli/src/pause.rs)), shared by the
native app, macros and the headless semantic path:

- `:reverse 8f` (default 8f) inserts at the Edit cursor a pause playing the
  frames before it backwards. Every picture must be the same Original leaf at
  its natural rate (exact source steps); the span runs from the first picture's
  PTS to the end of the last, and the sound heard over those frames, as exact
  inward-rounded source samples, is reversed (silence stays silence; a sound
  that is not one continuous passage refuses). At most 20 s.
- `:ping-pong 12f` (default 12f) is the same with the picture at the cursor
  left out: the pause is one frame shorter and its span ends at that picture's
  PTS, so the turn is not shown twice.
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
The inspector shows `Reversed` picture and sound, and `Reverb tail 15 f` with
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
  applying once, a Repeat with pause and gap tails split and re-read through
  retained clocks with identical PCM and the Original after them unchanged,
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
- Export (release, [verification](PREVIEW_EXPORT_VERIFICATION.md)):
  `reverse-hiccup` (the reversed click lands 3,250 samples into the pause),
  `ping-pong`, `hanging-tail` (reverb, 29 compared blocks), `tail-echo` (echo at
  43,181), `are-we-done`, `bleep` (pictures unchanged, the click replaced by
  the tone) and `lift`.

## Remaining

A bleep mixed over the sound instead of replacing it, bleeping across a cut,
effect sends from arbitrary beats or sounds into a tail (§10.4 "selected
sends"), choosing the tail's source range by hand, reversing composite
structure (groups, Repeats, framed scopes) or at other speeds, a reverse
picture of edited (cutaway) content with its own sound, and listening
qualification of the two effects.
