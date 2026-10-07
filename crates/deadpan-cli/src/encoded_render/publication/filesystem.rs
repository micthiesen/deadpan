//! Descriptor-relative, no-replace destination publication. This layer knows
//! neither the media contract nor its hashes. Its caller must validate readback
//! before committing and rehash published bytes before reporting success. Rename
//! can change ctime, so metadata alone cannot establish byte integrity across it.
//! All partials remain on disk, including incomplete ones.

use std::{
    ffi::{OsStr, OsString},
    fs::{File, Metadata},
    io::{self, Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{FileExt, MetadataExt},
    },
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use rustix::fs::{
    AtFlags, CWD, FileType, Mode, OFlags, RenameFlags, Stat, fchmod, fstat, fsync, openat,
    renameat_with, statat,
};

const IO_BYTES: usize = 64 * 1024;
const MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_COMPONENTS: usize = 256;
const MAX_PATH_BYTES: usize = 4096;
const CREATE_ATTEMPTS: usize = 16;
#[cfg(target_os = "macos")]
const UF_TRACKED: u32 = 0x0000_0040;

#[derive(Debug, thiserror::Error)]
#[error("{code}: {operation}: {source}")]
pub(super) struct FsError {
    code: &'static str,
    operation: &'static str,
    published: bool,
    partial_path: Option<PathBuf>,
    #[source]
    source: io::Error,
}

impl FsError {
    pub(super) fn code(&self) -> &'static str {
        self.code
    }
    pub(super) fn published(&self) -> bool {
        self.published
    }
    pub(super) fn partial_path(&self) -> Option<&Path> {
        self.partial_path.as_deref()
    }

    fn new(code: &'static str, operation: &'static str, source: impl Into<io::Error>) -> Self {
        let source = source.into();
        // Name the conditions a person can act on: free space or fix access.
        // Every filesystem stage (copy, report, durability) can hit these.
        let code = match (code, source.kind()) {
            (_, io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded) => "destination_full",
            (_, io::ErrorKind::ReadOnlyFilesystem) => "destination_read_only",
            ("destination_io", io::ErrorKind::PermissionDenied) => "destination_permission_denied",
            (code, _) => code,
        };
        Self {
            code,
            operation,
            source,
            published: false,
            partial_path: None,
        }
    }

    pub(super) fn invalid(code: &'static str, message: &'static str) -> Self {
        Self::new(code, message, io::Error::other(message))
    }

    fn after_rename(mut self) -> Self {
        self.published = true;
        self
    }

    fn with_partial(mut self, path: PathBuf) -> Self {
        self.partial_path = Some(path);
        self
    }
}

type Result<T> = std::result::Result<T, FsError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    device: i128,
    inode: i128,
    owner: i128,
    mode: i128,
}

