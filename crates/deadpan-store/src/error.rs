use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Project package must have a .deadpan extension")]
    PackageExtension,
    #[error("Project storage entry is missing, has the wrong type, or is a symbolic link: {0}")]
    UnsafePath(PathBuf),
    #[error("Another process owns this project; close its writable session before editing")]
    AlreadyOpen,
    #[error("This project was opened read-only")]
    ReadOnly,
    #[error("Unsupported database schema {0}; the project has not been rewritten")]
    UnsupportedSchema(u32),
    #[error("This database is not a Deadpan project")]
    WrongApplication,
    #[error("Project revision {0} has already been used; supply a new revision ID")]
    RevisionReused(String),
    #[error("Revision conflict: expected {expected}, current revision is {current}")]
    RevisionConflict { expected: String, current: String },
    #[error("There is no edit to undo")]
    NothingToUndo,
    #[error("There is no edit to redo")]
    NothingToRedo,
    #[error("Project history is inconsistent: {0}")]
    History(String),
    #[error("Project integrity check failed: {0}")]
    Integrity(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Document(#[from] deadpan_core::DocumentError),
    #[error(transparent)]
    Edit(#[from] deadpan_core::EditError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl StoreError {
    /// Stable protocol codes shared by GUI and headless callers.
    pub fn code(&self) -> &'static str {
        match self {
            Self::AlreadyOpen => "ProjectAlreadyOpen",
            Self::ReadOnly => "ProjectReadOnly",
            Self::UnsupportedSchema(_) => "SchemaUnsupported",
            Self::RevisionReused(_) => "RevisionReused",
            Self::RevisionConflict { .. } => "RevisionConflict",
            Self::NothingToUndo => "NothingToUndo",
            Self::NothingToRedo => "NothingToRedo",
            Self::Edit(error) => error.code.as_str(),
            Self::Database(rusqlite::Error::SqliteFailure(error, _)) => match error.code {
                rusqlite::ErrorCode::DiskFull => "DiskFull",
                rusqlite::ErrorCode::ReadOnly => "ProjectReadOnly",
                rusqlite::ErrorCode::PermissionDenied => "PermissionDenied",
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => {
                    "ProjectBusy"
                }
                _ => "ProjectFailure",
            },
            Self::Io(error) => match error.kind() {
                std::io::ErrorKind::StorageFull => "DiskFull",
                std::io::ErrorKind::ReadOnlyFilesystem => "ProjectReadOnly",
                std::io::ErrorKind::PermissionDenied => "PermissionDenied",
                _ => "IoFailure",
            },
            _ => "ProjectFailure",
        }
    }
}
