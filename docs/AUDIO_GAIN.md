# Audio gain and owner clocks

Specification sections 7.5, 8, 10 and 17 require selective emphasis, preserved
intentional dynamics and shared preview/export treatment semantics. The
[gain interface board](design/boards/clip-gain-board-v2.png) is the native target.

## Current boundary

Core schema 33 attaches `AudioTreatments` to `BeatNode`, with direct and
occurrence `SetAudioTreatments` commands and reversible patches. Database 39
replays the complete older chronology, including database 38 through frozen
core 32. Neutral legacy nodes gain no invented recipe. `FrozenAudioContext`
schema 4 retains complete treatment evidence in a separate sparse owner map.

The canonical authored bus evaluates gain after complete time/pitch and existing
edges, before mixing and the shared limiter. CLI `command` accepts the typed
setter; `inspect-audio --authored-bus` exposes at most 256 pre-limiter samples.
Original/root sound scopes remain independent. The native app now implements
captured beat gain commands, an unsaved exact envelope/mute editor and
same-window Before/Draft audition. Its focused Metal replay passes 266 gain
checks and the 5,456-case shortcut audit; all 298 app/harness tests pass.
The [native-gain qualification](qualification/native-gain-2026-09-28.md)
records corrected visual runs, native CUA editing/cancellation, retained failures
and the separate performance and listening limits.
The [measured beat overview](WAVEFORMS.md) now has scoped
[qualification](qualification/gain-waveform-2026-09-28.md). It measures the
committed owner before effects, separately from full-mix Before/Draft audition.
Waveform editing and an encoded export path remain open.
The [earlier foundation qualification](qualification/gain-clocks-2026-09-27.md)
records the pure recipe and owner-query checkpoint; it does not qualify this
later persistence and PCM integration.
The [authored-gain record](qualification/authored-gain-2026-09-28.md) tracks the
persisted treatment and PCM increment's review, verification and remaining limits.

## Recipe and numerical contract

`AudioTreatments` has an explicitly serialized closed stage order. Its current
vocabulary is only `ClipGain`; a configured unity recipe remains authored intent,
distinct from an absent stage. This does not introduce a plugin host.

`ClipGain` retains an independent trim, whole-owner mute, fixed gain envelopes
and half-open mute ranges. Adjusting trim preserves all curves and mute choices.
`GainDb` admits integer millidecibels from -96000 through +24000. The lower bound
is finite gain, never a mute sentinel. Overlapping envelopes add dB without
normalization or clamping; mute remains a separate exact boolean.

Every envelope declares `GainClock::OwnerOutput` and exact nonnegative
owner-local project-frame coordinates. Rational coordinates can represent a
measured source/sample-derived boundary without first rounding to a project
frame. The range is half-open. Outside it an envelope contributes exactly zero
dB. Keys are right-continuous; the final endpoint is excluded with the range.
Step, Linear, Smoothstep and explicitly bounded Cubic value controls are distinct
serialized choices. Interpolation occurs in dB with deterministic Q32 interior
arithmetic; range selection and endpoint comparisons stay exact. Core returns
evaluated millidecibels and mute, not floating-point PCM or amplitude.

Per owner, the bounds are 16 envelopes, 64 segments per envelope and 64 mute
ranges. Every authored endpoint/control is bounded independently. Public
validation also charges an aggregate 100000 records and a separate 16 active
treatment layers. Documents, incoming subtrees, patch sides and frozen contexts
enforce those bounds; the PCM consumer independently checks active owner layers.
Wire collections reject excess entries before retaining an unbounded vector.
The standalone `AudioTreatments::from_json` and `to_json` boundary caps encoded
JSON at 512 KiB, including inherited exact-ratio decimal strings. A streaming
curve visitor rejects unknown keys and malformed controls before consuming
their payloads. Direct Serde embedding still requires a byte-bounded enclosing
document or command; collection counts alone do not bound encoded bytes.

## Ownership and processing

