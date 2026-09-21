use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use deadpan_core::{
    IterationOrder, NodeId, NodeKind, ProjectDocument, RevisionId, legacy_v1, legacy_v2, legacy_v3,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn fixture(scratch: &Path) -> Result<PathBuf> {
    fixture_version(scratch, 1)
}
fn fixture_version(scratch: &Path, version: u32) -> Result<PathBuf> {
    let package = scratch.join("legacy.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let connection = Connection::open(package.join("project.sqlite"))?;
    connection.pragma_update(None, "foreign_keys", false)?;
    connection.execute_batch(match version {
        1 => include_str!("fixtures/v1-history.sql"),
        2 => include_str!("fixtures/v2-history.sql"),
        3 => include_str!("fixtures/v3-history.sql"),
        4 => include_str!("fixtures/v4-history.sql"),
        _ => panic!("unsupported fixture"),
    })?;
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
fn history_json(connection: &Connection) -> Result<Vec<(String, String)>> {
    Ok(connection
        .prepare("SELECT request,edit FROM history ORDER BY id")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?)
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
fn corrupt_old_tables_and_oversized_rows_retain_backup_before_validation() -> Result {
    let oversized_bytes = i64::try_from(deadpan_core::MAX_DOCUMENT_JSON_BYTES)? + 1;
    for version in [1, 2, 3, 4] {
        for missing_table in [true, false] {
            let scratch = tempfile::tempdir()?;
            let path = fixture_version(scratch.path(), version)?;
            let database_path = path.join("project.sqlite");
            let database = Connection::open(&database_path)?;
            database.pragma_update(None, "foreign_keys", false)?;
            if missing_table {
                database.execute_batch("DROP TABLE history")?;
            } else {
                database.pragma_update(None, "ignore_check_constraints", true)?;
                database.execute(
                    "UPDATE revisions SET document=CAST(zeroblob(?1) AS TEXT) WHERE kind='initial'",
                    [oversized_bytes],
                )?;
            }
            let original_revisions: i64 =
                database.query_row("SELECT COUNT(*) FROM revisions", [], |row| row.get(0))?;
            drop(database);
            let before = fs::read(&database_path)?;
            let failure = ProjectStore::migrate(&path).unwrap_err();
            let StoreError::MigrationFailed { backup, source } = failure else {
                panic!("expected migration failure with retained backup");
            };
            if !missing_table {
                assert!(matches!(*source, StoreError::Integrity(_)));
            }
            assert_eq!(fs::read(&database_path)?, before);
            assert!(backup.is_file());
            let backup = Connection::open(backup)?;
            assert_eq!(
                backup.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
                version
            );
            assert_eq!(
                backup.query_row("SELECT COUNT(*) FROM revisions", [], |row| row
                    .get::<_, i64>(0))?,
                original_revisions
            );
            if missing_table {
                assert!(!backup.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='history')",
                    [],
                    |row| row.get::<_, bool>(0),
                )?);
            } else {
                assert!(backup.query_row(
                    "SELECT document=CAST(zeroblob(?1) AS TEXT) FROM revisions WHERE kind='initial'",
                    [oversized_bytes],
                    |row| row.get::<_, bool>(0),
                )?);
            }
        }
    }
    Ok(())
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
    assert_eq!((migration.from_schema, migration.to_schema), (1, 5));
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
        5
    );
    Ok(())
}

#[test]
fn schema_four_adds_operational_tables_without_rewriting_authored_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 4)?;
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&path, mode),
            Err(StoreError::MigrationRequired(4))
        ));
    }
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let original_docs = docs(&database)?;
    let original_history = history_json(&database)?;
    let original_metadata = metadata(&database)?;
    assert_eq!(original_docs.len(), 11);
    assert_eq!(original_history.len(), 6);
    let outcome = ProjectStore::migrate(&path)?;
    assert_eq!((outcome.from_schema, outcome.to_schema), (4, 5));
    assert_eq!(
        contents(&Connection::open(outcome.backup.unwrap())?)?,
        before
    );
    assert_eq!(docs(&database)?, original_docs);
    assert_eq!(history_json(&database)?, original_history);
    assert_eq!(metadata(&database)?, original_metadata);
    assert_eq!(
        database.query_row(
            "SELECT (SELECT COUNT(*) FROM hold_request_clocks) + (SELECT COUNT(*) FROM generation_requests)",
            [],
            |row| row.get::<_, i64>(0),
        )?,
        0
    );
    let shrunk = snapshot(&database, "v4-shrink")?;
    drop(database);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let restored = store.snapshot()?;
    assert_eq!(restored.overrides().len(), 1);
    assert_eq!(restored.marks().len(), 1);
    store.redo(restored.revision_id(), RevisionId::new("v5-redo")?)?;
    let redone = store.snapshot()?;
    assert_eq!(redone.nodes(), shrunk.nodes());
    assert_eq!(redone.overrides(), shrunk.overrides());
    assert_eq!(redone.marks(), shrunk.marks());
    store.undo(redone.revision_id(), RevisionId::new("v5-undo")?)?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), restored.nodes());
    assert_eq!(undone.overrides(), restored.overrides());
    assert_eq!(undone.marks(), restored.marks());
    store.validate()?;
    drop(store);
    assert!(ProjectStore::migrate(&path)?.backup.is_none());
    Ok(())
}

