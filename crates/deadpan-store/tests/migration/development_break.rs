//! Unused core-33 through 39 development formats are rejected without a migration.

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
    Ok(())
}

#[test]
fn schemas39_through51_and53_through54_fail_before_reading_document_or_acquiring_writer() -> Result
{
    for version in (39..=51).chain([53, 54]) {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("unsupported.deadpan");
        fs::create_dir(&package)?;
        fs::create_dir(package.join("Snapshots"))?;
        let database = Connection::open(package.join("project.sqlite"))?;
        database.execute_batch(
            "PRAGMA application_id=1146113585;
             CREATE TABLE untouched(value TEXT NOT NULL);
             INSERT INTO untouched VALUES('unparsed development data');",
        )?;
        database.pragma_update(None, "user_version", version)?;
        drop(database);
        // This deliberately has no current tables or media directories. The
        // version preflight must reject it before parsing or repairing them.
        assert_refused(&package, version)?;
        assert!(!package.join(".writer.lock").exists());
        assert_eq!(fs::read_dir(package.join("Snapshots"))?.count(), 0);
    }
    Ok(())
}

#[test]
fn current_schema55_migration_is_read_only_and_needs_no_backup_or_writer() -> Result {
    use deadpan_core::{ColorPolicy, FrameRate, PresentationBasis, ProjectId};

    assert_eq!(DATABASE_SCHEMA_VERSION, 55);
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
    assert_eq!((outcome.from_schema, outcome.to_schema), (55, 55));
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

#[test]
fn schema52_additive_migration_preserves_history_and_creates_empty_register_bank() -> Result {
    use deadpan_core::{ColorPolicy, FrameRate, PresentationBasis, ProjectId};
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("register-upgrade.deadpan");
    let document = ProjectDocument::new(
        ProjectId::new("register-upgrade")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    drop(ProjectStore::create(&package, &document)?);
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute_batch("DROP TABLE registers; DROP TABLE register_contents; DROP TABLE register_state; DROP TABLE transaction_steps; PRAGMA user_version=52;")?;
    let before = cells(&database)?;
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&package, mode),
            Err(StoreError::MigrationRequired(52))
        ));
        assert_eq!(cells(&database)?, before);
    }
    let migrated = ProjectStore::migrate(&package)?;
    assert_eq!((migrated.from_schema, migrated.to_schema), (52, 55));
    assert!(migrated.backup.as_ref().is_some_and(|path| path.is_file()));
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(store.snapshot()?, document);
    assert_eq!(store.history_availability()?, (false, false));
    assert_eq!(store.register_version()?, 0);
    assert!(store.registers()?.entries.is_empty());
    // Drop only the additive bank from a private backup of the migrated DB to
    // compare every prior table, schema and row with the original schema 52.
    let comparison_path = scratch.path().join("comparison.sqlite");
    database.backup(rusqlite::MAIN_DB, &comparison_path, None)?;
    let comparison = Connection::open(comparison_path)?;
    comparison.execute_batch("DROP TABLE registers; DROP TABLE register_contents; DROP TABLE register_state; DROP TABLE transaction_steps; PRAGMA user_version=52;")?;
    assert_eq!(cells(&comparison)?, before);
    Ok(())
}

#[test]
fn schema52_rejects_compound_history_even_without_capture_checkpoints() -> Result {
    use deadpan_core::{
        BeatNode, ColorPolicy, Command, CommandRequest, FrameRate, LeafEdit, PresentationBasis,
        ProjectId, ResolvedStep, ResolvedTransaction, Subtree,
    };
    use std::collections::BTreeMap;

    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("forged-compound.deadpan");
    let initial = ProjectDocument::new(
        ProjectId::new("forged-compound")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create(&package, &initial)?;
    let child = NodeId::new("child")?;
    store.commit(&CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: RevisionId::new("compound")?,
        command: Command::Compound {
            transaction: ResolvedTransaction::new(
                0,
                BTreeMap::new(),
                vec![ResolvedStep::Edit {
                    edit: LeafEdit::new(
                        RevisionId::new("stage")?,
                        Command::Insert {
                            parent: initial.root().clone(),
                            index: 0,
                            subtree: Subtree {
                                root: child.clone(),
                                nodes: BTreeMap::from([(
                                    child,
                                    BeatNode::sequence("Empty", Vec::new()),
                                )]),
                                overrides: BTreeMap::new(),
                                gap_overrides: BTreeMap::new(),
                            },
                        },
                    )?,
                }],
            )?,
        },
    })?;
    drop(store);
    let database = Connection::open(package.join("project.sqlite"))?;
    // Its request, net patches and final snapshot agree. Removing the modern
    // operational tables must not make that unknown command valid in schema52.
    database.execute_batch("DROP TABLE registers; DROP TABLE register_contents; DROP TABLE register_state; DROP TABLE transaction_steps; PRAGMA user_version=52;")?;
    let before = cells(&database)?;
    let error = ProjectStore::migrate(&package).unwrap_err();
    let StoreError::MigrationFailed { backup, source } = error else {
        panic!("expected backed-up migration refusal")
    };
    assert!(
        source
            .to_string()
            .contains("schema 52 cannot contain compound commands"),
        "{source}"
    );
    assert_eq!(cells(&database)?, before);
    let backup = Connection::open(backup)?;
    assert_eq!(cells(&backup)?, before);
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(52))
    ));
    Ok(())
}

/// Synthetic legacy fixtures may remove only the newly created empty bank.
/// Authentic older packages and deliberately injected collisions keep all cells.
pub(super) fn remove_empty_register_tables(database: &Connection) -> Result {
    for table in ["registers", "register_contents", "transaction_steps"] {
        assert_eq!(
            database.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row
                .get::<_, i64>(0))?,
            0
        );
    }
    let states = database
        .prepare("SELECT singleton,version FROM register_state ORDER BY singleton")?
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    assert_eq!(states, vec![(1, 0)]);
    database.execute_batch(
        "DROP TABLE registers; DROP TABLE register_contents; DROP TABLE register_state; DROP TABLE transaction_steps;",
    )?;
    Ok(())
}
