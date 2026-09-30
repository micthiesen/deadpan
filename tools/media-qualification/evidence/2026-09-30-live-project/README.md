# Open-project command evidence

This record qualifies structural edits, history and automatic Render routed to a
real native writer on the development host. See
[qualification and limits](../../../../docs/qualification/live-project-2026-09-30.md)
and [the implemented contract](../../../../docs/LIVE_PROJECT.md).

## Results

| Check | Result |
| --- | --- |
| Original full workspace tests | 2,518 passed, four failed, none ignored; original failure retained |
| Corrected integration targets | All four affected targets pass, 39 tests |
| Final CLI library/live-project tests | 216 passed after lint corrections |
| Combined distinct workspace coverage | 2,523 passed across the recorded scopes; no clean full rerun claimed |
| Optional UI-feature tests | 330 passed, none failed or ignored |
| Strict workspace/UI Clippy, formatting, debug/release builds | Passed; debug linker unwind warning retained |
| Native Metal startup/shutdown | Passed |
| Native owner workflow | 36 CLI invocations; edits, concurrent Render, exact cancellation, historical recovery |
| Actual preview refusals | Camera, Gain and Room tone; no document or Render-row changes |
| Independent file decoding | Initial and retry movies; all 768 picture planes and complete authored audio |
| Visual Render replay | 79 checks plus Kestrel audit; eight inspected captures |
| Full release replay | 2,426 checks; no findings or failed/timed-out timing samples |

The separate accepted-generated-picture fixture is explicitly skipped in the
ordinary release replay. Native live progress for CLI-started work, persisted-job
recovery, VoiceOver, IME, non-US layouts, physical display color and release
packaging remain unqualified. Heavy media-preparation commands still require a
closed project. The new endpoint has no crash/power-loss qualification.

## Retained material

- `summary.json` records command outcomes, scopes and limits. Frozen workflow
  script scope notes predate the separately recorded native/decoder checks and
  are labelled accordingly.
- Journals preserve the original test, lint, build and decoder-admission failures
  alongside their scoped corrections. Preview-refusal CLI exits are expected
  failures; their separate reports confirm the assertions passed.
- `source-coverage.json` and compressed source inventories bind checks to exact
  code. Source equality alone does not establish behavioral coverage.
- `native-owner-evidence/` retains every command, exact target and durable
  receipt. The Room tone Apply/Undo investigation retains the actual history.
- `native-ui-report.json` records observations, clean quit and explicit native
  accessibility/progress limitations. Native screenshots could only be viewed
  inline; the retained PNGs come from the deterministic Metal replay.
- `decoded-native-owner/` retains the failed pre-reader admission.
  `decoded-native-owner-final/` retains the corrected independent-reader report.
  `decode-admission-proof.json` proves the pretty/compact JSON mismatch and the
  exact permitted revision/label differences. Media thresholds are unchanged.
- `replays/` retains compressed complete reports. `performance-summary.json`
  contains distributions without samples; `visual-review.json` records the
  parent image inspection. `screenshots.json` identifies eight selected PNGs.
- `native-files.tar.gz` contains 32 checked members: consistent SQLite backups,
  synthetic project media, published movies, independent decoded bytes and
  canonical references. `native-files.json` records every hash and database
  backup method. The collector reopened and rehashed all members.
- Qualification and collection scripts are retained as text, including the
  pinned helper source. `retained-files.json` identifies original inputs.
- `manifest.json` hashes every other retained file, including this README.

Discovery secrets, runtime sockets/locks, compiled binaries, unselected captures
and transient SQLite sidecars are excluded. Databases were copied through
SQLite's backup API. No direct live main-file copy is used as evidence.