impl Identity {
    fn stat(value: &Stat) -> Self {
        Self {
            device: i128::from(value.st_dev),
            inode: i128::from(value.st_ino),
            owner: i128::from(value.st_uid),
            mode: i128::from(value.st_mode),
        }
    }
    fn metadata(value: &Metadata) -> Self {
        Self {
            device: i128::from(value.dev()),
            inode: i128::from(value.ino()),
            owner: i128::from(value.uid()),
            mode: i128::from(value.mode()),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct PathEntry {
    path: PathBuf,
    identity: Identity,
}

struct Directory {
    file: File,
    selected: PathBuf,
    canonical: PathBuf,
    selected_entries: Vec<PathEntry>,
    canonical_entries: Vec<Identity>,
    identity: Identity,
    owner: u32,
}

/// Reusable pin for sibling report/movie destinations. Neither directory
/// ownership nor device is inferred from the project or its source media.
#[derive(Clone)]
pub(super) struct Destination {
    directory: Arc<Directory>,
    final_name: OsString,
}

impl Destination {
    pub(super) fn pin(parent: &Path, final_name: &OsStr) -> Result<Self> {
        validate_name(final_name)?;
        let directory = Directory::open(parent)?;
        let result = Self {
            directory,
            final_name: final_name.to_os_string(),
        };
        result.require_absent()?;
        Ok(result)
    }

    pub(super) fn for_name(&self, final_name: &OsStr) -> Result<Self> {
        validate_name(final_name)?;
        self.directory.confirm()?;
        let result = Self {
            directory: Arc::clone(&self.directory),
            final_name: final_name.to_os_string(),
        };
        result.require_absent()?;
        Ok(result)
    }

    pub(super) fn path(&self) -> PathBuf {
        self.directory.canonical.join(&self.final_name)
    }

    fn require_absent(&self) -> Result<()> {
        match statat(
            &self.directory.file,
            &self.final_name,
            AtFlags::SYMLINK_NOFOLLOW,
        ) {
            Err(rustix::io::Errno::NOENT) => Ok(()),
            Ok(_) => Err(FsError::invalid(
                "destination_exists",
                "destination already exists",
            )),
            Err(e) => Err(FsError::new(
                "destination_io",
                "inspect destination entry",
                e,
            )),
        }
    }

    pub(super) fn create_partial(&self, maximum_bytes: u64) -> Result<PartialFile> {
        for _ in 0..CREATE_ATTEMPTS {
            let name = OsString::from(format!(".deadpan-{}.partial", uuid::Uuid::new_v4()));
            match self.create_partial_named(&name, maximum_bytes) {
                Err(error) if error.code() == "partial_exists" => continue,
                result => return result,
            }
        }
        Err(FsError::invalid(
            "destination_io",
            "exclusive partial names exhausted",
        ))
    }

    /// Create exactly the basename already recorded by the operational journal.
    /// Existing entries are never opened, modified, or removed.
    pub(super) fn create_partial_named(
        &self,
        name: &OsStr,
        maximum_bytes: u64,
    ) -> Result<PartialFile> {
        validate_name(name)?;
        if name == self.final_name {
            return Err(FsError::invalid(
                "invalid_destination",
                "partial and final names must differ",
            ));
        }
        if !(1..=MAX_BYTES).contains(&maximum_bytes) {
            return Err(FsError::invalid(
                "partial_limit",
                "partial byte bound must be within 1..=64 GiB",
            ));
        }
        self.directory.confirm()?;
        self.require_absent()?;
        let descriptor = openat(
            &self.directory.file,
            name,
            OFlags::RDWR
                | OFlags::CREATE
                | OFlags::EXCL
                | OFlags::NOFOLLOW
                | OFlags::NONBLOCK
                | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|e| {
            FsError::new(
                if e == rustix::io::Errno::EXIST {
                    "partial_exists"
                } else {
                    "destination_io"
                },
                "create exclusive partial",
                e,
            )
        })?;
        let file = File::from(descriptor);
        let path = self.directory.canonical.join(name);
        lock_file(&file).map_err(|e| e.with_partial(path.clone()))?;
        fchmod(&file, Mode::RUSR | Mode::WUSR).map_err(|e| {
            FsError::new("destination_io", "set partial owner permissions", e)
                .with_partial(path.clone())
        })?;
        let state = inspect(&file).map_err(|e| e.with_partial(path.clone()))?;
        validate_file(&state, &self.directory, maximum_bytes)
            .map_err(|e| e.with_partial(path.clone()))?;
        if state.st_size != 0 {
            return Err(
                FsError::invalid("destination_changed", "new partial is not empty")
                    .with_partial(path),
            );
        }
        let partial = PartialFile {
            destination: self.clone(),
            name: name.to_owned(),
            file,
            state,
            written: 0,
            maximum_bytes,
            sealed: false,
            poisoned: false,
            published: false,
        };
        partial
            .confirm()
            .map_err(|e| e.with_partial(path.clone()))?;
        self.directory.confirm().map_err(|e| e.with_partial(path))?;
        Ok(partial)
    }
}

impl Directory {
    fn open(parent: &Path) -> Result<Arc<Self>> {
        let selected = std::path::absolute(parent)
            .map_err(|e| FsError::new("invalid_destination", "resolve destination path", e))?;
        validate_path(&selected)?;
        let selected_entries = path_entries(&selected)?;
        let canonical = std::fs::canonicalize(&selected).map_err(|e| {
            FsError::new(
                "invalid_destination",
                "canonicalize destination directory",
                e,
            )
        })?;
        validate_path(&canonical)?;
        let (file, canonical_entries) = open_directory_chain(&canonical)?;
        let metadata = inspect(&file)?;
        let directory = Arc::new(Directory {
            file,
            selected,
            canonical,
            selected_entries,
            canonical_entries,
            identity: Identity::stat(&metadata),
            owner: rustix::process::geteuid().as_raw(),
        });
        directory.confirm()?;
        Ok(directory)
    }

    fn confirm(&self) -> Result<()> {
        if path_entries(&self.selected)? != self.selected_entries {
            return Err(FsError::invalid(
                "destination_changed",
                "selected directory path was replaced",
            ));
        }
        let canonical = std::fs::canonicalize(&self.selected).map_err(|e| {
            FsError::new(
                "destination_changed",
                "resolve pinned destination directory",
                e,
            )
        })?;
        if canonical != self.canonical {
            return Err(FsError::invalid(
                "destination_changed",
                "selected directory resolution changed",
            ));
        }
        let (file, identities) = open_directory_chain(&self.canonical)?;
        if identities != self.canonical_entries
            || Identity::stat(&inspect(&file)?) != self.identity
            || Identity::stat(&inspect(&self.file)?) != self.identity
        {
            return Err(FsError::invalid(
                "destination_changed",
                "pinned destination directory changed",
            ));
        }
        Ok(())
    }
}

/// The only writable capability. No descriptor escapes, writes are bounded,
/// and sealing permanently revokes this API's writer access. No Drop cleanup
/// unlinks names: a failure may leave useful bytes or a foreign replacement.
pub(super) struct PartialFile {
    destination: Destination,
    name: OsString,
    file: File,
    state: Stat,
    written: u64,
    maximum_bytes: u64,
    sealed: bool,
    poisoned: bool,
    published: bool,
}

impl PartialFile {
    /// The original partial name, including after successful rename.
    pub(super) fn path(&self) -> PathBuf {
        self.destination.directory.canonical.join(&self.name)
    }
    pub(super) fn is_published(&self) -> bool {
        self.published
    }

    pub(super) fn writer<'a>(
        &'a mut self,
        cancelled: &'a AtomicBool,
        deadline: Instant,
    ) -> Result<PartialWriter<'a>> {
        check_control(cancelled, deadline)?;
        if self.sealed || self.published || self.poisoned {
            return Err(FsError::invalid(
                "partial_state",
                "partial no longer accepts writes",
            ));
        }
        self.destination.directory.confirm()?;
        self.confirm()?;
        Ok(PartialWriter {
            partial: self,
            cancelled,
            deadline,
        })
    }

    pub(super) fn seal(
        &mut self,
        expected_bytes: u64,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<()> {
        check_control(cancelled, deadline)?;
        if self.sealed
            || self.published
            || self.poisoned
            || expected_bytes == 0
            || expected_bytes != self.written
            || expected_bytes > self.maximum_bytes
        {
            return Err(FsError::invalid(
                "partial_state",
                "partial length or state does not permit sealing",
            ));
        }
        self.destination.directory.confirm()?;
        self.confirm()?;
        // Seal even when synchronization fails, so a caller cannot accidentally
        // overwrite a complete partial retained for recovery.
        self.sealed = true;
        full_sync(&self.file)
            .map_err(|e| FsError::new("publication_durability", "sync complete partial", e))?;
        fsync(&self.destination.directory.file).map_err(|e| {
            FsError::new("publication_durability", "sync partial directory entry", e)
        })?;
        full_sync(&self.file).map_err(|e| {
            FsError::new(
                "publication_durability",
                "complete partial durability barrier",
                e,
            )
        })?;
        self.confirm()?;
        self.destination.directory.confirm()?;
        check_control(cancelled, deadline)
    }

    pub(super) fn reader<'a>(
        &'a self,
        cancelled: &'a AtomicBool,
        deadline: Instant,
    ) -> Result<PartialReader<'a>> {
        check_control(cancelled, deadline)?;
        if !self.sealed || self.published || self.poisoned {
            return Err(FsError::invalid(
                "partial_state",
                "partial is not sealed for readback",
            ));
        }
        self.destination.directory.confirm()?;
        self.confirm()?;
        Ok(PartialReader {
            partial: self,
            offset: 0,
            cancelled,
            deadline,
        })
    }

