//! Rotating, verified database backups in the package's `Backups/` folder.
//!
//! A backup is a standalone SQLite database (DELETE journal mode, no
//! sidecars) copied from one consistent read snapshot through SQLite's backup
//! API. It never borrows the writer connection: the copy runs on whichever
//! thread calls [`create_backup`], with its own read-only connection, so the
//! project's writer keeps committing while it runs. Before a backup is
//! published under its final name it is verified on its own: complete SQLite
//! `integrity_check`, every stored-size bound, the history hash chain (and a
//! replay of anything the chain does not prove) and each operational table,
//! exactly as opening a project verifies. A published backup is therefore a
//! database this build has opened successfully.
//!
//! Media stays in the package. Backups pin every object they mention the way
//! `Snapshots/` checkpoints do (see `storage`), so restoring a backup cannot
//! name an object that cleanup removed while the backup existed.
//!
//! Names are `backup-<UTC>-<reason>-<8 hex>.sqlite`; the reason and time are
//! read back from the name, the project and revision from the database.
//! Unfinished copies are hidden `.staging-*` files that only their creator
//! publishes; rotation removes ones older than [`STALE_STAGING`].
//!
//! Rotation ([`BackupPolicy`]) keeps the newest backups, then the newest per
//! hour, day and week within their windows, plus the newest safety backups
//! taken before risky operations and the newest manual ones, within a count
//! and byte budget. The newest
//! backup always survives. Publication and rotation hold an exclusive `flock`
//! on `Backups/.backups.lock`; a restore holds it shared while it reads.
//!
//! Restoring replaces the live database's content through the backup API in
//! one destination transaction, after taking a `before-restore` backup of
//! the current state, so a restore is itself reversible by restoring that
//! backup. See docs/BACKUPS.md.

use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use deadpan_core::{FrameRate, ProjectId, RevisionId};
use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

use crate::{ProjectStore, StoreError, schema, validation};

/// The package folder holding backups.
pub const DIRECTORY: &str = "Backups";
const LOCK: &str = ".backups.lock";
/// Held shared by every backup while it copies, exclusively by cleanup while
/// it computes references and removes objects.
const ACTIVE: &str = ".backups-active.lock";
const STAGING_PREFIX: &str = ".staging-";
const PREFIX: &str = "backup-";
const SUFFIX: &str = ".sqlite";
/// `YYYYMMDDTHHMMSS.mmmZ`
const STAMP_LENGTH: usize = 20;
/// No backup takes this long (the timeout is at most an hour), so an older
/// staging file belongs to a creator that died.
pub const STALE_STAGING: Duration = Duration::from_secs(6 * 60 * 60);
const MAX_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// Why a backup was taken. Safety reasons precede an operation that replaces
/// or removes something; rotation keeps the newest of those separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackupReason {
    /// The app's timer while edits were being saved.
    Periodic,
    /// The project closed after edits since the last backup.
    Close,
    /// Asked for explicitly (`:backup`, `project backup`).
    Manual,
    BeforeRestore,
    BeforeMigration,
}

impl BackupReason {
    pub const ALL: [Self; 5] = [
        Self::Periodic,
        Self::Close,
        Self::Manual,
        Self::BeforeRestore,
        Self::BeforeMigration,
    ];

    pub fn token(self) -> &'static str {
        match self {
            Self::Periodic => "periodic",
            Self::Close => "close",
            Self::Manual => "manual",
            Self::BeforeRestore => "before-restore",
            Self::BeforeMigration => "before-migration",
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reason| reason.token() == token)
    }

    /// Plain words for people.
    pub fn label(self) -> &'static str {
        match self {
            Self::Periodic => "Automatic",
            Self::Close => "When closed",
            Self::Manual => "Backed up by you",
            Self::BeforeRestore => "Before a restore",
            Self::BeforeMigration => "Before an upgrade",
        }
    }

    /// Taken before an operation that replaces or removes something.
    pub fn safety(self) -> bool {
        matches!(self, Self::BeforeRestore | Self::BeforeMigration)
    }
}

/// Which backups rotation keeps. Every rule selects the newest backup of its
/// bucket; the union is then trimmed to `max_count` and `max_total_bytes`
/// from the oldest, never removing the newest backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupPolicy {
    /// How often the app backs up while edits are being saved.
    pub interval: Duration,
    /// The newest backups kept regardless of age.
    pub keep_recent: usize,
    /// One per hour for this many hours.
    pub hourly_hours: u32,
    /// One per day for this many days.
    pub daily_days: u32,
    /// One per week for this many weeks.
    pub weekly_weeks: u32,
    /// The newest backups taken before risky operations.
    pub keep_safety: usize,
    /// The newest backups someone asked for explicitly.
    pub keep_manual: usize,
    pub max_count: usize,
    pub max_total_bytes: u64,
}

impl Default for BackupPolicy {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(15 * 60),
            keep_recent: 8,
            hourly_hours: 24,
            daily_days: 14,
            weekly_weeks: 8,
            keep_safety: 4,
            keep_manual: 4,
            max_count: 48,
            max_total_bytes: 4 << 30,
        }
    }
}

/// Bounds on one backup's copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupLimits {
    pub max_database_bytes: u64,
    pub pages_per_step: u32,
    /// Covers copying, verification and publication.
    pub timeout: Duration,
}

impl Default for BackupLimits {
    fn default() -> Self {
        Self {
            max_database_bytes: 16 << 30,
            pages_per_step: 256,
            timeout: Duration::from_secs(30 * 60),
        }
    }
}

