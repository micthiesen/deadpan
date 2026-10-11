# YouTube size-bound regression and packaged 4K import

See the [qualification](../../../../docs/qualification/youtube-limits-2026-10-11.md)
for the behavior change, failures, results and limits.

- `checks/` retains the configured old-code regression failures, the initial
  missing-prefix build failure, all later checks and both harness setup errors.
  The zero-test app-library invocation is followed by the actual app-binary
  test run. The real-URL app test is disabled in the ordinary selected test run.
- `packaged-4k/` contains the actual relocated app's helper probe, URL import,
  validation, reopened view, document, diagnostics, signature check and command
  timings. Source and executable hashes bind this evidence to the final build.
- `bundle/` retains release checksums, the software inventory and build
  provenance. `scripts/` retains the executed commands and their continuation;
  these scripts use explicit local paths and are not a portable test runner.
- `source-identity.json` binds the complete reviewed source diff and the
  packaged executables. `compression.json` records original lengths and hashes
  for deterministic gzip files. `SHA256SUMS` covers every file except itself.

The public test source is *CHARGE*, Blender Studio, YouTube ID `UXqq0ZvbOnk`;
the source reference and credit are retained in `packaged-4k/identity.json`.
No cookies or account were used. The movie is not duplicated in this evidence
directory. The full app and project remain in
`/tmp/deadpan-youtube-limits-20261011/packaged-4k`, with the initial bundle in
`/tmp/deadpan-youtube-limits-bundle-20261011`.
