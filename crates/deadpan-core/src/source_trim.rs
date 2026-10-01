//! Exact ripple edge trimming with a grow-only physical Source owner.

use serde::{Deserialize, Serialize};

use crate::source_edit::{
    Admission, admit, contains, frame_range, intersect, invalid, selected_audio, support,
    window_range,
};
use crate::{
    AssetId, DocumentError, EditError, EndpointPolicy, ExactFrameRange, ExactRatio, FrameDuration,
    FrameRange, NodeId, ProjectDocument, ProjectFrame, RootSoundOperation, SourceAudioMapping,
    SourceEditWindow, SourceNode, SourceQualificationId, SourceVideoMapping, TimeError,
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

/// Only ripple authoring is currently implemented. Other serialized modes fail
/// explicitly; overwrite and adjacent roll remain separate required operations.
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
    let (minimum_delta, maximum_delta) = limits(&admission, edge)?;
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
    let (allocation_after, effective_after, window_after, prefix, after) =
        candidate(document, &admission, edge, applied)?;
    let needs_wrapper = applied != 0
        && admission.target == admission.physical_source
        && (allocation_after.start().0 != 0 || allocation_after.end().0 != after.duration.frames());
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

fn limits(
    admission: &Admission<'_>,
    edge: SourceTrimEdge,
) -> Result<(SourceTrimLimit, SourceTrimLimit), TimeError> {
    let width = admission
        .effective
        .end()
        .checked_sub(admission.effective.start())?;
    let output = admission.allocation.duration().frames();
    let selected = SourceTrimLimit {
        delta: match edge {
            SourceTrimEdge::In => width,
            SourceTrimEdge::Out => ExactRatio::ZERO.checked_sub(width)?,
        },
        inclusive: false,
        reason: SourceTrimClamp::MinimumSelectedDuration,
    };
    let allocation = SourceTrimLimit {
        delta: ExactRatio::integer(match edge {
            SourceTrimEdge::In => output.checked_sub(1).ok_or(TimeError::Overflow)?,
            SourceTrimEdge::Out => 1i64.checked_sub(output).ok_or(TimeError::Overflow)?,
        }),
        inclusive: true,
        reason: SourceTrimClamp::MinimumOutputDuration,
    };
    Ok(match edge {
        SourceTrimEdge::In => (
            SourceTrimLimit {
                delta: admission
                    .video_support
                    .start
                    .checked_sub(admission.effective.start())?,
                inclusive: true,
                reason: SourceTrimClamp::PictureStart,
            },
            // At equal boundaries, positive selected width is the strict rule.
            if selected.delta.compare(allocation.delta).is_le() {
                selected
            } else {
                allocation
            },
        ),
        SourceTrimEdge::Out => (
            if selected.delta.compare(allocation.delta).is_ge() {
                selected
            } else {
                allocation
            },
            SourceTrimLimit {
                delta: admission
                    .video_support
                    .end
                    .checked_sub(admission.effective.end())?,
                inclusive: true,
                reason: SourceTrimClamp::PictureEnd,
            },
        ),
    })
}

impl SourceTrimLimit {
    fn minimum_integer(self) -> Result<i64, TimeError> {
        let value = if self.inclusive {
            self.delta.ceil()?
        } else {
            self.delta
                .floor()
                .checked_add(1)
                .ok_or(TimeError::Overflow)?
        };
        i64::try_from(value).map_err(|_| TimeError::Overflow)
    }
    fn maximum_integer(self) -> Result<i64, TimeError> {
        let value = if self.inclusive {
            self.delta.floor()
        } else {
            self.delta
                .ceil()?
                .checked_sub(1)
                .ok_or(TimeError::Overflow)?
        };
        i64::try_from(value).map_err(|_| TimeError::Overflow)
    }
}

type Candidate = (
    FrameRange,
    SourceEditWindow,
    SourceEditWindow,
    FrameDuration,
    SourceNode,
);

fn candidate(
    document: &ProjectDocument,
    admission: &Admission<'_>,
    edge: SourceTrimEdge,
    applied: i64,
) -> Result<Candidate, DocumentError> {
    if applied == 0 {
        return Ok((
            admission.allocation,
            admission.effective,
            admission.window,
            FrameDuration::ZERO,
            admission.source.clone(),
        ));
    }
    let delta = ExactRatio::integer(applied);
    let mut allocation = admission.allocation;
    let mut effective = window_range(admission.effective);
    let mut window = window_range(admission.window);
    match edge {
        SourceTrimEdge::In => {
            allocation = FrameRange::new(
                ProjectFrame(
                    allocation
                        .start()
                        .0
                        .checked_add(applied)
                        .ok_or(TimeError::Overflow)?,
                ),
                allocation.end(),
            )?;
            effective.start = effective.start.checked_add(delta)?;
            // Preserve hidden selected support when the delivered edge has no
            // fractional padding; otherwise move the authored fractional edge.
            window.start = if effective.start == ExactRatio::integer(allocation.start().0)
                && window.start.compare(effective.start).is_lt()
            {
                window.start
            } else {
                effective.start
            };
        }
        SourceTrimEdge::Out => {
            allocation = FrameRange::new(
                allocation.start(),
                ProjectFrame(
                    allocation
                        .end()
                        .0
                        .checked_add(applied)
                        .ok_or(TimeError::Overflow)?,
                ),
            )?;
            effective.end = effective.end.checked_add(delta)?;
            window.end = if effective.end == ExactRatio::integer(allocation.end().0)
                && window.end.compare(effective.end).is_gt()
            {
                window.end
            } else {
                effective.end
            };
        }
    }
    if !contains(admission.video_support, effective)
        || intersect(window, frame_range(allocation)) != effective
        || effective.start.compare(effective.end).is_ge()
    {
        return Err(invalid(
            "source trim moved beyond its exact selected picture interval",
        ));
    }
    // Temporary extension coordinates may be negative. Add the physical prefix
    // before constructing the persisted nonnegative SourceEditWindow.
    let prefix = FrameDuration::new(if allocation.start().0 < 0 {
        allocation
            .start()
            .0
            .checked_neg()
            .ok_or(TimeError::Overflow)?
    } else {
        0
    })?;
    let p = ExactRatio::integer(prefix.frames());
    allocation = FrameRange::new(
        ProjectFrame(
            allocation
                .start()
                .0
                .checked_add(prefix.frames())
                .ok_or(TimeError::Overflow)?,
        ),
        ProjectFrame(
            allocation
                .end()
                .0
                .checked_add(prefix.frames())
                .ok_or(TimeError::Overflow)?,
        ),
    )?;
    let mut after = admission.source.clone();
    after.duration = FrameDuration::new(
        after
            .duration
            .frames()
            .checked_add(prefix.frames())
            .ok_or(TimeError::Overflow)?
            .max(allocation.end().0),
    )?;
    let window_after =
        SourceEditWindow::new(window.start.checked_add(p)?, window.end.checked_add(p)?)?;
    window_after.validate(after.duration)?;
    let effective_after = SourceEditWindow::new(
        effective.start.checked_add(p)?,
        effective.end.checked_add(p)?,
    )?;
    after.edit_window = Some(window_after);
    if window != window_range(admission.window) {
        after.video_mapping = SourceVideoMapping::SelectedPlacement {
            start: admission.video_start,
            frames: admission.video_frames,
            selection: intersect(window, admission.video_support),
            endpoints: EndpointPolicy::HoldAdjacent,
        };
        if let Some(audio) = admission.audio {
            let selection = selected_audio(
                window,
                support(audio.start.checked_add(audio.offset)?, audio.frames)?,
            );
            after.audio_mapping = SourceAudioMapping::SelectedPlacement {
                start: audio.start,
                frames: audio.frames,
                selection: ExactFrameRange {
                    start: selection.start.checked_sub(audio.offset)?,
                    end: selection.end.checked_sub(audio.offset)?,
                },
            };
        }
    }
    if prefix != FrameDuration::ZERO {
        after.video_mapping = translate_video(after.video_mapping, p)?;
        if after.audio.is_some() {
            after.audio_mapping = translate_audio(after.audio_mapping, p)?;
        }
    }
    after
        .video_mapping
        .selection_in_source(admission.video_span, after.duration)?;
    if after.audio.is_some() {
        after.audio_mapping.selection_frames_with_offset(
            after.duration,
            after.audio_offset,
            document.presentation_basis().frame_rate,
        )?;
    }
    Ok((allocation, effective_after, window_after, prefix, after))
}

fn translate_video(
    mapping: SourceVideoMapping,
    prefix: ExactRatio,
) -> Result<SourceVideoMapping, TimeError> {
    Ok(match mapping {
        SourceVideoMapping::Duration { frames, endpoints } => SourceVideoMapping::Placement {
            start: prefix,
            frames,
            endpoints,
        },
        SourceVideoMapping::Placement {
            start,
            frames,
            endpoints,
        } => SourceVideoMapping::Placement {
            start: start.checked_add(prefix)?,
            frames,
            endpoints,
        },
        SourceVideoMapping::SelectedPlacement {
            start,
            frames,
            selection,
            endpoints,
        } => SourceVideoMapping::SelectedPlacement {
            start: start.checked_add(prefix)?,
            frames,
            selection: translate_range(selection, prefix)?,
            endpoints,
        },
        SourceVideoMapping::FitBeat => return Err(TimeError::InvalidRatio),
    })
}
fn translate_audio(
    mapping: SourceAudioMapping,
    prefix: ExactRatio,
) -> Result<SourceAudioMapping, TimeError> {
    Ok(match mapping {
        SourceAudioMapping::Duration { frames } => SourceAudioMapping::Placement {
            start: prefix,
            frames,
        },
        SourceAudioMapping::Placement { start, frames } => SourceAudioMapping::Placement {
            start: start.checked_add(prefix)?,
            frames,
        },
        SourceAudioMapping::SelectedPlacement {
            start,
            frames,
            selection,
        } => SourceAudioMapping::SelectedPlacement {
            start: start.checked_add(prefix)?,
            frames,
            selection: translate_range(selection, prefix)?,
        },
        SourceAudioMapping::FitBeat => return Err(TimeError::InvalidRatio),
    })
}
fn translate_range(
    range: ExactFrameRange,
    prefix: ExactRatio,
) -> Result<ExactFrameRange, TimeError> {
    Ok(ExactFrameRange {
        start: range.start.checked_add(prefix)?,
        end: range.end.checked_add(prefix)?,
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
