//! Streaming semantic validation. Documents are bounded before SQLite returns
//! their text; historical documents are replayed one at a time, not accumulated.

use deadpan_core::{
    CommandRequest, EditTransaction, MAX_IDENTITY_BYTES, ProjectDocument, RevisionId, legacy_v1,
    legacy_v2, legacy_v3, legacy_v4, legacy_v5, legacy_v6, legacy_v7,
};
use rusqlite::{Connection, params};

use crate::{StoreError, history};

pub(crate) struct RevisionRecord {
    pub document: ProjectDocument,
}

pub(crate) struct HistoryRecord {
    pub parent: Option<i64>,
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
    let (_, _, json) = read_revision_json(connection, id, limit)?;
    let document = ProjectDocument::from_json(&json)?;
    if document.revision_id().as_str() != id {
        return Err(StoreError::Integrity(
            "revision identity disagrees with document".into(),
        ));
    }
    Ok(RevisionRecord { document })
}

fn read_revision_json(
    connection: &Connection,
    id: &str,
    limit: usize,
) -> Result<(Option<String>, String, String), StoreError> {
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
    Ok((parent, kind, json))
}

pub(crate) fn read_history(connection: &Connection, id: i64) -> Result<HistoryRecord, StoreError> {
    read_history_bounded(connection, id, crate::schema::MAX_DOCUMENT_BYTES)
}

fn read_history_bounded(
    connection: &Connection,
    id: i64,
    limit: usize,
) -> Result<HistoryRecord, StoreError> {
    let (parent, revision, request, edit) = read_history_json(connection, id, limit)?;
    let request: CommandRequest = serde_json::from_str(&request)?;
    let edit: EditTransaction = serde_json::from_str(&edit)?;
    if request.new_revision.as_str() != revision
        || request.new_revision != edit.forward.to_revision
        || request.expected_revision != edit.forward.from_revision
        || request.project_id != edit.forward.project_id
    {
        return Err(history_error(
            "history request and patch identities disagree",
        ));
    }
    Ok(HistoryRecord { parent, edit })
}

fn read_history_json(
    connection: &Connection,
    id: i64,
    limit: usize,
) -> Result<(Option<i64>, String, String, String), StoreError> {
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
    Ok((parent, checked_id(revision)?, request, edit))
}

fn history_error(message: &str) -> StoreError {
    StoreError::History(message.into())
}

pub(crate) fn validate_history(connection: &Connection) -> Result<(), StoreError> {
    replay(connection, ReplaySchema::Current)
}

