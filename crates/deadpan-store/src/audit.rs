//! Verified history receipts.
//!
//! Every revision's stored rows (the revision, its history entry or
//! navigation patch, and its Compound step reservations) are linked into a
//! SHA-256 hash chain in chronological order. `history_receipt` records the
//! chain value after the last revision that this exact validator build has
//! proved, together with digests of the other rows those proofs read: the
//! single-original profile and the qualification receipts that existed.
//!
//! Opening rehashes the stored rows, which costs far less than recomputing
//! every command, and replays only the revisions after the receipt. Any
//! modified, removed, reordered or inserted row changes the chain, and any
//! change to the validator's source, the profile or an earlier qualification
//! row invalidates the receipt, so that history is replayed again in full.
//! A commit extends the receipt only after performing every check replay
//! would perform for its new revision.
//!
//! Trust model: the chain detects accidental corruption and edits made
//! outside Deadpan that do not also recompute the receipt. It is stored in
//! the same file and is not keyed, so it is not authentication against a
//! deliberate adversary with write access to the package, who could equally
//! have rewritten history before this receipt existed. `project validate
//! --full` ignores the receipt and recomputes everything.

use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::StoreError;

/// Identity of the compiled validator: a SHA-256 of every workspace crate the
/// store reaches in the lockfile graph, the lockfile and the pinned toolchain
/// (see `build.rs`). A receipt from any other build is ignored, so changing
/// that code makes the next open replay, and read-only opens keep replaying
/// until a writer recertifies; read-only stores never write a receipt.
pub(crate) const VALIDATOR: &str =
    concat!("deadpan-history-v1/", env!("DEADPAN_HISTORY_VALIDATOR"));

pub(crate) type Chain = [u8; 32];

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE history_receipt (
            singleton INTEGER PRIMARY KEY CHECK(singleton=1),
            validator TEXT NOT NULL,
            revisions INTEGER NOT NULL CHECK(revisions>=1),
            head TEXT NOT NULL,
            chain BLOB NOT NULL CHECK(length(chain)=32),
            profile BLOB NOT NULL CHECK(length(profile)=32),
            qualification_rowid INTEGER NOT NULL CHECK(qualification_rowid>=0),
            qualifications BLOB NOT NULL CHECK(length(qualifications)=32)
        ) STRICT;",
    )?;
    Ok(())
}

/// The stored rows of one revision, exactly as hashed.
pub(crate) struct RevisionRows {
    pub id: String,
    pub parent: Option<String>,
    pub kind: String,
    pub document: String,
    pub depth: i64,
    pub json_bound: i64,
    pub patch: Option<String>,
    pub history: Option<HistoryRow>,
    pub steps: Vec<(i64, String, Option<String>)>,
}

pub(crate) struct HistoryRow {
    pub id: i64,
    pub parent: Option<i64>,
    pub request: String,
    pub edit: String,
}

fn field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn optional(hasher: &mut Sha256, bytes: Option<&[u8]>) {
    match bytes {
        None => hasher.update([0]),
        Some(bytes) => {
            hasher.update([1]);
            field(hasher, bytes);
        }
    }
}

fn integer(hasher: &mut Sha256, value: i64) {
    hasher.update(value.to_le_bytes());
}

pub(crate) fn genesis() -> Chain {
    Sha256::digest(b"deadpan-history-chain-v1").into()
}

/// Extend a chain by one revision's stored rows.
pub(crate) fn link(previous: &Chain, rows: &RevisionRows) -> Chain {
    let mut hasher = Sha256::new();
    hasher.update(previous);
    field(&mut hasher, rows.id.as_bytes());
    optional(&mut hasher, rows.parent.as_deref().map(str::as_bytes));
    field(&mut hasher, rows.kind.as_bytes());
    field(&mut hasher, rows.document.as_bytes());
    integer(&mut hasher, rows.depth);
    integer(&mut hasher, rows.json_bound);
    optional(&mut hasher, rows.patch.as_deref().map(str::as_bytes));
    match &rows.history {
        None => hasher.update([0]),
        Some(history) => {
            hasher.update([1]);
            integer(&mut hasher, history.id);
            match history.parent {
                None => hasher.update([0]),
                Some(parent) => {
                    hasher.update([1]);
                    integer(&mut hasher, parent);
                }
            }
            field(&mut hasher, history.request.as_bytes());
            field(&mut hasher, history.edit.as_bytes());
        }
    }
    integer(&mut hasher, rows.steps.len() as i64);
    for (ordinal, revision, document) in &rows.steps {
        integer(&mut hasher, *ordinal);
        field(&mut hasher, revision.as_bytes());
        optional(&mut hasher, document.as_deref().map(str::as_bytes));
    }
    hasher.finalize().into()
}

