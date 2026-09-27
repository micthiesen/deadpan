# Structural splice prerequisites

This is an implementation design record. The [transparent audio partition
layer](AUDIO_PARTITIONS.md) was introduced in core 12/database 18. The
[mark binding lifecycle](MARK_FRAGMENTS.md) follows in core 13/database 19.
[Pure Split](STRUCTURAL_SPLIT.md) was introduced in core 14/database 20.
[Insert Time](INSERT_TIME.md) adds atomic root Source/Hold insertion in core
17/database 23. Arbitrary nested insertion and automatic range planning below
remain open. Sections 4.2, 6.3, 6.5 and 12.2 of the
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

## Exact boundary descent

`AnchorIndex::locate_boundary` now resolves a revision-bound project boundary
down through the authored tree. The [headless `locate-boundary` query](HEADLESS.md)
exposes the same result. This is a read-only prerequisite for nested insertion,
not itself an insertion command or a choice of insertion parent. Its original
qualification used core schema 25/database schema 31.

Each scope retains its `InstancePath`, exact owner-local boundary, full owner
duration, and entry edge. Sequence entries retain the authored child index,
including preceding zero-duration children; empty children are never selected
as content. Repeat entries distinguish plays from explicitly owned gap branches
and retain stable iteration IDs. An implicit gap terminates at its Repeat owner
with the preceding play ID and a separate gap-local coordinate. A configured
final gap contributes no rendered interval.

The query uses boundary coordinates, never sampled picture centers. Retime
descent evaluates `mapping.start + position * mapping.duration / duration`
without rounding. All grouping and transparent-partition scopes remain visible,
including full retained clocks behind crops. Each emitted scope can be resolved
back to the same exact project boundary through the existing occurrence anchor
API. Bias chooses the preceding or following content at a seam. Outward bias at
project start/end reports an explicit edge with only the root scope; inward bias
descends. An empty project returns start for Left and end for Right.

Construction indexes authored structure, not expanded plays. Queries share a
scope budget and a prefix-comparison budget across all Sequence and Repeat
levels, checking each budget before its next charged operation. The defaults
allow 257 scopes and 8,192 binary-prefix comparisons. These bounds exclude index
construction, document validation and map lookups. Persistent mark transforms
reuse the same positive-duration Sequence prefix index while retaining their
existing edge and content-relocation semantics.

### Remaining mutation ownership

The future atomic splice must isolate selected Repeat ancestors outside-in
before changing their child structure. A Hold inserted in a selected play must
remain under its enclosing Repeat and group effects. Captured picture context
includes only owners below the chosen insertion parent; parent and ancestor
framing stays live and applies once. Moving root-level L/Hold/R fragments around
a Repeat would lose that ownership.

An affine Retime can map an integer project cut to a fractional child boundary.
Never round that child cut or lengthen its child under the unchanged mapping.
Output partitions can retain speech timing, but do not by themselves keep every
inner effect owner live over the Hold. The splice author must preserve the
required ownership explicitly; capturing a live group as a static picture is
not a substitute for that requirement. The locator reports the complete path
so this decision cannot be hidden by leaf-only resolution.

One bounded case can retain the selected Retime's ID as a Sequence owner with
its live framing: place full old Retime contexts beneath left/right output
partitions and a Hold between them, removing the lifted framing from both
contexts. The owner envelope then spans the new duration, as ordinary duration
edits require. Owner-local marks need biased insertion transforms; descendant
marks need retained fragments. Preserve output bindings belong on the retained
Retime contexts, not the new Sequence, and node edge policies need deliberate
ownership transfer. Existing commands must keep their historical output.

This does not represent the general case. An outer Retime mapping three child
frames into two project frames maps a one-frame cut and a one-frame insertion
to `3/2` child frames. Integer partitions cannot express that without changing
speech timing. Also, a framed Sequence below the selected Retime would remain
only in the retained contexts, so its effect would not stay live over the Hold.
The complete implementation needs exact retained-output selections and live
owner clocks spanning inserted time. These may extend the existing primitives;
static captured framing and integer partitions alone are insufficient.

