//! Release migrations: backed up, run on a copy, validated, then promoted.
//!
//! The production chain [`MIGRATIONS`] holds two steps, 66 to 67 (adding the
//! empty `retired_identities` table) and 67 to 68 (adding AI variant
//! retention records), so packages of the previous builds keep opening.
//! Under the 2026-09-30 development-format authorization every earlier
//! development schema is still refused as `UnsupportedSchema` before
//! a writer, backup or parse, and current packages only validate. The runner below is the release
//! mechanism the specification (20.4) requires, exercised by a synthetic
//! N to N+1 migration in this module's tests:
//!
//! 1. Hold the package's writer lock, so no editor or headless writer runs.
//! 2. Publish a raw `before-migration` backup of the old database in
//!    `Backups/` (SQLite backup API, integrity-checked).
//! 3. Copy the old database to a hidden staging file beside it and apply
//!    each step `N -> N+1` in one transaction on that copy, ending at the
//!    target `user_version`.
//! 4. Validate the copy with the target build's complete validator (SQLite
//!    integrity, foreign keys, full history replay and every table).
//! 5. Fold the old database's WAL into its main file and leave WAL mode, so
//!    no stale WAL can be applied to the new file, then atomically rename the
//!    copy over `project.sqlite` and synchronize the package directory.
//!
//! Any failure before step 5's rename leaves `project.sqlite` byte-for-byte
//! as it was (apart from checkpointing its own WAL) and removes the staging
//! copy; the backup stays. A crash leaves either the old or the new
//! database, never a mixture, plus at most a hidden staging file that the
//! next migration removes.

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::fs::{self, File, OpenOptions};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::sync::atomic::AtomicBool;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use rusqlite::OpenFlags;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use rusqlite::backup::Backup;
use rusqlite::{Connection, Transaction};
use serde::Serialize;

use crate::{
    AccessMode, ProjectStore, StoreError, read_flags, require_regular_file, schema,
    validate_extension,
};

#[derive(Debug, Serialize)]
pub struct MigrationOutcome {
    pub from_schema: u32,
    pub to_schema: u32,
    pub backup: Option<PathBuf>,
}

/// One release step from database schema `from` to `from + 1`. It runs
/// inside the single migration transaction on the staged copy and must not
/// commit, change `user_version` or touch files.
pub struct Migration {
    pub from: u32,
    pub apply: fn(&Transaction<'_>) -> Result<(), StoreError>,
}

/// The target build's whole-database check of a migrated copy.
pub type MigrationValidator = fn(&Connection) -> Result<(), StoreError>;

/// Production migrations, oldest first. Development formats may still break
/// without one; a step is added when existing packages would otherwise be
/// stranded.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        from: 66,
        apply: add_retired_identities,
    },
    Migration {
        from: 67,
        apply: add_variant_retention,
    },
];

/// 66 to 67: restores now record what they discarded. A schema-66 package
/// never restored anything that could have freed an identity (restore and
/// the table arrived together), so the table starts empty.
fn add_retired_identities(transaction: &Transaction<'_>) -> Result<(), StoreError> {
    crate::retired::create_tables(transaction)
}

/// 67 to 68: every AI variant gets a retention record. Present variants
/// count their retention period from the upgrade, so none expires sooner
/// than a full period afterwards.
fn add_variant_retention(transaction: &Transaction<'_>) -> Result<(), StoreError> {
    crate::generation_retention::migrate(transaction, std::time::SystemTime::now())
}

