# Render candidate byte retention implementation

Source frozen for parent formatting, compilation and tests. No builds, tests, formatters, native programs or commits were run by this agent.

## Owned files

- `crates/deadpan-store/src/render_media.rs`
- `crates/deadpan-store/src/render_media/tests.rs`
- `crates/deadpan-store/src/object_storage.rs`

No other repository files were edited. The parent/store agent owns module, store fields, error, schema and checkpoint wiring.

## Public API

`RenderObjectRef` privately wraps a checked `GeneratedObjectRef`. Its constructor checks positive length and the 64 GiB hard cap. Getters are `content()` and `byte_length()`. It has strict checked deserialization, with no conversion to a generated-artifact capability.

`RenderCandidateMedia::new(movie, movie_sha256, manifest, manifest_sha256)` creates persistable byte claims, with getters of those names. Its strict deserializer rejects unknown fields and an oversized manifest. `validate(RenderMediaLimits)` checks actual per-object and combined lengths. SHA values are `deadpan_jobs::Sha256`.

`RenderMediaLimits::new(maximum_movie_bytes, maximum_manifest_bytes, maximum_combined_bytes, maximum_namespace_bytes, maximum_namespace_entries: u32)` validates caller limits. Hard caps are 64 GiB movie, 256 KiB manifest, their sum for combined bytes, and 100,000 namespace entries. Corresponding getter methods are available. Namespace bytes are an explicit positive caller-selected `u64` bound, with checked aggregate addition.

`ProjectStore::render_read_handle()` is available for either store mode. `render_write_handle()` requires writable access. Both handles contain only an `Arc<ObjectStorage>` and an `Arc<AtomicBool>` revocation flag; neither contains SQLite or the store writer lock.

```rust
RenderWriteHandle::prepare_retention(
    &self,
    identity: &deadpan_jobs::render::RenderAttemptIdentity,
    movie: &mut impl Read,
    expected_movie_bytes: u64,
    expected_movie_sha256: &deadpan_jobs::Sha256,
    manifest: &[u8],
    limits: RenderMediaLimits,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<PreparedRenderRetention, RenderMediaError>

RenderReadHandle::snapshot(
    &self,
    expected: &RenderCandidateMedia,
    limits: RenderMediaLimits,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<PreparedRenderSnapshot, RenderMediaError>
```

`PreparedRenderRetention` exposes `media()` and `identity()`. Its crate-private `validate_for(&Arc<ObjectStorage>, &Arc<AtomicBool>, cancelled, deadline)` compares exact session Arc identities, checks liveness/control and both retained descriptor freshness guards. It reads no movie bytes and performs no hashing. The SQLite checkpoint caller must compare the exact attempt identity, require writable access, and call this before its transaction and immediately before commit.

`PreparedRenderSnapshot` exposes `media()`, `manifest_bytes()`, `check_live(cancelled)`, and `read_at(&self, offset, &mut [u8], cancelled, deadline)`. Reads are capped at 64 KiB, permit EOF, reject offsets beyond EOF, and check controls before and after reading. No pathname or writable descriptor escapes. Snapshots are privately copied and independently BLAKE3/SHA-256 checked. They cannot create media trust.

All preparation, snapshot, lock waits, hashes and block reads use the caller's same absolute deadline. No operation creates a replacement timeout. These filesystem and reader calls remain cooperative, not preemptive interruption of a blocked kernel call or arbitrary supplied `Read` implementation.

## Namespace and shared-engine changes

Added `StorageNamespace::RenderCandidates`, `ObjectControl::check` crate visibility, a private descriptor read method on `VerifiedObject`, and `ObjectStorageError::NamespaceCapacity` with code `RenderMediaNamespaceCapacity`.

`ObjectStorage::lock_render_namespace(control)` safely opens `.render-candidates.lock` relative to the pinned package root. It requires a regular zero-length file, one hard link, expected owner/device, no symlink and no group/other write permissions. `File::try_lock()` is retried with bounded 5 ms polling under the shared deadline. The lock stays held through staging, promotion, durability and freshness guard construction. Store revocation does not release an old worker's in-progress lock; another session opens the same fixed lock and waits. The guard explicitly unlocks on drop.

Only this write operation lazily creates `Media/RenderCandidates`. Read-only opening and failed read snapshots create neither directory nor lock. Media and namespace directories retain the existing owner/mode/device/no-follow admission. The guard confirms the fixed lock identity and both directory identities, detecting replacement. Package-root descriptor pinning retains the existing shared-engine authority model.

The namespace scan uses descriptor-relative `rustix::fs::Dir`, stops at the caller's entry limit, checks the deadline per entry, and counts every regular single-link owned file, including crash pending entries and unreferenced objects. Unsafe entries reject admission. It checks directory state around the scan. A preliminary scan rejects unsafe or over-capacity storage before movie copying; after hashing, exact object names permit deduplication credits and reserve any new bytes/entries before publication. It never removes a preexisting entry. The shared engine continues to remove only an operation's own unrenamed temporary, which the parent explicitly approved. Published objects survive all later failures.

## Required wiring

The store agent has implemented the agreed wiring:

- Platform-gated `pub mod render_media`.
- `ProjectStore.render_storage: Arc<ObjectStorage>` opened with `StorageNamespace::RenderCandidates` on creation and opening. Opening storage does not create the namespace.
- `ProjectStore.render_closed: Arc<AtomicBool>` initialized false and set true before writer-lock release on drop.
- `StoreError::RenderMedia(#[from] crate::render_media::RenderMediaError)`.
- Checkpoint uses `prepared.identity()`, `prepared.media()`, and two `prepared.validate_for(...)` calls with the cloned exact session Arcs.

## Added tests, not executed

Twelve unit tests exercise strict claim deserialization and hard limits; read-only no-creation; exact namespace-budget deduplication and independent snapshots; wrong/revoked sessions; truncated, oversized and wrong-hash streams; combined limits; pending/orphan byte and entry capacity; namespace symlinks and hard-linked entries; published movie preservation when manifest admission fails; fresh snapshot rehash and SHA binding; late read cancellation, deadlines and read bounds; independent session lock serialization after revocation; and replaced namespace/lock detection.

Parent should run focused `deadpan-store` render-media tests, store checkpoint/recovery tests, shared object-storage regression tests, then the existing required workspace gates. No test result is claimed here.

## Scope and limits

This is complete-byte retention only. The CLI owns strict manifest meaning, job/document/contract binding and fresh isolated finished-file verification. Persisted identities and historical reports cannot construct a verified candidate.

Namespace accounting is a logical named-file budget, not physical free-space prediction. Per-operation private staging and snapshot copies are separately capped by the combined budget and can consume additional disk space. APFS clones may share physical extents, but no capacity credit assumes that. The host must bound concurrent retained snapshots/handles; this increment has no process-wide scratch quota or disk reservation API. Filesystem allocation failures remain explicit `DiskFull`/I/O errors. Existing safe same-user namespace authority limits still apply; this does not sandbox hostile code running as the owner.

No database/authored-history mutation, automatic orphan cleanup, media interpretation, source import relaxation, or media-validity constructor was added.
