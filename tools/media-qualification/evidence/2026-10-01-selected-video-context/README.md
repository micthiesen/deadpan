# Exact picture selection context evidence

See [qualification](../../../../docs/qualification/selected-video-context-2026-10-01.md)
for behavior, review findings, corrected failures and remaining Trim work.

The implementation base is `dbd89f1897f7fd92427849e56bb1ef2b1f59a273`.
Each named JSON file records the command, UTC start, source-manifest hash,
tracked-diff hash, PID, exit code and elapsed time. Its matching log is complete.
`source-*.json` retains SHA-256 hashes of the source files present at each run.

- `workspace-ui-tests` retains the broad run, including its historical storage
  fixture failure after all 2,552 non-storage tests passed.
- `storage-tests` retains the complete corrected storage run: 404
  unit/integration tests plus one compile-fail documentation test.
- `workspace-doc-tests` runs both workspace compile-fail documentation tests.
- `plan-tests-compact` reruns all plan tests after removing redundant private
  clock storage. `strict-clippy-compact` and `formatting-compact` are the final
  static checks.
- `core-plan-initial`, `formatting`, `strict-clippy` and
  `strict-clippy-reviewed` retain the earlier build, formatting and enum-size
  lint failures. The intermediate passing attempts also remain in this directory.
- `runs.json` summarizes results and source changes between the broad test start
  and subsequent gates. It reports 2,956 distinct unit/integration tests; the
  storage tests already run in the broad prefix are not counted twice.
- `host.json` records hardware, OS, the unchanged historical fixture hash and the
  final process scan. No native app was opened for this increment.
- `SHA256SUMS.json` covers all retained files except itself.

Run `python3 summarize.py .` from this directory to recompute the test summary.
`run-native.py` is the original runner used for these checks. To repeat a command
without overwriting retained evidence, set `DEADPAN_CHECK_OUTPUT` to an existing
scratch directory and pass a new label followed by the command. The runner uses
the qualified FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`.

These are exact picture, persistence and regression checks. They do not qualify
native Trim controls, physical input, performance or release packaging.