/// One published backup, as its name and size describe it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupInfo {
    /// The file name without `.sqlite`; what restore names.
    pub id: String,
    pub path: PathBuf,
    pub reason: BackupReason,
    pub created_unix_ms: u64,
    pub database_bytes: u64,
}

/// What one backup contains, read from its database.
#[derive(Debug, Clone, Serialize)]
pub struct BackupPreview {
    pub info: BackupInfo,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub schema: u32,
    /// Revisions in its history, including undo and redo.
    pub revisions: u64,
    /// Edits in its history.
    pub edits: u64,
    /// Beats (nodes) of its current document, the root excluded.
    pub beats: usize,
    pub duration_frames: i64,
    pub frame_rate: FrameRate,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupOutcome {
    pub backup: BackupInfo,
    /// The captured head revision; `None` for a raw copy of a database this
    /// build cannot validate (taken before migrating it).
    pub revision_id: Option<RevisionId>,
    /// Backups rotation removed, by id.
    pub removed: Vec<String>,
    /// Abandoned staging files removed.
    pub removed_staging: usize,
    /// Problems after publication (folder sync, rotation); the backup itself
    /// is published and verified.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreOutcome {
    /// What is now the project.
    pub restored: BackupPreview,
    /// The state before restoring, itself restorable.
    pub safety: BackupOutcome,
    /// The head revision that was replaced.
    pub replaced_revision: RevisionId,
}

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("The backup was cancelled")]
    Cancelled,
    #[error("The backup took longer than its time limit")]
    Deadline,
    #[error("The project database is larger than the backup limit")]
    TooLarge,
    #[error("Backup limits are invalid")]
    InvalidLimits,
    #[error("There is no backup named {0}")]
    NotFound(String),
    #[error("That backup belongs to a different project")]
    OtherProject,
    #[error("The backup failed verification and was not used: {0}")]
    Verification(String),
    #[error("A render is using the project; restore when it finishes")]
    Busy,
    #[error(
        "Not enough free space: this needs {required} bytes including a reserve and {available} are free. Your saved edits are intact"
    )]
    Space { required: u64, available: u64 },
    #[error(
        "The project was replaced by the backup, but checking it afterwards failed: {source}. Reopen the project; the state before restoring is in backup {safety}"
    )]
    RestoredButUnverified {
        safety: String,
        #[source]
        source: Box<StoreError>,
    },
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl BackupError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Cancelled => "BackupCancelled",
            Self::Deadline => "BackupDeadline",
            Self::TooLarge => "BackupTooLarge",
            Self::InvalidLimits => "BackupLimitsInvalid",
            Self::NotFound(_) => "BackupNotFound",
            Self::OtherProject => "BackupOtherProject",
            Self::Verification(_) => "BackupInvalid",
            Self::Busy => "ProjectBusy",
            Self::Space { .. } => "DiskFull",
            Self::RestoredButUnverified { .. } => "BackupRestoredUnverified",
            Self::Store(error) => error.code(),
            Self::Database(rusqlite::Error::SqliteFailure(error, _)) => match error.code {
                rusqlite::ErrorCode::DiskFull => "DiskFull",
                rusqlite::ErrorCode::ReadOnly => "ProjectReadOnly",
                rusqlite::ErrorCode::PermissionDenied => "PermissionDenied",
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => {
                    "ProjectBusy"
                }
                _ => "BackupDatabaseFailure",
            },
            Self::Database(_) => "BackupDatabaseFailure",
            Self::Io(error) => match error.kind() {
                std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded => "DiskFull",
                std::io::ErrorKind::ReadOnlyFilesystem => "ProjectReadOnly",
                std::io::ErrorKind::PermissionDenied => "PermissionDenied",
                _ => "IoFailure",
            },
        }
    }
}

/// Keep backups from copying while storage cleanup runs; waits at most
/// `wait` for copies in progress to finish.
pub(crate) fn exclude_backups(package: &Path, wait: Duration) -> Result<Option<File>, StoreError> {
    let folder = directory(package);
    if !folder.is_dir() {
        return Ok(None);
    }
    let file = lock_file_named(&folder, ACTIVE).map_err(|error| match error {
        BackupError::Store(error) => error,
        BackupError::Io(error) => StoreError::Io(error),
        other => StoreError::Storage(other.to_string()),
    })?;
    let deadline = Instant::now() + wait;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Some(file)),
            Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(StoreError::Storage(
                    "a backup is being made; clean up again when it finishes".into(),
                ));
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
        }
    }
}

/// Free space kept beyond a copy's own size, so a backup never fills the
/// disk the project's next commit needs.
const SPACE_RESERVE: u64 = 256 << 20;

/// Refuse before copying when the volume cannot hold `bytes` plus the
/// reserve.
fn require_space(package: &Path, bytes: u64) -> Result<(), BackupError> {
    let volume = rustix::fs::statvfs(package).map_err(std::io::Error::from)?;
    let available = volume.f_bavail.saturating_mul(volume.f_frsize);
    let required = bytes.saturating_add(SPACE_RESERVE);
    if available < required {
        return Err(BackupError::Space {
            required,
            available,
        });
    }
    Ok(())
}

/// The backups folder of a package.
pub fn directory(package: &Path) -> PathBuf {
    package.join(DIRECTORY)
}

