# Independent review: Source endpoint phase primitive

Scope: `/tmp/deadpan-source-roll-20261001/staged-source-endpoint/source-endpoint.patch` at SHA `379eced71e151a6c10489a363e2661ea9eabf99c2786e3ced1f983dae69f0795`, README and manifests, `source-endpoint-phase-design.md`, current binding/projection/lifecycle seams, and the staged core/plan/audio/legacy tests. Read-only; no Cargo/native execution.

## Findings

1. **Major, compile blocker** — `crates/deadpan-core/tests/audio_bindings/gaps.rs:747` has a one-line `AudioReanchorStep { placement, window }` literal inside `closed_gap_origin!` that the patch does not update. The patch adds a required Rust field to `AudioReanchorStep`; add `anchor: Default::default()` or use `AudioReanchorStep::for_allocation`. An exhaustive `rg` of Rust literals found this as the only missed existing initializer.

2. No other actionable correctness finding in the endpoint primitive scope.

The endpoint resolver keeps allocation geometry separate from audible Source support: the frozen projection clips structural allocation to the resolved occurrence/definition and intrinsic clock support, while Source audio presence/dormant placement is not used to select Start/End. It rejects an empty endpoint allocation rather than taking AllocationEntry's historical skip path. End selects the exact closed `range.end`, Start selects `range.start`, then subtracts `reference_local_offset` once. The existing chronological resolver retains the prior resume and each placement's grid, and signed `sample_boundary` conversion remains checked. Prefix rebasing updates placement offsets and leaves the symbolic side and historical layouts untouched.

Current validation requires a Node/Source historical alias with no gap argument or window, and `validate_for` requires a current Source owner. The custom selector deserializer is closed; v36's streaming legacy Step grammar rejects any explicit new `anchor`, and its `supports` gate prevents lossy projection. The adapter tests check both transaction directions. Split and slice tests exercise selector copying and alias renaming, birth tests exercise scope-specific endpoint resolution, and the plan/audio cases include signed virtual anchors, offset arithmetic, dormant/absent audio, exact phase controls, and bounded queries.

The endpoint tests and PCM witnesses in this scratch patch remain uncompiled and unexecuted per its README. I did not run them.
