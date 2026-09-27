# Compact audio reanchor verification

This record belongs to [the qualification](../../../../docs/qualification/audio-reanchors-2026-09-26.md)
and the local changes above base `c03a5edde5f28d27074745eb15711cb28b1f2e50`.
The earlier captured-framing, Original-moment and Repeat-gap clock changes remain
present. Git metadata is read-only in this session; no commit or push occurred.

`verification.json` and `gate-1/report.json` record the required checks:

- Formatting, all-target Clippy with warnings denied, the locked workspace build
  and CLI doctor passed.
- Complete workspace tests: 1,463 passed, one failed, zero ignored.
- The failure was fixture creation at `deadpan-jobs/tests/artifact.rs:200`.
  The sandbox denied `UnixListener::bind` with `PermissionDenied`, OS error 1.
  Its socket rejection assertion was not exercised. `--no-fail-fast` continued
  through all other suites and documentation tests. No test was suppressed.
- All 446 source/configuration hashes remained unchanged across the run.

The failed test keeps the overall gate failed. These checks do not qualify live
GUI behavior, GPU output, device audio, listening or export. Current schemas are
core 21/database 27; audio-context schema 2 is unchanged.

Focused final results comprise 242 cases: 129 core, 91 store, six decoded-PCM and
16 CLI project-command tests. The full workspace run repeats those cases.
`focused-1` records core/store checks. `pcm-cli-1` and `pcm-cli-2` preserve failed
test oracles; `pcm-cli-3` records the corrected PCM and CLI checks. The Preserve
oracle needed the Source host's actual filter support `[100,218)`, and its full
384-output comparison needed two requests within the 256-sample limit. Production
timing and preparation limits were not relaxed. `verification.json` also records
the initial unlogged missing test-module path and unused-import correction.

The actual preserved core-20/database-26 CLI produced 27 fixture revisions and
15 history entries, including selected audio, old InsertTime phase terms and
pending redo. Its retained command log is `old-binary/commands.json`; the
[fixture provenance](../../../../crates/deadpan-store/tests/fixtures/v26-audio-binding-history.provenance.json)
records binary and SQL hashes. No binary is included in this evidence directory.

`review.json` records independent general, exact-timing/PCM and migration reviews.
All three returned no findings. The timing review also verified the corrected
Preserve source-support oracle and full original-output comparison.

The existing [ImageGen interface targets](../../../../docs/design/README.md) remain
unchanged. This increment changes no native UI, startup or lifecycle behavior.
Authored gap bindings, complete movement/raw-recipe lifecycle, atomic selected-
moment reuse and its native Visual/register workflow remain open.

`sha256.json` covers every retained evidence file except itself. Runner and
retention scripts preserve the commands and paths used on this host. They are
development evidence, not end-user runtime requirements.