/// Called only on an isolated, backed-up migration candidate inside a transaction.
pub(crate) fn migrate_history(connection: &Connection, version: u32) -> Result<(), StoreError> {
    let schema = match version {
        1 => ReplaySchema::V1,
        2 => ReplaySchema::V2,
        3 => ReplaySchema::V3,
        4..=6 => ReplaySchema::V4,
        7..=10 => ReplaySchema::V5,
        11 => ReplaySchema::V6,
        12 => ReplaySchema::V7,
        _ => return Err(StoreError::UnsupportedSchema(version)),
    };
    replay(connection, schema)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReplaySchema {
    Current,
    V1,
    V2,
    V3,
    V4,
    V5,
    V6,
    V7,
}

enum StoredDocument {
    Current(ProjectDocument),
    V1(legacy_v1::Document),
    V2(legacy_v2::Document),
    V3(legacy_v3::Document),
    V4(legacy_v4::Document),
    V5(legacy_v5::Document),
    V6(legacy_v6::Document),
    V7(legacy_v7::Document),
}
impl StoredDocument {
    fn revision_id(&self) -> &RevisionId {
        match self {
            Self::Current(doc) => doc.revision_id(),
            Self::V1(doc) => doc.revision_id(),
            Self::V2(doc) => doc.revision_id(),
            Self::V3(doc) => doc.revision_id(),
            Self::V4(doc) => doc.revision_id(),
            Self::V5(doc) => doc.revision_id(),
            Self::V6(doc) => doc.revision_id(),
            Self::V7(doc) => doc.revision_id(),
        }
    }
    fn initial(self) -> Result<ProjectDocument, StoreError> {
        match self {
            Self::Current(doc) => Ok(doc),
            Self::V1(doc) => Ok(doc.upgrade()?),
            Self::V2(doc) => Ok(doc.upgrade()?),
            Self::V3(doc) => Ok(doc.upgrade()?),
            Self::V4(doc) => Ok(doc.upgrade()?),
            Self::V5(doc) => Ok(doc.upgrade()?),
            Self::V6(doc) => Ok(doc.upgrade()?),
            Self::V7(doc) => Ok(doc.upgrade()?),
        }
    }
    fn matches(&self, doc: &ProjectDocument) -> bool {
        match self {
            Self::Current(stored) => stored == doc,
            Self::V1(stored) => stored.matches(doc),
            Self::V2(stored) => stored.matches(doc),
            Self::V3(stored) => stored.matches(doc),
            Self::V4(stored) => stored.matches(doc),
            Self::V5(stored) => stored.matches(doc),
            Self::V6(stored) => stored.matches(doc),
            Self::V7(stored) => stored.matches(doc),
        }
    }
}
fn read_replay_revision(
    connection: &Connection,
    id: &str,
    schema: ReplaySchema,
) -> Result<(Option<String>, String, StoredDocument), StoreError> {
    let (parent, kind, json) =
        read_revision_json(connection, id, crate::schema::MAX_DOCUMENT_BYTES)?;
    let document = match schema {
        ReplaySchema::Current => StoredDocument::Current(ProjectDocument::from_json(&json)?),
        ReplaySchema::V1 => StoredDocument::V1(legacy_v1::Document::from_json(&json)?),
        ReplaySchema::V2 => StoredDocument::V2(legacy_v2::Document::from_json(&json)?),
        ReplaySchema::V3 => StoredDocument::V3(legacy_v3::Document::from_json(&json)?),
        ReplaySchema::V4 => StoredDocument::V4(legacy_v4::Document::from_json(&json)?),
        ReplaySchema::V5 => StoredDocument::V5(legacy_v5::Document::from_json(&json)?),
        ReplaySchema::V6 => StoredDocument::V6(legacy_v6::Document::from_json(&json)?),
        ReplaySchema::V7 => StoredDocument::V7(legacy_v7::Document::from_json(&json)?),
    };
    if document.revision_id().as_str() != id {
        return Err(history_error("revision identity disagrees with document"));
    }
    Ok((parent, kind, document))
}
fn write_migrated_revision(
    connection: &Connection,
    document: &ProjectDocument,
) -> Result<(), StoreError> {
    connection.execute(
        "UPDATE revisions SET document=?1 WHERE id=?2",
        params![document.to_json()?, document.revision_id().as_str()],
    )?;
    Ok(())
}

pub(crate) fn read_initial_id(connection: &Connection) -> Result<String, StoreError> {
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
    Ok(initial)
}

fn replay(connection: &Connection, schema: ReplaySchema) -> Result<(), StoreError> {
    let migrate = schema != ReplaySchema::Current;
    let count: i64 =
        connection.query_row("SELECT COUNT(*) FROM revisions", [], |row| row.get(0))?;
    let initial = read_initial_id(connection)?;
    let (_, kind, first) = read_replay_revision(connection, &initial, schema)?;
    if kind != "initial" {
        return Err(history_error("root revision is not initial"));
    }
    let mut current = first.initial()?;
    let initial_allocations: std::collections::BTreeSet<_> = current
        .nodes()
        .values()
        .filter_map(|node| match &node.kind {
            deadpan_core::NodeKind::Repeat { iterations, .. } => Some(iterations),
            _ => None,
        })
        .flat_map(|iterations| {
            iterations
                .segments()
                .map(|(allocation, _, _)| allocation.as_str().to_owned())
        })
        .collect();
    if migrate {
        write_migrated_revision(connection, &current)?;
    }
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
        if initial_allocations.contains(&id) {
            return Err(history_error(
                "revision reuses an initial occurrence allocation",
            ));
        }
        if rows.next()?.is_some() {
            return Err(history_error("revision chronology forks"));
        }
        let (parent, kind, next) = read_replay_revision(connection, &id, schema)?;
        if parent.as_deref() != Some(current.revision_id().as_str()) {
            return Err(history_error("revision parent disagrees"));
        }
        let next_document = match kind.as_str() {
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
                let (parent, revision, request_json, edit_json) =
                    read_history_json(connection, entry, crate::schema::MAX_DOCUMENT_BYTES)?;
                if parent != cursor || revision != id {
                    return Err(history_error("history parent or revision disagrees"));
                }
                let request = match schema {
                    ReplaySchema::Current => serde_json::from_str(&request_json)?,
                    ReplaySchema::V1 => legacy_v1::upgrade_request(&request_json)?,
                    ReplaySchema::V2 => legacy_v2::upgrade_request(&request_json)?,
                    ReplaySchema::V3 => legacy_v3::upgrade_request(&request_json)?,
                    ReplaySchema::V4 => legacy_v4::upgrade_request(&request_json)?,
                    ReplaySchema::V5 => legacy_v5::upgrade_request(&request_json)?,
                    ReplaySchema::V6 => legacy_v6::upgrade_request(&request_json)?,
                    ReplaySchema::V7 => legacy_v7::upgrade_request(&request_json)?,
                };
                let calculated = deadpan_core::apply(&current, &request)?;
                let matches_edit = match schema {
                    ReplaySchema::Current => {
                        calculated == serde_json::from_str::<EditTransaction>(&edit_json)?
                    }
                    ReplaySchema::V1 => legacy_v1::matches_edit(&edit_json, &calculated)?,
                    ReplaySchema::V2 => legacy_v2::matches_edit(&edit_json, &calculated)?,
                    ReplaySchema::V3 => legacy_v3::matches_edit(&edit_json, &calculated)?,
                    ReplaySchema::V4 => legacy_v4::matches_edit(&edit_json, &calculated)?,
                    ReplaySchema::V5 => legacy_v5::matches_edit(&edit_json, &calculated)?,
                    ReplaySchema::V6 => legacy_v6::matches_edit(&edit_json, &calculated)?,
                    ReplaySchema::V7 => legacy_v7::matches_edit(&edit_json, &calculated)?,
                };
                let next_document = calculated.forward.apply(&current)?;
                if !matches_edit || !next.matches(&next_document) {
                    return Err(history_error(
                        "stored command, patches, and revision disagree",
                    ));
                }
                if calculated.inverse.apply(&next_document)? != current {
                    return Err(history_error(
                        "inverse does not restore the preceding revision",
                    ));
                }
                if migrate {
                    let request = serde_json::to_string(&request)?;
                    let edit = serde_json::to_string(&calculated)?;
                    crate::check_document_size(&request)?;
                    crate::check_document_size(&edit)?;
                    connection.execute(
                        "UPDATE history SET request=?1,edit=?2 WHERE id=?3",
                        params![request, edit, entry],
                    )?;
                }
                cursor = Some(entry);
                redo.clear();
                edits += 1;
                next_document
            }
            kind @ ("undo" | "redo") => {
                let is_redo = kind == "redo";
                let entry = if is_redo { redo.pop() } else { cursor }
                    .ok_or_else(|| history_error("history navigation has no target"))?;
                let plan = history::build_navigation(
                    connection,
                    current,
                    next.revision_id().clone(),
                    is_redo,
                    cursor,
                    entry,
                )?;
                if !next.matches(&plan.next) {
                    return Err(history_error(
                        "history navigation disagrees with its revision",
                    ));
                }
                cursor = plan.next_cursor;
                if !is_redo {
                    redo.push(entry);
                }
                plan.next
            }
            _ => return Err(history_error("noninitial revision has an invalid kind")),
        };
        current = next_document;
        if migrate {
            write_migrated_revision(connection, &current)?;
        }
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
