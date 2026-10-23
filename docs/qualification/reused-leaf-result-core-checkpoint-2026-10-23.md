# Reused leaf result Core checkpoint (2026-10-23)

## Result

The uncommitted Core candidate passed the corrected differential test and the complete Core suite. The final focused receipt has four steps, all exit 0; the selected test passed once. The all-Core archive passed 1,514 tests across 69 binaries with zero skipped. Source, input, and executable guards passed. The suite ran against base commit `1e67fdb27bbb3a59d282e5240c65b860b4090b76` with the one-site helper change and new differential test.

The production helper reuses the validated document returned by `apply_with_result` instead of applying the generated forward patch a second time. The initial focused test failed with `OutsideMapping` while resolving a raw primary mark coordinate after an interior split. A logical mark can retain multiple bindings, so its primary coordinate alone can lie outside one clipped interval. The production helper did not change for the correction.

The corrected fixture resolves the mark through a revision-aware `SelectionRequest`, `BoundarySelector::Mark`, and `NamedMarkTarget`. It still requires exact frame 12, mark and binding equality, equivalent legacy leaf patches, authoritative compound replay, inverse restoration, and unchanged input. The focused test and complete Core suite then passed.

## Scope and qualification status

This checkpoint records Core evidence only. It does not include later UI, strict, media, native, or final timing phases, and it does not qualify the production candidate. Root has added the same one-line production change with a differential test; broader qualification is a separate record. The earlier private five-pair timing prototype remains diagnostic and separate from this Core result. There is no schema change; Core 58 and SQLite 87 remain unchanged.

Three read-only reviews reported no defects: the helper review, differential-test review and post-failure fixture-correction review. The static test review preceded its runtime assertion failure; neither review nor prototype timing is counted as a passing test. All review records remain separate from executed results.

The initial failed run selected one test and failed; its binary SHA-256 (`aeb1018287dd22a8effef271618a2694c3721375ef5695f2d8313044954d6ea1`) is retained as metadata, without retaining the executable. Both the failed and corrected test sources are included with their source maps. A separate historical test draft is also retained and labeled uncompiled; it is not counted as test evidence.

## Evidence

The bounded package is [2026-10-23-reused-leaf-result-core-checkpoint](../../tools/media-qualification/evidence/2026-10-23-reused-leaf-result-core-checkpoint/). It contains focused and all-Core receipts, logs, source/input/binary manifests, the helper and test source snapshots, failure metadata, historical draft, review records, and report summaries. It excludes executables, Cargo archives and targets, and generated project packages.

The independently re-read inventory lists 86 files totaling 1,672,053 bytes. `inventory.json` SHA-256 is `fd9fc7c60526605df1827a1a7b9e91074ef267140c98711263bed41c0f953150`.
