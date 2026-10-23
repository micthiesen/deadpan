# Standalone Core refutation tests: source-only handoff

Owned source file only: `source/crates/deadpan-core/tests/semantic_leaf_replay.rs`.
No production file, main checkout, Cargo configuration or other worker's file was edited.

Twelve new `one_leaf_replay_*` tests:

1. Exact interior result, full visitor before/after/request/step/value bytes, aggregate transaction bytes, forward/inverse restoration and same-value JSON reconstruction.
2. Same-project/revision label and mark changes; swapped witnesses from different entries and different requests.
3. Public `SemanticPlan.document` and `.request` mutation cannot change the private candidate's original binding or retarget it to a changed request.
4. Changed Hold duration/picture context, split pool, timing, leaf revision and outer revision compare exact fallback results/refusals.
5. Wrong project/stale revision, leaf/outer identity overlap and split identity collision preserve exact original errors and zero visitors.
6. Host visitor refusal remains exact, reached once, and prevents a returned outcome.
7. Normal-created witness under both reference modes; candidate creation is absent under each mode; ordinary behavior after each scope remains unchanged. The bound nested fixture must actually produce different compact/reference representation bytes.
8. Multiple authored leaves, Yank before/after the pause, Cut, macro inputs, AI pause, Repeat occurrence pause, derived result and later derived-entry pause are ineligible.
9. ExactRatio `-0`/`7` entry and captured-framing request spellings normalize to the same typed values and compact bytes; IEEE ratio/float integer ingress remains refused.
10. Numeric-looking `-0.0`/`+0.0` labels are distinct strings and changed label bytes survive fallback.
11. Representative framed/effected/marked/bound-audio entry reconstructs from explicit default binding fields with identical current values/wire, then replays identically.
12. Public transaction step-count/duplicate identity and forbidden snapshot-leaf constructors retain their existing refusals.

The file uses the agreed APIs: `SemanticPlan::one_leaf_replay()` and `replay_compound_with_one_leaf(document, request, visitor, candidate)`. It compares the new `None` wrapper with the legacy `replay_compound` as well as the candidate path. All helpers are private. It manufactures no witness and adds no mutable or diagnostic production hooks.

Source review completed; the file remains uncompiled, unformatted and unexecuted. No Cargo, formatter, test, app/runtime, benchmark, Git or parser execution occurred. The few shell calls only read source. Parent owns the source hash map, snapshots, full audit, formatting and all verification.

Unexecuted fixture expectations:

- The pre-bound nested ordinary Hold fixture is intended to expose compact versus complete timing storage; its explicit byte inequality assertion must be established by the first focused run.
- The two inserted Hold recipes include captured Background framing with exact zero geometry, which inspected `validate_hold` permits.
- The later derived-entry pause uses a distinct allocation namespace to avoid accidental outer/leaf identity reuse.
- Geometry float refusal expects the existing `InvalidJson` / `expected u32` vocabulary; GainDb's refusal is compared against its actual i32 ingress codec.
- Public correctness parity cannot prove that an optimization was hit rather than conservatively missed. The opaque candidate's presence is checked; parent private tests/source review and later diagnostic execution establish the hit implementation.

Budget scope: the eligible one-leaf cumulative staged512MiB refusal cannot be reached when its four mandatory traversals each cap64MiB. No fake production/test ceiling was introduced. Parent retains the original-charge source audit and private canonical/provisional serialization and TLS tests, plus focused Store authority/atomicity coverage.
