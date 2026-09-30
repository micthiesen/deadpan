//! A service-issued, reversible linked Original placement proposal.

use std::ops::Range;
use std::sync::Arc;

use deadpan_core::{
    AssetId, FrameDuration, FrameRange, NodeId, ProjectId, RevisionId, SourceQualificationId,
};
use deadpan_plan::RenderPlan;

use super::{CommittedEdit, SequenceScope, Workspace};

/// Native draft/change counters are nonzero and increase within a project session.
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
    pub id: ProposalId,
    pub asset: AssetId,
    pub qualification: SourceQualificationId,
    pub ordinals: Range<u64>,
    pub scope: SequenceScope,
    pub parent: NodeId,
    pub destination: Destination,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    Slot(usize),
    Interior { target: NodeId, at: FrameDuration },
}

#[derive(Clone)]
pub struct Prepared {
    /// Exact committed base admitted by Snapshot::proposed, including media
    /// capabilities. A same-revision refresh may have replaced the UI's Arc.
    pub base: Arc<Workspace>,
    pub snapshot: Arc<deadpan_playback::Snapshot>,
    pub plan: Arc<RenderPlan>,
    pub node: NodeId,
    /// Exact proposed Edit interval occupied by the linked insertion.
    pub range: FrameRange,
}

#[derive(Clone)]
pub struct ProposalUpdate {
    pub id: ProposalId,
    pub result: Result<Arc<Prepared>, String>,
}

/// A successful durable receipt survives a subsequent preview refresh failure.
#[derive(Clone, Debug)]
pub struct SpliceCommitUpdate {
    pub id: ProposalId,
    pub result: Result<CommittedEdit, String>,
}
