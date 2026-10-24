# Exactly one InsertTime leaf: reuse of its successful net transaction

Read-only design investigation against the Main 9211db7043d14e57e06782342aa7defd1313a8b6 source identified by Root. No builds, tests, runtime experiments, Git actions, or repository changes were performed. This note owns only its /tmp path. Source anchors describe the inspected source; Root should recheck them against concurrent edits.

## Decision

One small private prototype is defensible. Retain the EditTransaction already produced successfully inside apply_semantic_leaf, and use it only when the existing capability actually hits at replay's ordinary leaf boundary. The final compound transaction is that leaf transaction with three metadata changes: forward.to_revision, inverse.from_revision, and description.

This is not direct reuse of the unmodified leaf transaction. It replaces the final net_transaction validation/diff/inverse/changed-ID traversal using an already successful, immutable result whose only subsequent authored change is its current revision header. All original prefix admission, revision checks, Ledger.reserve, wire serialization, work charges, Ledger.observe, visitor, final document size check, Store transition authority, actual aggregate edit size/readback, inverse verification, and history checks must remain.

The proof has a concrete maintenance condition: after a successful reuse hit, no authored mutation beyond current.revision_id may occur before aggregate selection. If that ceases to be true, invalidate this fast path. Do not turn this into a generic cache, public rebasing facility, or a second plan-authority API.

Retaining the leaf edit retains its sparse forward and inverse patch state longer. This adds real live memory even though it introduces no ProjectDocument snapshot, encoded buffer, serializer, persistent object, or global cache. Patch cloning can erase any measured benefit. This note predicts no performance or gate win.

## Exact leaf versus compound result

Relevant definitions are command.rs:690 (DocumentPatch), :770 (DocumentPatch::rebased), :1052 (EditTransaction), :1873 (net_transaction_known), and :3594 (command descriptions). The final aggregate call is compound.rs:397 approximately, with the exact source statement:

    command::net_transaction(document, &current, "Apply resolved transaction")

"aggregate_from" is a profiling label, not this source function's name.

Let E be the exact entry, L its successful ordinary InsertTime leaf result, rL the leaf new_revision, and rO a valid outer compound new_revision. Existing replay has exactly one step, invokes its read-only visitor with L, then makes O by assigning L.revision_id = rO. There are no inputs or register writes.

| Field | Leaf transaction E -> L | Final aggregate E -> O |
| --- | --- | --- |
| forward.project_id | E.project_id | identical |
| forward.from_revision | E.revision_id | identical |
| forward.to_revision | rL | rO |
| forward optional presentation and every authored change map | diff(E, L) | identical |
| forward.audio_bindings, including timing table/allocation IDs | diff(E, L) with leaf identities | identical, including leaf identities |
| inverse.project_id | E.project_id | identical |
| inverse.from_revision | rL | rO |
| inverse.to_revision | E.revision_id | identical |
| inverse authored values and audio bindings | inverse of the same changes | identical |
| changed_ids | sorted union computed from those changes | identical |
| duration_delta | duration(L) - duration(E) | identical |
| description | "Insert pause" | "Apply resolved transaction" |

EditTransaction has no outer request, ticket, command payload, register bank, visitor, or proof field. Store serializes the actual outer CommandRequest separately. This reuse must not change semantic-plan tickets, prepared command identity, revision reservations, Store authority, or which request is persisted.

DocumentPatch::rebased changes only from_revision and to_revision via a cloned patch. It does not recursively rewrite authored revision identifiers. That is precisely the needed helper. An AudioTimingId allocation made by InsertTime still names rL after ordinary compound replay. Replacing it with rO changes authored state and is incorrect.

The construction should clone each patch once through rebased, clone changed_ids, copy duration_delta, and assign the compound description. Cloning the whole EditTransaction and then rebasing its already-cloned patches performs avoidable additional patch clones.

## Why equality is defensible

one_leaf_replay.rs:40 currently obtains (_, next) from apply_with_result. That means the entire real leaf application, result validation, net patch creation, inverse construction, changed-ID enumeration, and identity checks succeeded before the capability exists. Its admit_next callback must also succeed before retention.

