use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use deadpan_core::{IterationOrder, NodeId, NodeKind, ProjectDocument, RevisionId, legacy_v1};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn fixture(scratch: &Path) -> Result<PathBuf> {
    let package = scratch.join("legacy.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let connection = Connection::open(package.join("project.sqlite"))?;
    connection.pragma_update(None, "foreign_keys", false)?;
    connection.execute_batch(include_str!("fixtures/v1-history.sql"))?;
    Ok(package)
}
fn docs(connection: &Connection) -> Result<Vec<(String, String)>> {
    Ok(connection
        .prepare("SELECT id, document FROM revisions ORDER BY id")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?)
}
fn metadata(connection: &Connection) -> Result<String> {
    let mut parts = Vec::new();
    for sql in [
        "SELECT json_array(id,parent_id,kind) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
    ] {
        parts.extend(
            connection
                .prepare(sql)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(parts.join("\n"))
}
fn contents(connection: &Connection) -> Result<String> {
    let mut values = vec![
        metadata(connection)?,
        format!(
            "{}",
            connection.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?
        ),
    ];
    values.extend(docs(connection)?.into_iter().map(|(_, json)| json));
    values.extend(
        connection
            .prepare("SELECT json_array(request,edit) FROM history ORDER BY id")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?,
    );
    Ok(values.join("\n"))
}
fn snapshot(connection: &Connection, revision: &str) -> Result<ProjectDocument> {
    let json: String = connection.query_row(
        "SELECT document FROM revisions WHERE id=?1",
        [revision],
        |row| row.get(0),
    )?;
    Ok(ProjectDocument::from_json(&json)?)
}
fn order(document: &ProjectDocument) -> &IterationOrder {
    let NodeKind::Repeat { iterations, .. } =
        &document.nodes()[&NodeId::new("repeat").unwrap()].kind
    else {
        panic!("fixture Repeat")
    };
    iterations
}

#[test]
fn old_binary_history_migrates_with_stable_ids_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture(scratch.path())?;
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&path, mode),
            Err(StoreError::MigrationRequired(1))
        ));
    }
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_metadata = metadata(&database)?;
    let old_docs = docs(&database)?;
    drop(database);
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!((migration.from_schema, migration.to_schema), (1, 2));
    let backup = Connection::open(migration.backup.unwrap())?;
    assert_eq!(contents(&backup)?, before);
    let database = Connection::open(path.join("project.sqlite"))?;
    assert_eq!(metadata(&database)?, old_metadata);
    for (revision, json) in old_docs {
        assert!(
            legacy_v1::Document::from_json(&json)?.matches(&snapshot(&database, &revision)?),
            "{revision}"
        );
    }
    let original = snapshot(&database, "v1-wrap")?;
    let smaller = snapshot(&database, "v1-shrink")?;
    let grown = snapshot(&database, "v1-grow")?;
    let branch = snapshot(&database, "v1-branch")?;
    for position in 0..2 {
        assert_eq!(order(&original).at(position), order(&smaller).at(position));
        assert_eq!(order(&original).at(position), order(&grown).at(position));
        assert_eq!(order(&original).at(position), order(&branch).at(position));
    }
    assert_eq!(order(&grown).at(2).unwrap().allocation.as_str(), "v1-grow");
    assert_eq!(
        order(&branch).at(2).unwrap().allocation.as_str(),
        "v1-branch"
    );
    assert_ne!(order(&grown).at(2), order(&branch).at(2));
    assert!(
        order(&grown)
            .position(&order(&original).at(2).unwrap())
            .is_none()
    );
    drop(database);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(order(&store.snapshot()?).len(), 2);
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("after-migration-redo-1")?,
    )?;
    assert_eq!(order(&store.snapshot()?), order(&branch));
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("after-migration-redo-2")?,
    )?;
    let current = store.snapshot()?;
    let NodeKind::Repeat { iterations, .. } = &current.nodes()[&NodeId::new("insert-repeat")?].kind
    else {
        panic!("inserted repeat")
    };
    assert_eq!(iterations.at(0).unwrap().allocation.as_str(), "v1-subtree");
    store.undo(
        current.revision_id(),
        RevisionId::new("after-migration-undo")?,
    )?;
    store.validate()?;
    drop(store);
    assert!(ProjectStore::migrate(&path)?.backup.is_none());
    Ok(())
}

#[test]
fn migration_promotes_with_a_live_wal_reader_without_replacing_its_snapshot() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture(scratch.path())?;
    let reader = Connection::open(path.join("project.sqlite"))?;
    reader.pragma_update(None, "journal_mode", "WAL")?;
    reader.execute_batch("BEGIN")?;
    let before = contents(&reader)?;
    ProjectStore::migrate(&path)?;
    assert_eq!(
        contents(&reader)?,
        before,
        "existing read transaction retains schema 1"
    );
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    reader.execute_batch("COMMIT")?;
    assert_eq!(
        reader.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        2
    );
    Ok(())
}

#[test]
fn corruption_is_rejected_without_promoting_any_migrated_rows() -> Result {
    for corruption in [
        "UPDATE revisions SET document=json_set(document,'$.nodes.repeat.kind.plays',7) WHERE id='v1-grow'",
        "UPDATE history SET edit=json_set(edit,'$.duration_delta',999) WHERE revision_id='v1-wrap'",
        "UPDATE history SET request=json_set(request,'$.command.command','move_plays') WHERE revision_id='v1-wrap'",
        "UPDATE history SET request=replace(request,'\"plays\":3','\"plays\":3,\"plays\":3') WHERE revision_id='v1-wrap'",
        "UPDATE revisions SET document=json_set(document,'$.schema_version',2) WHERE id='v1-insert'",
        "DELETE FROM redo WHERE position=2",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture(scratch.path())?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(corruption)?;
        let before = contents(&database)?;
        let error = ProjectStore::migrate(&path).unwrap_err();
        assert_eq!(error.code(), "MigrationFailed", "{corruption}");
        let StoreError::MigrationFailed { backup, .. } = error else {
            panic!("backup path is required")
        };
        assert!(backup.is_file());
        assert_eq!(contents(&database)?, before, "{corruption}");
        assert!(fs::read_dir(path.join("Snapshots"))?.any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("before-schema-2-")
        }));
    }
    Ok(())
}

#[test]
fn writer_and_database_locks_prevent_migration_without_damage() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture(scratch.path())?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
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
    assert!(matches!(error, StoreError::MigrationFailed { .. }));
    assert_eq!(contents(&database)?, before);
    database.execute_batch("ROLLBACK")?;
    ProjectStore::migrate(&path)?;
    Ok(())
}
