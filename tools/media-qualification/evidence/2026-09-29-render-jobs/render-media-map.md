# Durable render candidate media boundary

Source research only, against the checkout following `097f735`. No repository edits, builds, tests, formatters, native programs, or commits were performed. Ripwire task mapping was useful; its exemplar query fell back to an unrelated low-confidence core helper, so the recommendations below use the exact storage/import sources.

## Recommendation

Add a `deadpan-store::render_media` adapter over the existing byte-object engine, with a distinct `Media/RenderCandidates` namespace. Retain the complete MP4 and a small strict manifest as two content-addressed objects. Publish and synchronize both before returning an opaque prepared token to the writer. A short operational SQLite transaction checkpoints that token. Recovery opens fresh private snapshots and runs the existing isolated verifier again before destination publication.

The stored record and manifest are claims and byte identities. They cannot construct `VerifiedCandidate`, authorize publication, or mutate authored history. Keep the store independent of CLI, GPU, and native-encoder types.

## Existing mechanisms to reuse

| Source | Useful existing boundary |
| --- | --- |
| `crates/deadpan-store/src/object_storage.rs:33` | `StorageNamespace::{Generated, Originals}` already selects the fixed media directory. Add one variant, not a caller-supplied path. |
| `object_storage.rs:48`, `:88`, `:147` | `ObjectIdentity`, `ObjectControl`, and `ObjectLimits` provide checked BLAKE3 identity, one absolute deadline, cancellation, store-closure checks, and per-object byte bounds. `ObjectLimits` alone has no hard or aggregate cap. |
| `object_storage.rs:280`, `:719`, `:799` | Controlled promotion verifies complete bytes, rejects unsafe existing objects, uses exclusive pending files and `NOREPLACE`, and completes file/namespace durability. `promote_file_controlled` supports APFS clone with verified copy fallback. |
| `object_storage.rs:504` | Every media operation reopens `Media/<fixed namespace>` beneath the pinned package descriptor. It checks directory owner, mode, same device, and no symlinks. Different filesystems are legitimate for exported destinations, but project-managed media intentionally stays on the package's device. |
| `object_storage.rs:937`, `:962`, `:1015` | `guarded_snapshot_controlled`, `guard_controlled`, and `recheck_guard` bind complete hashing to retained source descriptors and stable metadata. These support short final writer checks without movie copying/hashing on the SQLite thread. |
| `crates/deadpan-store/src/original_media.rs:255`, `:280` | `OriginalImportHandle` and `PreparedOriginalRetention` are the exact write-handle/token pattern: private fields, `Arc<ObjectStorage>`, revocation flag, no SQLite connection, no writer lock, no deserialization of admission proof. |
| `original_media.rs:352`, `:362`, `:375`, `:565` | Session validation uses `Arc::ptr_eq`, closure takes precedence over cancellation, filesystem preparation precedes the inventory transaction, and guards are checked again immediately before commit. Published objects survive transaction failure or cancellation. |
| `crates/deadpan-store/src/generated_media.rs:51` | `GeneratedReadHandle` is available to read-only and writable stores and revokes on owner drop. Its existing relative-timeout API should not be copied for this pipeline; pass the caller's `Instant` directly. |
| `crates/deadpan-store/src/lib.rs:55`, `:72`, `:165`, `:220` | Store construction owns storage/session state; `Drop` revokes handles before releasing the writer lock. |
| `crates/deadpan-store/src/source_registration.rs:116` | `PreparedSourceRegistration` demonstrates opaque, non-deserializable preparation plus bounded canonical serialization off the writer thread. |
| `crates/deadpan-cli/src/encoded_render/host.rs:52`, `:78` | `EncodedCandidate` has private fields and bounded `read_at`/`copy_to`. It deliberately exposes no file path or writable descriptor. |
| `crates/deadpan-cli/src/encoded_render/verification/host.rs:24` | `verify` is the current fresh isolated media-admission boundary. It stages a private candidate into a controlled verifier input and requires clean teardown. |

## Proposed store API

Suggested names below are concrete, but render job/attempt/checkpoint identifiers should use the parent-selected operational types.

