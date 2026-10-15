//! Revision documents are stored as periodic keyframes plus forward patches.
//!
//! A commit writes only its patch: an edit's history entry, or an undo/redo
//! revision's own row in `revision_patches`. Its document is the `null`
//! marker unless it is the initial revision or a keyframe. Each row records
//! `depth`, the number of patches since its nearest stored ancestor, and
//! `json_bound`, an upper bound on its document's compact JSON length. A
//! revision is stored as a keyframe in its own commit when its depth would
//! reach [`MAX_PATCH_CHAIN`] or its bound would exceed the document limit, so
//! any revision is rebuilt from at most `MAX_PATCH_CHAIN - 1` patches and a
//! document over the limit is always caught by an exact serialization.
//!
//! A keyframe is due every [`KEYFRAME_INTERVAL`] patches, but writing one
//! (serializing, storing and hashing the complete document) is kept off the
//! edit's commit: a commit at or past the interval stays elided, and
//! [`write_due_keyframe`] later stores the head's document in its own row in
//! a separate transaction, together with the history receipt's last link.
//! Only the head can be converted, because no row depends on its metadata
//! yet. Until it lands (the host was busy, or the process ended first) depth
//! keeps growing, bounded by `MAX_PATCH_CHAIN` where the commit itself stores
//! the keyframe again; a writable open converts a due head.
//!
//! The bound holds because a patch carries every after-value it installs,
//! serialized by the same `Serialize` implementations as the document, and
//! wrapped in strictly more syntax (keys, `before`, `after`, nulls). A patch
//! can therefore grow the document's compact JSON by less than its own
//! length; [`PATCH_GROWTH_SLACK`] covers field-level syntax such as a map
//! field appearing for the first time. History rows store the forward and
//! inverse patch together, which only loosens the bound.

use std::sync::Arc;

use deadpan_core::{DocumentPatch, ProjectDocument, RevisionId, ValidatedDocument};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;

use crate::StoreError;

/// A stored document marker for an elided revision. A project document is
/// always a JSON object, so the literal can never be a real document.
pub(crate) const ELIDED: &str = "null";

/// A keyframe is due once a revision is this many patches from its nearest
/// stored ancestor; [`write_due_keyframe`] stores it outside the commit.
pub(crate) const KEYFRAME_INTERVAL: i64 = 64;

/// A revision is rebuilt by applying fewer than this many stored patches: a
/// commit that would reach it stores its keyframe itself.
pub(crate) const MAX_PATCH_CHAIN: i64 = 2 * KEYFRAME_INTERVAL;

/// Field-level JSON syntax a patch may add beyond its own length.
pub(crate) const PATCH_GROWTH_SLACK: i64 = 4096;

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE revision_patches (
            revision_id TEXT PRIMARY KEY REFERENCES revisions(id),
            patch TEXT NOT NULL CHECK (json_valid(patch))
        ) STRICT;",
    )?;
    Ok(())
}

/// How a new revision's row is written.
pub(crate) enum StoredPatch<'a> {
    /// The initial revision has no parent and always stores its document.
    Initial,
    /// An edit's forward patch is in its history entry of this many bytes.
    Edit { bytes: usize },
    /// An undo or redo revision's forward patch, stored in `revision_patches`.
    Navigation { json: &'a str },
}

