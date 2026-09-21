# Plan-driven source PCM

`SequenceAudio` reads source-stage stereo PCM from one immutable `RenderPlan`.
The shared headless host exposes it through `inspect-audio`. This connects
structural timing to actual qualified media; it is before edge fades, effects,
the master limiter and monitoring gain. The result explicitly identifies its
stage as `source_pcm_before_effects`, with project and revision IDs. It is not a
final mix, export file, or native playback implementation.

## Exact mapping

The reader requests bounded audio spans from the plan and stages 1 through 256
samples atomically. Silence, Source placement, Sequence, Repeat and sparse
occurrence mappings retain their existing plan semantics. A gap occurs only
between repeat plays. A silent Hold contributes exactly its allocated samples;
subsequent speech continues at its original source coordinates.

Every source recipe is anchored at the span's full `allocated_samples.start`,
not the current query start. The source origin and per-output-sample step come
from exact original source coordinates converted with the actual decoded sample
rate. Source timestamps are not assumed to already be 48 kHz samples. NTSC
placements can therefore give successive plays different fractional source
phases without changing total duration or restarting phase at read boundaries.

`AudioSpan::source_point_at_project_frame` exposes exact source coordinates at
the span's structurally clipped `project_extent` edges. The allowed source PCM
selection is the half-open discrete interval `[ceil(start), ceil(end))`,
intersected with the authored original trim. The filter never reads context
outside that selection. An exact crop containing no original sample positions
contributes zero-extended samples, rather than borrowing neighboring speech.
Original trim endpoints must lie on original sample boundaries. Query bounds,
arithmetic overflow, decoder coverage and sampler limits remain explicit errors.

Source-local audio placement must retain its natural rate. Enclosing Retime
stages with `FollowSpeed` compose exactly and use the qualified tape-speed
sampler. Nonunity `Preserve`, room tone, and effect tails are rejected before
any source in that query is opened. A Source-local rate change currently has no
authored pitch policy, so the reader rejects it instead of inventing one.
Unity Preserve and time-mapped silence need no omitted DSP stage and are valid.
Supporting the rejected operations remains required; the inspection API does
not redefine them as silence or ordinary speech.

## Historical media binding

`AudioSourceProvider` receives the exact project, revision and asset requested.
`deadpan_cli::audio::ProjectAudioSession` implements the production host boundary:

1. Open the project read-only and retain its immutable document and plan.
2. Resolve the asset through `registered_source` at that revision, including the
   complete receipt-derived asset metadata check. Confirm the retained document's
   receipt ID and content hash as well.
3. Snapshot the retained original, checking object identity, SHA-256 and byte
   length against that receipt. Open the selected original audio stream.
4. Compare the entire reopened audio index before preparing any PCM, including
   raw observations and derived coverage. Unknown speaker layouts remain errors.

Undo, later edits and reuse of an asset alias cannot change an existing reader's
meaning. This is tested with a cold reader, so a warm cache cannot conceal an
incorrect latest-revision lookup. The headless reader may coexist with a writer;
it does not change authored snapshots, history, receipts or inventory.

The host retains one private decoded PCM session and releases it before opening
a different source. Existing limits admit at most 64 GiB of original input and
1 GiB of private PCM; original snapshotting and audio opening each retain their
separate 300-second deadline. Reads use a ten-second media-read deadline plus
bounded, cancellable filtering. Alternating sources currently reopen decoders.
These limits establish bounded preparation, not acceptable app playback latency
or a completed prepared-cache lifecycle. All I/O and filtering stay off UI and
device callbacks.

## Inspection and verification

```sh
cargo run --locked -p deadpan-cli -- inspect-audio /tmp/example.deadpan --samples 0 256
cargo run --locked -p deadpan-app -- --headless inspect-audio /tmp/example.deadpan --samples 0 256
```

Protocol 1 returns the source-stage block under `audio`. Invalid/empty ranges
produce `AudioRangeOutOfRange`; unsupported processing produces
`AudioOperationUnsupported`; unknown layouts produce `AudioLayoutUnsupported`.
Missing qualification or original bytes produce `SourceAudioUnavailable`.
Failures emit no successful partial block. See [headless commands](HEADLESS.md)
and the [qualification record](qualification/sequence-audio-2026-09-21.md).

The remaining voice graph must consume the same immutable plan and retain its
distinctions between absent audio, silent Holds and permitted tails. It must
integrate pitch-preserving stages, room tone, fades, authored treatments and the
oversampled master limiter before this becomes user-audible playback or export.