#[test]
fn schema_four_rejects_corruption_and_operational_table_collisions_before_promotion() -> Result {
    for corruption in [
        "UPDATE history SET edit=json_set(edit,'$.inverse.overrides',json('{}')) WHERE revision_id='v4-clear'",
        "UPDATE revisions SET document=json_set(document,'$.schema_version',5) WHERE kind='initial'",
        "UPDATE revisions SET document=json_set(document,'$.generation_requests',json('{}')) WHERE kind='initial'",
        "CREATE TABLE generation_requests(preserved TEXT); INSERT INTO generation_requests VALUES ('existing')",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 4)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(corruption)?;
        let before = contents(&database)?;
        let failure = ProjectStore::migrate(&path).unwrap_err();
        assert_eq!(failure.code(), "MigrationFailed", "{corruption}");
        let StoreError::MigrationFailed { backup, .. } = failure else {
            panic!("migration must retain its backup")
        };
        assert_eq!(contents(&Connection::open(backup)?)?, before);
        assert_eq!(contents(&database)?, before);
        if corruption.starts_with("CREATE") {
            assert_eq!(
                database.query_row("SELECT preserved FROM generation_requests", [], |row| {
                    row.get::<_, String>(0)
                })?,
                "existing"
            );
        }
    }
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
                .starts_with("before-schema-5-")
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

#[test]
fn schema_two_history_preserves_compact_identities_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 2)?;
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&path, mode),
            Err(StoreError::MigrationRequired(2))
        ));
    }
    let database = Connection::open(path.join("project.sqlite"))?;
    let original = contents(&database)?;
    let original_metadata = metadata(&database)?;
    let old_docs = docs(&database)?;
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!((migration.from_schema, migration.to_schema), (2, 5));
    assert_eq!(
        contents(&Connection::open(migration.backup.unwrap())?)?,
        original
    );
    assert_eq!(metadata(&database)?, original_metadata);
    for (revision, json) in old_docs {
        let current = snapshot(&database, &revision)?;
        assert!(
            legacy_v2::Document::from_json(&json)?.matches(&current),
            "{revision}"
        );
        assert!(current.marks().is_empty());
        assert!(current.overrides().is_empty());
    }
    let wrapped = snapshot(&database, "v2-wrap")?;
    let inserted = snapshot(&database, "v2-insert-plays")?;
    let moved = snapshot(&database, "v2-move-plays")?;
    let grown = snapshot(&database, "v2-grow-abandoned")?;
    let branch = snapshot(&database, "v2-branch")?;
    for old in 0..3 {
        let identity = order(&wrapped).at(old).unwrap();
        assert!(order(&inserted).position(&identity).is_some());
        assert!(order(&moved).position(&identity).is_some());
    }
    assert_ne!(order(&grown).at(3), order(&branch).at(3));
    let history = database
        .prepare("SELECT request,edit FROM history ORDER BY id")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (request, edit) in history {
        let _: deadpan_core::CommandRequest = serde_json::from_str(&request)?;
        let edit: deadpan_core::EditTransaction = serde_json::from_str(&edit)?;
        assert!(edit.forward.marks.is_empty() && edit.inverse.marks.is_empty());
        assert!(edit.forward.overrides.is_empty() && edit.inverse.overrides.is_empty());
    }
    drop(database);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(order(&store.snapshot()?).len(), 3);
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("v3-redo-branch")?,
    )?;
    assert_eq!(order(&store.snapshot()?), order(&branch));
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("v3-redo-subtree")?,
    )?;
    let current = store.snapshot()?;
    let NodeKind::Repeat { iterations, .. } =
        &current.nodes()[&NodeId::new("inserted-repeat")?].kind
    else {
        panic!()
    };
    assert_eq!(iterations.at(0).unwrap().allocation.as_str(), "v2-subtree");
    store.undo(current.revision_id(), RevisionId::new("v3-undo")?)?;
    store.validate()?;
    drop(store);
    assert!(ProjectStore::migrate(&path)?.backup.is_none());
    Ok(())
}

