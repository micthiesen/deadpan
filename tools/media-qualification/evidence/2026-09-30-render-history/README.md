# Native saved-render recovery evidence

See [qualification](../../../../docs/qualification/render-history-2026-09-30.md)
and [contract](../../../../docs/RENDER_JOBS.md#native-saved-render-browser).

- Command JSON/log pairs retain the exact arguments, source inventory, exit and
  duration. `source-*.json.gz` binds tracked and new source files to hashes.
- `replays/ui-first` through `ui-third` preserve initial failures.
  `ui-fourth/report.json.gz` is the passing 112-check Render run plus the Kestrel
  compatibility audit. Intermediate screenshots are omitted; selected final
  named captures are in `captures/`.
- `movies/` contains four actual published MP4s and their local reports.
  `movies.json` records their exact sizes and hashes. Checkpoint retry and
  reconciliation use freshly admitted production verification; stored metadata
  is not treated as a live capability.
- `native-ui/native-ui-report.txt` records actual native keyboard and save-sheet
  observations. Its screenshots were inline only. Native AX did not expose the
  custom overlay, and real native IME was not tested.
- `native-before.sqlite.gz` and `native-after.sqlite.gz` are consistent SQLite
  backups. `native-database-check.json` records exact equality of all 20 tables,
  database integrity, released writer lock and native executable identity.
- `review.json` records independent review, corrections and remaining native
  accessibility evidence. `test-counts.json` derives counts from retained logs.
- `collect.py.txt`, `final-checks.py.txt` and `verify-native.py.txt` preserve the
  small execution/collection scripts. `manifest.json` hashes this evidence set.

The mistaken `ui-check` Cargo feature call failed before tests; the corrected
`ui-harness` invocation passes. Initial fixture compile failures, scroll/focus
replay failures, the bounded screenshot warning and native linker warnings remain
retained. No compiled binaries, runtime discovery secrets, sockets or direct
copies of a live main database are included. Absolute paths describe the original
run and are not a portable project package. This is developer-host evidence, not
signed release or clean-machine qualification.
