//! Streaming semantic validation. Documents are bounded before SQLite returns
//! their text; historical documents are replayed one at a time, not accumulated.

use deadpan_core::{
    CommandRequest, EditTransaction, MAX_IDENTITY_BYTES, ProjectDocument, RevisionId,
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
    crate::revision_storage::check_stored_sizes(connection, limit)
}

/// Visit every revision document from chronology index `from`, including
/// abandoned branches. The first visited document is rebuilt from its
/// nearest keyframe; each later one by applying its stored patch in place.
/// Callers run history validation first; this does not replay commands.
pub(crate) fn for_each_revision_document(
    connection: &Connection,
    from: usize,
    mut visit: impl FnMut(&ProjectDocument) -> Result<(), StoreError>,
) -> Result<(), StoreError> {
    let order = chronology(connection)?;
    let Some(first) = order.get(from) else {
        return Ok(());
    };
    let mut current = ProjectDocument::clone(&*read_validated_revision(connection, first)?);
    visit(&current)?;
    for child in &order[from + 1..] {
        let (_, kind) = read_revision_meta(connection, child)?;
        crate::revision_storage::stored_patch(connection, child, &kind)?
            .ok_or_else(|| history_error("revision has no stored patch"))?
            .apply_stored_in_place(&mut current)?;
        if current.revision_id().as_str() != child {
            return Err(history_error("revision identity disagrees with document"));
        }
        visit(&current)?;
    }
    Ok(())
}

