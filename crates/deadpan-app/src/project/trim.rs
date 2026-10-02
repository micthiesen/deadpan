//! Entry-bound combined Trim input and independently correlated service feedback.

use std::sync::Arc;

use deadpan_core::{
    FrameRange, NodeId, ProjectFrame, ProjectId, RevisionId, SourceTrimControl,
    SourceTrimEditResolution, SourceTrimGeometryLimit, SourceTrimIntent, SourceTrimPolicy,
    SourceTrimResultIdentities,
};

use super::{CommittedEdit, SequenceScope, Workspace};

/// Preserve Err at command/prefix entry; a later selection cannot supply a target.
pub type CapturedTarget = Result<Target, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub session: u64,
    pub project: ProjectId,
    pub base_revision: RevisionId,
    pub scope: SequenceScope,
    pub parent: NodeId,
    pub node: NodeId,
    /// Literal next child, including an ineligible/empty child or captured absence.
    pub right: Option<NodeId>,
    pub range: FrameRange,
    /// Real Edit cursor. Inspection and audition must never replace this value.
    pub cursor: ProjectFrame,
}

impl Target {
    /// Caller checks focused-pane and Visual-selection eligibility before capture.
    pub fn capture(
        workspace: &Workspace,
        scope: SequenceScope,
        node: Option<&NodeId>,
        cursor: ProjectFrame,
    ) -> CapturedTarget {
        let node = node.ok_or("Select a beat before opening Trim; no target was captured.")?;
        let view = scope.resolve(workspace)?;
        let parent = view.owner.clone();
        let index = view
            .children
            .iter()
            .position(|child| child == node)
            .ok_or("Trim target is not a direct child of the captured Sequence.")?;
        let right = view.children.get(index + 1).cloned();
        let geometry = workspace
            .document
            .source_trim_geometry(&parent, node, right.as_ref(), SourceTrimIntent::default())
            .map_err(|error| error.to_string())?;
        let target = Self {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            base_revision: workspace.document.revision_id().clone(),
            scope,
            parent,
            node: node.clone(),
            right,
            range: geometry.target.output_before,
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
            return Err("Project changed since Trim entry; reopen Trim.".into());
        }
        if self.cursor.0 < 0 || self.cursor.0 > workspace.plan.duration().frames() {
            return Err("Captured Trim cursor is outside the project.".into());
        }
        let view = self.scope.resolve(workspace)?;
        if view.owner != &self.parent {
            return Err("Trim parent differs from the captured Sequence scope.".into());
        }
        let index = view
            .children
            .iter()
            .position(|child| child == &self.node)
            .ok_or("Trim target is not a direct child of the captured Sequence.")?;
        if view.children.get(index + 1) != self.right.as_ref() {
            return Err("Captured Trim right sibling changed; reopen Trim.".into());
        }
        let geometry = workspace
            .document
            .source_trim_geometry(
                &self.parent,
                &self.node,
                self.right.as_ref(),
                SourceTrimIntent::default(),
            )
            .map_err(|error| error.to_string())?;
        if geometry.target.output_before != self.range {
            return Err("Captured Trim target range changed; reopen Trim.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalId {
    pub session: u64,
    pub project: ProjectId,
    pub base_revision: RevisionId,
    pub draft: u64,
    pub change: u64,
}

/// One service admission replays at most this many ordered events. The UI retains
/// later events in its own bounded queue and waits for the exact acknowledgment.
pub const MAX_EVENTS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Nudge {
        control: SourceTrimControl,
        frames: i64,
    },
    /// Absolute authored amount, still subject to the shared exact handle clamp.
    SetAmount {
        control: SourceTrimControl,
        frames: i64,
    },
    SetPolicy(SourceTrimPolicy),
    /// Resolve against the current accepted policy, including earlier refusals.
    TogglePolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub target: Target,
    pub draft: u64,
    pub change: u64,
    /// None starts a newer draft at zero Ripple; Some must name its last ack.
    pub previous_change: Option<u64>,
    /// Empty is valid for first admission or a fresh retry of failed preparation.
    /// Never sum these steps: +10 followed by -1 can differ from +9 at a clamp.
    pub events: Vec<Event>,
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

/// Compact geometry feedback; do not retain a whole candidate tree per event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdjustmentFeedback {
    pub control: SourceTrimControl,
    pub previous_value: i64,
    pub requested_step: i64,
    pub requested_value: i64,
    pub applied_value: i64,
    pub minimum: SourceTrimGeometryLimit,
    pub maximum: SourceTrimGeometryLimit,
    pub minimum_value: i64,
    pub maximum_value: i64,
    pub clamp: Option<SourceTrimGeometryLimit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventOutcome {
    pub event: Event,
    /// Refusal leaves the previous tuple intact; the next event starts here.
    pub accepted: SourceTrimIntent,
    pub adjustment: Option<AdjustmentFeedback>,
    /// Distinct from an applied source-handle clamp and from final admission.
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acknowledged {
    pub accepted: SourceTrimIntent,
    pub events: Vec<EventOutcome>,
}

#[derive(Clone)]
pub struct Prepared {
    /// Exact immutable entry Arc, even when a later refresh replaces the UI Arc.
    pub base: Arc<Workspace>,
    pub target: Target,
    pub accepted: SourceTrimIntent,
    pub resolution: SourceTrimEditResolution,
    pub result: SourceTrimResultIdentities,
    /// Preserved absolute entry cursor, or the explicit new end after contraction.
    pub cursor_after: ProjectFrame,
    pub cursor_clamped: bool,
    /// Zero returns None and creates neither a candidate nor an authored revision.
    pub snapshot: Option<Arc<deadpan_playback::Snapshot>>,
}

#[derive(Clone)]
pub struct ProposalUpdate {
    pub id: ProposalId,
    /// None means the envelope was rejected and no input prefix was consumed.
    /// Some remains meaningful even if receipt/candidate admission below failed.
    pub acknowledgment: Option<Acknowledged>,
    pub result: Result<Arc<Prepared>, String>,
}

#[derive(Clone, Debug)]
pub struct CommitUpdate {
    pub id: ProposalId,
    pub result: Result<CommittedEdit, String>,
}

/// Durable outcome independent of view refresh, later failed requests and Undo.
/// UI consumes a receipt only for its matching current applying transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitReceipt {
    pub id: ProposalId,
    pub committed: CommittedEdit,
    pub refresh_error: Option<String>,
}
