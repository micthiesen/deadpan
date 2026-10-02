# Named session register evidence

See [qualification and limits](../../../../docs/qualification/named-registers-2026-10-02.md).

- `checks/` retains exact commands, exits, complete source inventories before
  and after execution, and compressed logs. Failed checks are retained.
- `replays/` retains complete reports. `replay` is the initial failed assertion;
  `replay-final` and `full-replay-final` passed before native QA found stale
  cancellation feedback. `replay-verified` includes that correction and check.
- `images/` contains inspected final register states and their source index.
- `native/` retains native AX observations, screenshots and cleanup evidence.
  Unprefixed captures precede the feedback correction; `verified-` captures
  check the final executable. App-only screenshots contain the synthetic fixture.
- `summary.json` indexes every command and replay. Overlapping suites are not
  separate coverage totals. `binaries.json` binds executable and source hashes.
- `review.md` distinguishes product findings from harness corrections.
- `scripts/` retains the recorder and collection commands as text.
  `SHA256SUMS` seals every retained file except itself.

Registers are session-only. Replay does not read personal keymaps. Native QA
loaded the shipped map without changing user settings. Test instances are closed
after testing. Physical keyboards, native IME, VoiceOver and audio acceptance
remain open.
