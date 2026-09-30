# Verified destination publication

`deadpan_cli::encoded_render::publication::publish` accepts a private
`VerifiedCandidate`, its project package and an explicitly selected MP4 path.
It runs off UI/audio threads, reads the captured historical revision and leaves
project state unchanged. This library boundary leaves native Render, public
headless render commands and durable publication recovery as separate work.
The [render job boundary](RENDER_JOBS.md) retains completed candidates for a fresh
verification attempt after restart; it does not journal the publication commit.

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

Crash recovery must independently admit retained bytes before trusting them.
Neither a serialized verification report nor a publication receipt can construct
a `VerifiedCandidate`. Retained checkpoints use the separate render job boundary;
durable destination publication records and reconciliation remain open.

## Report evidence and bounds

The report binds the movie and report names to a unique publication identity,
destination readback hash, complete encoded manifest, verification observations,
captured project/revision/range and document SHA-256. The current encoder choice
is explicitly identified as an engineering selection; automatic platform policy
is not inferred from a successful attempt.

The sibling report is named `deadpan-render-<publication_id>.json`. It records
evidence prepared for publication, since it is committed before the movie.
Only the returned outcome distinguishes `Published` from `PublishedUnconfirmed`;
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

This boundary does not implement native Render, automatic hardware policy,
durable publication reconciliation, complete mastering/effects, HDR or release qualification.
The generated-picture receipt flag is available for a future nonblocking upload
disclosure reminder; it does not set any upload-service metadata.
