# Persistent slice register evidence

See [qualification and limits](../../../../docs/qualification/durable-registers-2026-10-02.md).

- `checks/` retains exact commands, exits, complete source inventories and
  compressed logs, including failed and interrupted runs.
- `replays/` retains complete release reports, including explicit skips.
- `images/` contains inspected rendered states and their source index.
- `native/` retains app-only observations, consistent SQLite snapshots and
  cleanup evidence. The snapshots use SQLite's backup API.
- `summary.json` indexes commands and replays. Overlapping suites are not
  separate coverage totals. `binaries.json` binds executable and source hashes.
- `review.txt` distinguishes product findings from test-fixture corrections.
- `scripts/` retains the recorder, snapshot and collection scripts as text.
  `SHA256SUMS` seals every retained file except itself.

The complete workspace test run includes six failed fixture assertions. The
subsequent complete CLI and migration targets pass after those corrections.
Earlier compilation failures and an interrupted build do not count as passes.
The combined default coverage is 3,534 distinct tests; optional UI tests overlap.

Registers are project data in SQLite schema 53. Replay uses the shipped keymap.
Test instances are closed after native testing. Physical keyboards, native IME,
VoiceOver, the full crash/recovery matrix and audio acceptance remain open.
