# Exact historical audio clocks

`FrozenAudioLayout` retains timing and audibility facts independently of current
media recipes. Its optional `derived_clocks` table records each derived Retime's
exact `extent`, `child_start` and `child_end`. `FrozenAudioNode::duration` and its
integral Retime mapping remain the original processing recipe. Capturing a
layout never copies an owned Preserve input projection, media or prepared PCM.

Admission first validates the complete integral recipe graph, then evaluates
the effective graph independently. Sequence offsets and Repeat play/gap extents
are exact ratios. Repeat indexes remain compact, including sparse play overrides
and owned gaps. Derived windows must be positive, contained in the effective
child and arithmetically representable; transparent Partitions retain unity
timing. The effective root must be integral and all extents fit the existing
duration bound. Every clock belongs to an existing Retime. The existing node,
edge, depth, compact-run and JSON byte bounds remain enforced. The flat clock
map is bounded before parsing its values, and unknown or duplicate fields fail.

`node_extent` returns effective allocation; `retime_clock` returns the effective
window and extent for either a derived or ordinary Retime. `duration` returns
the effective integral root. `FrozenAudioProjection::local_duration` retains
the original recipe count, while `local_extent` describes actual allocation.
Projection uses exact ancestor windows, Sequence offsets and stable Repeat
occurrences. It preserves unclamped origins and clips support separately.
Default gaps keep their integral authored duration; owned gaps project through
their own nodes. A nonunity Preserve remains an opaque support boundary.

Equality and processing correspondence include derived timing facts. Placement
comparison uses exact offsets and branch extents. Renaming historical aliases
also renames the clock owners. Integral spacer compaction cannot represent
these clocks: scoped capture requests a complete capture, and slicing retains
the complete bounded layout after validating requested aliases.

`AudioReferencePlan` compiles exact historical FollowSpeed layouts into the same
absolute RoundEven root allocation as the live plan. It uses effective geometry
for policy and processing-domain queries. Historical Preserve/Shift processing,
raw captured audio contexts, independent sound processing and retained binding
states still refuse exact layouts until their owned output scheduling is
integrated. Timing-only projection is not permission to process a changed child
or stretch an intrinsic result to the new allocation.

This is historical geometry support. Core 55/store 83 remain unchanged because
existing project-embedded audio contexts and bindings do not yet admit this new
frozen payload. Standalone bounded layout JSON round-trips it. Owned output
history must still persist and resolve each occurrence's actual sample grids,
chronological Keep/Gap selections, integer resume indices, envelopes and policy.
Affine frame offsets cannot substitute for RoundEven or PointCeil continuation.
Atomic Retime insertion and its metadata, effect and editing lifecycle remain
open. See [owned inputs](OWNED_PRESERVE_INPUTS.md) and the
[splice design](STRUCTURAL_SPLICE_DESIGN.md).
