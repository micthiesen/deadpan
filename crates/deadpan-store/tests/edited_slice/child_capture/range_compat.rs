//! Literal range command shapes survive the deliberate development-format break.
use super::*;
use deadpan_store::StoreError;
use serde_json::Value;

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap()
}

fn literal() -> Result<Value> {
    let literal: Value =
        serde_json::from_str(include_str!("../../fixtures/edited_slice/range-v1.json"))?;
    assert_eq!(
        literal["base_commit"],
        "2ea675646abc24e8410616af74b4193ed4af51da"
    );
    assert_eq!(literal["document_schema"], 34);
    assert_eq!(literal["database_schema"], 43);
    Ok(literal)
}

// TEST ONLY: this permits command-shape assertions on current documents by
// changing exactly their schema header. It is not a project-format migration,
// and neither production readers nor the literal historical rows use it.
fn current_test_document(old_json: &str) -> Result<ProjectDocument> {
    let mut value: Value = serde_json::from_str(old_json)?;
    assert_eq!(value["schema_version"], 34);
    value["schema_version"] = serde_json::to_value(DOCUMENT_SCHEMA_VERSION)?;
    Ok(ProjectDocument::from_json(&value.to_string())?)
}

// Adapt only the expected current writer value. The literal historical
// capture and database rows remain unchanged and still decode directly.

// Restore literal old rows into a fresh package shell. No request is applied to
// generate expected history; the old writer's JSON strings enter SQLite intact.
fn restore_rows(path: &Path, original: &ProjectDocument, tables: &Value) -> Result {
    drop(ProjectStore::create(path, original)?);
    let mut database = Connection::open(path.join("project.sqlite"))?;
    let transaction = database.transaction()?;
    transaction.execute("DELETE FROM state", [])?;
    transaction.execute("DELETE FROM revisions", [])?;
    for (table, sql) in [
        (
            "revisions",
            "INSERT INTO revisions SELECT json_extract(?1,'$[0]'),json_extract(?1,'$[1]'),json_extract(?1,'$[2]'),json_extract(?1,'$[3]'),0,length(CAST(json_extract(?1,'$[3]') AS BLOB))",
        ),
        (
            "history",
            "INSERT INTO history SELECT json_extract(?1,'$[0]'),json_extract(?1,'$[1]'),json_extract(?1,'$[2]'),json_extract(?1,'$[3]'),json_extract(?1,'$[4]')",
        ),
        (
            "state",
            "INSERT INTO state SELECT json_extract(?1,'$[0]'),json_extract(?1,'$[1]'),json_extract(?1,'$[2]'),json_extract(?1,'$[3]')",
        ),
        (
            "redo",
            "INSERT INTO redo SELECT json_extract(?1,'$[0]'),json_extract(?1,'$[1]')",
        ),
    ] {
        for row in tables[table].as_array().unwrap() {
            transaction.execute(sql, [row.as_str().unwrap()])?;
        }
    }
    transaction.pragma_update(None, "user_version", 43)?;
    transaction.commit()?;
    Ok(())
}

#[test]
fn literal_core_34_and_database_43_are_rejected_without_rewriting_authored_rows() -> Result {
    let literal = literal()?;
    assert_eq!(
        ProjectDocument::from_json(text(&literal, "original"))
            .unwrap_err()
            .code,
        DocumentErrorCode::UnsupportedSchema
    );
    let original = current_test_document(text(&literal, "original"))?;
    for case in literal["cases"].as_array().unwrap() {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("old-history.deadpan");
        restore_rows(&path, &original, &case["tables"])?;
        let saved = history_rows(&path)?;
        let old_rows: Vec<_> = case["tables"]["history"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(saved, old_rows);
        let all_cells = authored(&path)?;
        let snapshots = std::fs::read_dir(path.join("Snapshots"))?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
        for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
            assert!(matches!(
                ProjectStore::open(&path, mode),
                Err(StoreError::UnsupportedSchema(43))
            ));
            assert_eq!(authored(&path)?, all_cells);
        }
        assert!(matches!(
            ProjectStore::migrate(&path),
            Err(StoreError::UnsupportedSchema(43))
        ));
        assert_eq!(authored(&path)?, all_cells);
        assert_eq!(
            std::fs::read_dir(path.join("Snapshots"))?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?,
            snapshots,
            "rejection must not create a migration backup"
        );
        let database = Connection::open(path.join("project.sqlite"))?;
        assert_eq!(
            database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            43
        );
        for row in case["tables"]["revisions"].as_array().unwrap() {
            let row: Value = serde_json::from_str(row.as_str().unwrap())?;
            let current: String = database.query_row(
                "SELECT document FROM revisions WHERE id=?1",
                [row[0].as_str().unwrap()],
                |row| row.get(0),
            )?;
            assert_eq!(current, row[3].as_str().unwrap());
        }
    }
    Ok(())
}
