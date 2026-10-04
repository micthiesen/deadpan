use rusqlite::{Connection, limits::Limit};

use crate::StoreError;

// Storage has operational tables beyond the independently versioned core JSON.
pub const VERSION: u32 = 60;
/// The one earlier schema a writer upgrades in place: 60 only adds the
/// `speech_activity` table, so a schema-59 package needs no backup.
pub const UPGRADABLE_VERSION: u32 = 59;
pub const APPLICATION_ID: u32 = 0x4450_4e31;
pub const MAX_DOCUMENT_BYTES: usize = deadpan_core::MAX_DOCUMENT_JSON_BYTES;

pub fn configure(connection: &Connection) -> Result<(), StoreError> {
    connection.busy_timeout(std::time::Duration::from_millis(250))?;
    connection.set_limit(Limit::SQLITE_LIMIT_LENGTH, (MAX_DOCUMENT_BYTES * 4) as i32)?;
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.pragma_update(None, "trusted_schema", false)?;
    connection.pragma_update(None, "fullfsync", true)?;
    Ok(())
}

pub fn check_version(connection: &Connection) -> Result<(), StoreError> {
    let version = read_version(connection)?;
    // Database schema 60 adds speech activity. Refuse prior unused development
    // packages before writable open or document parsing.
    if version != VERSION {
        return Err(StoreError::UnsupportedSchema(version));
    }
    Ok(())
}

/// Accept the current schema or the additively upgradable one, returning it.
pub fn check_openable_version(connection: &Connection) -> Result<u32, StoreError> {
    let version = read_version(connection)?;
    if version != VERSION && version != UPGRADABLE_VERSION {
        return Err(StoreError::UnsupportedSchema(version));
    }
    Ok(version)
}

/// Upgrade a schema-59 database to 60 in one immediate transaction. Current
/// databases are unchanged.
pub fn upgrade(connection: &Connection) -> Result<(), StoreError> {
    let transaction =
        rusqlite::Transaction::new_unchecked(connection, rusqlite::TransactionBehavior::Immediate)?;
    match read_version(&transaction)? {
        VERSION => return Ok(()),
        UPGRADABLE_VERSION => {}
        version => return Err(StoreError::UnsupportedSchema(version)),
    }
    crate::speech_activity::create_tables(&transaction)?;
    transaction.pragma_update(None, "user_version", VERSION)?;
    transaction.commit()?;
    Ok(())
}

pub fn read_version(connection: &Connection) -> Result<u32, StoreError> {
    let application: u32 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application != APPLICATION_ID {
        return Err(StoreError::WrongApplication);
    }
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    Ok(version)
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
            cursor INTEGER REFERENCES history(id),
            workflow TEXT NOT NULL DEFAULT 'generic' CHECK(workflow IN ('generic','single_source_v1'))
        ) STRICT;
        CREATE TABLE redo (
            position INTEGER PRIMARY KEY,
            history_id INTEGER NOT NULL REFERENCES history(id)
        ) STRICT;
        CREATE INDEX revision_parent ON revisions(parent_id);
        CREATE INDEX history_revision ON history(revision_id);",
    )?;
    crate::generation::create_tables(&transaction)?;
    crate::compound::create_tables(&transaction)?;
    crate::registers::create_tables(&transaction)?;
    crate::generation_attempts::create_tables(&transaction)?;
    crate::transcripts::create_tables(&transaction)?;
    crate::speech_activity::create_tables(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::render_jobs::create_tables(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::render_jobs::create_decision_table(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::publication::create_tables(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::original_media::create_tables(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::source_registration::create_tables(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::single_source::create_tables(&transaction)?;
    transaction.pragma_update(None, "application_id", APPLICATION_ID)?;
    transaction.pragma_update(None, "user_version", VERSION)?;
    transaction.commit()?;
    Ok(())
}
