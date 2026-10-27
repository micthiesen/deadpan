# Checked sound placement history, 2026-10-27

Audio's route builder and Plan's afterbirth validator previously repeated the
same Core placement projections. A sealed Plan cursor now owns the existing
scalar placement vector and issues finished evidence after every clock step
and the current allocation check. Route construction borrows its rows;
afterbirth consumes the proof and rechecks the complete journal, binding,
alias, recipes and context, every route window/grid/count, birth/disappearance
and Split continuation. The manual-route API still recomputes placements.

A charged six-unit initial step bounds allocation by the canonical journal.
Each following advance performs one existing instance remap, context remap,
historical placement or current placement query. Audio checks cancellation,
deadline and aggregate work around every advance. Error poisons the cursor;
partial histories cannot finish. The 65,536-unit query cap, afterbirth's supplied
call-wide cap and 16,777,216-unit read cap stay intact.

Four identity checks suffice for the proof's exact immutable original/current
objects, complete scope slice and context. Other handles receive the full
semantic comparison, charging those four attempts plus every comparison.
Cloned occurrences and fresh equivalent scope handles remain accepted.
Afterbirth's canonical journal and other authority checks remain independent.
Certified-Split proof reads still project exact context clocks when required,
charging their queries and scalar captures. No context-clock vector is added to
the rows. No cache, dependency, permission, authored format, media admission or
plan-owning Arc changes.

Six Plan tests cover old-query row oracles, per-step exact/one-short budgets,
poisoning and incomplete finish, oversized slices before allocation, separate
context steps, skipped Repeat-birth instances, storage and both authority paths.
An Audio test checks cancellation after remap and aggregate exhaustion between
steps. The public fixture compares complete and partitioned route queries,
exact/one-short afterbirth work, equivalent proof minting/consumption and 14
rejection cases.

The literal 4/9/15 fixture retains sample support `24024..30431`. Manual
afterbirth costs 117 units; the initial fieldwise-only proof cost 108. The exact
borrow version costs 78. Including the cursor's extra six-unit allocation guard
removes 33 actual units. This is query-work evidence, not a time benchmark.

Rows remain 144 bytes, with exactly `clocks.len()+1` capacity. The finished proof
header is 64 bytes, the cursor 208, its phase 88 and borrowed authorities 40.
These are measured compiler/platform sizes, not an ABI promise. One bounded row
vector replaces the old vector, alongside the existing route and Split vectors.
The header stores borrowed references. Construction-peak savings are unmeasured.

## Verification

Apple M5 Max, macOS 26.5.2, Rust 1.97.1, locked dependencies and qualified FFmpeg.
Audio uses optimization level 2 with debug assertions and overflow checks.
Independent review finds no concrete defect in either the cursor or final
exact-borrow discriminator.

| Final check | Result |
| --- | --- |
| Current Plan | 540 pass across 28 executables; zero ignored. |
| Current Audio library/definition | 199/171 pass; zero ignored. |
| Current existing real-WAV stages | 203 pass; seven unfinished Split tests run separately; zero ignored. |
| Isolated ten-file patch | Builds; 527 Plan, 185 Audio library and eight historical birth/processing tests pass; zero ignored. |
| Isolated Core carryover | 775 pass; its executable is byte-identical after the final Plan-only discriminator change. |
| Strict final check | `cargo xtask check --no-test` passes formatting and strict all-target Core/Plan/Audio/Store lint. |
| Unchanged real-WAV Split matrix | Four pass, three work-limit failures; 359.49 seconds. |

The first definition run had 170 passes and one new fixture failure: shifting
the alternate recipe by one sample exceeded its full owner before the rejection
check. Shortening its selected interval keeps that recipe valid; the rejection
assertion remains. A redundant test-only format borrow also failed initial
strict lint. Both failures are retained. Final full definition/stage suites and
strict lint pass after those corrections and the exact-borrow optimization.
The intermediate fieldwise-only matrix remained four pass/three fail at
396.63 seconds. Neither matrix is a calibrated performance comparison.

Current runs guard 1,395 inputs; isolated runs guard 1,167. Source and executed
binary hashes remain stable. Raw records and pinned executables are in
`/tmp/deadpan-placement-token-2026-10-27`. The
[evidence inventory](../../tools/media-qualification/evidence/2026-10-27-placement-history/inventory.json)
retains the exact independent patch, commands, outputs, original failures,
intermediate/final results, identities and reviews, and references the preceding
full runtime reconstruction. All 474 payloads verify stored and expanded hashes:
18,791,731 stored bytes and 74,623,041 expanded bytes. Inventory SHA-256:
`eca10101cd2e52057a24268f725d5700877dce9ece008d1407a35131279930ac`.

Remaining failures are Source/Sequence `cut-three`, limited `0+511`, and selected
`prefix-two`, limited `0+4096`, at `plan work per read`; nested selected at the
same prefix/window reaches `AudioQueryLimit("sound plan binding work")`.
Processed root-sound refinement still passes its complete PCM comparison.
General sound-bearing Split remains unqualified. No requirement, criterion or
delivery gate closes; the full product goal stays active.