command.rs:1873 constructs forward from E and L. Revision IDs affect only its patch headers. Other DocumentPatch changes come from document fields that are identical between L and O. The aggregate's inverse and changed_ids are deterministic functions of those same changes. Root duration is unchanged by revision identity.

The leaf command tail, command.rs:1799 approximately, explicitly records that validation reads neither revision identity nor unchanged tail state. document.rs:1118 and :1172 perform normal complete validation over structure, sound support, framing, attachments, source interpretation, gains, picture context, basis, lineage, bindings and marks. The inspected production validators do not consult the current revision header. This claim must remain true; an authored timing ID that contains a revision is data and remains unchanged.

The aggregate path uses KnownValidation::Nothing, while a successful leaf can reuse retained complete or structural validation. Successful apply_with_result still establishes complete result validity under the normal document limits. This proposal reuses that successful proof across a revision-header change. It does not claim the complete aggregate validation function is still called in release mode.

A debug-only exact oracle can recompute the old net_transaction after final admission and assert whole EditTransaction equality. That is useful protection against future drift; it is not release-path work avoidance evidence.

## Smallest private prototype

Change only one_leaf_replay.rs, compound.rs, and focused tests. Store should need no implementation change.

1. In apply_semantic_leaf, bind (edit, next). Move edit into the private candidate only after the original admit_next callback and existing retain/mode/value/InsertTime eligibility succeed. Do not mint a capability from mutable public SemanticPlan fields. Do not add Clone, serde, constructors, or public accessors.
2. Have the existing crate-private reuse return a private hit record containing the existing next clone plus a borrow of that private edit, or an equivalent scoped internal mechanism. Return the edit only after the existing full entry/request comparisons and the original EXACT_EDIT reset, revision check, and sealed-preserved behavior have succeeded.
3. In replay_compound_inner retain a local borrowed edit only from that actual successful hit. Exact shape must still be one ordinary InsertTime, no inputs, and isolation must be None. Any miss, reference/legacy mode, provisional or derived state, changed entry/request, extra step, AI/occurrence shape, capture/register operation, or isolation path retains the original net_transaction fallback.
4. Keep post-leaf wire size, staged charge, identities.observe, visitor, and current assignment in the same order. Do not construct the aggregate early. Set only current.revision_id as before, run the actual final document wire check and work charge as before, then construct the header-rebased cached edit or call the old aggregate factory.
5. Keep Store's actual final validation and every existing codec/size/authority/SQL/history check. Update any comment that says the original aggregate factory always executes.

Do not select cached net data independently by revision ID at the end. A candidate can be present while reuse missed. Carrying the borrowed edit from the actual hit makes that provenance explicit and prevents accidental use on fallback results.

## Error order and bound obligations

compound.rs:262 replay_compound_inner has the relevant order: outer revision admission; transaction validation and leaf/outer revision separation; initial 64 MiB document wire check; Ledger construction; leaf identity reserve; pre-leaf wire and 512 MiB cumulative work charge; actual hit or original apply; post-leaf wire and cumulative charge; identity observation; visitor; final outer revision assignment; final wire and cumulative charge; aggregate.

All those call sites stay, not merely equivalent estimated byte counts. A different outer revision length can make the final document too large even when the cached leaf document fitted. A visitor error must still win before final admission or aggregate construction. Prefix, identity and post-leaf refusals must retain their existing visitor counts.

The leaf edit and aggregate edit are not the same serialized length: outer revision headers and the compound description differ. Never reuse a leaf edit's encoded bytes or a cached leaf byte count. deadpan-store/src/lib.rs:1445 command_plan must still serialize the actual outer request and actual rebased aggregate edit, check each 64 MiB limit, decode/readback and compare them, and verify edit.inverse.restores(next, current). Store compound.rs:437 also retains ValidatedDocument::new for the actual result.

A single eligible step has four document-work charges: initial, pre-leaf, post-leaf and final. Each admitted charge is at most 64 MiB, so it cannot itself cross the 512 MiB cumulative limit. Preserve the charge sites anyway. Exercise cumulative overflow with an existing noneligible multi-step fallback test, not an artificial lowered limit.

