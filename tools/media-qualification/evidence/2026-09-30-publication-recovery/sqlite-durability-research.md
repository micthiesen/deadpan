# SQLite durability and schema-41 publication journal research

Scope: read-only source inspection at `6a0d82e7cdae060eba4db3d900072d705a035e71`, the pinned SQLite source, and SQLite/Apple primary documentation. No build, tests, formatter, or native program was run. The authentic schema-40 fixture is owned by the parent and was not changed.

## Finding

The store already requests the standard SQLite WAL durability contract: creation selects WAL and `synchronous=FULL` in `schema::create`; writable open repeats both settings in `ProjectStore::open`. SQLite documents `synchronous=FULL` in WAL as ACID and says it adds a WAL sync after each commit. That is enough to make committed publication phases recoverable after ordinary process termination and is the normal SQLite durability contract.

It does **not** establish that macOS `F_FULLFSYNC` succeeded. The bundled SQLite VFS deliberately hides that distinction: `PRAGMA fullfsync=ON` makes SQLite request the stronger sync flag, but the bundled Unix VFS falls back from failed `F_FULLFSYNC` to `fsync()` and returns success when the fallback succeeds. Reading back `PRAGMA fullfsync=1`, checking `synchronous=2`, or observing `SQLITE_SYNC_FULL` therefore proves configuration/request, not a successful drive-cache flush.

For the narrower requirement “do not issue a report/movie rename until this exact phase commit has passed a strict macOS flush,” the smallest publication-specific addition is a fail-closed direct barrier after the SQLite transaction commits and before the store returns the phase permit. Reuse the direct `rustix::fs::fcntl_fullfsync` approach already used by the encoded-publication filesystem code, and sync both SQLite files because auto-checkpoint can leave the latest committed row in either the WAL or the main database. Then `fsync` the pinned package directory and re-check the opened descriptors against the named entries. If any open, identity check, sync, or directory sync fails, do not issue the permit and do not rename. Keep the SQLite transaction itself on `synchronous=FULL`.

This boundary is distinct from SQLite's standard FULL contract. It is a fail-closed extra check for the publication transition; it cannot prove that hardware honors flush requests under physical power loss.

## Source and API evidence

