use super::*;
use deadpan_core::{CommandRequest, EditTransaction, legacy_v21, legacy_v22};
use serde_json::{Value, json};

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("gap-branch-history.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "foreign_keys", false)?;
    database.execute_batch(include_str!("../fixtures/v28-gap-branch-history.sql"))?;
    Ok(package)
}

fn binding_fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("gap-binding-history.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "foreign_keys", false)?;
    database.execute_batch(include_str!("../fixtures/v28-gap-binding-history.sql"))?;
    Ok(package)
}

#[test]
fn actual_schema28_binary_history_replays_through_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert_eq!(old_documents.len(), 11);
    assert_eq!(old_history.len(), 6);
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(28))
    ));
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (28, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(outcome.backup.unwrap())?)?,
        before
    );
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    for ((old_id, old), (new_id, new)) in old_documents.iter().zip(docs(&database)?) {
        assert_eq!(old_id, &new_id);
        let modern = ProjectDocument::from_json(&new)?;
        let legacy = legacy_v22::Document::from_json(old)?;
        assert!(legacy.matches(&modern));
        assert_eq!(legacy.upgrade()?, modern);
        assert!(modern.gap_overrides().is_empty());
        let mut expected: Value = serde_json::from_str(old)?;
        expected["schema_version"] = json!(deadpan_core::DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(modern)?, expected);
    }
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        assert_eq!(
            legacy_v22::upgrade_request(old_request)?,
            serde_json::from_str::<CommandRequest>(&new_request)?
        );
        assert!(legacy_v22::matches_edit(
            old_edit,
            &serde_json::from_str::<EditTransaction>(&new_edit)?
        )?);
    }
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let baseline = store.snapshot()?;
    store.redo(baseline.revision_id(), RevisionId::new("v28-pending-redo")?)?;
    let redone = store.snapshot()?;
    assert_eq!(
        redone.nodes()[redone.root()].label,
        "Core 22 authored rename"
    );
    store.undo(redone.revision_id(), RevisionId::new("v28-undo-redo")?)?;
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    store.validate()?;
    Ok(())
}

#[test]
fn schema28_rejects_gap_branch_fields_in_snapshot_patch_and_frozen_layout() -> Result {
    for location in [
        "initial",
        "later",
        "forward",
        "inverse",
        "frozen-initial",
        "frozen-forward",
        "frozen-inverse",
    ] {
        for value in [json!({}), Value::Null] {
            let scratch = tempfile::tempdir()?;
            let package = fixture(scratch.path())?;
            let database = Connection::open(package.join("project.sqlite"))?;
            let (table, column, key, identity, encoded) =
                if location == "initial" || location == "later" || location == "frozen-initial" {
                    let selector = if location == "later" {
                        "parent_id IS NOT NULL"
                    } else {
                        "parent_id IS NULL"
                    };
                    let (identity, encoded): (String, String) = database.query_row(
                        &format!("SELECT id,document FROM revisions WHERE {selector} LIMIT 1"),
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )?;
                    ("revisions", "document", "id", identity, encoded)
                } else {
                    let revision = if location.starts_with("frozen-") {
                        "append-reanchor"
                    } else {
                        "core22-gap-branch-rename"
                    };
                    let (identity, encoded): (String, String) = database.query_row(
                        "SELECT revision_id,edit FROM history WHERE revision_id=?1",
                        [revision],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )?;
                    ("history", "edit", "revision_id", identity, encoded)
                };
            let mut wire: Value = serde_json::from_str(&encoded)?;
            let target = if location == "initial" || location == "later" {
                &mut wire
            } else if location == "forward" || location == "inverse" {
                &mut wire[location]
            } else if location == "frozen-initial" {
                &mut wire["audio_bindings"]["timings"][0]["layout"]
            } else {
                let direction = location.strip_prefix("frozen-").unwrap();
                let side = if wire[direction]["audio_bindings"]["after"].is_object() {
                    "after"
                } else {
                    "before"
                };
                &mut wire[direction]["audio_bindings"][side]["timings"][0]["layout"]
            };
            target["gap_overrides"] = value.clone();
            database.execute(
                &format!("UPDATE {table} SET {column}=?1 WHERE {key}=?2"),
                [&wire.to_string(), &identity],
            )?;
            let before = contents(&database)?;
            let StoreError::MigrationFailed { backup, .. } =
                ProjectStore::migrate(&package).unwrap_err()
            else {
                panic!("{location} admitted gap_overrides={value}");
            };
            assert_eq!(contents(&database)?, before);
            assert_eq!(contents(&Connection::open(backup)?)?, before);
        }
    }
    Ok(())
}

