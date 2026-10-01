# Source-origin timing evidence

The implementation base is `f2388e5a379288ea2a9284f7cf40a2c2ac1ee642`.
See [the contract](../../../../docs/SOURCE_ORIGINS.md) and
[qualification](../../../../docs/qualification/source-origins-2026-10-01.md).

- `pcm-origin` retains the first fixture failures: an oversized oracle read and
  a source span beyond the fixture's admitted end.
- `pcm-origin-corrected` retains the remaining oversized stage-read failure.
- `pcm-origin-bounded` verifies both PCM regressions with valid read sizes and
  the measured fixture extent.
- `workspace-tests` retains the test-only `floor().unwrap()` compile error and
  the debug-link unwind-table warning; no tests ran in that attempt.
- `workspace-tests-corrected` runs the workspace with locked dependencies after
  removing the extra unwrap. `formatting-final` checks the corrected sources.
- `strict-clippy` checks all workspace targets with `deadpan-app/ui-harness`
  and `-D warnings`. These three final checks share source manifest
  `8aa01bb8b1783d7476bfdbe425a7f9fdc4c71c90f6bb7d992087b157fbe4c039`.
- Source manifests bind each command to the complete source inventory. Command
  JSON records the base, diff hash, start time, duration and exit status.
- `summarize.py` counts unit/integration and documentation tests from retained
  output, without combining duplicate successful runs.
- `host.json` records the host, fixture hash and absence of a running Deadpan app.
  `SHA256SUMS.json` binds every retained evidence file.

No native app is opened for this model increment. These checks do not qualify
the native Trim workflow, physical input, device playback or release packaging.
