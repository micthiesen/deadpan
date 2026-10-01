//! Exact ripple edge trimming with a grow-only physical Source owner.

use serde::{Deserialize, Serialize};

use crate::source_edit::edge::{SourceEdgeCandidate, edge_candidate, edge_limits};
use crate::source_edit::{Admission, admit, invalid};
use crate::{
    AssetId, DocumentError, EditError, ExactFrameRange, ExactRatio, FrameDuration, FrameRange,
    NodeId, ProjectDocument, ProjectFrame, RootSoundOperation, SourceEditWindow, SourceNode,
    SourceQualificationId, TimeError,
};

mod apply;
pub(crate) use apply::apply;

/// Positive delta moves the named edge later in Original material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTrimEdge {
    In,
    Out,
}

/// Only ripple authoring is currently implemented here. Other serialized modes
/// fail explicitly; adjacent Roll uses its separate fixed-duration command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTrimMode {
    Ripple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTrimClamp {
    PictureStart,
    PictureEnd,
    MinimumOutputDuration,
    MinimumSelectedDuration,
}

/// An exact limiting boundary, including whether that boundary is admissible.
/// Selected time must remain positive, so its limiting endpoint is exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SourceTrimLimit {
    pub delta: ExactRatio,
    pub inclusive: bool,
    pub reason: SourceTrimClamp,
}

/// Resolution against one immutable document, not a media admission token.
/// Allocation and window coordinates are physical Source-local frames; `after`
/// coordinates include `physical_prefix`. Output and timing windows use the
/// project clock. Timing windows refer to the old document, before any prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceTrimResolution {
    pub parent: NodeId,
    pub target: NodeId,
    pub physical_source: NodeId,
    pub asset: AssetId,
    pub qualification: SourceQualificationId,
    pub slot: usize,
    pub edge: SourceTrimEdge,
    pub mode: SourceTrimMode,
    pub requested_delta_frames: i64,
    pub applied_delta_frames: i64,
    pub minimum_delta: SourceTrimLimit,
    pub maximum_delta: SourceTrimLimit,
    pub minimum_delta_frames: i64,
    pub maximum_delta_frames: i64,
    pub clamp: Option<SourceTrimClamp>,
    pub allocation_before: FrameRange,
    pub allocation_after: FrameRange,
    pub output_before: FrameRange,
    pub output_after: FrameRange,
    pub window_before: SourceEditWindow,
    pub window_after: SourceEditWindow,
    pub effective_before: SourceEditWindow,
    pub effective_after: SourceEditWindow,
    pub before: SourceNode,
    /// Media, editorial window and physical duration only. The atomic reducer
    /// also translates owner effects, bindings and marks when adding a prefix.
    pub after: SourceNode,
    pub physical_prefix: FrameDuration,
    /// A new neutral Partition is needed around a formerly direct Source.
    /// An existing Partition always retains its identity, including at full span.
    pub needs_wrapper: bool,
    pub duration_delta_frames: i64,
    pub root_operation: Option<RootSoundOperation>,
    pub target_timing_window: Option<ExactFrameRange>,
    pub suffix_timing_window: Option<ExactFrameRange>,
}

impl ProjectDocument {
    /// Resolve an In or Out movement under ordinary Sequence ancestors. The
    /// effective selected window must remain inside measured picture support;
    /// audio-only picture lead/tail and composite targets remain unsupported.
    /// A resolved zero is a preview result; the authored command rejects it.
    pub fn source_trim(
        &self,
        parent: &NodeId,
        node: &NodeId,
        edge: SourceTrimEdge,
        delta_frames: i64,
        mode: SourceTrimMode,
    ) -> Result<SourceTrimResolution, EditError> {
        let admission = admit(self, parent, node, "source trim")?;
        resolve(self, admission, edge, delta_frames, mode).map_err(Into::into)
    }
}

