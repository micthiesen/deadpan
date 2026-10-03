# Exact sibling-forest capture

Scratch-only implementation based on the parent's reported clean HEAD
`4deae8e5`. All five owned file baselines still matched the checkout at packaging.
No shared source, build output or Git state was changed.

## API and behavior

```rust
SliceCaptureSelection::Children { first: NodeId, last: NodeId }

pub struct SequenceChildrenPlan {
    pub first: usize,
    pub end: usize, // exclusive
    pub range: FrameRange,
}

impl ProjectDocument {
    pub fn sequence_children(
        &self,
        parent: &NodeId,
        first: &NodeId,
        last: &NodeId,
    ) -> Result<SequenceChildrenPlan, EditError>;
}
```

The public query validates ordinary Sequence ancestry through the existing
`source_splice_boundary` boundary. It resolves both exact direct-child identities
and rejects missing, nonchild and reversed endpoints. Equal endpoints select
one child. An inclusive nonempty structural span can have zero frame duration.
Checked addition computes the absolute range without expanding Repeat plays.

Capture includes every selected sibling's complete subtree in slot order,
including empty endpoints and interior empty children. Existing mark capture,
media retention, audio binding capture and fresh paste renaming remain shared.
The unselected parent contributes none of its owned effects or marks. A
zero-duration forest contains only Sequence nodes and default audio binding
state; no zero-root forest is admitted.

Children has the strict tagged wire shape
`{"type":"children","first":"a","last":"c"}`. Existing Range and Child
serialization is unchanged. Structural admission checks the selector's endpoints
against the exact first/last whole parts, distinct roots, complete mappings and
contiguous absolute output. The historical `validate_capture` path recaptures
the selector, so missing or reordered interior empty siblings fail even when the
remaining standalone forest is structurally valid at the same frame boundary.

This implements structural capture only. It adds no `ib`/`ab` attachment policy,
text-object grammar or new independent sound ownership semantics.

## Files

- `src/edit_slice.rs`: new selector branch, slot-based capture, wire validation.
- `src/edit_slice/children.rs`: public preflight query/result and forest checks.
- `src/lib.rs`: result reexport only.
- `tests/edited_slice.rs`: new test module declaration only.
- `tests/edited_slice/children.rs`: eight focused integration tests.

## Test coverage and checks

The new tests cover exact nested scope offsets; leading/trailing empty children;
all-empty forests; same-endpoint equivalence with Child; excluded neighboring
empties; invalid identity/order/ancestry; parent versus selected ownership;
empty-boundary and unresolved marks; strict selector/part corruption; historical
omission and reordering of empty interiors; fresh repeated paste identities;
identity pool failures; zero-time paste with the maximum unused clock ordinal;
and complete inverse equality through the existing shared edit helper.

Suggested parent check:
`cargo test --locked -p deadpan-core --test edited_slice`.
Parent-owned mutation, store, native and PCM checks remain separate qualification.

Completed here: scratch-only rustfmt, AST syntax scan, complete production/test
source review, whitespace check, exact before/after hashes and patch dry-run.
No Cargo, tests, Clippy, native app or Git command was run.

The new enum variant requires corresponding exhaustive match handling outside
this patch. The parent reports that Group/Repeat, semantic, Compound, store and
host matching have been integrated independently. Do not replace those files
with these scratch snapshots. Current limits and paste admission are unchanged;
no schema constant is changed. Real media equivalence is not established by
these unrun structural tests.

## Artifacts

- `children-capture.patch`: git-apply-compatible patch.
- `manifest.json`: all exact before/after hashes and packaging state.
- `before/`, `after/`: exact owned source snapshots.

Patch SHA-256:
`ceb3fc9580a69b4542f3427d7ac85a86dbd8e4a3d99da1734f499a3d0f5ff05f`
