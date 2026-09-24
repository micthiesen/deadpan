# Reading a retained physical audio domain

`AudioDomain` and `StageAudio::read_domain` render one physical Source, Hold,
Repeat gap or opaque Preserve context independently of its visible allocation.
This supplies the full signal that a future live resume binding must select.
It does not author that binding or an inserted-time command. Core 15/database 21
and retained-context schema 1 remain unchanged.

## Why whole-root reads are insufficient

Consider a one-sample-per-frame sequence containing X for six frames, followed
by a transparent Partition showing A's frames 4 through 8. A is visible at root
samples `[6,10)`, but its full meaningful context projects to `[2,10)`. A whole-root
read at sample 2 returns X. A physical-domain read there must return A's sample 0.
With no preceding X, the same Partition has meaningful context at `[-4,4)`.

Resetting the domain's frame origin is also wrong. At 30000/1001 fps, put a
three-frame X before a Partition showing frames `[2,4)` of a four-frame A.
A's meaningful start projects to frame 1, while its visible start is frame 3.
The original absolute boundary is `B(1)=1602`, which maps to A local 0.4 mix
sample, or `147/400` original samples at 44.1 kHz. Rounding a new local origin
to sample zero loses this phase.

## Borrowed query boundary

`RenderPlan::audio_domain_at` resolves one currently allocated root sample with
bounded indexed traversal. It stops at the first nonunity Preserve. FollowSpeed
and unity retimes remain transparent. The returned handle borrows that exact
plan and retains the physical node, complete stable Repeat path, optional
gap-after identity, root transform, meaningful extent and inherited boundary
constraints. A public span or serialized descriptor cannot create a handle.

`root_samples` and `root_extent` describe full meaningful support;
`visible_samples` describes the allocation through which it was found. The
first can be negative or extend beyond the project root. Both use the original
absolute round-even grid. Source placement and ordinary Edit crops constrain
meaningful support. Partition, Sequence and Repeat allocation do not create
new source-filter cuts. A silent Hold, absent Source audio and a gap outside
Source placement retain their distinct policy meanings.

Both `audio` and `processing` seed the existing walker at the physical subtree
or gap. Neither starts again at the global root. Flattened policy, source support
and inherited edge queries therefore cannot select an overlapping sibling.
Preserve retains its complete intrinsic input and output history. Compact Repeat
indexes and sparse overrides remain bounded; no play expansion is performed.
Invalid probes, out-of-support requests and exhausted query budgets fail.

## PCM and transfer

`StageAudio::read_domain` requires a handle from its own immutable plan, including
pointer identity. It prepares Source, RoomTone and Preserve through the same
qualified providers and caches as root reads. Retained contexts still require
`source_for_context`, including cache dependencies. Domain blocks carry the
project/revision, physical occurrence, meaningful and visible root samples,
signed requested start, stereo PCM and merged explicit suppression.
Bounded reads also accept a physical allocation wider than `i64::MAX` when each
signed endpoint fits. Allocation validation does not subtract or materialize that
whole range; envelope progress remains wide and exact.

The block is before creative fades. Silent Holds and retained-envelope exhaustion
are applied before returning raw PCM. Absent input does not suppress processed
decay. A private signed-output resampler constructor permits the captured hidden
coordinates without changing source support, rate or phase. Public root readers
and the public resampler constructor retain their nonnegative range admission.

`DomainSignalTransfer` binds one borrowed domain to an explicit `SignalSample`
grid. Its exact anchor is expressed in signed captured root samples. Internally
it subtracts the already allocated integer `root_samples.start` to give the shared
`RootSignalTransfer` a zero-based carrier. Callback reads add that integer back.
No frame boundary is rerounded, including half-sample ties and negative starts.
Inspection retains the signed root support and the exact transfer map.
This carrier currently requires the full support length to fit a positive `i64`;
a wider domain remains readable directly but cannot construct this transfer.

`read_domain_transferred` shares one work counter, provenance observation set,
cache/residency allowance and cooperative deadline across all halo callbacks.
Old explicit silence and envelope exhaustion mask input taps before interpolation;
exact output suppression is reapplied afterwards. Demand outside the domain
supplies zero context and explicit suppression, never an adjacent sibling.
The existing 1..256 output bound, 1/64..64 rate range and bounded halo apply.
Cancellation, changed source/layout provenance and unsupported DSP fail without
returning partial output.

## Headless host

```sh
cargo run --locked -p deadpan-cli -- inspect-audio-domain example.deadpan --at 0 --samples -1600 -1344
```

The probe selects the current physical domain. The signed sample range selects
its captured context and must contain 1..256 samples. Protocol 1 returns an
`audio` object labeled `physical_domain_pcm_before_effects`. This is raw context
inspection; it does not describe a preview mix or export. The same command is
available through `deadpan-app --headless`.

`ProjectAudioSession::read_domain` also works after `open_context` authenticates
a complete retained context against its historical revision. Receipts and
verified original bytes remain required. Inspection never changes the history
cursor or reinterprets a latest-head asset alias.

## Remaining binding work

The rendering boundary is implemented; authored selection of that boundary is
still required. Bindings need independent later-domain anchors, composed phase,
compact Repeat birth/edit correspondence and live ownership transforms. A
frozen-body implementation would also require a bounded dependency graph and
preparation shared across contexts. The preferred [owned recipe](OWNED_AUDIO_CLOCKS.md)
approach evaluates Split's current children in explicit clocks within one plan,
avoiding a duplicate raw-body graph. Its authored lifecycle is still unimplemented.

[Definition-output reads](AUDIO_DEFINITIONS.md) now select a Node or actual
Repeat default directly for the separate future birth operand. This avoids
inventing an old occurrence when a Repeat gains new plays. Preserving intrinsic
bindings while rebasing enclosing lexical Repeat placement remains required.

Retained policies also need exact queries on both input and output grids of a
new consuming Preserve stage. Scaling rounded input suppression ranges is
insufficient: a silent interval with no input-grid point can acquire output
samples after slowing. Changing raw audio or a silence policy must replace the
affected processing contributions rather than preserve an obsolete mute.
These remain prerequisites for atomic Hold insertion, genuine seam envelopes
and application playback. See [the splice design](STRUCTURAL_SPLICE_DESIGN.md).