/// Insert one revision row. The caller has already checked that the
/// revision identity is unused and that `after` is the validated result of
/// applying the stored patch to `before`.
pub(crate) fn insert(
    connection: &Connection,
    before: Option<&RevisionId>,
    after: &ProjectDocument,
    kind: &str,
    patch: StoredPatch<'_>,
) -> Result<(), StoreError> {
    let (patch_bytes, navigation) = match (kind, &patch) {
        ("initial", StoredPatch::Initial) if before.is_none() => (None, None),
        ("edit", StoredPatch::Edit { bytes }) if before.is_some() => (Some(*bytes), None),
        ("undo" | "redo", StoredPatch::Navigation { json }) if before.is_some() => {
            (Some(json.len()), Some(*json))
        }
        _ => {
            return Err(StoreError::Integrity(
                "revision patch does not match its kind".into(),
            ));
        }
    };
    let elided = match (before, patch_bytes) {
        (Some(parent), Some(bytes)) => {
            let (depth, bound): (i64, i64) = connection.query_row(
                "SELECT depth,json_bound FROM revisions WHERE id=?1",
                [parent.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let bound = i64::try_from(bytes)
                .ok()
                .and_then(|bytes| bound.checked_add(bytes)?.checked_add(PATCH_GROWTH_SLACK));
            match bound {
                Some(bound)
                    if depth + 1 < MAX_PATCH_CHAIN
                        && bound <= crate::schema::MAX_DOCUMENT_BYTES as i64 =>
                {
                    Some((depth + 1, bound))
                }
                _ => None,
            }
        }
        _ => None,
    };
    let (document, depth, bound) = match elided {
        Some((depth, bound)) => (ELIDED.to_owned(), depth, bound),
        None => {
            let json = after.to_compact_json()?;
            crate::check_document_size(&json)?;
            let bound = json.len() as i64;
            (json, 0, bound)
        }
    };
    connection.execute(
        "INSERT INTO revisions(id,parent_id,kind,document,depth,json_bound) VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            after.revision_id().as_str(),
            before.map(RevisionId::as_str),
            kind,
            document,
            depth,
            bound
        ],
    )?;
    if let Some(json) = navigation {
        connection.execute(
            "INSERT INTO revision_patches(revision_id,patch) VALUES (?1,?2)",
            params![after.revision_id().as_str(), json],
        )?;
    }
    // Counted once the rows are accepted by SQLite (the enclosing transaction
    // may still roll back).
    deadpan_diagnostics::IO
        .store_revisions
        .write(document.len() as u64 + navigation.map_or(0, |json| json.len() as u64));
    crate::publish_page_io(connection);
    Ok(())
}

/// The head's identity when its keyframe is due: it is elided at least
/// [`KEYFRAME_INTERVAL`] patches from its nearest stored ancestor.
pub(crate) fn due_head(connection: &Connection) -> Result<Option<String>, StoreError> {
    let head = crate::validation::read_head(connection)?;
    let depth: Option<i64> = connection
        .query_row(
            "SELECT depth FROM revisions WHERE id=?1 AND document=?2",
            params![head, ELIDED],
            |row| row.get(0),
        )
        .optional()?;
    Ok(depth
        .is_some_and(|depth| depth >= KEYFRAME_INTERVAL)
        .then_some(head))
}

/// Store the due head's complete document in its own row, inside the
/// caller's write transaction, and recompute the receipt's last link over
/// the replaced row. `json` is `head`'s compact serialization.
///
/// The row becomes exactly what a keyframe commit would have written
/// (document, depth 0, bound its length), and nothing depends on its old
/// metadata: only an elided head without children is converted, and every
/// later revision is committed after this one and links to the new row.
/// Returns `false`, leaving the caller to roll back, when the head changed
/// or the receipt ends at the head but `link` cannot be proved to precede
/// it; the keyframe then stays due.
pub(crate) fn write_due_keyframe(
    connection: &Connection,
    head: &ProjectDocument,
    json: &str,
    link: Option<&crate::audit::ReceiptLink>,
) -> Result<bool, StoreError> {
    let id = head.revision_id().as_str();
    if due_head(connection)?.as_deref() != Some(id) {
        return Ok(false);
    }
    crate::check_document_size(json)?;
    let bound = i64::try_from(json.len())
        .map_err(|_| StoreError::Integrity("keyframe length exceeds its bound".into()))?;
    let old = crate::audit::read_rows(connection, id)?;
    let previous = match crate::audit::head_receipt(connection, link, &old)? {
        crate::audit::HeadReceipt::Unproved => return Ok(false),
        crate::audit::HeadReceipt::Uncovered => None,
        crate::audit::HeadReceipt::Covered { previous } => Some(previous),
    };
    let changed = connection.execute(
        "UPDATE revisions SET document=?2,depth=0,json_bound=?3
         WHERE id=?1 AND document=?4 AND NOT EXISTS(SELECT 1 FROM revisions WHERE parent_id=?1)",
        params![id, json, bound, ELIDED],
    )?;
    if changed != 1 {
        return Err(StoreError::History(
            "a due keyframe's revision changed while it was written".into(),
        ));
    }
    if let Some(previous) = previous {
        let new = crate::audit::RevisionRows {
            document: json.to_owned(),
            depth: 0,
            json_bound: bound,
            ..old
        };
        crate::audit::replace_head_link(connection, &previous, &new)?;
    }
    deadpan_diagnostics::IO
        .store_revisions
        .write(json.len() as u64);
    crate::publish_page_io(connection);
    Ok(true)
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
        "undo" | "redo" => connection
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
    deadpan_diagnostics::IO
        .store_revisions
        .read(json.len() as u64);
    crate::publish_page_io(connection);
    Ok(Some(if kind == "edit" {
        // Rebuilding needs only the forward patch; skip materializing the
        // inverse half of the history entry.
        #[derive(Deserialize)]
        struct Forward {
            forward: DocumentPatch,
        }
        serde_json::from_str::<Forward>(&json)?.forward
    } else {
        serde_json::from_str::<DocumentPatch>(&json)?
    }))
}

/// Rebuild an elided revision from its nearest stored ancestor, applying each
/// patch in place, and validate the result once.
pub(crate) fn reconstruct(
    connection: &Connection,
    id: &str,
) -> Result<ValidatedDocument, StoreError> {
    let mut patches = Vec::new();
    let mut current = id.to_owned();
    let base = loop {
        if i64::try_from(patches.len()).unwrap_or(i64::MAX) >= MAX_PATCH_CHAIN {
            return Err(StoreError::History(
                "elided revision has no stored ancestor within the keyframe interval".into(),
            ));
        }
        let (parent, kind, json) = crate::validation::read_revision_text(connection, &current)?;
        if json != ELIDED {
            break json;
        }
        patches
            .push(stored_patch(connection, &current, &kind)?.ok_or_else(|| {
                StoreError::History("elided revision has no stored patch".into())
            })?);
        current = parent.ok_or_else(|| StoreError::History("elided initial revision".into()))?;
    };
    let document = if patches.is_empty() {
        ProjectDocument::from_json_validated(&base)?
    } else {
        let mut document = ProjectDocument::from_json(&base)?;
        for patch in patches.iter().rev() {
            patch.apply_stored_in_place(&mut document)?;
        }
        ValidatedDocument::new(Arc::new(document))?
    };
    if document.revision_id().as_str() != id {
        return Err(StoreError::Integrity(
            "revision identity disagrees with document".into(),
        ));
    }
    Ok(document)
}

/// Bound and type every stored patch and keyframe column, and require each
/// patch row to belong to an undo or redo revision.
pub(crate) fn check_stored_sizes(connection: &Connection, limit: usize) -> Result<(), StoreError> {
    let invalid: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM revision_patches p WHERE typeof(p.patch) != 'text'
            OR length(CAST(p.patch AS BLOB)) > ?1
            OR NOT EXISTS(SELECT 1 FROM revisions r WHERE r.id=p.revision_id AND r.kind IN ('undo','redo')))
         OR EXISTS(SELECT 1 FROM revisions WHERE typeof(depth) != 'integer' OR typeof(json_bound) != 'integer'
            OR depth < 0 OR depth >= ?2 OR json_bound < 0 OR json_bound > ?1
            OR (depth = 0) != (document != ?3)
            OR (parent_id IS NULL AND depth != 0))",
        params![limit as i64, MAX_PATCH_CHAIN, ELIDED],
        |row| row.get(0),
    )?;
    if invalid {
        return Err(StoreError::Integrity(
            "stored revision patch or keyframe metadata is invalid".into(),
        ));
    }
    Ok(())
}

