# Equalizer processing

Specification §§10.2 and 17 require shared EQ/filter processing with explicit
history, latency and tails. `AudioTreatments` now stores an explicit `equalizer`
stage, and semantic `set_audio` changes commit or remove its bounded recipe.
The shared authored bus recursively evaluates EQ on Original and independent
sound owners, including nested groups and retained history, before mastering.
Shared saturation processes the combined sum in its authored order. Preview
and emitted-file verification pass for the real headless Render path. Native
authoring controls and remaining automation/send/tail work remain open.

## Stored and recursive processing

The stage vocabulary is gain, EQ, saturation and balance, with balance last.
Each present recipe appears exactly once in the serialized order. New EQ is
inserted before saturation/balance without reordering existing stages. Updates
keep its existing position; gain, saturation, balance and owner-prefix edits
preserve EQ. A semantic change has the form:

```json
{"type":"set_audio","change":{"type":"equalizer","recipe":{"bass":0,"middle":6000,"treble":0}}}
```

`recipe: null` removes EQ. This uses the ordinary selected-owner transaction,
compound replay and undo path. The native inspector reports stored band and cut
parameters; a native EQ authoring command has not yet been added.
Frozen audio contexts capture schema 10 and authenticate the ordered recipe;
older schema declarations reject EQ rather than adopting new semantics.

`authored_eq` chooses the outermost remaining EQ in an owner's program, evaluates
its complete input prefix recursively and applies its suffix afterwards. Each
checked context owns its raw time/pitch processor. Captured domain queries can
resolve children outside current delivery; retained contexts keep their exact
consumer map. All temporal intermediates remain stereo f64. Explicit silence
and exhaustion gate each output; authored mute stays at its program position.

Filtered tiles are reused within one preparation request. Identity includes the
parent processing context, checked owner provenance, stage position and exact
affine clock; a whole Repeat's changing play is evaluated by queries instead of
creating a new owner. Cumulative work, frames, depth and PCM residency cover the
recursive graph. EQ compilation and tiles spend separate fixed work/frame
allowances; a finite tile is not another canonical stretch stage. Tile reuse
does not survive the request, and no realtime latency claim is made.

Tail input uses the same ordered processing while omitting shared exterior
owners. Those owners process the tail's output once. The tail recursion guard
remains separate from each EQ graph, and source dependencies cover all halos.
See [authored EQ qualification](qualification/authored-eq-2026-10-11.md).

## Shared independent sounds

`shared_eq` uses the same Original contexts alongside complete current and saved
sound contexts. `shared_mix` retains the gain/EQ/saturation order at each owner,
summing before shared saturation. Root SoundEvents enter at the root only.
Per-sample contribution permission sets select complete filtered prefixes, so an
allowed sound cannot release forbidden Original or peer history during a Hold.
The permission set participates in tile identity; all intermediates remain f64.
Neutral EQ executes as an identity on current delivery, preserving saved crop
resampling exactly. See [shared EQ qualification](qualification/shared-eq-2026-10-12.md).

## Owner input contexts

Independent sounds now have a separate checked
`RenderPlan::audio_sound_owner_context`. It exposes their complete current
structural input behind exterior crops, including negative sample positions,
without adopting an Original's retained clock. Enclosing Preserve input and
inner selections remain intact, and Hold policy follows the current structure.
Cold decoded PCM matches uncropped references across split reads and nested
Preserve. See [sound history qualification](qualification/sound-owner-context-2026-10-11.md).
The beat-sound preparer now consumes this handle with saved move/copy clocks.
`read_beat_sound_owner_input` reads their full input with current Hold gates,
event edges and gain, retaining f64 sums. Expanded history keeps the original
rounded delivery anchor instead of rounding a new crop boundary; a fractional
copy regression caught a one-sample shift in the first implementation.
See [saved sound input qualification](qualification/beat-sound-owner-input-2026-10-11.md).
The authored group EQ graph still needs to consume these per-voice preparations;
the explicit mixed-EQ restriction remains until stage ordering and final
per-voice policy are wired.

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
treatments. The stored recursive path described above extends that foundation
to authored EQ; the later shared graph now consumes independent sound input.

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

Typed construction and deserialization reject out-of-range gains,
frequencies, reversed/narrow cuts and unknown fields. All three gains are
required; omitted cuts are off. `AudioTreatments.equalizer` stores this recipe
beside its explicit `equalizer` order entry. Parameters are static; there is no
EQ automation stage.

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

Complete captured native controls and independently timed automation. Extend
qualification across the remaining nested bindings, copies, Repeats and
time/pitch interactions, and complete arbitrary sends with incoming-tail policy.
Request-local filtering is implemented; persistent cache reuse across preview,
cold seeks, loops and export, and measured interactive latency remain required.

DP-09 remains partial. The qualified shared preview/export path does not close
the remaining voice/send/tail interactions or final native workflow. These remain
agent work under the full specification. Listening and other hardware remain
subject to the existing owner-verification scope.
