//! Literal range-only transactions generated before Child capture existed.
use super::*;
use serde_json::Value;

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap()
}

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
            "INSERT INTO revisions SELECT json_extract(?1,'$[0]'),json_extract(?1,'$[1]'),json_extract(?1,'$[2]'),json_extract(?1,'$[3]')",
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
    transaction.commit()?;
    Ok(())
}

#[test]
fn literal_pre_selector_range_envelopes_transactions_and_history_remain_unchanged() -> Result {
    let literal: Value =
        serde_json::from_str(include_str!("../../fixtures/edited_slice/range-v1.json"))?;
    assert_eq!(
        literal["base_commit"],
        "2ea675646abc24e8410616af74b4193ed4af51da"
    );
    assert_eq!(literal["document_schema"], 34);
    assert_eq!(literal["database_schema"], 43);
    let original = ProjectDocument::from_json(text(&literal, "original"))?;
    let slice = CapturedEditSlice::from_json(text(&literal, "capture"))?;
    assert_eq!(
        slice.selection(),
        &SliceCaptureSelection::Range {
            range: slice.range()
        }
    );
    assert_eq!(slice.to_json()?, text(&literal, "capture"));
    assert_eq!(capture(&original)?, slice);
    assert_eq!(capture(&original)?.to_json()?, text(&literal, "capture"));
    assert!(serde_json::to_value(&slice)?.get("selection").is_none());
    let removal: CommandRequest = serde_json::from_str(text(&literal, "removal_request"))?;
    let removal_edit: EditTransaction =
        serde_json::from_str(text(&literal, "removal_transaction"))?;
    assert_eq!(apply(&original, &removal)?, removal_edit);
    assert_eq!(literal["cases"].as_array().unwrap().len(), 3);
    for case in literal["cases"].as_array().unwrap() {
        let before = ProjectDocument::from_json(text(case, "before"))?;
        let after = ProjectDocument::from_json(text(case, "after"))?;
        let command: CommandRequest = serde_json::from_str(text(case, "request"))?;
        let expected: EditTransaction = serde_json::from_str(text(case, "transaction"))?;
        assert_eq!(removal_edit.forward.apply(&original)?, before);
        let embedded = match &command.command {
            Command::SpliceSlice { slice, .. } => {
                assert_eq!(case["kind"], "seam");
                slice
            }
            Command::SpliceSliceAt { slice, .. } => {
                assert_eq!(case["kind"], "interior");
                slice
            }
            Command::ReplaceSlice { slice, .. } => {
                assert_eq!(case["kind"], "replacement");
                slice
            }
            _ => panic!("literal must cover each placement envelope"),
        };
        assert_eq!(embedded, &slice);
        assert_eq!(serde_json::to_string(&command)?, text(case, "request"));
        let actual = apply(&before, &command)?;
        assert_eq!(
            actual, expected,
            "complete transaction for {}",
            case["kind"]
        );
        assert_eq!(serde_json::to_string(&actual)?, text(case, "transaction"));
        assert_eq!(
            actual.forward.apply(&before)?.to_json()?,
            text(case, "after")
        );
        assert_eq!(actual.inverse.apply(&after)?, before);

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
        let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
        reader.validate()?;
        assert_eq!(reader.snapshot()?, after);
        assert_eq!(reader.snapshot_at(slice.revision_id())?, original);
        assert_eq!(authored(&path)?, all_cells);
        drop(reader);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        store.validate()?;
        assert_eq!(
            authored(&path)?,
            all_cells,
            "opening current schema cannot rewrite old history"
        );
        store.undo(after.revision_id(), revision("compat-undo-placement"))?;
        assert_authored(&store.snapshot()?, &before)?;
        store.undo(
            &revision("compat-undo-placement"),
            revision("compat-undo-removal"),
        )?;
        assert_authored(&store.snapshot()?, &original)?;
        store.validate()?;
        assert_eq!(history_rows(&path)?, saved);
        drop(store);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        store.redo(
            &revision("compat-undo-removal"),
            revision("compat-redo-removal"),
        )?;
        assert_authored(&store.snapshot()?, &before)?;
        store.redo(
            &revision("compat-redo-removal"),
            revision("compat-redo-placement"),
        )?;
        assert_authored(&store.snapshot()?, &after)?;
        store.validate()?;
        assert_eq!(history_rows(&path)?, saved);
        let database = Connection::open(path.join("project.sqlite"))?;
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