/// Parent and kind of one revision, without its document text.
fn read_revision_meta(
    connection: &Connection,
    id: &str,
) -> Result<(Option<String>, String), StoreError> {
    let (parent, kind): (Option<String>, Option<String>) = connection.query_row(
        "SELECT CASE WHEN parent_id IS NULL THEN '' WHEN typeof(parent_id)='text' AND length(CAST(parent_id AS BLOB)) BETWEEN 1 AND ?2 THEN parent_id END,
         CASE WHEN typeof(kind)='text' AND length(CAST(kind AS BLOB)) BETWEEN 1 AND 7 AND kind IN ('initial','edit','undo','redo') THEN kind END
         FROM revisions WHERE id=?1",
        params![id, MAX_IDENTITY_BYTES as i64],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let parent = if parent.as_deref() == Some("") {
        None
    } else {
        Some(checked_id(parent)?)
    };
    let kind =
        kind.ok_or_else(|| StoreError::Integrity("invalid or oversized revision kind".into()))?;
    Ok((parent, kind))
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
    #[cfg(test)]
    REVISION_READS.with(|count| count.set(count.get() + 1));
    read_revision_bounded(connection, id, crate::schema::MAX_DOCUMENT_BYTES)
}

#[cfg(test)]
thread_local! {
    pub(crate) static REVISION_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn read_revision_bounded(
    connection: &Connection,
    id: &str,
    limit: usize,
) -> Result<RevisionRecord, StoreError> {
    let document = read_validated_bounded(connection, id, limit)?;
    Ok(RevisionRecord {
        document: ProjectDocument::clone(&document),
    })
}

/// One stored or rebuilt revision, validated once with its proof retained.
pub(crate) fn read_validated_revision(
    connection: &Connection,
    id: &str,
) -> Result<deadpan_core::ValidatedDocument, StoreError> {
    #[cfg(test)]
    REVISION_READS.with(|count| count.set(count.get() + 1));
    read_validated_bounded(connection, id, crate::schema::MAX_DOCUMENT_BYTES)
}

fn read_validated_bounded(
    connection: &Connection,
    id: &str,
    limit: usize,
) -> Result<deadpan_core::ValidatedDocument, StoreError> {
    let (_, _, json) = read_revision_json(connection, id, limit)?;
    if json == crate::revision_storage::ELIDED {
        return crate::revision_storage::reconstruct(connection, id);
    }
    let document = ProjectDocument::from_json_validated(&json)?;
    if document.revision_id().as_str() != id {
        return Err(StoreError::Integrity(
            "revision identity disagrees with document".into(),
        ));
    }
    Ok(document)
}

/// Parent, kind and stored text (a document or the elision marker).
pub(crate) fn read_revision_text(
    connection: &Connection,
    id: &str,
) -> Result<(Option<String>, String, String), StoreError> {
    read_revision_json(connection, id, crate::schema::MAX_DOCUMENT_BYTES)
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
    deadpan_diagnostics::IO
        .store_revisions
        .read(json.len() as u64);
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

/// Whether history validation may rely on a stored receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryMode {
    /// Verify the hash chain over every stored row, then replay only the
    /// revisions after a matching receipt from this validator build.
    Receipt,
    /// Recompute every command and navigation from the initial revision.
    Full,
}

/// The validated chronology.
pub(crate) struct HistoryAudit {
    /// Revision identities from the initial revision to the head.
    pub order: Vec<String>,
    /// The leading revisions a receipt proved; later ones were replayed now.
    pub verified: usize,
    /// The hash chain over every revision.
    pub chain: crate::audit::Chain,
}

pub(crate) fn validate_history(
    connection: &Connection,
    mode: HistoryMode,
) -> Result<HistoryAudit, StoreError> {
    replay(connection, mode)
}

/// An elided revision has no stored document; replay recomputes it and then
/// checks its stored patch instead.
fn read_replay_revision(
    connection: &Connection,
    id: &str,
) -> Result<(Option<String>, String, Option<ProjectDocument>), StoreError> {
    let (parent, kind, json) =
        read_revision_json(connection, id, crate::schema::MAX_DOCUMENT_BYTES)?;
    if json == crate::revision_storage::ELIDED {
        if kind == "initial" {
            return Err(history_error("initial revision document is elided"));
        }
        return Ok((parent, kind, None));
    }
    let document = ProjectDocument::from_json(&json)?;
    if document.revision_id().as_str() != id {
        return Err(history_error("revision identity disagrees with document"));
    }
    Ok((parent, kind, Some(document)))
}
pub(crate) fn read_initial_id(connection: &Connection) -> Result<String, StoreError> {
    let mut roots = connection.prepare_cached("SELECT CASE WHEN typeof(id)='text' AND length(CAST(id AS BLOB)) BETWEEN 1 AND ?1 THEN id END FROM revisions WHERE parent_id IS NULL LIMIT 2")?;
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

/// The linear chronology from the initial revision, rejecting forks, cycles
/// and unreachable rows without reading any document.
pub(crate) fn chronology(connection: &Connection) -> Result<Vec<String>, StoreError> {
    let count: i64 =
        connection.query_row("SELECT COUNT(*) FROM revisions", [], |row| row.get(0))?;
    let mut order = vec![read_initial_id(connection)?];
    let mut children = connection.prepare_cached("SELECT CASE WHEN typeof(id)='text' AND length(CAST(id AS BLOB)) BETWEEN 1 AND ?2 THEN id END FROM revisions WHERE parent_id=?1 LIMIT 2")?;
    loop {
        let current = order
            .last()
            .expect("chronology starts at the initial revision");
        let mut rows = children.query(params![current.as_str(), MAX_IDENTITY_BYTES as i64])?;
        let Some(row) = rows.next()? else { break };
        let id = checked_id(row.get(0)?)?;
        if rows.next()?.is_some() {
            return Err(history_error("revision chronology forks"));
        }
        drop(rows);
        order.push(id);
        if i64::try_from(order.len()).unwrap_or(i64::MAX) > count {
            return Err(history_error("revision chronology contains a cycle"));
        }
    }
    if i64::try_from(order.len()).ok() != Some(count) {
        return Err(history_error(
            "revision chronology contains unreachable revisions",
        ));
    }
    Ok(order)
}

/// Cursor and redo state after a prefix of the chronology.
#[derive(Clone, Default)]
struct Navigation {
    cursor: Option<i64>,
    redo: Vec<i64>,
    edits: i64,
}

fn replay(connection: &Connection, mode: HistoryMode) -> Result<HistoryAudit, StoreError> {
    let order = chronology(connection)?;
    let (_, kind, first) = read_replay_revision(connection, &order[0])?;
    if kind != "initial" {
        return Err(history_error("root revision is not initial"));
    }
    let first = first.ok_or_else(|| history_error("initial revision document is elided"))?;
    crate::compound::validate_namespace(connection, &first)?;
    let initial_allocations: std::collections::BTreeSet<_> = first
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
        .chain(
            first
                .audio_lineage()
                .values()
                .map(|lineage| lineage.allocation.as_str().to_owned()),
        )
        .chain(
            first
                .audio_bindings()
                .allocation_ids()
                .into_iter()
                .map(|allocation| allocation.as_str().to_owned()),
        )
        .collect();
    let receipt = match mode {
        HistoryMode::Full => None,
        HistoryMode::Receipt => crate::audit::read(connection)?
            .filter(|receipt| receipt.revisions >= 1)
            .map(|receipt| {
                crate::audit::dependencies_hold(connection, &receipt)
                    .map(|ok| ok.then_some(receipt))
            })
            .transpose()?
            .flatten(),
    };

    // Hash every stored row and check keyframe metadata and the history
    // cursor structure, all without parsing any document or command.
    let mut chain = crate::audit::genesis();
    let mut navigation = Navigation::default();
    let mut verified = 0usize;
    let mut verified_navigation = Navigation::default();
    let mut metadata: Option<(i64, i64)> = None;
    let mut parent_entry =
        connection.prepare_cached("SELECT parent_id FROM history WHERE id=?1")?;
    for (index, id) in order.iter().enumerate() {
        if index > 0 && initial_allocations.contains(id) {
            return Err(history_error(
                "revision reuses an initial occurrence, audio-lineage or timing allocation",
            ));
        }
        let rows = crate::audit::read_rows(connection, id)?;
        if rows.parent.as_deref() != index.checked_sub(1).map(|parent| order[parent].as_str()) {
            return Err(history_error("revision parent disagrees"));
        }
        let patch_bytes = match (rows.kind.as_str(), &rows.history, &rows.patch) {
            ("initial", None, None) if index == 0 => None,
            ("edit", Some(history), None) if index > 0 => {
                if history.parent != navigation.cursor {
                    return Err(history_error("history parent or revision disagrees"));
                }
                navigation.cursor = Some(history.id);
                navigation.redo.clear();
                navigation.edits += 1;
                Some(history.edit.len())
            }
            ("undo", None, patch) if index > 0 => {
                let entry = navigation
                    .cursor
                    .ok_or_else(|| history_error("history navigation has no target"))?;
                navigation.cursor = parent_entry.query_row([entry], |row| row.get(0))?;
                navigation.redo.push(entry);
                patch.as_ref().map(String::len)
            }
            ("redo", None, patch) if index > 0 => {
                let entry = navigation
                    .redo
                    .pop()
                    .ok_or_else(|| history_error("history navigation has no target"))?;
                let parent: Option<i64> = parent_entry.query_row([entry], |row| row.get(0))?;
                if parent != navigation.cursor {
                    return Err(history_error(
                        "redo entry is not a child of the current edit",
                    ));
                }
                navigation.cursor = Some(entry);
                patch.as_ref().map(String::len)
            }
            ("edit", None, _) => return Err(history_error("edit has no history entry")),
            _ => return Err(history_error("noninitial revision has an invalid kind")),
        };
        let stored =
            (rows.document != crate::revision_storage::ELIDED).then_some(rows.document.len());
        if !crate::revision_storage::metadata_holds(
            metadata,
            stored,
            patch_bytes,
            (rows.depth, rows.json_bound),
        ) {
            return Err(history_error(
                "revision keyframe metadata disagrees with its stored rows",
            ));
        }
        metadata = Some((rows.depth, rows.json_bound));
        chain = crate::audit::link(&chain, &rows);
        if let Some(receipt) = &receipt
            && i64::try_from(index + 1).ok() == Some(receipt.revisions)
            && receipt.head == *id
            && receipt.chain == chain
        {
            verified = index + 1;
            verified_navigation = navigation.clone();
        }
    }
    let history_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))?;
    if history_count != navigation.edits {
        return Err(history_error(
            "history contains entries without corresponding edits",
        ));
    }

    if verified < order.len() {
        let (current, state) = if verified == 0 {
            (
                deadpan_core::ValidatedDocument::new(std::sync::Arc::new(first))?,
                Navigation::default(),
            )
        } else {
            (
                crate::revision_storage::reconstruct(connection, &order[verified - 1])?,
                verified_navigation,
            )
        };
        let replayed = replay_from(connection, &order, verified.max(1), current, state)?;
        if replayed.cursor != navigation.cursor || replayed.redo != navigation.redo {
            return Err(history_error("history navigation disagrees with replay"));
        }
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
        || head != *order.last().expect("nonempty chronology")
        || saved_cursor != navigation.cursor
        || state.next()?.is_some()
    {
        return Err(history_error(
            "current state disagrees with revision history",
        ));
    }
    let mut saved_redo =
        connection.prepare("SELECT position,history_id FROM redo ORDER BY position")?;
    let mut rows = saved_redo.query([])?;
    for (index, entry) in navigation.redo.iter().enumerate() {
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
    Ok(HistoryAudit {
        order,
        verified,
        chain,
    })
}

/// Recompute every command and navigation from chronology index `from`,
/// whose predecessor's validated document and navigation state are given.
fn replay_from(
    connection: &Connection,
    order: &[String],
    from: usize,
    mut current: deadpan_core::ValidatedDocument,
    state: Navigation,
) -> Result<Navigation, StoreError> {
    let Navigation {
        mut cursor,
        mut redo,
        mut edits,
    } = state;
    // Retain identities, never historical documents. Admitted revision IDs
    // prevent future checkpoint references; the redo stack holds history IDs.
    let mut admitted: std::collections::BTreeSet<RevisionId> = order[..from]
        .iter()
        .map(|id| RevisionId::new(id.clone()))
        .collect::<Result<_, _>>()?;
    for id in &order[from..] {
        let (parent, kind, next) = read_replay_revision(connection, id)?;
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
                if parent != cursor || revision != *id {
                    return Err(history_error("history parent or revision disagrees"));
                }
                let request: CommandRequest = serde_json::from_str(&request_json)?;
                let calculated =
                    if matches!(request.command, deadpan_core::Command::Compound { .. }) {
                        crate::compound::replay(connection, &current, &request, &admitted)?
                    } else {
                        deadpan_core::apply_validated(&current, &request)?.0
                    };
                let matches_edit =
                    calculated == serde_json::from_str::<EditTransaction>(&edit_json)?;
                // Validated once, reusing unchanged binding owners' proofs.
                let next_document = current.apply_patch(&calculated.forward)?;
                if !matches!(request.command, deadpan_core::Command::Compound { .. }) {
                    crate::compound::validate_ordinary_history(
                        connection,
                        &current,
                        &next_document,
                        &request,
                        &admitted,
                    )?;
                }
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                crate::source_registration::validate_hold_audio_source(
                    connection,
                    &current,
                    &next_document,
                    &request,
                )?;
                if !matches_edit || next.as_ref().is_some_and(|next| *next != *next_document) {
                    return Err(history_error(
                        "stored command, patches, and revision disagree",
                    ));
                }
                // Equal to the validated preceding revision, so itself valid.
                if calculated.inverse.apply_stored(&next_document)? != **current.document() {
                    return Err(history_error(
                        "inverse does not restore the preceding revision",
                    ));
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
                    &current,
                    RevisionId::new(id.clone())?,
                    is_redo,
                    cursor,
                    entry,
                )?;
                let stored = crate::revision_storage::stored_patch(connection, id, kind)?;
                if next.as_ref().is_some_and(|next| *next != *plan.next)
                    || stored
                        .as_ref()
                        .is_none_or(|stored| *stored != plan.edit.forward)
                {
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
        admitted.insert(next_document.revision_id().clone());
        current = next_document;
    }
    Ok(Navigation {
        cursor,
        redo,
        edits,
    })
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
                    "INSERT INTO revisions(id,kind,document,depth,json_bound) VALUES ('r','initial','{}',0,2);
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
                    ("revisions", "id") => validate_history(&db, HistoryMode::Full).err(),
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
            "INSERT INTO revisions(id,kind,document,depth,json_bound) VALUES ('r','initial','{}',0,2)",
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