    /// Read the retained published descriptor with the same bounded, stable-state
    /// checks as partial readback. The caller compares its bytes with the hash
    /// established before rename; successful commit alone does not prove this.
    pub(super) fn published_reader<'a>(
        &'a self,
        cancelled: &'a AtomicBool,
        deadline: Instant,
    ) -> Result<PartialReader<'a>> {
        if !self.sealed || !self.published || self.poisoned {
            let error = FsError::invalid("partial_state", "file is not published for readback");
            return Err(if self.published {
                error.after_rename()
            } else {
                error
            });
        }
        check_control(cancelled, deadline).map_err(FsError::after_rename)?;
        self.confirm_published()?;
        Ok(PartialReader {
            partial: self,
            offset: 0,
            cancelled,
            deadline,
        })
    }

    #[cfg(test)]
    pub(super) fn commit(&mut self, cancelled: &AtomicBool, deadline: Instant) -> Result<()> {
        self.commit_with_sync(cancelled, deadline, |phase, file| match phase {
            SyncPhase::PublishedDirectory => fsync(file).map_err(Into::into),
            _ => full_sync(file),
        })
    }

    /// The live permit is checked at the last pre-rename point. Once rename
    /// succeeds, cancellation and a revoked permit cannot abandon durability.
    pub(super) fn commit_guarded(
        &mut self,
        cancelled: &AtomicBool,
        deadline: Instant,
        mut guard: impl FnMut() -> Result<()>,
    ) -> Result<()> {
        self.commit_with_guard(
            cancelled,
            deadline,
            &mut |phase, file| match phase {
                SyncPhase::PublishedDirectory => fsync(file).map_err(Into::into),
                _ => full_sync(file),
            },
            &mut guard,
        )
    }

    pub(super) fn confirm_published(&self) -> Result<()> {
        let result = if self.published {
            self.confirm()
                .and_then(|()| self.destination.directory.confirm())
        } else {
            Err(FsError::invalid(
                "partial_state",
                "file has not been published",
            ))
        };
        result.map_err(|error| {
            if self.published {
                error.after_rename()
            } else {
                error
            }
        })
    }

    /// Admit only macOS adding its document-tracking flag. The caller must
    /// immediately rehash the complete expected bytes: the accompanying ctime
    /// change could also conceal a write with restored mtime. The new state
    /// keeps UF_TRACKED set, so this cannot permit a second metadata rebase.
    pub(super) fn rebase_published_tracking(&mut self) -> Result<bool> {
        if !self.sealed || !self.published || self.poisoned {
            return Err(FsError::invalid(
                "partial_state",
                "file is not published for tracking readback",
            ));
        }
        self.destination.directory.confirm()?;
        let state = self.current_state()?;
        if !tracking_added(&self.state, &state) {
            return Ok(false);
        }
        self.state = state;
        self.confirm_published()?;
        Ok(true)
    }

    #[cfg(test)]
    fn commit_with_sync(
        &mut self,
        cancelled: &AtomicBool,
        deadline: Instant,
        mut sync: impl FnMut(SyncPhase, &File) -> io::Result<()>,
    ) -> Result<()> {
        self.commit_with_guard(cancelled, deadline, &mut sync, &mut || Ok(()))
    }

    fn commit_with_guard(
        &mut self,
        cancelled: &AtomicBool,
        deadline: Instant,
        sync: &mut impl FnMut(SyncPhase, &File) -> io::Result<()>,
        guard: &mut impl FnMut() -> Result<()>,
    ) -> Result<()> {
        let result = self.commit_inner(cancelled, deadline, sync, guard);
        result.map_err(|error| {
            let error = error.with_partial(self.path());
            if self.published {
                error.after_rename()
            } else {
                error
            }
        })
    }

    fn commit_inner(
        &mut self,
        cancelled: &AtomicBool,
        deadline: Instant,
        sync: &mut impl FnMut(SyncPhase, &File) -> io::Result<()>,
        guard: &mut impl FnMut() -> Result<()>,
    ) -> Result<()> {
        check_control(cancelled, deadline)?;
        if !self.sealed || self.poisoned || self.published {
            return Err(FsError::invalid(
                "partial_state",
                "partial is not eligible for publication",
            ));
        }
        self.destination.directory.confirm()?;
        self.confirm()?;
        self.destination.require_absent()?;
        sync(SyncPhase::BeforeRename, &self.file)
            .map_err(|e| FsError::new("publication_durability", "sync partial before rename", e))?;
        self.destination.directory.confirm()?;
        self.confirm()?;
        check_control(cancelled, deadline)?;
        guard()?;
        renameat_with(
            &self.destination.directory.file,
            &self.name,
            &self.destination.directory.file,
            &self.destination.final_name,
            RenameFlags::NOREPLACE,
        )
        .map_err(|e| {
            FsError::new(
                if e == rustix::io::Errno::EXIST {
                    "destination_exists"
                } else {
                    "publication_io"
                },
                "atomically publish without replacement",
                e,
            )
        })?;
        self.published = true;

        // From here on, never remove the final file or exit early on control.
        // Sync the retained descriptor even if a namespace entry was replaced.
        sync(SyncPhase::PublishedFile, &self.file)
            .map_err(|e| FsError::new("publication_durability", "sync published file", e))?;
        sync(
            SyncPhase::PublishedDirectory,
            &self.destination.directory.file,
        )
        .map_err(|e| {
            FsError::new(
                "publication_durability",
                "sync published directory entry",
                e,
            )
        })?;
        sync(SyncPhase::AfterDirectory, &self.file).map_err(|e| {
            FsError::new(
                "publication_durability",
                "complete publication durability barrier",
                e,
            )
        })?;
        let after = inspect(&self.file)?;
        validate_file(&after, &self.destination.directory, self.maximum_bytes)?;
        // Rename may change ctime, and macOS may add its document-tracking
        // flag. Every other field must match; mandatory post-rename readback
        // proves the bytes under this newly captured exact state.
        if !same_state(&self.state, &after, false) && !tracking_added(&self.state, &after) {
            return Err(FsError::invalid(
                "destination_changed",
                "published descriptor differs from sealed bytes",
            ));
        }
        self.state = after;
        if let Err(error) = self.confirm()
            && !self.rebase_published_tracking()?
        {
            return Err(error);
        }
        self.destination.directory.confirm()?;
        Ok(())
    }

    fn confirm(&self) -> Result<()> {
        let state = self.current_state()?;
        if !same_state(&self.state, &state, true) {
            return Err(FsError::invalid(
                "destination_changed",
                "stable descriptor state changed",
            ));
        }
        Ok(())
    }

    fn current_state(&self) -> Result<Stat> {
        let descriptor = inspect(&self.file)?;
        validate_file(&descriptor, &self.destination.directory, self.maximum_bytes)?;
        let name = if self.published {
            &self.destination.final_name
        } else {
            &self.name
        };
        let named = statat(
            &self.destination.directory.file,
            name,
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(|e| FsError::new("destination_changed", "inspect pinned file entry", e))?;
        validate_file(&named, &self.destination.directory, self.maximum_bytes)?;
        if !same_state(&descriptor, &named, true) {
            return Err(FsError::invalid(
                "destination_changed",
                "file entry differs from retained descriptor",
            ));
        }
        Ok(descriptor)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyncPhase {
    BeforeRename,
    PublishedFile,
    PublishedDirectory,
    AfterDirectory,
}

pub(super) struct PartialWriter<'a> {
    partial: &'a mut PartialFile,
    cancelled: &'a AtomicBool,
    deadline: Instant,
}

impl Write for PartialWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        check_control(self.cancelled, self.deadline).map_err(io::Error::other)?;
        if self.partial.poisoned {
            return Err(io::Error::other(FsError::invalid(
                "partial_state",
                "partial writer is poisoned",
            )));
        }
        self.partial.confirm().map_err(io::Error::other)?;
        if bytes.is_empty() {
            return Ok(0);
        }
        let remaining = self.partial.maximum_bytes - self.partial.written;
        let count = bytes
            .len()
            .min(IO_BYTES)
            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
        if count == 0 {
            return Err(io::Error::other(FsError::invalid(
                "partial_limit",
                "partial exceeds its byte bound",
            )));
        }
        let written = match self
            .partial
            .file
            .write_at(&bytes[..count], self.partial.written)
        {
            Ok(written) => written,
            Err(error) => {
                self.partial.poisoned = true;
                return Err(io::Error::other(FsError::new(
                    "destination_io",
                    "write partial",
                    error,
                )));
            }
        };
        if written == 0 {
            self.partial.poisoned = true;
            return Err(io::Error::other(FsError::new(
                "destination_io",
                "write partial",
                io::Error::new(io::ErrorKind::WriteZero, "partial write made no progress"),
            )));
        }
        self.partial.written += written as u64;
        let after = inspect(&self.partial.file).map_err(io::Error::other)?;
        if Identity::stat(&after) != Identity::stat(&self.partial.state)
            || after.st_nlink != 1
            || u64::try_from(after.st_size).ok() != Some(self.partial.written)
        {
            self.partial.poisoned = true;
            return Err(io::Error::other(FsError::invalid(
                "destination_changed",
                "partial changed during write",
            )));
        }
        self.partial.state = after;
        check_control(self.cancelled, self.deadline).map_err(io::Error::other)?;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        check_control(self.cancelled, self.deadline).map_err(io::Error::other)?;
        self.partial.confirm().map_err(io::Error::other)
    }
}

