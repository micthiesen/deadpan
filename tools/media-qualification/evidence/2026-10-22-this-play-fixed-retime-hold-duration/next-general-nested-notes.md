# Generalize nested This-play fixed-Retime duration scope

This is read-only planning for the next increment. It does not change repository files and does not imply the current one-Repeat milestone has passed verification.

## Recommendation

Admit any nonempty, existing bounded vector of explicit Repeat body choices, subject to the document's existing `MAX_DOCUMENT_DEPTH` and `InstancePath` limits. Require exactly one fixed, unclocked Edit Retime strictly below the innermost Repeat, with Sequence ancestry everywhere else between the inspected outermost Repeat and the Hold. The outermost Repeat must remain the direct child of the captured parent/root. No arbitrary two-level cap appears necessary.

Keep mixed Default/Play, selected counterpart, owned/implicit gap as the target, Partition, derived clocks, sounds, any additional Retime, and a Retime between Repeat ancestors refused. This only generalizes the path where every Repeat ancestor already names one explicit stable Play.

Why the cap is unnecessary:

- `InstancePath::validate` bounds repeat ancestry by `MAX_DOCUMENT_DEPTH` (currently 256); the project graph separately bounds depth and nodes. The preflight already walks bounded structural ancestry.
- `ProjectDocument::hold_duration_visibility_for_instance` already consumes every `instance.repeats` entry in leaf-to-root order, validates its matching Repeat node and effective child via `ExactRepeatLayout`, and accumulates exact play offsets without expanding play counts. It stops at the supplied inspector root only after all repeat steps are consumed.
- `ScopedNodeTarget`, `temporal_requirements`, and `isolate_temporal` already operate on a vector of Repeat choices. Isolation proceeds outside-in, maps cloned Repeat IDs and the target, and validates the resulting presentation. `isolation_plan` counts subtree copies and checks document node, mark, and binding limits before names are allocated.
- The native index retains selected ancestry and compact Repeat layouts. Its hint can walk the same path without enumerating plays. The exact core preflight and endpoint-picture check remain authoritative at apply time.

The generalized root should be the first (outermost) Repeat in the outside-in `InstancePath.repeats` vector. During the core leaf-to-root walk, require the one Retime before the first Repeat encountered (therefore below the innermost Repeat), then consume every Repeat step in reverse order, matching its exact node and selected effective body; after the outermost Repeat allow only Sequences until the project root. Reject any Retime encountered after the first Repeat, any second Retime, and any unaccounted Repeat. Return the outermost Repeat as the eligibility root so the captured parent/root validation remains direct and exact. Confirm that native `State.root` and semantic scope construction retain this outermost root for nested selections before implementing.

## Exact isolation-pool witnesses

Use simple trees with no marks, sounds, or existing overrides for deterministic counts. Each branch's `subtree_order` includes its structural descendants.

For two Repeat ancestors:

- `Outer -> Inner -> Stage(Retime) -> Body[Before, Held, Speech]`: fresh Outer and Inner plays need `6 + 5 = 11` node IDs, zero marks. Outer copies Inner plus Stage, Body, and the three leaves (6); Inner then copies Stage, Body, and leaves (5). Require two isolation proof steps.
- With the selected Outer branch already owned and selected Inner branch shared: `5` nodes, zero marks, one proof step.
- With both selected branches already owned: `0` nodes and marks, zero new isolation steps; the duration still changes the authored Hold once.
- With Outer shared and the selected Inner play already owned: the outer copy must include the nested Inner's current default and owned branches exactly once. For the minimal fixture with one owned Inner branch of the same five-node subtree, expect `11` copied nodes, zero marks, one proof step, and a mapped selected owned branch. This catches loss or double-cloning of nested override state.
- A subsequent setter on the newly owned selected branch needs `0` node/mark IDs and no new isolation proof step.

For three Repeat ancestors:

- `Outer -> Middle -> Inner -> Stage(Retime) -> Body[Before, Held, Speech]`: fresh branches need `7 + 6 + 5 = 18` node IDs, zero marks, and three ordered proof steps. Assert the `ScopedIsolationStep` prefixes are progressively outer, outer+middle, and outer+middle+inner; assert target and every Repeat identity map to their copies.
- With Outer already owned, Middle and Inner shared: `6 + 5 = 11` nodes and two steps.
- With Outer and Middle already owned, Inner shared: `5` nodes and one step.
- With all three selected branches owned: `0` nodes and zero new steps.
- Add a selected Inner override under a shared Outer and verify the copied outer subtree preserves that inner override without allocating a second copy for it. Compute this case from a fixture with a known owned subtree and assert exact `temporal_requirements`, not just successful application.

For every pool case, wrong-size pools, duplicate or occupied IDs, and stale revisions must refuse before authored writes. Verify exact pools against both `occurrence_hold_duration_requirements` and the semantic allocator request; then validate the execution proof and compound replay/inverse.

## Behavior and refusal coverage

1. Use the existing two-level fixture as the first old-code control. A public `EditScoped::Rename` must prove the selected play can own the two branches, then `SetHoldDuration` must be the sole expected old-code failure. Avoid hand-constructing the post-isolation authority.
2. Add the three-level witness above so the API has no undocumented two-level product cap. Select first, interior, and last stable IDs at each level; include reordered IDs and nonuniform sibling durations, and derive expected offsets with `ExactRepeatLayout` rather than ordinal multiplication.
3. Put the only Retime below all Repeat ancestors. Cover left crop removing the intrinsic Hold start while a later sampled part remains visible; verify cursor and trace start at the first sampled picture. Cover exact In/Out center ties, shrink and growth, a whole-hidden crop, an unsampled interval, and a staged setter that makes the target disappear. The last three must roll back the entire run before commit.
4. Fixed Retime output duration, source mapping, every selected Repeat start, overall project duration, unrelated Hold recipes, and suffix/source recipe lattice must remain fixed/preserved as applicable. A silent placement fixture proves geometry only. Use nonzero qualified Source PCM to prove old-origin sample phase and output audio behavior separately.
5. Refuse before isolation allocation for any Default choice, owned gap target, mixed or selected-multiple-play context, three-or-more additional unsupported structural choices only if not Repeat bodies, `Partition`, derived pause clocks at the target Retime, root/attached sound events, two Retimes, a Retime between Repeat ancestors, or a Retime above the captured outer root. A dedicated one-Retime-under-two-Repeats topology should become supported, so move the old `scoped_hold_duration_noop_mixed_nonhold_and_retime_fail_before_allocation` refusal case to a genuinely unsupported topology such as Partition or an extra Retime.
6. Native cached-hint tests should prove the path is available for a three-Repeat explicit body and unavailable for a Default at any level, wrong effective override, an extra Retime, an ancestor Retime above the outer root, or a clocked Retime. Native service checks should retain the captured root, exact context, cursor and complete proof through delayed receipts, staged calls, Undo/Redo, reopen, and stale-capture rejection.

## Scope boundaries to preserve

Ordinary nested This-play duration edits with no Retime remain supported as before. Do not alter mixed or selected-play duration authority in this increment. Do not generalize Retime placement between Repeat ancestors: that changes which fixed temporal window isolates the Hold and needs a distinct policy and geometry review. Keep the final status contingent on root's full compile and test run plus the planned owner verification record.
