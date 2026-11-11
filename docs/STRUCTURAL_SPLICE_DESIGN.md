# Structural splice prerequisites

This is an implementation design record. Original-audio sampling bindings
(retained lattices, resume anchors and reanchor steps) were removed on
2026-11-11; passages below that mention them are history. Original audio
placement is now derived from the current tree (see
[audio sampling](AUDIO_SAMPLING.md)). The [transparent audio partition
layer](AUDIO_PARTITIONS.md) was introduced in core 12/database 18. The
[mark binding lifecycle](MARK_FRAGMENTS.md) follows in core 13/database 19.
[Pure Split](STRUCTURAL_SPLIT.md) was introduced in core 14/database 20.
[Insert Time](INSERT_TIME.md) adds atomic root Source/Hold insertion in core
17/database 23. Arbitrary nested insertion and automatic range planning below
remain open. Native root-beat
Split/Repeat/delete/Hold-duration commands do not implement arbitrary-boundary Hold
insertion.

These operations reshape the already populated full
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
marks need retained fragments. Node edge policies need deliberate
ownership transfer. Existing commands must keep their historical output.

This does not represent the general case. An outer Retime mapping three child
frames into two project frames maps a one-frame cut and a one-frame insertion
to `3/2` child frames. Integer partitions cannot express that without changing
speech timing. Also, a framed Sequence below the selected Retime would remain
only in the retained contexts, so its effect would not stay live over the Hold.
The complete implementation needs exact retained-output selections and live
owner clocks spanning inserted time. These may extend the existing primitives;
static captured framing and integer partitions alone are insufficient.

Audio placement needs no capture at the edit: every Source, Hold and processing
owner is sampled from its current position, so an insertion changes structure
only. Each occurrence derives its own absolute rounded sample boundary, and a
Preserve output owner keeps its complete input history. Implicit-gap
materialization, mark-identity allocation, scoped suffix planning and one atomic
history patch remain part of the required splice implementation.

### Derived owner clocks

The representation keeps authored `FrameDuration` counts integral and introduces
separate exact effective extents and owned clock maps for splice-produced
structure. Core 54 introduced a bounded `derived_clocks` table for FollowSpeed
Retime allocations and exact child windows; production plans consume it.
Core 55 also persists the owned Preserve input described below. Authored splice
construction and retained output routes remain open.
Historical timing layouts now retain these exact clocks independently of their
integral recipe counts; see [historical audio clocks](FROZEN_AUDIO_CLOCKS.md) (used by sound clocks).
This geometry does not yet establish owned output sample continuation.
Source, Hold and authored Retime counts, the insertion boundary and
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
would still need every clock consumer audited.

Keep Preserve as an opaque retained processing context. A pause adds output
allocation, not new samples in that context's input history.
Apply silence on its output grid even when the inserted interval owns no input
sample. Existing no-input-point suppression and distinct root RoundEven versus
Preserve PointCeil tests are prerequisites, not proof of a complete splice.

`Framing::evaluate_exact` now supplies the numerical evaluation needed by a
derived owner extent. It compares segment progress and interpolates through
bounded wide integers without materializing an overflowing rational quotient or
endpoint. The existing integer evaluator delegates to it. No document fields,
clock adapters, duration maps or command admission change in this increment;
the structural and audio work above remains open.

The exact Repeat index supplies
compact rational prefixes, play/gap boundary lookup and stable-identity lookup
for those effective clocks. It shares the existing Repeat compiler and lookup
algorithm, with separate closed integer
and exact arithmetic implementations. Ordinary documents keep the integer
`RepeatLayout` API and storage. Exact lookup reports arithmetic exhaustion
explicitly, including when a reduced total fits but an interior prefix does
not. A dormant final gap is validated and retained without entering that
play's output arithmetic. This completes the numerical Repeat-index prerequisite;
persisted effective extents, Sequence/Retime clock consumers, marks and Preserve
routes still need integration before fractional insertion can be admitted.

Production plan prefixes now use
that exact interface in picture, audio, owner and retained-context walkers.
Sequence entries and compiled definition offsets also carry exact ratios.
Integer Repeat storage embeds directly behind the interface; implicit gap
recipes retain their separate authored duration. `single_occurrence_exact_range`
preserves exact offsets, while the integer wrapper refuses fractional endpoints.
That prefix increment left node extents and stored Retime mappings integral.
Changing prefixes alone does not admit a fractional project or a Preserve splice.