Before copying or splitting, retain unbound audio sampling lattices. After
zero-time isolation/splitting, capture current placements and append root-window
resume steps only to owners with affected allocations. Each occurrence derives
its own absolute rounded sample boundary. Stop at Preserve output owners to
retain complete input history. A global no-op reanchor on every owner would
consume the bounded history of unrelated earlier material. Implicit-gap
materialization, mark-identity allocation, scoped suffix planning and one atomic
history patch remain part of the required splice implementation.

### Derived owner clocks

The current preferred representation keeps authored `FrameDuration` counts
integral and introduces separate exact effective extents and owned clock maps
for splice-produced structure. This is a design decision, not a persisted
capability. Source, Hold and authored Retime counts, the insertion boundary and
requested duration, and the final project extent retain their integer contracts.
Every internal effective clock must compose without rounding. A transparent
adapter must not introduce a DSP stage, sampling-support boundary or fade.

For a Retime mapping three child frames to two output frames, inserting one
project frame at frame one requires a child cut and added span of `3/2`.
The child group grows from `3` to `9/2`, with retained selections around an
authored one-frame Hold adapted to an effective `3/2` span. The enclosing Retime
then maps `0..9/2` to three output frames, preserving its old `3/2` slope.
The group remains an actual ancestor of the Hold, so its framing stays live.

Exact Sequence and Repeat totals alone are insufficient. With an outer Retime
mapping `0..3` to two frames and an inner Retime mapping `0..4` to three frames,
a one-frame project insertion adds `3/2` to the inner Retime's effective output
and `2` to its child's extent. The inner authored count cannot represent that
effective output. A complete representation needs exact effective Retime clocks
as well as exact retained windows, compact Repeat prefixes, mark fragments and
plan coordinates. Preserve the existing integer reducers and introduce the new
structure through a closed command/schema boundary.

Later explicit Retime edits should retime the inserted Hold with its group,
following ordinary tree ownership. Its insertion-time project duration is not
an implicit permanent project-clock pin. Duration setters must resolve their
declared clock, recompute effective extents and validate the integral root;
overflow or an unrepresentable result must fail atomically. This rule still
needs implementation and normative integration with the final clock contract.

Multiplying every integer duration by a common denominator is not a safe
shortcut. A one-second Source under Preserve mapping `30→20` (thirty input
frames to twenty output frames) prepares 48,000 input samples at 30 fps.
Doubling the child duration and using a `60→20` mapping makes the current engine
prepare 96,000 samples and changes the Source resampling pitch before Preserve.
The picture mappings cancel, but the PCM does not. Explicit coordinate units
could avoid this only by retaining identical physical input/output grids, exact
DSP rates, complete processing history, support and phase. Those conversions
would still need every clock consumer and frozen binding representation audited.

Keep Preserve as an opaque retained processing context. A pause adds output
allocation and a resume map, not new samples in that context's input history.
Apply silence on its output grid even when the inserted interval owns no input
sample. Existing no-input-point suppression and distinct root RoundEven versus
Preserve PointCeil tests are prerequisites, not proof of a complete splice.

`Framing::evaluate_exact` now supplies the numerical evaluation needed by a
derived owner extent. It compares segment progress and interpolates through
bounded wide integers without materializing an overflowing rational quotient or
endpoint. The existing integer evaluator delegates to it. No document fields,
clock adapters, duration maps or command admission change in this increment;
the structural and audio work above remains open.

### Preserve input projection (design)

Retaining a timing layout is not enough to retain a processing operand.
`AudioBindingState` stores old topology/clocks and resume terms, but the current
`AudioSignal` walker still feeds the current child into a nonunity Preserve
stage. Inserting a Hold inside that child would therefore change its input and
history. `FrozenAudioContext` is a detached revision snapshot, not a live body;
audio lineage is not a pointer to equivalent current PCM. Neither supplies this
missing projection.

