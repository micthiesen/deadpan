//! Revision documents are stored as periodic snapshots plus forward patches.
//!
//! Every revision used to store its complete document, so a long history cost
//! revisions × document size on disk and every open parsed each one. Now the
//! newest revision always keeps its document; when a child is written, the
//! parent's document is replaced by the `null` marker unless the parent is the
//! initial revision or a keyframe (every [`KEYFRAME_INTERVAL`]th revision in the
//! linear chronology). An elided revision is rebuilt from its nearest stored
//! ancestor by applying the stored forward patches: an edit's history entry, or
//! an undo/redo revision's own row in `revision_patches`. Patches keep their
//! before-value guards; only the final reconstructed document is validated.

use deadpan_core::{DocumentPatch, EditTransaction, ProjectDocument, RevisionId};
use rusqlite::{Connection, OptionalExtension, params};

use crate::StoreError;

/// A stored document marker for an elided revision. A project document is
/// always a JSON object, so the literal can never be a real document.
pub(crate) const ELIDED: &str = "null";

/// At most this many patches are applied to rebuild an old revision.
pub(crate) const KEYFRAME_INTERVAL: i64 = 16;

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE revision_patches (
            revision_id TEXT PRIMARY KEY REFERENCES revisions(id),
            patch TEXT NOT NULL CHECK (json_valid(patch))
        ) STRICT;",
    )?;
    Ok(())
}

/// A read-only open of a schema-62 package has no table, and no elided rows.
pub(crate) fn has_table(connection: &Connection) -> Result<bool, StoreError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='revision_patches')",
        [],
        |row| row.get(0),
    )?)
}

/// Record a new revision's navigation patch (undo/redo only: an edit's patch is
/// its history entry), then elide its parent's document when permitted.
pub(crate) fn after_insert(
    connection: &Connection,
    revision: &RevisionId,
    parent: &RevisionId,
    kind: &str,
    patch: Option<&DocumentPatch>,
) -> Result<(), StoreError> {
    match (kind, patch) {
        ("edit", None) => {}
        ("undo" | "redo", Some(patch)) => {
            let json = serde_json::to_string(patch)?;
            crate::check_document_size(&json)?;
            connection.execute(
                "INSERT INTO revision_patches(revision_id,patch) VALUES (?1,?2)",
                params![revision.as_str(), json],
            )?;
        }
        _ => {
            return Err(StoreError::Integrity(
                "revision patch does not match its kind".into(),
            ));
        }
    }
    // The chronology is one linear chain, so the parent has count-2 ancestors.
    let (count, kind, patched): (i64, String, bool) = connection.query_row(
        "SELECT (SELECT count(*) FROM revisions), kind,
            EXISTS(SELECT 1 FROM revision_patches WHERE revision_id=?1)
            OR EXISTS(SELECT 1 FROM history WHERE revision_id=?1)
         FROM revisions WHERE id=?1",
        [parent.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let depth = count - 2;
    if kind != "initial" && patched && depth % KEYFRAME_INTERVAL != 0 {
        connection.execute(
            "UPDATE revisions SET document=?2 WHERE id=?1",
            params![parent.as_str(), ELIDED],
        )?;
    }
    Ok(())
}

/// The stored forward patch from `revision`'s parent to `revision`.
pub(crate) fn stored_patch(
    connection: &Connection,
    revision: &str,
    kind: &str,
) -> Result<Option<DocumentPatch>, StoreError> {
    let limit = crate::schema::MAX_DOCUMENT_BYTES as i64;
    let json: Option<Option<String>> = match kind {
        "edit" => connection
            .query_row(
                "SELECT CASE WHEN typeof(edit)='text' AND length(CAST(edit AS BLOB))<=?2 THEN edit END
                 FROM history WHERE revision_id=?1",
                params![revision, limit],
                |row| row.get(0),
            )
            .optional()?,
        "undo" | "redo" if has_table(connection)? => connection
            .query_row(
                "SELECT CASE WHEN typeof(patch)='text' AND length(CAST(patch AS BLOB))<=?2 THEN patch END
                 FROM revision_patches WHERE revision_id=?1",
                params![revision, limit],
                |row| row.get(0),
            )
            .optional()?,
        _ => None,
    };
    let Some(json) = json else {
        return Ok(None);
    };
    let json = json.ok_or_else(|| {
        StoreError::Integrity("stored revision patch exceeds its bound or is not text".into())
    })?;
    Ok(Some(if kind == "edit" {
        serde_json::from_str::<EditTransaction>(&json)?.forward
    } else {
        serde_json::from_str::<DocumentPatch>(&json)?
    }))
}

/// Rebuild an elided revision from its nearest stored ancestor.
pub(crate) fn reconstruct(
    connection: &Connection,
    id: &str,
) -> Result<ProjectDocument, StoreError> {
    let count: i64 =
        connection.query_row("SELECT COUNT(*) FROM revisions", [], |row| row.get(0))?;
    let mut patches = Vec::new();
    let mut current = id.to_owned();
    let base = loop {
        if i64::try_from(patches.len()).unwrap_or(i64::MAX) >= count {
            return Err(StoreError::History(
                "elided revision has no stored ancestor".into(),
            ));
        }
        let (parent, kind, json) = crate::validation::read_revision_text(connection, &current)?;
        if json != ELIDED {
            break ProjectDocument::from_json(&json)?;
        }
        patches
            .push(stored_patch(connection, &current, &kind)?.ok_or_else(|| {
                StoreError::History("elided revision has no stored patch".into())
            })?);
        current = parent.ok_or_else(|| StoreError::History("elided initial revision".into()))?;
    };
    let mut document = base;
    for patch in patches.iter().rev() {
        document = patch.apply_stored(&document)?;
    }
    document.validate()?;
    if document.revision_id().as_str() != id {
        return Err(StoreError::Integrity(
            "revision identity disagrees with document".into(),
        ));
    }
    Ok(document)
}

/// Bound and type every stored patch, and require each row to belong to an
/// undo or redo revision.
pub(crate) fn check_stored_sizes(connection: &Connection, limit: usize) -> Result<(), StoreError> {
    // The newest revision is the reconstruction base for every later write,
    // and the initial revision has no parent to rebuild from.
    let elided_base: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM revisions WHERE document=?1 AND (parent_id IS NULL
            OR id=(SELECT head_revision FROM state WHERE singleton=1)))",
        [ELIDED],
        |row| row.get(0),
    )?;
    if elided_base {
        return Err(StoreError::Integrity(
            "the initial or current revision document is elided".into(),
        ));
    }
    if !has_table(connection)? {
        return Ok(());
    }
    let invalid: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM revision_patches p WHERE typeof(p.patch) != 'text'
            OR length(CAST(p.patch AS BLOB)) > ?1
            OR NOT EXISTS(SELECT 1 FROM revisions r WHERE r.id=p.revision_id AND r.kind IN ('undo','redo')))",
        [limit as i64],
        |row| row.get(0),
    )?;
    if invalid {
        return Err(StoreError::Integrity(
            "stored revision patch exceeds its bound or names a non-navigation revision".into(),
        ));
    }
    Ok(())
}
