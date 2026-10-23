# Compact frozen-edge test refinement draft

The full proposed test file is `compact_frozen_edges.rs.txt`. It is a scratch-only draft copied from the current repository test and was not applied.

## Exact behavioral changes

- `explicit_nondefault_allowed_edges_survive_exactly` now uses asymmetric node policies (`node_start: hard`, `node_end: automatic`) and checks that decoded policy and re-encoded JSON preserve that asymmetry.
- The mixed layout sets Source placement start to Hard/end to Automatic, and Repeat gap start to Automatic/end to Hard. It checks decoded exact values and JSON equality for both typed frozen node kinds. Automatic root/Hold edges remain omitted.
- The context regression embeds the historical automatic and asymmetric compact JSON strings as fixed constants. It checks current context capture equals the historical automatic fixture, direct serde parity, and for each fixture under schema headers 1 through 12, exact `to_json()` bytes. The empty document matches the automatic fixture and does not match the asymmetric fixture.

## Golden provenance

`historical-codec-provenance.json` is the original `historical-codec-probe/receipt.json` byte-for-byte. It records base `a18ff65443aa8fd1aa5ab63fa33678c9afd88220`, old Core rlib SHA-256 `532aa006494ea2c3e8050c1bf14d460d4c550f3ed3cf67b38992ae9b4832673b`, and probe source SHA-256 `d846a174a77fd0643055899e59cc90c1202aa59572c062c8a64330e6384192e0`. Embedded fixture bytes are copied from `historical-codec-probe/automatic-golden.json` (SHA-256 `ae2c176815ef7947e2d451c52cce2d10e560ac9cce52ef640c8fb6b882a30e40`) and `asymmetric-golden.json` (SHA-256 `f07e03024c41da162c5c68fc67e43714f64f90b3531edf47dfb51e62a1f3bf54`).

No Cargo, formatter, or runtime command was run. The only checks were reading the source/golden files and reviewing the draft diff.
