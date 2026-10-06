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
| Offered AI variants | `Media/Generated/blake3-…` | No while their request is current and the variant is present | Explicit cleanup once discarded or stale |
| Render candidates | `Media/RenderCandidates/blake3-…` | Yes after the job's movie is confirmed published, or its latest attempt ended Failed or Cancelled | Explicit cleanup |
| Unfinished writes | `.pending-*` in any namespace | Yes after the grace period | Explicit cleanup |
| Damaged copies | `.damaged-*` (set aside by restore) | Kept for diagnosis | Never |
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
  A discarded (`evicted`) or stale variant's receipt pins nothing.
- For render candidates, a checkpoint, attempt or publication of a job that
  can still need its candidate. A job releases it when its movie and report
  were published and confirmed, or when its latest attempt ended `failed` or
  `cancelled`. An explicit retry of such a job then reports its candidate
  missing, as the retry contract already requires it to rehash the retained
  objects. `interrupted` (abandoned by a crash, meant to be retried), active,
  verified-but-unpublished and `published_unconfirmed` jobs keep theirs.
- Every recovery checkpoint database under `Snapshots/` (SQLite sidecars
  excepted). A checkpoint is a restorable database, so everything it
  mentions stays pinned, without the live-receipt or render exceptions. A
  `Snapshots/` entry that cannot be read as a database makes cleanup refuse,
  because what it references is unknown; the report lists it.

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
| `project storage <pkg> [--grace-hours N]` | JSON report: database, each namespace's entries with state (`referenced` with reasons, `unreferenced`, `pending`, `damaged`, `unexpected`), sizes, what cleanup would remove; plus the per-user report. Read-only; works while the app has the project open. |
| `project storage <pkg> --clean [--dry-run] [--grace-hours N]` | Remove (or list) unreferenced objects and unfinished writes older than the grace period. Needs a closed project. |
| `project copy-portable <pkg> <new.deadpan>` | Verified self-contained copy. |
| `cache status` / `cache clean [--dry-run] [--grace-hours N]` | Per-user caches: proxies unused for the grace period, then least recently used ones while the cache exceeds its budget (the dry run lists both), and abandoned downloader staging. The grace period is at least one hour. Reports, never touches, model packs, AI runtimes and qualification weights. |

In the app, `:storage` or Deadpan › Storage… opens the Storage panel beside
the picture. It shows the project's and Deadpan's usage with accessible
"label: value" rows. P previews a project cleanup off the writer, R removes
exactly the previewed files that are still removable (refused without a
preview of the current session and revision), C cleans
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

DP-19 listed "source-clock/colour evidence" for accepted media. The bridge
context manifest records the plan, both prepared pictures' SHA-256 and the
stated colour interpretation (`rec709-sdr-full-range-rgb8-interpreted-as-srgb`);
it names no source asset, receipt or timestamp. Conditioning decodes the
frames on both sides of the pause from the request's origin revision through
the shared project picture path, and refuses HDR, 10-bit and rotated
pictures. The request binds the manifest's SHA-256 and the receipt retains
its bytes.

The offline test now checks this binding by re-derivation. On the portable
copy, with the source gone, `conditioning::prepare` at the request's origin
revision reproduces the manifest whose SHA-256 the request binds, and the
retained manifest object is byte-identical. The retained conditioning is
therefore exactly what the project's own committed pictures produce, and the
copy carries everything needed to show it.

What this does not establish, and what moves out of DP-19 into AI pause
qualification: the manifest still states the colour interpretation instead of
recording measured source colour metadata, the Lanczos fit and PNG encoding
are deterministic on this build but not specified across builds, and the
model's own colour handling is a worker claim. Accepted media portability does
not depend on any of these: rendering reads the verified FFV1 masters, never
the conditioning.

## Limits

- Cleanup is never automatic.
- Cleaning a project open in the app goes through the app; the headless
  command does not route through the live endpoint. Nothing schedules it, and the app's proxy
  builder keeps its own budget-driven cleanup.
- An unreferenced original copy is removable after the grace period. A
  retained but unregistered original is referenced by its inventory record and
  is kept.
- Physical power loss during removal or copying is not qualified; process
  crashes leave whole objects and the hidden staging directory.
- Network volumes without `flock` read without the reader lock.
- The scan reads every stored row; very long histories make the report take
  seconds. It runs off the UI thread in the app.
