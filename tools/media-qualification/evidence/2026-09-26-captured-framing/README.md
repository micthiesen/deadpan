# Captured pause framing evidence

This directory records the increment against
`c03a5edde5f28d27074745eb15711cb28b1f2e50`. See the
[qualification narrative](../../../../docs/qualification/captured-framing-2026-09-26.md)
and [contract](../../../../docs/CAPTURED_FRAMING.md).

- `review.json`: three independent read-only reviews and resolved findings.
- `old-binary/`: the actual core-18/database-24 fixture producer, command transcript
  and hashes. The SQL itself lives in the store migration fixtures.
- `integrated-core-test*.log.gz`: earlier integration attempts before the final
  typed-admission corrections. They do not identify the final source state.
- `context-admission-test.log.gz`: focused admission tests before the later
  private-isolation budget correction. The final repository run covers that correction.
- `gate-1/`: the initial Clippy warning, retained without rewriting the result.
- `gate-2/`: formatting and Clippy pass; the workspace test run stops after 743
  passing tests when an existing fixture cannot bind a Unix socket. Its report
  includes 423 unchanged source/configuration hashes.
- `gate.py` and `continue-gate.py`: exact check runners. The continuation excludes
  completed library/integration suites, runs remaining tests without stopping at
  the first failure, then covers unfinished doc-tests, build and doctor. It does
  not turn the interrupted full gate into a pass.
- `gate-remaining/`: all 617 remaining tests and doc-tests pass; workspace build
  and doctor pass, with the same 423 unchanged source/configuration hashes.
- `jobs-example.log.gz`: the worker qualification example's three tests, run
  separately after the selected jobs integration suites.
- `verification.json`: combined results and exact constraints. The count is
  1,363 passed, one failed, none ignored; the full test gate remains failed.
- `captured-metal-1.json` and its log: actual Metal harness failure before any
  comparison because no adapter was available. No software fallback was used.
- `native-gui/`: exact rebuilt binary identity, read-only backup baseline, synthetic
  scratch project identities and the Computer Use denial. No live interaction or
  screenshot of this build is claimed; pending checks are explicit.
- `design-integrity.json`: hash/dimension verification of the nine existing
  ImageGen board and prompt pairs. These remain targets, not implementation evidence.

Logs are gzip-compressed with deterministic timestamps. App binaries, private
media and database files are not included. This evidence establishes no release,
physical-display, HDR, acoustic, performance or encoded-export qualification.