#[test]
fn old_schemas_reject_new_mark_vocabulary_without_changing_original() -> Result {
    for version in [1, 2] {
        let id = if version == 1 { "v1-wrap" } else { "v2-wrap" };
        let statements = [
            "UPDATE revisions SET document=json_set(document,'$.marks',json('{}'))",
            "UPDATE history SET edit=json_set(edit,'$.forward.marks',json('{}'))",
            "UPDATE history SET edit=json_set(edit,'$.inverse.marks',json('{}'))",
            "UPDATE history SET request=json_set(request,'$.command.command','set_mark')",
        ];
        let mut corruptions = statements
            .iter()
            .map(|sql| (*sql).to_owned())
            .collect::<Vec<_>>();
        corruptions.push(format!("UPDATE history SET request=json_set(request,'$.command.anchor_policy','first') WHERE revision_id='{id}'"));
        corruptions.push(format!(
            "UPDATE revisions SET document=json_set(document,'$.schema_version',3) WHERE id='{id}'"
        ));
        for corruption in corruptions {
            let scratch = tempfile::tempdir()?;
            let path = fixture_version(scratch.path(), version)?;
            let database = Connection::open(path.join("project.sqlite"))?;
            database.execute_batch(&corruption)?;
            let before = contents(&database)?;
            let failure = ProjectStore::migrate(&path).unwrap_err();
            assert_eq!(
                failure.code(),
                "MigrationFailed",
                "schema {version}, {corruption}"
            );
            let StoreError::MigrationFailed { backup, .. } = failure else {
                panic!()
            };
            assert_eq!(contents(&Connection::open(backup)?)?, before);
            assert_eq!(contents(&database)?, before);
        }
    }
    Ok(())
}

#[test]
fn schema_two_duplicate_identities_are_rejected_before_promotion() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 2)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("UPDATE revisions SET document=json_set(document,'$.nodes.repeat.kind.iterations.runs',json('[{\"allocation\":\"v2-wrap\",\"first\":0,\"count\":2},{\"allocation\":\"v2-wrap\",\"first\":1,\"count\":1}]')) WHERE id='v2-wrap'")?;
    let before = contents(&database)?;
    let failure = ProjectStore::migrate(&path).unwrap_err();
    assert_eq!(failure.code(), "MigrationFailed");
    assert_eq!(contents(&database)?, before);
    let StoreError::MigrationFailed { backup, .. } = failure else {
        panic!()
    };
    assert_eq!(contents(&Connection::open(backup)?)?, before);
    Ok(())
}

