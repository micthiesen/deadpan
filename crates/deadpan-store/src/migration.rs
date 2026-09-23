//! Migrate a consistent copy and atomically promote it through SQLite itself.
//! Never rename/copy an open database's main file around its WAL or live readers.

use std::{
    fs::File,
    path::{Path, PathBuf},
};

use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
};
use serde::Serialize;

use crate::{
    AccessMode, ProjectStore, StoreError, acquire_lock, read_flags, require_regular_file, schema,
    validate_extension, validation,
};

#[derive(Debug, Serialize)]
pub struct MigrationOutcome {
    pub from_schema: u32,
    pub to_schema: u32,
    pub backup: Option<PathBuf>,
}

impl ProjectStore {
    /// Explicit engineering migration. Normal read-only inspection never writes.
    /// A persistent, consistent pre-migration backup is retained on both success
    /// and failure after backup creation. The source is untouched until promotion.
    pub fn migrate(path: &Path) -> Result<MigrationOutcome, StoreError> {
        validate_extension(path)?;
        let package = std::fs::canonicalize(path)?;
        let database = package.join("project.sqlite");
        require_regular_file(&database)?;
        let probe = Connection::open_with_flags(&database, read_flags())?;
        schema::configure(&probe)?;
        let version = schema::read_version(&probe)?;
        drop(probe);
        if version == schema::VERSION {
            Self::open(path, AccessMode::ReadOnly)?;
            return Ok(MigrationOutcome {
                from_schema: version,
                to_schema: version,
                backup: None,
            });
        }
        if !matches!(version, 1..=19) {
            return Err(StoreError::UnsupportedSchema(version));
        }
        let lock = acquire_lock(&package)?;
        // Explicit unlock also covers duplicated descriptors on early failure.
        struct Lock(File);
        impl Drop for Lock {
            fn drop(&mut self) {
                let _ = self.0.unlock();
            }
        }
        let _lock = Lock(lock);
        let mut original = Connection::open_with_flags(
            &database,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        schema::configure(&original)?;
        if schema::read_version(&original)? != version {
            return Err(StoreError::MigrationBusy);
        }
        original.pragma_update(None, "synchronous", "FULL")?;
        let directory = package.join("Snapshots");
        if !std::fs::symlink_metadata(&directory)?.file_type().is_dir() {
            return Err(StoreError::UnsafePath(directory));
        }
        let backup = tempfile::Builder::new()
            .prefix(&format!("before-schema-{}-", schema::VERSION))
            .suffix(".sqlite")
            .tempfile_in(&directory)?;
        original.backup(rusqlite::MAIN_DB, backup.path(), None)?;
        backup.as_file().sync_all()?;
        let (_, backup_path) = backup.keep().map_err(|error| error.error)?;
        let migration = (|| {
            File::open(&directory)?.sync_all()?;
            migrate_candidate(&mut original, &directory, &backup_path, version)
        })();
        if let Err(source) = migration {
            return Err(StoreError::MigrationFailed {
                backup: backup_path,
                source: Box::new(source),
            });
        }
        Ok(MigrationOutcome {
            from_schema: version,
            to_schema: schema::VERSION,
            backup: Some(backup_path),
        })
    }
}

fn migrate_candidate(
    original: &mut Connection,
    directory: &Path,
    backup_path: &Path,
    source_version: u32,
) -> Result<(), StoreError> {
    let candidate_file = tempfile::Builder::new()
        .prefix("migration-")
        .suffix(".sqlite")
        .tempfile_in(directory)?;
    let backup = Connection::open_with_flags(backup_path, read_flags())?;
    backup.backup(rusqlite::MAIN_DB, candidate_file.path(), None)?;
    drop(backup);
    let mut candidate = Connection::open(candidate_file.path())?;
    schema::configure(&candidate)?;
    candidate.pragma_update(None, "journal_mode", "DELETE")?;
    candidate.pragma_update(None, "synchronous", "FULL")?;
    let transaction = candidate.transaction()?;
    let integrity: String =
        transaction.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(StoreError::Integrity(integrity));
    }
    let violations: i64 =
        transaction.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if violations != 0 {
        return Err(StoreError::Integrity("foreign-key violation".into()));
    }
    // Install only the schema-8 operational additions before parsing old
    // request/attempt rows. ALTER/CREATE intentionally has no IF NOT EXISTS:
    // a legacy database that already contains modern operational vocabulary is
    // rejected instead of being silently reinterpreted.
    if (5..8).contains(&source_version) {
        crate::generation::add_schema8_columns(&transaction)?;
    }
    if (6..8).contains(&source_version) {
        crate::generation_attempts::add_schema8_tables(&transaction)?;
    }
    if source_version < 5 {
        crate::generation::create_tables(&transaction)?;
    }
    if source_version < 6 {
        crate::generation_attempts::create_tables(&transaction)?;
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if source_version < 10 {
        crate::original_media::create_tables(&transaction)?;
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if source_version < 14 {
        crate::source_registration::create_tables(&transaction)?;
    }
    if source_version < 17 {
        transaction.execute_batch("ALTER TABLE state ADD COLUMN workflow TEXT NOT NULL DEFAULT 'generic' CHECK(workflow IN ('generic','single_source_v1'));")?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::single_source::create_tables(&transaction)?;
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::source_registration::check_stored_sizes(&transaction)?;
    validation::check_stored_sizes(&transaction, schema::MAX_DOCUMENT_BYTES)?;
    if source_version >= 5 {
        crate::generation::check_stored_sizes(&transaction)?;
    }
    if source_version >= 6 {
        crate::generation_attempts::check_stored_sizes(&transaction)?;
    }
    // Schema 8 receipts had no admission evidence. Reject new vocabulary even
    // when set to null rather than interpreting it as an old qualified bundle.
    // Valid old JSON is retained byte-for-byte, without fabricated evidence.
    if source_version == 8 {
        let modern: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM generation_bundle_receipts
             WHERE json_type(bundle, '$.admission') IS NOT NULL)",
            [],
            |row| row.get(0),
        )?;
        if modern {
            return Err(StoreError::Integrity(
                "schema-8 receipt contains schema-9 admission evidence".into(),
            ));
        }
    }
    // Database versions 4 through 6 share core schema 4, 7 through 10 share
    // core schema 5, 11 uses core schema 6 with explicit audio mappings, and
    // 12 uses core schema 7 with independent picture durations, and 13 uses
    // core schema 8 with signed stream placements and no source qualification.
    // Schema 14 uses core schema 9 with immutable source qualifications.
    // Schema 15 uses core schema 10 with authored presentation basis policy.
    // Schemas 16 and 17 use core schema 11 with authored audio edges. Their
    // Retimes gain Edit purpose; no old crop becomes a transparent partition.
    // Schema 18 uses core schema 12, including transparent partitions. Through
    // schema 18, every mark retains one binding; old JSON cannot add fragments.
    // Schema 19 uses core schema 13 with multiple bindings per logical mark;
    // its frozen command grammar does not admit Split.
    // Replay all authored history, preserving operational rows and identities
    // while assigning FitBeat only to mappings absent in that legacy schema.
    // Schemas before 15 gain an explicit basis; schema 15 retains its policy.
    // Pre-16 audio edges gain Automatic without changing allocated time;
    // schemas 16 and 17 retain their authored edge choices.
    validation::migrate_history(&transaction, source_version)?;
    crate::generation::validate_store(&transaction)?;
    transaction.pragma_update(None, "user_version", schema::VERSION)?;
    validation::validate_history(&transaction)?;
    crate::generation::validate_store(&transaction)?;
    crate::generation_attempts::validate_store(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::original_media::validate_store(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::source_registration::validate_store(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::single_source::validate_store(&transaction)?;
    transaction.commit()?;
    candidate_file.as_file().sync_all()?;
    // One step copies all pages in one destination transaction. SQLITE_BUSY
    // or LOCKED is reported, never retried forever. Backup::drop rolls back
    // incomplete work; Done commits the complete validated database.
    promote(&candidate, original)
}

fn promote(candidate: &Connection, original: &mut Connection) -> Result<(), StoreError> {
    let promotion = Backup::new(candidate, original)?;
    match promotion.step(-1)? {
        StepResult::Done => {}
        _ => return Err(StoreError::MigrationBusy),
    }
    drop(promotion);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn promotion_fixture(root: &Path) -> Result<(Connection, Connection), StoreError> {
        let original = Connection::open(root.join("original.sqlite"))?;
        original.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;
            CREATE TABLE preserved(value TEXT); INSERT INTO preserved VALUES ('before'); PRAGMA user_version=1;")?;
        original.backup(rusqlite::MAIN_DB, root.join("candidate.sqlite"), None)?;
        let candidate = Connection::open(root.join("candidate.sqlite"))?;
        candidate.execute_batch(
            "UPDATE preserved SET value='after'; PRAGMA user_version=2;
            CREATE TABLE growth(data BLOB); INSERT INTO growth VALUES (zeroblob(1048576));",
        )?;
        Ok((candidate, original))
    }

    fn assert_original(connection: &Connection) -> Result<(), StoreError> {
        assert_eq!(
            connection.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            1
        );
        assert_eq!(
            connection.query_row("SELECT value FROM preserved", [], |row| row
                .get::<_, String>(0))?,
            "before"
        );
        assert_eq!(
            connection.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))?,
            "ok"
        );
        Ok(())
    }

    #[test]
    fn disk_full_during_promotion_rolls_back_original() -> Result<(), StoreError> {
        let scratch = tempfile::tempdir()?;
        let (candidate, mut original) = promotion_fixture(scratch.path())?;
        let pages: i64 = original.pragma_query_value(None, "page_count", |row| row.get(0))?;
        original.pragma_update(None, "max_page_count", pages)?;
        assert_eq!(
            promote(&candidate, &mut original).unwrap_err().code(),
            "DiskFull"
        );
        assert_original(&original)?;
        drop(original);
        assert_original(&Connection::open(scratch.path().join("original.sqlite"))?)
    }

    #[test]
    fn crash_during_partial_promotion() -> Result<(), StoreError> {
        if let Some(root) = std::env::var_os("DEADPAN_PROMOTION_CRASH_ROOT") {
            let root = Path::new(&root);
            let (candidate, mut original) = promotion_fixture(root)?;
            let path = root.join("original.sqlite");
            let original_bytes = std::fs::metadata(&path)?.len();
            original.pragma_update(None, "cache_size", 5)?;
            let backup = Backup::new(&candidate, &mut original)?;
            assert_eq!(backup.step(128)?, StepResult::More);
            assert!(
                std::fs::metadata(&path)?.len() > original_bytes,
                "dirty destination pages must have reached the file before the crash"
            );
            // No Rust destructors or sqlite3_backup_finish: recovery must use the
            // interrupted destination transaction, not a cooperative rollback.
            std::process::exit(79);
        }
        let scratch = tempfile::tempdir()?;
        let child = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "migration::tests::crash_during_partial_promotion",
            ])
            .env("DEADPAN_PROMOTION_CRASH_ROOT", scratch.path())
            .output()?;
        assert_eq!(
            child.status.code(),
            Some(79),
            "{}",
            String::from_utf8_lossy(&child.stderr)
        );
        assert_original(&Connection::open(scratch.path().join("original.sqlite"))?)
    }
}
