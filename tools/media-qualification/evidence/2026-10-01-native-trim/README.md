# Native combined Trim evidence

See the [qualification record](../../../../docs/qualification/native-trim-2026-10-01.md)
for results, corrections and limits. All product requirements and release gates
remain open or partial.

- `environment.json`: base commit, final and broad-gate source inventories,
  exact source files changed after the broad gate, host and compiler.
- `checks/`: commands, exit codes, durations and before/after source hashes.
  Logs and exact source inventories are compressed with gzip. Failed checks are
  deliberately retained alongside their corrected runs.
- `replay-summary.json.gz`: per-run checks, failures, skips and timing summaries.
  Full replay reports are in `replays/`; output-device delivery is injected there.
- `images/`: selected final offscreen Metal captures plus failed replay witnesses.
  `index.json.gz` binds each capture to its input and semantic state. Native
  screenshots were inspected in conversation, not retained as PNGs here.
- `native/`: synthetic fixture snapshots made through the SQLite backup API,
  exact cancellation/history/reopen comparisons, native release observations
  and process/lock shutdown checks. No app bundle or source movie is included.
- `scripts/`: execution and collection scripts plus exact follow-up patches.
- `SHA256SUMS.json`: SHA-256 of every retained file except this inventory itself.

The normal native release build exercised real output and saved history. Its
exact source/binary identities and the later feedback-only changes are described
in `native/release-observations.md`. A short fixture does not establish general
playback throughput, acoustic quality, OS input coverage or distribution readiness.
