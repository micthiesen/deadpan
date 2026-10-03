# Repeat operator evidence

See the [qualification record](../../../../docs/qualification/repeat-operator-2026-10-03.md)
for implementation scope, results and limits.

- `checks/`: exact commands, exits, logs and runtime source inventories. The
  first core/plan/audio run passed tests but rejected the global source guard
  during independent app edits; `initial-core-scope.json` records the difference.
  The first app run's obsolete help assertion failed; its corrected run passed.
- `replays/`: complete compressed JSON reports for Repeat, rapid input, macros,
  cut dot and remapped keys. Combined latest workflow checks: 1,094. Each report also
  retains the same 3,319,728-case Kestrel audit and its explicit skipped checks.
- `native/`: accessibility observations from actual native key delivery on a
  private project copy. `metadata.json` records actions and clean app shutdown.
- `screenshots/native-pending.png`: pending Repeat guidance and retained register.
- `screenshots/native-visual.png`: four selected frames, three total plays,
  one structural Repeat, retained register and updated edit duration.
- `screenshots/macro-repeat.png`: counted named Repeat macro, nested editable
  result and one Undo, from the rendered replay.
- `screenshots/remapped-help.png`: the final help correction uses configured
  operator `b` and frame motion `ah`, with complete painted text at 960×640.
- `scripts/`: collectors and validators retained as text so they do not become
  part of the runtime source inventory. `SHA256SUMS` covers the retained files.

The dot replay emitted one unexplained H.264 `decode_slice_header error` while
passing all 381 checks. Debug build logs retain the existing compact-unwind
linker warning. These checks establish no release-performance, physical-layout,
IME, VoiceOver, acoustic or packaging qualification.
