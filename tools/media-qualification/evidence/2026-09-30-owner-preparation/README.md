# Open-project preparation evidence

See [qualification and limits](../../../../docs/qualification/owner-preparation-2026-09-30.md)
and [the implemented contract](../../../../docs/LIVE_PROJECT.md#background-preparation).

| Check | Result |
| --- | --- |
| Locked workspace tests, including doctests | 2,572 passed, zero failed or ignored |
| Optional UI-feature tests | 338 passed, zero failed or ignored |
| Strict workspace/UI Clippy and formatting | Passed |
| Native Metal startup/shutdown | Passed |
| Actual CLI workflow | 28 invocations, including 10 expected failures |
| Native window | Catalog/picture refresh, keyboard navigation and post-relink preview inspected |

The command workflow includes linked and managed retention, exact source and
audio-only registration, nine failed stream preparations followed by success,
versioned/no-op/stale relinking, consistent checkpoint publication and current
schema validation. The nine failures exceed the owner's eight-entry archive
limit and demonstrate terminal release. They do not establish long-run stress
coverage. Operational changes preserved authored/history rows.

Known UI follow-up: returning from Sound to the same Video resets the Original
cursor through the existing `select_source` path. That behavior is not qualified
as correct and remains recorded for the next fix.

## Retained records

- Journals and logs retain every initial compile/test/lint/format failure and
  the later passing scope. `summary.json` lists exits and counts; the full
  workspace run passed without scoped test corrections afterward.
- `source-coverage.json` maps the native/base test snapshot and final code.
  Only one help string changed after the native/base snapshot. UI-feature tests
  and final lint/formatting use the final inventory. Compressed `source-*.json.gz`
  files record all included code hashes; staging can make the journal's ordinary
  unstaged diff hash incomplete, so use these complete source inventories.
- `native/` contains exact argv, stdout/stderr, expected exits, requests, final
  documents, synthetic fixture bytes, SQLite backups and the published checkpoint.
  The initial/registered/live databases were made through SQLite's backup API;
  no live main database was copied directly.
- `native-ui-report.json` and the operator's `.txt` preserve observations and
  limitations. An app-specific query after Quit showed an empty running app;
  a later inventory-only query confirmed normal exit. Inspection relaunch is
  a possible explanation, not a proven tool mechanism. Parent process, lock,
  discovery and saved-document checks passed.
- Scripts are retained as `.py.txt`. `collection-first-failure.json` records a
  collector rejection of read-only SQLite sidecars. The corrected collection
  excludes those sidecars; the observed WAL was empty. Product checks did not fail.
- `retained-checks.json` verifies the collected databases. `manifest.json`
  hashes every other retained file, including this README.

Discovery secrets, runtime sockets, compiled executables and SQLite sidecars
are excluded. Native screenshots were inspected inline but the documented CUA
API exposed no local-save path. No image file, acoustic result, VoiceOver/IME,
large-project performance, process-crash, physical power-loss or release-packaging
qualification is claimed. Native recovery and remaining product gates stay open.
