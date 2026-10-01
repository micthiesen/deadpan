# Ripple Trim: independent store and CLI review

Read-only review, 2026-10-01. Reviewed the new store admission/preview module,
store command-preparation wiring, CLI `live_project::execute_short`, the new
store/CLI regression tests, schema rejection and current-history replay seams.
No source edits, Cargo or native execution.

## Result

No remaining concrete defect found in the reviewed store/CLI implementation.
One test-version defect was reported during review and root fixed it:

- `tests/migration/development_break.rs:105`, `tests/delete_range.rs:183` and
  `tests/delete_ripple.rs:83` still asserted database schema 48 after runtime
  advanced to 49. The assertions would fail before their intended checks.
  A subsequent read confirmed all three now assert 49.

## Admission and request behavior

- `preview_source_trim` opens one SQLite read transaction and uses the shared
  project/revision guard. Both zero and nonzero paths require an unused proposed
  revision, including allocation IDs reserved by an imported initial snapshot.
  Stale expected revisions and an equal current/new revision cannot become no-op
  successes.
- Zero previews still check timing allocation, forbid a wrapper allocation,
  re-resolve qualification and require unchanged Source/document plus absent
  timing/root operations. They return `edit: null`. Raw preview and commit use
  the core reducer and reject zero before writing history.
- Nonzero preview and commit share `prepare_current_command_with_admission`.
  The store re-resolves the explicit target and compares the exact physical
  Source and allocation against the core-produced candidate. New wrappers must
  occupy the captured parent slot with the required neutral fields; existing
  Partitions retain their slot and resolved allocation.
- Receipt lookup verifies bounded canonical snapshot bytes, receipt identity and
  original ownership. The complete immutable asset record must equal measured
  metadata; both before/after video and present audio spans must equal that full
  measured context. Preview output grants no commit authority. Fresh media-byte
  verification remains with playback/export, consistent with the existing Slip
  boundary and ordinary editing without media I/O.

## Persistence and protocol

- Commit uses the existing immediate transaction for revision, history and cursor.
  Current-history replay recomputes the command, compares its transaction and
  resulting document, and checks exact inverse restoration. Store qualification
  validation scans every stored revision, including abandoned histories.
- The new store test reopens after commit and again before Redo, validates history,
  checks fresh Undo/Redo revision IDs and reobtains the receipt. It also checks
  no-write zero/invalid requests, exactly one new revision/history row, and missing
  receipt refusal on the descriptive zero path.
- The CLI keeps protocol 1, adds `source_trim` only to typed Trim dry-run output,
  and uses the same execution path for cold and hosted calls. The command envelope
  and core command reject unknown fields; unsupported trim modes cannot deserialize.
  Commit retains the ordinary receipt envelope. The CLI test compares cold/hosted
  dry-run JSON and preview/committed transaction equality, then exercises stale
  refusal and durable Undo.
- Database 49 accepts the current core vocabulary. Unsupported development schemas
  39 through 48 reject before writable open or migration backup; supported older
  adapters remain closed to the new command. No new compatibility migration was
  introduced.

The store/CLI tests had not been run when this review was requested. This report
records source review only; root owns compilation and runtime results.
