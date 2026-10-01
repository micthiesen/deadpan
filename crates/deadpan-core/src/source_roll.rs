//! One exact adjacent seam movement with fixed pair and project duration.

use serde::Serialize;

use crate::source_edit::edge::{SourceEdgeCandidate, edge_candidate, edge_limits};
use crate::source_edit::{Admission, admit, invalid};
use crate::{
    AssetId, DocumentError, EditError, ExactRatio, FrameDuration, FrameRange, NodeId,
    ProjectDocument, ProjectFrame, SourceEditWindow, SourceNode, SourceQualificationId,
    SourceTrimClamp, SourceTrimEdge, SourceTrimLimit, TimeError,
};

mod apply;
pub(crate) use apply::apply;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRollSide {
    Left,
    Right,
}

/// One controlling exact bound. At equal boundaries, an exclusive constraint
/// wins; remaining equal ties report Left deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SourceRollLimit {
    pub side: SourceRollSide,
    pub delta: ExactRatio,
    pub inclusive: bool,
    pub reason: SourceTrimClamp,
}

impl SourceRollLimit {
    fn new(side: SourceRollSide, limit: SourceTrimLimit) -> Self {
        Self {
            side,
            delta: limit.delta,
            inclusive: limit.inclusive,
            reason: limit.reason,
        }
    }
    fn edge_limit(self) -> SourceTrimLimit {
        SourceTrimLimit {
            delta: self.delta,
            inclusive: self.inclusive,
            reason: self.reason,
        }
    }
}

/// Physical-local after coordinates include any positive physical prefix.
/// Output coordinates are absolute project frames, independently of source PTS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceRollSideResolution {
    pub target: NodeId,
    pub physical_source: NodeId,
    pub asset: AssetId,
    pub qualification: SourceQualificationId,
    pub slot: usize,
    pub allocation_before: FrameRange,
    pub allocation_after: FrameRange,
    pub output_before: FrameRange,
    pub output_after: FrameRange,
    pub window_before: SourceEditWindow,
    pub window_after: SourceEditWindow,
    pub effective_before: SourceEditWindow,
    pub effective_after: SourceEditWindow,
    pub before: SourceNode,
    /// Media/window/duration only; the reducer preserves owner effects and clocks.
    pub after: SourceNode,
    pub physical_prefix: FrameDuration,
    pub needs_wrapper: bool,
}

/// Descriptive resolution against one immutable document, not media authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceRollResolution {
    pub parent: NodeId,
    pub requested_delta_frames: i64,
    pub applied_delta_frames: i64,
    pub minimum_delta: SourceRollLimit,
    pub maximum_delta: SourceRollLimit,
    pub minimum_delta_frames: i64,
    pub maximum_delta_frames: i64,
    pub clamp: Option<SourceRollLimit>,
    pub pair_output: FrameRange,
    pub seam_before: ProjectFrame,
    pub seam_after: ProjectFrame,
    pub left: SourceRollSideResolution,
    pub right: SourceRollSideResolution,
}

impl ProjectDocument {
    /// Move the seam between two explicitly ordered, literally adjacent Source
    /// allocations under one ordinary Sequence. Positive delta moves it later.
    /// A zero result is a preview; authoring it is rejected without a revision.
    pub fn source_roll(
        &self,
        parent: &NodeId,
        left: &NodeId,
        right: &NodeId,
        delta_frames: i64,
    ) -> Result<SourceRollResolution, EditError> {
        let left = admit(self, parent, left, "source roll")?;
        let right = admit(self, parent, right, "source roll")?;
        resolve(self, left, right, delta_frames).map_err(Into::into)
    }
}

fn resolve(
    document: &ProjectDocument,
    left: Admission<'_>,
    right: Admission<'_>,
    requested: i64,
) -> Result<SourceRollResolution, DocumentError> {
    if left.slot.checked_add(1) != Some(right.slot)
        || left.target == right.target
        || left.output.end() != right.output.start()
    {
        return Err(invalid(
            "source roll requires two ordered, literally adjacent children",
        ));
    }
    let (left_min, left_max) = edge_limits(&left, SourceTrimEdge::Out)?;
    let (right_min, right_max) = edge_limits(&right, SourceTrimEdge::In)?;
    let minimum_delta = controlling(
        SourceRollLimit::new(SourceRollSide::Left, left_min),
        SourceRollLimit::new(SourceRollSide::Right, right_min),
        true,
    );
    let maximum_delta = controlling(
        SourceRollLimit::new(SourceRollSide::Left, left_max),
        SourceRollLimit::new(SourceRollSide::Right, right_max),
        false,
    );
    let minimum_delta_frames = minimum_delta.edge_limit().minimum_integer()?;
    let maximum_delta_frames = maximum_delta.edge_limit().maximum_integer()?;
    if minimum_delta_frames > 0 || maximum_delta_frames < 0 {
        return Err(invalid(
            "source roll has no valid current whole-frame interval",
        ));
    }
    let applied = requested.clamp(minimum_delta_frames, maximum_delta_frames);
    let seam_before = left.output.end();
    let seam_after = ProjectFrame(
        seam_before
            .0
            .checked_add(applied)
            .ok_or(TimeError::Overflow)?,
    );
    let pair_output = FrameRange::new(left.output.start(), right.output.end())?;
    let left_output = FrameRange::new(left.output.start(), seam_after)?;
    let right_output = FrameRange::new(seam_after, right.output.end())?;
    // No unary intermediate tree is constructed or checked for total overflow.
    let left_candidate = edge_candidate(document, &left, SourceTrimEdge::Out, applied)?;
    let right_candidate = edge_candidate(document, &right, SourceTrimEdge::In, applied)?;
    Ok(SourceRollResolution {
        parent: left.parent.clone(),
        requested_delta_frames: requested,
        applied_delta_frames: applied,
        minimum_delta,
        maximum_delta,
        minimum_delta_frames,
        maximum_delta_frames,
        clamp: if requested < minimum_delta_frames {
            Some(minimum_delta)
        } else if requested > maximum_delta_frames {
            Some(maximum_delta)
        } else {
            None
        },
        pair_output,
        seam_before,
        seam_after,
        left: side(left, left_candidate, left_output),
        right: side(right, right_candidate, right_output),
    })
}

fn controlling(left: SourceRollLimit, right: SourceRollLimit, lower: bool) -> SourceRollLimit {
    let comparison = left.delta.compare(right.delta);
    if (lower && comparison.is_lt())
        || (!lower && comparison.is_gt())
        || (comparison.is_eq() && left.inclusive && !right.inclusive)
    {
        right
    } else {
        left
    }
}

fn side(
    admission: Admission<'_>,
    candidate: SourceEdgeCandidate,
    output: FrameRange,
) -> SourceRollSideResolution {
    SourceRollSideResolution {
        target: admission.target.clone(),
        physical_source: admission.physical_source.clone(),
        asset: admission.asset.clone(),
        qualification: admission.qualification.clone(),
        slot: admission.slot,
        allocation_before: admission.allocation,
        allocation_after: candidate.allocation,
        output_before: admission.output,
        output_after: output,
        window_before: admission.window,
        window_after: candidate.window,
        effective_before: admission.effective,
        effective_after: candidate.effective,
        before: admission.source.clone(),
        after: candidate.source,
        physical_prefix: candidate.prefix,
        needs_wrapper: candidate.needs_wrapper,
    }
}