pub(super) struct PartialReader<'a> {
    partial: &'a PartialFile,
    offset: u64,
    cancelled: &'a AtomicBool,
    deadline: Instant,
}

impl Read for PartialReader<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let published = self.partial.published;
        let io_error = |error: FsError| {
            io::Error::other(if published {
                error.after_rename()
            } else {
                error
            })
        };
        check_control(self.cancelled, self.deadline).map_err(io_error)?;
        self.partial.confirm().map_err(io_error)?;
        let remaining = self.partial.written - self.offset;
        let count = bytes
            .len()
            .min(IO_BYTES)
            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
        if count == 0 {
            return Ok(0);
        }
        let read = self
            .partial
            .file
            .read_at(&mut bytes[..count], self.offset)
            .map_err(|e| io_error(FsError::new("destination_io", "read sealed file", e)))?;
        if read == 0 {
            return Err(io_error(FsError::new(
                "destination_changed",
                "read sealed file",
                io::Error::new(io::ErrorKind::UnexpectedEof, "sealed file ended early"),
            )));
        }
        self.partial.confirm().map_err(io_error)?;
        check_control(self.cancelled, self.deadline).map_err(io_error)?;
        self.offset += read as u64;
        Ok(read)
    }
}

fn check_control(cancelled: &AtomicBool, deadline: Instant) -> Result<()> {
    if cancelled.load(Ordering::Acquire) {
        return Err(FsError::new(
            "cancelled",
            "publication cancelled",
            io::Error::new(io::ErrorKind::Interrupted, "cancelled"),
        ));
    }
    if Instant::now() >= deadline {
        return Err(FsError::new(
            "deadline_exceeded",
            "publication deadline elapsed",
            io::Error::new(io::ErrorKind::TimedOut, "deadline elapsed"),
        ));
    }
    Ok(())
}