The existing real-limit private test one_leaf_replay_real_wire_boundaries_preserve_initial_post_and_final_refusals at one_leaf_replay.rs:288 is especially relevant. It uses 12,002 nodes and legal escaped labels, checks initial/post-leaf refusal before visitor, seals a leaf result exactly at 64 MiB, then lengthens the valid outer revision to cause final refusal after visitor1. Preserve this actual boundary.

That document-limit fixture does not prove the Store aggregate edit limit, since unchanged padding labels need not occur in sparse patches. A new Store edit-limit fixture, if required, must genuinely widen changed patch data or legal IDs enough to cross the production 64 MiB limit with the outer headers/description. Do not lower caps to manufacture a claim. Do not omit or weaken the existing Store size check if such a large fixture is costly.

## Weakest decisive counterexamples and verification

Run the current candidate-Some versus candidate-None oracles first. Compare whole EditTransaction and its actual serialized representation, final document wire, per-step before/after/request/value visitor sequence, register writes, error code/message, visitor count, and inverse/forward behavior. A forward-patch-only oracle misses the description and inverse-header errors.

Decisive additions:

- Replay one real capability with two distinct valid outer revision IDs of different lengths. Assert forward.to_revision and inverse.from_revision name each outer revision, while inserted authored timing allocation IDs still name the leaf revision. Assert the compound description exactly. This kills unrebased reuse and recursive authored-ID rebasing with one fixture.
- Exercise a genuine hit and all existing changed-entry, changed-leaf, swapped-witness, mutable-plan, isolation, reference-mode, provisional, derived, extra-step, register/capture, AI and occurrence misses. Cached aggregate data must never escape a miss.
- Retain the visitor refusal oracle. It must see one unchanged visit and fail before aggregate cloning/construction. Nested scoped mode/reference helper calls from the visitor must restore their state; no persistent cache state should exist after a refusal or panic.
- Retain real 64 MiB initial, post-leaf and final-outer boundary refusals, with exactly the same visitor counts. Retain actual 512 MiB fallback cumulative-work refusal.
- Retain Store prepared commit Some/None equality, authority refusal before callback, callback/SQL rollback, transcript/correction invalidation, reverse-audio qualification and historic-allocation refusal. Actual aggregate size/readback and inverse checks remain authoritative.
- Retain real-media Some/None PCM, limiter gain bits, emitted-file verification and export equivalence. This code change does not by itself establish those results.

Relevant existing files and cases:

- core/tests/semantic_leaf_replay.rs:444 exact interior result/visitor/aggregate; :464 changed entry/swapped witness; :512 mutable plan; :539 changed leaf; :577 prefix refusal; :605 visitor refusal; :617 mode miss; :644 noneligible shapes.
- core/tests/semantic_leaf_replay.rs:743 signed zero; :793 numeric-looking labels; :815 explicit default wire timing. Preserve structural equality semantics.
- store/tests/compound_preparation.rs:511 prepared commit/history/undo/redo/reopen/portable copy; :567 authority; :640 rollback; :687 transcript; :733 reverse audio; :791 correction; :850 historic allocation.
- group_endpoints_export::one_leaf_replay_qualified_media_commit_export_matches_uncached is the existing real-media oracle (45 pictures, 72,072 samples).

The hard obstruction would be a single successful leaf result for which normal final validation, root duration, diff payload, changed-ID ownership, or inverse changes after changing only current.revision_id. The inspected factory and validators provide no such case. Revision-sensitive authored IDs do not create that obstruction because ordinary compound replay preserves their leaf values.

## Bounded experiment, only after Root's independent triage

First complete exact Candidate/None, refusal and real-media verification. Then compare a private prototype against the existing matched FFBA instrumentation baseline and the already qualified uninstrumented e6 control, using separate target directories and executed binary hashes. No new broad profiler is needed. Keep Root's existing matched fresh-process protocol and regions.

Measure retained/peak memory separately because one candidate now retains both sparse patches and aggregate construction clones them. First-pause patches can contain wide binding changes; "sparse" does not guarantee small. Keep or discard based on observed total latency and footprint with unchanged correctness/refusals. A lower net-construction region alone is not sufficient evidence. This note supplies no performance estimate and makes no product-gate claim.
