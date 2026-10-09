//! Identities a restore discarded, kept so they are never issued again.
//!
//! Restoring a backup replaces the database with an older one. Without this
//! table every revision, generation request, Hold request version and
//! original location version issued after that backup would become free
//! again, and a stale request or receipt could match something new. Before a
//! restore copies a backup in, [`carry_forward`] records in the copy what the
//! replaced database had issued that the copy lacks:
//!
//! - `revision`: revision and Compound step identities (checked by
//!   `ensure_unused_revisions`);
//! - `generation_request`: request identities (attempts are scoped to them);
//! - `generation_scope_version`: the highest request version per authored scope;
//! - `original_version`: the highest original location version per content.
//!
//! Allocation consults the table, so the never-reuse rules hold across
//! restores. The table only grows, is copied by backups and portable copies,
//! and is not part of the authored history or its receipt chain.

use rusqlite::{Connection, OptionalExtension, params};

use crate::StoreError;

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE retired_identities (
            kind TEXT NOT NULL CHECK (kind IN ('revision','generation_request','generation_scope_version','original_version')),
            key TEXT NOT NULL CHECK (length(CAST(key AS BLOB)) BETWEEN 1 AND 1024),
            value INTEGER NOT NULL DEFAULT 0 CHECK (value >= 0),
            PRIMARY KEY (kind, key)
        ) STRICT;",
    )?;
    Ok(())
}

/// Whether `key` of `kind` was issued before a restore discarded it.
pub(crate) fn contains(connection: &Connection, kind: &str, key: &str) -> Result<bool, StoreError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM retired_identities WHERE kind=?1 AND key=?2)",
        params![kind, key],
        |row| row.get(0),
    )?)
}

/// The highest value a restore discarded for `key`, or zero.
pub(crate) fn floor(connection: &Connection, kind: &str, key: &str) -> Result<i64, StoreError> {
    Ok(connection
        .query_row(
            "SELECT value FROM retired_identities WHERE kind=?1 AND key=?2",
            params![kind, key],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

/// Record in `copy` (an attached-free standalone database about to replace
/// `live`) everything `live` issued that `copy` would otherwise free again.
/// Runs in one transaction on `copy`.
pub(crate) fn carry_forward(live: &Connection, copy: &mut Connection) -> Result<(), StoreError> {
    fn strings(connection: &Connection, sql: &str) -> Result<Vec<String>, StoreError> {
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
    fn maxima(connection: &Connection, sql: &str) -> Result<Vec<(String, i64)>, StoreError> {
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
    let revisions = strings(
        live,
        "SELECT id FROM revisions
         UNION SELECT step_revision FROM transaction_steps
         UNION SELECT key FROM retired_identities WHERE kind='revision'",
    )?;
    let requests = strings(
        live,
        "SELECT request_id FROM generation_requests
         UNION SELECT key FROM retired_identities WHERE kind='generation_request'",
    )?;
    let scopes = maxima(
        live,
        "SELECT scope_id, MAX(high_water) FROM (
            SELECT scope_id, high_water FROM generation_scopes
            UNION ALL SELECT key, value FROM retired_identities WHERE kind='generation_scope_version'
         ) GROUP BY scope_id",
    )?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let originals = maxima(
        live,
        "SELECT content_id, MAX(version) FROM (
            SELECT content_id, version FROM original_media
            UNION ALL SELECT key, value FROM retired_identities WHERE kind='original_version'
         ) GROUP BY content_id",
    )?;
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let originals: Vec<(String, i64)> = Vec::new();
    let transaction = copy.transaction()?;
    {
        let mut kept = transaction.prepare(
            "INSERT OR IGNORE INTO retired_identities(kind,key,value)
             SELECT 'revision', ?1, 0
             WHERE NOT EXISTS(SELECT 1 FROM revisions WHERE id=?1)
               AND NOT EXISTS(SELECT 1 FROM transaction_steps WHERE step_revision=?1)",
        )?;
        for revision in &revisions {
            kept.execute([revision])?;
        }
        let mut request = transaction.prepare(
            "INSERT OR IGNORE INTO retired_identities(kind,key,value)
             SELECT 'generation_request', ?1, 0
             WHERE NOT EXISTS(SELECT 1 FROM generation_requests WHERE request_id=?1)",
        )?;
        for id in &requests {
            request.execute([id])?;
        }
        let mut high = transaction.prepare(
            "INSERT INTO retired_identities(kind,key,value) VALUES (?1,?2,?3)
             ON CONFLICT(kind,key) DO UPDATE SET value=MAX(value, excluded.value)",
        )?;
        for (scope, value) in &scopes {
            high.execute(params!["generation_scope_version", scope, value])?;
        }
        for (content, value) in &originals {
            high.execute(params!["original_version", content, value])?;
        }
    }
    crate::takes::carry_forward(live, &transaction)?;
    transaction.commit()?;
    Ok(())
}