fn lock_file(file: &File) -> Result<()> {
    file.try_lock().map_err(|error| match error {
        std::fs::TryLockError::WouldBlock => {
            FsError::invalid("publication_locked", "publication file is in use")
        }
        std::fs::TryLockError::Error(error) => {
            FsError::new("destination_io", "lock publication file", error)
        }
    })
}

fn inspect(file: &File) -> Result<Stat> {
    fstat(file).map_err(|e| FsError::new("destination_io", "inspect retained descriptor", e))
}

fn validate_file(value: &Stat, directory: &Directory, maximum: u64) -> Result<()> {
    if !FileType::from_raw_mode(value.st_mode).is_file()
        || value.st_nlink != 1
        || i128::from(value.st_dev) != directory.identity.device
        || value.st_uid != directory.owner
        || value.st_mode & (Mode::WGRP | Mode::WOTH).bits() != 0
        || u64::try_from(value.st_size)
            .ok()
            .is_none_or(|size| size > maximum)
    {
        return Err(FsError::invalid(
            "destination_changed",
            "file is not an owned bounded single-link regular file",
        ));
    }
    Ok(())
}

fn same_state(left: &Stat, right: &Stat, include_ctime: bool) -> bool {
    Identity::stat(left) == Identity::stat(right)
        && same_platform_state(left, right)
        && left.st_gid == right.st_gid
        && left.st_size == right.st_size
        && left.st_nlink == right.st_nlink
        && left.st_mtime == right.st_mtime
        && left.st_mtime_nsec == right.st_mtime_nsec
        && (!include_ctime
            || (left.st_ctime == right.st_ctime && left.st_ctime_nsec == right.st_ctime_nsec))
}

