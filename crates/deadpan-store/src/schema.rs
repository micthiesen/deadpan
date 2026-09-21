use rusqlite::{Connection, limits::Limit};

use crate::StoreError;

pub const VERSION: u32 = 1;
pub const APPLICATION_ID: u32 = 0x4450_4e31;
pub const MAX_DOCUMENT_BYTES: usize = deadpan_core::MAX_DOCUMENT_JSON_BYTES;

pub fn configure(connection: &Connection) -> Result<(), StoreError> {
    connection.busy_timeout(std::time::Duration::from_millis(250))?;
    connection.set_limit(Limit::SQLITE_LIMIT_LENGTH, (MAX_DOCUMENT_BYTES * 4) as i32)?;
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.pragma_update(None, "trusted_schema", false)?;
    Ok(())
}

pub fn check_version(connection: &Connection) -> Result<(), StoreError> {
    let application: u32 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application != APPLICATION_ID {
        return Err(StoreError::WrongApplication);
    }
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version != VERSION {
        return Err(StoreError::UnsupportedSchema(version));
    }
    Ok(())
}

pub fn create(connection: &mut Connection) -> Result<(), StoreError> {
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE revisions (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES revisions(id),
            kind TEXT NOT NULL CHECK (kind IN ('initial','edit','undo','redo')),
            document TEXT NOT NULL CHECK (json_valid(document))
        ) STRICT;
        CREATE TABLE history (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER REFERENCES history(id),
            revision_id TEXT NOT NULL REFERENCES revisions(id),
            request TEXT NOT NULL CHECK (json_valid(request)),
            edit TEXT NOT NULL CHECK (json_valid(edit))
        ) STRICT;
        CREATE TABLE state (
            singleton INTEGER PRIMARY KEY CHECK (singleton=1),
            head_revision TEXT NOT NULL REFERENCES revisions(id),
            cursor INTEGER REFERENCES history(id)
        ) STRICT;
        CREATE TABLE redo (
            position INTEGER PRIMARY KEY,
            history_id INTEGER NOT NULL REFERENCES history(id)
        ) STRICT;
        CREATE INDEX revision_parent ON revisions(parent_id);
        CREATE INDEX history_revision ON history(revision_id);",
    )?;
    transaction.pragma_update(None, "application_id", APPLICATION_ID)?;
    transaction.pragma_update(None, "user_version", VERSION)?;
    transaction.commit()?;
    Ok(())
}
