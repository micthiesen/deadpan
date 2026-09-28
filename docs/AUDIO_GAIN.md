# Audio gain and owner clocks

Specification sections 7.5, 8, 10 and 17 require selective emphasis, preserved
intentional dynamics and shared preview/export treatment semantics. The
[gain interface board](design/boards/clip-gain-board-v2.png) is the native target.

## Current boundary

The core gain types define bounded recipes and evaluate them without media I/O.
The plan owner query reports exact structural clocks without splitting the
continuous Original voice into new processing engines. These are preparation
contracts. They do not yet attach gain to `BeatNode`, persist a gain command,
change rendered PCM, or expose a native envelope editor. Core schema 32 and
database schema 38 remain unchanged. Existing root `SoundEvent` gain continues
to use its qualified sound path.
The [qualification record](qualification/gain-clocks-2026-09-27.md) retains
independent review findings, command results and verification limits.

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
treatment layers. Document and plan integration must actually invoke these
aggregate checks; exporting a validator alone does not enforce a document limit.
Wire collections reject excess entries before retaining an unbounded vector.
The standalone `AudioTreatments::from_json` and `to_json` boundary caps encoded
JSON at 512 KiB, including inherited exact-ratio decimal strings. A streaming
curve visitor rejects unknown keys and malformed controls before consuming
their payloads. Direct Serde embedding still requires a byte-bounded enclosing
document or command; collection counts alone do not bound encoded bytes.

## Ownership and processing

The intended canonical order is complete voice time/pitch mapping, existing edge
treatment, authored gain in its declared owner clock, group mix, final limiter,
then independent monitor gain. Ownership selects the clock and contributions;
it does not insert gain ahead of an ancestor Preserve processor. Preserve uses
the exact nominal time map for automation, without claiming sample-level speech
correspondence. Existing raw/time-mapped inspection APIs retain their meanings.

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
distinct owner from the whole Repeat. Work and span limits bound large queries;
a distant play in a compact Repeat must not expand preceding plays.

Bindings retain source/Preserve recipe clocks while independently current outer
group clocks remain current. Resolving only the root frame, or only a terminal
Source local time, loses information after an odd-sample insertion or resume.
Transparent partitions must preserve full underlying envelope origins and curve
shape, rather than normalizing every fragment. Exhausted support and missing
retained provenance must be explicit; an inspection query is not media admission
or proof that all treated voices can already be rendered.

## Required integration

Fixed Sequence-local keys remain in that Sequence clock on an internal insertion.
Content-following emphasis needs the appropriate Source owner or explicit
content anchors, not an incidental shift of smoothstep/cubic endpoints. Shortening
an owner clips evaluation but retains hidden keys; re-extension reveals them.
Whole-owner trim follows the complete current allocation. Split/refinement must
retain treated partitions, and Ungroup must preserve equivalent processing or
explicitly reject an unsupported transformation.

Node treatments, reversible setters, occurrence isolation and duration edits
need a new core/store schema and frozen schema-32 history replay. Legacy wires
must reject modern treatment fields even when explicitly empty or null. Old
neutral documents gain no invented gain intent. `FrozenAudioContext` should
retain a separate owner-treatment map and advance its own closed vocabulary;
`FrozenAudioLayout` and raw-audio lineage stay timing/content-only. Context
authentication still compares complete retained authored evidence.

DSP integration must add finite dB in a wide accumulator and convert once to
amplitude, preserving a bit-identical unity path. Reject nonfinite output rather
than clipping to conceal a preparation error. Gain belongs before the shared
limiter and its halo/cache calculations. Muted dependencies still need source
admission, including cache hits; gain mute does not grant silence/tail policy.

Native temporary editing must bind the owner, session, base revision, draft
identity and audition window. Before/Draft compares the same heard position.
Distinct draft PCM must never share a canonical cache or resume token merely
because the base revision matches. Enter commits once; Escape restores entry
state. Normal `+`/`-` change selected audio or the current beat by 3 dB, with scope
and result shown; native text, Camera and Placed sounds retain their own keys.

Persistence, real PCM equivalence, cache/admission failures, native routing,
keyboard/IME behavior, measured waveforms, aesthetic comparison and export remain
required. A recipe type, clock query or generated board closes none of those
requirements by itself.
