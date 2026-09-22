# Worker cleanup evidence

Base source: `64ee95f1e03e3be780a88c4eb2e2b004518ff44f` plus the source files
bound by `source-manifest.json`. Environment: Apple M5 Max, Mac17,7, 128 GiB,
macOS 26.5.2 build 25F84, Rust 1.97.1. The gate used the previously qualified
LGPL FFmpeg 8.0.3 prefix recorded in its results.

- `ci-failure.log`: original GitHub run 35678400735 failure at that base revision.
- `gate-failed-shell-diagnostic/`: first local gate after runtime changes,
  before isolating the shell fixture's child diagnostics.
- `shell_diagnostic_probe.py`, `shell-diagnostics.json`: independent raw-pipe
  reproduction; trial index 11 captures a SIGKILL shell diagnostic after JSON.
- `gate/`: final exact repository gate; all five commands passed, including
  794 tests, zero failures and zero ignored tests.
- `gate-before-review-fix/`, `source-manifest-before-review-fix.json`: a passing
  earlier gate before the reviewed ownership/fallback corrections. It does not
  qualify the final implementation.
- `linux/`, `run_linux.py`: 75 passing targeted native-process/jobs tests in a
  digest-pinned Linux aarch64 Rust 1.97.1 container with read-only source mount.
  The command record retains the image, toolchain and stable source hashes.
- `run_gate.py`: final developer capture script, retaining its original scratch
  output path. Run a copy outside the evidence directory to avoid changing it.
- `review.md`: independent review scope, findings and disposition.

The failed gate's original `results.json` only counted successful test binaries.
Its `test_totals` therefore omit the failing binary. The corrected totals derived
from the unchanged raw log are in `failed-gate-counts-corrected.json`: 414 passed,
one failed, zero ignored before the run stopped. The final runner counts both
successful and failed binary summaries. Raw logs are preserved verbatim,
including their original whitespace and compiler/platform diagnostics.
The final gate also hashes the changed sources before execution and verifies
they remain unchanged after every command.

The focused media/native tests and jobs supervisor tests passed before the
first full gate; that did not establish whole-workspace success. Native GUI,
device, accessibility and startup checks were not repeated for process-only
changes. The Linux media/FFmpeg workspace was not exercised. This evidence is
not full process containment or chaos qualification.
