# Structural speed editing

Use `:retime 0.75 pitch=preserve` to play the selected beat at three-quarter
input speed while preserving pitch. `pitch=tape` makes pitch follow speed.
Choose the policy explicitly. Positive exact fractions such as `3/4` are also
accepted; floating-point parsing is never used for authored timing.

In Your edit, the inspector's **Change speed** action opens the same command.
The entry shows its old and resulting duration in project frames, effective
speed and pitch policy before Enter applies it. Escape cancels entry. The
Original remains unchanged. Native text editing, IME and focus ownership follow
the ordinary command field.

The native operation selects a direct child of the active ordinary Sequence,
including a Source, Hold, Repeat, group or split fragment. Changing speed wraps
that complete beat in an ordinary Retime. On an existing ordinary Retime,
`:retime` adjusts that stage in place while preserving its child, input range,
framing and identity. `:wrap-retime 2 pitch=tape` explicitly adds another stage.
Speed is relative to that stage's input, not its previously rounded output.
An existing split Partition is always wrapped; changing speed must not rewrite
the fragment's retained source or sound clocks.

Output duration is `round_even(input_frames / speed)`, calculated once with
checked exact arithmetic. The resulting effective speed is
`input_frames / output_frames`; rounding can make it differ from the requested
value. Empty inputs, nonpositive speeds, zero-frame results and overflow fail
without a revision. Native adjustment to the same duration and pitch is a no-op.
The inspector pre-fills the exact effective fraction, so opening and applying an
unchanged value cannot drift the duration.

## Core and history

`WrapRetime { node, id, duration, pitch }` retains the selected node beneath a new
Retime with a full input range. It creates no media and flattens no structure.
`SetRetime { node, duration, pitch }` accepts ordinary Edit purpose only and keeps
its input range. Both use the existing checked command, atomic patch, mark and
undo paths. An unresolved active generation request still requires real host
relevance observations; the native service does not invent them.

A changed Retime processing output is newly authored on its current output
allocation. Its own retained output binding is removed; its descendant input
bindings and unrelated ancestor/sibling bindings remain. Unused clocks are
pruned through normal binding lifecycle. An identical duration and pitch retain
the binding exactly. Undo restores the complete prior binding state. This is
necessary when changing from a physical pitch-preserving stage to tape speed or
unity, or shortening beyond a retained output resume entry. It does not clear
source phase or the history needed by a retained child stage.

Core schema 28 and database schema 34 freeze the previous core-27 command
vocabulary. Database 33 history replays through `legacy_v27`; new speed commands
cannot appear retroactively in an old history. Existing Retime document recipes
retain their meaning. Migration validates the complete history on a consistent
copy and preserves its pre-migration backup.

Native `:retime`, `:wrap-retime`, `:pitch` and `:hold-duration` commit as one
semantic instruction each (`Retime { speed, pitch, wrap }`, `Pitch { semitones }`,
`SetHoldDuration { length }`) through the shared planner, which authors exactly
the `WrapRetime`, `SetRetime` and `SetHoldDuration` commands above against the
staged selected direct child. A macro therefore records them, and `.` repeats
them on another beat with the same speed applied to that beat's own input, the
same shift at that beat's own speed, or the same pause length. An unchanged
speed and pitch, or an unchanged pause length, refuses without a revision. The
wrapper becomes the selected beat and the cursor stays at its start. Evidence:
`speed_wraps_a_beat_then_updates_its_own_retime_from_the_exact_input`,
`pitch_keeps_the_speed_or_wraps_at_unity_and_zero_restores_preservation`,
`a_pause_duration_changes_only_the_selected_hold` (each replayed as a compound
and inverted) and the `creative-dot` replay.