- Workspace `Cargo.toml:43` pins `rusqlite = 0.40.2` with `bundled`; `Cargo.lock` pins `libsqlite3-sys 0.38.2` (checksum `f1d20bef...`). The cached crate's bundled header reports SQLite `3.53.2`. Its `build.rs` compiles `sqlite3/sqlite3.c`; it does not use the host's SQLite library. The target SDK headers determine whether `F_FULLFSYNC` is defined.
- `crates/deadpan-store/src/schema.rs:10-15` is the shared connection configuration, currently with no `fullfsync` pragmas. `schema.rs:39-42` sets `journal_mode=WAL` and `synchronous=FULL` before creating the schema.
- `crates/deadpan-store/src/lib.rs:209-239` configures and validates all opened connections, and repeats WAL/FULL on writable opens. `lib.rs:365-380` uses SQLite's online backup API for a consistent checkpoint, then `sync_all()` on the snapshot file and snapshots directory.
- `crates/deadpan-cli/src/encoded_render/publication/filesystem.rs:844-852` already uses direct `fcntl_fullfsync` on macOS, with no fallback, and plain `fsync` on Linux. Its commit order at lines 501-546 is file sync, no-replace rename, published-file sync, directory sync, then a final file sync. This is a local example of an explicit fail-closed durability boundary.
- In pinned `libsqlite3-sys-0.38.2/sqlite3/sqlite3.c`, `sqlite3PagerSetFlags` at lines 63246-63256 maps `PRAGMA fullfsync` to `SQLITE_SYNC_FULL`; `checkpoint_fullfsync` sets the checkpoint WAL sync flag separately. In `full_fsync()` at lines 43965-44016, the `HAVE_FULLFSYNC` branch calls `F_FULLFSYNC` and, on any error, calls `fsync(fd)`. If that succeeds, `unixSync()` returns success. This is the silent fallback.
- SQLite docs distinguish `PRAGMA synchronous=FULL` from the `SQLITE_SYNC_FULL` VFS flag. FULL controls **when** SQLite invokes xSync; the VFS flag selects Mac-style fullsync on supported platforms. See [SQLite synchronous and fullfsync pragmas](https://sqlite.org/pragma.html#pragma_synchronous), [SQLite fullfsync pragma](https://sqlite.org/pragma.html#pragma_fullfsync), and [SQLite sync flags](https://sqlite.org/c3ref/c_sync_dataonly.html).
- [SQLite WAL documentation](https://sqlite.org/wal.html) says FULL syncs the WAL on every commit, while checkpointing syncs WAL before copying pages and syncs the database before resetting/reusing the WAL. The [checkpoint pragma documentation](https://sqlite.org/pragma.html#pragma_wal_checkpoint) says FULL mode waits for readers to the latest snapshot, copies all frames, and syncs the database. `checkpoint_fullfsync` only affects checkpoint synchronization; when `fullfsync` is on, SQLite says that setting is irrelevant. Neither pragma changes the source VFS fallback.
- SQLite's [xSync contract](https://sqlite.org/c3ref/io_methods.html) defines `SQLITE_SYNC_FULL` as Mac-style fullsync, but it does not expose to the caller whether the VFS used a fallback. Apple's archived [fcntl(2) reference](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fcntl.2.html) describes `F_FULLFSYNC` as `fsync` plus a request for the drive to flush buffered data to permanent storage, and explicitly notes that some drives have ignored such requests.

## Minimal barrier API

Add one synchronous store-side method around publication phase commits, rather than asking each CLI stage to improvise durability:

```text
commit_publication_phase(txn, next_phase) -> PublicationPermit
    validate exact publication / operation / token / expected-sequence
    txn.commit()?                                  // synchronous=FULL remains enabled
    strict_publication_db_barrier(package_pin)?    // error => no permit
    mint live permit for this committed sequence
```

`strict_publication_db_barrier` should use the existing package-directory pin/identity pattern (or introduce an equivalent pin held for the writable-store lifetime):

1. Open `project.sqlite` and `project.sqlite-wal` relative to that pinned directory with no-follow and close-on-exec flags. Require regular files, same expected device/owner, and expected single-link policy. Fail closed if the WAL is absent while a WAL writer connection is active.
2. Compare each opened descriptor's identity with its current directory entry before syncing. Directly call `fcntl_fullfsync` on **both** descriptors on macOS. This call returns an error instead of silently substituting ordinary fsync. Sync both because SQLite's default 1000-page passive auto-checkpoint can copy committed frames into the main DB during the commit; after a checkpoint the latest value may no longer be solely in WAL.
3. Call `fsync` on the pinned package directory so any current WAL directory entry is covered. Re-check the package chain and that both descriptor identities still match their names. Any replacement/change/error denies the permit.
4. The CLI must consume that exact live permit immediately before the existing descriptor-relative no-replace rename. Keep the permit private, bound to session/publication/attempt/token/sequence/hash, and revoke it on store close or superseding transitions. The writer stores bounded metadata only; it never reads/hashes the movie.

The code already has `rustix` as a macOS/Linux dependency of `deadpan-store`, and the publication filesystem already makes the direct macOS `fcntl_fullfsync` call. Centralize the syscall helper rather than depending on the CLI crate from the store. On Linux, use `fsync` and describe the guarantee as the platform's ordinary VFS durability contract; `F_FULLFSYNC` is macOS-only. Do not claim a strict Apple-style flush on a platform or filesystem where the direct syscall is unsupported.

The two file descriptors must be tied to the actual store/package namespace, not blindly opened from an unchecked string path. A package-directory descriptor and captured database identity should be retained or revalidated; `O_NOFOLLOW` on the leaf alone does not prevent a replaced parent directory. If the database/WAL path or identity changed, fail closed. Since `Connection` does not expose its active VFS file descriptor through a safe rusqlite API, this path-based barrier relies on the store's exclusive writer lock plus pinned package and inode/name checks. If the implementation cannot establish that binding robustly, the alternative is a small custom VFS/native adapter that fails xSync on an F_FULLFSYNC error; simply setting the PRAGMA is not that alternative.

`PRAGMA fullfsync=ON` in shared `schema::configure` is a reasonable low-cost baseline for create/open/migration writer connections and makes SQLite request F_FULLFSYNC for commits and checkpoints. Check its readback and keep `synchronous=FULL` explicitly set on each writer connection. It improves normal SQLite behavior, but is not a substitute for the direct barrier because of the bundled fallback. `checkpoint_fullfsync=ON` adds no value when fullfsync is already on, and a full checkpoint is not needed for the publication barrier if both main DB and WAL are directly fullsynced. Avoid disabling auto-checkpoint globally just to simplify the barrier; it can let WAL growth become unbounded.

The guarantee is conditional. F_FULLFSYNC success means the OS accepted the request; it cannot defeat a lying device/controller, sudden hardware failure, unsupported/remote filesystem behavior, or corruption outside SQLite's assumptions. The standard SQLite FULL guarantee is still the proper general claim; phrase the extra publication barrier as “direct F_FULLFSYNC calls succeeded on the package database/WAL files and directory sync succeeded,” not as proof of arbitrary physical power-loss behavior.

## Schema-41 hooks without rewriting schema-40 chronology

Current `schema::VERSION` is 40 and `check_version` treats only `1..=39` as migration-required. Current `ProjectStore::migrate` accepts only `1..=39`, and `migrate_candidate` unconditionally creates render-job tables before history processing. Simply admitting version 40 into the existing generic branch would attempt to recreate those tables and replay/revalidate a chronology that schema 40 already owns.

Use an explicit source-40 migration branch inside the existing backup/candidate/promotion framework:

- Bump `schema::VERSION` to 41; classify versions `1..=40` as migration-required.
- New project creation creates the new publication tables in the same initial schema transaction and sets user_version 41.
- For source version 40, retain the existing pre-upgrade backup, copy to candidate, run integrity/FK checks and full schema-40 render/history validation, then create **only** publication tables, set user_version 41, validate the old stores plus publication tables, commit the candidate and use the existing SQLite backup promotion. Do not call `migrate_history`, re-create `render_jobs`, or rewrite any schema-40 JSON/attempt/checkpoint bytes.
- For source versions 1..39, keep their current migration path to current render-job storage and add the publication tables before setting version 41; version 39's special `validate_history` branch remains intact. Existing current-schema validation and strict size checks must include the new rows.
- Add publication validation and writable-open recovery adjacent to render-job validation/recovery in `ProjectStore::validate` and `ProjectStore::open`. Read-only open validates but performs no recovery. Recovery marks live/unresolved publication operations as needing reconciliation; it never touches destination files or treats stored report/Verified rows as media authority.
- The existing create layout already contains `Media/RenderCandidates` and `Reports`; no package directory change is necessary unless the new schema adds a new internal artifact namespace. `ProjectStore::checkpoint` and the pre-upgrade backup already use SQLite's backup API, so they include committed WAL state consistently.

There is an authentic schema-40 SQL fixture captured by the parent at `crates/deadpan-store/tests/fixtures/v40-publication-render.sql`; use it for an exact 40-to-41 migration assertion that keeps revision/history/render rows byte-for-byte unchanged and adds no inferred publication record.

## Fault and crash tests

1. **Configuration and VFS behavior:** assert each writer connection reports `journal_mode=wal`, `synchronous=2`, and (if adopted) `fullfsync=1`. In a macOS VFS/OS fault-injection harness, fail the F_FULLFSYNC operation while allowing fsync to succeed; verify pinned SQLite still returns commit success. This locks in the documented implementation limitation and prevents tests from treating PRAGMA readback as proof.
2. **Fail-closed direct barrier:** inject failure independently at main DB fullsync, WAL fullsync, directory fsync, and before/after identity checks. The journal commit may be visible when reopened, but no permit and no report/movie rename may occur. Also test missing WAL and replaced database/WAL/parent entries.
3. **Ordering:** record test events and assert `SQLite COMMIT -> direct DB/WAL F_FULLFSYNC -> directory fsync -> permit acknowledgement -> report rename`; repeat for movie. The commit-intent transaction must be acknowledged before entering the rename path. Ensure the final external publication sync sequence still runs after rename even if cancellation arrives.
4. **Process crash:** use a subprocess with deterministic pause/failpoints and kill it (SIGKILL / abrupt child termination, not graceful store drop) immediately after phase commit/barrier, before rename, after report rename, after movie rename, and before recording post-rename outcome. Reopen read-only and writable stores. Assert SQLite integrity, durable last-known phase/outcome, and no automatic rename/delete/adoption. Reconciliation must freshly verify the retained checkpoint and compare destination evidence.
5. **Power-loss claim:** process termination tests do not clear kernel/page/device caches and are not evidence of power-loss durability. Only describe them as process-crash/recovery tests. A real power-loss qualification needs a supported local filesystem/device setup and controlled hard-power interruption or an appropriately faithful block-device fault harness; report OS, filesystem, storage device, F_FULLFSYNC return observations and outcome. Even that only qualifies tested configurations.

## Sources

- [SQLite PRAGMA reference](https://sqlite.org/pragma.html#pragma_synchronous)
- [SQLite WAL](https://sqlite.org/wal.html)
- [SQLite WAL checkpoint pragma](https://sqlite.org/pragma.html#pragma_wal_checkpoint)
- [SQLite sync flags](https://sqlite.org/c3ref/c_sync_dataonly.html)
- [SQLite file-method xSync contract](https://sqlite.org/c3ref/io_methods.html)
- [Apple fcntl(2) reference](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fcntl.2.html)
