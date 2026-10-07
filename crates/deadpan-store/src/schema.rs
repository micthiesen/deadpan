use rusqlite::{Connection, limits::Limit};

use crate::StoreError;

// Storage has operational tables beyond the independently versioned core JSON.
pub const VERSION: u32 = 71;
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
    // Schema 71 proves AI pause insertion and accepted extension as distinct
    // durable preparation origins. Schema 70 queues conditioning after accepted Holds
    // outgrow their retained frames. Schema 69 gives AI requests explicit authoring scopes and
    // independent version clocks, with audited isolation events and scope
    // addresses. Unused development packages need no migration.
    // Schema 68 added AI variant retention records; 67 added
    // identities retired by restores (66 added
    // manual analysis corrections and resumable shot
    // scan progress (65 added the register bank digest; 64 keyframe metadata
    // and history receipts). Refuse prior unused development packages
    // before writable open or parsing.
    if version > VERSION {
        // A later Deadpan wrote this package. Writers and validators refuse;
        // `ProjectStore::open` can still show it read-only.
        return Err(StoreError::NewerSchema {
            found: version,
            supported: VERSION,
        });
    }
    if crate::migration::migratable(version) {
        // `ProjectStore::migrate` backs up and upgrades it; nothing written.
        return Err(StoreError::MigrationRequired(version));
    }
    if version != VERSION {
        return Err(StoreError::UnsupportedSchema(version));
    }
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
            document TEXT NOT NULL CHECK (json_valid(document)),
            depth INTEGER NOT NULL CHECK (depth>=0),
            json_bound INTEGER NOT NULL CHECK (json_bound>=0)
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
    crate::generation_preparations::create_tables(&transaction)?;
    crate::generation_retention::create_tables(&transaction)?;
    crate::transcripts::create_tables(&transaction)?;
    crate::speech_activity::create_tables(&transaction)?;
    crate::shot_analysis::create_tables(&transaction)?;
    crate::analysis_corrections::create_tables(&transaction)?;
    crate::revision_storage::create_tables(&transaction)?;
    crate::retired::create_tables(&transaction)?;
    crate::audit::create_tables(&transaction)?;
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
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::original_provenance::create_tables(&transaction)?;
    transaction.pragma_update(None, "application_id", APPLICATION_ID)?;
    transaction.pragma_update(None, "user_version", VERSION)?;
    transaction.commit()?;
    Ok(())
}
