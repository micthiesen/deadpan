# Publication filesystem identity and recovery API

Read-only handoff for the filesystem implementation worker. No repository source was changed for this note.

## Keep ordinary publication unchanged

`Destination::pin` and the current publish path remain usable on filesystems that cannot provide qualified persistent identity. Add evidence capture/reopen as an optional capability. If it is unsupported, the caller must leave restart reconciliation unresolved; it must not fail an otherwise valid standalone publication or infer identity from byte equality.

## Private durable evidence

Use strict, versioned, bounded serde types scoped to `encoded_render::publication::filesystem` and exposed only `pub(super)`:

- `DirectoryEvidence { version, selected_path_bytes, canonical_path_bytes, selected_chain, canonical_chain, destination_identity }`
- `FileEvidence { version, object_identity, owner, mode, length, link_count, mtime_sec, mtime_nsec, captured_ctime_sec, captured_ctime_nsec }`
- `ObjectIdentity { volume_uuid: [u8; 16], device: u64, inode: u64, birth_sec: i64, birth_nsec: u32, generation: Option<u32> }`
- Each path-chain element records raw Unix path/component bytes and its lstat/opened identity, including symlink entries. Bound total path bytes to the existing 4096-byte limit and component count to 256. Reject unknown fields, unsupported versions, malformed byte counts, nanoseconds outside `0..1_000_000_000`, zero volume UUIDs, and generation zero (serialize it as `None`).

For durable capture, require APFS, a nonzero descriptor-derived volume UUID, and a real birthtime (do not accept `stat`'s ctime fallback). On current macOS, obtain birth sec/nsec and `st_gen` through the safe Darwin `MetadataExt` on descriptor metadata; `st_gen == 0` means unavailable. The narrow platform adapter is only needed for descriptor-derived APFS qualification and volume UUID. Do not use `ATTR_CMN_GEN_COUNT` as the inode generation: it is a content-change count, and zero is invalid.

Evidence is cooperative recovery evidence, not a cryptographic identity. The host still hashes all bytes after reopening. It cannot defend against a malicious process with the same account deliberately forging metadata or ignoring advisory locks.

## Suggested filesystem surface

- `Destination: Clone` (the existing directory is already `Arc<Directory>`).
- `Destination::durable_evidence() -> Result<DirectoryEvidence>`: re-confirm the selected path, canonical path, path-entry chain, open directory chain, volume UUID, and APFS type before recording.
- `Destination::create_partial_named(name, maximum_bytes) -> Result<PartialFile>`: use the exact validated basename with `CREATE|EXCL|NOFOLLOW|NONBLOCK`; retain `create_partial` as the random-name wrapper.
- `PartialFile` owns `Destination` (remove the lifetime), captures `FileEvidence` only for its sealed descriptor, and acquires/retains an exclusive `File::try_lock` immediately after creation.
- `PartialFile::commit_guarded(cancelled, deadline, guard)` calls the supplied checkpoint guard at the start of precommit and again after all pre-rename syncing/identity checks immediately before `renameat(NOREPLACE)`. After rename succeeds, do not check cancellation or the guard; finish durability and report a committed-but-unconfirmed error when later checks fail.
- `reopen_existing(directory_evidence, basename, file_evidence, maximum_bytes) -> Result<RecoveredFile>` only opens an already-existing recorded partial/final basename. Use descriptor-relative `openat` with `RDONLY|NOFOLLOW|NONBLOCK|CLOEXEC`; never create, rename, unlink, chmod, or repair an entry.
- Recovery checks the selected and canonical path chains and directory identity first. Then compare opened-descriptor and no-follow named-entry identity, regular-file type, owner, exact allowed mode, size bound, and `nlink == 1` before reading. Compare stable object identity plus owner/mode/length/nlink/mtime to the persisted evidence. Ignore persisted ctime once because the one authorized partial-to-final rename may change it. Capture the reopened file's *current* full stat as the baseline; every bounded read checks descriptor and name before/after each chunk with ctime included.
- `RecoveredFile` is an owned read-only capability with a bounded `Read` implementation. `sync_verified(expected_len, expected_digest, observed_digest, guard)` is callable only after a complete exact-length read; it checks equality, re-confirms stable current metadata, rechecks the host's checkpoint guard, and only then performs the durability barrier. Document that `observed_digest` must be the digest computed by the host over this same completed read. Keep the method private to the publication parent module.

## Locking movie and report

The publication host owns both partial/recovered descriptors through the complete stage. Hold each exclusive `try_lock` through publication or readback. Recovery acquires movie then report in the same fixed order, and if either lock is busy or errors it drops any already-acquired lock and returns unresolved without touching either entry. A nonblocking lock detects a still-live old worker after the store has reopened; process exit releases it, so there is no stale lockfile cleanup protocol. These are advisory coordination locks: the path/descriptor checks and host hash still matter.

## Meaningful source tests for the implementing worker

- strict serde rejects unknown version/fields and out-of-bound paths/components;
- unsupported filesystem or missing birthtime/UUID yields explicit unsupported evidence while existing publish remains available;
- replacing a file with identical bytes and restored mtime fails identity matching;
- a legitimate partial-to-final rename passes the initial evidence check despite ctime change, while later mutation during a read fails the current-state check;
- symlink-chain substitution, canonical-path substitution, hardlink addition, nonregular file, mode/owner/extent change, and lock contention all fail closed without changing any foreign path;
- named create never overwrites, and commit guard revocation immediately before rename leaves the complete partial and no final entry;
- the post-rename durability path ignores cancellation/guard revocation and reports the actual committed state.
