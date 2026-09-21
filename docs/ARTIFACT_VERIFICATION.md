# Worker artifact snapshots

This slice extends the [worker boundary](WORKER_VERIFICATION.md) with host-side
path containment, byte limits, SHA-256 verification, and an ephemeral stable
copy. It does not validate media, make a candidate ready, persist an accepted
artifact, or qualify an inference backend. DP-18 remains partial.

## Ownership and validation

[`ArtifactWorkspace`](../crates/deadpan-jobs/src/artifact.rs) retains an open
directory descriptor before worker launch. The host selects that workspace, an
output scope, and a positive byte budget. After successful process and pipe
cleanup, `snapshot` requires the worker reference to be strictly below that
scope by path component. A similarly named sibling or an input/context reference
cannot pass as output.

Resolution walks from the retained descriptor with `openat` and `NOFOLLOW` on
every component. Each opened directory and final file must have the workspace's
owner and device. The workspace itself must belong to the host's effective UID.
The final descriptor opens nonblocking before type inspection, so a FIFO cannot
hold up validation. Only a regular file with one hard link is allowed. Its actual
length must match the declaration and fit the host byte budget before copying.

The host reads that same descriptor with a 64 KiB buffer, hashes the bytes, and
copies them to an unlinked host-owned temporary file. It aborts if bytes exceed
the declaration or budget, then checks length, digest, and before/after file
metadata including inode, mode, owner, link count, size, mtime, and ctime.
`HashedArtifactSnapshot` exposes read/seek access and the verified declaration;
it exposes neither a writable handle nor the worker path. Subsequent source
replacement or modification cannot change the copied bytes.

This proves an artifact's declared bytes, not the truth of its provenance or
media metadata. Hashing runs on the job service, never on the audio callback.
Snapshot files remain temporary and unaccepted; dropping them releases their
storage. They do not replace durable generated-media ownership or recovery.

## Verification

The [integration tests](../crates/deadpan-jobs/tests/artifact.rs) exercise real
filesystem behavior: nested output, source changes after snapshotting, a replaced
workspace pathname, strict output scope, final/intermediate/root symlinks,
internal/external hardlinks, directories, FIFOs, Unix sockets,
wrong hashes, wrong lengths, and byte-budget failures. A fixed standard `abc`
SHA-256 answer checks digest and hexadecimal encoding independently of the test
helper. Private unit hooks deterministically modify or grow an opened source.
Pure metadata checks exercise wrong-device and wrong-owner classification.

On 2026-09-20, the full repository gate passed on Apple M5 Max, arm64,
128 GiB RAM, macOS 26.5.2 (25F84), Rust 1.97.1: rustfmt, Clippy with warnings
denied, 198 Rust tests with none failed or ignored, workspace build, and the
headless doctor. The count includes 3 new artifact unit tests and 8 new artifact
integration tests. The 20 audio-measurement Python tests also passed.

App startup smoke and GUI interaction were skipped because this slice changes
neither application startup nor UI behavior. Actual filesystem integration
tests provide the relevant platform evidence.

Independent review of containment, mutation races, hashing, and resource bounds
found no outstanding issues. Parent review tightened growth rejection to stop
before writing a chunk beyond the declaration and added the fixed hash answer.

## Limits and remaining work

The host must capture the directory before launch and use the same workspace
for the worker. This is descriptor-based file consumption, not an OS sandbox
against arbitrary same-user processes. Actual nested mounts and foreign-UID
files were not created for tests; those rejections have metadata coverage.
Linux is cfg-supported but has not been exercised locally.

Next layers must decode the snapshot, validate dimensions, duration, color,
provider provenance, and request constraints, and check target relevance again.
Only then may the host offer explicit acceptance. Durable promotion, reference
tracking, accepted-artifact portability, eviction, recovery, and the shared media
decoder remain open. No model or GUI behavior is established by these tests.