```rust
// New crates/deadpan-store/src/render_media.rs.
// Serialized values are claims. All fields validate on decode.
pub struct RenderObjectRef(/* private algorithm-tagged identity + length */);
pub struct RenderCandidateMedia {
    // private fields; getters; strict Serialize/Deserialize
    movie: RenderObjectRef,
    movie_sha256: deadpan_jobs::Sha256,
    manifest: RenderObjectRef,
    manifest_sha256: deadpan_jobs::Sha256,
}

pub struct RenderMediaLimits {
    // checked constructor; no deadline reset inside operations
    maximum_movie_bytes: u64,
    maximum_manifest_bytes: u64,
    maximum_combined_bytes: u64,
}

#[derive(Clone)]
pub struct RenderReadHandle { /* Arc<ObjectStorage>, Arc<AtomicBool> */ }
#[derive(Clone)]
pub struct RenderWriteHandle { /* same pin/revocation, issued only by writer */ }

pub struct PreparedRenderRetention {
    // private: write handle, RenderCandidateMedia, two ObjectFreshnessGuards
}
pub struct PreparedRenderSnapshot {
    // private: read handle, RenderCandidateMedia, movie VerifiedObject,
    // bounded manifest bytes, and guards if needed for checkpoint admission
}

impl ProjectStore {
    pub fn render_read_handle(&self) -> RenderReadHandle;
    pub fn render_write_handle(&self) -> Result<RenderWriteHandle, StoreError>;

    // The parent-owned render-job module chooses exact ID/version types.
    pub fn checkpoint_render_candidate(
        &mut self,
        expected_attempt: &RenderAttemptCheckpoint,
        prepared: &PreparedRenderRetention,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<RenderCandidateCheckpoint, StoreError>;
}

impl RenderWriteHandle {
    pub fn prepare_retention(
        &self,
        movie: &mut impl Read,
        expected_movie_bytes: u64,
        expected_movie_sha256: &deadpan_jobs::Sha256,
        manifest: &[u8],
        limits: RenderMediaLimits,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<PreparedRenderRetention, StoreError>;
}

impl RenderReadHandle {
    pub fn check_live(&self, cancelled: &AtomicBool) -> Result<(), StoreError>;
    pub fn snapshot(
        &self,
        expected: &RenderCandidateMedia,
        limits: RenderMediaLimits,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<PreparedRenderSnapshot, StoreError>;
}
```

`RenderObjectRef` can privately wrap the existing checked `GeneratedObjectRef`, as `OriginalObjectRef` already does. Do not expose a conversion to a generated-artifact reference or use `Media/Generated`. The shared hash implementation does not grant generation/acceptance authority; the distinct public wrapper prevents accidental cross-namespace calls. Keep algorithm tags in persistence.

Expose `PreparedRenderRetention::media()`, and bounded read/seek or `read_at` access to the movie in `PreparedRenderSnapshot`, plus `manifest_bytes() -> &[u8]`. Expose neither a mutable file nor a path. Reads of an already-created private snapshot may remain valid after close, following existing storage semantics; starting or admitting new render work must check the live owning session.

## Manifest ownership and strictness

Define a versioned `RetainedRenderManifest` in the CLI render-job adapter. It should wrap the existing strict `EncodedManifest` with the captured job/attempt identity. `EncodedManifest` already includes the complete picture/encoder contract, full document SHA-256, exact movie length/SHA-256, and native encoder report. Its `output/movie.mp4` value remains historical wire evidence and is never resolved as a recovery path.

- Use `#[serde(deny_unknown_fields)]` through the envelope and existing nested strict types; reject unsupported schema versions and malformed claims.
- Use a bounded serializer before allocating complete output, and cap parsing before deserialization. Prefer the existing protocol-sized 256 KiB cap (`deadpan_jobs::protocol::MAX_FRAME_BYTES`) for this manifest. It contains no per-frame index, PCM, or source corpus. The separately bounded 16 MiB publication provenance report is a different object and need not be retained for this first checkpoint.
- The storage adapter receives opaque bounded bytes and proves their identity/durability. CLI serialization/parsing owns the exact manifest schema because the store cannot depend on `EncodedRenderContract` or `EncodeReport` without reversing dependencies.
- Persist both object identities in the SQLite checkpoint. On recovery, compare the strict manifest's job/attempt, complete document hash, contract, movie length and SHA-256 against the retained checkpoint and captured intent.
- Do not deserialize `ExportPictureContract`, `EncodeContract`, `EncodedCandidate`, or `VerifiedCandidate`. Do not promote an old verification report into new media authority.

This split means a prepared storage token proves retained bytes, not that the encoder or media claims are true. Name the first checkpoint `encoded_retained` or equivalent; reserve verified/publication states for their actual later work.

