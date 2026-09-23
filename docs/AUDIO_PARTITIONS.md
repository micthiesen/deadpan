# Transparent audio partitions

Core schema 12 adds `RetimePurpose::Partition`. It exposes a selected interval of
its retained child at unity speed while preserving the child's audio processing
and envelope context. This is an implemented building block for structural
splices. The later [Split command](STRUCTURAL_SPLIT.md) adds mark binding and
occurrence handling plus native entry. Arbitrary-boundary Hold insertion still
requires exact resume anchors and host integration.

## Authored intent

An ordinary Retime has `purpose: "edit"`, omitted from canonical JSON. It keeps
its existing crop semantics, including restricted resampling support and new
edit edges. A partition has `purpose: "partition"`; its output duration must
exactly equal its mapping duration, and its audio edge policies must all be
Automatic. Core validation rejects inconsistent timing or explicit edge exceptions
before any transaction is committed. The mapping must remain inside its retained
child. Pitch policy remains present, but a unity partition introduces no pitch or
stretch stage.

Picture uses the existing exact Retime mapping. Audio separates three domains:

- **Allocation:** the actual exposed interval, including every structural crop.
  Absolute project-frame endpoints are rounded once to 48 kHz mix samples.
- **Sampling:** exact source-clock support, carried in `SourceSamplingSupport`.
  It intersects the retained Source host, its audio placement, and meaningful
  authored crop constraints. A partition does not trim this support.
- **Envelope:** the original meaningful edges and their complete sample range.
  Fade width and sample offset derive from this range, which may extend beyond
  the allocation or before project sample zero.

Sequence and Repeat durations derive from their children. They do not trim the
retained context behind a partition. Their authored edge policies still
participate when exactly coincident with a meaningful envelope edge. Ordinary
Retime crops remain real constraints, including crops above partitions.

The root, signal and processing paths carry the same explicit sampling support.
`SequenceAudio` and `StageAudio` consume it through the existing bounded sinc
resampler. Sample selection stays half-open, with no reads outside the admitted
original sample interval. Root allocation uses ties-to-even rounding; signal
point storage uses ceil. Neither changes the authored rate.

Preserve stages retain their complete preparation history. Room tone retains its
intrinsic Hold duration and loop origin. Partitioning their output does not
restart either process. Edge fades apply after time mapping using the retained
envelope, not each fragment's newly shortened allocation.

## Compatibility and persistence

Database schema 19 introduced core 13 and its [logical mark bindings](MARK_FRAGMENTS.md).
Current core 14/database 20 add [Split](STRUCTURAL_SPLIT.md), using this retained
context boundary without changing the old Partition representation.
Schema 18 replays through frozen core 12, retaining Partition purpose and one
binding per old mark. Schemas 16 and 17 replay their complete
core-11 history through the frozen `legacy_v11` adapter. Schema 17's workflow
profile and protected Original baseline are preserved. Earlier databases gain
only the operational tables they did not yet have.

Every Retime predating core 12 remains an ordinary Edit. Those legacy document,
subtree, command and patch adapters reject `purpose`, even if its value is `null` or `"edit"`.
Projection cannot hide a modern partition as an old crop. Default purpose is
omitted, avoiding growth of every old node during bounded JSON migration.

Partition intent follows normal atomic insertion, patch inversion, durable
undo/redo and writer reopen. A partition does not itself allocate a generated
provider, copy marks, or assert that a retained clone has valid logical lineage.

## Remaining splice contract

A pair of partitions can preserve an unmoved render's sound. Moving a suffix by
an inserted Hold, removing its prefix or reordering it additionally needs an
exact source/DSP resume anchor at the new absolute sample boundary. Frame
translation alone does not provide that at
fractional frame rates. Real new cuts also need intentional edge semantics.

The [splice design](STRUCTURAL_SPLICE_DESIGN.md) records these obligations,
including compact Repeat handling and mark ownership across retained copies.
The normative specification remains authoritative; this layer does not close
DP-04, DP-05, DP-06, DP-12 or the complete editorial workflow.
