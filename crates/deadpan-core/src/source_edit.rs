//! Shared admission for exact linked edits on ordinary Source allocations.

pub(crate) mod edge;
mod editorial;
pub(crate) use editorial::mark_edges;

use crate::{
    AssetId, AssetRecord, DocumentError, DocumentErrorCode, EditError, EditErrorCode,
    EndpointPolicy, ExactFrameRange, ExactRatio, FrameRange, LinkRelation, NodeId, NodeKind,
    ProjectDocument, ProjectFrame, RetimePurpose, SourceAudioMapping, SourceEditWindow, SourceNode,
    SourceQualificationId, SourceSpan, SourceVideo, SourceVideoMapping, TimeError,
};

pub(crate) struct Admission<'a> {
    pub parent: &'a NodeId,
    pub target: &'a NodeId,
    pub physical_source: &'a NodeId,
    pub slot: usize,
    pub allocation: FrameRange,
    pub output: FrameRange,
    pub source: &'a SourceNode,
    pub asset: &'a AssetId,
    pub qualification: &'a SourceQualificationId,
    pub video_span: SourceSpan,
    pub video_start: ExactRatio,
    pub video_frames: ExactRatio,
    pub video_support: ExactFrameRange,
    pub window: SourceEditWindow,
    pub effective: SourceEditWindow,
    pub audio: Option<AudioPlacement>,
}

#[derive(Clone, Copy)]
pub(crate) struct AudioPlacement {
    pub start: ExactRatio,
    pub frames: ExactRatio,
    pub offset: ExactRatio,
}

/// Validate structure and linked full-span clocks without granting media authority.
/// The store still admits the immutable asset against its measured receipt.
pub(crate) fn admit<'a>(
    document: &'a ProjectDocument,
    parent: &'a NodeId,
    node: &'a NodeId,
    operation: &str,
) -> Result<Admission<'a>, EditError> {
    document.validate()?;
    let Some(NodeKind::Sequence { children }) = document.nodes().get(parent).map(|n| &n.kind)
    else {
        return Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            format!("{operation} requires an ordinary Sequence parent"),
        ));
    };
    let slot = children
        .iter()
        .position(|child| child == node)
        .ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                format!("{operation} target is not a direct child of the named Sequence"),
            )
        })?;
    let start = document.source_splice_boundary(parent, slot)?;
    let (physical_source, allocation) = target(document, node, operation)?;
    admit_source(
        document,
        Scope {
            parent,
            target: node,
            physical_source,
            slot,
            allocation,
            start,
        },
        operation,
    )
    .map_err(Into::into)
}

struct Scope<'a> {
    parent: &'a NodeId,
    target: &'a NodeId,
    physical_source: &'a NodeId,
    slot: usize,
    allocation: FrameRange,
    start: ProjectFrame,
}

fn admit_source<'a>(
    document: &'a ProjectDocument,
    scope: Scope<'a>,
    operation: &str,
) -> Result<Admission<'a>, DocumentError> {
    let Scope {
        parent,
        target: node,
        physical_source,
        slot,
        allocation,
        start,
    } = scope;
    let NodeKind::Source { source } = &document.nodes()[physical_source].kind else {
        unreachable!()
    };
    let output = FrameRange::new(
        start,
        ProjectFrame(
            start
                .0
                .checked_add(allocation.duration().frames())
                .ok_or(TimeError::Overflow)?,
        ),
    )?;
    let window = source.edit_window.ok_or_else(|| {
        invalid(&format!(
            "{operation} requires an explicit editorial window"
        ))
    })?;
    let effective = intersect(window_range(window), frame_range(allocation));
    if effective.start.compare(effective.end).is_ge() {
        return Err(invalid(&format!(
            "{operation} cannot select only endpoint padding"
        )));
    }
    let effective = SourceEditWindow::new(effective.start, effective.end)?;
    let SourceVideo::Stream { asset, span } = &source.video else {
        return Err(invalid(&format!(
            "{operation} requires measured stream video"
        )));
    };
    let record = &document.assets()[asset];
    let qualification = record.source_qualification.as_ref().ok_or_else(|| {
        invalid(&format!(
            "{operation} requires measured source qualification"
        ))
    })?;
    if record.video != Some(*span)
        || matches!(source.video_mapping, SourceVideoMapping::FitBeat)
        || source.video_mapping.endpoints() != EndpointPolicy::HoldAdjacent
    {
        return Err(invalid(&format!(
            "{operation} requires the complete explicit video mapping with adjacent endpoint holds",
        )));
    }
    let video_start = source.video_mapping.start_frames();
    let video_frames = source.video_mapping.duration_frames(source.duration)?;
    let video_support = support(video_start, video_frames)?;
    if !contains(video_support, window_range(effective)) {
        return Err(invalid(&format!(
            "{operation} does not yet support an editorial window in audio-only picture lead or tail",
        )));
    }
    if source.video_mapping.selection_frames(source.duration)?
        != intersect(window_range(window), video_support)
    {
        return Err(invalid(&format!(
            "{operation} picture selection disagrees with its editorial window",
        )));
    }
    let audio = linked_audio(document, source, record, asset, *span, window, operation)?;
    Ok(Admission {
        parent,
        target: node,
        physical_source,
        slot,
        allocation,
        output,
        source,
        asset,
        qualification,
        video_span: *span,
        video_start,
        video_frames,
        video_support,
        window,
        effective,
        audio,
    })
}

