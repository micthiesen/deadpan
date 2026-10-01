//! Exact edge geometry shared by ripple Trim and fixed-duration Roll.

use super::{
    Admission, contains, frame_range, intersect, invalid, selected_audio, support, window_range,
};
use crate::{
    DocumentError, EndpointPolicy, ExactFrameRange, ExactRatio, FrameDuration, FrameRange,
    ProjectDocument, ProjectFrame, SourceAudioMapping, SourceEditWindow, SourceNode,
    SourceTrimClamp, SourceTrimEdge, SourceTrimLimit, SourceVideoMapping, TimeError,
};

pub(crate) fn edge_limits(
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
    pub(crate) fn minimum_integer(self) -> Result<i64, TimeError> {
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
    pub(crate) fn maximum_integer(self) -> Result<i64, TimeError> {
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

pub(crate) struct SourceEdgeCandidate {
    pub allocation: FrameRange,
    pub effective: SourceEditWindow,
    pub window: SourceEditWindow,
    pub prefix: FrameDuration,
    pub source: SourceNode,
    pub needs_wrapper: bool,
}

pub(crate) fn edge_candidate(
    document: &ProjectDocument,
    admission: &Admission<'_>,
    edge: SourceTrimEdge,
    applied: i64,
) -> Result<SourceEdgeCandidate, DocumentError> {
    let delta = ExactRatio::integer(applied);
    let (start, end) = match edge {
        SourceTrimEdge::In => (delta, ExactRatio::ZERO),
        SourceTrimEdge::Out => (ExactRatio::ZERO, delta),
    };
    window_candidate(document, admission, start, end, ExactRatio::ZERO)
}

/// Simultaneously move both delivered endpoints and the common media clock.
/// Inputs are integral frame deltas; no intermediate Source or duration exists.
pub(crate) fn window_candidate(
    document: &ProjectDocument,
    admission: &Admission<'_>,
    in_delta: ExactRatio,
    out_delta: ExactRatio,
    slip_delta: ExactRatio,
) -> Result<SourceEdgeCandidate, DocumentError> {
    if [in_delta, out_delta, slip_delta]
        .iter()
        .all(|value| *value == ExactRatio::ZERO)
    {
        return Ok(SourceEdgeCandidate {
            allocation: admission.allocation,
            effective: admission.effective,
            window: admission.window,
            prefix: FrameDuration::ZERO,
            source: admission.source.clone(),
            needs_wrapper: false,
        });
    }
    let mut allocation = FrameRange::new(
        integral_frame(ExactRatio::integer(admission.allocation.start().0).checked_add(in_delta)?)?,
        integral_frame(ExactRatio::integer(admission.allocation.end().0).checked_add(out_delta)?)?,
    )?;
    let effective = ExactFrameRange {
        start: admission.effective.start().checked_add(in_delta)?,
        end: admission.effective.end().checked_add(out_delta)?,
    };
    let mut window = window_range(admission.window);
    // Retain hidden selected support independently at each integral crop edge.
    // Fractional padding moves with its corresponding delivered endpoint.
    if effective.start != ExactRatio::integer(allocation.start().0)
        || window.start.compare(effective.start).is_ge()
    {
        window.start = effective.start;
    }
    if effective.end != ExactRatio::integer(allocation.end().0)
        || window.end.compare(effective.end).is_le()
    {
        window.end = effective.end;
    }
    let video_start = admission.video_start.checked_sub(slip_delta)?;
    let video_support = support(video_start, admission.video_frames)?;
    if !contains(video_support, effective)
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
    if window != window_range(admission.window) || slip_delta != ExactRatio::ZERO {
        after.video_mapping = SourceVideoMapping::SelectedPlacement {
            start: video_start,
            frames: admission.video_frames,
            selection: intersect(window, video_support),
            endpoints: EndpointPolicy::HoldAdjacent,
        };
        if let Some(audio) = admission.audio {
            let start = audio.start.checked_sub(slip_delta)?;
            let selection = selected_audio(
                window,
                support(start.checked_add(audio.offset)?, audio.frames)?,
            );
            after.audio_mapping = SourceAudioMapping::SelectedPlacement {
                start,
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
    let needs_wrapper = admission.target == admission.physical_source
        && (allocation.start().0 != 0 || allocation.end().0 != after.duration.frames());
    Ok(SourceEdgeCandidate {
        allocation,
        effective: effective_after,
        window: window_after,
        prefix,
        source: after,
        needs_wrapper,
    })
}

fn integral_frame(value: ExactRatio) -> Result<ProjectFrame, TimeError> {
    let integer = value.floor();
    if ExactRatio::new(integer, 1)? != value {
        return Err(TimeError::InvalidRatio);
    }
    Ok(ProjectFrame(
        i64::try_from(integer).map_err(|_| TimeError::Overflow)?,
    ))
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
