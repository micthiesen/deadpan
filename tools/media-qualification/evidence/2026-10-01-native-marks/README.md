# Native marks and jump history evidence

See [qualification](../../../../docs/qualification/native-marks-2026-10-01.md)
for behavior, corrections, measured results and limits.

- `checks/`: every command, exit code, duration, log and source manifest,
  including the corrected compile/test/replay failures. Large files are gzip.
- `replays/`: complete focused and full release reports.
- `images/`: inspected minimum/default pending/list/final-row captures and
  earlier failed states. The index retains each final capture's semantic state.
- `native/`: release binary identity, seven consistent SQLite backups,
  authored documents, verification scripts, observations and confirmed cleanup.
- `environment.json`: host/compiler, all 1,369 source hashes rechecked, and
  the exact source difference since UI unit tests. Only the marks replay changed.
- `performance-summary.json.gz`: all scenario timing summaries and skipped checks.
- `scripts/collect.py.gz`: evidence collector; `SHA256SUMS.json` covers every
  retained file except itself.

Implementation base: `291853145eb434864cdf84dcbbab92c81881ff73`.
Final source manifest:
`9f81d48542fa16934f3aabafe0af4f2d5b1a3511d47b343c704936442c786ea8`.
The full release replay passes 3,589 checks with no findings or failed samples.
The accepted-generated fixture, physical listening, complete native IME/layout
and accessibility acceptance, signing and release packaging remain unqualified.
