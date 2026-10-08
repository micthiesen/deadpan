# Retained extension input evidence

See the [qualification record](../../../../docs/qualification/extension-evidence-2026-10-08.md)
for the implemented contract, measurements and remaining work.

- `source.json` names the base commit and exact changed Rust/Python source bytes.
- `environment.json` records this Mac, toolchain and FFmpeg prefix.
- `extension-evidence-gate.log.gz` is the complete repository gate output.
  The gate covers formatting, strict workspace and UI Clippy, the workspace
  tests with the synthetic-worker feature, UI-harness tests and doctests.
- `extension-evidence-focused.log.gz` records the earlier 97-test pass. It
  predates the final unavailable-anchor target-record regression.
- `extension-evidence-clippy.log.gz` is an earlier passing Clippy run. The full
  gate covers the final source after strict parsing and target validation fixes.
- `worker-inventory.json` records an isolated import from the exact seven
  runtime distribution files, including the new extension reader. It does not
  establish a new application bundle build or real model run.
- `results.json` extracts Rust summaries and records the worker agent's final
  111-test Python result and independent review outcome. That Python tool result
  had no saved raw log; the suite was not repeated solely to create one.
- `manifest.json` hashes every retained evidence file except itself.

No archived-source comparison, new real model inference, release bundle,
live native UI, export or output quality qualification was run for this milestone.
Skipped tests remain skipped, not passes. Extension Ready and acceptance remain
disabled; this evidence qualifies retained inputs only.
