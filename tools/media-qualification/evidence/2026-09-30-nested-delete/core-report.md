# Nested Partition DeleteRange: core handoff

## Change

`ProjectDocument::range_deletion` now uses a deletion-specific shared preflight that selects the existing recursive unity Partition endpoint mode. Public APIs, identity ordering/count semantics and the DeleteRange reducer are unchanged. Split still retains complete owner subtrees, captures clocks before splitting, captures the suffix before removal, and returns one reversible patch.

Only these owned files changed:

- `crates/deadpan-core/src/insert_time/delete_range.rs`: call `preflight_deletion`.
- `crates/deadpan-core/src/insert_time/sequence_range.rs`: add that narrow wrapper using existing `EndpointMode::SliceSplit`.
- `crates/deadpan-core/tests/insert_time/delete_range.rs`: declare focused child module.
- `crates/deadpan-core/tests/insert_time/delete_range/nested_partitions.rs`: nine regression tests.

The Source replacement/general preflight, standalone Split, InsertTime and Source insertion admission paths were not changed. Partial Repeat/general Retime/Generated Hold endpoints remain rejected. Whole composites remain deletable. Recursive traversal and checked mapping accumulation use the existing bounded `slice_physical` implementation; depth, temporary node, identity, timing, mark and work limits still apply.

## Tests added

1. Two and three untreated nested windows, both cuts within one Source child, with exact pools of six/eight IDs, unchanged full physical recipe and intermediate owner metadata, retained original lattice, one suffix reanchor and exact inverse/serialization through the existing `edit` helper. The selected endpoints are also independently admitted by CapturedEditSlice.
2. Different two/three-window endpoints around a whole removed Hold, requiring exactly seven split IDs; Source and ordinary Hold retain their independent complete contexts and the downstream suffix moves once.
3. Freeze with a framed outer window and gain-treated middle owner; eleven exact split IDs preserve two complete copies of each owner and original recipe clocks.
4. Empty/out-of-bounds/wrong-scope ranges, insufficient/colliding IDs and exhausted second timing ordinal fail atomically.
5. Source replacement, Source interior insertion and pause insertion retain their old rejection, while standalone Split retains its existing successful transparent-context behavior.
6. Partial Repeat and general Preserve Retime behind unity windows remain rejected; aligned whole removal succeeds.
7. Root Local start/Left and end/Right marks meet at the join, interior intent becomes unresolved, suffix local coordinates shift, absolute pins and pre-existing unresolved intent remain exact.
8. A valid accepted Generated provider behind three windows rejects partial deletion but permits the whole owned window to be removed.
9. An independent root sound retains its recipe and receives exactly one Delete route over the selected original interval, with the original root grid.

Independent picture/PCM tests belong to the media worker. Core tests do not claim decoded media equivalence.

## Checks and execution ownership

Owned checks completed successfully, exit 0:

```text
rustup run 1.97.1 rustfmt --edition 2024 --config skip_children=true crates/deadpan-core/src/insert_time/delete_range.rs crates/deadpan-core/src/insert_time/sequence_range.rs crates/deadpan-core/tests/insert_time/delete_range.rs crates/deadpan-core/tests/insert_time/delete_range/nested_partitions.rs
git diff --check -- crates/deadpan-core/src/insert_time/delete_range.rs crates/deadpan-core/src/insert_time/sequence_range.rs crates/deadpan-core/tests/insert_time/delete_range.rs crates/deadpan-core/tests/insert_time/delete_range/nested_partitions.rs
```

No Cargo, subprocess/media worker, native UI or Git history operations run by this worker. Root owns execution and has been told the source is compile-ready. Suggested focused command:

```text
rustup run 1.97.1 cargo test -p deadpan-core --test insert_time delete_range --locked
```

Broader existing admission regressions can use the complete `--test insert_time` target when root schedules it. Strict Clippy remains root-owned.

## Status / risks

Production scope is a narrow admission extension onto existing context-preserving splitting. Root's initial focused run compiled and passed 17 tests; one new root-sound fixture failed during document construction before deletion. The fixture used 100 mapping frames for a 100/30-second source span at 30000/1001 project fps, contrary to the required exact natural duration of 100000/1001 frames. Corrected the fixture to derive that duration through `SourceAudioMapping::natural_rate`, as existing root-sound fixtures do. Production code and expected deletion behavior are unchanged. Log: `/tmp/deadpan-nested-delete-20260930/core-delete-initial.log`. Corrected source is formatted and whitespace-checked; rerun remains root-owned.

Cut-to-register, structural zero-duration capture/paste and native changes are not part of this increment. No migrations or compatibility code changes were introduced.
