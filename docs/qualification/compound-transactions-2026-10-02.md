# Resolved compound transactions, 2026-10-02

Resolved sequences of edits and register operations now share one atomic core
and SQLite execution path. Each step sees the preceding staged state and uses
ordinary command validation. An authored sequence saves one history entry and
one final register-bank version; Undo restores the complete edit. A failure in
any step preserves the previous document, bank, history and redo.

This increment starts at `e2b4b0368b8ba54fd7e0dbf21acea45408b9dc51`.
The [contract](../COMPOUND_TRANSACTIONS.md) records the APIs and bounds.
SQLite schema 54 reserves every editing step's allocation identity and retains
intermediate states used by copies. Schema 53 development packages require
recreation under the user's unused-project policy. Core document schema 43 is
unchanged.

## Covered behavior

The new tests exercise these boundaries:

- Repeat, cut and paste resolve in sequence, retain an intermediate copy and
  produce one exact inverse. Deleted allocations cannot be reused later.
- A copied Repeat survives Undo, register replacement, abandoned redo, reopen
  and restoration from a real SQLite checkpoint. Historical copies can be
  pasted again without making intermediate revisions live timeline targets.
- Per-step source and Generated-media admission rejects invalid intermediate
  work, including acceptance followed by deletion. Original pastes must match
  the selected qualified ordinal range exactly.
- Register-only compounds leave a populated redo stack and history cells
  unchanged. Frozen bank versions, explicit absence and copied payloads reject
  stale or forged inputs without writes.
- Corrupt checkpoint rows and forward references fail reopen or checkpoint
  validation. Schema 52 cannot acquire Compound history through migration.
- Sound deletion followed by insertion matches ordinary sequential editing,
  transforms its routes exactly twice and retains one complete inverse.
- The public headless command supports preview, commit, stale refusal, Undo
  and Redo. A failed later leaf leaves all relevant SQLite tables unchanged.
- The native project service reopens an intermediate Original-derived Repeat
  copy after Undo and pastes it into the current edit. Its restored copy identity
  cannot issue a new interactive capture or become an expected live revision.

## Review

Independent core and store reviewers inspected the source without running
Cargo or the app. Review found a size-limit mismatch between a transaction body
and its enclosing tagged command. The fixed guard measures the whole command
without cloning it; a boundary test covers exactly 64 MiB and rejection of one
additional byte. Additional review coverage includes populated redo, checkpoint
restoration and cumulative sound routing.

Further review found that a directly constructed Rust command could meet the
byte limits while exceeding the replay reader's value-count limit. Store
preparation now decodes its exact canonical request before saving. The retained
regression witness uses a valid 80,000-Hold insertion under the node and byte
limits, then verifies rejection without database writes for ordinary and
compound requests.

The core reader bounds decoded JSON content. Raw request bytes, including
whitespace and escaped text, are separately bounded before parsing by the CLI,
live IPC and store. Generic Serde callers must provide that ingress bound.

## Verification

The [retained evidence](../../tools/media-qualification/evidence/2026-10-02-compound-transactions/README.md)
contains exact commands, logs, exits and source inventories. The full locked
workspace suite passes 3,563 tests with zero failures or ignored tests,
including both compile-fail documentation tests. That includes 606 native app
tests and three headless app tests. Formatting and strict all-target workspace
lint pass.

With `ui-harness` enabled, all 642 app tests and three headless tests pass,
with zero failures or ignored tests. This suite overlaps the workspace run;
its counts must not be added to the workspace total. Strict all-target app lint
also passes with this optional configuration.

The final source inventory is
`a8021562d926dcae89bb9751b460783a9f87526194968b3bed4a099f07b94094`.
The recorder checked this complete tracked and untracked source inventory
before and after each final run. Workspace lint overlapped the last workspace
documentation tests; later checks ran sequentially.

Early core diagnostic builds exposed missing Serde `rc` support and an
ambiguous error conversion. The first full build exposed an app test calling
a private helper; its fixture now allocates deterministic identities locally.
Unused store imports were also removed. The recorded focused run then passed
22 compound tests, before the final replay-reader regression was added.
The retained failing replay-limit witness demonstrates the unsafe save; the
corrected store run passes all 75 library tests, including both ordinary and
compound refusal. These focused runs overlap the full workspace suite.
The existing nonfatal macOS debug linker warning about the 16 MB `__eh_frame`
limit remains in the logs.

## Verification limits

Macro recording, semantic selectors, count/call expansion, Macro register
content and dot-repeat remain unimplemented. The compound API supplies their
resolved execution boundary. This increment changes no keybinding or layout.
Native visual, physical-input, IME and full accessibility qualification remain
open. No ordinary native window was opened; the QA app remains closed. No DP
requirement or Gate A through G is complete.