/// Whether this build can migrate a package of `found` to its own schema.
pub(crate) fn migratable(found: u32) -> bool {
    found < schema::VERSION && chain(MIGRATIONS, found, schema::VERSION).is_some()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
const STAGING_PREFIX: &str = ".migrating-";

impl ProjectStore {
    /// Bring a package to this build's schema. A current package is only
    /// validated read-only (no writer, no backup). An older package with a
    /// complete chain in [`MIGRATIONS`] is backed up, migrated on a copy,
    /// validated and promoted; without one (every development format today)
    /// it is refused as `UnsupportedSchema` before a writer, backup or parse.
    /// A newer package is refused as `NewerSchema`.
    pub fn migrate(path: &Path) -> Result<MigrationOutcome, StoreError> {
        validate_extension(path)?;
        let package = std::fs::canonicalize(path)?;
        let database = package.join("project.sqlite");
        require_regular_file(&database)?;
        let probe = Connection::open_with_flags(&database, read_flags())?;
        schema::configure(&probe)?;
        let found = schema::read_version(&probe)?;
        drop(probe);
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if migratable(found) {
            return migrate_package_with(path, MIGRATIONS, schema::VERSION, validate_current);
        }
        let probe = Connection::open_with_flags(&database, read_flags())?;
        schema::configure(&probe)?;
        schema::check_version(&probe)?;
        drop(probe);
        Self::open(path, AccessMode::ReadOnly)?.validate_full()?;
        Ok(MigrationOutcome {
            from_schema: schema::VERSION,
            to_schema: schema::VERSION,
            backup: None,
        })
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
/// The complete validator of this build, as `project validate` runs it.
fn validate_current(connection: &Connection) -> Result<(), StoreError> {
    crate::validate_database(connection, crate::validation::HistoryMode::Full).map(|_| ())
}

/// The contiguous steps from `found` to `target`, if the chain covers them.
pub(crate) fn chain(migrations: &[Migration], found: u32, target: u32) -> Option<Vec<&Migration>> {
    let mut steps = Vec::new();
    let mut version = found;
    while version < target {
        let step = migrations.iter().find(|step| step.from == version)?;
        steps.push(step);
        version = version.checked_add(1)?;
    }
    (version == target).then_some(steps)
}

/// Run `migrations` from the package's schema to `target`, validating the
/// result with `validate`. The release runner behind [`ProjectStore::migrate`];
/// public so that release tooling and tests can exercise a chain other than
/// the (currently empty) production one.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[doc(hidden)]
pub fn migrate_package_with(
    path: &Path,
    migrations: &[Migration],
    target: u32,
    validate: MigrationValidator,
) -> Result<MigrationOutcome, StoreError> {
    validate_extension(path)?;
    let package = std::fs::canonicalize(path)?;
    let database = package.join("project.sqlite");
    require_regular_file(&database)?;
    let probe = Connection::open_with_flags(&database, read_flags())?;
    schema::configure(&probe)?;
    let found = schema::read_version(&probe)?;
    drop(probe);
    if found == target {
        return Ok(MigrationOutcome {
            from_schema: found,
            to_schema: target,
            backup: None,
        });
    }
    if found > target {
        return Err(StoreError::NewerSchema {
            found,
            supported: target,
        });
    }
    let steps = chain(migrations, found, target).ok_or(StoreError::UnsupportedSchema(found))?;
    // Exclusive ownership for the whole migration; an open editor refuses it.
    let _lock = crate::acquire_lock(&package)?;
    remove_stale_staging(&package)?;
    let backup = crate::backups::create_raw_backup(
        &package,
        crate::backups::BackupReason::BeforeMigration,
        &AtomicBool::new(false),
    )
    .map_err(|error| match error {
        crate::backups::BackupError::Store(error) => error,
        crate::backups::BackupError::Io(error) => StoreError::Io(error),
        crate::backups::BackupError::Database(error) => StoreError::Database(error),
        other => StoreError::Storage(format!("the backup before migrating failed: {other}")),
    })?
    .backup
    .path;
    let failed = |source: StoreError| StoreError::MigrationFailed {
        backup: backup.clone(),
        source: Box::new(source),
    };
    let staged = Staging::create(&package).map_err(failed)?;
    run_on_copy(&database, &staged.path, &steps, target, validate).map_err(failed)?;
    promote(&package, &database, staged).map_err(failed)?;
    Ok(MigrationOutcome {
        from_schema: found,
        to_schema: target,
        backup: Some(backup),
    })
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn run_on_copy(
    database: &Path,
    staged: &Path,
    steps: &[&Migration],
    target: u32,
    validate: MigrationValidator,
) -> Result<(), StoreError> {
    let source = Connection::open_with_flags(database, read_flags())?;
    schema::configure(&source)?;
    let mut copy = Connection::open_with_flags(
        staged,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    schema::configure(&copy)?;
    copy.pragma_update(None, "journal_mode", "DELETE")?;
    Backup::new(&source, &mut copy)?.run_to_completion(
        256,
        std::time::Duration::from_millis(2),
        None,
    )?;
    drop(source);
    copy.pragma_update(None, "journal_mode", "DELETE")?;
    let transaction = copy.transaction()?;
    for step in steps {
        (step.apply)(&transaction)?;
    }
    transaction.pragma_update(None, "user_version", target)?;
    transaction.commit()?;
    let integrity: String = copy.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(StoreError::Integrity(integrity));
    }
    validate(&copy)?;
    copy.close()
        .map_err(|(_, error)| StoreError::Database(error))?;
    File::open(staged)?.sync_all()?;
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
/// Replace `project.sqlite` with the validated copy in one rename.
fn promote(package: &Path, database: &Path, mut staged: Staging) -> Result<(), StoreError> {
    // Fold the WAL into the old main file and leave WAL mode, which removes
    // the WAL: a stale WAL must never be replayed onto the new file.
    let old = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    schema::configure(&old)?;
    let (busy, _, _): (i64, i64, i64) =
        old.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    if busy != 0 {
        return Err(StoreError::MigrationBusy);
    }
    let mode: String = old.query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("delete") {
        return Err(StoreError::MigrationBusy);
    }
    old.close()
        .map_err(|(_, error)| StoreError::Database(error))?;
    for sidecar in ["project.sqlite-wal", "project.sqlite-shm"] {
        match fs::symlink_metadata(package.join(sidecar)) {
            Ok(metadata) if metadata.len() > 0 && sidecar.ends_with("-wal") => {
                return Err(StoreError::MigrationBusy);
            }
            _ => {}
        }
    }
    crate::failpoint("migration-after-wal-fold");
    fs::rename(&staged.path, database)?;
    staged.promoted = true;
    crate::failpoint("migration-after-rename");
    File::open(package)?.sync_all()?;
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn remove_stale_staging(package: &Path) -> Result<(), StoreError> {
    for entry in fs::read_dir(package)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(STAGING_PREFIX))
            && fs::symlink_metadata(entry.path())?.is_file()
        {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
struct Staging {
    path: PathBuf,
    promoted: bool,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Staging {
    fn create(package: &Path) -> Result<Self, StoreError> {
        let path = package.join(format!(
            "{STAGING_PREFIX}{}.sqlite",
            uuid::Uuid::new_v4().simple()
        ));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(&path)?;
        Ok(Self {
            path,
            promoted: false,
        })
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Drop for Staging {
    fn drop(&mut self) {
        if !self.promoted {
            let _ = fs::remove_file(&self.path);
            let _ = fs::remove_file(self.path.with_extension("sqlite-journal"));
        }
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests {
    use super::*;
    use deadpan_core::{ColorPolicy, FrameRate, NodeId, PresentationBasis, ProjectDocument};
    use deadpan_core::{ProjectId, RevisionId};

    const NEXT: u32 = schema::VERSION + 1;

    fn synthetic(transaction: &Transaction<'_>) -> Result<(), StoreError> {
        transaction.execute_batch(
            "CREATE TABLE synthetic_release(singleton INTEGER PRIMARY KEY CHECK(singleton=1), note TEXT NOT NULL) STRICT;
             INSERT INTO synthetic_release VALUES (1, 'added by the synthetic release step');",
        )?;
        Ok(())
    }

    fn failing(transaction: &Transaction<'_>) -> Result<(), StoreError> {
        synthetic(transaction)?;
        Err(StoreError::Storage("the synthetic step failed".into()))
    }

    fn rejecting(_: &Connection) -> Result<(), StoreError> {
        Err(StoreError::Integrity(
            "the synthetic validator rejected the copy".into(),
        ))
    }

    fn package() -> Result<(tempfile::TempDir, PathBuf, ProjectDocument), Box<dyn std::error::Error>>
    {
        let root = tempfile::tempdir()?;
        let path = root.path().join("release.deadpan");
        let document = ProjectDocument::new(
            ProjectId::new("project")?,
            RevisionId::new("initial")?,
            PresentationBasis {
                width: 1920,
                height: 1080,
                frame_rate: FrameRate::new(30, 1)?,
                color_policy: ColorPolicy::SdrRec709,
            },
            NodeId::new("root")?,
        )?;
        drop(ProjectStore::create(&path, &document)?);
        let path = path.canonicalize()?;
        Ok((root, path, document))
    }

    fn database_bytes(path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Fold any WAL so the comparison sees the whole committed state.
        let connection = Connection::open(path.join("project.sqlite"))?;
        connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        drop(connection);
        Ok(fs::read(path.join("project.sqlite"))?)
    }

    fn hidden(path: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        Ok(fs::read_dir(path)?
            .flatten()
            .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
            .filter(|name| name.starts_with(STAGING_PREFIX))
            .collect())
    }

    const STEP: &[Migration] = &[Migration {
        from: schema::VERSION,
        apply: synthetic,
    }];

    #[test]
    fn synthetic_release_step_backs_up_validates_and_promotes()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_root, path, document) = package()?;
        let outcome = migrate_package_with(&path, STEP, NEXT, validate_current)?;
        assert_eq!(
            (outcome.from_schema, outcome.to_schema),
            (schema::VERSION, NEXT)
        );
        let backup = outcome.backup.ok_or("missing backup")?;
        // The backup is the old database, listed with its reason.
        let listed = crate::backups::list_backups(&path)?;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, backup);
        assert_eq!(
            listed[0].reason,
            crate::backups::BackupReason::BeforeMigration
        );
        let old = Connection::open_with_flags(&backup, read_flags())?;
        assert_eq!(schema::read_version(&old)?, schema::VERSION);
        // The package now carries the new schema and the step's table.
        let new = Connection::open_with_flags(path.join("project.sqlite"), read_flags())?;
        assert_eq!(schema::read_version(&new)?, NEXT);
        let note: String =
            new.query_row("SELECT note FROM synthetic_release", [], |row| row.get(0))?;
        assert_eq!(note, "added by the synthetic release step");
        drop(new);
        assert!(hidden(&path)?.is_empty());
        // This build sees a newer package: writers refuse, viewing works and
        // shows the same document.
        assert!(matches!(
            ProjectStore::open(&path, AccessMode::ReadWrite),
            Err(StoreError::NewerSchema { found, .. }) if found == NEXT
        ));
        let viewer = ProjectStore::open(&path, AccessMode::ReadOnly)?;
        assert_eq!(viewer.newer_schema(), Some(NEXT));
        assert_eq!(viewer.snapshot()?, document);
        // Running again is a no-op.
        let again = migrate_package_with(&path, STEP, NEXT, validate_current)?;
        assert!(again.backup.is_none());
        Ok(())
    }

    #[test]
    fn failed_steps_and_rejected_copies_leave_the_database_unchanged()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_root, path, document) = package()?;
        let before = database_bytes(&path)?;
        let failing_step: &[Migration] = &[Migration {
            from: schema::VERSION,
            apply: failing,
        }];
        let error = migrate_package_with(&path, failing_step, NEXT, validate_current)
            .expect_err("the step fails");
        assert!(
            matches!(error, StoreError::MigrationFailed { .. }),
            "{error}"
        );
        assert_eq!(database_bytes(&path)?, before);
        assert!(hidden(&path)?.is_empty());
        let error = migrate_package_with(&path, STEP, NEXT, rejecting).expect_err("rejected");
        assert!(
            matches!(error, StoreError::MigrationFailed { .. }),
            "{error}"
        );
        assert_eq!(database_bytes(&path)?, before);
        assert!(hidden(&path)?.is_empty());
        // Both attempts kept their backups; the project still opens.
        assert_eq!(crate::backups::list_backups(&path)?.len(), 2);
        assert_eq!(
            ProjectStore::open(&path, AccessMode::ReadWrite)?.snapshot()?,
            document
        );
        Ok(())
    }

    #[test]
    fn gaps_open_writers_and_newer_packages_refuse_without_writing()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_root, path, _) = package()?;
        let before = database_bytes(&path)?;
        // No step reaches the target.
        assert!(matches!(
            migrate_package_with(&path, &[], NEXT, validate_current),
            Err(StoreError::UnsupportedSchema(found)) if found == schema::VERSION
        ));
        // An open writer owns the package.
        let writer = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert!(matches!(
            migrate_package_with(&path, STEP, NEXT, validate_current),
            Err(StoreError::AlreadyOpen)
        ));
        drop(writer);
        // A package newer than the target is never rewritten.
        assert!(matches!(
            migrate_package_with(&path, STEP, schema::VERSION - 1, validate_current),
            Err(StoreError::NewerSchema { .. })
        ));
        assert_eq!(database_bytes(&path)?, before);
        assert!(crate::backups::list_backups(&path)?.is_empty());
        // Only the two previous schemas migrate; older development formats
        // are still refused.
        assert!(chain(MIGRATIONS, schema::VERSION - 1, schema::VERSION).is_some());
        assert!(chain(MIGRATIONS, schema::VERSION - 2, schema::VERSION).is_some());
        assert!(chain(MIGRATIONS, schema::VERSION - 3, schema::VERSION).is_none());
        Ok(())
    }
}
