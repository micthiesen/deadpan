# Schema 39 to 40 render-job migration map

Read-only inspection at `097f735`. No `native/**/AGENTS.md` files were found (`find native -name AGENTS.md -print` returned none). No repo files changed and no build/test/program was run.

## Exact wiring

- `crates/deadpan-store/src/schema.rs`: `VERSION=39`; `create()` assembles the fresh DB: authored `revisions/history/state/redo`, then operational generation requests/attempts, original media, source registrations, single-source profile. Schema 40 should add its operational render tables in the same creation path and increment only DB version. `check_version()` accepts 1..=38 as requiring migration.
- `crates/deadpan-store/src/migration.rs`: `ProjectStore::migrate()` accepts 1..=38, backs up to `Snapshots`, migrates a SQLite backup candidate and promotes atomically. `migrate_candidate()` is the only old-schema table-add path. For v39->40 append render-table installation for `source_version < 40` after old-only additions; do not run authored replay on v39.
- `crates/deadpan-store/src/validation.rs`: `validate_history()` calls current replay. `migrate_history()` only accepts versions through 38 (38 maps to core V32). Preserve that closed mapping; operational-only v39->40 migration should validate the existing current history, not rewrite it.
- Migration candidate tail (`migration.rs` ~247): currently validates generation requests, history, requests again, attempts, then original media, source registration, and single-source; sets `user_version` before final full validation. Add render stored-size and semantic validation in both normal open/validate (`lib.rs::ProjectStore::validate`, around 263-290) and migration candidate validation, following their separation (bounds before JSON parse; semantic table validation after history).
- Existing current-store create/validate paths are `crates/deadpan-store/src/lib.rs`: `create_inner` around 160-220; `validate()` around 263-292. `DATABASE_SCHEMA_VERSION` aliases `schema::VERSION`.
- Existing operational precedent: `crates/deadpan-store/src/generation_attempts.rs` owns DDL, bounded parse/validation, and store methods. Keep new render jobs separate from authored `history` and generation attempt identity/state unless their contracts explicitly bind them.

## Smallest genuine v39 fixture producer route

No checked-in v39 SQLite snapshot was found. Checked-in migration fixtures are SQL snapshots through v38 (`crates/deadpan-store/tests/fixtures/v38-gain-history.sql`); the 2026-09-29 generated-picture `fixture-reference.json` points to JSON picture evidence, not a DB. Existing genuine source preparation path is in `crates/deadpan-store/tests/single_source.rs`: `create()` uses `ProjectStore::create_single_source`, and `prepare()` uses a real tiny media fixture under `native/deadpan-source/tests/fixtures`, `retain_original`, verified copy, `SourceSession`/`AudioSession` decode, and `PreparedSourceRegistration::from_decoded`; test `full_original_is_atomic_and_undo_floor_survives_branch_and_reopen` already authors edits and exercises undo/redo. Existing generation request/attempt APIs and a compact end-to-end example are in `crates/deadpan-store/tests/generation_attempts.rs`: `allocate()` and `begin()` around lines 90-130.

Recommended producer is a narrowly scoped Rust integration-test/fixture producer, run against the untouched v39 source before bumping it. Reuse the above helpers: create single-source package, initialize measured Original, create at least two authored revisions, undo the final revision so one history row remains redoable, allocate a generation request tied to an existing Hold and begin/fail or ready an attempt, then call `store.validate()`, close the writer, and retain a SQLite backup with `Connection::backup` (plus `Media/Original` and `Snapshots` if migration checks need those objects). Record `PRAGMA user_version=39`, app id, row counts, `state.cursor`, redo positions, original receipt/hash and generation attempt identity/state. Avoid manually setting `user_version` on a DB made under schema 40: the old executable/test must produce the actual 39 DDL and row shapes.

Practical run command once that test/producer exists (parent owns implementation):

```sh
cargo test -p deadpan-store --test <fixture-producer-test> -- --exact produce_schema39_render_migration_fixture --nocapture
```

Keep its output under `/tmp/deadpan-render-jobs-95xpekq5/` or `tools/.../evidence/...`; do not make a fixture by editing a v38 SQL dump's pragma. A v39 producer can probably be an opt-in env path on the existing `single_source` test, but a dedicated test is clearer because it must intentionally leave a package and build valid operational generation evidence. For request creation, reuse the public flow from `generation_attempts.rs` test helpers rather than SQL inserts: `GenerationRequestInput`, `allocate_generation_request`, `BeginGenerationAttempt`, `begin_generation_attempt`.

Potential limitation: producing a fully valid request requires a Hold in the current document and complete relevance context; the simplest source of request setup is copy the setup from `generation_attempts.rs` (`allocate`, lines ~90-110) and adjust its store document to include a Hold, or use its existing request/attempt fixture logic. Do not use source-registration SQL rows directly; genuine qualification requires actual decoded-source admission.
