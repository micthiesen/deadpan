# Semantic selection refresh reuse qualification (2026-10-23)

**Status: INCOMPLETE.** Core, broad, UI, strict, and UI-lint checks have completed successfully. Debug/release media verification, release-native workflow qualification, portable-copy validation, and the final timing series are **PENDING**. No production qualification or performance conclusion is complete yet.

## Change under qualification

The candidate is based on `ae5eedd065b851242917b38a428ee40a3588978d` and changes two Rust paths: `crates/deadpan-core/src/semantic/planner/selection.rs` and the new `crates/deadpan-core/tests/semantic_refresh.rs`. The production helper reuses the immutable staged document's already-proved durations to refresh selection bounds and child indexes when local command work is active and the document has no derived clocks. Other paths keep the existing refresh behavior. Core schema 58 and SQLite schema 87 are unchanged.

The frozen source map contains 2,051 Rust paths, versus 2,050 in the qualified control. The existing selection file is changed; `semantic_refresh.rs` is new and has no baseline file. The exact tested source hashes are recorded in `adoption-source-identities.json`; the current source-map and post-commit guard remain **PENDING**.

The new Core tests exercise wide-bound repeated pauses, nested scopes with zero groups and marks, provider/allocator failure ordering and input preservation, and nested follow-speed timing fallback. All four passed in the completed Core run.

## Completed checks

| Check | Result |
|---|---|
| All Core tests | 1,526 passed across 70 binaries, zero skipped |
| Broader selected tests | 3,506 passed, 740 skipped |
| UI-feature tests | 1,296 passed, 11 skipped |
| Core strict check | Passed |
| Rust formatting check | Passed |
| UI Clippy with warnings denied | Passed |
| Source/input/executable guards for completed phases | Passed, per phase receipts |

The completed logs show no `LEAK` annotations. Skips in the broad and UI runs are reported as recorded; they are not counted as passes.

## Prototype history, separate from production qualification

An earlier private prototype integration-test compile failed with Rust error E0624 because the test attempted to call crate-private `EditError::new`. No test ran and that prototype failure did not change production source. The fixture was corrected to construct the same injected `InvalidCommand` error from its public fields; a review records that the production source and assertions were unchanged. This is prototype history, not a failure in the production qualification runs above. The production Core suite includes the corrected new tests and passes them.

## Remaining qualification checks

- **PENDING:** debug media fixture and decoder verification.
- **PENDING:** release neutral and aligned media fixtures, including offsets and canonical PCM/history comparisons.
- **PENDING:** release-native app build and six native workflow reports, with the separate Kestrel check counts and any warnings reported from the actual reports.
- **PENDING:** both owner-portable copies, including their exact file counts and unchanged validate/doctor results.
- **PENDING:** all post-qualification commands, final source/input/executable checks, and exact candidate app identity.
- **PENDING:** five alternating fresh-process control/candidate pause pairs, raw reports, input maps, and summary verification.
- **PENDING:** final evidence package, inventory count/bytes/hash, and independent reread of retained payloads.

No media, native, portable, or timing result is asserted in this draft. No median, percentage change, percentile, target pass, or timing attribution is available yet. The 100 ms fallback and 50 ms ordinary-edit targets remain open unless separately closed by the project owner after the complete qualification.

## Timing comparison

**PENDING final timing series.** The planned diagnostic uses five alternating control/candidate pairs. It does not establish a percentile or close a latency target. Control app SHA-256 is `2761f275c70d471b202ffdf2faa9e047724a169c9607775ff4b4ee6208942608`; the candidate release app identity is **PENDING**. Do not infer a performance improvement from the earlier private prototype or combine its measurements with production qualification.

## Evidence

**PENDING final evidence package and inventory.** The packaging script is only a scratch draft and has not been executed. The completed phase receipts and logs are in `/tmp/deadpan-semantic-refresh-duration-reuse-qualification-2026-10-23/`; the private prototype source/review records are in `/tmp/deadpan-semantic-refresh-duration-reuse-2026-10-23/`. Final repository evidence path, retained-file count, total bytes, and inventory SHA-256 are **PENDING**.