/// Whether one revision's stored keyframe metadata is a sound bound given
/// its parent's: a stored document starts a chain with a bound at least its
/// own length, and an elided revision extends its parent's chain by one patch
/// with a bound at least the parent's plus that patch and the growth slack.
/// By induction every bound is at least the true compact document length.
pub(crate) fn metadata_holds(
    parent: Option<(i64, i64)>,
    document_bytes: Option<usize>,
    patch_bytes: Option<usize>,
    (depth, bound): (i64, i64),
) -> bool {
    let within = depth < MAX_PATCH_CHAIN && bound <= crate::schema::MAX_DOCUMENT_BYTES as i64;
    within
        && match (document_bytes, parent, patch_bytes) {
            (Some(bytes), _, _) => {
                depth == 0 && i64::try_from(bytes).is_ok_and(|bytes| bytes <= bound)
            }
            (None, Some((parent_depth, parent_bound)), Some(bytes)) => {
                depth == parent_depth + 1
                    && i64::try_from(bytes)
                        .ok()
                        .and_then(|bytes| parent_bound.checked_add(bytes))
                        .and_then(|minimum| minimum.checked_add(PATCH_GROWTH_SLACK))
                        .is_some_and(|minimum| minimum <= bound)
            }
            _ => false,
        }
}
