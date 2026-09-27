# Compact audio reanchors: 2026-09-26

This increment implements [ordered per-occurrence reanchor steps](../AUDIO_REANCHORS.md)
and retained visible-allocation projection. The existing plan and PCM consumers
evaluate the resolved scalar anchor and phase. Supported root pause insertion
composes after existing steps. Authored gap bindings, general atomic Original
moment splice and its native Visual/register workflow remain open. No DP
requirement or release gate is complete.

The checkout remains based on `c03a5edde5f28d27074745eb15711cb28b1f2e50`, with the
prior captured-framing, Original-moment and gap-clock increments retained. Git
metadata is read-only in this session; these changes are not committed or pushed.
Current schemas are core 21/database 27, with audio-context schema 2 unchanged.

## Behavior and evidence

- Retained allocation clips every ancestor selection, including Partitions,
  separately from meaningful raw support. Hidden occurrences have no entry.
- A cut first Repeat play and later full plays resolve different local anchors.
  Queries remain bounded for billion-play sparse layouts and actual gaps.
- Each step resolves its own birth scope. New plays retain intrinsic Partitions;
  narrower roots discard enclosing windows, while the same retained root keeps
  its intrinsic window. Preserve input/output scopes remain distinct.
- Chronological steps accumulate actual sample-boundary differences on their
  respective clocks, retaining older phase terms. Split copies live arguments,
  pruning preserves step-only clocks, and changed-owner/allocation inventories
  visit every step.
- Actual decoded 44.1 kHz PCM checks first and later NTSC Repeat entries, prior
  resumes plus sequential clocks, born plays, hidden occurrences, a subsequent
  real InsertTime command, and retained opaque Preserve preparation. Irregular
  reads and fresh seeks agree; short reads cannot bypass full preparation limits.
- The preserved actual core20/database26 binary produced a fixture with 27
  revisions, 15 history entries, selected audio, old InsertTime phase terms and
  pending redo. Migration compares every snapshot and transaction, preserves
  operational rows and the backup, and exercises later fresh revisions.
- Frozen core16 through core20 reject new binding vocabulary even as empty,
  null or escaped fields in snapshots and both patch directions. Modern bindings
  survive normal SQLite history, undo/redo and reopen without invented steps.

## Verification

Focused checks passed 242 cases: 129 core, 91 store migration/persistence,
six decoded-PCM reanchor and 16 CLI project-command cases. The final workspace
run passed 1,463 tests, failed one and ignored none. Formatting, all-target Clippy
with warnings denied, the locked workspace build and CLI doctor passed. All 446
source/configuration hashes remained unchanged throughout that run.

The sole failure is `directories_fifos_and_sockets_are_rejected_without_blocking`
at `crates/deadpan-jobs/tests/artifact.rs:200`: the sandbox denies
`UnixListener::bind` with `PermissionDenied`, OS error 1. This prevents the socket
rejection assertion from running. The overall gate remains failed; no test was
skipped or suppressed, and `--no-fail-fast` completed all other suites and
documentation tests. [Retained evidence](../../tools/media-qualification/evidence/2026-09-26-audio-reanchors/README.md)
includes commands, logs, source hashes and review results.

Three independent reviews covered general correctness, exact timing/PCM and
strict migration. All returned no findings. The timing reviewer also checked
the corrected Preserve oracle against the source-support contract.

The first core compilation exposed a new test module's missing path attribute;
it was corrected. One unused import was removed. The first PCM run exposed an
oracle using filter context past the Source host's authored end. Its independent
support is `[100, ceil(100 + 128 * 147/160)) = [100,218)`, and the test now compares
all 384 original Preserve outputs before moved slices. That comparison initially
exceeded the inspection API's 256-sample request limit; it now uses two legal
reads. Neither production timing nor preparation limits were relaxed.

## Limits

No native UI, startup, lifecycle, GPU or device changes are included. GUI and
listening checks would not establish this pure timing/history contract and were
not repeated. The existing ImageGen boards remain unchanged interface targets,
not implemented Visual/register UI evidence. The full product and acceptance
requirements remain open in [the tracker](../REQUIREMENTS.md).
