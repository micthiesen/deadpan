# Filesystem API implementation

Ownership: `native/deadpan-filesystem/`, `publication/filesystem.rs`, `publication/filesystem/recovery.rs` and filesystem tests. Parent must remove any `PartialFile<'_>` annotations outside these files.

The code is now present. No build, tests, formatter or native program has run from this worker.

```rust
// Existing Destination/PartialFile are pub(super) to publication.
Destination::pin(parent: &Path, final_name: &OsStr) -> Result<Destination>
Destination::for_name(&self, final_name: &OsStr) -> Result<Destination>
Destination::path(&self) -> PathBuf
Destination::create_partial(&self, maximum_bytes: u64) -> Result<PartialFile>
Destination::create_partial_named(&self, name: &OsStr, maximum_bytes: u64) -> Result<PartialFile>
Destination::evidence(&self) -> Result<DirectoryEvidence>
PartialFile::evidence(&self) -> Result<FileEvidence>
PartialFile::commit_guarded(&mut self, cancelled: &AtomicBool, deadline: Instant,
    guard: impl FnMut() -> Result<()>) -> Result<()>
// Existing commit, writer, reader, published_reader, confirm_published remain.
// PartialFile owns a cloned Destination and retains an exclusive File::try_lock.
FsError::invalid(code: &'static str, message: &'static str) -> FsError
```

`DirectoryEvidence` and `FileEvidence` are cloned strict serde records with private fields. Capture is APFS-only and fails `unsupported_recovery_identity` if native volume identity cannot be qualified. Unix paths use raw byte arrays. `DirectoryEvidence::selected() -> &Path`, `canonical() -> &Path`, `validate() -> Result<()>`; `FileEvidence::byte_length() -> u64`, `validate() -> Result<()>`.

```rust
RecoveredDirectory::open(evidence: &DirectoryEvidence) -> Result<RecoveredDirectory>
RecoveredDirectory::confirm(&self) -> Result<()>
RecoveredDirectory::open_file(&self, name: &OsStr,
    evidence: &FileEvidence) -> Result<Option<RecoveredFile>>
RecoveredFile::reader<'a>(&'a self, cancelled: &'a AtomicBool, deadline: Instant)
    -> Result<RecoveredReader<'a>> // implements Read, at most 64KiB per read
RecoveredFile::confirm(&self) -> Result<()>
RecoveredFile::sync_verified(&self) -> Result<()>
```

Import `RecoveredDirectory` from `filesystem`; `RecoveredFile`/`RecoveredReader` can be inferred, or re-export if needed. `open_file` returns `None` only for absent recorded basename; foreign/replaced entries fail before byte reads. Lock contention is `publication_locked`. Open pair in movie then report order. Keep both handles alive through host hashing and final records. `sync_verified` is trusted-host-only: call it only after exact expected SHA and byte extent comparison. It does file full-sync, directory fsync, file full-sync, rechecks identities/current state. It offers no rename/delete/resume authority.

Saved pre-rename identity includes APFS volume UUID, device/inode, birth seconds+nanos, optional nonzero st_gen, owner/group/mode/flags; FileEvidence also includes exact extent, single link and mtime. ctime is allowed to change at rename but newly reopened state includes ctime and must remain stable during all reads. Directory evidence records selected component identities including symlink target bytes and canonical component identities. Scope is cooperative replacement detection, not malicious same-user authentication.
