Store implementation complete for initial review/checks. No builds/tests/formatters/native programs run by worker.

New protocol: crates/deadpan-jobs/src/render/publication.rs (strict versioned/bounded immutable publication declaration, prepared evidence, phases/outcomes/operation history, reconciliation results).
New store: publication.rs plus publication/tests.rs, publication_durability.rs.
Hooks: lib.rs, schema.rs (41/fullfsync), migration.rs additive source40, error.rs. render_jobs exposes three existing bounded readers crate-internally for exact publication binding checks.
Tests: render_jobs/publication.rs (included in render_jobs integration target), migration/publication.rs authentic40 exact-cell comparison, synthetic legacy empty-table removal extended. Original schema40 SQL bytes unchanged.

Key invariants: retained movie claims derived from exact Verified attempt/checkpoint; sequential stage permits carry owner close and per-record epoch; COMMIT revokes old epoch before strict no-fallback database/WAL fullsync and package fsync. Any pin/barrier failure invalidates all publication permits and blocks publication writes until reopen. Publication writes never hash movie bytes or follow destination paths. Readonly open makes no recovery changes; writer reopen interrupts operations, preserves prepared/commit knowledge. Fresh verification is required for reconciliation; Failed/Cancelled cannot be reinterpreted as committed. Positive commit evidence survives missing report and later interruption. Full integrity audits are distinct from bounded selected-record reads.

Requested parent checks: cargo fmt; cargo check affected libs/tests; deadpan-jobs render::publication; deadpan-store lib (new fault tests), render_jobs, migration; strict Clippy and broader project checks as required. Parent native harness must establish actual destination identities/hash/rename/recovery behavior; metadata fixture tests do not qualify media or physical power-loss durability.
