# Structural splice prerequisites

This is an implementation design record, not implemented behavior or a change to
the normative specification. Sections 4.2, 6.3, 6.5 and 12.2 of the
[specification](spec/DEADPAN_SPEC.md) remain authoritative. Native root-beat
Repeat/delete/Hold-duration commands do not implement arbitrary-boundary Hold
insertion or pure Split.

Under specification 1.1, these operations reshape the already populated full
Original baseline. Range reuse resolves moments from the project's pinned
Original; it does not add another video source. Splits, cuts and inserted Holds
must retain that profile and its protected undo floor. External sound effects
remain anchored audio events rather than sequential blank-picture inserts.
The general structural representation and legacy projects keep their existing
capabilities. See [the single-Original contract](SINGLE_ORIGINAL.md).

## Separate allocation, sampling and envelopes

The current core inserts at a Sequence child index. It has no semantic split or
insert-at-project-boundary command. A source can retain its exact original spans
when divided by materializing FitBeat against its original duration and shifting
the right fragment's Placement start by the negative cut position. This retains
picture coordinates but is insufficient for complete audio semantics.

Three distinct domains are needed:

- Structural allocation owns samples `[B(a), B(b))` at absolute project-frame
  boundaries, with origin-based ties-to-even rounding.
- Sampling retains the exact source/filter support, phase and intrinsic DSP
  processing domain. Its authored rate cannot be inferred from rounded counts.
- The envelope retains its original meaningful edges, width and sample offset.
  A transparent structural partition must not create a new fade.

Today `StageAudio::source_recipe` clips sinc support to each structural extent,
and edge fades derive width from each allocated span. Pure Split would change
both. A two-sample automatic envelope has gains `[0.5, 0.5]`; dividing it into
two one-sample spans changes them to `[1, 1]`. Marking the seam Hard does not fix
the original envelope width, and Hard can suppress a legitimate coincident
ancestor edge. Transparent partitions therefore need explicit continuity rather
than creative Hard exceptions.

Retain full intrinsic Preserve/RoomTone processing domains beneath crops.
Restarting shortened domains changes DSP history or loop phase. A genuine Hold
insertion can establish new outgoing/incoming fades after mapping while retaining
the underlying source/filter context. Its silence remains explicitly suppressed.

## Exact resume at fractional frame rates

For an old sampling function `q_old(n) = q0 + (n - s0) * r`, preserve the exact
authored step `r` and establish a resume anchor:

```text
s_cut    = B(f)
s_resume = B(f + N)
q_resume = q_old(s_cut)
q_new(n) = q_resume + (n - s_resume) * r
```

This makes `q_new(B(f + N)) == q_old(B(f))` explicit. Merely shifting picture
Placement does not: at 30000/1001 fps there are 1601.6 samples/frame, so inserting
one frame at frame 1 shifts allocation from sample 1602 to 3203. Subtracting one
exact frame gives source position 1601.4 instead of 1602.

Persist the exact domain coordinate and structural boundary anchor, resolving
its current absolute sample position at plan compilation. Repeated occurrences
need compact affine mappings, not a record per rendered play. Existing genuine
cuts can retain separate domain start anchors; transparent partitions share a
domain and must not introduce a reanchoring point.

The specification's absolute allocation rule does not permit a stronger promise
that every previous PCM sample survives movement unchanged. Let `T` be the old
end; inserted silence owns `B(f+N)-B(f)` samples, the old suffix owns
`B(T)-B(f)`, and the new suffix owns `B(T+N)-B(f+N)`:

| Samples/frame | f, N, T | Silence | Old suffix | New suffix |
| --- | --- | --- | --- | --- |
| 8008/5 | 0, 1, 1 | 1602 | 1602 | 1601 |
| 8008/5 | 1, 1, 2 | 1601 | 1601 | 1602 |
| 1001/2 | 1, 2, 3 | 1002 | 1002 | 1000 |

Counts are not invariant under rounded translation. Preserve the original
selection, resume coordinate and rate, while each new interval still owns its
absolute allocation. Do not squeeze all old samples into the new count by changing
speed. Qualify the behavior at shortened/extended endpoints explicitly. These
mathematical constraints are not proof that the proposed representation meets
the complete media contract.

## Structural and mark requirements

Resolve boundary coordinates through `RepeatLayout`, using the right-hand object
except at document end. Do not use picture-center sampling to choose the cut.
Isolate repeated ancestors from outside inward with bounded caller identities,
preserving compact plays and existing override branches.

Fractional local cuts beneath Retime cannot be rounded into integer child frame
durations. An alternative worth implementing and verifying is a pair of unity
Retime crops around retained full inner domains, partitioned at the integer
project-facing boundary. Owned subtree copies must remain bounded; authored nodes
cannot gain two parents. This also avoids pretending generated Holds can sample
arbitrary suffixes using their current retained-prefix representation.

Marks require explicit old-occurrence-to-fragment lineage. The current generic
transform cannot move points beyond a shortened leaf into a newly created right
fragment. Preserve bias, owner/coordinate-host distinction, source coordinates,
sequence-pinned coordinates, unresolved marks and occurrence isolation's copying
rules. The insertion and transforms must form one reversible transaction.

Individual Repeat gaps need a representation that does not modify every shared
gap. Resolve Freeze fallback from the immutable measured picture plan before the
edit, including project start/end. Audio-only content uses Background. Still
picture fallback needs an explicit supported representation. Zero insertion
duration must be a semantic no-op without a new authored revision.

## Acceptance work

Before claiming Split or arbitrary Hold insertion, test actual PCM and picture
identity across fractional rates, 44.1 kHz resampling, short envelopes, source
offsets, VFR endpoints, nested Preserve/FollowSpeed, RoomTone and generated crops.
Assert exact resume anchors, original sampling steps, absolute later boundaries,
silence suppression, irregular-seek parity, compact billion-play queries, all
mark spaces/biases and atomic identity exhaustion. Durable undo/redo must retain
domain/anchor/envelope metadata as well as picture structure.