fn target<'a>(
    document: &'a ProjectDocument,
    node: &'a NodeId,
    operation: &str,
) -> Result<(&'a NodeId, FrameRange), EditError> {
    let selected = &document.nodes()[node];
    match &selected.kind {
        NodeKind::Source { source } => Ok((
            node,
            FrameRange::new(ProjectFrame(0), ProjectFrame(source.duration.frames()))
                .map_err(DocumentError::from)?,
        )),
        NodeKind::Retime {
            child,
            mapping,
            purpose: RetimePurpose::Partition,
            ..
        } if selected.framing.is_none()
            && selected.audio_treatments.is_empty()
            && matches!(document.nodes()[child].kind, NodeKind::Source { .. }) =>
        {
            Ok((child, *mapping))
        }
        _ => Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            format!(
                "{operation} requires a Source or one neutral unity Partition; nested, treated and repeated targets remain unsupported"
            ),
        )),
    }
}

fn linked_audio(
    document: &ProjectDocument,
    source: &SourceNode,
    record: &AssetRecord,
    asset: &AssetId,
    video_span: SourceSpan,
    window: SourceEditWindow,
    operation: &str,
) -> Result<Option<AudioPlacement>, DocumentError> {
    let Some(audio) = &source.audio else {
        return Ok(None);
    };
    if source.link != LinkRelation::Linked
        || &audio.asset != asset
        || record.audio != Some(audio.span)
        || matches!(source.audio_mapping, SourceAudioMapping::FitBeat)
    {
        return Err(invalid(&format!(
            "{operation} requires linked full audio context from the same qualified asset with an explicit map",
        )));
    }
    let start = source.audio_mapping.start_frames();
    let frames = source.audio_mapping.duration_frames(source.duration)?;
    let video_clock = affine(
        video_span,
        source.video_mapping.start_frames(),
        source.video_mapping.duration_frames(source.duration)?,
    )?;
    if affine(audio.span, start, frames)? != video_clock {
        return Err(invalid(&format!(
            "{operation} requires equal exact audio/video affine clocks before the independent audio offset",
        )));
    }
    let effective_start = source.audio_mapping.start_frames_with_offset(
        source.audio_offset,
        document.presentation_basis().frame_rate,
    )?;
    let offset = effective_start.checked_sub(start)?;
    if source.audio_mapping.selection_frames_with_offset(
        source.duration,
        source.audio_offset,
        document.presentation_basis().frame_rate,
    )? != selected_audio(window_range(window), support(effective_start, frames)?)
    {
        return Err(invalid(&format!(
            "{operation} audio selection disagrees with its editorial window and offset",
        )));
    }
    Ok(Some(AudioPlacement {
        start,
        frames,
        offset,
    }))
}

fn affine(
    span: SourceSpan,
    start: ExactRatio,
    frames: ExactRatio,
) -> Result<(ExactRatio, ExactRatio), TimeError> {
    let time_base = span.start().time_base;
    let seconds_per_tick = ExactRatio::new(
        i128::from(time_base.numerator()),
        i128::from(time_base.denominator()),
    )?;
    let first = ExactRatio::integer(span.start().ticks).checked_mul(seconds_per_tick)?;
    let extent =
        ExactRatio::integer(span.end().ticks - span.start().ticks).checked_mul(seconds_per_tick)?;
    let alpha = frames.checked_div(extent)?;
    Ok((alpha, start.checked_sub(alpha.checked_mul(first)?)?))
}

pub(crate) fn support(start: ExactRatio, frames: ExactRatio) -> Result<ExactFrameRange, TimeError> {
    crate::source_mapping::validate_placement(start, frames)?;
    Ok(ExactFrameRange {
        start,
        end: start.checked_add(frames)?,
    })
}
pub(crate) fn window_range(window: SourceEditWindow) -> ExactFrameRange {
    ExactFrameRange {
        start: window.start(),
        end: window.end(),
    }
}
pub(crate) fn frame_range(range: FrameRange) -> ExactFrameRange {
    ExactFrameRange {
        start: ExactRatio::integer(range.start().0),
        end: ExactRatio::integer(range.end().0),
    }
}
pub(crate) fn intersect(a: ExactFrameRange, b: ExactFrameRange) -> ExactFrameRange {
    ExactFrameRange {
        start: if a.start.compare(b.start).is_gt() {
            a.start
        } else {
            b.start
        },
        end: if a.end.compare(b.end).is_lt() {
            a.end
        } else {
            b.end
        },
    }
}
pub(crate) fn contains(outer: ExactFrameRange, inner: ExactFrameRange) -> bool {
    outer.start.compare(inner.start).is_le() && inner.end.compare(outer.end).is_le()
}
pub(crate) fn selected_audio(
    window: ExactFrameRange,
    available: ExactFrameRange,
) -> ExactFrameRange {
    let selected = intersect(window, available);
    if selected.start.compare(selected.end).is_lt() {
        return selected;
    }
    let point = if available.end.compare(window.start).is_le() {
        available.end
    } else {
        available.start
    };
    ExactFrameRange {
        start: point,
        end: point,
    }
}
pub(crate) fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::SourceRangeInvalid, message)
}
