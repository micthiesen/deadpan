# Recovery, autosave and storage failures

This records what DP-01 and the recovery parts of Gate F require
([specification 18.3, 20 and 28](spec/DEADPAN_SPEC.md#20-projects-history-and-recovery)),
what exists and how it is tested, and what remains. It covers the store and
the native app; headless commands keep their documented contracts.

## Inventory

| Requirement (spec) | Status | Implementation and evidence |
|---|---|---|
| Documents library and one-Original baseline (20.1, 20.2) | Implemented earlier | [Single Original](SINGLE_ORIGINAL.md); `library.rs` tests. |
| Every edit durable before "saved" (20.2) | Implemented, verified | One SQLite transaction per revision, WAL with `synchronous=FULL`. A killed writer keeps every committed revision (`deadpan-store/tests/recovery.rs`). |
| Interactive drafts transient until committed (20.2) | Implemented, now explicit | Camera, Gain, Room tone, Trim, Slip, Place slice and typed command text are never persisted. Closing the window with one open asks first; a crash loses them and the launch offer says so. |
| Detect unclean shutdown on startup (20.4) | **New** | Writer session marker in the package and the app launch journal (below). |
| Validate integrity on open (20.4) | Implemented earlier | Open hashes every stored row against the [verified history receipt](TIMING_STORAGE.md#verified-history-receipts); `project validate` replays everything. |
| Reconcile worker attempts on open (18.3, 20.4) | Implemented earlier, **now reported** | Writable open interrupts render, generation and publication attempts; `ProjectStore::open_recovery` lists them and the app shows them. |
| Offer the latest consistent state (20.4) | **New** | Launch offer reopens the project at its head revision; nothing is rolled back. |
| Native persisted render-job recovery (RENDER_JOBS) | **New entry point** on existing actions | The recovery report opens Renders on the interrupted job, whose existing actions verify a retained encoding or render the saved edit again. No retry starts automatically. |
| Disk-full and permission errors stop commits and show unsaved status (20.4, 26.3) | **New** messaging; failure behavior verified | Real ENOSPC, EROFS and EACCES tests; persistent "Not saved" alert; never "saved" after a failed transaction. |
| Missing media placeholder and relink (20.3, 26.3) | **New** native flow | Degraded open with a report, `:relink`, managed restore and linked relink verified against content identity. |
| Open newer unsupported schema read-only with an explanation (20.4) | **Implemented** | Writers refuse a newer schema as `SchemaNewer`; read-only opens view it without writing (header **Read-only**, edits refused with the reason); `project view`. Older development schemas still refuse. See [newer packages](BACKUPS.md#packages-a-newer-deadpan-saved). |
| Rotating consistent backups, including before migration (20.4) | **Implemented** | Verified backups every 15 minutes while editing, on close, on request, before every restore and before a release migration, rotated hourly/daily/weekly within a budget; restore from the Storage panel (`:backups`) and `project restore`. See [backups](BACKUPS.md). |
| Release migration policy (Gate F/G) | Policy and hook | [Release migration policy](BACKUPS.md#release-migration-policy): back up, migrate a copy, validate, promote atomically; exercised by a synthetic step and process kills. The production chain upgrades schema 66 to 67; [development formats](DEVELOPMENT_FORMATS.md) still refuse schemas 1-65. |

## Detecting an unclean exit

Two independent records, each owned by the layer that can know:

- **Package writer marker.** Every writable store records `.writer.session`
  beside `.writer.lock`, descriptor-relative and private, holding the process
  ID, open time and the package's device/inode. Dropping the store removes it
  before releasing the lock. A writable open first *reads* any marker: one
  for this package is reported as `unclean_previous_writer`; one naming
  another package's identity came from a copy made while its writer was open
  and is ignored; a link or directory there is reported as unreadable
  evidence. The new marker is written only after recovery succeeds, so a
  failed or crashed recovery leaves the earlier evidence for the next writer.
  Orphaned `.writer-session-*.tmp` files are removed. Failing to read or write
  evidence (for example on a full disk) never prevents opening; it is
  reported as `record_error`. The kernel releases the `flock` of a killed
  process; the marker stays. Read-only opens neither read nor change it.
- **App launch journal.** `~/Library/Application Support/Deadpan/session.json`
  (0600, atomic replace under `session.lock`) lists each running instance's
  process ID, launch identity and open project path (raw bytes, so non-UTF-8
  paths survive). An instance adds its record when a project opens and
  removes only its own record when the project closes or the app exits after
  its service has drained. At launch, a record whose process no longer exists
  and whose package is present shows **Deadpan did not close normally** with
  **Reopen** (Enter, focused) and **Not now** (Escape); either answer removes
  that record. A running instance is never offered, and a reused process ID
  only suppresses an offer. An explicit `--project` or `--preview-source`
  launch, the smoke test, tests and replay never read the user journal.

## What opening reports

`ProjectStore::open_recovery()` returns what writable opens found and nobody
has acknowledged. Each writable open merges its findings into
`Reports/recovery-pending.json` (bounded, atomic); a headless writer in between
cannot swallow them. `ProjectStore::acknowledge_recovery()` clears it; the app
acknowledges when the person answers the report. SQLite stays authoritative:
the file only summarizes attempts already recorded there.

| Field | Meaning | What the person can do |
|---|---|---|
| `unclean_previous_writer` | The last writer never closed. | Nothing is lost that was saved; unsaved previews are gone. |
| `interrupted_renders` | Active render attempts became `Interrupted` (`InterruptedOnOpen`). | **Open Renders** opens that job's attempts: **Save movie from attempt N** verifies a retained encoding without encoding again; **Render this saved edit again** encodes the captured revision afresh. |
| `interrupted_publications` | A destination save stopped; `movie_committed` when its rename had happened. | **Check previous destination** in Renders. Recovery never touches destination files. |
| `interrupted_generations` | AI pause attempts failed `interrupted`. | Select the pause and generate again (a new attempt). Accepted pauses are unaffected. |

Counts are exact; each list is bounded to 64 entries. The native service
attaches this report and a metadata-only presence check of every registered
original (`ProjectStore::original_availability`) to the session as
`OpenReport`. When it has something to say, the app shows one modal report
(**Project recovered**, **Original missing** or **Interrupted work**) with
its first action focused: **Locate Original…** (`:relink`), **Open Renders**
(`:renders`) and **Continue** (Escape). **Open Renders** opens one interrupted
job's attempts directly, or the saved-edit list when several jobs were
interrupted. `:recovery` shows the session's report again.

Other work has no persisted state to recover: tracking runs only in-session
and is started again; proxy builds publish atomically and abandoned staging is
removed by cache cleanup; an interrupted initial import leaves the explicit
Awaiting Source project, which **Choose Original** retries
([import preparation](IMPORT_PREPARATION.md)). Model-pack downloads belong to
the [model pack manager](MODEL_PACKS.md).

## Missing and moved Originals

Opening never fails because media is missing. The workspace, catalog and
history load from SQLite; only snapshots, previews, playback and Render need
the bytes, and they fail explicitly rather than inventing a placeholder.

`:relink` (or **Locate Original…**) captures the session, the original's
content identity and its location version, then asks for a file. With nothing
missing it targets the Original itself, so a copy that is present but damaged
(presence is checked from metadata only) is verified on demand:

- A **project-managed** original (the native default) is restored by
  `OriginalImportHandle::prepare_restore`. The chosen file is hashed first and
  must match the registered BLAKE3 identity and SHA-256, so a different file
  is refused before anything is written and the message names the chosen
  file. A retained copy that then fails its own verification (wrong length,
  changed bytes, writable or multiply linked) is moved aside to
  `.damaged-blake3-<digest>-<uuid>` and replaced; an intact copy is left
  alone. The record and its location version are unchanged.
- A **linked** original is relinked with the existing versioned
  `prepare_relink`, which verifies identical content and bumps the location
  version.

Both prepare on the import worker and commit on the writer; neither creates an
authored revision. Structural edits and Undo work with the Original missing,
since they never read media bytes. A wrong file reports *That file is not this
project's Original*; the matching file reports *found and verified* (or that a
damaged copy was replaced, or that the copy is intact) and refreshes the
picture and thumbnails.

### Moved linked files

Linked originals and sounds record a system bookmark when they are retained
(the import worker, `project retain-original --linked`, relinks), through the
[`deadpan-filesystem`](../native/deadpan-filesystem/src/bookmark.rs) adapter.
When a project opens and a linked file is missing at its recorded path, the
service resolves its bookmark (no UI, no mounting). A file the system finds
elsewhere is reported as *moved*, not missing, and verified on the import
worker with the ordinary versioned relink: it is relinked only if its length,
BLAKE3 identity and SHA-256 match exactly, with a fresh bookmark. The message
says where it was found. If the bytes differ, the record is unchanged, the
error says the file at that location is not the same content, and the report
shows the original as missing for `:relink`. Each candidate is tried once per
session, one at a time. `project relink-moved <p>` does the same for a closed
project. Bookmarks are regular, not security-scoped (the app is not
sandboxed), and only find files on mounted volumes; tests cover renames and
moves within a volume, changed content and records without a bookmark
(`deadpan-store` `bookmarks`, app `project::tests::backups`, CLI
`relink_moved`).

## Storage failures

| Failure | Store result | Verified by |
|---|---|---|
| Commit on a full disk | `DiskFull`; head and document unchanged; the same request commits once space returns | `storage_failures.rs` (16 MB APFS image), app `project::tests::recovery` |
| Checkpoint on a full disk | `DiskFull`; no partial file in `Snapshots` | `storage_failures.rs` |
| Original retention on a full disk | `DiskFull`; no partial object, no inventory row | `storage_failures.rs` |
| Package on a read-only volume | `ProjectReadOnly` (`ReadOnlyLocation`) for both writable and read-only opens, with a copy-it-elsewhere action; nothing written | `storage_failures.rs` (image attached `-readonly`) |
| Unwritable package and database | `ProjectReadOnly`; nothing written | `storage_failures.rs` (`chmod`) |
| Generated-media promotion on a full disk | `DiskFull`; no object and no `.pending-*` in `Media/Generated`; the project validates; the same promotion succeeds once space returns | `storage_failures_media.rs` |
| Render candidate retention on a full disk | `DiskFull`; nothing in `Media/RenderCandidates`; the same retention succeeds once space returns | `storage_failures_media.rs` |
| Seek proxy build on a full cache volume | The worker's own movie write (classified by the worker itself as `disk_full` from ENOSPC/EDQUOT) and the sidecar write at publication both fail as disk-full, an environmental condition never remembered as the Original's failure; no entry and no staging left; the same build publishes once space returns; a published proxy stays readable on a full volume | `deadpan-cli` `proxy_disk_full.rs` (cache on a 16 MB image, real worker) |
| Model pack volume fills during a download | The write fails with `No space left on device` (CLI code `DiskFull`); no finished file, receipt or active version; the kept `.part` is an exact prefix; while full the preflight refuses with `ModelPackSpace`; the install resumes from the kept bytes once space returns | `deadpan-models` `packs::disk_full_tests` (320 MB image filled by another writer mid-download) |
| Model pack volume fills during offline folder or archive import | Real ENOSPC after preflight preserves the previously verified installed version and receipt. The failed replacement has no finished payload, receipt or installed directory; retry stages and verifies fully after space returns. | `deadpan-cli` `model_pack_import_disk_full.rs` (320 MB APFS image, source on another volume) |
| Render publication to a full destination | `destination_full`; no movie at the destination; the verified candidate publishes once space returns | `encoded_verification` `publication` |

A WAL database needs writable shared memory even to read, so a project on a
read-only volume cannot be inspected in place; the error says to copy it.
Publication also reports `destination_read_only` and
`destination_permission_denied`; a full or read-only destination is named at
every publication stage (copy, report and durability), and quota exhaustion
counts as full here and in store codes.

In the app, store errors carry an explanation and an action
(`crate::recovery::describe_store_error`): *Not saved: the disk is full. Your
last saved edit is intact. Free space…*. The headline is the classification:
`storage_code` recovers it from the message on any thread, including import
worker replies, with SQLite's own wording as a fallback. The service checks
every failure it is about to publish (commands, copies and cuts, registers,
macros, slip/trim/splice commits, Render and its history, annotation saves,
imports and relinks) in one place, so a newly appearing storage failure always
raises the persistent **Not saved** alert and the header reads **Not saved**
instead of **Saved**. The alert stays through navigation and repeated refusals
and clears when a later save succeeds: a new head revision, or a save without
one (register bank, annotation, relink), or when the project closes. Editing
stays available; each attempt either saves or is refused truthfully. Obsolete
and newer schemas explain that nothing was changed and what to do.

## Closing with unsaved previews

A window close, including ⌘Q from Deadpan's menu, with an open Camera, Gain,
Room tone, Trim, Slip or Place slice draft, a typed command, an unfinished key
sequence, a macro being recorded, a typed YouTube address or a running YouTube
import shows **Close with unsaved previews?** listing them. **Keep editing**
is focused (Escape); **Discard previews and close** closes. Saved edits need
no prompt.

A system Quit from the Dock or logout reaches the app as
`applicationWillTerminate:`, which winit 0.30 cannot veto, so it closes without
the prompt and discards open previews. Exit still waits up to three seconds
for the project service to release the writer; only then does the journal
record a clean exit, otherwise the next launch offers the project again and
the writer marker reports an unclean close.

## Replays

`cargo xtask replays --scenario recovery,relink,storage-failure` drives the
production router, service and Metal picture path:

- `recovery`: saves a split, closes, leaves the on-disk state of a killed
  writer (its marker, an active automatic render attempt and a journal still
  marked open), then answers the launch offer with Enter, checks the reopened
  head revision and the report, opens Renders on the interrupted job from the
  focused button, reopens the report with `:recovery` and dismisses it.
- `relink`: deletes the managed Original copy, reopens to the degraded report,
  refuses a different file and restores the identical one with `:relink`.
- `storage-failure`: refuses one split as a full disk would (the only injected
  failure; real ENOSPC is qualified by the tests above), checks the message,
  alert and **Not saved** header through navigation, saves and clears it,
  then asks before closing with a Camera draft and keeps it on Escape.

On 2026-10-05 the three scenarios passed 37 checks in one run (recovery 14,
relink 10, storage-failure 13, each including its one Kestrel audit check);
relink passed its 10 again after the thumbnail fix below. Inspected captures show the launch offer with Reopen focused,
the recovery report, Renders opened on the interrupted job's attempts, the
missing-Original report and refusal, the restored picture, the header reading
**Not saved** beside the alert, and the close prompt over a Camera draft. Image
review found two defects that were fixed: the header still read **Saved**
during the alert, and cached failed thumbnails stayed black after a relink
(they are now dropped). The beat card's own thumbnail can still take a few
frames longer than the Original rail's to reappear.

After the independent review the three replays passed 37 checks again.

Other evidence: `deadpan-store` `recovery` (6: killed writer, retained and
acknowledged findings, copied/damaged/orphaned markers, missing and damaged
managed copies of equal and different length, linked relink) and
`storage_failures` (4, each disk proven full before use), the app's
`project::tests::recovery` (4, including edits and Undo with the Original
missing) and `recovery` unit tests (3: message classification, formats, the
per-instance journal),
and `encoded_verification` `a_full_destination_volume_publishes_nothing_and_keeps_the_candidate`.

On 2026-10-08 both offline-import disk-image cases passed under nextest and
ordinary `cargo test`; strict target Clippy also passed. The volume is filled
with real writes after the first file is copied and read for verification.
Small random byte fixtures exercise the normal CLI installer and PackStore,
without loading a model. Their previous version has a verified receipt but no
approved catalog selection; the tests prove that selection remains absent,
not replacement of an active production model. Nextest's disk-image group and
a process-local test mutex serialize the cases. The initial compile failure
used unsupported digest hex formatting; explicit byte formatting corrected it.
Logs are retained in `/tmp/deadpan-extension-native-20261008/offline-import-*`.

## Remaining work

- Moved files on unmounted or other volumes, and iCloud-evicted media
  ([File Provider domains](ORIGINAL_MEDIA.md#file-provider-domains)).
- Store errors wrapped in other error types keep SQLite's raw wording; the
  alert still recognizes SQLite's disk-full and read-only messages but not
  other raw I/O wording. Render workflow journal failures are classified only
  by those messages.
- The Dock/system Quit path cannot show the close prompt (see above).
- Physical power-loss behavior (owner: To verify). Process kills during
  commits, AI attempt states, backups, checkpoints, restores and migrations
  and proxy publication are covered ([process kills](BACKUPS.md#process-kills));
  kills during renders are not part of that suite.
- Physical-input, VoiceOver and IME acceptance of these dialogs.
