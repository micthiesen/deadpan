//! Obsolete development formats are rejected before package or history admission.

use super::*;

/// Include every table and schema definition so refusal cannot hide changes to
/// operational state behind an unchanged authored document.
pub(super) fn cells(database: &Connection) -> Result<Vec<String>> {
    let mut cells = vec![format!(
        "version:{}",
        database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?
    )];
    cells.extend(
        database
            .prepare("SELECT type || '|' || name || '|' || coalesce(sql, '') FROM sqlite_schema ORDER BY type,name")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?,
    );
    let tables = database
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for table in tables {
        assert!(
            table
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        );
        cells.push(table.clone());
        let mut statement = database.prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))?;
        let width = statement.column_count();
        cells.extend(
            statement
                .query_map([], |row| {
                    (0..width)
                        .map(|index| row.get_ref(index).map(|value| format!("{value:?}")))
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map(|values| values.join("|"))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(cells)
}

fn entries(directory: &Path) -> Result<Vec<std::ffi::OsString>> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut entries = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort();
    Ok(entries)
}

pub(super) fn assert_refused(package: &Path, version: u32) -> Result {
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = cells(&database)?;
    let package_entries = entries(package)?;
    let backups = entries(&package.join("Snapshots"))?;
    let main_bytes = fs::read(package.join("project.sqlite"))?;
    let wal_bytes = fs::read(package.join("project.sqlite-wal")).ok();
    let journal: String = database.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(package, mode),
            Err(StoreError::UnsupportedSchema(found)) if found == version
        ));
        assert_eq!(cells(&database)?, before);
        assert_eq!(entries(package)?, package_entries);
        assert_eq!(entries(&package.join("Snapshots"))?, backups);
    }
    assert!(matches!(
        ProjectStore::migrate(package),
        Err(StoreError::UnsupportedSchema(found)) if found == version
    ));
    assert_eq!(cells(&database)?, before);
    assert_eq!(entries(package)?, package_entries);
    assert_eq!(entries(&package.join("Snapshots"))?, backups);
    assert_eq!(fs::read(package.join("project.sqlite"))?, main_bytes);
    assert_eq!(fs::read(package.join("project.sqlite-wal")).ok(), wal_bytes);
    assert_eq!(
        database.pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))?,
        journal
    );
    Ok(())
}

#[test]
fn schemas1_through72_fail_before_reading_document_or_acquiring_writer() -> Result {
    for version in 1..=72 {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("unsupported.deadpan");
        fs::create_dir(&package)?;
        let database = Connection::open(package.join("project.sqlite"))?;
        database.execute_batch(
            "PRAGMA application_id=1146113585;
             CREATE TABLE untouched(value TEXT NOT NULL);
             INSERT INTO untouched VALUES('unparsed development data');
             CREATE TABLE revisions(document);
             INSERT INTO revisions VALUES('not JSON');
             INSERT INTO revisions VALUES(X'00');",
        )?;
        database.pragma_update(None, "user_version", version)?;
        drop(database);
        // This deliberately has no current tables or media directories. The
        // version preflight must reject it before parsing or repairing them.
        assert_refused(&package, version)?;
        assert!(!package.join(".writer.lock").exists());
        assert!(!package.join("Snapshots").exists());
    }
    Ok(())
}

#[test]
fn obsolete_schema_refusal_precedes_writer_lock_and_preserves_live_wal() -> Result {
    for version in [
        1, 16, 38, 51, 52, 53, 54, 55, 56, 59, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72,
    ] {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("locked.deadpan");
        fs::create_dir(&package)?;
        let lock = fs::File::create(package.join(".writer.lock"))?;
        lock.lock()?;
        let database = Connection::open(package.join("project.sqlite"))?;
        database.execute_batch("PRAGMA application_id=1146113585; PRAGMA journal_mode=WAL; CREATE TABLE untouched(value); INSERT INTO untouched VALUES('retained WAL');")?;
        database.pragma_update(None, "user_version", version)?;
        assert!(package.join("project.sqlite-wal").is_file());
        // Missing native package directories and an already-held writer must
        // not obscure the format refusal or initiate recovery.
        assert_refused(&package, version)?;
        lock.unlock()?;
    }
    Ok(())
}

#[test]
fn current_schema73_migration_is_read_only_and_needs_no_backup_or_writer() -> Result {
    use deadpan_core::{ColorPolicy, FrameRate, PresentationBasis, ProjectId};

    assert_eq!(DATABASE_SCHEMA_VERSION, 73);
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("current.deadpan");
    let document = ProjectDocument::new(
        ProjectId::new("current-development-project")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 1280,
            height: 720,
            frame_rate: FrameRate::new(30_000, 1_001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let writer = ProjectStore::create(&package, &document)?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = cells(&database)?;
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!((outcome.from_schema, outcome.to_schema), (73, 73));
    assert!(outcome.backup.is_none());
    assert_eq!(cells(&database)?, before);
    assert_eq!(fs::read_dir(package.join("Snapshots"))?.count(), 0);
    assert_eq!(writer.snapshot()?, document);
    drop(writer);
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert_eq!(ProjectStore::open(&package, mode)?.snapshot()?, document);
    }
    Ok(())
}