The canonical authored-bus order is complete voice time/pitch mapping, existing edge
treatment, authored gain in its declared owner clock, group mix, final limiter,
then independent monitor gain. Ownership selects the clock and contributions;
it does not insert gain ahead of an ancestor Preserve processor. Preserve uses
the exact nominal time map for automation, without claiming sample-level speech
correspondence. Existing raw/time-mapped and edge-only inspection APIs retain
their meanings.

A Source treatment affects its Original contribution, not unrelated root-owned
sound events. A Repeat child envelope restarts on each play; a Repeat-owned
envelope spans the complete Repeat, including its gaps. A group treatment must
apply to its contained voices once. Root sound gain and group gain must compose
without duplicating the group factor. Nested sound ownership remains separate
unimplemented work.

`RenderPlan::audio_owners` returns contiguous sample spans with outer-to-inner
owners, exact affine owner-local maps, definition/occurrence namespaces and
current-versus-retained clock provenance. Checked physical `AudioDomain` and
intrinsic PointCeil `AudioDefinition` queries retain their respective namespaces.
Private construction binds each result to its immutable plan; inspection data
cannot be deserialized as a trusted plan handle. The Repeat's default gap is a
distinct owner from the whole Repeat; it contributes no second node treatment.
Work and span limits bound large queries;
a distant play in a compact Repeat must not expand preceding plays.

Bindings retain source/Preserve recipe clocks while independently current outer
group clocks remain current. Resolving only the root frame, or only a terminal
Source local time, loses information after an odd-sample insertion or resume.
Transparent partitions must preserve full underlying envelope origins and curve
shape, rather than normalizing every fragment. Exhausted support and missing
retained provenance must be explicit. The separate `audio_gain_owners` query
marks known exhausted retained support inactive while keeping current ancestor
clocks. Current-clock errors and missing provenance still reject. The consumer
requires zero Original PCM under inactive support, while root sounds continue
through their independently current root clock. An inspection query never
constitutes media admission.

## Structural and persistence behavior

Fixed Sequence-local keys remain in that Sequence clock on an internal insertion.
Content-following emphasis needs the appropriate Source owner or explicit
content anchors, not an incidental shift of smoothstep/cubic endpoints. Shortening
an owner clips evaluation but retains hidden keys; re-extension reveals them.
Whole-owner trim follows the complete current allocation. Split/refinement
retains treated partitions. Ungroup explicitly rejects a treated Sequence until
an equivalent distribution of its owner clock and contribution scope exists.

Node treatments, reversible setters, occurrence isolation and duration edits
use core 33/database 39 with frozen schema-32 history replay. Legacy wires
reject modern treatment fields even when explicitly empty or null. Old neutral
documents gain no invented gain intent. `FrozenAudioContext` schema 4 retains
all nonempty recipes, including picture-only and group owners, separately from
timing-only `FrozenAudioLayout` and raw-audio lineage. Context schemas 1 through
3 reject the new field even when null or empty; unchanged older contexts keep
their original version. Authentication compares complete retained evidence.

DSP adds finite dB on a wide Q32 accumulator and converts once to amplitude,
preserving the unity path. Nonfinite output rejects rather than clipping to
conceal a preparation error. Gain precedes the shared limiter and its halo/cache
calculations. Muted dependencies still need source admission, including cache
hits; gain mute does not grant silence/tail policy.

## Saturation

`AudioTreatmentStage` admits two stages, `ClipGain` and `Saturation`, and a
recipe serializes their order: each present stage is listed exactly once and
no absent stage is listed (`MAX_AUDIO_TREATMENT_STAGES` is 2). `Saturation {
drive }` holds an exact `GainDb` drive from 0 through 24 dB. Its transfer is
the memoryless soft clipper `tanh(drive · x)`: small signals gain the drive,
peaks approach ±1, digital silence stays exact silence, and because it keeps no
state it evaluates identically at any read or tile boundary in preview and
export. A new stage is appended after clip gain (specification §10.2 order);
a recipe authored with `Saturation` first trims after the clipper.

