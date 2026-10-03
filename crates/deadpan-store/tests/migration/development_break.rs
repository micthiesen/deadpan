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
fn schemas1_through51_and53_through54_fail_before_reading_document_or_acquiring_writer() -> Result {
    for version in (1..=51).chain([53, 54]) {
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
    for version in [1, 16, 38, 51, 53, 54] {
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
    let mut writer = ProjectStore::create(&package, &document)?;
    writer.commit(&deadpan_core::CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("rename")?,
        command: deadpan_core::Command::Rename {
            node: document.root().clone(),
            label: "Retained pending redo".into(),
        },
    })?;
    let renamed = writer.snapshot()?;
    writer.undo(renamed.revision_id(), RevisionId::new("undo")?)?;
    let document = writer.snapshot()?;
    drop(writer);
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
    assert_eq!(store.history_availability()?, (false, true));
    assert_eq!(store.register_version()?, 0);
    assert!(store.registers()?.entries.is_empty());
    // Drop only the additive bank from a private backup of the migrated DB to
    // compare every prior table, schema and row with the original schema 52.
    let comparison_path = scratch.path().join("comparison.sqlite");
    database.backup(rusqlite::MAIN_DB, &comparison_path, None)?;
    let comparison = Connection::open(comparison_path)?;
    comparison.execute_batch("DROP TABLE registers; DROP TABLE register_contents; DROP TABLE register_state; DROP TABLE transaction_steps; PRAGMA user_version=52;")?;
    assert_eq!(cells(&comparison)?, before);
    assert_eq!(cells(&Connection::open(migrated.backup.unwrap())?)?, before);
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    store.redo(document.revision_id(), RevisionId::new("redo-migrated")?)?;
    assert_eq!(store.snapshot()?.nodes(), renamed.nodes());
    store.undo(
        &RevisionId::new("redo-migrated")?,
        RevisionId::new("undo-migrated")?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), document.nodes());
    store.validate()?;
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

fn schema52_fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("additive.deadpan");
    let document = ProjectDocument::new(
        deadpan_core::ProjectId::new("additive-project")?,
        RevisionId::new("initial")?,
        deadpan_core::PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: deadpan_core::FrameRate::new(30, 1)?,
            color_policy: deadpan_core::ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    drop(ProjectStore::create(&package, &document)?);
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute_batch("DROP TABLE registers; DROP TABLE register_contents; DROP TABLE register_state; DROP TABLE transaction_steps; PRAGMA user_version=52;")?;
    Ok(package)
}

#[test]
fn schema52_promotes_with_a_live_wal_reader_without_replacing_its_snapshot() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = schema52_fixture(scratch.path())?;
    let reader = Connection::open(path.join("project.sqlite"))?;
    reader.pragma_update(None, "journal_mode", "WAL")?;
    reader.execute_batch("BEGIN")?;
    let before = cells(&reader)?;
    ProjectStore::migrate(&path)?;
    assert_eq!(
        cells(&reader)?,
        before,
        "existing read transaction retains schema 52"
    );
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    reader.execute_batch("COMMIT")?;
    assert_eq!(
        reader.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        DATABASE_SCHEMA_VERSION
    );
    Ok(())
}

#[test]
fn schema52_writer_and_database_locks_prevent_migration_without_damage() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = schema52_fixture(scratch.path())?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = cells(&database)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path.join(".writer.lock"))?;
    lock.try_lock()?;
    assert!(matches!(
        ProjectStore::migrate(&path),
        Err(StoreError::AlreadyOpen)
    ));
    lock.unlock()?;
    database.execute_batch("BEGIN IMMEDIATE")?;
    let error = ProjectStore::migrate(&path).unwrap_err();
    assert_eq!(error.code(), "ProjectBusy");
    let StoreError::MigrationFailed { backup, .. } = error else {
        panic!("expected retained backup")
    };
    assert_eq!(cells(&database)?, before);
    assert_eq!(cells(&Connection::open(backup)?)?, before);
    database.execute_batch("ROLLBACK")?;
    ProjectStore::migrate(&path)?;
    Ok(())
}

#[test]
fn schema52_malformed_ordinary_history_retains_original_and_exact_backup() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = schema52_fixture(scratch.path())?;
    let database_path = package.join("project.sqlite");
    let mut database = Connection::open(&database_path)?;
    database.pragma_update(None, "journal_mode", "DELETE")?;
    let initial: String =
        database.query_row("SELECT document FROM revisions", [], |row| row.get(0))?;
    let initial = ProjectDocument::from_json(&initial)?;
    let request = deadpan_core::CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: RevisionId::new("ordinary-rename")?,
        command: deadpan_core::Command::Rename {
            node: initial.root().clone(),
            label: "Saved label".into(),
        },
    };
    let edit = deadpan_core::apply(&initial, &request)?;
    let next = edit.forward.apply(&initial)?;
    assert_eq!(edit.inverse.apply(&next)?, initial);
    let transaction = database.transaction()?;
    transaction.execute(
        "INSERT INTO revisions(id,parent_id,kind,document) VALUES(?1,?2,'edit',?3)",
        rusqlite::params![
            next.revision_id().as_str(),
            initial.revision_id().as_str(),
            next.to_json()?
        ],
    )?;
    transaction.execute(
        "INSERT INTO history(id,parent_id,revision_id,request,edit) VALUES(1,NULL,?1,?2,?3)",
        rusqlite::params![
            next.revision_id().as_str(),
            serde_json::to_string(&request)?,
            serde_json::to_string(&edit)?
        ],
    )?;
    transaction.execute(
        "UPDATE state SET head_revision=?1,cursor=1",
        [next.revision_id().as_str()],
    )?;
    // Every identity, both patch directions and the saved snapshot agree. Only
    // the requested label is forged, so replay must reject semantic disagreement.
    transaction.execute(
        "UPDATE history SET request=json_set(request,'$.command.label','Forged label') WHERE id=1",
        [],
    )?;
    transaction.commit()?;
    let prior_cells = cells(&database)?;
    let prior_bytes = fs::read(&database_path)?;
    assert!(entries(&package.join("Snapshots"))?.is_empty());
    let error = ProjectStore::migrate(&package).unwrap_err();
    let StoreError::MigrationFailed { backup, source } = error else {
        panic!("expected semantic rejection after backup")
    };
    assert!(
        matches!(*source, StoreError::History(ref message) if message == "stored command, patches, and revision disagree")
    );
    assert_eq!(fs::read(&database_path)?, prior_bytes);
    assert_eq!(cells(&database)?, prior_cells);
    let snapshots = fs::canonicalize(package.join("Snapshots"))?;
    assert_eq!(backup.parent(), Some(snapshots.as_path()));
    assert_eq!(entries(&package.join("Snapshots"))?.len(), 1);
    let retained =
        Connection::open_with_flags(&backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    // SQLite backup may normalize file-header counters; exact cells include all
    // original JSON text and blobs, including the deliberately forged request.
    assert_eq!(cells(&retained)?, prior_cells);
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&package, mode),
            Err(StoreError::MigrationRequired(52))
        ));
    }
    assert_eq!(fs::read(&database_path)?, prior_bytes);
    assert!(!package.join("project.sqlite-wal").exists());
    Ok(())
}
