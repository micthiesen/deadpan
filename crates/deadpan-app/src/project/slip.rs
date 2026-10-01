//! Captured Source Slip targets and correlated, uncommitted service replies.

use std::sync::Arc;

use deadpan_core::{FrameRange, NodeId, ProjectFrame, ProjectId, RevisionId, SourceSlipResolution};

use super::{CommittedEdit, SequenceScope, Workspace};

/// UI command entry stores `Option<CapturedTarget>`: None means never captured;
/// Err retains absence or ineligibility at entry and must never be recaptured.
pub type CapturedTarget = Result<Target, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub session: u64,
    pub project: ProjectId,
    pub base_revision: RevisionId,
    pub scope: SequenceScope,
    pub parent: NodeId,
    /// Selected direct child, retaining a Partition wrapper when present.
    pub node: NodeId,
    pub range: FrameRange,
    /// Real Edit cursor, which may lie outside the selected child's range.
    pub cursor: ProjectFrame,
}

impl Target {
    /// Capture an explicit selected identity, never the child nearest the cursor.
    /// UI focus and Visual-selection eligibility are checked by the caller.
    pub fn capture(
        workspace: &Workspace,
        scope: SequenceScope,
        node: Option<&NodeId>,
        cursor: ProjectFrame,
    ) -> CapturedTarget {
        let node = node.ok_or("Select a beat before opening Slip; no target was captured.")?;
        let view = scope.resolve(workspace)?;
        let parent = view.owner.clone();
        let range = target_range(workspace, &scope, &parent, node)?;
        let target = Self {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            base_revision: workspace.document.revision_id().clone(),
            scope,
            parent,
            node: node.clone(),
            range,
            cursor,
        };
        target.validate(workspace)?;
        Ok(target)
    }

    pub fn validate(&self, workspace: &Workspace) -> Result<(), String> {
        if self.session == 0
            || self.session != workspace.session
            || &self.project != workspace.document.project_id()
            || &self.base_revision != workspace.document.revision_id()
        {
            return Err("Project changed since Slip entry; reopen Slip.".into());
        }
        if self.cursor.0 < 0 || self.cursor.0 > workspace.plan.duration().frames() {
            return Err("Captured Slip cursor is outside the project.".into());
        }
        if target_range(workspace, &self.scope, &self.parent, &self.node)? != self.range {
            return Err("Captured Slip target range changed; reopen Slip.".into());
        }
        Ok(())
    }
}

fn target_range(
    workspace: &Workspace,
    scope: &SequenceScope,
    parent: &NodeId,
    node: &NodeId,
) -> Result<FrameRange, String> {
    let view = scope.resolve(workspace)?;
    if view.owner != parent {
        return Err("Slip parent differs from the captured Sequence scope.".into());
    }
    let index = view
        .children
        .iter()
        .position(|child| child == node)
        .ok_or("Slip target is not a direct child of the captured Sequence.")?;
    let start = workspace
        .document
        .source_splice_boundary(parent, index)
        .map_err(|e| e.to_string())?;
    let duration = workspace
        .plan
        .node_duration(node)
        .ok_or("Slip target is missing from its plan.")?;
    if duration.frames() == 0 {
        return Err("Slip requires a nonempty selected beat.".into());
    }
    let end = start
        .0
        .checked_add(duration.frames())
        .ok_or("Slip target range overflow.")?;
    FrameRange::new(start, ProjectFrame(end)).map_err(|e| e.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalId {
    pub session: u64,
    pub project: ProjectId,
    pub base_revision: RevisionId,
    pub draft: u64,
    pub change: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub target: Target,
    pub draft: u64,
    pub change: u64,
    pub delta_frames: i64,
}

impl Proposal {
    pub fn id(&self) -> ProposalId {
        ProposalId {
            session: self.target.session,
            project: self.target.project.clone(),
            base_revision: self.target.base_revision.clone(),
            draft: self.draft,
            change: self.change,
        }
    }
}

#[derive(Clone)]
pub struct Prepared {
    /// Use this exact admitted base for Work::Proposed; refresh may replace an Arc.
    pub base: Arc<Workspace>,
    pub target: Target,
    pub resolution: SourceSlipResolution,
    /// None is a successful zero-movement report. Display the committed base.
    pub snapshot: Option<Arc<deadpan_playback::Snapshot>>,
}

#[derive(Clone)]
pub struct ProposalUpdate {
    pub id: ProposalId,
    pub result: Result<Arc<Prepared>, String>,
}

#[derive(Clone, Debug)]
pub struct CommitUpdate {
    pub id: ProposalId,
    pub result: Result<CommittedEdit, String>,
}

/// Latest durable Slip success in this project session. Later failed commands
/// cannot erase it. Replacing/closing the session drops it; Undo does not.
/// Refresh failure is historical outcome data, not proof the current view is stale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitReceipt {
    pub id: ProposalId,
    pub committed: CommittedEdit,
    pub refresh_error: Option<String>,
}