Effective plan extents now separate
exact output allocation from integral recipe counts throughout the compiled
picture/audio walkers. Normalized framing and Camera's temporary preview use
the sampled effective extent. Definition point grids, owner windows and natural
voice admission use exact output bounds. At that milestone, compilers embedded
integral document durations. Ordinary Preserve refuses differing allocation and intrinsic counts
until a route can express that relationship without resizing its input.

Persisted effective clocks now
connect that representation to strict document admission, reversible patches,
SQLite snapshots/history and production picture/audio compilation. Sequence
and compact Repeat totals use exact child extents; the root must be integral.
Legacy duration maps and ordinary reducers refuse these documents. Current
admission still refuses marks and sounds. Audio lineage
is admitted while no Retime processes pitch: FollowSpeed stages and unity
Preserve partitions only resample affinely. Shift is refused, and core 55
admits Preserve only with an owned input projection.
[Retimed pauses](INSERT_TIME.md#pauses-inside-followspeed-speed-changes) are the
first command that authors these clocks, and ordinary commands now
[edit around them](INSERT_TIME.md#editing-documents-with-exact-clocks) while
refusing changes inside a clocked speed change.
Exact snapshot restore
and guarded patches preserve the complete composition. These explicit temporary
boundaries must be replaced by real lifecycle transforms as splice authoring
is integrated; they are not completion of the insertion requirement.

### Preserve input projection

Retaining a timing layout is not enough to retain a processing operand.
The current `AudioSignal` walker still feeds the current child into a nonunity Preserve
stage. Inserting a Hold inside that child would therefore change its input and
history. `FrozenAudioContext` is a detached revision snapshot, not a live body;
audio lineage is not a pointer to equivalent current PCM. Neither supplies this
missing projection.

The full splice needs an owned route with separate input and output maps.
An input map projects the full intrinsic input range onto current owned
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

Core 55 now persists the [owned input](OWNED_PRESERVE_INPUTS.md) in each derived
Preserve clock and compiles it into the checked preparation consumer below.
Targets stay inside the stage's owned subtree and are validated under bounded
work; strict descendant ownership prevents cycles. The output route remains
open. Split, isolation,
deletion, trimming and movement need atomic route transforms or explicit refusal.
Cache identity must include the resolved projection and all current recipe
dependencies. Complete retained input support, envelope ownership, repeated
occurrences, non-silent inserted policies and later stage edits still require
explicit rules and end-to-end PCM qualification. Intrinsic preparation alone
does not establish the complete authoring boundary.

The borrowed [audio input tape](AUDIO_INPUT_TAPES.md) now supplies the current-tree
input view and an actual PCM reader through `StageAudio`. Exact runs share one
PointCeil grid and preserve full sampling support across allocation seams.
`AudioStageProjection` now supplies checked full intrinsic operands and an
independent exact output-policy view. Nested inputs refer to child intrinsic
output; a separate PointCeil tape places pauses around it. Request-local identity
memoization preserves canonical history across chunked reads. The owned input
compiler now supplies these projections from saved current-tree selectors.
Ordinary root-plan evaluation refuses a projected owner until its output
placement is integrated.
[Projected root placement](AUDIO_PROJECTED_ROOT.md) now supplies separate
absolute RoundEven allocation for one physical projection, preserving its
exact PCM phase and root-policy clock through crops and repeated reads.
Persistent output clocks/lifecycle, aggregate output scheduling and atomic
splice construction remain necessary for the nested splice.

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
insertion derives its audio placement from the edited tree.

## Repeat gap operands

`AudioDefinitionSelector::RepeatGap` exposes the current configured gap
recipe in its local-zero PointCeil clock or an explicit root/point placement.
It remains available with one play and invents no preceding-play identity.
A surviving gap is sampled from its current position like any other owner.
See [gap clocks](AUDIO_DEFINITIONS.md) and
[the current Original-moment contract](SOURCE_MOMENTS.md).

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
equality cannot merge distinct positions.

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
Assert original sampling steps, absolute later boundaries,
silence suppression, irregular-seek parity, compact billion-play queries, all
mark spaces/biases and atomic identity exhaustion. Durable undo/redo must retain
domain/anchor/envelope metadata as well as picture structure.

Core 13/database 19 freeze the old mark grammar throughout all legacy adapters.
Database 18 replays through frozen core 12, retaining Partition purpose; database
16/17 replay through frozen core 11 and reject purpose. Every old mark gains only
its original binding. Core 14/database 20 freeze the schema-13 multi-binding
grammar and reject Split in old requests. Legacy history cannot acquire
fabricated lineage evidence.
