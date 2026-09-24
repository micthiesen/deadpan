# Audio sampling clocks and retained envelopes

The plan separates three quantities: structural sample allocation, exact PCM
lookup, and envelope progress. These values are used by the source and continuous
stage readers. They are derived from existing authored structure; this increment
does not persist resume intent or implement an inserted-time command. Core 14 and
database 20 are unchanged.

`AudioSampleGrid<S>` records the frame origin, positive exact spacing and boundary
rule of the owning grid. Root `AudioSample` allocation uses ties-to-even. Prepared
`SignalSample` storage uses ceil over point positions. At 24 fps an extent of
2/3 frame owns 1333 root samples but contains 1334 preparation points. Grid origin
also matters for a selected stage input. A sample type or count alone cannot
recover these distinctions. The containing span/stage retains revision and full
occurrence identity; equal grid values do not prove interchangeable media.

The grid's search probe resolves the structural interval allocated to a sample.
For root sample n this is n+1/2 with the appropriate ties-to-even side; prepared
signals search at n. Neither probe is a PCM interpolation coordinate.

`AudioSampleMap<S>` retains an output anchor, an exact local coordinate there and
a positive local-frame step per sample. It maps to a Source/Hold's local clock,
or the complete intrinsic output clock of a prepared Preserve stage. It does not
encode allocation, filter support, an envelope, or permission to rebind a stage.
Anchors can lie outside a current crop. Signed differences use checked wide
arithmetic; unrepresentable results fail instead of clamping.

`AudioSpan`, `AudioProcessingSpan` and `AudioSignalSpan` expose the grid and map.
`source_point(sample)` and all source/RoomTone/Preserve sampling recipes consume
the map. Exact project/signal-frame queries continue to use the structural
transform. Ordinary plans derive maps with the previous phase and step. Query
partitioning changes neither map nor allocated extent.

## Resume composition

For a current map q and an insertion of N frames at f:

```text
cut    = B(f)
resume = B(f+N)
q_new(n) = q(cut) + (n-resume) * step
```

`resume(cut, resume)` returns that exact affine map. A second insertion must
compose the current map. At 30000/1001 fps, B(1)=1602, B(2)=3203 and B(3)=4805.
For original-rate 44.1 kHz audio the source step is 147/160. Inserting 1f at f=1
resumes at source position 117747/80. Inserting another 1f at current f=3 must
resume original sample-grid position 3204, not recompute position 3203 from the
old picture frame. The old/new suffix counts may differ; neither operation
rescales the rate to make them equal.

This map operation is a plan value, not an authored edit. Editing a returned
inspection span does not change the immutable plan. Persisted domain identities,
reference placements, compact repeated-play phase, relevance, mark transforms
and atomic command construction remain necessary for actual Hold insertion.

## Envelope progress and audibility

`AudioEnvelope` retains the full meaningful length, an output anchor and signed
progress there. Its fixed, explicit endpoint policy is `silence`: progress
outside `[0,length)` produces exact zero, including with Hard edges. Length is
positive and fits u64, including the full distance between signed sample
endpoints. Progress uses checked i128 and is inspected as an exact decimal
string; unrepresentable progress is rejected. Reanchoring
keeps length and progress independently of current allocation. This root-output
contract advances one envelope sample per output sample; an outer rate change
would require composing its progress in the proper grid.

For the NTSC 2f envelope of length 3203, inserting 1f at f=1 gives resumed
allocation `[3203,4805)`, starting at old progress 1602. Sample 4803 has progress
3202; sample 4804 has exhausted the old envelope and is zero. The previous
range-containment check could not express that extra allocated sample.

Both raw readers and edge-faded reads apply retained endpoint audibility after
sampling. Edge-faded reads additionally apply the existing sample-centered
2ms envelope using retained length/progress. Raw reads keep in-domain levels.
Source/filter support and prepared DSP history are unchanged, and genuine new
seam fades are not synthesized by reanchoring. Endpoint audibility does not
replace explicit silent-Hold suppression.

## Reference policies and remaining inserted-time work

Retain old audibility policies in their reference clock as well as the new
structural policy. A concrete counterexample is an outer Preserve with a silent
Hold at NTSC frames `[2,3)`: prepared point-grid suppression starts at 3204 while
root suppression starts at 3203. After inserting 1f at f=1, new sample 4804 maps
to old prepared sample 3203, but current structural silence starts at 4805.
Without the retained root policy, previously muted audio can become audible.
The opposite rounding phase requires current structural silence to win too.
The [frozen reference policy APIs](AUDIO_REFERENCE.md) now capture the old timing
layout and apply its root silence alongside current suppression. The sampled
values alone do not encode it; authored persistence and command binding remain
open.

Compact Repeat phase must follow stable play identity and retained reference
placement, not live ordinal or a per-play expansion. A later reordered/deleted
play must not rewrite another play's old rounding phase. Full Preserve input
preparation remains separate from output resume. Current caches are scoped to
an immutable plan and complete instance; future cross-context sharing must also
identify intrinsic input maps, grids, support and nested processing, not only
source fingerprints.

The [splice design](STRUCTURAL_SPLICE_DESIGN.md) tracks those remaining authored
requirements. Contract tests and existing real-media parity cover this plumbing;
they do not qualify shifted Hold renders, listening, application playback or
export. This change has no GUI behavior to review.

[Qualification](qualification/audio-sampling-2026-09-23.md) records the full gate,
985 passing tests and independent review of this increment.