/// Read one revision's rows. Every text value is bounded by type and byte
/// length in SQL before SQLite returns it, independently of the store's
/// stored-size checks, so hashing never materializes an oversized value.
pub(crate) fn read_rows(connection: &Connection, id: &str) -> Result<RevisionRows, StoreError> {
    let limit = crate::schema::MAX_DOCUMENT_BYTES as i64;
    let identity = deadpan_core::MAX_IDENTITY_BYTES as i64;
    let oversized = || {
        StoreError::Integrity("stored history row exceeds its bound or has the wrong type".into())
    };
    let (parent, kind, document) = crate::validation::read_revision_text(connection, id)?;
    let (depth, json_bound): (Option<i64>, Option<i64>) = connection.query_row(
        "SELECT CASE WHEN typeof(depth)='integer' THEN depth END,
                CASE WHEN typeof(json_bound)='integer' THEN json_bound END
         FROM revisions WHERE id=?1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let (depth, json_bound) = (
        depth.ok_or_else(oversized)?,
        json_bound.ok_or_else(oversized)?,
    );
    let patch: Option<Option<String>> = connection
        .query_row(
            "SELECT CASE WHEN typeof(patch)='text' AND length(CAST(patch AS BLOB))<=?2 THEN patch END
             FROM revision_patches WHERE revision_id=?1",
            params![id, limit],
            |row| row.get(0),
        )
        .optional()?;
    let patch = patch.map(|patch| patch.ok_or_else(oversized)).transpose()?;
    let mut statement = connection.prepare_cached(
        "SELECT CASE WHEN typeof(id)='integer' THEN id END,
                CASE WHEN parent_id IS NULL OR typeof(parent_id)='integer' THEN parent_id END,
                parent_id IS NULL,
                CASE WHEN typeof(request)='text' AND length(CAST(request AS BLOB))<=?2 THEN request END,
                CASE WHEN typeof(edit)='text' AND length(CAST(edit AS BLOB))<=?2 THEN edit END
         FROM history WHERE revision_id=?1",
    )?;
    let mut entries = statement.query(params![id, limit])?;
    let history = match entries.next()? {
        None => None,
        Some(row) => {
            let parent: Option<i64> = row.get(1)?;
            let root: bool = row.get(2)?;
            if parent.is_none() && !root {
                return Err(oversized());
            }
            Some(HistoryRow {
                id: row.get::<_, Option<i64>>(0)?.ok_or_else(oversized)?,
                parent,
                request: row.get::<_, Option<String>>(3)?.ok_or_else(oversized)?,
                edit: row.get::<_, Option<String>>(4)?.ok_or_else(oversized)?,
            })
        }
    };
    if entries.next()?.is_some() {
        return Err(StoreError::History(
            "revision has multiple history entries".into(),
        ));
    }
    drop(entries);
    let mut statement = connection.prepare_cached(
        "SELECT CASE WHEN typeof(ordinal)='integer' THEN ordinal END,
                CASE WHEN typeof(step_revision)='text' AND length(CAST(step_revision AS BLOB)) BETWEEN 1 AND ?2 THEN step_revision END,
                document IS NULL,
                CASE WHEN typeof(document)='text' AND length(CAST(document AS BLOB))<=?3 THEN document END
         FROM transaction_steps WHERE owner_revision=?1 ORDER BY ordinal",
    )?;
    let mut rows = statement.query(params![id, identity, limit])?;
    let mut steps = Vec::new();
    while let Some(row) = rows.next()? {
        let absent: bool = row.get(2)?;
        let document: Option<String> = row.get(3)?;
        if !absent && document.is_none() {
            return Err(oversized());
        }
        steps.push((
            row.get::<_, Option<i64>>(0)?.ok_or_else(oversized)?,
            row.get::<_, Option<String>>(1)?.ok_or_else(oversized)?,
            document,
        ));
    }
    Ok(RevisionRows {
        id: id.to_owned(),
        parent,
        kind,
        document,
        depth,
        json_bound,
        patch,
        history,
        steps,
    })
}

/// A stored receipt. Only one exists; it names the last proved revision.
pub(crate) struct Receipt {
    pub validator: String,
    pub revisions: i64,
    pub head: String,
    pub chain: Chain,
    pub profile: Chain,
    pub qualification_rowid: i64,
    pub qualifications: Chain,
}

pub(crate) fn read(connection: &Connection) -> Result<Option<Receipt>, StoreError> {
    let row = connection
        .query_row(
            "SELECT validator,revisions,head,chain,profile,qualification_rowid,qualifications
             FROM history_receipt WHERE singleton=1
             AND typeof(validator)='text' AND length(CAST(validator AS BLOB))<=256
             AND typeof(head)='text' AND length(CAST(head AS BLOB)) BETWEEN 1 AND ?1",
            [deadpan_core::MAX_IDENTITY_BYTES as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                ))
            },
        )
        .optional()?;
    let Some((validator, revisions, head, chain, profile, qualification_rowid, qualifications)) =
        row
    else {
        return Ok(None);
    };
    let digest = |bytes: Vec<u8>| Chain::try_from(bytes).ok();
    Ok(
        match (digest(chain), digest(profile), digest(qualifications)) {
            (Some(chain), Some(profile), Some(qualifications)) => Some(Receipt {
                validator,
                revisions,
                head,
                chain,
                profile,
                qualification_rowid,
                qualifications,
            }),
            _ => None,
        },
    )
}

