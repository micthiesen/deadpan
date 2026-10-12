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

For ordinary derived placement `project_frame=O+T*local_frame`, project sample
`n` requests stage sample coordinate `m=(n-C*O)/T`. The plan retains this in an
explicit [sampling map](AUDIO_SAMPLING.md); root and signal stage recipes consume
its local origin and step. A resumed map can differ from the structural transform.
Reconstruct the full prepared stage at that exact coordinate. Fractional phase
belongs in this sampling operation, not an offset rounded to an integer native
input boundary. Final allocation still uses the absolute project-frame endpoints
rounded once. Persisted Hold resume and retained reference-policy masks remain open.

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
policies and engine versions in prepared-cache provenance. Preserve input now
streams through bounded windows while retaining one continuous processor.
Its complete input/output work and retained output still require admission;
streaming never resets a stage or changes its exact rate.

`AudioStage` handles borrow their plan and cannot be reconstructed from a foreign
descriptor. `AudioSignal` uses distinct `SignalSample` indices. Its point grid
owns `[ceil(C*(left-a)), ceil(C*(right-a)))`, with cuts belonging to the right
child. Storage encloses the exact support using ceil: a three-frame NTSC input
stores 4805 points and a two-frame output stores 3204. Final root allocation
remains round-even and emits 3203 samples. Neither count changes speed 3/2.
`audio_processing` stops at nonunity Preserve stages; `query_flattened` retains
leaf policies on the same grid. Unity Preserve is transparent.

## Policies, cache and limits

[Retained audio contexts](AUDIO_CONTEXT.md) compile into the same stage graph.
Every source read, including RoomTone, nested preparation and prepared-cache
dependency checks, uses explicit context admission with the full retained asset
contract. A provider implementing only ordinary revision lookup cannot supply
context PCM. The production headless host first authenticates the complete
context against retained history, then verifies source receipts and originals.

The renderer validates each complete stage output before decoding its input.
This matters when a Hold owns no input-grid point but acquires output samples
after slowing, including through nested stages. Silent Holds suppress incoming
DSP energy at intermediate and final output coordinates. Final blocks retain
`suppressed` ranges for downstream effects. [Room-tone Holds](ROOM_TONE_AUDIO.md)
prepare an explicit selected source loop with exact overlap timing. Effect tails
and implicit Source speed changes still fail explicitly. No normalization or clipping hides
nonfinite or excessive PCM.

Each prepared Preserve or room-tone occurrence keeps its full intrinsic output and transitive source
dependencies. `PreparedSource` streams the complete validated index, byte
identity, selected speaker layout and preparation engine IDs into SHA-256.
Cache hits revalidate those fingerprints through the revision-aware provider.
A changed interpretation invalidates the entry and its dependent parents;
inconsistent provenance within one read fails without publishing PCM. The cache
is private to one immutable plan and one executable's engine implementations.

Default admission limits are:

- At most 86,400,000 logical input or output frames (30 minutes at 48 kHz)
  per Preserve stage, depth 32, also subject to the combined work limit below.
- At most 65,536 input frames resident in the streaming window; its interleaved
  and planar conversion are reserved together. Non-streamed Hold preparation
  retains its separate 1,048,576-frame input bound.
- Preserve outputs above 1,048,576 frames stream into one private, unlinked
  temporary file. A 2,048-frame write buffer is reserved with the input window.
  Short outputs remain in memory. The threshold is configurable, including zero.
- At most 1 GiB of prepared files across active output reservations, cached
  stages and pinned request-local projections. File descriptors keep readers
  alive; LRU eviction and dropped preparations release their files. No cache
  file is an authored asset or a cross-process admission token.
- 16,777,216 resident stereo-frame equivalents, including simultaneous input
  conversion, output and pinned descendants; at most 64 cached stages with LRU
  eviction. Native FFT state, decoder caches and a bounded sampler halo are
  additional memory.
- 64 newly prepared stages and 134,217,728 total prepared input/output frames
  per read. Cache hits do not restart DSP or consume the preparation quota.
- 1024 distinct source dependencies and 1,048,576 provenance checks per read.
- A shared cooperative deadline, with cancellation checked between bounded
  plan, source and canonical blocks. The host uses ten seconds. Source opening
  retains its separate bounded operation; neither that call nor native
  initialization is preempted at an exact wall-clock instant.

Hard limits can be reduced, not raised beyond these defaults. The memory/file
threshold alone may be configured from zero through 16,777,216 frames, still
subject to the shared residency limit. Failed reads publish
no block; complete child caches may remain useful for a later bounded retry.
`AudioPreparationLimit`, `AudioPreparationTimeout`, `AudioOperationUnsupported`,
`AudioProcessingFailed` and `AudioCacheFailed` distinguish admission, deadline,
policy, DSP and prepared-file I/O failures. Source identity/layout and range errors retain the source inspector's
existing codes. This bounded worker path is not suitable for an audio callback.

An optional `PreparationMonitor` observes the current worker request. Its scoped
activities report output-policy checks, actual produced frames, remaining-input
admission and final cache writes. Nested processing and cold source opens restore
their parent's activity on success or failure. A new playback request attaches
a new monitor without discarding valid PCM. These observations contain no media,
authored state or readiness authority, and do not affect DSP scheduling or cache
admission. The playback controller samples them outside the device callback;
see [preparation progress](qualification/audio-progress-2026-10-11.md).

## Evidence and remaining work

Tests cover actual PCM through seams, fractional input origins, nested history,
mixed pitch-policy order, outer-crop and irregular-seek parity, stable repeats
and overrides, output-only Hold policies, cache interpretation changes, eviction,
shared work limits and cancellation. Plan tests separately cover exact grids,
rounding and indexed billion-play queries. See
[qualification](qualification/audio-stages-2026-09-21.md) for the repository gate
and real process evidence.

Input streaming is covered by the [streamed-input qualification](qualification/streamed-input-2026-10-11.md),
including a two-minute decoded-WAV signal, cold seeks and complete output
comparison under a PCM budget smaller than the input. [Disk-backed output](qualification/disk-pcm-2026-10-11.md)
preserves exact random-access PCM with bounded RAM, disk and complete-stage
work. Ordinary stages and request-local projections use the same storage.
Cache I/O errors are explicit and partial preparations never publish. The
existing PCM file I/O diagnostics include these reads/writes; the controller
also reports retained memory frames and disk bytes.

[Idle preparation](qualification/audio-warming-2026-10-11.md) now examines a
bounded horizon ahead of the stopped native cursor, including future Preserve
stages, on the existing PCM worker. It shares canonical caches, source admission
and request-bound progress with Play. Idle work yields to explicit playback
and does not guarantee that every examined tile remains cached.
[Active playback preparation](qualification/audio-ahead-2026-10-11.md) uses
the same worker with a separately bounded canonical PCM FIFO, allowing future
stages to prepare while the device queue is full. It does not guarantee that
an arbitrarily expensive stage finishes before its audible deadline.
Persistent reuse after restart and the remaining
cache lifecycle stay open. Fractional pitch, variable rates, remaining voice/group effects,
full mastering, listening and the complete device/preview/export corpus remain
open; this qualification does not close DP-09 or DP-16.