#[cfg(target_os = "macos")]
fn tracked_flag_added(before: u32, after: u32) -> bool {
    before & UF_TRACKED == 0 && after == (before | UF_TRACKED)
}

#[cfg(not(target_os = "macos"))]
fn tracked_flag_added(_before: u32, _after: u32) -> bool {
    false
}

#[cfg(target_os = "macos")]
fn tracking_added(before: &Stat, after: &Stat) -> bool {
    if !tracked_flag_added(before.st_flags, after.st_flags) {
        return false;
    }
    let mut without_tracking = *after;
    without_tracking.st_flags = before.st_flags;
    same_state(before, &without_tracking, false)
}

#[cfg(not(target_os = "macos"))]
fn tracking_added(_before: &Stat, _after: &Stat) -> bool {
    false
}

#[cfg(target_os = "macos")]
fn same_platform_state(left: &Stat, right: &Stat) -> bool {
    left.st_birthtime == right.st_birthtime
        && left.st_birthtime_nsec == right.st_birthtime_nsec
        && left.st_gen == right.st_gen
        && left.st_flags == right.st_flags
}

#[cfg(not(target_os = "macos"))]
fn same_platform_state(_left: &Stat, _right: &Stat) -> bool {
    true
}

fn validate_name(name: &OsStr) -> Result<()> {
    let bytes = name.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 255
        || bytes.contains(&0)
        || bytes.contains(&b'/')
        || name == OsStr::new(".")
        || name == OsStr::new("..")
    {
        return Err(FsError::invalid(
            "invalid_destination",
            "final name must be one bounded basename",
        ));
    }
    Ok(())
}

