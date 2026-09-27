# Sound audition evidence

See the [qualification record](../../../../docs/qualification/sound-audition-2026-09-27.md)
for implementation boundaries, review corrections and acceptance limits.

- `verification.json` retains exact invocations and outcomes, including initial
  failures. Logs are compressed without rewriting their contents.
- `gate-01` retains the required workspace gate and harness-feature lint/tests.
  Its source comparison records the final selected-sound label added during
  workspace lint. Workspace tests/build and feature lint/tests followed that edit.
  `final-source-check` separately rechecks formatting and seals the unchanged
  final source; it does not replace or hide any workspace test failure.
- Visual/performance replay records identify the actual executed binary and
  distinguish adapter failure from scenario, image or timing evidence.
- `review.json`, `increment.json` and `increment.diff.gz` identify the reviewed
  increment. `final-source.json` seals source/config files; `context.json` records
  toolchain, platform, schema and the new ImageGen board identity.
- `sha256.json` hashes retained evidence files except itself.

The workspace tests recorded 1,743 passes and four failures: one sandbox-denied
Unix socket setup and three playback worker-wait timeouts. The three playback
cases passed individually with the unchanged binary. All 236 final app/harness
tests passed. Retrying does not turn the original workspace result green.

Visual and separately built release replay passed the live 3,472-case shortcut
audit, then failed to acquire Metal before any sound scenario steps, captures or
timing samples. They supply no aesthetic or latency acceptance evidence. The final
formatting check passed with all 572 source/config hashes unchanged.

The prior contributed harness and design assets remain included. Sound placement
and the full audio graph remain open. No DP requirement or gate is promoted.
Git metadata was read-only; there is no new commit or push.
