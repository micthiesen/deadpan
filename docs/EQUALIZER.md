# Equalizer processing

Specification §§10.2 and 17 require shared EQ/filter processing with explicit
history, latency and tails. `EqualizerRecipe`, `deadpan_audio::Equalizer` and
`PreparedOwnerEqualizer` implement parameters, numerical processing and ordered
evaluation of one checked Original owner. They are not yet connected to
`AudioTreatments`, project commands, native controls or the shared authored bus.
Editor playback and export therefore retain their existing treatment chain.

## Owner input contexts

`RenderPlan::audio_owner_input` now resolves an observed owner's raw processing
context with one bounded query allowance. The handle belongs to that immutable
plan and occurrence. It rejects foreign plans, absent owners, changed Repeat
plays and incompatible clocks. A Sequence or whole Repeat keeps its complete
subtree, including hidden siblings behind a transparent Partition, on the
original absolute sample grid.

The first enclosing Preserve or retained binding remains opaque. The handle
reports `EnclosingProcessor` when its output also contains other descendant
owners. Its output must not be mistaken for the requested owner's isolated
contribution, and the effect must not move into that processor's input.
`StageAudio::read_owner_input` evaluates this delivery context through the
existing bounded time/pitch, policy and creative-edge path. It does not sum
independent sounds or apply authored treatments.

A resumed binding's current delivery interval can hide earlier reference
history. `AudioOwnerInput::retained_at` separately exposes its checked full
`AudioBoundDomain` and exact consumer-to-reference sample map. RootRoundEven
and PointCeil labels remain distinct. The existing domain/point readers and
domain transfer evaluate that history without looking up the same positions
through an unrelated current-root sibling. The handle now also queries creative
fades and owner clocks on the consumer grid, including positions before current
delivery. `read_retained_owner_input` evaluates time/pitch and transported edges;
`read_retained_owner_treatments` additionally applies the operand's current
gain/mute, saturation and balance, excluding exterior owners and independent
sounds. The latter evaluates the complete current treatment chain. A temporal
graph must choose explicit stage boundaries before using it as filter input.

[Owner-input qualification](qualification/owner-input-2026-10-11.md) compares
cold reads against uncropped PCM, feeds a cropped group's complete history into
the actual EQ processor, and verifies retained history before current delivery.
It does not qualify authored EQ, mixed-voice permissions or the editor workflow.
The subsequent [retained-treatment qualification](qualification/retained-treatments-2026-10-11.md)
checks transported fades, reference-clock gain/mute and ordered memoryless
treatments. EQ still needs the shared ordered graph and authored integration.

## Ordered owner evaluation

`AudioOwnerClock::treatment_boundary` checks a position in an immutable owner's
explicit treatment order. Matching retains the plan, definition, occurrence,
gap and retained provenance, while permitting a whole Repeat's play clock to
advance. `AudioOwnerInput::observed_at` retains the checked resolving sample.

`PreparedOwnerEqualizer` compiles a recipe at that boundary.
`StageAudio::read_owner_equalized` reads up to 256 signed consumer samples.
It evaluates complete time/pitch and creative edges, all descendant treatments,
then the selected owner's treatment prefix, EQ and treatment suffix. Repeat
escalation belongs to the prefix. Exterior owners and independent sounds remain
outside this Original-only operand. The result stays signed stereo f64.

Each 1,024-sample tile reads its 2,048-sample past and future context. A checked
retained operand supplies history before current delivery; an unbound domain
supplies its full support, including hidden group children. Only positions
outside meaningful support are zero padded. Silent-Hold and exhausted-support
masks apply after filtering. An authored mute instead applies at its explicit
position: pre-EQ mute shapes history, while post-EQ mute zeros output.

The private controller inherits work, deadline and depth, charges tile PCM to
the shared residency limit, and returns dependencies covering every halo.
Construction opens no media. Every read admits its sources, including muted
input and warm Preserve caches. EQ tiles are currently recomputed; no filtered
tile cache or realtime latency claim exists. The
[ordered-owner qualification](qualification/owner-eq-2026-10-11.md) records
direct-convolution, cold partition, retained-clock and policy checks on decoded
fixture audio. This is one owner evaluation, not the recursive authored graph.

## Recipe

The bounded recipe has three complementary tone bands: bass below 250 Hz,
middle between 250 Hz and 4 kHz, and treble above 4 kHz. Each gain is an exact
integer millidecibel value from −24 through +24 dB. Zero means unity; −24 dB is
finite attenuation, not mute. Optional low and high cuts take integer cutoff
frequencies from 80 through 20,000 Hz. When both are present, the high cut must
be at least 80 Hz above the low cut. A low cut is a high-pass filter; a high cut
is a low-pass filter.

```json
{"bass":6000,"middle":-6000,"treble":3000,"low_cut_hz":80,"high_cut_hz":12000}
```

This standalone JSON record does not authorize adding it to an existing node
wire. Typed construction and deserialization reject out-of-range gains,
frequencies, reversed/narrow cuts and unknown fields. All three gains are
required; omitted cuts are off. These static parameters do not yet introduce
automation, treatment order or a persistence schema change.

## Numerical contract

The three bands and both cuts combine into one 4,097-tap symmetric FIR at
48 kHz. Each pass interval contributes the difference between its high and low
low-pass bases. Equal adjacent band gains cancel before kernel evaluation, so
uniform gain without cuts is an exact central impulse scaled once. Each finite
low-pass basis uses a Blackman window and unit DC response. That coefficient
normalization does not measure, normalize or level the source audio.

`EQ_FILTER_ID` versions the coefficient design. The underlying
[`FiniteStereoFilter`](qualification/finite-filter-2026-10-11.md) versions FFT
evaluation separately. Cache consumers must retain both identities, the exact
recipe/kernel and admitted input/context identity. Native processing keeps
signed f64 channels independent until a later bus conversion; there is no
crossfeed, limiter or automatic per-word level change here.

Cutoffs and crossovers are transition centers. An isolated cut is approximately
−6.02 dB at its cutoff; this is a finite transition, not a brick wall or a
biquad's usual −3 dB corner. Tone bands blend in amplitude across that transition.
The [response qualification](qualification/eq-recipe-2026-10-11.md) records the
actual response, including the lowest/highest cutoff and narrowest passband.

One call supplies 2,048 preceding samples, 1,024 core samples and 2,048 following
samples per channel. The symmetric group delay is compensated by look-ahead,
so core output keeps the same sample labels. The required context is 42⅔ ms
on each side, with a fixed absolute tile grid. Construction and reads belong
on preparation workers. Construction checks cancellation during kernel design;
reads retain the native bounded transactional output contract. A fully neutral
recipe copies the core's exact bits, including signed zero and tiny samples,
while still admitting both complete input channels and honoring cancellation.

## Integration still owed

Add the ordered authored stage, typed command and captured editor controls.
Use the checked contexts and reference maps to evaluate complete owner input
before clipping current allocation, preserving historical clocks, copies,
Repeats and nested time/pitch stages. Sum and process
group-owned contributions once, with correct order relative to gain and
saturation. Apply silence and incoming-tail permission per voice, including
independently permitted sounds under a silent Hold. Admit aggregate graph work
and resident memory before opening media and share any materialized cache across
preview, cold seeks, loops and export. Do not treat a halo from the current
physical timeline as proof of these ownership and policy properties.

The numerical tests do not close DP-09 or qualify a working EQ editor. Real-media
preview/export, full owner/policy interactions and the final UI workflow remain
agent work under the full specification. Listening and other hardware remain
subject to the existing owner-verification scope.
