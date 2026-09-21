# Background import preparation

The store separates complete-file preparation from short inventory and authored
commits. A native import worker can hash, clone/copy, snapshot and decode while
the project service retains its sole writable `ProjectStore`. These APIs provide
the boundary; the native project service, bounded import queue and visible import
workflow still need integration.

## Ownership and lifetime

`ProjectStore::original_import_handle()` issues a connection-free
`OriginalImportHandle` from the writable session. The handle can move to a worker
thread and retains descriptor-relative storage authority. It holds no SQLite
connection or writer lock. Read-only stores cannot issue one.

The handle and its prepared results belong to that exact open session. Another
project or a later reopening of the same path cannot consume them. Dropping the
store marks the session closed before releasing its writer lock. Cooperative
copy/hash checks observe both caller cancellation and session closure. A single
filesystem call is still not preemptible. Closed sessions report
`OriginalImportClosed`, distinct from `OriginalCancelled`, including when both
signals have been raised.

Prepared values have private fields and no deserialization path. Cached metadata,
a matching filename or a copied project ID cannot construct admission proof.
Reading an already returned private snapshot remains valid after close; admitting
new inventory or authored state does not.

## Retention and qualification

1. Call `handle.prepare_retention(path, ownership, limits, cancelled)` on the
   import worker. It performs complete-file hashing and managed publication or
   linked-path verification without borrowing the writer. Its result is not an
   inventory record or authored asset.
2. Call `store.retain_prepared_original(prepared, cancelled)` on the project
   service. This rechecks the session and source/namespace state, merges ownership
   against the current record and writes the inventory transaction. The original
   synchronous `retain_original` convenience method uses these same steps.
   Retention can add a missing linked location, but cannot replace an established
   one. A competing location or bookmark requires explicit versioned
   `relink_original`; a delayed import cannot overwrite a newer relink.
3. Obtain the current `OriginalMediaRecord`, then call
   `handle.snapshot_original(record, limits, cancelled)` on the worker. Its
   `PreparedOriginalSnapshot` provides verified private bytes for the existing
   `VerifiedSourceInput`, `SourceSession` and `AudioSession` interfaces.
4. Produce the live `DecodedSourceQualification` from those selected sessions.
   `PreparedSourceRegistration::from_decoded` joins it to the prepared original,
   canonicalizes the measured indexes and hashes the immutable receipt on the
   worker. Keep selected-audio failures explicit.
5. Resolve an explicit `SourceRegistration` against the current authored revision.
   Use `preview_prepared_source_registration` for relevance resolution, then
   `register_prepared_source` to commit. Those paths use the same typed command
   reducer, atomic history and generation relevance checks as synchronous import.

Preparation stores no insertion target, expected authored revision or project
frame rate. A source prepared while the canvas is provisional must respect a
timed edit or explicit canvas decision made before insertion. A stale request
fails; the host can resolve fresh intent and reuse the still-valid token without
decoding again. This does not authorize silently changing a user's selected
target or automatically retrying an edit with different meaning.

## Availability and atomicity

The prepared snapshot retains both independent verified bytes and an availability
guard for the original. Admission reopens the current namespace entry and checks
the retained descriptor, device/inode, length and modification/change timestamps.
Managed objects also retain the object engine's containment, ownership, mode and
link-count checks. Linked sources retain their selected path and inventory
version. Replacement, in-place modification, deletion or stale location metadata
requires fresh preparation, even if a replacement happens to contain equal bytes.

These final checks inspect metadata; they do not copy or hash the complete file
on the writer thread. The retained private copy cannot silently replace an
original that disappeared. Final checks also run before the transaction commits.
As with existing storage, this does not sandbox a hostile process running as the
same user or prevent external modification after the last check.

An existing qualification receipt is compared with the incoming canonical bytes
inside SQLite. The writer does not deserialize and rehash that large index merely
to deduplicate it. Reopening still performs complete receipt/history validation.

Database failure or cancellation commits no partial authored edit. Already
published originals remain available for retry; they are never deleted to imitate
an atomic filesystem/database transaction. Prepared tokens can be retried after a
database failure while their session, source and revision intent remain valid.
Once a copy is published, its file and namespace durability steps complete before
cancellation can stop post-publication verification. A durability failure remains
an error, with the published bytes retained for a later verified retry.

## Scope and verification

No database or core schema changes are needed. These capabilities are process-local;
persisted original records and source receipts retain their existing meaning.
The synchronous CLI remains available, including independent read-only preview.

Headless thread and integration tests exercise live media, concurrent authored
edits, session closure, stale intent, namespace changes, rollback and deduplication.
The split removes complete-file copying and hashing from writer commits. SQLite
receipt writes, document validation and source timing analysis still have costs;
no full-size latency or memory budget is established by tiny fixtures. Native
queue scheduling, project controls, visual quality and keyboard behavior remain
separate implementation and acceptance work.
