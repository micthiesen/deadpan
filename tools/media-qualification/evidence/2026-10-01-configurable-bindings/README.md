# Configurable binding evidence

See [qualification and limits](../../../../docs/qualification/configurable-bindings-2026-10-01.md).

- `checks/` retains exact commands, completion codes, source inventories and
  compressed logs, including failures. Compression has no timestamp.
- `summary.json` indexes those records and the replay results. Overlapping
  test suites must not be added together.
- `replays/` retains complete reports. `full-replay` is the initial failing run;
  `full-replay-verified` is the final release run. `post-submit-red` is the production
  witness before the final held-opener correction. `shifted-opener-red` in
  `checks/` retains the earlier failing pure regression.
- `images/` retains inspected minimum-window key teaching, persistent error
  details and ordered input states. The initial error measurement frame is
  included to explain the corrected paint assertion.
- `binaries.json` identifies executed binaries and the four app files changed
  after the full workspace run. `cleanup.json` records the final process check.
- `scripts/` retains the execution and collection scripts as text.
  `SHA256SUMS` seals every retained file except itself.

The scripts use private keymap files. They do not read or change personal
settings. No ordinary native app window was opened.
