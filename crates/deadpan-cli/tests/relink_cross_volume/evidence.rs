use super::support::{Result, Run, require};
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

/// Capture logical cells under one read transaction. WAL/checkpoint/recovery
/// bookkeeping may change when ordinary CLI owners open; authored rows may not.
pub fn capture(package: &Path) -> Result<Value> {
    let mut connection = Connection::open_with_flags(
        package.join("project.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let transaction = connection.transaction()?;
    let mut tables = BTreeMap::new();
    let mut total = 0_i64;
    for (table, select) in [
        (
            "revisions",
            "SELECT json_array(id,parent_id,kind,document,depth,json_bound) AS value FROM revisions ORDER BY id",
        ),
        (
            "revision_patches",
            "SELECT json_array(revision_id,patch) AS value FROM revision_patches ORDER BY revision_id",
        ),
        (
            "history",
            "SELECT json_array(id,parent_id,revision_id,request,edit) AS value FROM history ORDER BY id",
        ),
        (
            "state",
            "SELECT json_array(singleton,head_revision,cursor,workflow) AS value FROM state ORDER BY singleton",
        ),
        (
            "redo",
            "SELECT json_array(position,history_id) AS value FROM redo ORDER BY position",
        ),
        (
            "single_source",
            "SELECT json_array(singleton,profile,baseline_history) AS value FROM single_source ORDER BY singleton",
        ),
        (
            "source_qualifications",
            "SELECT json_array(id,original_content_id,original_ref,hex(snapshot)) AS value FROM source_qualifications ORDER BY id",
        ),
        (
            "transaction_steps",
            "SELECT json_array(owner_revision,ordinal,step_revision,document) AS value FROM transaction_steps ORDER BY owner_revision,ordinal",
        ),
        (
            "register_state",
            "SELECT json_array(singleton,version,bank_digest) AS value FROM register_state ORDER BY singleton",
        ),
        (
            "register_contents",
            "SELECT json_array(id,capture_revision,capture_step,value) AS value FROM register_contents ORDER BY id",
        ),
        (
            "registers",
            "SELECT json_array(name,content_id) AS value FROM registers ORDER BY name",
        ),
        (
            "original_media",
            "SELECT json_array(content_id,version,record) AS value FROM original_media ORDER BY content_id",
        ),
    ] {
        let (rows, bytes): (i64, i64) = transaction.query_row(
            &format!("SELECT COUNT(*),COALESCE(SUM(length(CAST(value AS BLOB))),0) FROM ({select} LIMIT 129)"),
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        total = total.checked_add(bytes).ok_or("capture byte overflow")?;
        require(
            rows <= 128 && bytes >= 0 && total <= 2 * 1024 * 1024,
            "fixture row capture exceeds bound",
        )?;
        let mut statement = transaction.prepare(select)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        tables.insert(table, rows);
    }
    let originals = tables
        .remove("original_media")
        .ok_or("Original rows missing")?;
    Ok(json!({"authored":tables,"original_rows":originals}))
}

pub fn unchanged(
    run: &Run,
    phase: &str,
    package: &Path,
    before: &Value,
    originals: bool,
) -> Result<Value> {
    let after = capture(package)?;
    run.save(&format!("{phase}-rows.json"), &after)?;
    let mut differences = Vec::new();
    if let Some(tables) = before["authored"].as_object() {
        for (name, value) in tables {
            if after["authored"][name] != *value {
                differences.push(name.clone());
            }
        }
    }
    if originals && before["original_rows"] != after["original_rows"] {
        differences.push("original_media".into());
    }
    require(
        differences.is_empty(),
        &format!("{phase} changed protected rows: {}", differences.join(", ")),
    )?;
    Ok(after)
}