#[test]
fn legacy_projection_cannot_hide_marks_added_by_a_faulty_replay() -> Result {
    use deadpan_core::{
        Anchor, AnchorLossPolicy, BoundaryAnchor, Command, CommandRequest, InsertionBias, MarkId,
        ProjectFrame,
    };
    for version in [1, 2] {
        let scratch = tempfile::tempdir()?;
        let package = fixture_version(scratch.path(), version)?;
        let database = Connection::open(package.join("project.sqlite"))?;
        let initial_json: String = database.query_row(
            "SELECT document FROM revisions WHERE kind='initial'",
            [],
            |row| row.get(0),
        )?;
        let base = if version == 1 {
            legacy_v1::Document::from_json(&initial_json)?.upgrade()?
        } else {
            legacy_v2::Document::from_json(&initial_json)?.upgrade()?
        };
        let (request_json, edit_json): (String, String) = database.query_row(
            "SELECT request,edit FROM history WHERE parent_id IS NULL",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let request = if version == 1 {
            legacy_v1::upgrade_request(&request_json)?
        } else {
            legacy_v2::upgrade_request(&request_json)?
        };
        let expected = deadpan_core::apply(&base, &request)?;
        let matches = |edit: &deadpan_core::EditTransaction| {
            if version == 1 {
                legacy_v1::matches_edit(&edit_json, edit)
            } else {
                legacy_v2::matches_edit(&edit_json, edit)
            }
        };
        assert!(matches(&expected)?);
        let mark_edit = deadpan_core::apply(
            &base,
            &CommandRequest {
                project_id: base.project_id().clone(),
                expected_revision: base.revision_id().clone(),
                new_revision: RevisionId::new("injected-mark")?,
                command: Command::SetMark {
                    id: MarkId::new("unexpected")?,
                    owner: base.root().clone(),
                    label: "Should not be in old history".into(),
                    boundary: BoundaryAnchor {
                        coordinate: Anchor::Sequence {
                            frame: ProjectFrame(0),
                        },
                        bias: InsertionBias::Right,
                    },
                    loss_policy: AnchorLossPolicy::KeepUnresolved,
                },
            },
        )?;
        let mut forged = expected.clone();
        forged.forward.marks = mark_edit.forward.marks.clone();
        assert!(!matches(&forged)?);
        forged = expected;
        forged.inverse.marks = mark_edit.inverse.marks.clone();
        assert!(!matches(&forged)?);
        let mut marked = serde_json::to_value(mark_edit.forward.apply(&base)?)?;
        marked["revision_id"] = serde_json::json!(base.revision_id());
        let marked = ProjectDocument::from_json(&marked.to_string())?;
        let matches_document = if version == 1 {
            legacy_v1::Document::from_json(&initial_json)?.matches(&marked)
        } else {
            legacy_v2::Document::from_json(&initial_json)?.matches(&marked)
        };
        assert!(!matches_document);
    }
    Ok(())
}

#[test]
fn schema_three_history_preserves_every_mark_and_pending_redo() -> Result {
    use deadpan_core::{Anchor, InsertionBias, MarkId, MarkLossReason, MarkState};
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 3)?;
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&path, mode),
            Err(StoreError::MigrationRequired(3))
        ));
    }
    let database = Connection::open(path.join("project.sqlite"))?;
    let original = contents(&database)?;
    let original_metadata = metadata(&database)?;
    let old_docs = docs(&database)?;
    let old_edits = database
        .prepare("SELECT id,edit FROM history ORDER BY id")?
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    assert_eq!(old_docs.len(), 23);
    assert_eq!(old_edits.len(), 17);
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!((migration.from_schema, migration.to_schema), (3, 5));
    let backup = migration.backup.unwrap();
    assert!(
        backup
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("before-schema-5-")
    );
    assert_eq!(contents(&Connection::open(backup)?)?, original);
    assert_eq!(metadata(&database)?, original_metadata);
    for (revision, json) in old_docs {
        let current = snapshot(&database, &revision)?;
        assert!(
            legacy_v3::Document::from_json(&json)?.matches(&current),
            "{revision}"
        );
        let old: serde_json::Value = serde_json::from_str(&json)?;
        assert_eq!(
            serde_json::to_value(current.marks())?,
            old["marks"],
            "{revision}"
        );
        assert!(current.overrides().is_empty());
    }
    for (id, old_json) in old_edits {
        let json: String =
            database.query_row("SELECT edit FROM history WHERE id=?1", [id], |row| {
                row.get(0)
            })?;
        let current: deadpan_core::EditTransaction = serde_json::from_str(&json)?;
        assert!(legacy_v3::matches_edit(&old_json, &current)?);
        assert!(current.forward.overrides.is_empty() && current.inverse.overrides.is_empty());
    }
    let wrapped = snapshot(&database, "v3-wrap")?;
    let shrunk = snapshot(&database, "v3-shrink")?;
    let grown = snapshot(&database, "v3-grow")?;
    let retired = order(&wrapped).at(2).unwrap();
    assert!(order(&shrunk).position(&retired).is_none());
    assert!(order(&grown).position(&retired).is_none());
    assert_eq!(order(&grown).at(2).unwrap().allocation.as_str(), "v3-grow");
    let lost = MarkState::Unresolved {
        reason: MarkLossReason::OccurrenceMissing,
    };
    assert_eq!(shrunk.marks()[&MarkId::new("occurrence")?].state, lost);
    assert_eq!(grown.marks()[&MarkId::new("occurrence")?].state, lost);
    let deleted = snapshot(&database, "v3-delete")?;
    assert_eq!(
        deleted.marks()[&MarkId::new("owned-keep")?].state,
        MarkState::Unresolved {
            reason: MarkLossReason::OwnerMissing
        }
    );
    assert!(!deleted.marks().contains_key(&MarkId::new("owned-delete")?));
    assert_eq!(
        deleted.marks()[&MarkId::new("left")?].boundary.bias,
        InsertionBias::Left
    );
    assert_eq!(
        deleted.marks()[&MarkId::new("right")?].boundary.bias,
        InsertionBias::Right
    );
    assert!(matches!(
        deleted.marks()[&MarkId::new("pinned")?].boundary.coordinate,
        Anchor::Sequence {
            frame: deadpan_core::ProjectFrame(1)
        }
    ));
    let branch = snapshot(&database, "v3-branch")?;
    let mark_deleted = snapshot(&database, "v3-delete-mark")?;
    drop(database);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?.marks(), deleted.marks());
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("v4-redo-branch")?,
    )?;
    assert_eq!(store.snapshot()?.marks(), branch.marks());
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("v4-redo-delete-mark")?,
    )?;
    assert_eq!(store.snapshot()?.marks(), mark_deleted.marks());
    store.undo(
        store.snapshot()?.revision_id(),
        RevisionId::new("v4-undo-delete-mark")?,
    )?;
    assert_eq!(store.snapshot()?.marks(), branch.marks());
    store.validate()?;
    drop(store);
    assert!(ProjectStore::migrate(&path)?.backup.is_none());
    Ok(())
}