fn validate_path(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path.as_os_str().as_bytes().len() > MAX_PATH_BYTES
        || path.components().count() > MAX_COMPONENTS
    {
        return Err(FsError::invalid(
            "invalid_destination",
            "destination path exceeds its bounds",
        ));
    }
    Ok(())
}

fn path_entries(path: &Path) -> Result<Vec<PathEntry>> {
    let mut prefix = PathBuf::new();
    let mut entries = Vec::new();
    for component in path.components() {
        prefix.push(component.as_os_str());
        let metadata = std::fs::symlink_metadata(&prefix).map_err(|e| {
            FsError::new("destination_changed", "inspect selected directory path", e)
        })?;
        entries.push(PathEntry {
            path: prefix.clone(),
            identity: Identity::metadata(&metadata),
        });
    }
    Ok(entries)
}

fn open_directory_chain(path: &Path) -> Result<(File, Vec<Identity>)> {
    validate_path(path)?;
    let flags =
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let mut file = File::from(
        openat(CWD, "/", flags, Mode::empty())
            .map_err(|e| FsError::new("destination_io", "open destination root", e))?,
    );
    let mut entries = vec![Identity::stat(&inspect(&file)?)];
    for component in path.components() {
        match component {
            Component::RootDir => continue,
            Component::Normal(name) => {
                file = File::from(openat(&file, name, flags, Mode::empty()).map_err(|e| {
                    FsError::new("destination_changed", "open pinned directory component", e)
                })?);
                entries.push(Identity::stat(&inspect(&file)?));
            }
            _ => {
                return Err(FsError::invalid(
                    "invalid_destination",
                    "canonical directory contains a non-normal component",
                ));
            }
        }
    }
    Ok((file, entries))
}

#[cfg(target_os = "macos")]
fn full_sync(file: &File) -> io::Result<()> {
    rustix::fs::fcntl_fullfsync(file).map_err(Into::into)
}

#[cfg(target_os = "linux")]
fn full_sync(file: &File) -> io::Result<()> {
    fsync(file).map_err(Into::into)
}

mod recovery;
pub(super) use recovery::{DirectoryEvidence, FileEvidence, RecoveredDirectory, RecoveredFile};

#[cfg(test)]
mod tests;
