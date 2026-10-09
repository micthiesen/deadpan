# Backups, newer packages, release migrations and history size

This covers four DP-01 items from [specification 20.2 and 20.4](spec/DEADPAN_SPEC.md#20-projects-history-and-recovery):
rotating consistent backups, opening a package a newer Deadpan saved,
the release migration policy and its implemented hook, and history size.
It also records the process-kill evidence for these paths. Recovery from a
crash, missing media and storage failures are in [RECOVERY.md](RECOVERY.md);
reference tracking and cleanup are in [STORAGE.md](STORAGE.md).

Code: [`deadpan_store::backups`](../crates/deadpan-store/src/backups.rs),
[`deadpan_store::migration`](../crates/deadpan-store/src/migration.rs),
`ProjectStore::open` (newer schemas), [`deadpan_cli::backups`](../crates/deadpan-cli/src/backups.rs),
[`deadpan_cli::backup_settings`](../crates/deadpan-cli/src/backup_settings.rs),
the app's [backup service](../crates/deadpan-app/src/project/service/backups.rs)
and [Backups section](../crates/deadpan-app/src/preview/backups.rs).

## What a backup is

A backup is one standalone SQLite database under `<project>.deadpan/Backups/`,
named `backup-<UTC time>-<reason>-<8 hex>.sqlite`. It is copied from one
consistent read snapshot through SQLite's backup API, so it includes every
committed WAL page and never copies the open main file by itself (20.2). Its
journal mode is DELETE: it has no sidecar files. Media stays in the package
(originals, accepted AI media); a backup is a database recovery point, not a
portable copy ([portable copies](STORAGE.md#portable-copies) are separate).

Copying never uses the project writer. `create_backup` opens its own
read-only connection, pins a read transaction and copies bounded page batches,
so the writer keeps committing meanwhile (its later commits are simply not in
that backup). The commit path is unchanged.

Before a backup gets its final name it is **verified on its own**:

- SQLite's complete `PRAGMA integrity_check`;
- every stored-size bound, `quick_check`, foreign keys, the history hash chain
  against its [verified history receipt](TIMING_STORAGE.md#verified-history-receipts)
  (replaying anything the receipt does not prove) and every operational table,
  exactly the checks opening a project performs;
- the copy must hold the head revision captured when copying began, and have
  exactly the captured number of pages.

Only then is it renamed (no-replace) from a hidden `.staging-<uuid>.sqlite`
and the folder synchronized. A failure, cancellation or full disk before the
rename publishes nothing and removes the staging file. After the rename the
backup is published: a failed folder sync or rotation is reported in the
outcome's `warnings`, never as a failed backup, and the backup just published
is always kept by its own rotation. A process killed while copying leaves at
most the hidden staging file, which rotation removes after six hours. The
copy's deadline (at most an hour) is checked between page batches and phases,
not inside SQLite's verification calls, so a very slow verification can
overrun it; six hours leaves a wide margin.

Backups pin media like `Snapshots/` checkpoints: every object a published
backup mentions stays referenced, so cleanup never removes media a restore
would need ([reference tracking](STORAGE.md#reference-tracking)). Pinning
fails closed: if the backups cannot be listed, cleanup refuses. A copy in
progress holds `Backups/.backups-active.lock` shared from before its snapshot
until publication; cleanup takes it exclusively while it computes references
and removes objects (waiting at most two seconds, then refusing), so no
snapshot can name an object removed meanwhile. A dry run never waits.

Before copying, a backup checks the volume can hold the database plus a
256 MiB reserve and otherwise refuses with `DiskFull`, so a backup never takes
the space the project's next commit needs. The format is checked before
anything is created: a newer or older package gets no `Backups/` folder.

## When backups are taken

| Reason | When | Where |
|---|---|---|
| `periodic` (Automatic) | At the configured interval while the database changed since the latest backup began; the first one interval after opening a project that has none (15 minutes by default) | App backup thread |
| `close` (When closed) | A project closes, another opens, or the app quits, with changes the latest backup does not hold | Detached thread; quitting waits at most 1.5 s for it |
| `manual` (Backed up by you) | `B` in the Backups section, `project backup` | App backup thread, CLI |
| `before-restore` | Before every restore, of the state being replaced | Store, inside `restore_backup` |
| `before-migration` | Before a release migration rewrites the database | Store, inside the migration runner |

"Changed" means any commit, authored or operational: transcripts, pause and
shot analysis, corrections, registers, render and AI state all count. The
service watches SQLite's `data_version` on its own idle read-only connection
(`ChangeMonitor`), which changes whenever another connection commits; the
value read just before a copy starts is the one that copy covers.

One app backup runs at a time. Its failure (for example a full disk) is shown
in the Backups section and retried later, each consecutive failure doubling
the wait (up to sixteen intervals); it never blocks
editing or raises the "Not saved" alert, because no edit was refused. A close
backup still copying when the app quits is abandoned with the process and
publishes nothing. Read-only sessions (newer packages, below) are never backed
up.

Storage cleanup takes no backup: it removes only unreferenced media, which a
database backup could not bring back.

## Rotation

After regular backup publications, under an exclusive `flock` on
`Backups/.backups.lock`, rotation keeps:

- the 8 newest backups;
- the newest per UTC hour for the last 24 hours, per day for 14 days and per
  week for 8 weeks;
- the 4 newest `before-restore`/`before-migration` backups and the 4 newest
  manual ones;

then trims that set to the configured count and storage budget from the
oldest. The defaults are 48 backups and 4 GiB. The newest backup is always
kept even if it alone exceeds the byte budget. A restore tells rotation to
keep the backup it is restoring. Removed backups are listed in the creation
result.

### Per-user policy

The Backups section exposes the same settings in every project state,
including when no project is open or the current project is read-only. The
values belong to the current macOS user, not to an individual project, and
never create an edit or history entry. The CLI uses the same settings file and
validation:

```text
deadpan-cli backup-policy show
deadpan-cli backup-policy set --interval-minutes 15 --max-count 48 --budget-mib 4096
deadpan-cli backup-policy reset
```

The file is `~/Library/Application Support/Deadpan/backups.json`. It is
strictly validated, atomically replaced, and stored with mode `0600`. If it is
missing, Deadpan uses defaults without creating it. The interval accepts 1 to
1,440 minutes, the count accepts 8 to 256, and the budget accepts 256 to
65,536 MiB. The default values are 15 minutes, 48 backups and 4,096 MiB. The
hourly, daily, weekly, manual and safety retention buckets remain as described
above.

If settings cannot be read or validated, Deadpan reports the problem and
continues automatic backups at the default 15-minute interval while keeping
all existing backups. It does not apply an uncertain retention policy. The
CLI's `project backup` response includes the same warning. Saving new settings
changes the effective policy only after the atomic replacement succeeds; a
parent-folder sync failure is reported as a warning because the replacement
already happened.

`before-migration` is a special safety copy. It uses the raw backup path for a
database this build cannot validate and never rotates or removes existing
backups, regardless of the per-user settings. This preserves all recovery
points while the package format is being rewritten.

## Restore

`ProjectStore::restore_backup` needs the writer:

1. Find the backup by its exact id (file name without `.sqlite`); paths and
   unknown names are `BackupNotFound`.
2. Open it read-only and verify it completely (as above). A damaged copy is
   `BackupInvalid`; a backup of another project is `BackupOtherProject`. The
   project is unchanged in both cases and no safety backup is taken.
3. Check the volume has room for about three copies of the database (the
   safety backup, a private copy and the WAL) plus the reserve, then back up
   the current state as `before-restore`.
4. Copy the backup into a hidden private file and record in it, in
   `retired_identities`, every identity the replaced database issued that the
   backup lacks: revision and Compound step identities, AI generation request
   identities, the highest request version per Hold and the highest location
   version per original. Allocation consults that table, so no identity, Hold
   request version or location version is ever issued twice, restore or not.
5. Revoke every capability the writer session handed out (import, generated
   and render handles, checkpoint handles, writer ownership and the live
   command endpoint's discovery), as closing would.
6. Copy the private file into the live database through the backup API. The
   destination write transaction spans every page batch, so the live database
   is either entirely replaced or unchanged, also when the process dies part
   way.
7. Validate and recover exactly as opening does: the restored history is
   certified, attempts the backup recorded as running become interrupted, the
   open recovery report is retained and a fresh session marker is written.
   The writer and its lock stay; the app binds a new command endpoint.

History becomes the backup's history: edits after the backup are gone from
this database but remain in the `before-restore` backup, so restoring that
backup undoes the restore. Operational state returns to the backup's as well:
AI generation requests and their relevance, attempts, render jobs and the
publication journal, except that identities and versions the replaced state
issued stay retired (step 4). A stale request naming a discarded revision is
refused as reused, not merely as not the head.

The copy commits before step 7. If validating or recovering afterwards fails,
the error is `BackupRestoredUnverified`: the database is already the backup,
the store refuses further writes, and the app closes the session and asks for
a reopen; the error names the `before-restore` backup.

### A database that no longer opens

Restoring through the writer needs a database that opens. For one that does
not, `project restore <p> <id> --damaged` (`replace_damaged_database`) takes
the package's writer lock and the backups lock without reading the damaged
database, verifies the backup, and requires it to belong to the project that
`manifest.json` names (when the manifest is unreadable,
`--force-project <id>` must name the backup's project). It copies the backup
to a hidden file in the package and verifies that copy again, moves
`project.sqlite` and then its WAL files together into a new
`.damaged-project-<uuid>/` folder, and renames the copy into place. Nothing
is deleted, and a failed copy leaves no temporary file. A crash after the
first move leaves no `project.sqlite`, so opening fails loudly rather than
reading a main file without its WAL; running the command again moves any WAL
still beside it into its own quarantine before installing. Identities the
damaged database issued cannot be carried forward (it cannot be read).
Open the project normally afterwards. The app does not offer this yet.

The app refuses a restore while
an import, relink, render, AI pause or tracking job runs, then opens the
result as a **new session**, so drafts, selections, copies and caches of the
replaced revisions cannot reach it, and relinks moved files if needed.

## App

`:backups` opens the Storage panel on its BACKUPS section alone; `:storage`
shows it after the usage rows. Rows read "12 min ago · Automatic · 2.1 MiB",
newest first. J/K or the arrows choose a backup and the panel reads what it
holds off the UI thread: "Revision … · 42 beats · 3:12.0 long · 120 edits".
B backs up now. O asks first ("Restore the backup from 12 min ago (…)? It
replaces the project, history included; what you have now is backed up first.
Press O again to restore, or J/K to cancel.") and a second O restores. Escape
closes. Listing and previews use read-only opens on worker threads, never the
writer. Storage's usage rows count backups with checkpoints and reports and
show the history's own size (below).

The replay `cargo xtask replays --scenario backups` drives this through the
production router, service and Metal path: `:backups`, B, an edit, choosing the
backup, the confirming O, the restoring O (new session at the backup's
revision with a `before-restore` backup), then the newer-package view below.

## Headless

| Command | Effect |
|---|---|
| `project backups <p> [--verify]` | Every backup, newest first, with what it holds (`--verify` runs the complete verification on each). Read-only; works beside the app. |
| `project backup <p>` | One verified manual backup, then rotation. Works beside the app (it uses its own read connection). |
| `project restore <p> <id> [--expected <rev>] [--dry-run \| --damaged]` | Restore, as above; `--damaged` replaces a database that no longer opens. `--expected` refuses unless that is the head revision. With the app open, it restores on the app's writer through the live endpoint, which starts the app's new session; the reply comes from the replaced owner before its endpoint stops. `--dry-run` verifies and describes the backup and writes nothing. |
| `project view <p>` | Read-only summary that also opens newer packages and says why they are read-only. |

## Packages a newer Deadpan saved

A database whose `user_version` is above this build's is refused by writers,
validation, migration, checkpoints and backups as `NewerSchema`
(`SchemaNewer`): "This project was saved by a newer Deadpan (database schema
67; this build supports 66). It can only be opened read-only; nothing in it
has been changed". `ProjectStore::open(…, ReadOnly)` opens it for **viewing**:

- no writer lock, no WAL change, no recovery, no receipt write and no session
  marker; the database and WAL bytes are unchanged (tests compare them, also
  with a newer build still holding an uncheckpointed WAL). Like any SQLite
  reader of a WAL database it uses the shared-memory index (`-shm`), which it
  may create or update; (A writable
  open attempt may first roll back a hot rollback journal, below, before it
  sees the newer schema and refuses: crash recovery, not a new write);
- this build cannot validate a later build's history or operational tables,
  so it checks SQLite's `quick_check` and that the head document reads and
  validates under this build's document model. If it does not (a newer field,
  for example), opening fails with "this project was saved by a newer Deadpan
  … and this build cannot read it (…); nothing was changed";
- `newer_schema()` reports the version; `validate`, the storage report and
  cleanup refuse with `NewerSchema` (a later build's tables may name objects
  this build cannot see).

The app's Open falls back to this view: the header reads **Read-only** (hover
and accessibility text give the reason) instead of **Saved**, and the open
message says so. Commands that persist changes are refused with
"Not saved: …" before reaching the store (background analysis saves use the
store's own read-only refusal). Mark jumps, face detection, Room tone
preparation and private Gain, Slip and Trim previews remain available.
Place Slice preparation still requires store writer admission, and generation
Preview persists the selected variant, so both are refused. It has no live command
endpoint and no backups; media is read through a viewing handle that cannot
publish objects. The timeline, inspector, pictures and audition work as far as
this build understands the document. Render needs a writer (render jobs are
stored), so exporting a newer package is not available; copy it with a newer
build or use that build.

Every refused action with a pending control returns its captured identity
through the normal response channel. This covers marks, macros, corrections,
targets, generation, registers and cuts, proposal commits, cleanup, relinking,
Render and saved-render history. Controls leave their pending state while
retaining the previous register or private draft. New request variants must
choose an explicit admission and completion policy in the service's exhaustive
match. A refusal never starts a workflow or commits a preview.

The 2026-10-08 service regressions cover captured and stale identities,
repeated Render, preview commit, Cancel and both history queries, plus real
read-only queries and private previews. They compare every retained package
file, including database, WAL, registers and media; SQLite shared-memory
coordination is excluded. All 25 focused tests and strict app Clippy pass.
The production keyboard/service/Metal replay passes 50 checks, including
repeated read-only copy, target, Slip and cleanup actions with usable controls
after refusal. Its fixture waits for the previous writable session's owned
backup workers before comparing package bytes. See the
[qualification record](qualification/readonly-controls-2026-10-08.md) for
earlier failed assertions, the full app suite and verification limits.
This refusal handling does not implement export from newer packages.

A package an interrupted rollback-journal write left behind (a hot journal,
only possible during a release migration's switch out of WAL mode) is rolled
back by the next writable open under the package lock; a read-only open says
to open it for editing once.

## Release migration policy

The production chain `deadpan_store::migration::MIGRATIONS` holds one step,
66 → 67, which adds the empty `retired_identities` table so the previous
build's packages keep opening. Opening a schema-66 package returns
`MigrationRequired` without writing; native Open and `project migrate` then
run the step. Under the 2026-09-30 development-format authorization every
older development schema is still refused as `SchemaUnsupported` before a
writer, backup or parse ([development formats](DEVELOPMENT_FORMATS.md)). From
the first release, each database schema change must ship as one step in that
chain, and `project migrate` / native Open run them through the implemented
runner (`migrate_package_with`):

1. **Own the package.** Take the writer lock; an open editor refuses it.
   Remove hidden `.migrating-*` leftovers of an interrupted earlier run.
2. **Back up first.** Publish a raw `before-migration` backup of the old
   database in `Backups/` (backup API, integrity-checked; this build cannot
   validate an older schema's history, so the raw check is SQLite's
   `integrity_check` and the application identity). This publication does not
   rotate existing backups. Restoring it needs the build that wrote it.
3. **Migrate a copy.** Copy the database to a hidden staging file beside it and
   apply each `N → N+1` step, in order, in one transaction on that copy, ending
   at the target `user_version`. Steps must be contiguous; a gap is
   `SchemaUnsupported`.
4. **Validate invariants.** The target build's complete validator runs on the
   copy: `integrity_check`, foreign keys, full history replay and every table.
5. **Promote atomically.** Fold the old WAL into its main file and leave WAL
   mode (so no stale WAL can be replayed onto the new file), rename the copy
   over `project.sqlite`, and synchronize the package directory.

Any failure before promotion leaves `project.sqlite` byte-for-byte as it was
(tests compare the bytes); a failure inside promotion, after the WAL was folded
and WAL mode left, leaves the old content in a DELETE-mode file that the next
writable open returns to WAL and returns `MigrationFailed` with the retained
backup; a newer package is never migrated down. A process kill leaves either
the old database (still openable for writing) or the new one, never a mixture.
Release migrations must also keep accepted generated media usable without its
model (20.4) and must not invent identities for old formats (20.2).

The runner is exercised by a synthetic `66 → 67` step that adds a table
(`migration::tests`): backup published with the old schema, new schema and
table present, this build then sees a newer package and views it read-only
with the same document, a second run is a no-op, failing steps and a rejecting
validator leave the bytes unchanged, an open writer and a missing step refuse
without writing, and only the previous schema has a production step.

The real 66 → 67 step is tested on a synthetic package (store
`development_break`: open asks for migration without writing, the backup holds
schema 66, the history replays, undo works; app: native Open upgrades with a
backup and editing and undo continue) and on a package the schema-66 build
itself created on 2026-10-06 (Original registered, one edit, one published
render): after `project migrate`, `project view`, `render status` (the render
job is listed), `project undo` and full validation work. No schema-66 package
existed among the owner's projects (they are schemas 17 to 59, still refused).
Old-schema backups report "holds database schema N, from before an upgrade"
under `project backups --verify`.

## History limits

There are none, by decision: specification 20.2 requires history to remain
intact, so every revision is kept and there is no undo-depth limit. The
database keeps a keyframe document every 64 revisions plus one patch per
revision. The storage report and the Storage panel show the history's size
(`history.revisions`, `edits`, `keyframes`, `keyframe_bytes`, `patch_bytes`;
"History: 1,204 revisions · 18.2 MiB (16.9 MiB in 19 keyframes); all kept").
Rotating backups bound how much of it a damaged database could lose.

## Process kills

[`chaos_kills.rs`](../crates/deadpan-store/tests/chaos_kills.rs) re-executes
itself as a child that keeps a writer busy and is SIGKILLed after a seeded
random delay, repeatedly on the same package:

- **Commits, AI attempts, backups, checkpoints.** The child commits edits,
  starts AI generation attempts and advances their stages (in a second
  package, whose pauses carry the requests), publishes backups with rotation
  and writes checkpoints. After each kill: opening validates; every commit the
  child reported is in the history and the head is that commit or the one cut
  off before its report; no reported attempt is still running after a
  writable reopen; every published backup verifies; every checkpoint passes
  `integrity_check`; the complete history replays at the end.
- **Restores.** The child restores two backups back and forth; afterwards the
  head is one of the two states and every backup, including the automatic
  `before-restore` ones, verifies.
- **Migrations.** The child runs the synthetic release migration on a fresh
  copy each round; afterwards the package is either the old database (opens
  for writing, same document, history replays) or the new one (viewed
  read-only, same document). This found that a kill during the switch out of
  WAL mode left a hot rollback journal that read-only probing could not
  recover; writable opens now roll it back under the lock.

**Deterministic windows.** [`failpoints.rs`](../crates/deadpan-store/tests/failpoints.rs)
aborts a child at named crash windows (debug builds only; release builds
compile the failpoints away) and checks the child reached each one:

| Failpoint | Afterwards |
|---|---|
| `backup-after-rename` (published, folder not yet synchronized) | The new backup is listed and verifies; the project is unchanged |
| `restore-mid-copy` (inside the copy transaction) | The project is unchanged and validates; the safety backup verifies |
| `restore-after-copy` (copy committed, not yet revalidated) | The project is the backup, validates, and the replaced head's identity stays retired |
| `migration-after-wal-fold` (old file out of WAL mode, not renamed) | The old database opens for writing with the same document |
| `migration-after-rename` (renamed, folder not synchronized) | The new database; viewed read-only with the same document |
| `damaged-after-main-moved`, `damaged-after-quarantine` | No `project.sqlite`, opening refuses; a rerun completes, leaving no WAL beside the new file |

The regression run is 8 kills per test with a fixed seed;
`DEADPAN_CHAOS_SEED=<hex>` and `DEADPAN_CHAOS_ITERATIONS=<n>` widen it (a
40-kill run passed on 2026-10-06).

- **Proxy publication.** [`proxy::cache::tests`](../crates/deadpan-cli/src/proxy/cache_tests.rs)
  kills a child that keeps staging, writing and publishing multi-megabyte
  proxy movies over six keys. Every visible entry afterwards opens with its
  verified hash and holds exactly one publication's bytes; cleanup still
  works (a 30-kill run passed). The media worker's own encoding is a
  separate supervised process whose output is only staged; killing it is
  covered by the worker supervision tests, and full disks by the
  [disk-full tests](RECOVERY.md#storage-failures).

- **Render host during encoding and verification.** The
  [2026-10-08 qualification](qualification/render-host-crash-2026-10-08.md)
  kills the production coordinator after matching real worker and public
  progress. Both helper groups exit through their ordinary control-pipe EOF,
  reopening reports Interrupted attempts and authored history is unchanged.
  The verification case retains its exact checkpoint; retry runs a fresh
  verifier and publishes the same movie without another encoder.

## Limits

- A process kill is not a power loss: the kernel still writes back cached
  pages. Physical power loss during commits, backups, restores and
  migrations is not qualified (owner: To verify).
- Backup cadence and the maximum count and size are per-user settings, shared
  by the app and CLI; the age and reason retention buckets remain fixed. There
  is no per-project setting.
- Restore is whole-database: operational state (render jobs, AI attempts,
  registers) returns to the backup's too. Media published after the backup
  stays in the package and is cleaned only once nothing references it.
- Restoring a `before-migration` backup needs the build that wrote it.
- Backups live inside the package: they protect against damaged databases
  (`--damaged`) and mistakes, not against losing the disk or the package. Portable copies and
  Time Machine cover that.
- Restore, `project restore` and a migration run on the writer and block other
  commands of that project while they copy and verify.
