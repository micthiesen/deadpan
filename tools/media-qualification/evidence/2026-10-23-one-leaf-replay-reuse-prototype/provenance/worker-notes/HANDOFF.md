# One-leaf replay reuse: source-only handoff

Implementation and read-only source review complete. Unformatted, uncompiled and unexecuted. No Cargo, formatter, tests, app, runtime, benchmark, parser execution, Git writes or main-checkout edits occurred. Root owns verification, measurements and adoption.

## Source identity

Fresh archive of commit `3a32c923358c0cb7ca53f821bfb6db3e01ebb466`. All 2,051 baseline Rust files matched the qualified current-leaf original map before edits. Six pinned inputs are unchanged. Control app SHA-256: `a9ce60a4d227e8c8107b06602915e6323e0cfabbf12610b67a85eda77bdd4a0f`.

Fourteen files changed: twelve existing and two new. Proposed Rust count is 2,053. All 2,039 nonowned baseline Rust files and all 34,573 nonowned baseline files are byte-identical. No old instrumentation source or Cargo cache was reused; no app file changed.

Exact before/proposed hashes, full file list and audit counts: `source-hashes.json`. Full preformat diff: `prototype.patch`. `production-review.patch` is a convenience excluding test files and the tests-only audio_reference/retimed edits. Originals are retained under `before/<source-path>.rs.txt`; proposed snapshots under `after/<source-path>.rs.txt`. Complete source/input maps and basis are retained in the scratch root.

## APIs and lifetime

New public opaque Core type `OneLeafReplay`, reexported from its private module. Its complete entry ProjectDocument, leaf CommandRequest and leaf-revision next ProjectDocument are private immutable fields. No public constructor, Clone, Deserialize, serializer, setter or mutator exists; Debug reveals no values.

New public APIs:

- `SemanticPlan::one_leaf_replay() -> Option<&OneLeafReplay>`
- `deadpan_core::replay_compound_with_one_leaf(document, request, visit, candidate)`
- `ProjectStore::commit_compound_prepared_with_one_leaf(request, relevance, guard, candidate, prepare)`

Existing `HostSemanticPlan::commit_compound_prepared` borrows the optional capability with its existing transcript guard. Existing public APIs default to None; history replay explicitly supplies None. Store enters the same shared Immediate transaction body, with no duplicated transaction or app special case.

Private Core helpers are `eligible_value`, `eligible_mode`, `eligible_compound`, `apply_semantic_leaf` and `OneLeafReplay::reuse`. Store adds private `prepare_command_with_one_leaf` and `prepare_current_command_with_one_leaf` before its existing shared preparation.

A plan owns at most one capability. The first ordinary InsertTime may create it only while resolved steps, register inputs and writes are empty. A second resolved operation discards it. Later leaves cannot recreate it because steps are nonempty. Final retention also requires no writes and exactly one ordinary InsertTime Edit without inputs. AI, occurrence isolation and register shapes are excluded. Selection motion cannot retarget the private values.

The capability holds one entry/request/next triple, never the leaf EditTransaction, serialized bytes, a list or global state. It dies with its SemanticPlan/host operation and adds no history, register or queue persistence. Clones share existing immutable structures where supported, while some collections clone. Cardinality/lifetime are bounded; heap bytes and peak process memory remain unmeasured.

## Eligibility and sealing

Creation owns the original `apply_with_result` call and drops its transaction at the original destructuring statement. The unchanged post-leaf canonical serializer and cumulative charge execute through a private callback. Only after success is the capability constructed.

Creation and use require `command_work::local()` and compact timing representation. Stored entry and result require empty derived clocks and `!audio_bindings().has_provisional_timings()`. Use repeats these predicates for current, stored entry and stored result.

Use requires full same-value Eq of complete current document and complete leaf request against stored values. No hash-only, revision-only, pointer-only, wire-only or partial comparison substitutes for Eq. Same-revision changed values and public plan request mutation fall back. The accessor cannot mutate the capability. Original canonical serializers still execute; equality alone cannot admit provisional representation or bypass wire failures.

## Reused pure work and retained authority

A hit intentionally reuses the successful pure leaf validation/application result. It avoids repeating that leaf's picture/gain/input checks, reducer, binding/capture/split/phase/mark/lineage/pruning/timed-basis work and full leaf-result/patch construction/validation. This is not a claim that every check reruns.

