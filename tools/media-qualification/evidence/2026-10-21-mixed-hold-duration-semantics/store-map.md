# Store generation integration

Read-only source audit by review_jobs. No implementation or runtime claims.

`crates/deadpan-store/src/generation_scope/transitions.rs`: classify the future
`SetScopedHoldDuration` wrapper in `may_isolate`. Existing compound recursion
then derives the actual `ValidatedScopedIsolation` proof for stored scopes,
requests, intents and preparations through command, preview and history paths.

`crates/deadpan-store/src/generation_preparations/transitions.rs`: extend
`has_birth` and `duration_node`/`leaf_births` to retain the supplied complete
mixed `ScopedNodeTarget`, map it through the executed proof, then resolve that
target against the result document's `CanonicalTargets`. Do not reconstruct an
all-Play address from one presentation. Preserve ordinary duration behavior.
`command_births`, `history_births` and compound replay already derive each leaf.

Accepted-provider extension creates preparation only beyond artifact coverage
and when the resulting authored provider equals its fallback. Name the actual
isolated Hold and mixed scope. Shortening and extension within accepted support
retain the provider and create no birth. Pending requests/preparations still map
through the actual proof and remain revision-bound.

Tests: both mixed branch orders with generated extension; exact mapped birth;
shortening and within-support extension without birth; pending request and
preparation mapping through Undo/Redo; compound leaf replay using the same proof.
Use `generation_preparations/transitions/canonical.rs` fixtures as a pattern.