#[test]
fn every_old_schema_rejects_override_vocabulary_before_promotion() -> Result {
    for version in [1, 2, 3] {
        for corruption in [
            "UPDATE revisions SET document=json_set(document,'$.overrides',json('{}'))",
            "UPDATE history SET edit=json_set(edit,'$.forward.overrides',json('{}'))",
            "UPDATE history SET edit=json_set(edit,'$.inverse.overrides',json('{}'))",
            "UPDATE history SET request=json_set(request,'$.command.subtree.overrides',json('{}')) WHERE json_extract(request,'$.command.command')='insert'",
            "UPDATE history SET request=json_set(request,'$.command.command','set_play_override')",
            "UPDATE history SET request=json_set(request,'$.command.command','clear_play_override')",
            "UPDATE revisions SET document=json_set(document,'$.schema_version',4)",
        ] {
            let scratch = tempfile::tempdir()?;
            let path = fixture_version(scratch.path(), version)?;
            let database = Connection::open(path.join("project.sqlite"))?;
            database.execute_batch(corruption)?;
            let before = contents(&database)?;
            let failure = ProjectStore::migrate(&path).unwrap_err();
            let StoreError::MigrationFailed { backup, .. } = failure else {
                panic!("schema {version}: {corruption}");
            };
            assert_eq!(contents(&database)?, before);
            assert_eq!(contents(&Connection::open(backup)?)?, before);
        }
    }
    Ok(())
}

#[test]
fn legacy_projection_cannot_hide_empty_override_entries_from_faulty_replay() -> Result {
    use deadpan_core::{EditTransaction, PlayOverrides, ValueChange};
    for version in [1, 2, 3] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), version)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        let initial_json: String = database.query_row(
            "SELECT document FROM revisions WHERE kind='initial'",
            [],
            |row| row.get(0),
        )?;
        let base = match version {
            1 => legacy_v1::Document::from_json(&initial_json)?.upgrade()?,
            2 => legacy_v2::Document::from_json(&initial_json)?.upgrade()?,
            _ => legacy_v3::Document::from_json(&initial_json)?.upgrade()?,
        };
        let (request_json, edit_json): (String, String) = database.query_row(
            "SELECT request,edit FROM history WHERE parent_id IS NULL",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let request = match version {
            1 => legacy_v1::upgrade_request(&request_json)?,
            2 => legacy_v2::upgrade_request(&request_json)?,
            _ => legacy_v3::upgrade_request(&request_json)?,
        };
        let expected = deadpan_core::apply(&base, &request)?;
        let matches = |edit: &EditTransaction| match version {
            1 => legacy_v1::matches_edit(&edit_json, edit),
            2 => legacy_v2::matches_edit(&edit_json, edit),
            _ => legacy_v3::matches_edit(&edit_json, edit),
        };
        assert!(matches(&expected)?);
        for forward in [true, false] {
            let mut forged = expected.clone();
            let patch = if forward {
                &mut forged.forward
            } else {
                &mut forged.inverse
            };
            patch.overrides.insert(
                base.root().clone(),
                ValueChange {
                    before: None,
                    after: Some(PlayOverrides::default()),
                },
            );
            assert!(!matches(&forged)?);
        }
    }
    Ok(())
}

#[test]
fn schema_three_mark_corruption_retains_original_and_backup() -> Result {
    for corruption in [
        "UPDATE revisions SET document=json_set(document,'$.marks.left.boundary.bias','right') WHERE id='v3-mark-left'",
        "UPDATE history SET request=json_set(request,'$.command.loss_policy','delete_owned') WHERE revision_id='v3-mark-occurrence'",
        "UPDATE history SET edit=json_set(edit,'$.inverse.marks.left.after.label','tampered') WHERE revision_id='v3-delete-mark'",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 3)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(corruption)?;
        let before = contents(&database)?;
        let StoreError::MigrationFailed { backup, .. } = ProjectStore::migrate(&path).unwrap_err()
        else {
            panic!()
        };
        assert_eq!(contents(&database)?, before);
        assert_eq!(contents(&Connection::open(backup)?)?, before);
    }
    Ok(())
}