pub(crate) fn write(connection: &Connection, receipt: &Receipt) -> Result<(), StoreError> {
    connection.execute(
        "INSERT INTO history_receipt(singleton,validator,revisions,head,chain,profile,qualification_rowid,qualifications)
         VALUES (1,?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(singleton) DO UPDATE SET validator=excluded.validator,revisions=excluded.revisions,
            head=excluded.head,chain=excluded.chain,profile=excluded.profile,
            qualification_rowid=excluded.qualification_rowid,qualifications=excluded.qualifications",
        params![
            receipt.validator,
            receipt.revisions,
            receipt.head,
            &receipt.chain[..],
            &receipt.profile[..],
            receipt.qualification_rowid,
            &receipt.qualifications[..]
        ],
    )?;
    Ok(())
}

fn table_exists(connection: &Connection, name: &str) -> Result<bool, StoreError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
        [name],
        |row| row.get(0),
    )?)
}

/// Digest of the single-original profile row that per-revision checks read.
pub(crate) fn profile_digest(connection: &Connection) -> Result<Chain, StoreError> {
    let mut hasher = Sha256::new();
    hasher.update(b"deadpan-profile-v1");
    if table_exists(connection, "single_source")? {
        let row: Option<(String, Option<i64>)> = connection
            .query_row(
                "SELECT profile,baseline_history FROM single_source WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((profile, baseline)) = row {
            hasher.update([1]);
            field(&mut hasher, profile.as_bytes());
            optional(
                &mut hasher,
                baseline.map(i64::to_le_bytes).as_ref().map(|b| &b[..]),
            );
        }
    }
    Ok(hasher.finalize().into())
}

/// Extend a qualification digest over the rows after `after_rowid`, returning
/// the new digest and last row identity. Receipt rows are append-only and
/// content-addressed; the digest also detects deletion and reordering.
pub(crate) fn extend_qualifications(
    connection: &Connection,
    previous: Chain,
    after_rowid: i64,
    through_rowid: Option<i64>,
) -> Result<(Chain, i64), StoreError> {
    if !table_exists(connection, "source_qualifications")? {
        return Ok((previous, after_rowid));
    }
    let mut statement = connection.prepare(
        "SELECT rowid,id,original_content_id,original_ref,snapshot FROM source_qualifications
         WHERE rowid>?1 AND rowid<=?2 ORDER BY rowid",
    )?;
    let mut rows = statement.query(params![after_rowid, through_rowid.unwrap_or(i64::MAX)])?;
    let mut chain = previous;
    let mut last = after_rowid;
    while let Some(row) = rows.next()? {
        let rowid: i64 = row.get(0)?;
        let id: String = row.get(1)?;
        let content: String = row.get(2)?;
        let original: String = row.get(3)?;
        let snapshot: Vec<u8> = row.get(4)?;
        let mut hasher = Sha256::new();
        hasher.update(chain);
        integer(&mut hasher, rowid);
        field(&mut hasher, id.as_bytes());
        field(&mut hasher, content.as_bytes());
        field(&mut hasher, original.as_bytes());
        field(&mut hasher, &snapshot);
        chain = hasher.finalize().into();
        last = rowid;
    }
    Ok((chain, last))
}

