# Atomic MoveRange implementation review

## Result

No independent actionable defect found in the reviewed uncommitted changes against `a079a46`. The earlier design finding is corrected: MoveRange detaches and restores the exact root sound recipes and routes without adding a ripple operation.

Root separately requested that stale-source `RevisionConflict` include `current_revision`. That correction was in flight at handoff and is not duplicated here as a review finding.

## Scope inspected

- Complete new `crates/deadpan-core/src/move_range.rs`: `Plan::new`, joint `Cuts`, Split refinement and destination fence handling in `apply`, `validate_pools`, `reanchor` and `island_owners`.
- Core command dispatch and validation boundaries; `audio_binding_lifecycle::has_unbound_recipes` against the existing capture traversal; exposed Split budget/pool helpers; `RootSoundEditCapture::prepare`; sound command admission; explicit legacy v29-v32 rejection matches.
- Existing Split, mark, allowance, audio-lineage, binding-resolution and owner-clock code where the new command relies on those contracts.
- All eight new core tests, plus new plan, decoded-audio, store, source-registration, accepted-generation and CLI MoveRange tests present during review.

The inspected paths account for shared-target three-cut budgets and retained fences; both ancestor relationships; selected-descendant rejection; explicit empty slots; revision-only no-ops; complete supplied identity pools and retained timing aliases; checked consecutive timing ordinals; disjoint physical-owner reanchors from one unchanged-time layout; live Sequence ancestors; generic OutsideHost mark handling; and Split-remapped grants followed by exact root-bus restoration.

## Test coverage inspected

The tests assert behavior beyond the implementation's own preflight output: hand-authored picture permutations and owner clocks, independent fractional-rate sample-entry oracles, both continuing and exhausted extra-sample support, prior binding history, whole Repeat/Preserve ownership, unchanged routed and unrouted root sounds, live Hold gates, and transactional rollback/reopen/fresh-revision Undo/Redo. Core tests also assert exact identity counts, reanchor counts, empty ordering, no-op clock absence, invalid pools and ordinal overflow.

This inspection does not claim those tests passed. Root owns test execution and reported the core check and four store tests passing during this review.

## Verification and limits

Used `ripwire --situ`, Git diff/status including untracked files, and direct source inspection. `git diff --check a079a46` passed at the review point. No Cargo, test execution, subprocess media workers or UI actions were performed. No repository file was edited. Only this scratch report was written.

The checkout was shared and test files were still being finalized. This report covers the inspected production and tests, not later edits or a completed project gate. Native Move controls and preview integration are explicitly outside this core/headless milestone.