fn resolve(
    document: &ProjectDocument,
    admission: Admission<'_>,
    edge: SourceTrimEdge,
    requested: i64,
    mode: SourceTrimMode,
) -> Result<SourceTrimResolution, DocumentError> {
    let (minimum_delta, maximum_delta) = edge_limits(&admission, edge)?;
    let minimum_delta_frames = minimum_delta.minimum_integer()?;
    let maximum_delta_frames = maximum_delta.maximum_integer()?;
    if minimum_delta_frames > 0 || maximum_delta_frames < 0 {
        return Err(invalid(
            "source trim has no valid current whole-frame edge interval",
        ));
    }
    let applied = requested.clamp(minimum_delta_frames, maximum_delta_frames);
    let duration_delta = match edge {
        SourceTrimEdge::In => applied.checked_neg().ok_or(TimeError::Overflow)?,
        SourceTrimEdge::Out => applied,
    };
    let output_after = FrameRange::new(
        admission.output.start(),
        ProjectFrame(
            admission
                .output
                .end()
                .0
                .checked_add(duration_delta)
                .ok_or(TimeError::Overflow)?,
        ),
    )?;
    let total = document.duration()?.frames();
    total
        .checked_add(duration_delta)
        .ok_or(TimeError::Overflow)?;
    let SourceEdgeCandidate {
        allocation: allocation_after,
        effective: effective_after,
        window: window_after,
        prefix,
        source: after,
        needs_wrapper,
    } = edge_candidate(document, &admission, edge, applied)?;
    let root_operation = root_operation(admission.output, edge, applied)?;
    let target_timing_window = if edge == SourceTrimEdge::In && applied != 0 {
        let retained_start = admission
            .output
            .start()
            .0
            .checked_add(applied.max(0))
            .ok_or(TimeError::Overflow)?;
        Some(ExactFrameRange::new(
            ExactRatio::integer(retained_start),
            ExactRatio::integer(admission.output.end().0),
        )?)
    } else {
        None
    };
    let suffix_timing_window = if applied != 0 && admission.output.end().0 < total {
        Some(ExactFrameRange::new(
            ExactRatio::integer(admission.output.end().0),
            ExactRatio::integer(total),
        )?)
    } else {
        None
    };
    Ok(SourceTrimResolution {
        parent: admission.parent.clone(),
        target: admission.target.clone(),
        physical_source: admission.physical_source.clone(),
        asset: admission.asset.clone(),
        qualification: admission.qualification.clone(),
        slot: admission.slot,
        edge,
        mode,
        requested_delta_frames: requested,
        applied_delta_frames: applied,
        minimum_delta,
        maximum_delta,
        minimum_delta_frames,
        maximum_delta_frames,
        clamp: if requested < minimum_delta_frames {
            Some(minimum_delta.reason)
        } else if requested > maximum_delta_frames {
            Some(maximum_delta.reason)
        } else {
            None
        },
        allocation_before: admission.allocation,
        allocation_after,
        output_before: admission.output,
        output_after,
        window_before: admission.window,
        window_after,
        effective_before: admission.effective,
        effective_after,
        before: admission.source.clone(),
        after,
        physical_prefix: prefix,
        needs_wrapper,
        duration_delta_frames: duration_delta,
        root_operation,
        target_timing_window,
        suffix_timing_window,
    })
}

fn root_operation(
    output: FrameRange,
    edge: SourceTrimEdge,
    delta: i64,
) -> Result<Option<RootSoundOperation>, TimeError> {
    if delta == 0 {
        return Ok(None);
    }
    Ok(Some(match (edge, delta > 0) {
        (SourceTrimEdge::In, true) => RootSoundOperation::Delete {
            range: FrameRange::new(
                output.start(),
                ProjectFrame(
                    output
                        .start()
                        .0
                        .checked_add(delta)
                        .ok_or(TimeError::Overflow)?,
                ),
            )?,
        },
        (SourceTrimEdge::In, false) => RootSoundOperation::Insert {
            at: output.start(),
            duration: FrameDuration::new(delta.checked_neg().ok_or(TimeError::Overflow)?)?,
        },
        (SourceTrimEdge::Out, true) => RootSoundOperation::Insert {
            at: output.end(),
            duration: FrameDuration::new(delta)?,
        },
        (SourceTrimEdge::Out, false) => RootSoundOperation::Delete {
            range: FrameRange::new(
                ProjectFrame(
                    output
                        .end()
                        .0
                        .checked_add(delta)
                        .ok_or(TimeError::Overflow)?,
                ),
                output.end(),
            )?,
        },
    }))
}
