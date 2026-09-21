//! Streaming semantic validation. Documents are bounded before SQLite returns
//! their text; historical documents are replayed one at a time, not accumulated.

use deadpan_core::{
    CommandRequest, EditTransaction, MAX_IDENTITY_BYTES, ProjectDocument, RevisionId,
};
use rusqlite::{Connection, params};

use crate::{StoreError, history};

pub(crate) struct RevisionRecord {
    pub parent: Option<String>,
    pub kind: String,
    pub document: ProjectDocument,
}

pub(crate) struct HistoryRecord {
    pub parent: Option<i64>,
    pub revision: String,
    pub request: CommandRequest,
    pub edit: EditTransaction,
}

pub(crate) fn check_stored_sizes(connection: &Connection, limit: usize) -> Result<(), StoreError> {
    // Check byte length, not Unicode scalar count. Do this before quick_check,
    // whose JSON CHECK constraints would otherwise parse oversized values.
    let oversized: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM revisions WHERE typeof(document) != 'text' OR length(CAST(document AS BLOB)) > ?1
            OR typeof(id) != 'text' OR length(CAST(id AS BLOB)) NOT BETWEEN 1 AND ?2
            OR (parent_id IS NOT NULL AND (typeof(parent_id) != 'text' OR length(CAST(parent_id AS BLOB)) NOT BETWEEN 1 AND ?2))
            OR typeof(kind) != 'text' OR length(CAST(kind AS BLOB)) NOT BETWEEN 1 AND 7 OR kind NOT IN ('initial','edit','undo','redo'))
            OR EXISTS(SELECT 1 FROM history WHERE typeof(request) != 'text' OR typeof(edit) != 'text'
            OR length(CAST(request AS BLOB)) > ?1 OR length(CAST(edit AS BLOB)) > ?1
            OR typeof(revision_id) != 'text' OR length(CAST(revision_id AS BLOB)) NOT BETWEEN 1 AND ?2)
            OR EXISTS(SELECT 1 FROM state WHERE typeof(head_revision) != 'text' OR length(CAST(head_revision AS BLOB)) NOT BETWEEN 1 AND ?2)",
        params![limit as i64, MAX_IDENTITY_BYTES as i64], |row| row.get(0),
    )?;
    if oversized {
        return Err(StoreError::Integrity(
            "stored JSON or identity metadata exceeds its bound or has an invalid type".into(),
        ));
    }
    Ok(())
}

fn checked_id(value: Option<String>) -> Result<String, StoreError> {
    let value = value.ok_or_else(|| {
        StoreError::Integrity("invalid or oversized stored revision identity".into())
    })?;
    RevisionId::new(value.clone())?;
    Ok(value)
}

pub(crate) fn read_head(connection: &Connection) -> Result<String, StoreError> {
    let value = connection.query_row(
        "SELECT CASE WHEN typeof(head_revision)='text' AND length(CAST(head_revision AS BLOB)) BETWEEN 1 AND ?1 THEN head_revision END FROM state WHERE singleton=1",
        [MAX_IDENTITY_BYTES as i64], |row| row.get(0),
    )?;
    checked_id(value)
}

pub(crate) fn read_revision(
    connection: &Connection,
    id: &str,
) -> Result<RevisionRecord, StoreError> {
    read_revision_bounded(connection, id, crate::schema::MAX_DOCUMENT_BYTES)
}

