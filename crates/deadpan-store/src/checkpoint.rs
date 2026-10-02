//! Consistent database checkpoints prepared without borrowing the writer.
//!
//! A worker opens its own read-only SQLite connection and pins a read
//! transaction before copying bounded batches with SQLite's backup API. The
//! writer only admits the handle and publishes the finished file. Media remains
//! in the package; a database checkpoint is not a portable project copy.
//!
//! Cancellation and deadlines are cooperative between SQLite and filesystem
//! calls, not a preemptive wall-time guarantee. Namespace checks assume trusted
//! same-user code, as do the store's other descriptor-based capabilities.

use std::fs::File;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_core::{ProjectId, RevisionId};
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
};
use rustix::fs::{
    AtFlags, CWD, FileType, Mode, OFlags, RenameFlags, Stat, fstat, fsync, mkdirat, openat,
    renameat_with, statat, unlinkat,
};
use serde::{Deserialize, Serialize};

use crate::host_owner::{PackageIdentity, WriterOwnerHandle};
use crate::{ProjectStore, StoreError, read_snapshot, schema};

const DATABASE_NAME: &str = "project.sqlite";
const SNAPSHOTS_NAME: &str = "Snapshots";
const STAGED_NAME: &str = "checkpoint.sqlite";
const MAX_DATABASE_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_PAGES_PER_STEP: u32 = 256;
const MAX_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const DIRECTORY: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const READ_FILE: OFlags = OFlags::RDONLY
    .union(OFlags::NOFOLLOW)
    .union(OFlags::NONBLOCK)
    .union(OFlags::CLOEXEC);

#[derive(Debug, Clone, Copy)]
pub struct CheckpointLimits {
    pub max_database_bytes: u64,
    pub pages_per_step: u32,
    pub timeout: Duration,
}

impl Default for CheckpointLimits {
    fn default() -> Self {
        Self {
            max_database_bytes: 1024 * 1024 * 1024,
            pages_per_step: 32,
            timeout: Duration::from_secs(5 * 60),
        }
    }
}

