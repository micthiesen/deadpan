# Original media ownership

`deadpan-store::original_media` retains complete original files independently of
editorial asset registration. This preserves every container stream and metadata
byte. It does not transcode to preview resolution, infer an audio endpoint, or
create an `AssetRecord` with missing stream facts.

## Managed and linked originals

`retain_original` defaults to managed ownership at the headless boundary. It opens
the explicit absolute local path nonblocking, requires a regular file, and hashes
the entire file with BLAKE3 and SHA-256 under byte/time/cancellation limits.
BLAKE3 is the internal address; SHA-256 binds the existing source-index envelope.
Names are labels, never content identity or package filenames.

Managed originals live at `Media/Originals/blake3-<digest>`. The shared object
storage engine tries a descriptor-based APFS clone, then uses positional copying
when cloning is unsupported or crosses filesystems. Both paths verify complete
content and length, detect source changes, make the object read-only, synchronize
the file and namespace, and publish without replacing an existing name. A
preexisting object must independently verify before it can deduplicate a retry.
The narrow [`deadpan-fileclone`](../native/deadpan-fileclone/) adapter owns the
unsafe system call; it follows Apple's [clone API contract](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/clonefile.2).

Linked retention records an absolute path, optional bounded opaque bookmark,
and complete content identity. The current path API requires UTF-8, rejects
parent traversal and final symlinks, and does not resolve bookmarks itself.
Native bookmark creation/resolution and document dialogs remain open.

`relink_original` requires the expected monotonic location version and verifies
the entire replacement file. Identical content can gain a new path/bookmark;
different content is rejected without changing the record. Deliberate replacement
with different media requires a future explicit authored rebind operation.
Managed and linked locations may coexist; a managed snapshot uses the owned copy.

Linked retention and relinking compare the final named file and open descriptor
with the metadata captured after hashing, including size and modification/change
timestamps. An external file can still change after that check; every later use
must create another verified snapshot.

Every `snapshot_original` returns private verified bytes. Managed snapshots check
both recorded checksums; linked snapshots copy and check the currently located
file. Moving or changing that file cannot change an already returned snapshot.
Missing or corrupt media returns an error. No placeholder is silently rendered.

## Database and failure boundaries

Database schema 11 retains the `original_media` table introduced in schema 10. Content identity is its key;
location versions and bounded records are operational, outside document undo.
An authored `AssetId` is unsuitable as a location key because it can be reused
after its registration is undone. Original retention does not change project
revision, timeline state, or an existing `AssetRecord`.

Durable filesystem publication precedes the SQLite transaction. A database
failure leaves a verified unreferenced original available for retry. It never
deletes user data to simulate cross-filesystem/database atomicity. Records are
written only by the project writer; read-only inventory and snapshots may coexist.
Inventory uses bounded keyset pages. All retained originals are currently kept;
history-based reference collection and eviction policy are not implemented.

Schemas 1 through 10 migrate on a consistent backup/copy into schema 11.
Schema-10 original records are preserved. For schemas 1 through 9, the new
table is created without `IF NOT EXISTS`, so legacy files containing unexpected
modern tables fail rather than acquire implied trust. Existing document history,
generation requests, attempts and admission receipts retain their prior meaning.
The pre-migration backup remains available.

Temporary-object cleanup is best effort. Once an entry's device/inode is captured,
failure cleanup removes only that observed pending name if its identity still
matches. If identity inspection itself fails after cloning, the unidentifiable
entry is retained. No cleanup routine claims to isolate a hostile same-user
process that replaces an entry before its first identity observation.

Limits are cooperative: checks surround hashing/copying and native work. A single
filesystem call is not preempted. These methods belong on a service thread, not
the GUI or audio callback. Package ownership and descriptor-relative namespace
checks do not sandbox hostile processes already running as the same user.

## Stream readiness

The persistent source decoder now reports a bounded audio inventory alongside
its selected video stream. It records codec, stream index, original clock,
observed start/duration and known sample rate/channel count from the same input.
These observations do not constitute decoded audio sample bounds. An empty
inventory is distinct from audio that exists but has not been qualified.

Before an authored import can register an immutable asset, the host still needs
qualified selected-stream receipts, durable measured indexes, source interpretation,
presentation-basis selection and one atomic registration/insertion command.
Import progress, retry UI, native dialogs, source-browser integration and the full
format matrix remain open. The byte ownership APIs are usable infrastructure,
not completion of the product's local-import workflow.

See [headless commands](HEADLESS.md), [ownership tests](../crates/deadpan-store/tests/original_media.rs),
[CLI tests](../crates/deadpan-cli/tests/original_commands.rs), and
[migration tests](../crates/deadpan-store/tests/migration.rs).
