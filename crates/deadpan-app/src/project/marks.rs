//! Revision-bound native letter marks. The reserved ID selects a keyboard mark;
//! its exact single-letter label confirms that the native namespace is intended.

use deadpan_core::{
    AssetId, ExactRatio, MarkId, NodeId, ProjectFrame, ProjectId, RevisionId, SourceQualificationId,
};

use super::{SequenceScope, Workspace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Id {
    pub ticket: u64,
    pub session: u64,
    pub project: ProjectId,
    pub revision: RevisionId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    Original {
        asset: AssetId,
        qualification: SourceQualificationId,
        /// A measured presentation ordinal, including the exact terminal boundary.
        ordinal: u64,
    },
    Edit {
        scope: SequenceScope,
        at: ProjectFrame,
        selected: Option<NodeId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Set { letter: char, location: Location },
    Jump { letter: char },
    Delete { letter: char },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub id: Id,
    pub operation: Operation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedLocation {
    Original {
        asset: AssetId,
        qualification: SourceQualificationId,
        ordinal: u64,
    },
    Edit {
        scope: SequenceScope,
        selected: Option<NodeId>,
        frame: ProjectFrame,
        exact_frame: ExactRatio,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub id: Id,
    pub letter: char,
    pub revision: RevisionId,
    pub refresh_error: Option<String>,
}

impl Saved {
    /// A later Undo has a fresh revision and cannot revive this warning.
    pub fn needs_refresh(&self, workspace: Option<&Workspace>) -> bool {
        workspace.is_some_and(|workspace| {
            workspace.session == self.id.session
                && workspace.document.project_id() == &self.id.project
                && workspace.document.revision_id() == &self.id.revision
                && workspace.document.revision_id() != &self.revision
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Saved(Saved),
    Jumped(ResolvedLocation),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    pub id: Id,
    pub result: Result<Outcome, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Update {
    pub reply: Option<Reply>,
    /// Independent of later queries and failures, including refresh failure.
    pub saved: Option<Saved>,
}

pub fn mark_id(letter: char) -> Result<MarkId, String> {
    if !letter.is_ascii_alphabetic() {
        return Err("A mark uses one letter: a–z or A–Z".into());
    }
    MarkId::new(format!("native-mark-{letter}")).map_err(|error| error.to_string())
}