Lookup occurs only at the original ordinary apply insertion point, after outer revision, transaction/input admission, ledger reserve, mandatory entry serialization and original prefix charges. Isolation retains its original branch.

On hit, ordinary destructive EXACT_EDIT entry reset executes, followed by original leaf `command::check_revision`. The exact result is cloned, then exit `take_exact_edit` executes and `derived_clock::sealed_preserved` runs when appropriate. Eligible entry/result derived clocks are empty; nothing between these operations sets EXACT_EDIT. Fallback retains the original apply expression.

Original next serialization/charges, ledger observe, host visitor, bank writes, final aggregate net construction/validation, forward/inverse/history codecs and current Store authority still execute. Shared Store checks writer, Immediate transaction, transcript guard, current snapshot/head, register bank/frozen inputs, allocation/revision uniqueness, media/analysis/corrections and boundary transitions, runtime host preparation, history readback/inverse, SQL and atomic commit/publication. Existing real-media authority remains independent of eligibility.

No validator, refusal, limit, charge or production ceiling is weakened. The eligible one-leaf staged 512 MiB refusal is unreachable with four mandatory traversals individually capped at 64 MiB; no fake smaller ceiling is introduced. `budget-boundaries.md` gives exact stage reachability and the actual-cap fixture. It covers initial/post/final wire refusals; the final case proves a private helper-created actual hit, rather than claiming returned end-to-end SemanticPlan exposure. Initial/pre serialize the same entry, so pre cannot newly fail. Root retains existing multi-step budget verification.

## Tests and unresolved execution

Twenty-four new `one_leaf_replay_*` tests are listed in `test-inventory.json`: seventeen Core and seven Store. Detailed public-test scope is in `../test-worker-notes/HANDOFF.md`; the actual media/corrections additions are described in `../test-worker-notes/STORE-AUTHORITY-HANDOFF.md`.

Core covers exact document/request/edit/visitor/outcome bytes and forward/inverse, reconstructed equality, changed same-revision values, swapped witnesses, public-plan mutation, changed requests, prefix/identity error order, visitor refusal, reference modes, ineligible multi-step/register/AI/isolation/derived shapes, rational signed-zero normalization, distinct authored strings, explicit defaults and constructor bounds. Private cases establish actual Some hit, mode misses, sealing after admission, equal provisional canonical refusal, and destructive TLS entry/exit.

The TLS test preserves ambient scoped-timing-capture false/true across hit and changed-request fallback, comparing uncached exact outcomes. A prefix failure before the leaf preserves stale EXACT_EDIT exactly as uncached replay does; ordinary leaf entry resets it.

Store cases compare exact outcomes/cells/history/bank/document plus undo/redo/reopen/portable copy, current head/bank/input/identity refusal before callback, host rollback, SQL rollback with prepared-host Drop and successful retry, and current missing-Original transcript guard before preparation. Additional cases decode/admit the existing offset-bframes fixture, establish a valid preview, then remove the Reverse audio qualification receipt or change actual transcript corrections while the document and capability remain identical. Stale guard versus current fresh guard, exact Store variant/message, callback zero and every table cell are compared. Historical consumed outer/leaf revisions are checked after advancing the head. Existing broader real-media qualification remains owed by Root.

Unexecuted fixture expectations for the first focused compile/run:

- Bound nested Hold compact/reference wire inequality assertion.
- Captured Background framing with exact zero geometry, source-reviewed as permitted by validate_hold.
- Private normal partial-binding validity and provisional plan canonical first-error boundary.
- Existing JSON refusal vocabulary used in ingress assertions.
- Actual decoded-media fixture eligibility/valid previews and the source-reviewed missing-qualification and changed-analysis refusal messages.

These are fixture uncertainties, not passes. Independent source review finished with no unresolved findings and verified all fourteen proposed pins and patch SHA; its report is `../independent-review.json` (SHA-256 `9107e928c9cc9f5ac024ddf9cf6fe28bba6d7d9032a3b3c9b80dca48d91aa52c`). Formatting, compilation, focused Core/Store tests and existing budget/property/media checks remain owed by Root. No source-only claim establishes performance gain or a memory ceiling. The old roughly 43 ms replay group is an opportunity bound, not a prediction.
