//! Current-format validation through the historical migration entrypoint.
//! Unsupported development packages are refused before any writable open.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

use crate::{
    AccessMode, ProjectStore, StoreError, read_flags, require_regular_file, schema,
    validate_extension,
};

#[derive(Debug, Serialize)]
pub struct MigrationOutcome {
    pub from_schema: u32,
    pub to_schema: u32,
    pub backup: Option<PathBuf>,
}

impl ProjectStore {
    /// Validate a current package without obtaining a writer or creating a
    /// backup. A schema-59 or 60 package is upgraded in place by a writer:
    /// schemas 60 and 61 only add empty tables, so no backup is needed. Earlier unused
    /// development formats have no supported migration.
    pub fn migrate(path: &Path) -> Result<MigrationOutcome, StoreError> {
        validate_extension(path)?;
        let package = std::fs::canonicalize(path)?;
        let database = package.join("project.sqlite");
        require_regular_file(&database)?;
        let probe = Connection::open_with_flags(&database, read_flags())?;
        schema::configure(&probe)?;
        let from_schema = schema::check_openable_version(&probe)?;
        drop(probe);
        let mode = if from_schema == schema::VERSION {
            AccessMode::ReadOnly
        } else {
            AccessMode::ReadWrite
        };
        Self::open(path, mode)?;
        Ok(MigrationOutcome {
            from_schema,
            to_schema: schema::VERSION,
            backup: None,
        })
    }
}
