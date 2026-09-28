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
Original/root sound scopes remain independent. Native gain controls, temporary
Before/Draft audition, measured waveforms and an encoded export path remain open.
The [earlier foundation qualification](qualification/gain-clocks-2026-09-27.md)
records the pure recipe and owner-query checkpoint; it does not qualify this
later persistence and PCM integration.
The [authored-gain record](qualification/authored-gain-2026-09-28.md) tracks this
checkpoint's review, verification and remaining limits.

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

## Required native integration and qualification

The [native integration design](GAIN_EDITOR_DESIGN.md) records captured command
targets, service-issued proposals, source admission and same-window audition.
It is implementation guidance, not evidence of a native gain editor.

Native temporary editing must bind the owner, session, base revision, draft
identity and audition window. Before/Draft compares the same heard position.
Distinct draft PCM must never share a canonical cache or resume token merely
because the base revision matches. Enter commits once; Escape restores entry
state. Normal `+`/`-` change selected audio or the current beat by 3 dB, with scope
and result shown; native text, Camera and Placed sounds retain their own keys.

Native routing, keyboard/IME behavior, temporary audition identity, measured
waveforms, aesthetic comparison, long-source performance, listening acceptance
and export remain required. The persisted backend and generated board do not
close the complete DP-09 audio workflow.
