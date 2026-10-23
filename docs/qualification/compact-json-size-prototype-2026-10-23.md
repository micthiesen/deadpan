# Compact JSON size-counting prototype, 2026-10-23

The isolated prototype passes twelve focused tests and ten guarded native
timing replays. Five control/prototype pairs give input-to-commit medians
263.867166 /254.805167 ms and offscreen Background composition
266.491583 /257.379000 ms. The latter difference is 9.112583 ms, about 3.42%.
These are diagnostic medians, not percentile or performance-target PASSes.
The 100 ms fallback and 50 ms ordinary-edit targets remain open.

## Change and scope

`ProjectDocument::compact_json_size` uses the same compact serializer and
64 MiB bound as `to_compact_json`, with a counting sink instead of a complete
output buffer. It preserves limit-error precedence and other serialization
errors. It does not validate, cache a size or change the wire vocabulary.
Existing field serializers can still allocate; peak memory was not measured.

Store counts its entry, final and uncaptured staged documents. Captured
checkpoints retain their existing serialized bytes. Processed-byte charges,
step bounds, checkpoint charges and insertion order remain unchanged. Core 58
and SQLite 87 remain unchanged. Three production files and two test files
differ from source base `8dbea5ac77343a172bc4a4fd58bf34b015866b53`.

Two Core counter tests cover escaping, UTF-8, exact limits and overflow; nine
compact-binding cases include the new nested-binding size comparison. One
Store case compares captured/uncaptured checkpoints with the materializing
reference and verifies persisted bytes, Undo/Redo, reopening and registers.
All twelve pass. A read-only draft review reported no concrete defects.
The locked release build passes. Full Core, broader/UI, strict, media/native
and portable production qualification remains owed.

## Timing and identities

Every trial creates a fresh 10,000-Hold package and runs the seven production
`large-first-pause` checks plus one Kestrel routing check. All twenty make/run
commands pass. All Rust/input/binary hashes remain guarded, authored package
paths and bytes remain unchanged except SQLite coordination sidecars, and
WAL length remains zero. Pre-run load1 is at most 3. These samples are a
separate batch from the qualified leaf-result comparison.

| Boundary | Control median | Prototype median | Difference |
| --- | ---: | ---: | ---: |
| Input to commit | 263.867166 ms | 254.805167 ms | 9.061999 ms (3.43%) |
| Input to offscreen Background composition | 266.491583 ms | 257.379000 ms | 9.112583 ms (3.42%) |

The control is the qualified app
`1864c3acd798d7ce68a8d3a62ee144ec6956cdc4750c568bf0001d7c2a67be4c`;
the prototype is
`561c2072b910e42d9defe29a502e0d7fe7055d37e120d889663fb8a253d5ee18`.
The complete 2,050-file Rust map, six input identities and five original and
formatted source snapshots are retained. No production timers were added.
No physical-display, source-decode, PCM, listening or calibrated-overhead
claim follows from this Background-only workload.

## Retained setup history and evidence

The initial development-cache clone was deliberately stopped with SIGTERM
after 422.7 seconds, before formatting, compilation or tests. The resumed
driver preserves that partial private cache and completes six successful
steps. This is a setup interruption, not a product-test failure.

The first packager refused before creating output because its suffix lookup
matched multiple `Cargo.lock` files. Root retained that attempt and corrected
the trusted archive lookup to use exact paths. The corrected packager exits
zero. All product sources, focused results and timing trials stay unchanged.

The [evidence package](../../tools/media-qualification/evidence/2026-10-23-compact-json-size-prototype/)
contains 161 files /1,362,070 bytes. Inventory SHA-256:
`1f7875c23dde0259269d3054a67ff3bb7f195f3709fcc294452f2e5268d2ac51`.
Root independently reread every path, byte length, hash and gzip stream.
Executables, targets, the source archive, generated packages and media are
excluded. The source, build and measurement owners in
`/tmp/deadpan-compact-json-size-2026-10-23` are terminal; do not restart them.
