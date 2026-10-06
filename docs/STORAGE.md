# Storage, cleanup and portable copies

This page covers DP-19's reference tracking, explicit cleanup, portable
project copies and offline rendering of accepted AI pauses. Specification
Sections 19.2 and 20.1 set the storage classes. The code is
[`deadpan_store::storage`](../crates/deadpan-store/src/storage.rs),
[`deadpan_store::portable`](../crates/deadpan-store/src/portable.rs),
[`deadpan_cli::storage`](../crates/deadpan-cli/src/storage.rs) and the native
[Storage panel](../crates/deadpan-app/src/preview/storage.rs).

## Storage classes

| Class | Where | Evictable | Who removes it |
|---|---|---|---|
| Originals | `Media/Originals/blake3-…` | No while any row names it | Never automatic; orphaned copies (no inventory record or other row) after the grace period |
| Accepted AI media | `Media/Generated/blake3-…` | No while any retained revision names it | Never |
| Offered AI variants | `Media/Generated/blake3-…` | No while their request is current and the variant is offered; a variant that is not kept, picked, chosen or accepted stops being offered 7 days after it became Ready ([retention](#retention-of-unaccepted-ai-variants)) | The app's automatic check or explicit cleanup, once discarded, expired or stale |
| Render candidates | `Media/RenderCandidates/blake3-…` | Yes after the job's movie is confirmed published, or its latest attempt ended Failed or Cancelled | Explicit cleanup |
| Unfinished writes | `.pending-*` in any namespace | Yes after the grace period | Explicit cleanup |
| Damaged copies | `.damaged-*` (set aside by restore) | Kept for diagnosis | Never |
| Backups | `Backups/backup-*.sqlite` | By rotation only | [Backup rotation](BACKUPS.md#rotation); hidden `.staging-*` copies older than six hours are removed by rotation |
| Seek proxies | `~/Library/Caches/Deadpan/Proxies` | Yes | Explicit cache cleanup; also the proxy builder's own budget |
| Downloader staging | `~/Library/Application Support/Deadpan/helpers/.staging` | Yes once nothing in a download has changed for the grace period | Explicit cache cleanup |
| Model packs, AI runtimes, qualification weights | `Application Support/Deadpan/Models`, `Caches/Deadpan/ltx-*` | Reported only | `models remove`; never cache cleanup |
| Decoded PCM, pictures, thumbnails, limiter tiles | Process memory or anonymous temporary files | Gone with the process | Not on disk |
| Final exports | Beside the project in `Exports` | Never | The person |

## Reference tracking

An object is referenced when its 64-digit BLAKE3 digest appears in a stored
row that can still need it. `ProjectStore::storage_references` reads one
consistent SQLite snapshot and scans every text value of every table for
maximal runs of exactly 64 lowercase hexadecimal digits:

- Revision keyframes, history entries, navigation patches and Compound
  steps. A patch carries every value it installs, so a keyframe and the
  patches after it mention everything any revision contains, including undone
  revisions and abandoned branches. History pins its media.
- Registers and Macros, the original inventory, provenance and source
  qualifications, generation requests and attempts, and every other table.
- A generation bundle receipt only while it is *live*: present for a
  current request (it can still be chosen and accepted), or naming its own
  sampled, native or provenance object that another row already names (an
  accepted variant keeps all six objects, which picture admission verifies).
  A discarded or expired (`evicted`) or stale variant's receipt pins
  nothing.
- For render candidates, a checkpoint, attempt or publication of a job that
  can still need its candidate. A job releases it when its movie and report
  were published and confirmed, or when its latest attempt ended `failed` or
  `cancelled`. An explicit retry of such a job then reports its candidate
  missing, as the retry contract already requires it to rehash the retained
  objects. `interrupted` (abandoned by a crash, meant to be retried), active,
  verified-but-unpublished and `published_unconfirmed` jobs keep theirs.
- Every recovery checkpoint database under `Snapshots/` (SQLite sidecars
  excepted) and every published backup under `Backups/`. A checkpoint or
  backup is a restorable database, so everything it mentions stays pinned,
  without the live-receipt or render exceptions. An entry that cannot be read
  as a database makes cleanup refuse, because what it references is unknown;
  the report lists it. Hidden backup staging files are not scanned: a copy in
  progress names only objects the live database pins, and the grace period
  outlasts it.

The same scan also records *typed* references: serialized object references
(`{"algorithm":"blake3","digest":…},"byte_length":N`) and `blake3:<digest>`
content identities. Those name objects that must exist; portable copies
check them.

The scan is a deliberate superset. It never decodes a structure, so a new
table or field that names an object pins it without new code. A SHA-256 value
equal to an object's BLAKE3 digest would also pin it; over-retention is the
only possible error.

## Cleanup

`ProjectStore::clean_storage(CleanupPolicy)` needs the writable store, so no
commit, promotion or import of the same session interleaves with it. It holds
the package's render namespace lock while removing render candidates, so an
orphaned render worker of an earlier session cannot publish meanwhile. An
entry is removed only when all of these hold:

- it is unreferenced, or an unfinished `.pending-*` write;
- its modification and status-change times are both older than the grace
  period (default 24 hours, `DEFAULT_GRACE`). A write that has published bytes
  but not yet committed the row naming them is therefore never removed. APFS
  clones keep the source's modification time, so status change is included.
  macOS File Provider attribute updates also refresh it, which only delays
  removal;
- no reader holds it. Every object read (`verify_open_object_controlled`,
  used by generated, original and render snapshots) takes a shared `flock`
  for its duration. Removal takes an exclusive non-blocking lock and reports
  the entry `in_use` instead of waiting;
- it is still the file that was listed (device, inode, size and change time).

A write that deduplicates against an existing object (the same bytes
published again) restates the object's read-only mode. That refreshes its
status-change time, never its content or modification time, so the grace
period restarts before the new row naming it commits, and a removal planned
earlier sees the entry as changed.

Removal is one `unlinkat` under the exclusive lock, followed by a directory
sync. A crash leaves each object present or absent, never partial, and no
database row changes. A reader that opened the object first keeps reading
its complete bytes; a later reader finds no object and reports it missing. A
reader that had opened the object but was still waiting for its lock when
the object was unlinked also reports it missing, not damaged.
Every operation is relative to the package's pinned descriptors and never
follows a symbolic link. A dry run computes the same list and removes nothing.

`preview_storage_cleanup` lists the same candidates from any store,
read-only included. `clean_previewed_storage` removes exactly a previewed
list: an entry is removed only when its namespace, name, device and inode
match the preview *and* a fresh scan still finds it removable, so nothing
that appeared or was replaced since the preview is touched.

The native app previews on a read-only open in a worker thread, never on the
project writer, and discards the preview on any session or revision change.
Confirming sends the previewed list to the project service, which owns the
writer. There it rescans references (the safety check; it blocks other
project commands while it runs, typically well under a second and longer
for very long histories), waits at most two seconds for the render namespace
lock, and removes the intersection. The service refuses while an import,
relink, render, AI pause or tracking job runs, and always uses the default
grace period. `project storage --clean` refuses an open project. Because it
then holds the writer exclusively, it accepts `--grace-hours 0`; per-user
`cache clean` requires at least one hour, since other Deadpan processes write
those caches without a lock it can observe.

## Retention of unaccepted AI variants

Specification Section 19.2 makes unaccepted candidates evictable under a
visible retention policy. The code is
[`deadpan_store::generation_retention`](../crates/deadpan-store/src/generation_retention.rs).

Every Ready bridge variant has an operational retention record (database
schema 68, `generation_variant_retention`), written in the transaction that
records its receipt: when it became Ready, whether the person *kept* or
*picked* it, whether an acceptance of it committed or an expiry found it
named elsewhere, and, once its receipt is evicted, why (`discarded` or
`expired`) and when. `generation_retention_state` holds the watermark: when
the last expiry pass that trusted the clock ran. These records are not
authored history: Undo and Redo never change them, and they survive reopen,
backups, restores and portable copies with the database. A clock before
1970 records time zero instead of failing a Ready receipt.

An *offered* variant (a present Ready bundle of a current request)
**expires** `DEFAULT_VARIANT_RETENTION` (7 days) after it became Ready unless
it is:

- **kept**: `keep_generation_bundle_variant(identity, true)`, the app's
  Keep (`GenerationOperation::Keep`) or `keep-hold`. Un-keeping makes it
  expirable again, counted from when it became Ready, not from the un-keep;
- **picked**: the person explicitly chose it (`select_generation_bundle_variant`,
  which the app's Select and Preview and `accept-hold --attempt` use). The
  pick stays when a later Ready variant takes the selection, and moves only
  when the person picks another variant of the request, or ends with a
  discard. A variant that only the automatic selection protected loses that
  protection when a newer variant becomes the selection; the app then says
  so (`Job::unprotected`);
- its request's **selection**, which is what Preview and Accept use, so
  every request keeps at least the variant it would accept;
- **accepted**: flagged when an acceptance commits (Undo keeps the flag;
  the upgrade from schema 67 sets it from the rows that name each variant);
- **named elsewhere**: when a due variant's objects are named by any
  retained row other than its own receipt (history, a register, a
  checkpointed edit), expiry records that instead of expiring it, so later
  passes skip it without scanning.

Expiry has two steps. `plan_generation_expiry(now, retention, mode)` runs on
any store, read-only included, in one read snapshot: it finds due,
unprotected variants and, only when some exist, scans the non-receipt rows
for their objects. `apply_generation_expiry(&plan, dry_run)` runs on the
writer in one short transaction: it rechecks each planned row (still
offered, unprotected, unselected, same Ready time, due) and evicts it
exactly as Discard does, with reason `expired`, or records it as named
elsewhere; the whole-database scan never runs inside the write transaction.
`expire_generation_variants(now, retention, mode, dry_run)` does both on a
writer, for `project storage --clean`. A dry run reports the same list,
with each variant's object bytes (`expired_bytes`), and writes nothing.

An expired variant is no longer offered, cannot be selected, kept or
accepted, and its receipt no longer pins its six objects. Nothing is deleted
then: the masters stay until a cleanup finds them unreferenced and older than
the grace period, under every rule of [Cleanup](#cleanup). An accepted
variant's objects stay referenced by history and are never removed. Media
named by a published backup or recovery checkpoint stays pinned until that
copy is rotated away, so an upgrade's `before-migration` backup keeps the
variants it names for as long as it is kept.

### The clock

The policy runs on the wall clock, so an `Automatic` plan first checks it.
It expires nothing when the clock is before 1970, more than an hour
(`CLOCK_BEHIND_TOLERANCE`) before the newest recorded Ready, eviction or
pass time (`Behind`), or more than one retention period after all of them
(`Ahead`: a clock set forward, or a project not checked for a week). It
also caps itself at `MAX_AUTOMATIC_EXPIRY` (32) variants, oldest first;
the rest wait for the next pass (`deferred_by_cap`). An `Explicit` expiry
(`project storage --clean`) skips these checks and confirms the clock: it
records the watermark, so automatic passes resume. Every applied pass that
trusted the clock, including one that expired nothing, advances the
watermark.

In the app, a long gap (`Ahead`) can be confirmed from the Storage panel,
whose Clock row says which kind of anomaly the report finds. E (or the
Confirm clock button, shown only for a long gap) plans an `Explicit` expiry
on a read-only open, off the writer, and shows how many AI variants would
stop being offered and their bytes (`ExpiryPlan::preview`); nothing is
written. A second E sends exactly that plan
(`ProjectRequest::ConfirmVariantClock`). The service refuses it for another
session or revision, while any import, relink, render, AI pause, tracking
job or backup runs, or when the clock is behind the project's records; it
then applies the plan with every row rechecked and records the watermark,
as `project storage --clean`'s explicit expiry does. Files are left to
cleanup and its grace period. A clock behind the project's records
(`Behind`, including before 1970) is shown and never offered for
confirmation: correct the date and time, and expiry resumes on its own.

The storage report's `variant_retention` section states the policy and its
state: `retention_seconds`, `offered`, `kept`, `picked`, `selected`,
`accepted` (accepted or named elsewhere), `expiring` (offered and none of
those), `due` and `due_bytes` (past their expiry, waiting for a pass, with
their object bytes), `soonest_expiry_unix_seconds`,
`last_pass_unix_seconds`, `clock_anomaly` (what an automatic pass would
find now), the `discarded` and `expired` counts, and
`evicted_awaiting_cleanup` with its bytes (evicted variants whose own
objects are still present and unreferenced).

### Automatic check

A writable app session runs an automatic retention check when its project
service is first idle after opening or creating the project, then every six
hours of the session (`RETENTION_PASS_INTERVAL`), each time only while no
import, relink, render, AI pause, tracking job or backup runs (it waits and
reports `Deferred` with the reason). Each check

1. plans expiry on a read-only open in its own thread, off the writer;
2. on a clock anomaly, stops: nothing is expired and no file removed, since
   a wrong clock also defeats the cleanup grace period. It reports
   `ClockAnomaly` with what to do;
3. otherwise applies the plan on the writer (one short, rechecked
   transaction) and advances the watermark;
4. lists removable `Media/Generated` objects on another read-only open,
   using the generated-only cleanup scope (`CleanupPolicy::generated_only`,
   default 24-hour grace; originals, render candidates and unfinished writes
   are left to explicit cleanup);
5. only if that finds something, and once the writer is idle again, removes
   exactly those objects through `clean_previewed_storage_with`, which
   rescans references on the writer, honours reader locks and checks each
   file's identity, like a confirmed explicit cleanup. This rescan is the
   only whole-database scan on the writer, and it happens only when there is
   something to remove.

The check never takes the command admission, so it never makes a native or
remote command wait or fail as busy: it also waits while a submitted command
is pending, plans and scans off the writer, and its writer steps are short.
A check that changes nothing (deferred, running, or done with nothing
expired or removed) publishes no app update; its status travels with the
next one. Expiry, removal, a clock anomaly, a failure or the end of one is
published.

Its status (`ProjectUpdate::storage_retention`: deferred, running, clock
anomaly, done with variants expired and files and bytes removed, or failed)
appears in the Storage panel's AI VARIANTS section as "Automatic check",
with the policy rows and a Clock row when the report finds an anomaly. A
failure changes nothing and is retried at the next check.

## Portable copies

`copy_portable(source, destination)` builds a new package that needs nothing
outside itself:

1. The source opens read-only, so the editor may keep it open. SQLite's
   backup API copies one consistent database snapshot. References and the
   original inventory come from the *copied* database. The source's media is
   listed only after that snapshot: an object a snapshot row names was
   published before the row committed, so it is listed unless it has since
   been removed.
2. Each original becomes a managed copy: an existing managed copy is cloned
   (APFS) or copied, and a linked original is read from its location. Both
   verify against the recorded BLAKE3 and SHA-256 identity through the
   ordinary retention path. The link is then dropped from the copy's record.
   A missing original refuses the copy.
3. Every referenced generated object is copied through a verified snapshot.
   Every typed reference must then resolve to an original record or a copied
   object of the recorded length; a reference whose object vanished refuses
   the copy instead of producing one that is not self-contained. Discarded
   and stale variants, unreferenced objects, unfinished writes and all render
   candidates stay behind. Render job records remain as history; a retry in
   the copy reports its missing candidate.
4. The copy is reopened read-only. Its complete history is recomputed
   (`validate --full`, including the hash chain), every copied object and
   original is re-verified, and every asset of the head revision must have
   its media in the copy.

The copy is assembled under a hidden sibling
`.<name>-<uuid>.partial.deadpan` and appears at the destination only through
one no-replace rename after verification. A failure or cancellation removes
the staging package. A crash can leave the hidden staging directory, never a partial
package under the requested name. An existing destination is refused, never
merged.

## Commands and app

| Command | Effect |
|---|---|
| `project storage <pkg> [--grace-hours N]` | JSON report: database, each namespace's entries with state (`referenced` with reasons, `unreferenced`, `pending`, `damaged`, `unexpected`), sizes, what cleanup would remove, the history's own size (`history`: revisions, edits, keyframes and their bytes) and checkpoint, backup and report bytes (`auxiliary_bytes`); plus the per-user report. Read-only; works while the app has the project open. |
| `project storage <pkg> --clean [--dry-run] [--grace-hours N]` | First expire offered AI variants past the retention period, confirming the clock (`variant_expiry`; a dry run only lists them with their bytes), then remove (or list) unreferenced objects and unfinished writes older than the grace period. Needs a closed project. |
| `keep-hold <pkg> --request <id> --attempt <id> [--off]` | Keep one offered AI variant so it never expires, or with `--off` let it expire again. Operational, not undoable; needs a closed project. |
| `project copy-portable <pkg> <new.deadpan>` | Verified self-contained copy. |
| `cache status` / `cache clean [--dry-run] [--grace-hours N]` | Per-user caches: proxies unused for the grace period, then least recently used ones while the cache exceeds its budget (the dry run lists both), and abandoned downloader staging. The grace period is at least one hour. Reports, never touches, model packs, AI runtimes and qualification weights. |

In the app, `:storage` or Deadpan › Storage… opens the Storage panel beside
the picture; `:backups` opens it on its [backups](BACKUPS.md#app). It shows the project's and Deadpan's usage with accessible
"label: value" rows. P previews a project cleanup off the writer, R removes
exactly the previewed files that are still removable (refused without a
preview of the current session and revision), E reviews then confirms the
clock after a long gap since the last retention check. Its AI VARIANTS section shows
the retention policy, offered, kept, picked and chosen variants, when the
next one expires (and the bytes of those due), discarded and expired
variants awaiting cleanup with their bytes, any clock anomaly, and this
session's latest automatic check. C cleans
rebuildable caches and S (also File › Save Portable Copy… and
`:portable-copy`) saves a verified portable copy on a background thread
while editing continues. Escape closes it.

## Offline rendering

Rendering an accepted AI pause reads only the project's verified objects; it
never consults a model pack, runtime, proxy, helper or the network.
[`offline_portable`](../crates/deadpan-cli/tests/offline_portable.rs) proves
it with the real `deadpan-cli`:

- it builds the `black_pause` recipe, generates two variants with the
  synthetic worker (real conditioning, host qualification, six-object
  publication and acceptance; only the model is replaced), accepts one and
  discards the other;
- every storage, render, copy, validation and verification command runs
  under `sandbox-exec` with outbound IP denied, a cleared environment, `HOME`
  pointing at an empty directory and every `DEADPAN_BRIDGE_*` variable
  pointing nowhere;
- `project storage` lists exactly the discarded variant's three masters as
  unreferenced, and cleanup removes them;
- `render` publishes and verifies the movie, and `verify-export` matches the
  pause's generated frames and its neighbours against the committed preview;
- `project copy-portable` copies the project elsewhere; after the source
  package and its directory are deleted, the copy validates, renders and
  verifies the same way;
- the empty home is still empty afterwards.

## Source-clock and colour evidence

DP-19 listed "source-clock/colour evidence" for accepted media. Conditioning
decodes the frames on both sides of the pause from the request's origin
revision through the shared project picture path. The bridge context manifest
(schema 2) binds the plan and both prepared pictures' SHA-256, declares the
model's colour space (full-range sRGB, BT.709 primaries, RGB) and records, for
each side, what the picture path showed:

- an Original frame: its asset, receipt (qualification) ID, measured index
  identity, exact source PTS and time base, and the decoder's measured codec,
  pixel format, dimensions, SAR, rotation, decoded bit depth, transfer,
  primaries, matrix and range;
- a frame of an accepted generated Hold: its sampled asset, sampled master and
  provenance objects and the same measurements of that master;
- authored black (Background/Blank), with nothing decoded.

Each decoded side also names the conversion applied for the model: the
decoder's full-range RGB8 (declared matrix and range applied) passes its codes
unchanged, either sRGB codes as sRGB or BT.709-transfer codes read as sRGB, a
stated approximation. Conditioning refuses, with the reason, any picture this
does not cover: rotated pictures, PQ/HLG HDR, sixteen-bit decodes,
linear-light transfer, and BT.2020 or Display P3 primaries (no gamut or
transfer conversion exists). Every matrix and range the source decoder admits
is covered because the decoder applies it. The request binds the manifest's
SHA-256 and the receipt retains its bytes.

Qualification requires the declared model colour space to equal the canonical
masters' full-range sRGB BT.709 RGB, which the FFV1 converter writes and
verifies and the candidate/accepted master decoders re-check; a mismatch fails
as a colour interpretation mismatch. Contexts captured before schema 2 state
only `"srgb"` and an interpretation string; they remain admissible because
their bytes are bound by hash, and carry no measured source evidence.

The re-derivation checks: on the portable copy, with the source gone,
`conditioning::prepare` at the request's origin revision reproduces the
manifest whose SHA-256 the request binds, and the retained manifest object is
byte-identical (offline test). `tests/bridge_conditioning.rs` compares the
recorded colour, pixel format, geometry, frame identity and PTS with an
independent decoder of the fixture and with a fresh picture-path preparation
at the origin revision, including a generated neighbour, and checks the HDR
refusal. Because the manifest changed grammar, a request conditioned by an
earlier build cannot gain another variant (`--another`); generating again
creates a new request. Accepted and Ready bundles are unaffected.

What this does not establish: the Lanczos fit and PNG encoding are
deterministic on this build but not specified across builds; the BT.709-as-sRGB
reading is an approximation, not a transfer conversion; and the model's own
colour handling is a worker claim. Accepted media portability does not depend
on any of these: rendering reads the verified FFV1 masters, never the
conditioning.

## Limits

- The only automatic cleanup is the app's retention check (after opening,
  then every six hours while idle), limited to unreferenced
  `Media/Generated` objects past the grace period. Originals, render
  candidates and unfinished writes are removed only by explicit cleanup. A
  session that is never idle defers it.
- Cleaning a project open in the app goes through the app; the headless
  command does not route through the live endpoint. Nothing schedules the
  headless command, and the app's proxy builder keeps its own budget-driven
  cleanup.
- The retention clock is the machine's wall clock. Automatic checks refuse
  a clock behind recorded times or more than a week past the last check,
  but a clock set ahead by less than a week shortens the period by that
  much. Opening a project after more than a week away also defers automatic
  expiry until the Storage panel's E or `project storage --clean` (with the
  project closed) confirms the clock. There is no
  per-project or per-user setting for the period.
- An unreferenced original copy is removable after the grace period. A
  retained but unregistered original is referenced by its inventory record and
  is kept.
- Physical power loss during removal or copying is not qualified; process
  crashes leave whole objects and the hidden staging directory.
- Network volumes without `flock` read without the reader lock.
- The scan reads every stored row; very long histories make the report take
  seconds. It runs off the UI thread in the app.