The next candidate is an explicit owned `PreserveSpliceRoute`, with two exact
maps. An input map projects the full intrinsic input range onto current owned
descendant paths and local ranges, excluding the splice-born Hold. An output map
places intrinsic Preserve output ranges around the inserted policy interval.
The captured timing layout supplies coordinates, not old raw recipes. Every
plan resolves the current Source/Hold/Retime recipe at the retained target, so
subsequent raw edits remain observable.

For a stage mapping `G[0,3)` to output `[0,2)`, an output pause at one divides the
intrinsic input at `3/2`. G stays the actual framed ancestor of the left slice,
adapted Hold and right slice. The input map joins the two current source slices
into the original three-frame processing domain. The output map selects old
output `[0,1)`, the new interval, then old output `[1,2)`. It does not prepare a
new four-and-a-half-frame input containing the pause.

For nested Preserve, an enclosing stage must request the inner stage's intrinsic
output, before that inner stage's splice allocation. Compose each stage's exact
cut and insertion span through its own rate; do not reuse the root frame number
as an inner boundary. Apply the inserted silence on every relevant output grid,
including the outermost output, so processing tails cannot fill the pause. Root
RoundEven allocation and intrinsic PointCeil sampling remain distinct.

This route is a proposal, not current binding vocabulary. Its targets must stay
inside the stage's owned subtree and be validated under bounded work; it must
not introduce cycles or implicit links between ordinary copies. Split, isolation,
deletion, trimming and movement need atomic route transforms or explicit refusal.
Cache identity must include the resolved projection and all current recipe
dependencies. Complete retained input support, envelope ownership, repeated
occurrences, non-silent inserted policies and later stage edits still require
explicit rules and actual PCM qualification. The borrowed consumer below
qualifies a preparation prerequisite for that authoring boundary.

The borrowed [audio input tape](AUDIO_INPUT_TAPES.md) now supplies the current-tree
input view and an actual PCM reader through `StageAudio`. Exact runs share one
PointCeil grid and preserve full sampling support across allocation seams.
`AudioStageProjection` now supplies checked full intrinsic operands and an
independent exact output-policy view. Nested inputs refer to child intrinsic
output; a separate PointCeil tape places pauses around it. Request-local identity
memoization preserves canonical history across chunked reads. This does not
admit an authored route or change ordinary root-plan evaluation.
[Projected root placement](AUDIO_PROJECTED_ROOT.md) now supplies separate
absolute RoundEven allocation for one physical projection, preserving its
exact PCM phase and root-policy clock through crops and repeated resumes.
Persistent lifecycle, fractional effective clocks, aggregate output scheduling
and authored compiler integration remain necessary for the nested splice.

## Separate allocation, sampling and envelopes

Generic Insert uses a Sequence child index; Split targets an explicit beat or
occurrence. InsertTime has a Source/Hold path beneath ordinary Sequence groups, while
general project-boundary splice remains open. A source can retain its exact original spans
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

## Repeat gap operands

`AudioDefinitionSelector::RepeatGap` now exposes the current configured gap
recipe in its local-zero PointCeil clock or an explicit root/point placement.
It remains available with one play and invents no preceding-play identity.
`FrozenAudioLayout::project_scoped_with_support` resolves an actual gap's retained
clock and meaningful support from its stable preceding play. These separate
operands serve new gaps and surviving gaps, respectively.
[Authored gap ownership and compact resume dispatch](GAP_AUDIO_BINDINGS.md)
now consume both. General atomic splice remains open. See
[gap clocks](AUDIO_DEFINITIONS.md) and
[the current Original-moment contract](SOURCE_MOMENTS.md).

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
The owned binding representation described below now persists timing intent.
Its complete lifecycle, cross-grid consumption and the atomic insertion command
remain to be implemented.

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

The original frozen-body proposal would require a bounded dependency graph when
a newly captured context contains earlier bindings. The preferred owned-tree
approach below avoids duplicating those raw bodies. It still needs compact birth
rules for new Repeat plays and gaps. Preparation must share residency/work limits
through nested evaluations. Audibility must remain queryable on both input and
output grids of a later Preserve stage; scaling rounded input silence cannot
recover intervals that owned no input point.