/// Every published backup, newest first. Read-only; anything that is not a
/// regular file with a backup name is ignored.
pub fn list_backups(package: &Path) -> Result<Vec<BackupInfo>, BackupError> {
    // SQLite refuses symbolic links anywhere in a path it opens.
    let package = package.canonicalize()?;
    let directory = directory(&package);
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut backups = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some((reason, created_unix_ms)) = parse_name(name) else {
            continue;
        };
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.is_file() {
            continue;
        }
        backups.push(BackupInfo {
            id: name.trim_end_matches(SUFFIX).to_owned(),
            path: entry.path(),
            reason,
            created_unix_ms,
            database_bytes: metadata.len(),
        });
    }
    backups.sort_by(|a, b| {
        b.created_unix_ms
            .cmp(&a.created_unix_ms)
            .then_with(|| b.id.cmp(&a.id))
    });
    Ok(backups)
}

/// Read what a backup contains without verifying its history.
pub fn preview_backup(info: &BackupInfo) -> Result<BackupPreview, BackupError> {
    let connection = open_backup(&info.path)?;
    preview_connection(&connection, info)
}

/// Verify a backup completely, as creation and restore do, and describe it.
pub fn verify_backup(info: &BackupInfo) -> Result<BackupPreview, BackupError> {
    let connection = open_backup(&info.path)?;
    verify_connection(&connection)?;
    preview_connection(&connection, info)
}

/// Copy, verify and publish one backup of the package's committed state,
/// then rotate. Runs entirely on the calling thread with its own read-only
/// connection; a concurrent writer keeps committing (its later commits are
/// simply not in this backup).
pub fn create_backup(
    package: &Path,
    reason: BackupReason,
    policy: &BackupPolicy,
    limits: BackupLimits,
    cancelled: &AtomicBool,
) -> Result<BackupOutcome, BackupError> {
    create_backup_keeping(
        package,
        reason,
        policy,
        limits,
        cancelled,
        None,
        Verify::Current,
    )
}

/// How a copy is verified before it is published.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Verify {
    /// Everything opening a current project checks.
    Current,
    /// A database of another schema (before migrating it): SQLite's
    /// complete integrity check and the Deadpan application identity only.
    Raw,
}

/// Back up a database whose schema this build cannot validate, before a
/// migration rewrites it. Restoring such a backup needs the build that
/// wrote it; see docs/BACKUPS.md.
pub(crate) fn create_raw_backup(
    package: &Path,
    reason: BackupReason,
    cancelled: &AtomicBool,
) -> Result<BackupOutcome, BackupError> {
    create_backup_keeping(
        package,
        reason,
        &BackupPolicy::default(),
        BackupLimits::default(),
        cancelled,
        None,
        Verify::Raw,
    )
}

