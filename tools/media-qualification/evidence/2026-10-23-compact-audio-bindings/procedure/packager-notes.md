# Final evidence packager notes

The packager is at `retain_final_evidence.py`. It is deliberately unexecuted.
It packages only after root's qualification and media/performance drivers have
completed. Existing destination files are accepted only when their bytes match;
different bytes stop the script without overwriting them.

The prerequisites now use the exact result fields and paths from the owner's
correction:

- Each final nextest run needs its `*-result.json` with exact exit, test and
  binary counts plus `sources_inputs_executables_unchanged: true`, and its
  separate run receipt with `exit: 0`.
- The initial final-qualification receipt is retained without a pass gate so
  the intentional earlier failed broad phase remains in evidence. The second
  qualification receipt must have exactly seven successful rows.
- Strict check/format, UI lint, debug/release media, and release-native need
  their exact successful receipts: `candidate-strict-check-receipt.json`,
  `candidate-strict-format-receipt.json`, `ui-lint-receipt.json`,
  `debug-media-build-receipt.json`, `debug-media-neutral-receipt.json`,
  `release-media-build-receipt.json`, `release-media-neutral-receipt.json`,
  `release-media-aligned-receipt.json`, `release-native-build-receipt.json`,
  and `release-native-performance-receipt.json`.
- Portable qualification reads `owner-portable-check/receipt.json` and checks
  two passing cases, with unchanged package paths and bytes.
- Controlled performance checks `summary.json` for five trials in each
  baseline/candidate first/second group, `trials.json` for five trials per
  mode, and exactly twenty successful commands. Performance-validation checks
  five cases with exit zero, unchanged packages, and `response.valid: true`.

The packager retains eligible top-level receipts/logs, each phase's exact Rust
overlay files with `.rs.txt` suffixes, old-codec witness and initial-toolchain
text/log/receipt files, controlled performance raw text and JSON, performance
validation, release-native performance, and broad-final evidence. It excludes
binary outputs, archives, top-level `*-source`, `*-extracted`, and `*-inputs`
trees, generated benchmark packages, and live output/self-capture. Inventory
creation is the final step and includes payloads already retained by the media
driver.
