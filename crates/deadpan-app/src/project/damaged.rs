//! Failed-open recovery is bound to an attempt, never the currently selected path.

use std::path::PathBuf;
use std::sync::Arc;

use deadpan_core::ProjectId;
use deadpan_store::backups::{BackupInfo, BackupPreview};

#[derive(Debug, Clone)]
pub struct Offer {
    pub id: u64,
    pub path: PathBuf,
    pub error: String,
    pub backups: Vec<BackupInfo>,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub offer: u64,
    pub ticket: u64,
    pub action: Action,
}

#[derive(Debug, Clone)]
pub enum Action {
    Inspect {
        backup: String,
    },
    Restore {
        backup: String,
        confirmed_project: Option<ProjectId>,
    },
    Dismiss,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Inspected {
        preview: BackupPreview,
        requires_project_confirmation: bool,
    },
    Restored {
        quarantine: PathBuf,
        /// Replacement happened even when opening afterwards fails.
        open_error: Option<String>,
        warnings: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Reply {
    pub ticket: u64,
    pub result: Result<Outcome, String>,
}

#[derive(Debug, Clone)]
pub struct Update {
    pub offer: Arc<Offer>,
    pub reply: Option<Reply>,
}