pub(crate) fn qualification_genesis() -> Chain {
    Sha256::digest(b"deadpan-qualification-chain-v1").into()
}

/// Write a receipt for a completely validated chronology.
pub(crate) fn certify(
    connection: &Connection,
    revisions: i64,
    head: &str,
    chain: Chain,
) -> Result<(), StoreError> {
    let (qualifications, qualification_rowid) =
        extend_qualifications(connection, qualification_genesis(), 0, None)?;
    write(
        connection,
        &Receipt {
            validator: VALIDATOR.to_owned(),
            revisions,
            head: head.to_owned(),
            chain,
            profile: profile_digest(connection)?,
            qualification_rowid,
            qualifications,
        },
    )
}

/// After a commit has performed every replay check for `revision`, extend a
/// receipt that covered its parent. A receipt that is absent, from another
/// validator, or whose profile changed in this transaction stays as it is;
/// the next writer open then proves the remaining revisions.
pub(crate) fn extend(
    connection: &Connection,
    parent: &str,
    revision: &str,
) -> Result<(), StoreError> {
    let Some(receipt) = read(connection)? else {
        return Ok(());
    };
    if receipt.validator != VALIDATOR
        || receipt.head != parent
        || receipt.profile != profile_digest(connection)?
    {
        return Ok(());
    }
    let rows = read_rows(connection, revision)?;
    if rows.parent.as_deref() != Some(parent) {
        return Err(StoreError::History(
            "receipt extension does not follow the current revision".into(),
        ));
    }
    let (qualifications, qualification_rowid) = extend_qualifications(
        connection,
        receipt.qualifications,
        receipt.qualification_rowid,
        None,
    )?;
    write(
        connection,
        &Receipt {
            validator: receipt.validator,
            revisions: receipt.revisions + 1,
            head: revision.to_owned(),
            chain: link(&receipt.chain, &rows),
            profile: receipt.profile,
            qualification_rowid,
            qualifications,
        },
    )
}

/// After the caller has validated the single-original profile against every
/// revision the receipt covers, record the profile's new digest so the next
/// open does not replay. Only a receipt from this validator that covers the
/// current head is updated.
pub(crate) fn refresh_profile(connection: &Connection) -> Result<(), StoreError> {
    let Some(mut receipt) = read(connection)? else {
        return Ok(());
    };
    if receipt.validator != VALIDATOR || receipt.head != crate::validation::read_head(connection)? {
        return Ok(());
    }
    receipt.profile = profile_digest(connection)?;
    write(connection, &receipt)
}

/// Whether a stored receipt's dependency digests still hold. The caller then
/// matches its revision chain while hashing the chronology.
pub(crate) fn dependencies_hold(
    connection: &Connection,
    receipt: &Receipt,
) -> Result<bool, StoreError> {
    if receipt.validator != VALIDATOR || receipt.profile != profile_digest(connection)? {
        return Ok(false);
    }
    let (digest, last) = extend_qualifications(
        connection,
        qualification_genesis(),
        0,
        Some(receipt.qualification_rowid),
    )?;
    Ok(digest == receipt.qualifications && last == receipt.qualification_rowid)
}

#[cfg(test)]
mod tests {
    /// The validator identity hashes every path crate the store reaches,
    /// derived from the lockfile, so a change in a non-core dependency that
    /// validation uses (media receipts, analysis, jobs) invalidates receipts.
    #[test]
    fn validator_identity_covers_the_store_dependency_graph() {
        let crates: Vec<_> = env!("DEADPAN_HISTORY_VALIDATOR_CRATES")
            .split(',')
            .collect();
        for expected in [
            "deadpan-store",
            "deadpan-core",
            "deadpan-media",
            "deadpan-analysis",
            "deadpan-jobs",
            "deadpan-source",
        ] {
            assert!(crates.contains(&expected), "{expected}: {crates:?}");
        }
        // Crates the store cannot reach stay out, so unrelated work (the app,
        // the CLI) does not invalidate stored proofs.
        for unrelated in ["deadpan-app", "deadpan-cli", "deadpan-playback"] {
            assert!(!crates.contains(&unrelated), "{unrelated}: {crates:?}");
        }
        assert_eq!(super::VALIDATOR.len(), "deadpan-history-v1/".len() + 64);
    }
}
