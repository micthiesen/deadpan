# Exact child capture and empty structural paste

## Implementation

Public API:

```rust
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SliceCaptureSelection {
    Range { range: FrameRange },
    Child { node: NodeId },
}
pub fn CapturedEditSlice::capture_selection(
    document: &ProjectDocument,
    parent: &NodeId,
    selection: &SliceCaptureSelection,
    timing: AudioTimingId,
) -> Result<CapturedEditSlice, EditError>;
pub fn CapturedEditSlice::selection(&self) -> &SliceCaptureSelection;
```

Existing `capture(document,parent,range,timing)` delegates through Range. Child requires the exact direct child of a named ordinary Sequence with ordinary Sequence ancestry. Its resolved bounds and one complete part retain the exact child ID, including empty siblings sharing a timestamp. Historical validation recaptures by selector and compares the complete payload.

Zero-duration Child capture requires a validated Sequence-only subtree without physical audio bindings. It retains owned nodes, framing, audio treatments, lineage, owned mark intent and referenced assets, and does not run root-wide audio capture or retain a timing record. Positive Child uses the existing complete-owner audio capture. Root-owned sounds remain excluded from both.

Zero SpliceSlice uses the explicit pre-edit child index, a fresh neutral wrapper and typed imported identities. It skips the suffix slot, suffix capture, root sound transformation, existing mark transformation and lineage reconciliation. Existing allowances restore through the unchanged generic lifecycle. The outer audio prune cannot remove records from a valid unchanged binding graph because audio validation already rejects unreferenced timings. Timings still require the new revision's allocation, but ordinal `u32::MAX` consumes no unused slot.

Interior and replacement slice preflights reject zero content before import identity counting. Existing MoveRange already rejects an empty source interval through shared preflight, so no Move implementation change was needed. No new authoring command or patch grammar was added.

## Wire policy

Core document schema remains 34; database policy is unchanged. The private optional selector field is only a read adapter: missing selector normalizes to an explicit Range using the existing `range`; its getter never exposes absence. Range serialization omits the selector and preserves the previous field order/shape. Child serialization emits `selection: {"type":"child","node":"..."}`. Explicit null, duplicate, unknown and contradictory fields reject. No JSON value rewrite or historical grammar widening was added.

## Owned files

- `crates/deadpan-core/src/edit_slice.rs`: selector, adapter, exact Child capture, selector-aware validation, zero capture branch; original audio capture extracted without semantic change.
- `crates/deadpan-core/src/edit_slice/placement.rs`: no clocks, mark transforms or lineage resets for zero seam insertion.
- `crates/deadpan-core/src/insert_time/source_splice.rs`: explicit zero interior rejection.
- `crates/deadpan-core/src/insert_time/sequence_range.rs`: explicit zero replacement rejection.
- `crates/deadpan-core/src/sound_routing.rs`: zero seam retains root bus/routes without a zero-duration route edit.
- `crates/deadpan-core/src/lib.rs`: public enum export.
- `crates/deadpan-core/tests/edited_slice.rs`: new child test module.
- `crates/deadpan-core/tests/edited_slice/placement.rs`: existing sound fixture exposed only to its sibling tests.
- `crates/deadpan-core/tests/edited_slice/child.rs`: ten focused tests.

## Meaningful tests added

1. Same-time identical-label empty siblings stay distinct; nested empty tree paste and copy-of-copy retain structure and exact inverse.
2. Every empty Sequence slot, including both ends, preserves its integer slot; maximum timing ordinal succeeds with no binding changes; wrong allocation rejects.
3. Whole positive group containing Repeat and Preserve retains one owned part and excludes adjacent empty siblings; its payload equals the corresponding whole Range after removing only the selector.
4. Empty owned framing, gain, both biased local marks, unresolved intent, absolute Sequence pins and fresh lineage aliases survive.
5. A destination with real pre-existing timing/reanchor records, routed root sound and Hold allowances remains exactly equal in all those maps after zero paste before/within/after its children.
6. Missing/wrong scopes, malformed selector fields/null/duplicates, forged owned metadata, mismatched child ID and zero Range reject.
7. Old Range wire is canonical; an explicit matching Range normalizes, while contradictory metadata rejects.
8. Zero interior/replacement/Move, short/colliding identity pools and invalid slot reject atomically.
9. Sequence under a Repeat is not ordinary capture scope; a valid maximum-depth empty tree cannot gain an extra wrapper beyond the depth limit.
10. A dormant Source mark retains its asset without capturing audio or manufacturing time.

All paste helpers assert exact inverse and full document serialization round-trip. Parent/media/store workers own independent compatibility fixtures and integration checks.

## Verification and limits

No Cargo or media/UI execution by this agent. Parent ran:

```text
rustup run 1.97.1 cargo test -p deadpan-core --test edited_slice --locked
```

Result: **31 passed, 0 failed, 0 ignored**, exit 0, 39.22 seconds including compile (tests 0.03 seconds). All ten new Child tests passed. Exact command/source manifest and raw log: `/tmp/deadpan-structural-capture-20261001/checks/core-first.json` and `core-first.log`. The test run preceded the final formatting-only pass.

Final scoped formatting used `rustup run 1.97.1 rustfmt --edition 2024 --config skip_children=true` on the nine owned changed files. `git diff --check` passed. The first standalone formatter invocation had lacked `--edition 2024`; the final pass restored workspace import order and removed those unrelated formatting changes. No functional edits followed the successful test run.

Static review corrected test constructor assumptions before handoff: the ordinary Retime purpose is `Edit`, ClipGain's constructor includes the mute-range argument, and SourceMoment's sample is an integer. No compile or runtime failures occurred in the first focused run. Production diff was reviewed, including early zero rejection, complete Child validation, private selector normalization and the exact existing-state preservation branches. No remaining concrete issue was identified in that review.

Retained scope: ordinary Sequence ownership only, zero seam paste only, no new partial-composite admission, no persistent registers, native empty-source UI or cut transaction integration in this core patch. Source history authority remains the store's full immutable recapture. Independent old-history transaction equivalence, store admission and plan/audio integration are owned by other workers and remain outside this agent's verification claim. Parent reports that the old-baseline fixture generator succeeded; that alone does not establish live replay equivalence.