#[test]
fn schema22_command_and_nested_subtree_grammar_stays_closed() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let existing = history_json(&database)?.into_iter().next().unwrap().0;
    let mut request: Value = serde_json::from_str(&existing)?;
    request["command"] = json!({
        "command": "insert",
        "parent": "parent",
        "index": 0,
        "subtree": {
            "root": "branch",
            "nodes": {"branch": {"label": "Branch", "kind": {"type": "sequence", "children": []}}},
            "overrides": {}
        }
    });
    assert!(legacy_v22::upgrade_request(&request.to_string()).is_ok());
    let subtree = request["command"]["subtree"].clone();
    for value in [json!({}), Value::Null] {
        request["command"]["subtree"]["gap_overrides"] = value;
        assert!(legacy_v22::upgrade_request(&request.to_string()).is_err());
    }
    let iteration = json!({"allocation": "allocation", "ordinal": 0});
    for command in [
        json!({
            "command": "set_gap_override", "node": "repeat",
            "iteration": iteration, "subtree": subtree,
        }),
        json!({
            "command": "clear_gap_override", "node": "repeat", "iteration": iteration,
        }),
        json!({
            "command": "isolate_gap", "node": "repeat", "iteration": iteration,
            "id": "isolated-gap", "timing": {"allocation": "new-revision", "ordinal": 0},
        }),
    ] {
        request["command"] = command.clone();
        // These are valid modern wires, so legacy rejection proves a closed
        // command vocabulary rather than rejection of a malformed identity.
        serde_json::from_value::<CommandRequest>(request.clone())?;
        assert!(legacy_v22::upgrade_request(&request.to_string()).is_err());
        let mut operation = command.as_object().unwrap().clone();
        let kind = operation.remove("command").unwrap();
        operation.remove("node");
        operation.insert("type".into(), kind);
        request["command"] = json!({
            "command": "edit_occurrence",
            "instance": {"node": "repeat", "repeats": []},
            "edit": operation,
            "identities": {"nodes": [], "marks": []},
        });
        serde_json::from_value::<CommandRequest>(request.clone())?;
        assert!(legacy_v22::upgrade_request(&request.to_string()).is_err());
    }
    Ok(())
}

#[test]
fn earlier_frozen_layout_adapter_rejects_explicit_gap_map() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let old = docs(&database)?.into_iter().next().unwrap().1;
    let mut wire: Value = serde_json::from_str(&old)?;
    wire["schema_version"] = json!(21);
    assert!(legacy_v21::Document::from_json(&wire.to_string()).is_ok());
    wire["audio_bindings"]["timings"][0]["layout"]["gap_overrides"] = json!({});
    assert!(legacy_v21::Document::from_json(&wire.to_string()).is_err());
    Ok(())
}

#[test]
fn actual_schema28_gap_binding_survives_replay_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = binding_fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (28, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(outcome.backup.unwrap())?)?,
        before
    );
    assert_eq!(old_documents.len(), 6);
    assert_eq!(old_history.len(), 2);
    for ((_, old), (_, new)) in old_documents.iter().zip(docs(&database)?) {
        let modern = ProjectDocument::from_json(&new)?;
        let legacy = legacy_v22::Document::from_json(old)?;
        assert!(legacy.matches(&modern));
        assert_eq!(modern.audio_bindings().gap_bindings().len(), 1);
        assert!(modern.gap_overrides().is_empty());
    }
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        assert_eq!(
            legacy_v22::upgrade_request(old_request)?,
            serde_json::from_str::<CommandRequest>(&new_request)?
        );
        assert!(legacy_v22::matches_edit(
            old_edit,
            &serde_json::from_str::<EditTransaction>(&new_edit)?
        )?);
    }
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let baseline = store.snapshot()?;
    store.redo(
        baseline.revision_id(),
        RevisionId::new("gap-binding-pending-redo")?,
    )?;
    let redone = store.snapshot()?;
    assert_eq!(
        redone.nodes()[redone.root()].label,
        "Retained core 22 gap binding"
    );
    assert_eq!(
        redone.audio_bindings().gap_bindings(),
        baseline.audio_bindings().gap_bindings()
    );
    store.undo(
        redone.revision_id(),
        RevisionId::new("gap-binding-undo-redo")?,
    )?;
    assert_eq!(
        store.snapshot()?.audio_bindings(),
        baseline.audio_bindings()
    );
    store.validate()?;
    Ok(())
}
