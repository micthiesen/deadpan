# Store generation verification plan: selected several-play duration

Read-only plan, 2026-10-21. Store tests only; no repository edits or execution.

## Current-path assessment

The existing store architecture appears sufficient; no store change is indicated by this design. `generation_scope::transitions::may_isolate` descends `Compound` leaves and includes `SetScopedHoldDuration`. `generation_preparations::transitions::derive` replays each leaf, maps prior births through each actual leaf proof, removes births superseded by later provider edits, and runs `leaf_births` for the current leaf. The scoped-duration branch maps the supplied authoring target through the exact proof and uses the resulting target for provider lookup and AcceptedExtension creation. Commit and undo/redo scope transitions also use actual Compound isolation proofs. Existing single-leaf `scoped_mixed_duration.rs` already exercises accepted-provider admission, mixed target mapping, generation preparation, reopen, undo cancellation, and redo.

The gap is a multi-leaf Compound with selected targets that began on shared identities, therefore tests should establish test inputs where at least one target has an independently owned accepted Hold and another still references a shared Hold. Existing test's one mixed leaf (and the one-leaf Compound variant) does not prove birth mapping as earlier leaf clones common ancestry and later leaf edits a target on the staged tree, nor that a later leaf invalidates/supersedes earlier preparation intent.

## Minimal high-value fixture and oracles

Use existing `accepted(&package)` fixture and its real bundle acceptance. Wrap generated `hold` in inner and outer Repeats, retaining the accepted recipe/provider snapshot. Build two `ScopedNodeTarget`s at distinct explicit Play IDs on one common selected Repeat axis, with the same earlier prefix and body-only ancestry. Preserve both targets in the frozen invocation order. To test mixed ownership, first create a real scoped edit under one selected play so its Hold is independently owned; leave the other selected play sharing the original Hold. Read the staged document and assert this precondition directly: target node IDs are different where expected, and the unowned target is still the original shared node. Do not assume NodeId equality/inequality proves the selected plays or ownership by itself.

Use distinct leaf revisions and actual `SetScopedHoldDuration` leaves, with exact identity pools from requirements on each sequential staged document. Commit one Compound. Assert:

- Both requested branches now have the requested duration; unselected Default and Play branches retain their original recipes and accepted provider. Resolve the result by `derive_scoped_isolation` on the final request and compare against the preparation's staged target mappings, not reconstructed IDs or a net duration calculation.
- The accepted provider, audio, and picture context are preserved for any shortening/in-support target. For each over-support target, there is exactly one queued AcceptedExtension preparation at the exact mapped target/origin target and requested duration, carrying the admitted artifact and AcceptedArtifact controls. No unrelated/current provider target receives a preparation.
- Reopen retains the exact committed authored snapshot and preparation target/origin/state. Undo restores the pre-Compound snapshot modulo revision identity and cancels every pending preparation born from the compound. Redo restores the exact final snapshot and recreates only the final relevant preparations at the same mapped targets (new preparation IDs are expected). Run `validate_full` after commit, reopen, undo and redo.

## Two targeted lifecycle variants

1. **Independent ownership plus shared target, simple final births.** Use one selected family already independently owned, one still shared. Resize both beyond accepted support. Check there are exactly two births, one per actual final target, with no birth on the unselected/shared Default. Reopen, undo, redo as above. This is the minimal direct proof that the host does not merge targets by current NodeId and that preparation targets follow staged clones.

2. **Zero-net and superseding provider intent.** Choose two target Holds with different original durations (e.g. 8 and 16 frames) and set both to 12, so one expands by 4 and one shrinks by 4. Assert the final document content differs from the initial document although total project duration is unchanged; the Compound gets a history revision, has a non-empty forward patch, and undo/redo restore exact documents. Do not use root duration delta as the admission or commit oracle. This case protects audio/source boundary mappings: compare the affected Original/A/V sample and source boundaries (including adjacent suffix audio mappings) before/after each staged leaf and in the final plan. A zero aggregate frame delta must not mean zero timing work.

For lifecycle supersession, order a target so an early leaf grows beyond accepted support (creates a birth), then a later leaf shrinks that same mapped provider back within accepted support. Assert the final stored preparation set excludes/cancels the obsolete birth and the final video/provider equals the actual staged reducer's result. Do not assert that accepted video is automatically restored: the existing reducer's per-leaf semantics may have replaced it with deterministic fallback, and duration reversal alone is not proof of provider restoration. Conversely, if a later edit leaves the provider unsupported and changes its fallback, assert no stale birth remains. This distinguishes “generation is required by final state” from “some intermediate leaf once requested it.”

## Guardrails against weak assertions

- A preparation count alone is insufficient; assert exact target, origin target, accepted artifact/control, duration, state, and absence on unselected targets.
- A final total duration alone is insufficient, especially in the zero-net case; inspect exact target Hold recipes, branch ownership, full plan/audio boundaries, revision/history, and exact inverse.
- A generation request's canonical target is not the receipt oracle for a scoped mixed target; derive and assert the actual final target.
- Don't build synthetic Accepted artifacts or directly seed preparation rows; use `accepted` so store admission and durable receipts are exercised.
- Don't test only all-Play concrete occurrences for this batch: the feature accepts selected authoring families, including mixed Default/Play paths, and per-target staged mapping is the main risk.

## Store implementation expectation

No new generation transition or database representation should be necessary if app emits a flat Compound of existing scoped duration leaves and uses the existing `SetScopedHoldDuration` command. If the tests show stale prep births across leaf edits, wrong targets after later proof mapping, or undo/redo scope drift, fix the transition derivation/indexing; do not add a parallel batch-only preparation mechanism. Existing `may_isolate`, leaf birth mapping, proof-backed history mapping, and re-open persistence already cover the relevant primitives.
