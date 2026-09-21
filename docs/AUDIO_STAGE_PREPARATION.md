# Audio stage preparation contract

`StageAudio` connects immutable audio plans, qualified source PCM and
`CanonicalRecipe::with_rate` on preparation workers. It renders continuous
pitch-preserving retimes through cuts and nested stages. Its output identifies
`time_mapped_pcm_before_effects`; fades, voice effects, mastering, native
playback and export remain open. `SequenceAudio` retains the separate source-only
inspection contract.

```sh
cargo run --locked -p deadpan-cli -- inspect-audio /tmp/example.deadpan --samples 0 256 --time-mapped
```

The same command runs through `deadpan-app --headless`. It reads 1 through 256
samples from one immutable revision without changing the document or history.
The read-only host verifies historical receipts and original bytes. It retains
one decoded source at a time and reopens when the selected asset changes.

## Continuous processing domains

A Preserve Retime around a Sequence processes its selected child signal
continuously across leaf cuts. It must not restart the stretcher for each
`AudioSpan`. Each Retime occurrence is a processing stage with its own stable
origin. Process inner stages before outer stages. Preserve stages retain DSP
history; FollowSpeed stages use the qualified resampler. Overall timing and
steady-tone pitch products do not establish equivalent transient behavior or
intermediate anti-alias filtering.

Let `C = 48000 * fps.den / fps.num`, `[a,b)` be the selected child-frame range,
and `d` the stage's output duration in frames. The exact speed is `s=(b-a)/d`.
Prepare child PCM on the rational grid `X[k]=sampleChild(C*a+k)` so fractional
input phase survives. Preserve consumes this PCM with native boundary
`K(m)=round_even(s*m)`, using unity pitch. FollowSpeed reconstructs at `s*m`.
At 30000/1001 fps, three frames into two has exact speed 3/2; rounded buffer
counts 4805/3203 must not redefine that speed.

If the stage transform is `project_frame=O+T*local_frame`, project sample `n`
requests stage sample coordinate `m=(n-C*O)/T`. Reconstruct the prepared stage
at that exact coordinate. Fractional phase belongs in this sampling operation,
not an offset rounded to an integer native input boundary. Final allocation
still uses the absolute project-frame endpoints rounded once.

## Plan boundary

The plan exposes an occurrence-bound stage descriptor with its complete
`InstancePath`, child identity and selected range, output duration, pitch policy,
exact speed, and the stage's own transform captured before descending into its
child. Retain its intrinsic processing extent separately from outer crops and
the current query. A bounded child-signal query on a declared rational grid must
stop descent at nested Preserve stages so their prepared output remains a
continuous signal. Existing root leaf queries alone cannot supply this contract.

An outer crop or random seek must reproduce the corresponding portion of the
full stage render; it must not redefine an inner stage's history. Integer cache
capacity can enclose a fractional extent but cannot change it. Bind original
identities, speaker layouts, exact grids/extents, ordered stage recipes, context
policies and engine versions in prepared-cache provenance. The current native
1,048,576-frame input limit applies to every intermediate; reject excess until
a qualified long-input strategy exists, rather than resetting arbitrary chunks.

`AudioStage` handles borrow their plan and cannot be reconstructed from a foreign
descriptor. `AudioSignal` uses distinct `SignalSample` indices. Its point grid
owns `[ceil(C*(left-a)), ceil(C*(right-a)))`, with cuts belonging to the right
child. Storage encloses the exact support using ceil: a three-frame NTSC input
stores 4805 points and a two-frame output stores 3204. Final root allocation
remains round-even and emits 3203 samples. Neither count changes speed 3/2.
`audio_processing` stops at nonunity Preserve stages; `query_flattened` retains
leaf policies on the same grid. Unity Preserve is transparent.

## Policies, cache and limits

The renderer validates each complete stage output before decoding its input.
This matters when a Hold owns no input-grid point but acquires output samples
after slowing, including through nested stages. Silent Holds suppress incoming
DSP energy at intermediate and final output coordinates. Final blocks retain
`suppressed` ranges for downstream effects. Room tone, effect tails and implicit
Source speed changes still fail explicitly. No normalization or clipping hides
nonfinite or excessive PCM.

Each prepared occurrence keeps its full intrinsic output and transitive source
dependencies. `PreparedSource` streams the complete validated index, byte
identity, selected speaker layout and preparation engine IDs into SHA-256.
Cache hits revalidate those fingerprints through the revision-aware provider.
A changed interpretation invalidates the entry and its dependent parents;
inconsistent provenance within one read fails without publishing PCM. The cache
is private to one immutable plan and one executable's engine implementations.

Default admission limits are:

- 1,048,576 input and 8,388,608 output frames per Preserve stage, depth 32.
- 16,777,216 resident stereo-frame equivalents, including simultaneous input
  conversion, output and pinned descendants; at most 64 cached stages with LRU
  eviction. Native FFT state, decoder caches and a bounded sampler halo are
  additional memory.
- 64 newly prepared stages and 16,777,216 total prepared input/output frames
  per read. Cache hits do not restart DSP or consume the preparation quota.
- 1024 distinct source dependencies and 65,536 provenance checks per read.
- A shared cooperative deadline, with cancellation checked between bounded
  plan, source and canonical blocks. The host uses ten seconds. Source opening
  retains its separate bounded operation; neither that call nor native
  initialization is preempted at an exact wall-clock instant.

Limits can be reduced, not raised beyond these defaults. Failed reads publish
no block; complete child caches may remain useful for a later bounded retry.
`AudioPreparationLimit`, `AudioPreparationTimeout`, `AudioOperationUnsupported`
and `AudioProcessingFailed` distinguish admission, deadline, policy and DSP
failures. Source identity/layout and range errors retain the source inspector's
existing codes. This bounded worker path is not suitable for an audio callback.

## Evidence and remaining work

Tests cover actual PCM through seams, fractional input origins, nested history,
mixed pitch-policy order, outer-crop and irregular-seek parity, stable repeats
and overrides, output-only Hold policies, cache interpretation changes, eviction,
shared work limits and cancellation. Plan tests separately cover exact grids,
rounding and indexed billion-play queries. See
[qualification](qualification/audio-stages-2026-09-21.md) for the repository gate
and real process evidence.

Long-input preparation, fractional pitch, variable rates/reverse, full voice
effects, gain/fades, room tone/tails, true-peak mastering, listening, background
cache scheduling, device timing and preview/export equivalence remain open.
