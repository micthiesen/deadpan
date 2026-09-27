# Repeat-gap binding verification

This evidence belongs to [the qualification](../../../../docs/qualification/gap-bindings-2026-09-26.md)
for core 22/database 28. Prior dirty increments remain present above base
`c03a5edde5f28d27074745eb15711cb28b1f2e50`, alongside a separate native UI
harness. This session did not commit or push because Git metadata is read-only.

`verification.json` records scope, corrections, command results and limitations.
`gate-5` is the complete workspace run:

- All-target Clippy with warnings denied, the locked build and CLI doctor passed.
- Workspace tests passed 1,501 cases, failed four and ignored none.
- One failure is the sandbox denying Unix socket fixture creation at
  `deadpan-jobs/tests/artifact.rs:200`, before its rejection assertion can run.
- Three playback tests exceeded their existing ten-second worker wait.
  `playback-rerun` records an unchanged package retry with backtraces: those three
  passed, but the intentional-panic case timed out. It compiled a different
  feature-unified binary. `playback-exact` then ran the exact workspace artifact
  with original settings and default test parallelism: all 11 passed. Its report
  records the binary hash. No timeout or assertion was changed.
- The initial format check saw concurrent app edits. `post-format` records a
  later passing format check. Gate attempts 1 through 4 preserve earlier
  formatting/compilation failures and their corrections.

The overall workspace gate remains failed. No failure was suppressed or removed
from the record. Numeric system-load inspection was denied, so the playback
timeouts are not attributed conclusively to machine load.

All gap/backend source hashes stayed unchanged during the complete gate. Ten
app paths changed while it ran, including one added file; both source manifests
are retained. The optional UI harness and final concurrent app state need their
own verification. This record does not claim a stable whole-checkout result.

Completed focused checks total 102 cases: 55 plan, 12 frozen-adapter, five actual
decoded-PCM, 14 CLI audio-inspection and 16 CLI project-command cases. The full
run also exercises all core and store tests. `core/focused-rerun.log` and
`migration/store-final-2.log` have no completed result and are not counted as
passing runs. Their coverage is supplied by the full workspace run.

The preserved core-21/database-27 CLI produced nine fixture revisions and five
history edits after validating an explicitly seeded initial reanchor snapshot.
`old-binary/commands.json` matches the retained fixture provenance hash.
`old-binary/gap-refusal-commands.json` independently proves the old CLI refuses
InsertTime after both one-play and three-play configured positive gaps, with no
revision change. No binary or scratch project is included here.

`review.json` records general, timing/PCM and strict-history reviews. All three
closed with no findings. The migration reviewer withdrew a chronology claim
that the old binary cannot produce. The timing reviewer independently confirmed
canonical gap phase zero at the consuming allocated anchor and exact 2/3 step
for the signed, nonunity placement test. The first failed PCM oracle used an
unbound fractional phase; production timing was unchanged when it was corrected.

The existing ten ImageGen boards remain interface targets. This timing/history
increment introduces no GUI capability. General atomic moment splice,
movement/raw-recipe lifecycle, native Visual/register editing and the full
product/release requirements remain open.

`sha256.json` covers every retained evidence file except itself. Runner scripts
and logs are development evidence, not end-user runtime requirements.