[Definition-output reads](AUDIO_DEFINITIONS.md) now supply the separate operand
needed for new Repeat plays. Select the committed default child definition
directly, including when all existing plays are overridden; never choose an old
effective occurrence by convenience. Its scoped local-zero point grid provides
the proposed canonical fresh-play recipe. Surviving plays retain their old-root
continuity. The eventual binding graph must capture earlier descendant bindings
and lexical Repeat arguments as well as these raw recipes.

The [owned recipe reader](OWNED_AUDIO_CLOCKS.md) now evaluates a physical
definition from the selected current revision in an explicit signed root clock.
It uses the complete children already owned by Split, and their current policies,
instead of substituting an old raw body. This makes editing the retained recipe
observable without a second historical recipe graph. Explicit historical reads
still authenticate their complete capture against the committed revision.

Core 16/database 22 introduced the [owned binding representation](OWNED_AUDIO_BINDINGS.md):
bounded timing-only records, lexical arguments, ordered default-birth clauses
and symbolic local phase terms. Each term retains its own clock, so rounding at
a shared-definition cut can differ across existing plays without expanding them.
Split/isolation remap live aliases and removal prunes unused records atomically.
Normal plans and StageAudio now consume these bindings. Pure capture retains
existing clocks and compact default/override scope. Core 22/database 28 also
capture configured Repeat gaps, including unplayed gaps. The consumer preserves current raw
recipes, resumed phase, crop support, current Hard policies and virtual
post-mapping fades. Endpoint masks stay on the physical sampling grid; explicit
SilentHold policy also applies after a downstream Preserve, including intervals
with no input point. Query/preparation work, source provenance and relative cache
depth remain shared. Changed raw contributions and affected opaque ancestors
still need explicit lifecycle rules; the arbitrary insertion command remains open.

## Proposed compact resume dispatch

The retained allocation query and ordered steps below are now implemented in
[core 21/database 27](AUDIO_REANCHORS.md), with
[gap binding ownership](GAP_AUDIO_BINDINGS.md) added in core 22/database 28.
The general splice author remains open.

Core 24/database 30 extend `InsertTime` at existing root
Sequence seams to shift composite suffixes. The author reuses capture's current placement
templates without replacing an owner's retained lattice or accumulated phase.
Append one chronological step for each shifted physical or default-gap owner,
using the pre-edit root window. Allocation-entry dispatch chooses each concrete
occurrence's entry, including a partially visible first play and later full
plays. Movement traversal stops at the first nonunity Preserve output; moving
its intrinsic preparation descendants again would alter its history.

This changes contextual command admission. Legacy replay retains core 23's
`InsertTime` suffix restrictions against the pre-edit document through database
29. Closed JSON fields and exact
patch comparison alone cannot reject a consistently forged old history that
claims a formerly unsupported composite suffix. Preserve the old reducer's
output for inputs it admitted and add a forged-history refusal fixture beside
genuine old-binary replay.

Interior Repeat/Retime boundaries, fractional Retime cuts, outside-in occurrence
isolation, materialized-gap insertion and admitted Original-moment payloads
remain subsequent parts of the full atomic splice contract.

Core 25/database 31 add a root Source/ordinary Hold fragment interior followed
by a composite suffix. Both existing reducer paths remain unchanged. Capture
unbound lattices before the internal Split, let Split copy live aliases while
retaining historical references, then capture fresh current placements after
Split and before the Hold. The right copy's physical and Repeat aliases belong
to that second graph. A single timing identity cannot name both graphs: replacing
the first table would invalidate retained lattices. The new branch reserves two
consecutive checked timing ordinals without changing old requests, rejecting
collision or overflow before publishing a transaction. Do not install an unused
phase-only table before Split's intermediate validation.

