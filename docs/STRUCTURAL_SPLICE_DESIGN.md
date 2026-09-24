# Structural splice prerequisites

This is an implementation design record. The [transparent audio partition
layer](AUDIO_PARTITIONS.md) was introduced in core 12/database 18. The
[mark binding lifecycle](MARK_FRAGMENTS.md) follows in core 13/database 19.
[Pure Split](STRUCTURAL_SPLIT.md) is implemented in core 14/database 20; the
inserted-time and automatic range planning work below remains open. Sections 4.2, 6.3, 6.5 and 12.2 of the
[specification](spec/DEADPAN_SPEC.md) remain authoritative. Native root-beat
Split/Repeat/delete/Hold-duration commands do not implement arbitrary-boundary Hold
insertion.

Under specification 1.1, these operations reshape the already populated full
Original baseline. Range reuse resolves moments from the project's pinned
Original; it does not add another video source. Splits, cuts and inserted Holds
must retain that profile and its protected undo floor. External sound effects
remain anchored audio events rather than sequential blank-picture inserts.
The general structural representation and legacy projects keep their existing
capabilities. See [the single-Original contract](SINGLE_ORIGINAL.md).

## Separate allocation, sampling and envelopes

The core inserts at a Sequence child index and splits an explicit beat or
occurrence. It has no insert-at-project-boundary command. A source can retain its exact original spans
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

Before core 12, `StageAudio::source_recipe` clipped sinc support to each structural
extent, and edge fades derived width from each allocated span. Pure Split would change
both. A two-sample automatic envelope has gains `[0.5, 0.5]`; dividing it into
two one-sample spans changes them to `[1, 1]`. Marking the seam Hard does not fix
the original envelope width, and Hard can suppress a legitimate coincident
ancestor edge. Transparent partitions therefore need explicit continuity rather
than creative Hard exceptions.

Retain full intrinsic Preserve/RoomTone processing domains beneath crops.
Restarting shortened domains changes DSP history or loop phase. A genuine Hold
insertion can establish new outgoing/incoming fades after mapping while retaining
the underlying source/filter context. Its silence remains explicitly suppressed.

### Existing trim behavior must remain explicit

Review of `9af4a29` found that ordinary authored Retime crops deliberately limit
the source filter's support. The
[`structural_crops_exclude_filter_context_and_use_half_open_discrete_samples` test](../crates/deadpan-audio/tests/sequence.rs)
slows an 8,197-sample source by ten, then crops output samples `[5119,5121)`.
Its recipe admits only source samples `[512,513)`, with origin `5119/10` and
step `1/10`. A full-render slice instead retains support `[0,8197)`. The next
one-sample crop has no discrete selected source sample and returns zero.
These are intentionally different operations. Do not widen every existing
unity crop's support to implement transparent Split.

An explicit transparent partition needs a separate sampling-support descriptor,
derived from the full retained Source host intersected with its audio placement
and all meaningful authored crop constraints. Keep actual root/signal allocation
unchanged. Root, signal and processing spans must carry the same distinction;
both `SequenceAudio` and `StageAudio` must consume it. The existing resampler's
separate selection, origin, output origin and step already support this boundary.
Its zero-extension and bounded read rules still apply at the retained support's
real edges.

Core 12 now implements this distinction using `RetimePurpose::Partition`,
`SourceSamplingSupport`, and separate envelope extent/sample ranges. Current
tests compare paired retained partitions with the original PCM. Later actual
Split tests cover marks, copied occurrences and output parity. Subsequent time
insertion still needs the resume semantics below.

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

[Sampling clocks and retained envelopes](AUDIO_SAMPLING.md) now implement the
separate derived grid/map/progress values and consume them in both audio readers.
This does not persist the retained reference domain or author an insertion. That
contract also records the nested Preserve/silent-Hold rounding counterexample
which requires both old reference-policy and current structural suppression.
[Frozen timing capture and reference queries](AUDIO_REFERENCE.md) now preserve
those old policy facts and stable play placements outside the live tree. The
bounded root-resume PCM consumer proves both rounding phases with canonical DSP.
Persisted live-to-frozen bindings, lifecycle transforms, cross-grid composition
and the atomic insertion command remain to be implemented.

[Sampled-root transfer](AUDIO_SIGNAL_TRANSFER.md) now converts already mapped,
explicitly suppressed root PCM to a new point grid with bounded halo reads.
Old silence and envelope exhaustion affect input taps before interpolation and
are reapplied at exact destination points; creative fades remain separate.
The StageAudio entrypoint shares preparation work, provenance observations and
one deadline across the complete halo. This supplies the conversion operation,
not the authored binding or lifecycle rules that select its retained signal.

The reference plan now provides borrowed physical processing-domain lookup,
separate visible/meaningful extents and unit-rate root resume maps. Lookup stops
at opaque Preserve and remains bounded for compact repeats. Tests distinguish
the active resumed domain from later domains' independent starts and exercise
composed phase through another real DSP stage. Core 15/database 21 now retain
[logical audio copy lineage](AUDIO_LINEAGE.md) through Split, occurrence copies,
reversible patches and strict history migration. Physical aliases stay distinct.
The remaining sample binding must use this relationship with exact clocks and
live context; matching media/timing or lineage alone cannot authorize a retained
signal. See [the reference contract](AUDIO_REFERENCE.md).

