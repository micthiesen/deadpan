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
refuse a processing stage whose input exceeds the processor's 1,048,576-sample
bound (`MAX_PRESERVE_INPUT_SAMPLES`, about 21.8 s) or whose output exceeds eight
times it, at edit time; tape speed is unaffected.

## Verification boundaries

The existing shared picture plan and canonical audio path evaluate the resulting
Retime, including Preserve processing and FollowSpeed sampling. This increment
does not add a separate preview algorithm. Native descendants under Repeat or
Retime, arbitrary range/occurrence selection, reverse, variable speed, independent
pitch shifting, mastered output and encoded export remain open.

The current Preserve preparation path bounds each stage to 1,048,576 input
audio frames and 8,388,608 output audio frames. At 48 kHz its input cap is about 21.8 seconds.
Authoring a longer stage is valid, but audition can report
`AudioPreparationLimit`; this increment does not qualify streaming Preserve
preparation for a whole long Original. Tape speed uses the existing bounded
resampler. A requested speed that quantizes to unity needs no Preserve stage.

The `retime` UI replay exercises pointer entry/cancel, command preview, initial
wrapping, adjustment without compounding, explicit nesting, three undos and
non-destructive Original context. Replay requires actual Metal; its existence
does not establish that those steps ran. See the
[qualification record](qualification/retime-editing-2026-09-27.md) for executed
tests, review findings and unavailable native evidence.
