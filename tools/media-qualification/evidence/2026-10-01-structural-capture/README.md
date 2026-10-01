# Structural capture and cut register evidence

See `docs/qualification/structural-capture-2026-10-01.md` in the repository for
the findings, passing results, corrected failures and qualification limits.

- `checks/` retains exact commands, exit codes, durations and source manifests.
  Logs and manifests are gzip-compressed; failed attempts are retained.
- `replays/` retains complete JSON reports. `images/` includes the inspected
  final cut/paste and empty-group states plus earlier failure states.
- `native/` retains the private functional QA binary identity, seven consistent
  SQLite backups, authored snapshots, observations and complete comparison.
  Native screenshots were inspected through CUA but are not retained here.
- `scripts/` retains the runners, native snapshot/verifier and collection code.
- `old-range-generator.rs.txt.gz` is the test appended to the isolated old
  implementation. Its literal output also remains in the store fixture directory.
- `post-workspace-source-delta.json.gz` identifies the four files changed after
  the full workspace run: three replay corrections and one production label.
- `environment.json` records the compiler, host, source reconciliation and replay
  outcomes. `SHA256SUMS.json` covers every collected file except itself.

The implementation base is `2ea675646abc24e8410616af74b4193ed4af51da`.
Final implementation source manifest:
`55df165772764063e162c8b1eb397f8e691aff3f2a625bd39d4711355236d4c5`.
The original workspace gate used
`2ff5fa645251de51887575a0bafa8d71b59df3747a9b982e6617e2e844e68e1b`.
No accepted-generated fixture, physical listening, native IME, complete
accessibility, signing/notarization or complete-product qualification is added.
