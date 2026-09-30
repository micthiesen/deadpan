# Original return evidence

See [qualification and limits](../../../../docs/qualification/source-return-2026-09-30.md).

- `before-replay.json.gz` retains the expected regression failure: returning through
  Browse moved Original 5 to 0 while Edit stayed at 2.
- `after-replay.json.gz` passes all 39 sound-workflow checks and the shortcut audit.
  Audio delivery is simulated; project services, registration, input and Metal
  presentation are real. The three PNGs show minimum-size sound controls,
  return while picture refresh is held, and the final Original frame.
- Workspace: 2,572 tests passed. Optional UI feature: 338 passed. Neither suite
  failed or ignored tests. Strict workspace/UI Clippy and formatting passed.
- Journals identify commands and source inventories. Compressed inventories
  distinguish the deliberately failing old-code replay from the final code.
- `native-ui-report.txt` records the real single-Original check and the incomplete
  legacy project-switch attempt. The app remained on the user-assigned desktop.
  `native-before-databases.json` and `native-after-databases.json` compare all 20
  tables of both fixtures; all rows were unchanged.

The native binary hash is retained, but executables, project packages and
discovery credentials are excluded. Native screenshots were inspected inline;
the saved PNGs here are offscreen replay output. No performance, acoustic,
VoiceOver, IME, non-US layout or distribution claim is made.
