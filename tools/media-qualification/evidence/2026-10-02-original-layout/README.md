# Compact Original layout evidence

See [qualification and limits](../../../../docs/qualification/original-layout-2026-10-02.md).

- `checks/` retains exact commands, source inventories before/after execution,
  completion codes and compressed logs, including failures. Compression has no
  timestamp. `*-verified` records use the final source inventory.
- `replays/` retains complete reports. `full-release` is the initial passing run
  whose inspected images exposed retained-frame distortion. `full-release-verified`
  includes the corrected aspect checks and paint. `pointer-witness-red` proves
  premature GPU submission; `aspect-witness-red` proves stretched retained pixels.
- `images/` retains final default/minimum, long-key, active transport and catalog
  states, plus the failed submission/geometry witnesses. `sources.json` identifies
  exact source captures. Reports retain their narrower scripted-input limits.
- `summary.json` indexes commands and replay outcomes. Overlapping test suites
  must not be added together.
- `binaries.json` identifies the binaries, final source inventory and app-only
  changes since the previous full workspace gate. `cleanup.json` records the
  final process inventory.
- `review.md` records accepted findings and harness corrections. Review was
  read-only; the main agent owns runtime checks and image inspection.
- `scripts/` retains the execution and collection scripts as text.
  `SHA256SUMS` seals every retained file except itself.

Private replay keymaps never read or change personal settings. No ordinary
native window was opened; replay processes exit after testing. Physical keyboard,
OS IME, VoiceOver, audio-device and physical-display acceptance remain open.
