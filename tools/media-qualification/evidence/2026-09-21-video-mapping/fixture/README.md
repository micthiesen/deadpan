# Genuine schema-11 migration fixture

`v11-history.sql` was produced by archived source revision
`8a6c264e447615c585a2b9f849edfe44f51504fd`, whose doctor reports document schema 6
and database schema 11. The current implementation did not generate or relabel
its authored history.

`generate.py` archives that revision, rebuilds its CLI, migrates the committed
`v10-history.sql` fixture, then runs `schema11_fixture.rs` against the archived
store/core implementation. The harness uses public reconciled commit and history
navigation APIs with deterministic revision IDs. A second invocation reproduced
the committed SQL byte-for-byte. `manifest.json` records source, archive, binary,
script, harness, seed, and output hashes; `generation-output.txt` records the
commands and old-runtime validation output.

The fixture contains 24 revisions and 13 history entries. It retains the original
schema-10 source history, generated request/attempt/receipt rows, and original
inventory. New schema-11 chronology includes direct and occurrence audio edits
with exact durations 60000/1001 and 120000/1001 project frames, signed offsets
-137 and 2401 samples, an additional exact local mark, an abandoned branch, and
undo/redo ending with one pending redo. The old CLI reopens and validates that
final state before SQLite's backup API produces the dump.

Reproduce from the repository root:

```sh
python3 tools/media-qualification/evidence/2026-09-21-video-mapping/fixture/generate.py
```

The development-only generator uses Python, Git, and the pinned Rust toolchain.
It builds archived source and packages under `/tmp`; existing committed fixture
bytes must match exactly. Original and generated media bytes remain outside this
metadata migration fixture. These checks qualify schema replay and preservation,
not media readiness, playback, or application integration.