[Retained audio contexts](AUDIO_CONTEXT.md) now carry the complete raw processing
tree and its media inputs, beyond the timing-only reference layout. Direct
audio-only compilation preserves picture-only Source absence, exact original
mappings and mix offsets, Hold policies and nested processing. The headless host
reopens a context only after comparing it with the exact retained historical
revision, then qualifies source receipts and original bytes on demand. This
provides the old signal body. It does not yet persist which live occurrence reads
that body, its composed phase anchors or its transformation through later edits.

The [physical-domain reader](AUDIO_PHYSICAL_DOMAINS.md) now supplies hidden
processing context independently of root allocation. A moved Partition can
project its meaningful context into a sibling or before root zero. Whole-root
PCM and policy queries would then select the wrong contribution. Borrowed
domain handles seed both walkers at the physical subtree, preserving the original
signed grid and meaningful constraints. Domain-to-point transfer rebases only
integer sample labels and shares the preparation controller across its halo.

Persisted bindings still need a bounded dependency graph when a newly captured
context contains earlier bindings, and compact birth rules for new Repeat plays
and gaps. Multi-context preparation must share residency/work limits and qualify
cache dependencies by context as well as asset alias. Retained audibility must
remain queryable on both input and output grids of a later Preserve stage;
scaling rounded input silence cannot recover intervals that owned no input point.

[Definition-output reads](AUDIO_DEFINITIONS.md) now supply the separate operand
needed for new Repeat plays. Select the committed default child definition
directly, including when all existing plays are overridden; never choose an old
effective occurrence by convenience. Its scoped local-zero point grid provides
the proposed canonical fresh-play recipe. Surviving plays retain their old-root
continuity. The eventual binding graph must capture earlier descendant bindings
and lexical Repeat arguments as well as these raw recipes.

Keep retained contexts tied to committed before-state revisions. Store one flat,
bounded context table whose binding references form an actual DAG, with explicit
limits on aggregate nodes, rules, run segments, edges, depth and bytes. Live
rules separately own scope, destination clock, structural anchor and exact
operand coordinate. Birth allocation runs select definitions; existing paths
select physical contexts. Removing a silence policy replaces its affected raw
processing contribution, not merely a lineage token. These are binding design
requirements, not implemented authored storage.

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

The single-Original profile permits the old target ID to become a Sequence;
its full-source node requirement belongs to the immutable baseline. Current
edits retain the pinned asset, qualification and basis identity. Exact accepted
generated artifacts already present in the current document may be copied into
fresh retained Hold contexts without changing their ingress trust. An artifact
present only in past history is not admitted that way. Operational generation
requests do not follow copied nodes: a request for Hold T becomes stale if T
remains as a Sequence. Commit still needs complete relevance observations, and
new acceptance beneath Retime ancestors remains unsupported.

Marks require explicit old-occurrence-to-fragment lineage. The current generic
transform cannot move points beyond a shortened leaf into a newly created right
fragment. Preserve bias, owner/coordinate-host distinction, source coordinates,
sequence-pinned coordinates, unresolved marks and occurrence isolation's copying
rules. The insertion and transforms must form one reversible transaction.

### A copied tree is not complete mark lineage

The current Source anchor resolver requires an actual Source occurrence. Turning
an old Source node into a Sequence wrapper preserves an ownership identity, but
does not make that wrapper a valid source-to-project scope.

A more general counterexample is a root-owned Local mark on a Repeat's default
child. Splitting the Repeat after play two leaves appearances of that one authored
mark in both physical subtree copies. Remapping its one host to either child
loses the other half. Duplicating the mark as an ordinary copy changes its
externally owned identity. Conversely, a descendant-owned mark may be anchored
outside the split subtree; blindly copying it produces duplicate external points.

The representation must preserve one logical mark while resolving its physical
fragment scopes, either through consumed logical lineage or explicit fragment
bindings. Its lifecycle must cover later delete, move, copy, occurrence isolation
and undo. Owner loss and coordinate loss remain separate. Unresolved bindings
must never become resolved merely because an identifier acquires a new meaning.
Retaining only the split target's node ID solves target-local marks, not this
descendant case. Rejecting all such marked splits would leave the full operator
contract incomplete.

[Logical mark bindings](MARK_FRAGMENTS.md) now retain bounded physical bindings
under one MarkId. Ownership means authored node lifetime, not visible same-play
correspondence. In `Repeat(Sequence[A,B])`, an A-owned Local mark on B does not
require A to contribute output at B's position. Pair each retained copy's owner
and coordinate mapping independently; references outside the copied subtree stay
outside. Do not impose a new owner-visibility filter or enumerate plays. Hidden
Local bindings stay bound and can become visible through subsequent editing.
This preserves the existing external-host and occurrence-copy semantics tested
in `marks.rs` and `occurrence_edits.rs`.

The binding lifecycle, biased query visibility and Split construction are
implemented. Split retains target-local full-context coordinates and relocates
concrete events once using their exact old target position, independent of outer
visibility. Apply seam bias before deduplicating exact coordinates; rounded frame
equality cannot merge distinct positions. Shifted-fragment resume remains required.

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

Core 13/database 19 freeze the old mark grammar throughout all legacy adapters.
Database 18 replays through frozen core 12, retaining Partition purpose; database
16/17 replay through frozen core 11 and reject purpose. Every old mark gains only
its original binding. Core 14/database 20 freeze the schema-13 multi-binding
grammar and reject Split in old requests. Legacy history cannot acquire
fabricated lineage evidence.