Owners are evaluated innermost first. Between nonlinear stages, gain factors
(trim, envelopes and Repeat escalation, which enters before the owner's own
stages) still add exactly in Q32 millidecibels; each saturation closes the gain
accumulated before it. Without a saturation stage the chain is the previous
single exact gain sum, bit for bit. Saturation applies to each voice (the
Original, a placed sound, a beat sound) through its owners before mixing and the
shared limiter, not to a summed group bus. Mute still yields exact silence.

`:saturate 12dB` and `:saturate off` change the captured beat as one semantic
`SetAudio { change: saturation }` instruction (recorded in macros and repeated
by `.`); a single Repeat play is edited directly. While recording, `+`/`-`,
`:gain N` and `:gain +=N` record `SetAudio` trim and step changes. The
inspector lists the drive and its order; the gain section names it. Evidence:
`saturation_is_an_ordered_bounded_stage_with_a_closed_wire`,
`saturation_shapes_each_owner_output_inner_first_in_its_serialized_order` (real
decoded PCM against an independent oracle in both orders), the
`audio-treatments` replay and the `saturation` preview/export fixture. Oversampled
shaping remains open: at high drive the memoryless clipper aliases.

## Native editor and qualification

The [native integration design](GAIN_EDITOR_DESIGN.md) records captured command
targets, service-issued proposals, source admission and same-window audition.
Normal `+`/`-` adjusts the selected beat by counted 3 dB steps, with Placed sounds
retaining event precedence. Original and catalog Sound focus cannot change a
retained beat. `:gain -3.125` (or `:gain +6dB`; the unit is optional) sets
absolute trim and `:gain +=3dB` / `-=3dB` change it relative to the current
trim; `:gain-mute` toggles true mute;
`:gain` or the inspector opens the complete existing recipe without changing it.
Command entry captures a result, including an absent target, before typing begins.

The draft edits trim, whole-beat mute, multiple envelopes and separate mute
ranges. Native fields accept exact millidecibels and nonnegative decimal or
rational owner frames. Explicit row buttons apply buffered values to the draft;
pending or invalid fields block Apply. The graph shows the selected envelope's
dB contribution, with clickable keys and exact key/range fields. Each ending key
owns its incoming Step, Linear, Smoothstep or Cubic segment, including both cubic
controls. Range edits retain interior keys or reject; they do not discard hidden
keys after a duration shrink. Point dragging remains open. The separate
[measured beat overview](WAVEFORMS.md) retains the committed beat's audio before
effects throughout the draft; it is independent of full-mix Before/Draft audition.

Temporary editing binds the owner, scope, cursor, session, base revision and
draft/change identity. The writer validates a proposal without writing history.
Before/Draft uses one captured beat window in the full Sequence mix and the
admitted heard sample; distinct proposals carry distinct PCM/cache/resume
identities. Edits stop older
draft playback. Apply uses the ordinary revision-checked command and one undo;
unchanged Apply creates no history. Cancel restores entry targeting only while
its session/base remain current and retains the accepted picture during recovery.

Tab and Shift+Tab traverse and reveal draft controls while the surrounding
workspace is disabled. Boundary traversal wraps between Cancel and the heading.
Enter/Space on the heading apply/audition; native fields and focused buttons keep
their own input. Escape cancels outside active IME or popup ownership. The
focused replay checks complete populated keyboard circuits and actual paint/hit
clips at 960×640 and 1280×820; those captures were compared with the gain board.
Injected delivery proves UI handling, not device output. Physical keyboard/IME
and VoiceOver acceptance, long-source performance, listening and export remain
required. Native CUA also exercised exact trim/key edits, focus reveal, literal
shortcut text and Escape; the before/after project dumps were identical. This
increment does not close the complete DP-09 audio workflow.


A Repeat's per-play [escalation](REPEAT_ESCALATION.md) gain is added on the
Repeat owner like an authored factor, from the span's play position.
