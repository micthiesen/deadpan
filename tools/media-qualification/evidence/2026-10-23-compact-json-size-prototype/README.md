# Compact JSON size-counting prototype (private, unqualified)

This five-file private experiment adds an exact compact-JSON size counter with Store entry, final, and uncaptured counts. Captured checkpoint bytes, ordering, and limits remain unchanged; the Store checkpoint regression checks those invariants. This is a prototype, not a qualified production change.

## Checks

- Resumed build: six successful commands. The initial debug-cache clone was stopped with SIGTERM after 422.7 seconds, before formatting, compilation, or tests; it is retained as an interruption, not a product failure.
- Focused tests: 2 Core serialization tests, 9 binding tests, and 1 Store checkpoint test passed. Full Core/UI suites, strict Clippy, and full media/native qualification were not run for this prototype.
- Timing: five fresh-project control/prototype pairs (ten trials); all seven pause checks and one Kestrel routing check passed in each report. All twenty commands succeeded, authored project DBs were unchanged, WAL size was zero, and per-trial load1 was at most 3.

## Timing medians

| Measure | Control | Prototype | Difference |
|---|---:|---:|---:|
| Input to commit | 263.867166 ms | 254.805167 ms | 9.061999 ms (3.43%) |
| Input to offscreen background composition | 266.491583 ms | 257.379000 ms | 9.112583 ms (3.42%) |

These ten results are diagnostic medians, not p95 evidence or a target pass. The 100 ms fallback and 50 ms ordinary-edit targets remain open. No physical-display, source-decode, PCM, or listening performance claim is made.

## Identity and scope

- App SHA-256: prototype 561c2072b910e42d9defe29a502e0d7fe7055d37e120d889663fb8a253d5ee18; control 1864c3acd798d7ce68a8d3a62ee144ec6956cdc4750c568bf0001d7c2a67be4c.
- Five changed Rust files are named in the formatted-overlay audit. The full 2,050-file Rust source identity map and six input identities are retained. Original sources come from the frozen archive; formatted tested sources are retained separately.
- Core schema 58 and SQLite schema 87 are unchanged. Full product qualification remains open.

See inventory.json for retained paths, byte count, and digest.
