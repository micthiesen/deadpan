# Saved edit receipts, 2026-10-02

Native sound edits, cached and worker-prepared whole-Original insertion, and
first-Original initialization now retain their exact saved revision before
refreshing the workspace. Previously those paths could save successfully and
then lose their confirmation when workspace preparation failed. The error now
states that the operation was saved and asks the user to reopen before editing
or undoing. Worker completion also retains the saved asset identity.

The native receiver already checks the receipt against the visible revision.
An unrefreshed view therefore preserves its cursor and selection. Catalog-only
registration reports its saved asset without inventing an edit receipt.
An error before commit creates no new success receipt.

This increment starts at `66912edf1c4b4a4921fa6c4d8d2df3d8d329d1eb`.
There are no schema, keybinding or layout changes. Semantic dot-repeat, macro
recording and one-transaction macro execution remain unimplemented. No DP
requirement or Gate A through G is complete.

## Verification

The [retained evidence](../../tools/media-qualification/evidence/2026-10-02-saved-receipts/README.md)
contains exact commands, logs, exits and source inventories.

Six fault tests exercise the real project actor and SQLite writer using measured
media and explicitly scheduled worker completions. They establish:

- Sound placement and removal retain the exact saved revision and selected
  sound or empty selection. The old visible snapshot remains unchanged.
- Cached and freshly prepared insertion retain the inserted Source identity.
  Each creates one revision and one history entry; retrying the stale request
  creates neither. Reopen exposes the saved result, and one Undo restores it.
- First-Original initialization retains its 120-frame protected baseline and
  asset. A stale retry cannot initialize another baseline. Reopen retains the
  same full Original with Undo unavailable below it.
- Catalog registration preserves the saved asset and creates no picture
  selection receipt. Qualification failure preserves the document and history
  without claiming a save.

The full locked app suite passes 605 app tests and three headless tests. With
`ui-harness` enabled, 641 app tests and three headless tests pass. These suites
overlap and their counts must not be added. All have zero ignored tests.
Formatting and strict all-target app lint pass for the default and optional UI
configurations. The final source inventory is
`fcc6311c6af4f53008991333c3303fa1468ce224e69041586773f34d2fa94fc3`.

Independent read-only review found no actionable issues in receipt ordering,
identity, stale-view consumption or duplicate history prevention. The reviewer
ran neither Cargo nor the native app.

The first test build exposed a missed test-only `Shared` initializer. The first
executed focused run then exposed three assertions using 240 frames instead of
the fixture's established 120 frames. Both were corrected; the failures and
passing reruns are retained. The existing nonfatal debug linker warning about
the 16 MB `__eh_frame` limit remains in the logs.

## Limits

This is service and recovery verification. No ordinary native window was
opened, and no rendered replay, physical-input, IME or new visual qualification
was run for this increment. The QA app remains closed. These checks do not
qualify macro execution, full recovery or release packaging.
