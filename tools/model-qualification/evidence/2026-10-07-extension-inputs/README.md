# Durable extension-input evidence

Started 2026-10-07; final verification completed 2026-10-08.
See the [qualification record](../../../../docs/qualification/extension-inputs-2026-10-07.md).

## Source and environment

`source-before-fixture-corrections.json` identifies the source tested by the
full workspace run. `source.json` identifies the final source, and
`post-gate-source-changes.json` lists exactly six changed test files. Their
production dependencies did not change. The base commit plus per-file SHA-256
records covers tracked and new Rust files without a second repository archive.
`environment.json` records the reference Mac and pinned native prefix.

`post-gate-binaries.json` hashes the executables in the final Cargo workspace
and UI inventories, plus the built app, CLI and workers. Inventory logs contain
Cargo's exact paths; no filesystem glob chooses a test binary. These are debug
verification builds, not a release or packaged-model qualification.

## Gate results

The original `cargo xtask gate` did not pass as one invocation. Its first run
stopped at two Clippy errors. The corrected run passed both strict lint
configurations and completed the workspace tests: 5,245 passed, six failed and
10 were skipped. The failures were four old schema assertions, an old SQL
column count and a Bridge fulfilment fixture placed at an extension-only edge.

Only those test fixtures changed afterward. The complete six affected targets
passed all 147 tests under the unchanged workspace feature graph. The remaining
gate stages then passed: formatting, final workspace Clippy, 1,071 UI-harness
tests (two skipped) and both doctests. `continuation.json` records every command
and exit. Workspace and UI executable inventories were taken after their tests.
Together these runs cover all 5,251 current workspace tests and the required UI
configuration. Skips remain explicit and are not implementation evidence.

Two nextest pipe-closure observations are retained: the full run marked
`coverage_preserves_sparse_repeat_play_and_implicit_gap_seams`, and the affected
targets run marked `headless_migration_and_plan_inspection_are_explicit_and_read_only`.
Both pass in one serial diagnostic run without another leak report. No timeout,
assertion or teardown rule was weakened, and their cause is not established.

Earlier filtered compile/test failures and their corrected runs remain as
compressed logs. `results.json` extracts their test summaries and errors;
`manifest.json` hashes every retained evidence file except itself.

## Independent review and limits

Separate reviews covered exact measured support, fallback witnesses, replacement
dependencies and durable history. The lifecycle review found older-control replay
and batch-limit-as-input-identity defects. The final code fixes both and adds
regressions. Another review tightened historical request-version authority.
The final review reported no remaining actionable finding in those scopes.

The milestone verifies persisted input authority, relevance, replacement and
history. Protocol 3 completion, Ready, output quality admission and acceptance
remain refused. No new real-model generation, native visual replay, release
bundle, export or performance measurement was performed here.