## Preparation and checkpoint sequence

1. The project service captures immutable render intent and reserves the job/attempt and resource allowance in a short SQLite transaction. It obtains a write handle from that exact writable store session.
2. The worker encodes and receives the current `EncodedCandidate` only after clean encoder teardown. A small private CLI `Read` adapter over `EncodedCandidate::read_at` supplies its bytes; no candidate file descriptor or generic candidate constructor is required.
3. The worker strictly validates and bounded-serializes the retained manifest. All subsequent stages use the same caller `Instant` deadline and cancellation flag, plus the handle's closure flag.
4. `prepare_retention` checks positive sizes, movie cap, manifest cap and checked combined size before I/O. It streams the movie in 64 KiB chunks into an anonymous staging file while computing BLAKE3 and SHA-256; exact EOF/length and SHA-256 must match the candidate. This supplies the BLAKE3 identity that the existing engine requires before promotion. Avoid a whole-movie allocation.
5. Promote the staged movie through `ObjectStorage::promote_file_controlled`. Hash the bounded manifest and promote it through the same engine, using a small anonymous file or a narrow controlled-reader wrapper. Both objects are immutable package-owned objects, separate from owner-writable user exports.
6. Acquire guards for both published objects with `guard_controlled`; compare its observed SHA-256 with the hashes obtained before promotion. Return the private prepared token only after both full file/namespace durability sequences and final liveness checks succeed.
7. On the project service, `checkpoint_render_candidate` requires the issuing writer session, exact active attempt/version/intent, and expected movie identity. It checks both retained guards before opening the immediate transaction and again before committing the operational row. It performs no full-file copy/hash and changes no authored revision/history.
8. If the manifest promotion or SQLite commit fails, keep any already-published content-addressed movie/manifest. A retry verifies/reuses the existing identity. Never delete a valid published object to simulate a cross-filesystem/SQLite transaction.

Do not reuse `OriginalMediaLimits::control` for this sequence: it creates `Instant::now() + timeout` each time. Construct `ObjectControl::bounded(deadline, cancelled).with_closed(...)` from the same caller deadline for every stage.

## Restart and verifier integration

After writable reopen, the job layer recovers interrupted nonterminal states without inferring ownership from a saved PID. The old handles and opaque tokens are invalid even for the same package path; only new session handles can prepare new snapshots.

Read the bounded checkpoint, then have a worker call `RenderReadHandle::snapshot`. Hash/copy both named objects through the engine; compare both recorded hashes and lengths. Parse the bounded manifest strictly and reconstruct the real contract from the recorded revision/range using `ProjectPictureSession::open_revision`, `ExportPictureContract::capture`, and `document_hash`, just as `encode` currently does before launch. Compare every retained claim against that reconstructed authority.

The current `EncodedCandidate` owns only `HashedArtifactSnapshot`, whose fields are intentionally private. Prefer a small private byte-storage enum inside `encoded_render::host` supporting either the existing worker snapshot or the opaque store snapshot. A narrow recovery entrypoint selected by a persisted job/attempt obtains and validates the retained snapshot before producing an **unverified** in-memory candidate for the existing `verification::verify` path. Do not add a public `from_manifest`, arbitrary-reader/File constructor, or deserializable trusted candidate to either crate. Do not reconstruct `VerifiedCandidate` at all; only fresh verifier success does so.

Keep fresh verification failures retryable with the retained media references. A stored prior success can be shown as historical evidence, but it does not bypass current byte checks, runtime admission, complete decode, or destination publication checks.

## Narrow shared-engine and store changes

