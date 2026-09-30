# Verified destination publication

`deadpan_cli::encoded_render::publication::publish` accepts a private
`VerifiedCandidate`, its project package and an explicitly selected MP4 path.
It runs off UI/audio threads, reads the captured historical revision and leaves
authored state unchanged. This library boundary leaves native Render and public
headless render commands as separate work.
The [render job boundary](RENDER_JOBS.md) retains completed candidates for a fresh
verification attempt after restart. `publication::journal` adds staged publication
and explicit restart reconciliation under durable store permits.

## Destination transaction

The host pins the selected parent directory, retaining its identity and path
components. Movie and report names share that pin. The destination may be on a
different filesystem from the project or private candidate. Existing entries,
including symlinks, are conflicts; no existing file is replaced or adopted.

The movie is copied to an exclusive uniquely named sibling `.partial` under a
byte bound. Sealing revokes the host API's write access, synchronizes the file
and directory, and retains exact file metadata. Readback hashes the destination
descriptor at its fixed extent and compares it with the verified private movie.
Content, identity, owner, mode and link-count changes fail admission. Exact byte
identity carries the already completed media verification to the staged copy.

The bounded JSON report is separately written, synchronized, read back and
published using an atomic no-replace rename. The host then rechecks that report
and the movie partial before atomically renaming the movie to its final name.
The two files are not one atomic transaction: an orphan report may remain if
movie publication fails.

On macOS, file synchronization uses `F_FULLFSYNC`, directory synchronization
uses `fsync`, and a final full file synchronization follows the namespace change.
Linux uses file/directory `fsync`; it has separate platform qualification needs.
Unsupported durability or exclusive-rename operations fail explicitly.

## Commit, cancellation and errors

The movie rename is the commit point. Before it, failure returns the verified
candidate and diagnostic paths for all retained partials/reports. Paths are
recovery information, not permission to adopt or remove those entries. There is
no automatic cleanup that could delete a foreign replacement.

After rename, late cancellation or deadline expiry does not interrupt durability
work. Both published descriptors are rehashed against their verified identities
under stable post-rename metadata, followed by final entry checks. Rename can
legitimately change ctime, so metadata alone cannot establish unchanged bytes
across that operation. Final readback has a separate shared ten-minute cooperative
deadline and the same exact byte bounds. A successful durability/integrity check
returns `Published`. A failure after rename returns `PublishedUnconfirmed` with
the receipt and an actionable diagnostic. It never reports an unpublished
attempt or deletes the final file. The durable report is checked again before
claiming full success.

Crash recovery independently admits retained bytes before trusting them. Neither
a serialized verification report nor a publication receipt can construct a
`VerifiedCandidate`. The journal binds the live candidate to the exact completed
verification attempt as well as its movie hash, extent and historical contract.

## Durable publication journal

Database 41 adds operational publication records without changing core schema 33.
The store derives the retained movie identity and encoding attempt from a terminal
Verified render attempt. The caller supplies a fresh publication ID, operation ID,
cancellation token and absolute MP4 destination. Each transition requires the
exact operation, token and sequence. Prepared filesystem evidence is bounded to
128 KiB; the store does not open the destination or hash movies.

The host and writer alternate these stages:

1. `begin_render_publication` durably records Intent and returns an opaque permit.
2. `journal::prepare` checks the exact live verifier and stages, synchronizes and
   hashes both files. It retains their descriptors and exclusive advisory locks.
3. `record_prepared_publication` stores their immutable identity evidence and
   hashes. `advance_publication(ReportCommitting)` authorizes `commit_report`.
4. After report publication and readback, record ReportCommitted, then durably
   record MovieCommitting. Only that permit authorizes `commit_movie`.
5. Record the returned Published or PublishedUnconfirmed outcome. A precommit
   failure can be recorded as Failed or Cancelled only after owned work stops.

Every committed transition revokes the previous permit. Closing the writer or
requesting cancellation also revokes rename authority. The host checks authority
through preparation and immediately before rename. Once the movie rename
succeeds, the ordinary bounded postcommit checks finish despite late cancellation.
The prepared object cannot use a permit for a different operation or destination.

