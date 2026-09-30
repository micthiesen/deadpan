# Typed encoder failure evidence

See the [qualification report](../../../../docs/qualification/encoded-failures-2026-09-30.md)
for scope, results, initial failures and remaining work.

- [Summary](summary.json): final checks, native counts and unrun checks.
- [Independent review](failure-review.md): findings, fixes and final assessment.
- [Native files](native-files.tar.gz) and [inventory](native-files.json): retained
  media, decoder reports, commands and a source package captured with SQLite's
  backup API. Every archive member was rehashed after collection.
- [Backup admission](backup-admission.json): the retained package has the same
  authoring digest as the real worker run.
- [Manifest](manifest.json): byte lengths and SHA-256 for every other file here.

The qualified product source inventory is
`50c41c44faff1d2cc9419aa59ae56fdf38f52e787ee393b1602591bf13b9bb30`.
Per-command JSON journals record arguments, environment and source inventory.
The saved `run-native-checks.py.txt` and `run-gate.py.txt` contain the execution
steps; `retain-native.py.txt` and `collect-evidence.py.txt` record collection.
Their absolute scratch paths describe this run and need replacement to reproduce
it on another machine. The native run used Apple M5 Max, macOS 26.5.2 and the
pinned LGPL FFmpeg 8.0.3 prefix recorded in the reports.

The first focused test and native build failed, then passed after fixes. Both
sets of logs remain here. These results do not establish automatic encoder
selection, public Render controls or full-product acceptance.