Core 26/database 32 add actual insertion beneath pure Sequence ancestors. A
strict interior descends to the next Sequence; an existing child seam stays in
its owner. The typed preflight returns that owner, slot and required Split IDs.
Split only the admitted physical leaf or retained fragment, insert into its
actual parent, then reanchor suffix siblings from that parent through the root.
This preserves group identity and live framing, including on the inserted Hold.
The native capture omits the insertion parent and all ancestors. Core 25 replay
retains its former root-only context admission. The effective rational clock
proposal above is still needed for Retime interiors; no sparse clock map or
persisted projected DSP routes are introduced by this increment.

Frozen core 24 retains contextual admission as the old physical suffix or an existing root
seam; the stricter core-23 restriction still applies to database 29 and earlier.
The decoded-PCM tests include an NTSC Source cut at frame 1 resuming
new sample 3203 from old 1602 while the following Repeat resumes new 4805 from
old 3203. One common suffix sample offset cannot satisfy both. Keep cropped
Preserve history, gap births, mark bias and one-step undo in the same transaction.

One local resume anchor per physical owner cannot represent a cut Repeat. If a
two-frame Source repeats and insertion splits its first play at local frame one,
the first surviving fragment enters at one while later full plays enter at zero.
A single suffix sample offset also fails: each later physical domain starts on
its own absolute rounded boundary.

Prefer a bounded allocation-entry query against each retained timing layout over
explicit clauses enumerating play intervals. The query should project the
physical Source, Hold, Preserve output or actual gap, then intersect its visible
allocation with every ancestor selection, including transparent Partitions and
the selected clock's input constraint. An optional insertion window belongs to
that captured clock scope. Keep this allocation query separate from meaningful
raw support, which deliberately retains hidden Partition context.

Return an explicit absent entry when intersections are empty. A right Partition
retains earlier hidden plays; clamping an empty intersection to the old endpoint
would install a spurious resume that could become audible after later expansion.
An absent entry must leave the previous sampling map intact.

Resolve lexical birth scope before applying the enclosing window. A new play
must discard an old occurrence's outer cut while retaining intrinsic edits in
its definition. For example, a born play of `Repeat(Partition(A,[1,3)))` enters A
at one, not zero. A cut inside an old occurrence may enter at two, but that outer
cut must not become the new play's entry. Each retained step needs its own scope;
nested births cannot use one global reset. A former final play gaining a gap
requires gap-definition birth even though its play identity survived, and an
overridden child does not eliminate that play's ordinary gap.

A bounded persisted representation retains the existing resume as its initial
state and appends bounded, chronological reanchor steps. Each step contains a
placement template and an optional window in that template's captured scope.
Resolve its allocation entry `e`; skip absent entries. For the current local
anchor `a` and accumulated exact local phase `p`, apply:

```text
p = p + (B_step(e) - B_step(a)) * step.local_frames_per_sample
a = e
```

The initial state retains the existing resolved phase and anchor, or zero phase
at the lattice's meaningful start when no resume exists. A timing snapshot alone
cannot reconstruct phase accumulated by previous edits. Old symbolic phase
terms keep their original interpretation. Consumers can continue receiving a
resolved scalar anchor and phase after this bounded evaluation.

Root movement stops at the first opaque nonunity Preserve output; it must not
also shift that stage's intrinsic preparation descendants. Split/isolation,
argument closing, pruning, identity reservation and wire admission must visit
every new step reference. Live aliases may change; retained historical aliases
and stable play identities must not. The persistent representation uses frozen
core-16-through-20 adapters and complete history replay into database 27.

Required evidence includes clipped first versus full later plays, hidden plays
subsequently exposed, born plays retaining intrinsic Partitions, nested births
with prior resumes, former-final gap birth, repeated NTSC insertions, movement
and bounded billion-play lookup. Actual PCM tests must establish the recurrence
before the general splice command uses it.

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

[Sparse gap branches](REPEAT_GAP_BRANCHES.md) now provide independently owned
subtrees and exact default-gap materialization without changing other shared
gaps. General splice planning into those branches remains open. Resolve Freeze fallback from the immutable measured picture plan before the
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
