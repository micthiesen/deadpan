//! Captured gain targets and read-only writer proposals. No UI or media work.

use std::sync::Arc;

use deadpan_core::{AudioTreatments, NodeId, ProjectFrame, ProjectId, RevisionId};

use super::{SequenceScope, Workspace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub session: u64,
    pub project: ProjectId,
    pub revision: RevisionId,
    pub scope: SequenceScope,
    pub node: NodeId,
    pub cursor: ProjectFrame,
    pub entry: AudioTreatments,
}

impl Target {
    pub fn validate(&self, workspace: &Workspace) -> Result<(), String> {
        if workspace.session != self.session
            || workspace.document.project_id() != &self.project
            || workspace.document.revision_id() != &self.revision
        {
            return Err("Project changed since gain editing began; reopen Gain.".into());
        }
        if self.cursor.0 < 0 || self.cursor.0 > workspace.plan.duration().frames() {
            return Err("Captured gain cursor is outside the project.".into());
        }
        if !self.scope.resolve(workspace)?.children.contains(&self.node) {
            return Err("Gain owner is not a direct child of the captured Sequence.".into());
        }
        let owner = workspace
            .document
            .nodes()
            .get(&self.node)
            .ok_or("The captured gain owner is missing.")?;
        if owner.audio_treatments != self.entry {
            return Err("The captured gain recipe changed; reopen Gain.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalId {
    pub session: u64,
    pub project: ProjectId,
    pub revision: RevisionId,
    pub draft: u64,
    pub change: u64,
}

#[derive(Clone)]
pub struct Proposal {
    pub target: Target,
    pub draft: u64,
    pub change: u64,
    pub treatments: AudioTreatments,
}

impl Proposal {
    pub fn id(&self) -> ProposalId {
        ProposalId {
            session: self.target.session,
            project: self.target.project.clone(),
            revision: self.target.revision.clone(),
            draft: self.draft,
            change: self.change,
        }
    }
}

#[derive(Clone)]
pub struct ProposalUpdate {
    pub id: ProposalId,
    pub result: Result<Arc<deadpan_playback::Snapshot>, String>,
}
