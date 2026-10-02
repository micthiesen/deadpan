# Review disposition

Independent read-only review covered register storage, asynchronous capture/cut
receipts, command-entry intent, keyboard routing, pointer paths and tests. The
root agent owned Cargo, native processes, rendered replay and image inspection.

## Accepted and fixed

1. Original inspector Copy still called the old signature. It now uses the same
   selected-register copy entrypoint as the keyboard.
2. Blur or Escape could clear live selection while an open command revived its
   captured name. The command now retains cancellation and refuses explicitly.
3. Reopening a project could retain an old command's name in a fresh session.
   Command entry now binds its session and rejects that stale intent.
4. Native QA showed the old selection instruction after Escape. Cancellation
   now updates the status message as well as the selected name. The reviewer
   also caught the misleading Escape promise for explicit default-copy choice;
   that choice now reports only its content.

## Dismissed or narrowed

- Original pointer Copy explicitly focuses its picture inspector before invoking
  copy. Its prior sound-catalog focus is therefore not a sound-scope bypass.
- A delayed legitimate workspace snapshot can replace a generic status message
  with the service's preceding Undo receipt. Exact message equality was not a
  register integrity requirement. The replay instead checks all typed bank
  contents, selected name, Visual/Original selection, document, revision and
  history, plus no error or stale copy-specific success.

## Build and harness corrections

- Added register prefixes changed the audit inventory from 28 to 32. The audit
  retains exact expected counts and no-conflict assertions: 148,552 cases and
  62 Kestrel reservations.
- An existing cut helper used by the optional UI harness needed
  `cfg(any(test, feature = "ui-harness"))`. The unused Original helper remains
  test-only, eliminating a release warning.
- Held capture delivery now matches the full requested session, revision and
  range rather than the first available update.

Initial failing logs and replay are preserved alongside passing verification.

The final read-only pass confirmed the default-copy feedback fix and checked
the qualification and evidence prose. It found no remaining material discrepancy.
