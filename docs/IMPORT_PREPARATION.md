# Background import preparation

The store separates complete-file preparation from short inventory and authored
commits. A native import worker can hash, clone/copy, snapshot and decode while
the project service retains its sole writable `ProjectStore`. The native service
uses this worker for its import workflow and for authenticated CLI retention,
relinking, source registration and database checkpoints. Heavy preparation is
separate from the short writer commit; [open-project routing](LIVE_PROJECT.md)
defines admission, observation and cancellation.

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

The service admits one import/preparation at a time onto its existing bounded
worker. CLI preparation uses a separate operation UUID and cancellation token,
retains the exact store session and keeps its result outside the native command
mailbox. A completed worker result waits in `awaiting_commit` until queued native
commands and unread native commit continuations permit publication. Ordinary
editing and existing Render observation continue during the worker phase.

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

The store's `PreparedSourceRegistration` stores no insertion target, expected
authored revision or project frame rate. A source prepared while the canvas is
provisional must respect a timed edit or explicit canvas decision made before
insertion. A stale request fails; a host can resolve fresh intent and reuse the
still-valid token without decoding again. This does not authorize silently
changing a user's selected target or automatically retrying an edit.

The CLI wrapper deliberately captures more than that media token. Its
`PreparationWork` and `PreparedOperation` bind the complete submitted
`SourceRegistration` and explicit stream selection. Commit rejects a different
command before any write, then the store checks the original expected revision,
new revision, asset/insertion identities and source availability. It cannot use
the native import continuation to synthesize a fresh insertion. Registration
passes no fabricated generation relevance observations; requests needing that
context can fail with `GenerationRelevanceRequired`, as in the closed CLI.
An identical existing registration reuses its qualified asset, even when the
captured `new_asset_id` proposes another alias. Without insertion it may succeed
with no new authored revision and still returns that existing asset and
qualification receipt. Exact caller intent preserves this store deduplication
rule; it does not require allocating the proposed asset identity.

## Prepared relinking

Admission captures the complete current `OriginalMediaRecord` and expected
location version. `handle.prepare_relink(record, expected_version, location,
limits, cancelled)` verifies the replacement's whole-file content and retains
its descriptor/path freshness evidence on the worker. It does not update SQLite.
`store.relink_prepared_original(prepared, cancelled)` rechecks the issuing
session, complete captured record, location version and current source metadata
before committing the inventory transaction. A competing relink cannot be
overwritten by a delayed result. An identical path/bookmark is a successful
no-op that preserves the version; a changed location increments it once.
Neither case creates authored history.

## Prepared database checkpoints

`store.checkpoint_handle()` captures the held writer identity and pinned package,
database and `Snapshots` descriptors without copying the database. The worker
consumes that handle, opens its own read-only SQLite connection and pins a read
transaction before using the backup API. Committed WAL content is included;
later edits do not restart the copy or alter its captured revision. The worker
checks the copied project/revision, closes the destination database and
synchronizes the private file before returning `PreparedCheckpoint`.
A pinned reader can delay WAL reclamation until preparation ends.

`store.publish_prepared_checkpoint` checks the live owner, database and namespace
identities, staged file metadata, cancellation and deadline, then performs an
exclusive rename and synchronizes `Snapshots`. Its receipt gives the published
path, actual captured project/revision and database bytes. The source revision
comes from the worker's read snapshot, not the request's arrival time. The result
is a database checkpoint; media is not copied with it.

If rename succeeds but the final directory sync fails,
`CheckpointError::PublishedUnconfirmed` retains the receipt. The file stays
published. The service records a completed operation with `completion_error`,
and the CLI emits its output and receipt before exiting nonzero. Cancellation
after rename cannot relabel that publication as cancelled. No saved receipt
proves physical power-loss recovery.

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

## Limits and client observation

Typed CLI preparation commands admit at most 64 KiB of serialized JSON, with
16 KiB path/bookmark and 4 KiB label limits. Original preparation uses the shared
video/audio decoder input bound, currently 64 GiB, and a 300-second cooperative
limit for each phase. Checkpoint defaults are 1 GiB, 32 pages per backup step and
a five-minute deadline that also applies to publication. These limits do not
preempt a blocking SQLite, decoder or filesystem call.

The native service requests cancellation after 15 minutes and reports a timeout
only after the worker returns. Close/switch/shutdown cancel and drain the old
operation. The CLI retains its originally discovered owner and exact operation
target, polls over separate bounded requests, and never holds a socket through
media preparation. SIGINT, SIGTERM or its 15-minute limit request cancellation;
it waits up to five further minutes for drain. Losing a reply or reaching that
drain limit produces an unknown outcome, never a replay or a new owner lookup.

Completed observations retain typed operational receipts independently of
authored revision receipts, refresh errors and detailed reply capacity. The
service invalidates cached workspace data after an inventory or authored change.
A refresh failure cannot erase the committed result. Successful CLI stdout
delivery releases the terminal observation; failed stdout delivery retains it.
Known failed/cancelled operations also release their entries. The owner retains
at most eight preparation observations, with ten-minute expiry for terminals.
These live observations end when the owner closes; inventory, authored history
and published checkpoints remain persisted.

## Scope and verification

No database or core schema changes are needed. These capabilities are process-local;
persisted original records and source receipts retain their existing meaning.
The closed-project CLI remains available, including independent read-only
registration preview beside an open writer. Current-schema `project migrate`
retains its read-only validation fast path and does not contact the owner.
Only a migration writer-lock conflict selects the IPC no-op schema report;
legacy closed packages keep their backed-up migration path.

The test suite includes thread, store, service and CLI cases for live media,
concurrent edits, exact caller intent, no-op receipts, session closure, stale
versions, namespace changes, cancellation, rollback and lost replies. Those
cases define coverage; run results and native observations belong in dated
qualification evidence. This document does not establish full-size latency,
memory, visual or keyboard acceptance. SQLite receipt writes, document validation
and source timing analysis still have costs. Native relink/checkpoint controls,
portable project copies, full recovery UI and the remaining import acceptance
work remain open.
