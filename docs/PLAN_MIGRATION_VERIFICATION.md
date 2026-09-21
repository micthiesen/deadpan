# Exact picture plans and schema migration

This slice adds stable repeat occurrences, original presentation-frame lookup,
an immutable indexed picture plan, and complete schema-1-to-2 history migration.
It is groundwork for the editor, not a completed renderer or release gate.

## Implemented contract

- Checked `ExactRatio` retains fractional coordinates through nested retiming.
  JSON carries wide numerators and denominators as decimal strings. Intermediate
  overflow is an explicit error, never a rounded or saturated coordinate.
- `SourceFrameIndex` selects original presentation frames from strictly ordered
  PTS intervals. Negative/nonzero origins are preserved. The final interval needs
  a measured terminal boundary with provenance; missing terminal time is not
  inferred from an average frame rate. Endpoint holding is explicit.
- Repeat identities use `(Repeat node, allocation revision, ordinal)`. Compact
  runs retain surviving identities through shrink, insertion, and reorder.
  Growth after undo uses a fresh allocation revision. At most 100,000 runs are
  allowed per Repeat; excessive fragmentation is rejected. Billion-play repeats
  do not allocate a billion identities.
- `InstancePath` validates the target and every ordered Repeat ancestor.
  Non-repeating grouping nodes do not change occurrence identity.
- `deadpan-plan` compiles node durations and sequence/run prefix indexes. Picture
  lookup maps project frame centers exactly through nested Retimes and resolves
  Source, Hold, Repeat, and Sequence providers. It records stable occurrence
  identity and distinguishes a gap from a play. Source-index selection checks
  original asset identity and timestamp clock. Inspection exposes lookup work
  and storage counts without decoding media.
- Document and SQLite schemas are version 2. `project migrate` retains a
  consistent schema-1 backup and replays a separate candidate with the original
  revision IDs. Strict legacy projection checks every old snapshot and forward/
  inverse transaction. History branches, cursor, and redo ordering are retained.
  After validation, SQLite's backup API promotes the candidate in a single
  destination transaction, preserving active readers and WAL semantics.

## Verification evidence

The complete repository gate passed on Apple M5 Max / arm64 macOS 26.5.2
(25F84), Rust 1.97.1, and bundled SQLite 3.53.2 on 2026-09-20:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo run -p deadpan-cli -- doctor
```

All **86 tests** passed with none ignored: 36 core, 23 store, 16 plan, 9 CLI,
and 2 native headless tests. `doctor` reports document schema 2 and marks the
implemented foundation partial; media playback, audio output, keyboard editing,
AI, import/export, and distribution remain unimplemented in the application.

The migration fixture [`v1-history.sql`](../crates/deadpan-store/tests/fixtures/v1-history.sql)
was produced by building and running unmodified commit
`f15ad742bf1e0ec47981bc3416d239760abc9621` from a `git archive` in isolated scratch
storage. It contains a Hold, repeat wrapping, shrink/growth, undo/redo, an
abandoned branch, insertion of an already-repeated subtree, and pending redo.
It was validated by that old executable before export. The current migration
code did not manufacture its history or patches.

[`migration.rs`](../crates/deadpan-store/tests/migration.rs) verifies the old
binary's entire chronology, stable identity allocation, retained backup,
surviving redo after reopen, live WAL readers, writer/SQLite lock contention,
and corruption rejection without promoting changed rows. Corruptions include
changed authored state, forged patch duration, a new command in old history,
duplicate JSON fields, mixed document schemas, and a damaged redo stack.
Promotion tests additionally exhaust SQLite's actual destination page limit and
terminate a child process during a partial backup transaction without running
destructors; reopening retains the original schema/data and passes integrity
checking. CLI failure tests verify the structured retained-backup path and
unchanged original database bytes.

Core unit/integration/property tests check compact reorder against an expanded
identity model, inverse/serialization round trips, shrink/growth without ID
reuse, nested path completeness, negative VFR boundaries, and checked exact
arithmetic. Plan tests exercise nested structural mapping and indexed seeking;
the CLI integration test resolves both plays and the intervening gap from the
migrated fixture while another writable store remains open.

Independent core review found a high-ordinal addition overflow after compact
play reorder/insertion. The fix subtracts the run origin before checked addition;
a regression uses `u32::MAX` original plays without expanding them. Independent
storage review found missing recovery-backup paths on migration failures; the
structured error now retains that path while preserving actionable storage codes.
Final identity review confirmed that externally supplied allocation names could
collide with future revisions. Insert now allocates fresh play identities for
every inserted Repeat, and imported initial allocations remain reserved through
shrink and reopen. Regressions cover both ingress paths and reject forged history
that reuses an initial allocation namespace.

The compatible native media boundary has separate actual-media evidence in
[its measured report](qualification/media-compatible-2026-09-20.md). Its accepted
working configurations and explicit negative cases remain distinct from these
pure mapping tests.

## Remaining work

Sparse per-occurrence overrides, persistent anchors/attachments and their edit
transforms, semantic selectors, fragment reuse/content hashing, audio mapping and
DSP, actual decoded/GPU pictures, preview/export integration, cache ownership,
recovery UI, and release qualification remain open. Opening future unknown
schemas for compatible read-only inspection is also outstanding.

No live GUI testing is useful for this pure timing/history slice. The native
welcome window was unchanged. Aesthetics, focus/IME, accessibility, and natural
keyboard navigation still require explicit review as the real editor develops.
