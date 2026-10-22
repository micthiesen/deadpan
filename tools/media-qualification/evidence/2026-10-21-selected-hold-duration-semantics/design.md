# Selected several-play Hold duration: next-slice design

Read-only source assessment, 2026-10-21. No repository changes or execution evidence.
Current mixed-duration files remain frozen. Root alone runs Cargo and Git.

## Recommendation

Use one resolved Compound of the existing duration primitives, with one fresh
revision per changed target. Reuse actual prepared isolation proofs after every
leaf. Do not add duration to value-only ScopedNodeEdit/EditScopedMany, and do not
reinterpret mixed target choices as a representative's all-Play path.

First family: 2..=1024 captured Hold body targets, one common selected Repeat
axis with distinct explicit Play identities and identical earlier authoring
prefix. Remaining deeper Default/Play choices stay explicit and may differ in
the already mapped counterpart set. Ordinary Sequence/Repeat ancestry only.
Keep the current mixed owned-gap and all Retime/Partition exclusions. This
advances :scope plays without inventing arbitrary overlapping-set semantics.

Normative basis: spec 5.2 separates default definitions from stable per-play
overrides; 4.3 requires absolute origin-based sample boundaries; 7.4 makes a macro
one undoable transaction and resolves semantic intent against each staged state.
There is no product ambiguity about setting the same requested duration in each
selected family. Saved SetHoldDuration intent remains duration only.

## Existing boundaries and exact reuse

- app/project/scoped.rs::Target has primary target, optional presentation,
  absolute cursor, inspector root and also: Vec<ScopedNodeTarget>.
  validate rejects also==primary but does NOT prove uniqueness among also,
  common-prefix uniformity or disjointness. It is insufficient batch admission.
- app/preview/scoped/model.rs::switch_plays_in sorts/deduplicates UI play numbers;
  switch_in clears previous sets, so normal UI selection varies one Repeat
  level. also_targets retargets structural counterparts until an actual receipt
  provides mapped_also.
- app/project/scoped.rs::retarget checks matching node-kind/path slots and
  uses stable Play IDs. Do not invoke it again after edits to reconstruct
  counterparts. Pause insertion already demonstrated non-positional results.
- app/project/pause/batch.rs::uniform_level is the precise reusable selection
  contract: distinct explicit Play at one shared Repeat, identical preceding
  authoring prefix, not merely different final NodeIds.
- app/preview/scoped/model.rs::hold_parameter_in currently deliberately returns
  None for level.also or mapped_also, even when hold_duration_available is true.
  Native semantic constructors also reject nonempty Target.also.
- core/scoped_edit/isolation.rs::{apply_many_mapped,many_requirements} correctly
  map pending targets through earlier clones, but perform VALUE edits under
  one allocation. Do not use these reducers for temporal duration changes.
- core/scoped_edit/mixed_hold_duration.rs provides exact mixed requirements,
  positive dormant-body proof, actual temporal isolation, and then ordinary
  SetHoldDuration lifecycle once. PreparedScopedHoldDuration maps targets and
  concrete instances through actual clones and gives exact final local-zero
  boundary. Its command intentionally requires both Default and Play.
- For an originally all-Play target, the existing
  occurrence_hold_duration_requirements/prepare_occurrence_hold_duration and
  EditOccurrence::SetHoldDuration remain the admitted single-occurrence route.
  This is valid only because the *supplied authoring target itself* is already
  all-Play and matches the concrete instance. Never use it for a mixed target.
- core/insert_time/hold_duration.rs::apply captures old physical clocks of all
  affected default/owned suffixes; all-Play occurrence preparation has its
  existing concrete suffix capture. A later leaf reads the earlier leaf's
  retained clocks, rather than manufacturing fresh source origins.
- core/compound.rs::{LeafEdit,ResolvedStep,ResolvedTransaction,replay_compound}
  already provide bounded, unique leaf revisions, aggregate bytes/work limits,
  one final transaction and exact inverse. MAX_COMPOUND_STEPS and
  MAX_SCOPED_TARGETS are both 1024.
- core/scoped_edit/proof.rs::derive_scoped_isolation returns a VALIDATED proof,
  not an untrusted record. map_forward and map_instance_forward carry every
  captured target/presentation through the entire Compound.
- Existing app pause/batch.rs is an almost direct host preparation exemplar:
  staged document, map pending sites and earlier results after each leaf,
  create one Compound, then preview that exact final request.

