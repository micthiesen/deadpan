# Verification identities

- `source-identities-first.json`: production freeze and original replay fixture,
  based on25f40767 with the original Cargo.lock. Focused checks, normal/UI
  strict Clippy, 2807 selected regressions and first release exports ran here.
- `source-identities-before-security.json`: same production, with the real
  Shift-Tab focus step added to the replay. No runtime/router implementation
  changed. The first passed replay reports are retained in
  `visual-before-security` and `performance-before-security`.
- The earlier replay metadata reports the checkout state at replay start.
  Another session changed Cargo.lock while qualification was running, so those
  observed lock hashes alone must not be used to infer build dependencies.
  Executed binary hashes remain their actual identities.
- `source-identities.json`: final source and dependencies after concurrent
  security commit5f9fac98. The security integration rebuilds strict UI Clippy,
  release CLI/export test and release UI app using locked dependencies before
  rerunning media, visual and performance checks. `dependency-lock-start.txt`
  and `dependency-lock-end.txt` must match across that complete run.
- `export-binaries-before-security.json` and `export-results-before-security.json`
  preserve the first release media run. `export-binaries.json` pins the final
  CLI and export test immediately after export, before the UI feature build.
- `visual-first/report.json` is the failed replay focus assumption, not a final
  passing result. `visual/report.json` and `performance/report.json` are the
  final checked reports; warnings remain in them.

Root owns Cargo and all repository changes in this increment. Unrelated
`.claude/` and fuzz/Cargo.lock changes belong to another session and are not
part of the speed-update commit.