impl CheckpointLimits {
    fn validate(self) -> Result<Self, CheckpointError> {
        if self.max_database_bytes == 0
            || self.max_database_bytes > MAX_DATABASE_BYTES
            || self.pages_per_step == 0
            || self.pages_per_step > MAX_PAGES_PER_STEP
            || self.timeout.is_zero()
            || self.timeout > MAX_TIMEOUT
        {
            return Err(CheckpointError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointProgress {
    pub pages_copied: u64,
    pub total_pages: u64,
}

/// An observation of the completed backup, not the revision at admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointReceipt {
    pub path: PathBuf,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub database_bytes: u64,
}

/// Connection-free worker capability. It retains descriptors but no writer lock.
pub struct CheckpointHandle {
    owner: WriterOwnerHandle,
    package_path: PathBuf,
    package: OwnedFd,
    snapshots: OwnedFd,
    database: OwnedFd,
}

/// A complete, synchronized database that only its original writer can publish.
/// Dropping it removes its private staging entry without publishing a checkpoint.
pub struct PreparedCheckpoint {
    handle: CheckpointHandle,
    staged: StagedCheckpoint,
    receipt: CheckpointReceipt,
    deadline: Instant,
}

impl PreparedCheckpoint {
    pub fn project_id(&self) -> &ProjectId {
        &self.receipt.project_id
    }

    pub fn revision_id(&self) -> &RevisionId {
        &self.receipt.revision_id
    }
}

impl ProjectStore {
    /// Bounded metadata admission only; move the returned capability to a worker.
    pub fn checkpoint_handle(&mut self) -> Result<CheckpointHandle, CheckpointError> {
        let owner = self.writer_owner_handle()?;
        // This pin was captured when the writer opened. Checking it also rejects
        // a database replacement that happened before checkpoint admission.
        self.check_checkpoint_database()?;
        let handle = CheckpointHandle {
            owner,
            package_path: self.package.clone(),
            package: openat(CWD, &self.package, DIRECTORY, Mode::empty()).map_err(io)?,
            snapshots: openat(
                CWD,
                self.package.join(SNAPSHOTS_NAME),
                DIRECTORY,
                Mode::empty(),
            )
            .map_err(io)?,
            database: openat(
                CWD,
                self.package.join(DATABASE_NAME),
                READ_FILE,
                Mode::empty(),
            )
            .map_err(io)?,
        };
        handle.check()?;
        self.check_checkpoint_database()?;
        self.check_writer_owner(&handle.owner)?;
        Ok(handle)
    }

    /// Publishes a prepared checkpoint with the exact captured revision receipt.
    /// No database copying, parsing, or hashing runs on the writer here.
    pub fn publish_prepared_checkpoint(
        &mut self,
        prepared: PreparedCheckpoint,
        cancelled: &AtomicBool,
    ) -> Result<CheckpointReceipt, CheckpointError> {
        self.publish_checkpoint_with(prepared, cancelled, |directory| fsync(directory))
    }

    fn publish_checkpoint_with(
        &mut self,
        mut prepared: PreparedCheckpoint,
        cancelled: &AtomicBool,
        sync_directory: impl FnOnce(&OwnedFd) -> Result<(), rustix::io::Errno>,
    ) -> Result<CheckpointReceipt, CheckpointError> {
        prepared.handle.control(cancelled, prepared.deadline)?;
        self.check_writer_owner(&prepared.handle.owner)?;
        self.check_checkpoint_database()?;
        prepared.handle.check()?;
        prepared.staged.check(&prepared.handle)?;
        let name = prepared
            .receipt
            .path
            .file_name()
            .ok_or(CheckpointError::IdentityChanged)?;
        prepared.handle.control(cancelled, prepared.deadline)?;
        renameat_with(
            prepared.staged.directory()?,
            STAGED_NAME,
            &prepared.handle.snapshots,
            name,
            RenameFlags::NOREPLACE,
        )
        .map_err(io)?;
        // After rename the checkpoint is retained even if namespace sync fails.
        // Removing a potentially published recovery file would lose evidence.
        prepared.staged.published = true;
        sync_directory(&prepared.handle.snapshots).map_err(|source| {
            CheckpointError::PublishedUnconfirmed {
                receipt: Box::new(prepared.receipt.clone()),
                source: source.into(),
            }
        })?;
        Ok(prepared.receipt.clone())
    }

    fn check_checkpoint_database(&mut self) -> Result<(), CheckpointError> {
        self.publication_durability
            .as_mut()
            .ok_or(CheckpointError::IdentityChanged)?
            .capture_wal()?;
        Ok(())
    }
}

impl CheckpointHandle {
    pub fn prepare(
        self,
        limits: CheckpointLimits,
        cancelled: &AtomicBool,
    ) -> Result<PreparedCheckpoint, CheckpointError> {
        self.prepare_with_progress(limits, cancelled, |_| {})
    }

    /// Reports after pinning the read snapshot and after each completed batch.
    /// Callbacks must return promptly; the next step rechecks cancellation and
    /// deadline. Editing through the independent writer remains possible.
    pub fn prepare_with_progress(
        self,
        limits: CheckpointLimits,
        cancelled: &AtomicBool,
        mut progress: impl FnMut(CheckpointProgress),
    ) -> Result<PreparedCheckpoint, CheckpointError> {
        let limits = limits.validate()?;
        let deadline = Instant::now()
            .checked_add(limits.timeout)
            .ok_or(CheckpointError::InvalidLimits)?;
        self.control(cancelled, deadline)?;
        self.check()?;
        let source = Connection::open_with_flags(
            self.package_path.join(DATABASE_NAME),
            crate::read_flags(),
        )?;
        schema::configure(&source)?;
        source.busy_timeout(Duration::ZERO)?;
        source.pragma_update(None, "query_only", true)?;
        source.execute_batch("BEGIN DEFERRED")?;
        // The first read establishes the transaction snapshot. Later writer
        // commits cannot restart this backup or alter its revision receipt.
        schema::check_version(&source)?;
        let page_size = u64::try_from(
            source.pragma_query_value(None, "page_size", |row| row.get::<_, i64>(0))?,
        )
        .map_err(|_| CheckpointError::IdentityChanged)?;
        let page_count = u64::try_from(
            source.pragma_query_value(None, "page_count", |row| row.get::<_, i64>(0))?,
        )
        .map_err(|_| CheckpointError::IdentityChanged)?;
        let database_bytes = page_size
            .checked_mul(page_count)
            .filter(|bytes| *bytes > 0 && *bytes <= limits.max_database_bytes)
            .ok_or(CheckpointError::TooLarge)?;
        let captured = read_snapshot(&source)?;
        crate::compound::check_stored_sizes(&source)?;
        crate::validation::validate_history(&source)?;
        crate::registers::validate_store(&source)?;
        self.control(cancelled, deadline)?;
        self.check()?;
        let mut staged = StagedCheckpoint::create(&self)?;
        let mut destination = Connection::open_with_flags(
            staged.path(&self),
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        schema::configure(&destination)?;
        destination.busy_timeout(Duration::ZERO)?;
        destination.pragma_update(None, "journal_mode", "DELETE")?;
        staged.check(&self)?;
        progress(CheckpointProgress {
            pages_copied: 0,
            total_pages: page_count,
        });
        {
            let backup = Backup::new(&source, &mut destination)?;
            loop {
                self.control(cancelled, deadline)?;
                let step = backup.step(
                    i32::try_from(limits.pages_per_step)
                        .map_err(|_| CheckpointError::InvalidLimits)?,
                )?;
                let current = backup.progress();
                let total_pages = u64::try_from(current.pagecount)
                    .map_err(|_| CheckpointError::IdentityChanged)?;
                let remaining = u64::try_from(current.remaining)
                    .map_err(|_| CheckpointError::IdentityChanged)?;
                if total_pages > page_count || remaining > total_pages {
                    return Err(CheckpointError::IdentityChanged);
                }
                progress(CheckpointProgress {
                    pages_copied: total_pages - remaining,
                    total_pages,
                });
                self.control(cancelled, deadline)?;
                match step {
                    StepResult::Done => break,
                    StepResult::More => {}
                    StepResult::Busy | StepResult::Locked => {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    _ => return Err(CheckpointError::IdentityChanged),
                }
            }
        }
        // Ensure the result is a standalone main database, including when the
        // source header identifies WAL mode. No sidecar is part of the receipt.
        destination.pragma_update(None, "journal_mode", "DELETE")?;
        let observed = read_snapshot(&destination)?;
        if observed.project_id() != captured.project_id()
            || observed.revision_id() != captured.revision_id()
        {
            return Err(CheckpointError::IdentityChanged);
        }
        destination
            .close()
            .map_err(|(_, error)| CheckpointError::Database(error))?;
        source.execute_batch("ROLLBACK")?;
        drop(source);
        self.control(cancelled, deadline)?;
        staged.check(&self)?;
        if u64::try_from(fstat(staged.file()?).map_err(io)?.st_size).ok() != Some(database_bytes) {
            return Err(CheckpointError::IdentityChanged);
        }
        staged.file()?.sync_all()?;
        staged.sealed = Some(fstat(staged.file()?).map_err(io)?);
        self.control(cancelled, deadline)?;
        self.check()?;
        let receipt = CheckpointReceipt {
            path: self
                .package_path
                .join(SNAPSHOTS_NAME)
                .join(format!("checkpoint-{}.sqlite", staged.id)),
            project_id: captured.project_id().clone(),
            revision_id: captured.revision_id().clone(),
            database_bytes,
        };
        Ok(PreparedCheckpoint {
            handle: self,
            staged,
            receipt,
            deadline,
        })
    }

    fn control(&self, cancelled: &AtomicBool, deadline: Instant) -> Result<(), CheckpointError> {
        if self.owner.is_closed() {
            return Err(CheckpointError::SessionClosed);
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(CheckpointError::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(CheckpointError::Deadline);
        }
        Ok(())
    }

    fn check(&self) -> Result<(), CheckpointError> {
        if self.owner.is_closed() {
            return Err(CheckpointError::SessionClosed);
        }
        let package = fstat(&self.package).map_err(io)?;
        validate_directory(&package)?;
        let expected = self.owner.package_identity();
        if expected != identity(&package)? {
            return Err(CheckpointError::IdentityChanged);
        }
        let named = statat(CWD, &self.package_path, AtFlags::SYMLINK_NOFOLLOW).map_err(io)?;
        same_directory(&package, &named)?;
        let snapshots = fstat(&self.snapshots).map_err(io)?;
        let named = statat(&self.package, SNAPSHOTS_NAME, AtFlags::SYMLINK_NOFOLLOW).map_err(io)?;
        same_directory(&snapshots, &named)?;
        let database = fstat(&self.database).map_err(io)?;
        let named = statat(&self.package, DATABASE_NAME, AtFlags::SYMLINK_NOFOLLOW).map_err(io)?;
        same_file(&database, &named)?;
        Ok(())
    }
}

struct StagedCheckpoint {
    id: uuid::Uuid,
    name: String,
    parent: OwnedFd,
    directory_identity: Option<Stat>,
    directory: Option<OwnedFd>,
    file: Option<File>,
    published: bool,
    sealed: Option<Stat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupStep {
    DirectoryCreated,
    DirectoryOpened,
    FileCreated,
}

impl StagedCheckpoint {
    fn create(handle: &CheckpointHandle) -> Result<Self, CheckpointError> {
        Self::create_with(handle, |_, _| Ok(()))
    }

    fn create_with(
        handle: &CheckpointHandle,
        mut step: impl FnMut(SetupStep, &Self) -> Result<(), CheckpointError>,
    ) -> Result<Self, CheckpointError> {
        // Obtain every descriptor needed for rollback before creating names.
        let parent = rustix::io::fcntl_dupfd_cloexec(&handle.snapshots, 0).map_err(io)?;
        let id = uuid::Uuid::new_v4();
        let name = format!(".checkpoint-{id}");
        let mut staged = Self {
            id,
            name,
            parent,
            directory_identity: None,
            directory: None,
            file: None,
            published: false,
            sealed: None,
        };
        mkdirat(
            &staged.parent,
            staged.name.as_str(),
            Mode::RUSR | Mode::WUSR | Mode::XUSR,
        )
        .map_err(io)?;
        // Metadata lookup consumes no descriptor. If even this lookup fails,
        // ownership cannot be proved and Drop preserves the unknown entry.
        let created = statat(
            &staged.parent,
            staged.name.as_str(),
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(io)?;
        validate_directory(&created)?;
        staged.directory_identity = Some(created);
        step(SetupStep::DirectoryCreated, &staged)?;
        let directory = openat(
            &staged.parent,
            staged.name.as_str(),
            DIRECTORY,
            Mode::empty(),
        )
        .map_err(io)?;
        same_directory(&created, &fstat(&directory).map_err(io)?)?;
        staged.directory = Some(directory);
        step(SetupStep::DirectoryOpened, &staged)?;
        let file = openat(
            staged.directory()?,
            STAGED_NAME,
            OFlags::RDWR
                | OFlags::CREATE
                | OFlags::EXCL
                | OFlags::NOFOLLOW
                | OFlags::NONBLOCK
                | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(io)?;
        staged.file = Some(File::from(file));
        step(SetupStep::FileCreated, &staged)?;
        Ok(staged)
    }

    fn directory(&self) -> Result<&OwnedFd, CheckpointError> {
        self.directory
            .as_ref()
            .ok_or(CheckpointError::IdentityChanged)
    }

    fn file(&self) -> Result<&File, CheckpointError> {
        self.file.as_ref().ok_or(CheckpointError::IdentityChanged)
    }

    fn path(&self, handle: &CheckpointHandle) -> PathBuf {
        handle
            .package_path
            .join(SNAPSHOTS_NAME)
            .join(&self.name)
            .join(STAGED_NAME)
    }

    fn check(&self, handle: &CheckpointHandle) -> Result<(), CheckpointError> {
        handle.check()?;
        let directory = fstat(self.directory()?).map_err(io)?;
        let named = statat(
            &handle.snapshots,
            self.name.as_str(),
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(io)?;
        same_directory(&directory, &named)?;
        let file = fstat(self.file()?).map_err(io)?;
        let named =
            statat(self.directory()?, STAGED_NAME, AtFlags::SYMLINK_NOFOLLOW).map_err(io)?;
        same_file(&file, &named)?;
        if self.sealed.as_ref().is_some_and(|sealed| {
            sealed.st_size != file.st_size
                || sealed.st_mode != file.st_mode
                || sealed.st_mtime != file.st_mtime
                || sealed.st_mtime_nsec != file.st_mtime_nsec
                || sealed.st_ctime != file.st_ctime
                || sealed.st_ctime_nsec != file.st_ctime_nsec
        }) {
            return Err(CheckpointError::IdentityChanged);
        }
        Ok(())
    }
}

impl Drop for StagedCheckpoint {
    fn drop(&mut self) {
        if !self.published
            && let (Some(file), Some(directory)) = (&self.file, &self.directory)
        {
            // Only remove our own entry, never a same-name replacement.
            if let (Ok(held), Ok(named)) = (
                fstat(file),
                statat(directory, STAGED_NAME, AtFlags::SYMLINK_NOFOLLOW),
            ) && same_file(&held, &named).is_ok()
            {
                let _ = unlinkat(directory, STAGED_NAME, AtFlags::empty());
            }
        }
        if let (Some(held), Ok(named)) = (
            &self.directory_identity,
            statat(&self.parent, self.name.as_str(), AtFlags::SYMLINK_NOFOLLOW),
        ) && same_directory(held, &named).is_ok()
        {
            // SQLite cleans its own temporary journals on normal failure. If
            // anything unexpected remains, retain the directory for recovery.
            let _ = unlinkat(&self.parent, self.name.as_str(), AtFlags::REMOVEDIR);
        }
    }
}

fn identity(stat: &Stat) -> Result<PackageIdentity, CheckpointError> {
    Ok(PackageIdentity {
        device: u64::try_from(i128::from(stat.st_dev))
            .map_err(|_| CheckpointError::IdentityChanged)?,
        inode: u64::try_from(i128::from(stat.st_ino))
            .map_err(|_| CheckpointError::IdentityChanged)?,
    })
}

fn validate_directory(stat: &Stat) -> Result<(), CheckpointError> {
    if !FileType::from_raw_mode(stat.st_mode).is_dir()
        || stat.st_uid != rustix::process::geteuid().as_raw()
        || stat.st_mode & 0o022 != 0
    {
        return Err(CheckpointError::IdentityChanged);
    }
    Ok(())
}

fn same_directory(held: &Stat, named: &Stat) -> Result<(), CheckpointError> {
    validate_directory(held)?;
    validate_directory(named)?;
    if identity(held)? != identity(named)? {
        return Err(CheckpointError::IdentityChanged);
    }
    Ok(())
}

fn same_file(held: &Stat, named: &Stat) -> Result<(), CheckpointError> {
    for state in [held, named] {
        if !FileType::from_raw_mode(state.st_mode).is_file()
            || state.st_nlink != 1
            || state.st_uid != rustix::process::geteuid().as_raw()
            || state.st_mode & 0o022 != 0
        {
            return Err(CheckpointError::IdentityChanged);
        }
    }
    if identity(held)? != identity(named)? {
        return Err(CheckpointError::IdentityChanged);
    }
    Ok(())
}

fn io(error: rustix::io::Errno) -> CheckpointError {
    CheckpointError::Io(error.into())
}

#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("checkpoint limits are invalid")]
    InvalidLimits,
    #[error("checkpoint exceeds its database byte limit")]
    TooLarge,
    #[error("checkpoint was cancelled")]
    Cancelled,
    #[error("checkpoint exceeded its deadline")]
    Deadline,
    #[error("checkpoint writer session is closed")]
    SessionClosed,
    #[error("checkpoint package, database, or staging identity changed")]
    IdentityChanged,
    #[error("checkpoint was published at {} but its directory sync failed: {source}", receipt.path.display())]
    PublishedUnconfirmed {
        receipt: Box<CheckpointReceipt>,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl CheckpointError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidLimits => "CheckpointLimitsInvalid",
            Self::TooLarge => "CheckpointTooLarge",
            Self::Cancelled => "CheckpointCancelled",
            Self::Deadline => "CheckpointDeadline",
            Self::SessionClosed => "CheckpointSessionClosed",
            Self::IdentityChanged => "CheckpointIdentityChanged",
            Self::PublishedUnconfirmed { .. } => "CheckpointPublishedUnconfirmed",
            Self::Store(error) => error.code(),
            Self::Database(_) => "CheckpointDatabaseFailure",
            Self::Io(_) => "CheckpointIoFailure",
        }
    }

    /// The final file exists, but publication durability was not confirmed.
    pub fn published_receipt(&self) -> Option<&CheckpointReceipt> {
        match self {
            Self::PublishedUnconfirmed { receipt, .. } => Some(receipt),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{ColorPolicy, FrameRate, NodeId, PresentationBasis, ProjectDocument};

    fn fixture() -> Result<(tempfile::TempDir, ProjectStore), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let document = ProjectDocument::new(
            ProjectId::new("project")?,
            RevisionId::new("captured")?,
            PresentationBasis {
                width: 1920,
                height: 1080,
                frame_rate: FrameRate::new(30, 1)?,
                color_policy: ColorPolicy::SdrRec709,
            },
            NodeId::new("root")?,
        )?;
        let store = ProjectStore::create(&root.path().join("checkpoint.deadpan"), &document)?;
        Ok((root, store))
    }

    #[test]
    fn setup_failures_remove_only_owned_staging_entries() -> Result<(), Box<dyn std::error::Error>>
    {
        let (_root, mut store) = fixture()?;
        let handle = store.checkpoint_handle()?;
        for (fail, error) in [
            (SetupStep::DirectoryCreated, rustix::io::Errno::MFILE),
            (SetupStep::DirectoryOpened, rustix::io::Errno::NOSPC),
            (SetupStep::FileCreated, rustix::io::Errno::IO),
        ] {
            let result = StagedCheckpoint::create_with(&handle, |step, _| {
                if step == fail { Err(io(error)) } else { Ok(()) }
            });
            assert!(matches!(result, Err(CheckpointError::Io(_))));
            assert_eq!(
                std::fs::read_dir(store.package.join(SNAPSHOTS_NAME))?.count(),
                0
            );
        }
        Ok(())
    }

    #[test]
    fn setup_rollback_preserves_replacements_and_unknown_contents()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_root, mut store) = fixture()?;
        let handle = store.checkpoint_handle()?;
        for replace in [false, true] {
            let mut kept_path = None;
            let result = StagedCheckpoint::create_with(&handle, |step, staged| {
                if step != SetupStep::FileCreated {
                    return Ok(());
                }
                let directory = handle.package_path.join(SNAPSHOTS_NAME).join(&staged.name);
                let kept = if replace {
                    let file = directory.join(STAGED_NAME);
                    std::fs::rename(&file, directory.join("original"))?;
                    file
                } else {
                    directory.join("unrelated")
                };
                std::fs::write(&kept, b"preserve")?;
                kept_path = Some(kept);
                Err(io(rustix::io::Errno::IO))
            });
            assert!(matches!(result, Err(CheckpointError::Io(_))));
            assert_eq!(
                std::fs::read(kept_path.ok_or("missing retained path")?)?,
                b"preserve"
            );
        }
        Ok(())
    }

    #[test]
    fn failed_directory_sync_preserves_published_file_and_captured_receipt()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_root, mut store) = fixture()?;
        let document = store.snapshot()?;
        let package = store.package.clone();
        let cancelled = AtomicBool::new(false);
        let prepared = store
            .checkpoint_handle()?
            .prepare(Default::default(), &cancelled)?;
        let error = store
            .publish_checkpoint_with(prepared, &cancelled, |_| {
                // A racing cancellation after rename cannot erase a published result.
                cancelled.store(true, Ordering::Release);
                Err(rustix::io::Errno::IO)
            })
            .expect_err("directory sync was injected to fail");
        assert_eq!(error.code(), "CheckpointPublishedUnconfirmed");
        let receipt = error
            .published_receipt()
            .ok_or("missing published receipt")?;
        assert_eq!(&receipt.revision_id, document.revision_id());
        assert_eq!(
            std::fs::metadata(&receipt.path)?.len(),
            receipt.database_bytes
        );
        assert_eq!(std::fs::read_dir(package.join(SNAPSHOTS_NAME))?.count(), 1);
        let connection = Connection::open_with_flags(&receipt.path, crate::read_flags())?;
        assert_eq!(read_snapshot(&connection)?, document);
        assert_eq!(store.snapshot()?, document);
        Ok(())
    }
}