fn create_backup_keeping(
    package: &Path,
    reason: BackupReason,
    policy: &BackupPolicy,
    limits: BackupLimits,
    cancelled: &AtomicBool,
    keep: Option<&str>,
    mode: Verify,
) -> Result<BackupOutcome, BackupError> {
    if limits.pages_per_step == 0
        || limits.timeout.is_zero()
        || limits.timeout > MAX_TIMEOUT
        || limits.max_database_bytes == 0
    {
        return Err(BackupError::InvalidLimits);
    }
    let deadline = Instant::now() + limits.timeout;
    let control = || -> Result<(), BackupError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(BackupError::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(BackupError::Deadline);
        }
        Ok(())
    };
    control()?;
    let package = &package.canonicalize()?;
    // Check the format before creating anything in the package.
    {
        let probe =
            Connection::open_with_flags(package.join("project.sqlite"), crate::read_flags())?;
        schema::configure(&probe)?;
        match mode {
            Verify::Current => schema::check_version(&probe)?,
            Verify::Raw => {
                schema::read_version(&probe)?;
            }
        }
    }
    let folder = ensure_directory(package)?;
    // Cleanup must not compute references while this snapshot is copied.
    let active = lock_file_named(&folder, ACTIVE)?;
    active.lock_shared()?;
    let source = Connection::open_with_flags(package.join("project.sqlite"), crate::read_flags())?;
    schema::configure(&source)?;
    source.busy_timeout(Duration::from_millis(250))?;
    source.pragma_update(None, "query_only", true)?;
    source.execute_batch("BEGIN DEFERRED")?;
    // The first read pins the snapshot; later commits do not enter it.
    let head = match mode {
        Verify::Current => {
            schema::check_version(&source)?;
            Some(crate::read_head_project(&source)?)
        }
        Verify::Raw => {
            schema::read_version(&source)?;
            None
        }
    };
    let page_size: i64 = source.pragma_query_value(None, "page_size", |row| row.get(0))?;
    let page_count: i64 = source.pragma_query_value(None, "page_count", |row| row.get(0))?;
    let bytes = u64::try_from(page_size)
        .ok()
        .zip(u64::try_from(page_count).ok())
        .and_then(|(size, count)| size.checked_mul(count))
        .filter(|bytes| *bytes > 0 && *bytes <= limits.max_database_bytes)
        .ok_or(BackupError::TooLarge)?;
    require_space(package, bytes)?;
    let staged = Staged::create(&folder)?;
    {
        let mut destination = Connection::open_with_flags(
            &staged.path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        schema::configure(&destination)?;
        destination.busy_timeout(Duration::ZERO)?;
        destination.pragma_update(None, "journal_mode", "DELETE")?;
        {
            let backup = Backup::new(&source, &mut destination)?;
            let step =
                i32::try_from(limits.pages_per_step).map_err(|_| BackupError::InvalidLimits)?;
            loop {
                control()?;
                match backup.step(step)? {
                    StepResult::Done => break,
                    StepResult::More => {}
                    StepResult::Busy | StepResult::Locked => {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    _ => {
                        return Err(BackupError::Verification(
                            "SQLite reported an unknown backup step result".into(),
                        ));
                    }
                }
            }
        }
        // The source header names WAL; a backup stands alone.
        destination.pragma_update(None, "journal_mode", "DELETE")?;
        destination
            .close()
            .map_err(|(_, error)| BackupError::Database(error))?;
    }
    source.execute_batch("ROLLBACK")?;
    drop(source);
    control()?;
    let copy = open_backup(&staged.path)?;
    let revision_id = match &head {
        Some(head) => {
            let verified = verify_connection(&copy)?;
            if verified.revision != *head.revision_id() || verified.project != *head.project_id() {
                return Err(BackupError::Verification(
                    "the copy does not hold the captured revision".into(),
                ));
            }
            Some(verified.revision)
        }
        None => {
            schema::read_version(&copy)?;
            let integrity: String =
                copy.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
            if integrity != "ok" {
                return Err(BackupError::Verification(integrity));
            }
            None
        }
    };
    drop(copy);
    let file = File::open(&staged.path)?;
    if file.metadata()?.len() != bytes {
        // The backup API copies every page; a different size means the
        // staged file was changed by someone else.
        return Err(BackupError::Verification(
            "the copy's size differs from the captured database".into(),
        ));
    }
    file.sync_all()?;
    drop(file);
    control()?;
    let lock = lock_file(&folder)?;
    lock.lock()?;
    let created_unix_ms = now_ms();
    let name = format!(
        "{PREFIX}{}-{}-{}{SUFFIX}",
        format_stamp(created_unix_ms),
        reason.token(),
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let path = folder.join(&name);
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        &staged.path,
        rustix::fs::CWD,
        &path,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(std::io::Error::from)?;
    let mut staged = staged;
    staged.published = true;
    crate::failpoint("backup-after-rename");
    // Published from here on: later failures are warnings, never a failed
    // backup, so callers never mistake a published copy for none.
    let mut warnings = Vec::new();
    if let Err(error) = File::open(&folder).and_then(|folder| folder.sync_all()) {
        warnings.push(format!(
            "the backups folder could not be synchronized: {error}"
        ));
    }
    let backup = BackupInfo {
        id: name.trim_end_matches(SUFFIX).to_owned(),
        path,
        reason,
        created_unix_ms,
        database_bytes: bytes,
    };
    // The backup just published always survives its own rotation, even if
    // the clock moved backwards and its name sorts below older ones.
    let keep: Vec<&str> = keep.into_iter().chain([backup.id.as_str()]).collect();
    let (removed, removed_staging) = match rotate_locked(package, policy, &keep, SystemTime::now())
    {
        Ok(rotated) => rotated,
        Err(error) => {
            warnings.push(format!("rotation failed: {error}"));
            (Vec::new(), 0)
        }
    };
    drop(lock);
    Ok(BackupOutcome {
        backup,
        revision_id,
        removed,
        removed_staging,
        warnings,
    })
}

/// Apply the rotation policy now, outside a backup.
pub fn rotate_backups(
    package: &Path,
    policy: &BackupPolicy,
) -> Result<(Vec<String>, usize), BackupError> {
    let folder = directory(package);
    if !folder.exists() {
        return Ok((Vec::new(), 0));
    }
    let lock = lock_file(&folder)?;
    lock.lock()?;
    rotate_locked(package, policy, &[], SystemTime::now())
}

fn rotate_locked(
    package: &Path,
    policy: &BackupPolicy,
    keep: &[&str],
    now: SystemTime,
) -> Result<(Vec<String>, usize), BackupError> {
    let folder = directory(package);
    let backups = list_backups(package)?;
    let retained = retained(&backups, policy, now_ms_at(now));
    let mut removed = Vec::new();
    for (index, backup) in backups.iter().enumerate() {
        if retained.contains(&index) || keep.contains(&backup.id.as_str()) {
            continue;
        }
        match fs::remove_file(&backup.path) {
            Ok(()) => removed.push(backup.id.clone()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let mut removed_staging = 0;
    for entry in fs::read_dir(&folder)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(STAGING_PREFIX) {
            continue;
        }
        // A creator may remove its own staging file meanwhile.
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        let age = now.duration_since(metadata.modified()?).unwrap_or_default();
        if metadata.is_file() && age >= STALE_STAGING {
            match fs::remove_file(entry.path()) {
                Ok(()) => removed_staging += 1,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    if !removed.is_empty() || removed_staging > 0 {
        File::open(&folder)?.sync_all()?;
    }
    Ok((removed, removed_staging))
}

/// Indexes into `backups` (newest first) that the policy keeps.
pub fn retained(
    backups: &[BackupInfo],
    policy: &BackupPolicy,
    now_ms: u64,
) -> std::collections::BTreeSet<usize> {
    use std::collections::{BTreeSet, HashSet};
    let mut keep = BTreeSet::new();
    if backups.is_empty() {
        return keep;
    }
    keep.insert(0);
    keep.extend(0..policy.keep_recent.min(backups.len()));
    for (period, count) in [
        (3_600_000u64, u64::from(policy.hourly_hours)),
        (86_400_000, u64::from(policy.daily_days)),
        (7 * 86_400_000, u64::from(policy.weekly_weeks)),
    ] {
        let window = period.saturating_mul(count);
        let mut seen = HashSet::new();
        for (index, backup) in backups.iter().enumerate() {
            if now_ms.saturating_sub(backup.created_unix_ms) >= window {
                continue;
            }
            if seen.insert(backup.created_unix_ms / period) {
                keep.insert(index);
            }
        }
    }
    keep.extend(
        backups
            .iter()
            .enumerate()
            .filter(|(_, backup)| backup.reason.safety())
            .map(|(index, _)| index)
            .take(policy.keep_safety),
    );
    keep.extend(
        backups
            .iter()
            .enumerate()
            .filter(|(_, backup)| backup.reason == BackupReason::Manual)
            .map(|(index, _)| index)
            .take(policy.keep_manual),
    );
    // Budgets: keep the newest that fit.
    let mut bytes = 0u64;
    let mut count = 0usize;
    let mut budgeted = BTreeSet::new();
    for index in keep {
        let size = backups[index].database_bytes;
        if index == 0
            || (count < policy.max_count && bytes.saturating_add(size) <= policy.max_total_bytes)
        {
            bytes = bytes.saturating_add(size);
            count += 1;
            budgeted.insert(index);
        }
    }
    budgeted
}

/// What [`replace_damaged_database`] did.
#[derive(Debug, Clone, Serialize)]
pub struct DamagedReplacement {
    pub restored: BackupPreview,
    /// Where the damaged database and its WAL files were kept, untouched.
    pub quarantine: PathBuf,
}

/// Replace a database that no longer opens with a verified backup, without
/// reading the damaged one. Needs the package's writer lock (refused while
/// anyone has the project open) and holds the backups lock while it reads.
///
/// The backup's project must equal the one `manifest.json` names; when the
/// manifest cannot be read, `force_project` must confirm the backup's
/// project explicitly. The backup is copied to a hidden file in the package,
/// and that copy is verified again. Then `project.sqlite` and its WAL files
/// move together into `.damaged-project-<uuid>/` (main file first), and the
/// copy is renamed into place. Nothing is deleted.
///
/// Crash windows: before the first move nothing changed; after it there is
/// no `project.sqlite`, so opening fails loudly instead of reading a main
/// file without its WAL; a rerun moves any WAL left beside it into the new
/// quarantine before installing, so no stale WAL ever meets the new file.
pub fn replace_damaged_database(
    package: &Path,
    id: &str,
    force_project: Option<&ProjectId>,
) -> Result<DamagedReplacement, BackupError> {
    let package = package.canonicalize()?;
    let _writer = crate::acquire_lock(&package)?;
    let folder = directory(&package);
    let lock = lock_file(&folder)?;
    lock.lock_shared()?;
    let info = list_backups(&package)?
        .into_iter()
        .find(|backup| backup.id == id)
        .ok_or_else(|| BackupError::NotFound(id.to_owned()))?;
    let backup = open_backup(&info.path)?;
    let verified = verify_connection(&backup)?;
    drop(backup);
    #[derive(serde::Deserialize)]
    struct Manifest {
        project_id: String,
    }
    let manifest = fs::read(package.join("manifest.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Manifest>(&bytes).ok());
    match (manifest, force_project) {
        (Some(manifest), _) if manifest.project_id != verified.project.as_str() => {
            return Err(BackupError::OtherProject);
        }
        (Some(_), _) => {}
        (None, Some(project)) if *project == verified.project => {}
        (None, _) => {
            return Err(BackupError::Verification(format!(
                "the project's manifest cannot be read, so the backup's project {} cannot be confirmed; name it explicitly to proceed",
                verified.project.as_str()
            )));
        }
    }
    let staged = Staged::create_in(&package, ".restoring-")?;
    fs::copy(&info.path, &staged.path)?;
    File::open(&staged.path)?.sync_all()?;
    let copy = open_backup(&staged.path)?;
    let installed = verify_connection(&copy)?;
    let restored = preview_connection(&copy, &info)?;
    drop(copy);
    if installed.revision != verified.revision || installed.project != verified.project {
        return Err(BackupError::Verification(
            "the copy differs from the backup".into(),
        ));
    }
    drop(lock);
    let quarantine = package.join(format!(
        ".damaged-project-{}",
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir(&quarantine)?;
    for name in ["project.sqlite", "project.sqlite-wal", "project.sqlite-shm"] {
        let path = package.join(name);
        if fs::symlink_metadata(&path).is_ok() {
            fs::rename(&path, quarantine.join(name))?;
            if name == "project.sqlite" {
                crate::failpoint("damaged-after-main-moved");
            }
        }
    }
    File::open(&package)?.sync_all()?;
    crate::failpoint("damaged-after-quarantine");
    let mut staged = staged;
    fs::rename(&staged.path, package.join("project.sqlite"))?;
    staged.published = true;
    File::open(&package)?.sync_all()?;
    Ok(DamagedReplacement {
        restored,
        quarantine,
    })
}

/// Notices every commit to a package's database, authored or operational
/// (transcripts, corrections, registers, render and AI state alike), through
/// its own idle read-only connection: SQLite's `data_version` changes
/// whenever another connection commits.
pub struct ChangeMonitor {
    connection: Connection,
}

impl ChangeMonitor {
    /// An opaque value that differs after any other connection committed.
    pub fn version(&self) -> Result<i64, StoreError> {
        Ok(self
            .connection
            .pragma_query_value(None, "data_version", |row| row.get(0))?)
    }
}

impl ProjectStore {
    /// A monitor of this package's commits, for deciding when a backup is
    /// due. Writers and read-only stores alike.
    pub fn change_monitor(&self) -> Result<ChangeMonitor, StoreError> {
        let connection =
            Connection::open_with_flags(self.package.join("project.sqlite"), crate::read_flags())?;
        schema::configure(&connection)?;
        connection.pragma_update(None, "query_only", true)?;
        Ok(ChangeMonitor { connection })
    }

    /// Replace this project's database with a backup's. First takes a
    /// `before-restore` backup of the current state, then verifies the
    /// chosen backup completely and copies it into the live database in one
    /// transaction, and finally validates and recovers as opening does.
    /// Media is not touched. Refuses while a render owns the project.
    ///
    /// This runs on the writer's thread and blocks other project commands for
    /// its duration (two database copies and a verification).
    pub fn restore_backup(
        &mut self,
        id: &str,
        policy: &BackupPolicy,
        limits: BackupLimits,
        cancelled: &AtomicBool,
    ) -> Result<RestoreOutcome, BackupError> {
        self.require_writer()?;
        if self.render_workflow_claimed.load(Ordering::Acquire) {
            return Err(BackupError::Busy);
        }
        let info = list_backups(&self.package)?
            .into_iter()
            .find(|backup| backup.id == id)
            .ok_or_else(|| BackupError::NotFound(id.to_owned()))?;
        // Pin the chosen backup: an open connection keeps reading it even if
        // something removes its name, and rotation below is told to keep it.
        let backup = open_backup(&info.path).map_err(|error| match error {
            BackupError::Io(io) if io.kind() == std::io::ErrorKind::NotFound => {
                BackupError::NotFound(id.to_owned())
            }
            other => other,
        })?;
        let verified = verify_connection(&backup)?;
        let current = crate::read_head_project(&self.connection)?;
        if verified.project != *current.project_id() {
            return Err(BackupError::OtherProject);
        }
        let preview = preview_connection(&backup, &info)?;
        if cancelled.load(Ordering::Acquire) {
            return Err(BackupError::Cancelled);
        }
        // The safety backup, the private copy and the WAL each need about
        // one database of space.
        require_space(&self.package, info.database_bytes.saturating_mul(3))?;
        let safety = create_backup_keeping(
            &self.package,
            BackupReason::BeforeRestore,
            policy,
            limits,
            cancelled,
            Some(id),
            Verify::Current,
        )?;
        if cancelled.load(Ordering::Acquire) {
            return Err(BackupError::Cancelled);
        }
        // Carry forward what the replaced database issued, into a private
        // copy of the backup, so the restore cannot free identities again.
        let staged = Staged::create_in(&self.package, ".restoring-")?;
        {
            let mut copy = Connection::open_with_flags(
                &staged.path,
                OpenFlags::SQLITE_OPEN_READ_WRITE
                    | OpenFlags::SQLITE_OPEN_NO_MUTEX
                    | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            )?;
            schema::configure(&copy)?;
            copy.pragma_update(None, "journal_mode", "DELETE")?;
            Backup::new(&backup, &mut copy)?.run_to_completion(
                256,
                Duration::from_millis(2),
                None,
            )?;
            copy.pragma_update(None, "journal_mode", "DELETE")?;
            crate::retired::carry_forward(&self.connection, &mut copy)?;
            copy.close()
                .map_err(|(_, error)| BackupError::Database(error))?;
        }
        drop(backup);
        let source = open_backup(&staged.path)?;
        let folder = directory(&self.package);
        let lock = lock_file(&folder)?;
        lock.lock_shared()?;
        // Capabilities of the replaced state must not act on the restored
        // one: revoke every handle of this writer session first.
        self.revoke_session_capabilities()?;
        {
            // The destination write transaction spans every step, so the
            // live database is either entirely replaced or unchanged, also
            // when the process dies part way.
            let copy = Backup::new(&source, &mut self.connection)?;
            // One page first, so a crash window exists inside the copy even
            // for a small database; then bounded batches.
            let mut pages = 1;
            loop {
                match copy.step(pages)? {
                    StepResult::Done => break,
                    StepResult::More => {
                        if pages == 1 {
                            crate::failpoint("restore-mid-copy");
                        }
                        pages = 256;
                    }
                    StepResult::Busy | StepResult::Locked => {
                        return Err(BackupError::Store(StoreError::Storage(
                            "the project database was busy; nothing was restored".into(),
                        )));
                    }
                    _ => {
                        return Err(BackupError::Store(StoreError::Storage(
                            "SQLite reported an unknown backup step; nothing was restored".into(),
                        )));
                    }
                }
            }
        }
        crate::failpoint("restore-after-copy");
        drop(lock);
        drop(source);
        drop(staged);
        let mode: String = self
            .connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            self.connection.pragma_update(None, "journal_mode", "WAL")?;
        }
        self.documents = crate::document_cache::DocumentCache::default();
        let checked = (|| -> Result<(), StoreError> {
            let audit = self.validate_with(validation::HistoryMode::Receipt)?;
            self.opened = crate::HistoryValidation::from(&audit);
            self.recover_writable(&audit, true)
        })();
        if checked.is_ok() {
            // A fresh session marker for the restored state.
            self.begin_writer_session();
        }
        if let Err(source) = checked {
            // The database is already the backup's. Refuse further writes
            // through this store; the caller must reopen.
            self.mode = crate::AccessMode::ReadOnly;
            return Err(BackupError::RestoredButUnverified {
                safety: safety.backup.id.clone(),
                source: Box::new(source),
            });
        }
        Ok(RestoreOutcome {
            restored: preview,
            safety,
            replaced_revision: current.revision_id().clone(),
        })
    }

    /// Revoke every capability this writer session handed out (import,
    /// generated and render handles, checkpoint and endpoint ownership) and
    /// start fresh ones, as closing and reopening would.
    fn revoke_session_capabilities(&mut self) -> Result<(), StoreError> {
        for flag in [
            &mut self.import_closed,
            &mut self.generated_read_closed,
            &mut self.render_closed,
        ] {
            flag.store(true, Ordering::Release);
            *flag = std::sync::Arc::new(AtomicBool::new(false));
        }
        self.render_workflow_claimed = std::sync::Arc::new(AtomicBool::new(false));
        self.publication_epochs.clear();
        self.publication_barrier_failed = false;
        if let Some(mut owner) = self.writer_owner.take() {
            owner.close(
                self.writer_package.as_ref(),
                &self.package,
                self._writer_lock.as_ref(),
            );
        }
        self.publication_durability = Some(
            crate::publication_durability::PublicationDurability::open(&self.package)?,
        );
        Ok(())
    }

    /// The package's published backups, newest first.
    pub fn backups(&self) -> Result<Vec<BackupInfo>, BackupError> {
        list_backups(&self.package)
    }

    pub fn package_path(&self) -> &Path {
        &self.package
    }
}

struct Verified {
    project: ProjectId,
    revision: RevisionId,
}

fn open_backup(path: &Path) -> Result<Connection, BackupError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err(StoreError::UnsafePath(path.to_owned()).into());
    }
    let connection = Connection::open_with_flags(path, crate::read_flags())?;
    schema::configure(&connection)?;
    connection.pragma_update(None, "query_only", true)?;
    Ok(connection)
}

/// Everything opening a project checks, plus SQLite's complete integrity
/// check, on a standalone database.
fn verify_connection(connection: &Connection) -> Result<Verified, BackupError> {
    let failed = |error: StoreError| BackupError::Verification(error.to_string());
    schema::check_version(connection).map_err(|error| match error {
        StoreError::MigrationRequired(found) | StoreError::UnsupportedSchema(found) => {
            BackupError::Verification(format!(
                "it holds database schema {found}, from before an upgrade; this build cannot verify or restore it (the build that wrote it can)"
            ))
        }
        other => failed(other),
    })?;
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(BackupError::Verification(integrity));
    }
    crate::validate_database(connection, validation::HistoryMode::Receipt).map_err(failed)?;
    let head = crate::read_head_project(connection).map_err(failed)?;
    Ok(Verified {
        project: head.project_id().clone(),
        revision: head.revision_id().clone(),
    })
}

fn preview_connection(
    connection: &Connection,
    info: &BackupInfo,
) -> Result<BackupPreview, BackupError> {
    let schema = schema::read_version(connection)?;
    let head = validation::read_head(connection)?;
    let document = crate::revision_storage::reconstruct(connection, &head)?;
    let revisions: i64 =
        connection.query_row("SELECT COUNT(*) FROM revisions", [], |row| row.get(0))?;
    let edits: i64 = connection.query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))?;
    Ok(BackupPreview {
        info: info.clone(),
        project_id: document.project_id().clone(),
        revision_id: document.revision_id().clone(),
        schema,
        revisions: u64::try_from(revisions).unwrap_or(0),
        edits: u64::try_from(edits).unwrap_or(0),
        beats: document.nodes().len().saturating_sub(1),
        duration_frames: document
            .structural_duration()
            .map_err(StoreError::from)?
            .frames(),
        frame_rate: document.presentation_basis().frame_rate,
    })
}

fn ensure_directory(package: &Path) -> Result<PathBuf, BackupError> {
    let folder = directory(package);
    match fs::create_dir(&folder) {
        Ok(()) => File::open(package)?.sync_all()?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    if !fs::symlink_metadata(&folder)?.is_dir() {
        return Err(StoreError::UnsafePath(folder).into());
    }
    Ok(folder)
}

fn lock_file(folder: &Path) -> Result<File, BackupError> {
    lock_file_named(folder, LOCK)
}

fn lock_file_named(folder: &Path, name: &str) -> Result<File, BackupError> {
    let path = folder.join(name);
    if let Ok(metadata) = fs::symlink_metadata(&path)
        && !metadata.is_file()
    {
        return Err(StoreError::UnsafePath(path).into());
    }
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(path)?)
}

/// A hidden staging copy, removed unless published.
struct Staged {
    path: PathBuf,
    published: bool,
}

impl Staged {
    fn create(folder: &Path) -> Result<Self, BackupError> {
        Self::create_in(folder, STAGING_PREFIX)
    }

    fn create_in(folder: &Path, prefix: &str) -> Result<Self, BackupError> {
        let path = folder.join(format!("{prefix}{}{SUFFIX}", uuid::Uuid::new_v4().simple()));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(&path)?;
        Ok(Self {
            path,
            published: false,
        })
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_file(&self.path);
            for sidecar in ["sqlite-journal", "sqlite-wal", "sqlite-shm"] {
                let _ = fs::remove_file(self.path.with_extension(sidecar));
            }
        }
    }
}

fn now_ms() -> u64 {
    now_ms_at(SystemTime::now())
}

fn now_ms_at(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// `(reason, created)` from a published backup name.
fn parse_name(name: &str) -> Option<(BackupReason, u64)> {
    let rest = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    let (rest, suffix) = rest.rsplit_once('-')?;
    if suffix.len() != 8 || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let stamp = rest.get(..STAMP_LENGTH)?;
    let reason = rest.get(STAMP_LENGTH..)?.strip_prefix('-')?;
    Some((BackupReason::parse(reason)?, parse_stamp(stamp)?))
}

/// `YYYYMMDDTHHMMSS.mmmZ` in UTC.
pub fn format_stamp(unix_ms: u64) -> String {
    let seconds = unix_ms / 1000;
    let (year, month, day) = civil_from_days((seconds / 86_400) as i64);
    let second_of_day = seconds % 86_400;
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}.{:03}Z",
        second_of_day / 3600,
        second_of_day / 60 % 60,
        second_of_day % 60,
        unix_ms % 1000
    )
}

fn parse_stamp(stamp: &str) -> Option<u64> {
    let bytes = stamp.as_bytes();
    if bytes.len() != STAMP_LENGTH || bytes[8] != b'T' || bytes[15] != b'.' || bytes[19] != b'Z' {
        return None;
    }
    let number = |range: std::ops::Range<usize>| -> Option<u64> {
        let text = stamp.get(range)?;
        text.bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| text.parse().ok())?
    };
    let (year, month, day) = (number(0..4)?, number(4..6)?, number(6..8)?);
    let (hour, minute, second) = (number(9..11)?, number(11..13)?, number(13..15)?);
    let millis = number(16..19)?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let days = days_from_civil(year as i64, month as u32, day as u32);
    let days = u64::try_from(days).ok()?;
    Some(((days * 86_400 + hour * 3600 + minute * 60 + second) * 1000) + millis)
}

// Howard Hinnant's civil calendar algorithms (proleptic Gregorian, UTC).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(age_ms: u64, now: u64, reason: BackupReason, bytes: u64) -> BackupInfo {
        BackupInfo {
            id: format!("b{age_ms}"),
            path: PathBuf::new(),
            reason,
            created_unix_ms: now - age_ms,
            database_bytes: bytes,
        }
    }

    #[test]
    fn stamps_round_trip_and_names_parse() {
        for ms in [
            0,
            1_000,
            951_782_400_123,
            1_791_234_567_890,
            4_102_444_799_999,
        ] {
            assert_eq!(
                parse_stamp(&format_stamp(ms)),
                Some(ms),
                "{}",
                format_stamp(ms)
            );
        }
        assert_eq!(format_stamp(1_791_234_567_890), "20261005T210927.890Z");
        let name = format!(
            "backup-{}-before-restore-0a1b2c3d.sqlite",
            format_stamp(5_000)
        );
        assert_eq!(
            parse_name(&name),
            Some((BackupReason::BeforeRestore, 5_000))
        );
        for bad in [
            "backup-20261005T213927.890Z-periodic-0a1b2c3.sqlite",
            "backup-20261005T213927.890Z-sometimes-0a1b2c3d.sqlite",
            "backup-20261305T213927.890Z-periodic-0a1b2c3d.sqlite",
            ".staging-x.sqlite",
            "backup-20261005T213927.890Z-periodic-0a1b2c3d.sqlite-journal",
        ] {
            assert_eq!(parse_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn rotation_keeps_recent_hourly_daily_weekly_and_safety_backups() {
        let now = 100 * 86_400_000;
        let hour = 3_600_000;
        let policy = BackupPolicy {
            keep_recent: 2,
            hourly_hours: 3,
            daily_days: 2,
            weekly_weeks: 0,
            keep_safety: 1,
            ..BackupPolicy::default()
        };
        let backups = vec![
            info(1_000, now, BackupReason::Periodic, 10), // 0 recent
            info(2_000, now, BackupReason::Periodic, 10), // 1 recent
            info(3_000, now, BackupReason::Periodic, 10), // 2 same hour: dropped
            info(hour + 10, now, BackupReason::Periodic, 10), // 3 hour bucket -1
            info(hour + 20, now, BackupReason::Periodic, 10), // 4 dropped
            info(30 * hour, now, BackupReason::BeforeRestore, 10), // 5 daily -1 and safety
            info(31 * hour, now, BackupReason::BeforeMigration, 10), // 6 dropped (same day, older safety)
            info(40 * 86_400_000, now, BackupReason::Close, 10),     // 7 too old
        ];
        let kept: Vec<_> = retained(&backups, &policy, now).into_iter().collect();
        // Hourly buckets are UTC hours, so index 2 shares index 0's hour
        // unless the clock straddles one; the fixture's `now` is midnight.
        assert_eq!(kept, vec![0, 1, 3, 5]);
    }

    #[test]
    fn budgets_drop_the_oldest_but_never_the_newest() {
        let now = 10 * 86_400_000;
        let policy = BackupPolicy {
            max_total_bytes: 25,
            max_count: 2,
            ..BackupPolicy::default()
        };
        let backups = vec![
            info(1, now, BackupReason::Manual, 100),
            info(2, now, BackupReason::Manual, 10),
            info(3, now, BackupReason::Manual, 10),
        ];
        // The newest alone exceeds the byte budget and is still kept.
        assert_eq!(
            retained(&backups, &policy, now)
                .into_iter()
                .collect::<Vec<_>>(),
            vec![0]
        );
        let backups = vec![
            info(1, now, BackupReason::Manual, 10),
            info(2, now, BackupReason::Manual, 10),
            info(3, now, BackupReason::Manual, 10),
        ];
        assert_eq!(
            retained(&backups, &policy, now)
                .into_iter()
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
    }
}