## Proposed preparation algorithm

1. Capture primary + also once, in their stable supplied order. Preserve the
   first entry as primary; do not sort later by moved root time or fresh IDs.
   Prove 2..=1024 entries and the uniform selected-axis contract. Reject duplicate
   selected IterationIds at that axis. NodeId equality alone is NOT duplication:
   two currently unowned Play targets legitimately reference the same Hold.
2. Validate every target before allocating: current revision/project/root/scope,
   Hold kind, complete branch path, body-only Sequence/Repeat ancestry, current
   external root/beat sound refusal and positive requested integer duration.
   Validate even entries whose current duration already equals the request.
   Keep all authoring branches. Do not accept arbitrary Target.also from callers
   just because each target independently validates.
3. Retain a concrete presentation for the primary if live. For other mixed
   targets, exact identity mapping needs no presentation. Positive body-liveness
   proof permits dormant None; a presentation used for per-target visibility
   must satisfy matches_instance. If needed, pause/batch.rs::presentation shows
   a bounded representative search (sparse overrides+1), but it must return None
   for a positively dormant family rather than invent a play. An all-Play body
   target has its explicit stable iterations, so no default representative is
   selected or inferred.
4. Stage targets sequentially. For each current mapped target, read the staged
   Hold duration. If already requested, retain the target but skip allocation
   and edit. If ALL entries are no-ops, refuse without a document/history write.
   This matches existing many-value editing and avoids manufacturing overrides.
   For changed mixed targets, query scoped_hold_duration_requirements and
   prepare_scoped_hold_duration with exact pools. For changed all-Play targets,
   use the established all-Play requirements/preparation. Every changed leaf
   has a distinct fresh revision, including when its identity pools are empty.
5. Apply the leaf's ACTUAL proof to all pending targets and every already
   processed result, plus all retained concrete presentations. The common outer
   Play may clone the selected inner Repeat itself. Query the next target on
   that mapped staged document, not the original one. No per-rendered-play loop.
6. Build one ResolvedTransaction with every changed leaf and final revision;
   preview the full Compound under aggregate limits. Do not publish staged leaf
   results or one history row per target. An identity collision, generated-media
   qualification failure or unsupported last target rejects the whole request.
7. Map the original primary+also through derive_scoped_isolation on the FINAL
   actual request/snapshots. Compare against staged results. Resolve the primary
   Hold's final local-zero boundary only AFTER every resize; a later earlier-in-
   time target may shift the primary again. Preserve inspector root, ordinary
   parent/root selection and all Default/Play choices. Return mapped also
   explicitly in the receipt, even when some no-op entries produced no leaf.
   A dormant primary retains no fabricated boundary; preserve/check its captured
   cursor under existing context bounds rather than sampling another target.
8. Record/dot through duration-only batch invocation authority so native Apply,
   recording, named calls and dot share the exact resolver. A saved program still
   contains just SetHoldDuration {length}; it does not contain captured IDs,
   explicit selected play numbers, already-resolved leaves or representatives.
   This requires a separate batch runtime context or a deliberately approved
   extension of a duration-only context; generic semantic_framing must remain
   unchanged. No new serialized command primitive is necessary for this slice.

A small shared duration-batch preparation helper is reasonable, but keep the
scalar existing command admission intact. The initial mixed command cannot
directly accept every all-Play batch member without changing its current
contract; dispatch by the *explicit target's* branch family instead.

## Why naive alternatives are wrong

- Shared Hold NodeId dedup collapses play 1 and play 3 before either is isolated,
  editing just one selected play or the shared default.
- Precompute all pools from the initial document: outer Play1 / inner Play0 and
  outer Play1 / inner Play1 share the outer clone. The second target must use its
  mapped Repeat and needs no second outer clone. Original IDs/pools overallocate
  or edit the wrong branch.
- One allocation for many raw duration reductions collides at timing
  {leaf_revision, ordinal:0}, or wrongly makes a later capture use the first
  leaf's old layout. Use distinct existing Compound leaf revisions.
- Convert outer Default/inner Play0 to its shown outer Play1/inner Play0:
  original 3x2 sparse fixture should grow by2, but this grows by1.
- Merely disjoint leaf NodeIds is insufficient for arbitrary scopes. A first
  outer Play may clone a subtree that a later outer Default target still
  shares. Explicit Play/Default overlap requires a different union/canonical
  family contract; do not accidentally introduce it through Target.also.
