# Pre-selector range capture fixture

`range-v1.json` is literal output from commit
`2ea675646abc24e8410616af74b4193ed4af51da`, generated before whole-child capture
changed the core. It was copied byte-for-byte from the isolated baseline run,
not regenerated with the new serializer or command implementation.

SHA-256: `ea862010de94871087d282da55f982c0b967abb9f06d688af64f1dd7a5de6aca`

The fixture contains the range-only capture, source document, deletion request
and transaction, and three placement cases: seam, interior and replacement.
Each case contains complete before/request/transaction/after JSON strings plus
the old writer's literal revisions, history, state and redo rows. Core schema34
and database schema43 were used. The helper fixture includes partial Hold,
Repeat/gap context, retained clocks and an owned mark.

The scratch generator was appended only to the baseline copy of
`crates/deadpan-store/tests/edited_slice.rs`. The original prefix still hashed to
`20247de412eb31a6856996e53fc4e85a04a330e91555c2630065a1f45bd9fc23`.
The root agent ran:

```sh
DEADPAN_RANGE_FIXTURE_OUTPUT=/tmp/deadpan-structural-capture-20261001/old-range \
  rustup run 1.97.1 cargo test -p deadpan-store --test edited_slice --locked \
  export_pre_selector_range_fixture -- --exact --nocapture
```

The single generator test passed and also left three closed, validated old
packages in that scratch directory. The checked-in compatibility test restores
literal old rows into a fresh package shell, validates and reopens it, and runs
Undo/Redo while requiring old history and revision JSON to remain unchanged.
The expected transactions come exclusively from this retained file.
