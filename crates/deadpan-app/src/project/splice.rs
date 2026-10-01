//! A service-issued, reversible linked placement proposal.

use std::ops::Range;
use std::sync::Arc;

use deadpan_core::{
    AssetId, FrameDuration, FrameRange, NodeId, ProjectId, RevisionId, SourceQualificationId,
};
use deadpan_plan::RenderPlan;

use super::{CommittedEdit, SequenceScope, Workspace, slice};

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
    pub source: Source,
    pub scope: SequenceScope,
    pub parent: NodeId,
    pub destination: Destination,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Original {
        asset: AssetId,
        qualification: SourceQualificationId,
        ordinals: Range<u64>,
    },
    Edited {
        copied: Arc<slice::Captured>,
        /// Local refinement in the immutable source revision's global clock.
        range: FrameRange,
    },
}

impl Source {
    /// Original ordinals and historical Edit frames remain distinct source kinds.
    pub fn boundaries(&self) -> Result<Range<u64>, String> {
        match self {
            Self::Original { ordinals, .. } => Ok(ordinals.clone()),
            Self::Edited { range, .. } => Ok(u64::try_from(range.start().0)
                .map_err(|_| "Copied Edit In is negative")?
                ..u64::try_from(range.end().0).map_err(|_| "Copied Edit Out is negative")?),
        }
    }

    pub fn set_boundaries(&mut self, boundaries: Range<u64>) -> Result<(), String> {
        if boundaries.start >= boundaries.end {
            return Err("A slice must include at least one picture.".into());
        }
        match self {
            Self::Original { ordinals, .. } => *ordinals = boundaries,
            Self::Edited { range, .. } => {
                *range = FrameRange::new(
                    deadpan_core::ProjectFrame(
                        i64::try_from(boundaries.start).map_err(|_| "Copied Edit In overflows")?,
                    ),
                    deadpan_core::ProjectFrame(
                        i64::try_from(boundaries.end).map_err(|_| "Copied Edit Out overflows")?,
                    ),
                )
                .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    pub fn copied_view_id(&self) -> Option<slice::CopiedViewId> {
        match self {
            Self::Original { .. } => None,
            Self::Edited { copied, range } => Some(slice::CopiedViewId {
                copy: copied.id().clone(),
                parent: copied.slice().parent().clone(),
                range: *range,
            }),
        }
    }
}

#[derive(Clone)]
pub enum PreparedMedia {
    Original,
    Edited(Arc<slice::MediaView>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    Slot(usize),
    Interior {
        target: NodeId,
        at: FrameDuration,
    },
    /// Captured global Edit interval within the explicitly named Sequence.
    Replace {
        range: FrameRange,
    },
}

#[derive(Clone)]
pub struct Prepared {
    /// Exact committed base admitted by Snapshot::proposed, including media
    /// capabilities. A same-revision refresh may have replaced the UI's Arc.
    pub base: Arc<Workspace>,
    pub snapshot: Arc<deadpan_playback::Snapshot>,
    pub media: PreparedMedia,
    pub plan: Arc<RenderPlan>,
    pub node: NodeId,
    /// Exact proposed Edit interval occupied by the linked insertion.
    pub range: FrameRange,
    /// The exact interval removed from the committed base, if replacing.
    pub removed: Option<FrameRange>,
}

#[derive(Clone)]
pub struct ProposalUpdate {
    pub id: ProposalId,
    /// Valid source endpoints remain available when destination preflight fails.
    pub source_view: Option<slice::SourceViewUpdate>,
    pub result: Result<Arc<Prepared>, String>,
}

/// A successful durable receipt survives a subsequent preview refresh failure.
#[derive(Clone, Debug)]
pub struct SpliceCommitUpdate {
    pub id: ProposalId,
    pub result: Result<CommittedEdit, String>,
}