An existing Hold inside Repeat contents also accepts `:hold-duration` when
This play is explicit at every enclosing Repeat. It records the same length
intent and continues at the actual resized Hold's start, preserving downstream
sample clocks. This occurrence path still refuses Retime ancestry and external
sound events. See [scoped editing](SCOPED_EDITING.md) and
[one-play Hold length](SEMANTIC_MACROS.md#one-play-hold-length).

The ordinary `SetHoldDuration` path also retains old downstream Source, Repeat
gap and owned-play sample clocks when the root duration changes, including
initially unbound owners. Undo restores the complete binding state. See the
[clock qualification](qualification/hold-duration-clocks-2026-10-20.md).
For fixed affine windows, movement capture stays inside the window; the Retime
recipe, outer siblings and outer Repeat placements stay fixed.
Unused Repeat defaults preserve the entire binding state without captures.
See the [fixed-window qualification](qualification/fixed-retime-hold-clocks-2026-10-20.md).
Ordinary Hold resizing also retains moved physical owners beneath Preserve/Shift
on the correct processing-input PointCeil clock. Its current operand rebuilds
DSP while existing output bindings and fractional resumes remain unchanged.
See the [processing-input qualification](qualification/processing-hold-clocks-2026-10-20.md).
An independent sound on the fixed Retime output keeps its exact source phase,
sample offset and journal when an interior Hold changes. Its proof checks the
unchanged output owner and placement ancestry; the current physical operand
still rebuilds. Child-owned sounds whose processing subtree changes retain
their explicit refusal. The native inspector now exposes ordinary Hold resizing
inside fixed unclocked Edit Retimes with no Repeat ancestry. Its duration-only
capture supports Enter, command entry, recording, calls and dot, continuing at
the first sampled visible picture after every crop. See the
[native qualification](qualification/native-fixed-retime-hold-2026-10-21.md) and
the
[output-sound qualification](qualification/fixed-retime-output-sound-2026-10-21.md).
This remains separate from the dedicated clocked-Retime duration path.

## Speed changes after an exact pause

An ordinary tape-speed stage that owns exact pause clocks can be adjusted in
place under ordinary Sequence ancestors. Its exact selected input interval
stays fixed, including fractional endpoints. Only its output extent changes;
the retained fragment clocks and recipes stay intact. For example, a 12-frame
input stretched to 24 frames and then given a three-frame pause has an exact
13½-frame input. `:retime 7/8 pitch=tape` resolves that input to 15 output
frames. The inspector then shows and pre-fills `9/10`, its effective speed.
Macros resolve each staged update against the same exact input.

Existing interior audio bindings remain unchanged. Missing bindings capture
their visible entries on the pre-edit clock, and moved outside owners retain
their own entry phases. Audio still uses native-source preparation followed by
retained-domain transfer; multiplying the coordinate slopes does not replace
these two filtering stages. Admission checks the actual historical lattice's
transfer against the inclusive `1/64..64` range, including successive changes.
For an ordinary nonrepeated carrier, no allocated output samples imposes no
transfer bound.
For SelectedPlacement audio, an empty historical domain is tested after
intersecting the selected crop with current meaningful Source support. Direct
silent Holds return suppressed zeros without resampling a hidden carrier;
nonempty Source and RoomTone domains keep the transfer bound. See the
[crop-carrier qualification](qualification/source-crop-carriers-2026-10-20.md).

One inner Repeat level is also supported, including owned plays and independent
gaps. Every physical carrier retains its own clock and filtering stage. Admission
checks all historical rate classes, including born PointCeil plays beside older
root-clock plays, with bounded work over authored runs and sparse overrides.
Missing carriers capture their pre-edit entries once. A representative play
cannot prove its whole class empty, so repeated classes receive no empty-carrier
exception. Default gaps, including silent gaps, retain transfer checks. See the
[inner-Repeat qualification](qualification/inner-repeat-speed-2026-10-20.md).

This path requires an integral stage output extent and tape pitch on the
updated stage. Nested inner Repeats, processing stages, and non-Sequence
ancestors still refuse changed parameters. Explicit wrapping keeps its existing behavior.
Identical raw `SetRetime` parameters preserve no-op admission; semantic and
native unchanged commands create no revision. See the
[qualification](qualification/clocked-speed-update-2026-10-20.md).

## Pitch shift

`PitchPolicy::Shift { semitones }` is pitch-preserving processing with a fixed
shift of whole semitones (nonzero, within ±24, the qualified range of the
canonical Signalsmith adapter), independent of the duration. Every place that
asked whether a Retime is a "nonunity Preserve" stage now asks
`PitchPolicy::processes(unity_rate)`: pitch-preserving with a nonunity rate or a
nonzero shift. So a unity-speed shifted Retime is a real processing stage for
planning, owned audio bindings (including the Preserve-input point clock),
frozen contexts and reference clocks, definition placements, composite
insertion, moves and slice capture, and the stage descriptor passes its
semitones to `CanonicalRecipe::with_rate`, which keeps them in the preparation
cache identity. A first version missed five hardcoded `Preserve` matches
(binding clock admission, reference clocks and processing domains, binding
scope crossing and definition placement), so any edit that captured audio
clocks in a project with a shift failed; they now use the same predicate, and
`shifted_stages_keep_their_bound_pcm_through_insert_split_and_move` (decoded
PCM, unity and nonunity shifts) and
`split_edits_capture_clocks_beside_a_pitch_shifted_stage` cover InsertTime,
Split, MoveRange and a J-cut's Roll. The remaining literal `Preserve` patterns
construct test fixtures, Partitions (which never shift) or UI text.
Partitions cannot carry a shift; a hanging tail inside a shifted stage is invalid,
as inside a speed change.

`:pitch +3st` (the `st` unit is optional) shifts the selected beat at its
current speed: an ordinary Retime keeps its exact speed and changes its pitch
through `SetRetime`; any other beat, including a Split fragment, is wrapped in a
unity-speed Retime. `:pitch 0` returns a stage to plain `Preserve`.
`:retime 0.75 pitch=-2st` combines a speed and a shift. The inspector shows
"Shifted +3 semitones". Evidence:
`pitch_shift_is_a_bounded_pitch_preserving_stage_independent_of_duration`, real
PCM in `rate_and_pitch_changes_use_fresh_output_but_keep_bound_child_pcm`
(unity +12 and nonunity −5 shifts against an independently rebuilt stage), the
`audio-treatments` replay and the `pitch-shift` preview/export fixture, which
measures a 1 kHz tone pause shifted an octave at about 4,000 zero crossings per
second against 2,000 for the unshifted tone. `WrapRetime` and `SetRetime`
refuse a processing stage exceeding 134,217,728 combined input/output samples
or 86,400,000 input or output samples at edit time; tape speed is unaffected. Streamed
input no longer requires a full input buffer or the old 21.8-second cap.

## Verification boundaries

The existing shared picture plan and canonical audio path evaluate the resulting
Retime, including Preserve processing and FollowSpeed sampling. This increment
does not add a separate preview algorithm. Native descendants under Repeat or
Retime, arbitrary range/occurrence selection, reversing composite structure,
variable speed and mastered output remain open. Independent pitch shifting
(`:pitch`, `PitchPolicy::Shift`, see [Pitch shift](#pitch-shift)) and encoded
export (the `retime-half` and `pitch-shift` fixtures in
[preview-export-2026-10-04](qualification/preview-export-2026-10-04.md)) are
implemented. Reversing an Original passage exists as a Hold provider
([reverse](REVERSE_AND_TAILS.md)).

The current Preserve preparation path streams at most 65,536 input frames
at once and spills outputs above 1,048,576 frames to private temporary files.
The shared preparation budget counts 134,217,728 input plus output frames
across newly prepared stages, and prepared files are bounded to 1 GiB;
nested stages can exhaust either budget even when each individual edit fits.
The 30-minute per-stage ceiling is admission, not an interactive performance
claim. Background preparation remains open. See
[disk PCM qualification](qualification/disk-pcm-2026-10-11.md).
Tape speed uses the existing bounded resampler. A requested speed that quantizes
to unity needs no Preserve stage.

The `retime` UI replay exercises pointer entry/cancel, command preview, initial
wrapping, adjustment without compounding, explicit nesting, three undos and
non-destructive Original context. Replay requires actual Metal; its existence
does not establish that those steps ran. See the
[qualification record](qualification/retime-editing-2026-09-27.md) for executed
tests, review findings and unavailable native evidence.

## Speed ramps

`:ramp FROM TO [steps=N] [pitch=tape|preserve]` (semantic `Ramp`) changes the
selected direct child's speed from `FROM` to `TO` as one Compound and one
Undo. Macros record it and `.` repeats it on another beat from that beat's
own length.

### Continuous ramps

Without `steps` (since 2026-11-09) the ramp is continuous: `WrapRetime` with
`PitchPolicy::TapeRamp { ramp: SpeedRamp { from, to } }`. The rate changes
linearly across the Retime's output. For input length `L`, output duration
`D` and reduced integer weights `A:B = from:to`, the rate at output fraction
`u = t/D` is `r(u) = (L/D)·2(A + (B − A)u)/(A + B)` and the input position is
its exact integral

`x(u) = L·(2A·u + (B − A)·u²)/(A + B)`,

a quadratic in rationals with `x(0) = 0` and `x(1) = L`
(`deadpan_core::SpeedRamp::input_offset`). `D = 2L/(FROM + TO)` (the beat's
length at the mean speed) is rounded half-even once; the stored speeds are
the rate's shape, so the realised endpoint speeds are the authored ones times
`2L/(D·(FROM + TO))`, and the inspector shows them (`Ramp 0.988× to 1.975×`
for `:ramp 1 2` on 40 frames). Both realised endpoints stay within 1/16x to
16x. The ramp stores only its shape, the reduced weights `A:B` (each at most
65,535, so `:ramp 2 6` and `:ramp 1 3` store the same `{"start":1,"end":3}` and
differ only in duration); two `u16`s keep `PitchPolicy` six bytes, so every
plan and audio structure that embeds it keeps its measured layout, and the
integer evaluation below stays inside `i128`.

Pictures: every output frame center reads `x` exactly through the shared
picture plan (`retime_local`), anchors descend through it
(`retime_child_position`), and nothing is accumulated or rounded per frame.
Picture coverage reports `PictureClockSlope::Varying` for spans below a ramp
(consumers sample each position; ordinal-range shortcuts, transcript search
inside the ramp and AI-pause conditioning across it decline). The quadratic
has no rational inverse, so an interior child position has no exact edit time:
anchor mapping and picture owner-clock composition refuse inside a ramp, and
only its endpoints map.

Audio: a ramp is an audio processing stage (`PitchPolicy::processes` is true),
prepared whole like a Preserve stage and cached by its descriptor, which
carries the ramp. `deadpan-audio`'s `RampSampler` evaluates every output
sample's input position from one integer quotient,

`q(j) = L·(2A·D·c·j + (B − A)·n·j²) / ((A + B)·D²·c)`

for frame rate `n/den` and `c = 48000·den`, splits the integer floor before
converting the remainder to `f64`, and never carries phase between samples or
blocks. Its Blackman-Harris windowed sinc (half-width 64 input samples) widens
and lowers its cutoff (`0.95/r`) with the local rate `q'(j)` above 1x, the same
kernel and passband as constant tape speed. The child is read once through the
ordinary stage-input path with a sliding window; outside the input it is zero.
Each sample's taps start from two exact trigonometric evaluations and advance
by rotation (`KernelTaps`), so a 16x ramp's 2,048 taps need no per-tap
trigonometry. The stage reports `Stretching` progress like Preserve.

Known approximation outside the PCM: structural policy inspection
(`stop_at_preserve = false` walks such as Hold-policy flattening) still
composes through a ramp with its mean rate. Its contents are one Original
beat with no Holds, treatments or edges, so the only policy it can report is
that beat's own audio exhaustion; an Original beat whose selected audio ends
inside the ramp before its picture would have that boundary placed at the
affine time. Ordinary Original beats carry audio across their whole picture;
an `:audio-lag` applied before ramping can create this case, which is not
separately refused.

Scope, enforced by document validation so every edit that would break it
refuses: a ramp holds exactly one Original beat (a Source, possibly a split
fragment's unity Partition or inside single-child groups), on an ordinary
integral clock with no derived clock, and with no gain treatments or edge
policies of its own (those flatten onto the output through the affine owner
clock; set them on the ramp, whose own clock is its output). A pause, Repeat,
seam, other speed change or InsertTime inside it refuses, because each would
need either the missing inverse or an affine sample clock. Marks descend
through a ramp exactly; a mark whose host lies strictly inside one has no
rational edit time and is lost as `OutsideMapping` by its ordinary loss
policy, while marks at its endpoints survive. `:pitch` on a ramp refuses
(`:retime SPEED pitch=...` explicitly replaces it with one speed). Split of a ramp beat is exact: each
side is a unity Partition over a complete copy of the ramp. Undo, Redo, dot
and macros carry it as ordinary Retime intent (tested). Repeat wrapping,
Explode and occurrence isolation copy it as Retime intent like any other
processing stage; they have no ramp-specific test yet.

`pitch=preserve` without steps refuses ("a continuous ramp keeps tape pitch;
for pitch=preserve use :ramp FROM TO steps=N"). Signalsmith Stretch itself
accepts a different input/output ratio on each `process` call, but Deadpan's
canonical schedule (`CanonicalRecipe`/`StreamRecipe`, native `canonical.hpp`)
pins one exact rate per stage and its qualification covers only that; a
variable-rate schedule with a documented deviation bound is the remaining
work for pitch-preserving continuous ramps.

Evidence: `speed_ramp::tests` (exact endpoints, monotonic mapping, bounds),
`a_continuous_ramp_is_one_retime_on_the_exact_quadratic_clock` (planner,
Compound replay and inverse, refusals, exact Split), the plan's
`ramp_pictures_sample_the_exact_integral_at_every_frame_center` (all 72 frame
centers at 30000/1001 fps, ordinals 0 to 89, `Varying` coverage) and
`a_ramp_holds_exactly_one_original_beat`, the audio
`ramp_resample::tests` (exact positions against the core rational; a 1 kHz
tone through 1/2x to 2x and 3x to 1/3x ramps within 5e-7 of the analytic
`sin(ω·q(j))`; arbitrary block sizes bit-identical to one render, no seam
curvature), the `continuous-ramp` release preview/export fixture and the
`creative-dot` replay's continuous ramp, dot and refusal checks.

### Stepped ramps

With `steps=N` (2 to 32, since 2026-11-08; `pitch=` is then required) the
selected direct child splits into `N` equal parts at boundaries rounded
half-even once each, then each part is wrapped in its own ordinary Edit
Retime at the speed interpolated at the part's middle,
`FROM + (TO - FROM)·(2k + 1)/(2N)`, resolved exactly as `:retime` resolves it.
Each part stays an ordinary speed change that can be adjusted afterwards. The
rate is piecewise constant: each part is its own processing stage, so
Preserve parts start a fresh stretch context at each seam. Evidence:
`a_ramp_splits_a_beat_into_equal_parts_each_at_its_interpolated_speed`,
`ramps_take_two_speeds_optional_steps_and_an_explicit_pitch` and the
`creative-dot` replay's stepped ramp checks.