SQLite uses WAL, synchronous FULL and fullfsync. Because SQLite's native VFS can
fall back from a failed full sync to ordinary fsync, pragma settings alone do not
authorize rename. After COMMIT, the store directly checks full synchronization of
the pinned database and current WAL, synchronizes the package directory, performs
final full file syncs and rechecks the namespace. No fallback is accepted. A failed
barrier leaves the committed row observable, revokes all session publication
permits and denies further publication writes until reopen. This is a checked OS
durability contract; it does not prove hardware behavior during physical power loss.

## Restart reconciliation

Read-only open preserves publication state. Writer reopen interrupts active
operations without touching destination files. A previously observed movie commit
remains PublishedUnconfirmed. Nothing automatically renames, removes or resumes
an interrupted partial. Reconciliation requires a newer completed verification
attempt of the same checkpoint and its fresh live `VerifiedCandidate`.

Durable destination evidence currently requires macOS APFS. A narrow native
adapter reads the nonzero volume UUID from an owned descriptor. Evidence retains
device, inode, exact birth time, nonzero generation when available, owner, group,
mode and flags. File evidence also retains exact extent, single-link status and
mtime. The selected path's symlink identities and targets, and every canonical
directory component, are checked again. Rename may change ctime; fresh reads
instead require stable current metadata including ctime. This detects cooperative
replacement and mutation; it is not authentication against a malicious same-user
process. The standalone publisher retains its existing platform support.

`journal::reconcile` has three outcomes:

- Before MovieCommitting, the journal proves that no movie rename was authorized.
  Return NotPublished without opening or adopting any destination entry.
- At MovieCommitting, absent or mismatched final movie identity or bytes leave
  the outcome unresolved. An existing partial alone cannot prove that the final
  movie was never renamed and subsequently removed.
- Matching final movie identity and full SHA-256 establish an observed commit.
  Matching report identity and bytes plus successful synchronization confirm
  Published. Missing, replaced or damaged reports, or later check failures,
  produce PublishedUnconfirmed and retain the observed movie commit.

Recovered handles provide only checked reads and synchronization. They acquire
movie then report locks, retain exact file extents, and never grant rename or
deletion authority. Keep `RecoveryInspection` alive through the final store
transaction so admitted file locks remain held. Conflicting final or partial
entries are preserved. Routine selected reads stay bounded; store open and
explicit validation audit the complete operation history.

## Report evidence and bounds

The report binds the movie and report names to a unique publication identity,
destination readback hash, complete encoded manifest, verification observations,
captured project/revision/range and document SHA-256. The current encoder choice
is explicitly identified as an engineering selection; automatic platform policy
is not inferred from a successful attempt.

The sibling report is named `deadpan-render-<publication_id>.json`. It records
evidence prepared for publication, since it is committed before the movie.
Only the returned and recorded outcome distinguishes Published from PublishedUnconfirmed;
the report alone does not prove the movie rename or final durability succeeded.

Historical source receipts supply Original BLAKE3 references and source SHA-256.
The committed catalog is labeled as a superset, including unused entries.
Unqualified legacy or generated catalog entries retain that absence of evidence;
arbitrary asset strings do not become verified hashes. Source paths, labels,
URLs and authentication data are not copied into report metadata.

Effective Generated picture intervals come from the indexed picture resolver
over the exact output range, including Repeat gaps and overrides. Consecutive
frames with the same complete artifact identity coalesce. Direct sampled/native
master references and immutable provenance hashes are retained; conditioning
inputs remain transitively bound by that provenance object. Current generation
requests and unaccepted candidates cannot replace historical providers.

Capture scans at most one million output frames without expanding Repeat
structures. It admits at most 4,096 catalog assets, 4,096 effective generated
artifacts, 65,536 generated intervals and a 16 MiB report. Exceeding a bound fails
without truncation. Original media is not reopened: the completed movie and
historical receipts supply the relevant byte identities.

This boundary does not implement native Render, public headless render commands,
automatic hardware policy, complete mastering/effects, HDR or release qualification.
The generated-picture receipt flag is available for a future nonblocking upload
disclosure reminder; it does not set any upload-service metadata.