1. `object_storage.rs`: add `StorageNamespace::RenderCandidates` with fixed component `RenderCandidates`. Existing `ObjectStorage` promotion, guards, snapshots and durability need no semantic weakening.
2. Add a writer-only, controlled `ensure_namespace` operation for this new optional namespace. Resolve beneath the pinned package/Media descriptors, use `mkdirat` with exclusive creation semantics, reopen with `NOFOLLOW`, validate owner/device/mode, and sync the created namespace plus Media/package. Existing unsafe entries must fail; read-only access never creates or repairs directories. Prefer doing this on the preparation worker through the writer-issued handle.
3. New packages may include `Media/RenderCandidates` in `ProjectStore`'s fixed directory list (`lib.rs:118`). Existing packages need lazy namespace creation or the parent-selected schema migration path; do not make opening an otherwise-valid old/read-only package fail merely because it has no render candidates. `ObjectStorage::open` currently pins only the package, so lazy creation fits.
4. `lib.rs`: add the render storage `Arc` and its own session-closure flag, initialize in create/open, revoke in `Drop` before releasing the writer lock, and expose read/write handle methods. Keep the write handle unavailable to `AccessMode::ReadOnly`.
5. `render_media.rs`: own the render reference wrapper, limits, handles, copy/hash preparation, two-object token, and guarded snapshot adapter. Reuse internal generated reference/hash representation only inside this adapter/engine.
6. Add the small error variant plumbing in `StoreError`; retain distinct cancellation, session-closed/session-mismatch, missing/unsafe object, identity, quota and durability failures.
7. Optional engine convenience only: `promote_reader_controlled` can forward to `GeneratedStorage::promote_with_control_hooks` for a manifest whose BLAKE3 identity is already known. It must expose no dynamic namespace/path. An anonymous manifest file avoids needing this helper initially.

The job schema/checkpoint API is a separate parent-owned change. The new namespace itself should not change authored core schema or project history.

## Disk and aggregate budgets

- Enforce movie <= 64 GiB and manifest <= 256 KiB, with checked combined bytes. These are proposed hard caps aligned with the current verifier/protocol. The verifier's one-million-packet, 16 MiB header/table and 16 MiB packet limits still apply; retaining bytes does not enlarge supported media capacity.
- Current `ObjectLimits` bounds a single object only. Neither the object engine nor original storage enforces a cumulative render-candidate disk quota. `MAX_ORIGINALS` is an inventory-count limit, not usable render disk accounting.
- The job layer needs explicit maximum retained attempts/candidates and aggregate byte reservations before starting a new large copy. Deduplication can reduce actual retained bytes, but do not assume a clone or dedup hit for admission.
- Include unreferenced published survivors and interrupted `.pending-*` objects in namespace inventory/recovery accounting. A SQLite sum of referenced candidates alone misses bytes intentionally retained after promotion-before-checkpoint failure. Use bounded worker inventory/limits and serialize the first project's retention operations or otherwise reserve concurrent copies atomically. Never scan/hash the namespace on the UI/writer thread.
- Reserve worst-case scratch separately from retained storage. Initial encode already owns a private full movie; retention may require another anonymous staging movie plus a copy into the package. APFS clone saves physical work when available, but it is not guaranteed across volumes. Recovery snapshot plus verifier staging can also hold multiple full movies. Bound concurrent work accordingly and retain actionable disk-full failures.
- Free-space observation is advisory, not a reservation against other applications. A crash can leave an orphan despite a reservation. Do not auto-delete retained candidates or foreign pending entries in this increment; design explicit reconciliation/cleanup separately.
- Every copy/hash is bounded and cooperative. One filesystem call can still block; store handles are not a preemptive process deadline or filesystem sandbox.

## Meaningful tests for implementation

1. Move a write handle to a worker thread, retain movie+manifest while the writer performs normal edit/undo/redo, then checkpoint only the exact expected attempt. Confirm authored history is unchanged by retention/checkpoint itself.
2. Drop the writer during preparation; new work and checkpoint fail as session-closed, complete published objects remain, and a reopened session rejects the old token.
3. Inject failure after movie publication, after manifest publication, and before/inside SQLite checkpoint. Verify neither published object is removed and retry deduplicates only identical bytes.
4. Reject symlinked/missing/foreign-owner/writable/hard-linked objects, directory replacement, same-length mutation, wrong BLAKE3/SHA-256/length, wrong project/attempt/revision/document hash, unknown manifest fields and unsupported versions.
5. Read-only store can obtain a reader and verify a retained checkpoint but cannot obtain a writer or create the namespace. Legacy packages without the namespace still open normally.
6. Restart with valid retained bytes: rebuild the contract from the committed revision and run actual verifier admission. Tampered or media-invalid bytes never gain a `VerifiedCandidate` from stored JSON/report fields.
7. Check one shared deadline across staging, both promotions, both guard hashes, snapshot preparation and verifier staging. Include closure/cancellation after publication, and preserve durability errors over later cancellation.
8. Exercise combined-size overflow, manifest cap before allocation, one-object and cumulative byte exhaustion, bounded namespace inventory including orphan survivors, duplicate-content accounting, and competing retained attempts.

No tests above were run for this research task.
