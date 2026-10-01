# Adjacent Source Roll evidence, 2026-10-01

Backend increment on `0304852f7485d730e117d89f649b9ce0421b4945`. See the
[qualification](../../../../docs/qualification/source-roll-2026-10-01.md) and
[command contract](../../../../docs/SOURCE_ROLL.md). Full native Trim, overwrite
and complete product gates remain open.

## Results

- Full workspace with `ui-harness`: 3,275 unit/integration passes, one stale CLI
  doctor assertion failed, and both documentation tests passed. None ignored.
- The corrected doctor test passed on the same workspace feature graph. Only
  its expected core-schema number changed. `source-proof.json` and the exact
  diff show the test-only correction. Together the runs cover all 3,276 tests;
  the original full invocation remains recorded as failed.
- The selected Roll run passed 57 tests and failed one PCM oracle. Its corrected
  focused rerun passed before the full run. Counts overlap and are not added.
- Strict all-target workspace lint with warnings denied and final formatting passed.

## Inventory

| Files | Meaning |
| --- | --- |
| `roll-*.log` and matching `roll-*.json` | Complete output, arguments, exit status, elapsed time and source identity, including failed attempts. |
| `source-*.json` | SHA-256 source inventories captured before and after checks. |
| `summary.json` | Parsed test totals for each invocation. |
| `source-proof.json`, `doctor-assertion.diff` | Exact test-only difference after the full run. |
| `environment.json` | Host, toolchain, fixture hashes and final process cleanup. |
| `failure-notes.md`, `*-review.md` | Failed expectations, corrections and independent static reviews. |
| `tool-references.json` | Paths and hashes of the previously committed recorder and parser used for these checks. |
| `staging-provenance.json` | Pre-integration patch identities; executed sources are identified by the source inventories. |
| `SHA256SUMS.json` | Hashes of all retained files except this seal. |

The original reverse-Roll expectation ignored newly available filtering taps
within 128 samples of the old support boundary. The corrected test separates
expanded support from retained phase without tolerance or a production change.
The original mark-test compilation error and stale doctor assertion also remain
recorded. The existing debug linker `__eh_frame` warning persists.

No native GUI was opened. The final process scan found no executable whose name
starts with Deadpan. Default-feature app checks,
Python suites, release build and painted replay were not repeated. These indexed
picture, decoded-PCM and persistence checks do not qualify physical display,
device playback, export, performance or release packaging.