fn read_revision_bounded(
    connection: &Connection,
    id: &str,
    limit: usize,
) -> Result<RevisionRecord, StoreError> {
    let (parent, kind, json): (Option<String>, Option<String>, Option<String>) = connection.query_row(
        "SELECT CASE WHEN parent_id IS NULL THEN '' WHEN typeof(parent_id)='text' AND length(CAST(parent_id AS BLOB)) BETWEEN 1 AND ?3 THEN parent_id END,
         CASE WHEN typeof(kind)='text' AND length(CAST(kind AS BLOB)) BETWEEN 1 AND 7 AND kind IN ('initial','edit','undo','redo') THEN kind END,
         CASE WHEN typeof(document)='text' AND length(CAST(document AS BLOB)) <= ?2 THEN document END FROM revisions WHERE id=?1",
        params![id, limit as i64, MAX_IDENTITY_BYTES as i64],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let parent = if parent.as_deref() == Some("") {
        None
    } else {
        Some(checked_id(parent)?)
    };
    let kind =
        kind.ok_or_else(|| StoreError::Integrity("invalid or oversized revision kind".into()))?;
    let json = json.ok_or_else(|| {
        StoreError::Integrity("stored document exceeds the 64 MiB limit or is not text".into())
    })?;
    let document = ProjectDocument::from_json(&json)?;
    if document.revision_id().as_str() != id {
        return Err(StoreError::Integrity(
            "revision identity disagrees with document".into(),
        ));
    }
    Ok(RevisionRecord {
        parent,
        kind,
        document,
    })
}

pub(crate) fn read_history(connection: &Connection, id: i64) -> Result<HistoryRecord, StoreError> {
    read_history_bounded(connection, id, crate::schema::MAX_DOCUMENT_BYTES)
}

fn read_history_bounded(
    connection: &Connection,
    id: i64,
    limit: usize,
) -> Result<HistoryRecord, StoreError> {
    let (parent, revision, request, edit): (Option<i64>, Option<String>, Option<String>, Option<String>) =
        connection.query_row(
            "SELECT parent_id,CASE WHEN typeof(revision_id)='text' AND length(CAST(revision_id AS BLOB)) BETWEEN 1 AND ?3 THEN revision_id END,
         CASE WHEN typeof(request)='text' AND length(CAST(request AS BLOB)) <= ?2 THEN request END,
         CASE WHEN typeof(edit)='text' AND length(CAST(edit AS BLOB)) <= ?2 THEN edit END
         FROM history WHERE id=?1",
            params![id, limit as i64, MAX_IDENTITY_BYTES as i64],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    let oversized = || {
        StoreError::Integrity("stored history JSON exceeds the 64 MiB limit or is not text".into())
    };
    let request = request.ok_or_else(oversized)?;
    let edit = edit.ok_or_else(oversized)?;
    Ok(HistoryRecord {
        parent,
        revision: checked_id(revision)?,
        request: serde_json::from_str(&request)?,
        edit: serde_json::from_str(&edit)?,
    })
}

fn history_error(message: &str) -> StoreError {
    StoreError::History(message.into())
}

pub(crate) fn validate_history(connection: &Connection) -> Result<(), StoreError> {
    let count: i64 =
        connection.query_row("SELECT COUNT(*) FROM revisions", [], |row| row.get(0))?;
    let mut roots = connection.prepare("SELECT CASE WHEN typeof(id)='text' AND length(CAST(id AS BLOB)) BETWEEN 1 AND ?1 THEN id END FROM revisions WHERE parent_id IS NULL LIMIT 2")?;
    let mut rows = roots.query([MAX_IDENTITY_BYTES as i64])?;
    let initial = checked_id(
        rows.next()?
            .ok_or_else(|| history_error("missing initial revision"))?
            .get(0)?,
    )?;
    if rows.next()?.is_some() {
        return Err(history_error("multiple initial revisions"));
    }
    let first = read_revision(connection, &initial)?;
    if first.kind != "initial" {
        return Err(history_error("root revision is not initial"));
    }
    let mut current = first.document;
    let mut cursor = None;
    // Only numeric history identifiers are retained. Memory is independent of
    // total historical document size; the stack grows only with undone edits.
    let mut redo = Vec::<i64>::new();
    let mut visited = 1_i64;
    let mut edits = 0_i64;
    loop {
        let mut children = connection.prepare("SELECT CASE WHEN typeof(id)='text' AND length(CAST(id AS BLOB)) BETWEEN 1 AND ?2 THEN id END FROM revisions WHERE parent_id=?1 LIMIT 2")?;
        let mut rows = children.query(params![
            current.revision_id().as_str(),
            MAX_IDENTITY_BYTES as i64
        ])?;
        let Some(row) = rows.next()? else { break };
        let id = checked_id(row.get(0)?)?;
        if rows.next()?.is_some() {
            return Err(history_error("revision chronology forks"));
        }
        let next = read_revision(connection, &id)?;
        if next.parent.as_deref() != Some(current.revision_id().as_str()) {
            return Err(history_error("revision parent disagrees"));
        }
        match next.kind.as_str() {
            "edit" => {
                let mut entries =
                    connection.prepare("SELECT id FROM history WHERE revision_id=?1 LIMIT 2")?;
                let mut entries = entries.query([id.as_str()])?;
                let entry: i64 = entries
                    .next()?
                    .ok_or_else(|| history_error("edit has no history entry"))?
                    .get(0)?;
                if entries.next()?.is_some() {
                    return Err(history_error("edit has multiple history entries"));
                }
                let record = read_history(connection, entry)?;
                if record.parent != cursor || record.revision != id {
                    return Err(history_error("history parent or revision disagrees"));
                }
                let calculated = deadpan_core::apply(&current, &record.request)?;
                if calculated != record.edit || calculated.forward.apply(&current)? != next.document
                {
                    return Err(history_error(
                        "stored command, patches, and revision disagree",
                    ));
                }
                if calculated.inverse.apply(&next.document)? != current {
                    return Err(history_error(
                        "inverse does not restore the preceding revision",
                    ));
                }
                cursor = Some(entry);
                redo.clear();
                edits += 1;
            }
            kind @ ("undo" | "redo") => {
                let is_redo = kind == "redo";
                let entry = if is_redo { redo.pop() } else { cursor }
                    .ok_or_else(|| history_error("history navigation has no target"))?;
                let plan = history::build_navigation(
                    connection,
                    current,
                    next.document.revision_id().clone(),
                    is_redo,
                    cursor,
                    entry,
                )?;
                if plan.next != next.document {
                    return Err(history_error(
                        "history navigation disagrees with its revision",
                    ));
                }
                cursor = plan.next_cursor;
                if !is_redo {
                    redo.push(entry);
                }
            }
            _ => return Err(history_error("noninitial revision has an invalid kind")),
        }
        current = next.document;
        visited += 1;
        if visited > count {
            return Err(history_error("revision chronology contains a cycle"));
        }
    }
    if visited != count {
        return Err(history_error(
            "revision chronology contains unreachable revisions",
        ));
    }
    let history_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))?;
    if history_count != edits {
        return Err(history_error(
            "history contains entries without corresponding edits",
        ));
    }
    let head = read_head(connection)?;
    let mut state = connection.prepare("SELECT singleton,cursor FROM state")?;
    let mut state = state.query([])?;
    let row = state
        .next()?
        .ok_or_else(|| history_error("missing current state"))?;
    let singleton: i64 = row.get(0)?;
    let saved_cursor: Option<i64> = row.get(1)?;
    if singleton != 1
        || head != current.revision_id().as_str()
        || saved_cursor != cursor
        || state.next()?.is_some()
    {
        return Err(history_error(
            "current state disagrees with revision history",
        ));
    }
    let mut saved_redo =
        connection.prepare("SELECT position,history_id FROM redo ORDER BY position")?;
    let mut rows = saved_redo.query([])?;
    for (index, entry) in redo.iter().enumerate() {
        let row = rows
            .next()?
            .ok_or_else(|| history_error("redo stack is incomplete"))?;
        if row.get::<_, i64>(0)? != index as i64 + 1 || row.get::<_, i64>(1)? != *entry {
            return Err(history_error("redo stack disagrees with revision history"));
        }
    }
    if rows.next()?.is_some() {
        return Err(history_error("redo stack contains extra entries"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_identity_metadata_is_bounded_before_text_extraction()
    -> Result<(), Box<dyn std::error::Error>> {
        for oversized in [
            "x".repeat(MAX_IDENTITY_BYTES + 1),
            "é".repeat(MAX_IDENTITY_BYTES / 2 + 1),
            format!("initial{}", " ".repeat(MAX_IDENTITY_BYTES)),
        ] {
            for (table, column) in [
                ("revisions", "id"),
                ("revisions", "parent_id"),
                ("revisions", "kind"),
                ("history", "revision_id"),
                ("state", "head_revision"),
            ] {
                let mut db = Connection::open_in_memory()?;
                crate::schema::configure(&db)?;
                crate::schema::create(&mut db)?;
                db.execute_batch(
                    "INSERT INTO revisions(id,kind,document) VALUES ('r','initial','{}');
                    INSERT INTO history(id,revision_id,request,edit) VALUES (1,'r','{}','{}');
                    INSERT INTO state(singleton,head_revision,cursor) VALUES (1,'r',NULL);
                    PRAGMA foreign_keys=OFF; PRAGMA ignore_check_constraints=ON;",
                )?;
                if column == "kind" {
                    // A hostile schema can use RTRIM so a long whitespace
                    // suffix compares equal to 'initial'. Exact byte limits
                    // must hold independently of SQL comparison semantics.
                    db.execute_batch("ALTER TABLE revisions RENAME TO prior_revisions;
                        CREATE TABLE revisions(id TEXT, parent_id TEXT, kind TEXT COLLATE RTRIM, document TEXT) STRICT;
                        INSERT INTO revisions SELECT id,parent_id,kind,document FROM prior_revisions;")?;
                }
                db.execute(&format!("UPDATE {table} SET {column}=?1"), [&oversized])?;
                assert!(
                    matches!(
                        check_stored_sizes(&db, crate::schema::MAX_DOCUMENT_BYTES),
                        Err(StoreError::Integrity(_))
                    ),
                    "{table}.{column}"
                );
                // These read paths must reject metadata before parsing the
                // deliberately invalid document/request JSON above. Testing the
                // reads directly also covers tampering after a store was opened.
                let error = match (table, column) {
                    ("revisions", "id") => validate_history(&db).err(),
                    ("revisions", _) => read_revision(&db, "r").err(),
                    ("history", _) => read_history(&db, 1).err(),
                    _ => crate::read_snapshot(&db).err(),
                };
                assert!(
                    matches!(error, Some(StoreError::Integrity(_))),
                    "{table}.{column}: {error:?}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn stored_json_is_bounded_in_bytes_before_deserialization()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut db = Connection::open_in_memory()?;
        crate::schema::configure(&db)?;
        crate::schema::create(&mut db)?;
        db.execute(
            "INSERT INTO revisions(id,kind,document) VALUES ('r','initial','{}')",
            [],
        )?;
        db.execute(
            "INSERT INTO history(id,revision_id,request,edit) VALUES (1,'r','{}','{}')",
            [],
        )?;
        // Exercise the exact production predicates at a small limit. The JSON
        // contains fewer than 16 characters but more than 16 UTF-8 bytes.
        let oversized = serde_json::to_string(&"é".repeat(10))?;
        assert!(oversized.chars().count() <= 16 && oversized.len() > 16);
        for column in ["document", "request", "edit"] {
            let table = if column == "document" {
                "revisions"
            } else {
                "history"
            };
            db.execute(&format!("UPDATE {table} SET {column}=?1"), [&oversized])?;
            assert!(matches!(
                check_stored_sizes(&db, 16),
                Err(StoreError::Integrity(_))
            ));
            let error = if column == "document" {
                read_revision_bounded(&db, "r", 16).err()
            } else {
                read_history_bounded(&db, 1, 16).err()
            };
            assert!(
                matches!(error, Some(StoreError::Integrity(_))),
                "{column}: {error:?}"
            );
            db.execute(&format!("UPDATE {table} SET {column}='{{}}'"), [])?;
        }
        check_stored_sizes(&db, 16)?;
        Ok(())
    }
}
