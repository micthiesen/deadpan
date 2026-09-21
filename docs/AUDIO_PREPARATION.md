# Exact source audio preparation

`deadpan-audio` prepares bounded stereo blocks from qualified original PCM on a
worker. It is the source resampling/downmix boundary, not the complete voice
graph, stretch integration, device engine, or export renderer.

## Coordinates and filtering

`ResampleRecipe` retains the authored source selection, exact source origin,
output origin, exact source-samples-per-output-sample step, and allocated output
interval. Each sample uses `x(j) = origin + (j - output_origin) * step` in checked
rational arithmetic. Native rate conversion uses `source_rate / 48000`; rate
changes must be explicit. Never derive that ratio from rounded beat lengths.
The current implementation admits positive constant steps from 1/64 through 64.
It does not claim reverse, variable-rate, or pitch-preserving processing.

The source coordinate is split into its integer floor and fractional remainder
before floating-point filter evaluation. Large absolute timestamps never enter
the filter as floats. Every requested sample uses the same fixed tap order,
regardless of surrounding block sizes or prior seeks. Integer-phase unity rate
copies through the matrix exactly, without an unnecessary low-pass stage.

Other phases/rates use a finite windowed sinc. For step `s`, the cutoff relative
to source Nyquist is `0.95 / max(1,s)` and radius is `ceil(128 * max(1,s))`.
The symmetric four-term Blackman-Harris window uses coefficients 0.35875,
0.48829, 0.14128 and 0.01168. Summation and mixing use f64, with one f32 output
conversion. Full-kernel DC calibration depends only on filter coefficients,
never source level or the number of available edge samples. The transition band
is intentional; this kernel does not preserve all frequencies up to Nyquist.

The engine identity is `deadpan-exact-sinc-bh4-128-v1`. It deliberately uses
stateless exact phase rather than relying on a resampler's hidden running phase.
The pinned FFmpeg SWR API exposes integer drop/insertion and rate compensation,
but no arbitrary initial fractional sampling phase. Its phase-table setting is
not such a setter. Current Rubato also keeps running phase private. Introducing
another engine, phase table or interpolation policy requires new identity and
measured signal/seek equivalence, rather than changing this recipe silently.

## Trims, bounds and failure

Each call returns 1 through 256 output frames. Its one contiguous source read
is bounded to 32,706 frames, including filter context, below the media session's
65,536-frame hard cap. The sampler intersects this read with the authored trim
before I/O. Context outside that selection is zero under
`authored-trim-zero-extension-v1`. It does not extend speech beyond a cut,
clamp source coordinates, or renormalize a short fragment's level. Missing or
excluded PCM inside the required selected range remains an error.

The entire block is staged before return. Cancellation is checked before I/O,
through source mixing, per output sample and every 256 filter taps. Input must
be finite with absolute magnitude at most 16.0; extreme input is rejected, not
clipped. Output can exceed unity because no gain normalization or limiter is
applied here. The kernel has bounded work, but the largest ratios are expensive
and belong in preparation. No callback safety or interactive latency claim is
made. `AudioSession` still fully decodes into its bounded private disk cache.

## Explicit channel interpretation

`deadpan-stereo-speaker-matrix-v1` uses FFmpeg native speaker-bit order:

| Speaker | Left | Right |
| --- | ---: | ---: |
| Front left / front right | 1 / 0 | 0 / 1 |
| Center-only mono | 1 | 1 |
| Center within a larger layout | 1/√2 | 1/√2 |
| Back left or side left | 1/√2 | 0 |
| Back right or side right | 0 | 1/√2 |
| Back center | 1/2 | 1/2 |
| LFE | 0 | 0 |

Other speaker identities, LFE-only input, inconsistent masks and unspecified
layouts are rejected by automatic admission. This is a documented stereo
interpretation, without bass management. Original multichannel media is retained.
No coefficients depend on content amplitude, peak level, or selected word.

`PreparedSource::new` requires a native layout. `with_layout` allows a host to
explicitly interpret unspecified channels, with matching channel count; it
cannot contradict a native layout. It never infers speakers from channel count.
The committed PCM WAV fixtures genuinely have unspecified decoder metadata, so
tests supply the speaker declarations from their known synthetic fixture recipe.
The raw index remains unchanged. Persisting that choice as a user edit and
providing its UI are still required before application use.

## Source binding and integration

`PreparedSource` owns an `AudioSession` and compares its entire reopened index
with retained qualification before exposing PCM. Content identity, stream,
counts, raw observations and derived frame mappings must match. Long arrays are
compared with cancellation checks. Each read checks its returned range, rate,
layout and count. The adapter reads no database and mutates no authored state.

Future cache keys must include original byte and selected-stream identity, full
qualification contract/index, authored trim and exact affine recipe, chosen
speaker layout, and all three engine/matrix/boundary IDs. Neither a pathname nor
an engine ID alone identifies prepared audio. Prepared-cache publication is not
implemented. The [source-stage reader](SOURCE_STAGE_AUDIO.md) connects immutable
plan spans to this sampler; its shared headless host binds assets through their
historical store receipts and verified originals.

The complete voice graph must also honor the plan's retained pitch stages.
Feeding rounded input/output counts into
`deadpan-dsp` does not establish exact fractional phase or mixed pitch semantics.
Edge fades, gain/effects, room tone/tails, master limiting, long-clip stretch,
device scheduling and preview/export comparison remain open. See the
[qualification record](qualification/audio-preparation-2026-09-21.md).
