# Scoped mix preparation evidence

See [the qualification record](../../../../docs/qualification/scoped-mix-2026-09-27.md)
for scope, review corrections, failures and unverified product work.

- `verification.json` summarizes exact invocations and outcomes. Earlier failed
  PCM runs and the diagnostic policy query are retained, not counted as passes.
- `gate-01` retains formatting, strict workspace lint, workspace tests/build,
  doctor, strict harness-feature lint and app/harness tests. Logs use gzip.
- `playback-retry`, `playback-isolated` and `playback-serial` retain the unchanged
  playback binary's default-thread, individual-case and single-thread diagnostic
  runs. Diagnostic passes do not replace the failed default workspace gate.
- `review.json`, `increment.json` and `increment.diff.gz` identify the reviewed
  increment against the saved pre-sound working tree. `final-source.json` seals
  all source/config files used by the final gate.
- `context.json` identifies platform, toolchain and preserved design assets.
- `sha256.json` hashes every retained evidence file except itself.

This preparation work changes no painted interface and makes no new GUI,
aesthetic, keyboard, acoustic or performance acceptance claim. The existing
UI harness remains included. Core schema 28 and database schema 34 are unchanged; no DP or gate is
promoted. Git metadata was read-only, so there is no new commit or push.
