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
    #[error(
        "Project schema {0} requires migration; run `deadpan-cli project migrate <project.deadpan>` to back up and upgrade its complete history"
    )]
    MigrationRequired(u32),
    #[error(
        "Migration promotion could not acquire the SQLite write lock; the original project remains unchanged"
    )]
    MigrationBusy,
    #[error("Migration failed; retained backup at {backup}: {source}")]
    MigrationFailed {
        backup: PathBuf,
        #[source]
        source: Box<StoreError>,
    },
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
    #[error("Generation request ID {0} has already been used")]
    GenerationRequestReused(String),
    #[error("Generation target is invalid: {0}")]
    GenerationTarget(String),
    #[error("Current generation requests require an explicit complete relevance plan")]
    GenerationRelevanceRequired,
    #[error("Generation relevance plan is invalid: {0}")]
    GenerationPlan(String),
    #[error("Generation request versions are exhausted for Hold {0}")]
    GenerationVersionExhausted(String),
    #[error("Generation attempt {attempt} has already been used for request {request}")]
    GenerationAttemptReused { request: String, attempt: String },
    #[error("Generation attempt {attempt} does not exist for request {request}")]
    GenerationAttemptNotFound { request: String, attempt: String },
    #[error("Generation attempt ordinals are exhausted for request {0}")]
    GenerationAttemptExhausted(String),
    #[error("Generation progress is live UI data and is not persisted")]
    GenerationProgressNotPersistent,
    #[error("Generation attempt transition is invalid: {0}")]
    GenerationAttempt(String),
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[error(transparent)]
    GeneratedMedia(#[from] crate::generated_media::GeneratedMediaError),
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
            Self::MigrationRequired(_) => "MigrationRequired",
            Self::MigrationBusy => "ProjectBusy",
            Self::MigrationFailed { source, .. } => match source.code() {
                code @ ("DiskFull" | "PermissionDenied" | "ProjectBusy" | "ProjectReadOnly"
                | "IoFailure") => code,
                _ => "MigrationFailed",
            },
            Self::RevisionReused(_) => "RevisionReused",
            Self::RevisionConflict { .. } => "RevisionConflict",
            Self::NothingToUndo => "NothingToUndo",
            Self::NothingToRedo => "NothingToRedo",
            Self::GenerationRequestReused(_) => "GenerationRequestReused",
            Self::GenerationTarget(_) => "GenerationTargetInvalid",
            Self::GenerationRelevanceRequired => "GenerationRelevanceRequired",
            Self::GenerationPlan(_) => "GenerationPlanInvalid",
            Self::GenerationVersionExhausted(_) => "GenerationVersionExhausted",
            Self::GenerationAttemptReused { .. } => "GenerationAttemptReused",
            Self::GenerationAttemptNotFound { .. } => "GenerationAttemptNotFound",
            Self::GenerationAttemptExhausted(_) => "GenerationAttemptExhausted",
            Self::GenerationProgressNotPersistent => "GenerationProgressNotPersistent",
            Self::GenerationAttempt(_) => "GenerationAttemptInvalid",
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            Self::GeneratedMedia(error) => error.code(),
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
