//! Session-local semantic edit receipts, independent of visible refreshes.

use deadpan_core::{FrameCut, ProjectId, RevisionId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CutAttempt {
    pub operation: FrameCut,
    /// Dot-repeat binds the exact service snapshot visible at input time.
    pub repeat_version: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastEdit {
    pub operation: FrameCut,
    pub register: Option<char>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub session: u64,
    pub project: ProjectId,
    pub version: u64,
    pub head: Option<RevisionId>,
    pub edit: Option<LastEdit>,
    pub error: Option<String>,
}

impl Snapshot {
    pub fn edit_for(&self, workspace: &super::Workspace) -> Result<&LastEdit, String> {
        if self.session != workspace.session || &self.project != workspace.document.project_id() {
            return Err("The repeat belongs to another project session.".into());
        }
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if self.head.as_ref() != Some(workspace.document.revision_id()) {
            return Err("The saved project changed. Reopen it before repeating an edit.".into());
        }
        self.edit.as_ref().ok_or_else(|| {
            "No repeatable edit is available. Cut frames first; other edits cannot be repeated yet."
                .into()
        })
    }
}