- Do not choose one constant total delta: selected counterparts may have
  different old durations and mixed Default multiplicities. Compute actual
  aggregate result through the reducer, excluding independent overrides.
- Do not reuse only the primary leaf's prepared boundary/proof for receipt:
  later edits can move it and clone common ancestry.
- Growing then shrinking an accepted provider during one macro can invoke its
  deterministic fallback. A later duration reversal does not automatically
  restore accepted video. Preserve staged reducer semantics and exact inverse.

## Body, gaps and dormant definitions

Body families with a distinct selected Play axis are disjoint in concrete
presentations even when later Default steps expand each family. They may be
dormant when a later literal default has all plays overridden, or when an
earlier Default ancestor is unused. Such an entry may still require Play
isolation to preserve authoring meaning.

An implicit default Repeat gap has no Hold NodeId. An explicitly owned gap has
a Hold node, but its active uses depend on nonfinal preceding play and its own
subtree extent, not default gap duration. Current mixed duration explicitly
rejects owned-gap ancestry; retain that guard for this first batch. Existing
all-Play APIs may admit an owned gap, so the batch's body-only guard must be
checked independently before that dispatch. Do not falsely infer gap liveness
from body override counts. A later gap increment needs final-gap dormancy,
owned positive extent and actual gap-clock qualification.

## Generation, history and proof

Store generation_scope/transitions.rs::may_isolate already descends Compound.
generation_preparations/transitions.rs::derive replays every leaf, maps prior
births through later actual isolation and retains final relevant targets.
SetScopedHoldDuration leaf_births now maps the supplied mixed target, not a
canonical all-Play reconstruction. Reuse this path, but prove it with real
accepted-provider per-target tests: one independently owned accepted Hold,
another still-shared accepted Hold, differing supports, later qualification
refusal, undo/redo/reopen and final birth controls on exact mapped targets.
An outer accepted alias must not claim another selected family's preparation.

Do not aggregate into a new many-duration primitive without updating store leaf
visitation; Compound is already the correct durable lifecycle boundary.

## Qualification plan / independent oracles

- Old native red: unchanged hold_parameter_in returns None after :scope plays
  1,3; public individual-duration controls prove targets valid first.
- Pure 3x2 body [Source2,Hold2,Source2], inner-owned Hold4 and outer-owned Hold7.
  Selected outer0,1 + inner Default grow2->3 changes root +2, leaves owned inner
  and outer2 intact. Outer Default + selected inner0,1 set length3 gives zero
  aggregate: shared inner0 +2 and owned inner1 -2. Both still change, so a
  root-delta-only no-op check is wrong. Setting both to1 instead gives -8
  aggregate (-2 and -6). These catch alias dedup, Default conversion,
  per-target old-length mistakes and override loss.
- Common outer Play plus selected inner0,1: first clones outer subtree, next
  requirement/pool follows mapped owner and only isolates what remains needed.
  Repeat IDs sparse/reordered. First and last selected representatives.
- Exact no-op member skipped; all no-op no write; no-op members still validated.
  Duplicate also entries, duplicate primary, incompatible prefix/retired IDs,
  wrong pools, last invalid target and last source qualification all atomic.
- Already-owned sets: no new node/mark names, distinct leaf revisions still.
  Local/source/occurrence mark transforms, mapped_also preserved after Apply;
  navigate away invalidates carried non-positional set as existing model does.
- PCM at 30000/1001, natural 44.1k Source and RoomTone, partial historical
  bindings, default/owned gaps outside targets, selected first and last,
  shrink+grow and mixed multiplicities. Compare old allocations at analytic
  moved origins, never align by event detection. Compare chunked/un-chunked.
  Compare complete final physical clocks after BOTH leaves, not just leaf1.
- Million plays with sparse overrides and bounded selected count: resources
  proportional to authored graph+selected targets, no expanded enumeration.
  Respect 256 binding terms and aggregate Compound work; do not raise limits.
- Final prepared Compound exact preview/commit parity, inverse/JSON,
  dry-run no writes, one history entry, durable undo/redo/reopen, generated
  preparation controls, recording/dot/named counted calls on fresh selection.
- Real decoded/export verification uses final new bus/silence policy and exact
  selected Hold providers; unchanged Source speech keeps old phase.

No runtime claim is made. Execute an old-code witness before implementation to
preserve a valid regression baseline. The user's ongoing implementation
authorization already covers this work; no new approval is needed.
