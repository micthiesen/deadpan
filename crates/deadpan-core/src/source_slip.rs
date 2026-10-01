//! Atomic source movement on an unchanged physical and delivery clock.

use serde::Serialize;

use crate::{
    AssetId, AssetRecord, DocumentError, DocumentErrorCode, EditError, EditErrorCode,
    EndpointPolicy, ExactFrameRange, ExactRatio, LinkRelation, NodeId, NodeKind, ProjectDocument,
    RetimePurpose, SourceAudioMapping, SourceEditWindow, SourceNode, SourceQualificationId,
    SourceSpan, SourceVideo, SourceVideoMapping, TimeError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceSlipClamp {
    PictureStart,
    PictureEnd,
}

/// Derived resolution of one immutable document. This is not media admission or
/// a reusable authoring token: applying a command resolves the current revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceSlipResolution {
    pub parent: NodeId,
    pub target: NodeId,
    pub physical_source: NodeId,
    pub asset: AssetId,
    pub qualification: SourceQualificationId,
    pub requested_delta_frames: i64,
    pub applied_delta_frames: i64,
    pub minimum_delta: ExactRatio,
    pub maximum_delta: ExactRatio,
    pub minimum_delta_frames: i64,
    pub maximum_delta_frames: i64,
    pub clamp: Option<SourceSlipClamp>,
    pub effective_window: SourceEditWindow,
    pub before: SourceNode,
    pub after: SourceNode,
}

impl ProjectDocument {
    /// Resolve a direct Source or one neutral unity Partition under ordinary
    /// Sequence ancestors. Positive delta selects later Original material.
    /// The host must verify the retained qualification against its stored receipt.
    pub fn source_slip(
        &self,
        parent: &NodeId,
        node: &NodeId,
        delta_frames: i64,
    ) -> Result<SourceSlipResolution, EditError> {
        self.validate()?;
        let Some(NodeKind::Sequence { children }) = self.nodes().get(parent).map(|n| &n.kind)
        else {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "source slip requires an ordinary Sequence parent",
            ));
        };
        let index = children
            .iter()
            .position(|child| child == node)
            .ok_or_else(|| {
                EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    "source slip target is not a direct child of the named Sequence",
                )
            })?;
        self.source_splice_boundary(parent, index)?;
        let (owner, allocation) = target(self, node)?;
        let NodeKind::Source { source } = &self.nodes()[owner].kind else {
            unreachable!()
        };
        resolve(
            self,
            source,
            Scope {
                parent,
                target: node,
                physical_source: owner,
                allocation,
            },
            delta_frames,
        )
        .map_err(Into::into)
    }
}

fn target<'a>(
    document: &'a ProjectDocument,
    node: &'a NodeId,
) -> Result<(&'a NodeId, ExactFrameRange), EditError> {
    let selected = &document.nodes()[node];
    match &selected.kind {
        NodeKind::Source { source } => Ok((
            node,
            ExactFrameRange::new(
                ExactRatio::ZERO,
                ExactRatio::integer(source.duration.frames()),
            )?,
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
            Ok((
                child,
                ExactFrameRange::new(
                    ExactRatio::integer(mapping.start().0),
                    ExactRatio::integer(mapping.end().0),
                )?,
            ))
        }
        _ => Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            "source slip requires a Source or one neutral unity Partition; nested, treated and repeated targets remain unsupported",
        )),
    }
}

struct Scope<'a> {
    parent: &'a NodeId,
    target: &'a NodeId,
    physical_source: &'a NodeId,
    allocation: ExactFrameRange,
}

fn resolve(
    document: &ProjectDocument,
    source: &SourceNode,
    scope: Scope<'_>,
    requested: i64,
) -> Result<SourceSlipResolution, DocumentError> {
    let window = source
        .edit_window
        .ok_or_else(|| invalid("source slip requires an explicit editorial window"))?;
    let effective = intersect(window_range(window), scope.allocation);
    if effective.start.compare(effective.end).is_ge() {
        return Err(invalid("source slip cannot select only endpoint padding"));
    }
    let effective_window = SourceEditWindow::new(effective.start, effective.end)?;
    let SourceVideo::Stream { asset, span } = &source.video else {
        return Err(invalid("source slip requires measured stream video"));
    };
    let record = &document.assets()[asset];
    let qualification = record
        .source_qualification
        .clone()
        .ok_or_else(|| invalid("source slip requires measured source qualification"))?;
    if record.video != Some(*span)
        || matches!(source.video_mapping, SourceVideoMapping::FitBeat)
        || source.video_mapping.endpoints() != EndpointPolicy::HoldAdjacent
    {
        return Err(invalid(
            "source slip requires the complete explicit video mapping with adjacent endpoint holds",
        ));
    }
    let video_start = source.video_mapping.start_frames();
    let video_frames = source.video_mapping.duration_frames(source.duration)?;
    let video_support = support(video_start, video_frames)?;
    if !contains(video_support, effective) {
        return Err(invalid(
            "source slip does not yet support an editorial window in audio-only picture lead or tail",
        ));
    }
    let picture_selection = intersect(window_range(window), video_support);
    if source.video_mapping.selection_frames(source.duration)? != picture_selection {
        return Err(invalid(
            "source slip picture selection disagrees with its editorial window",
        ));
    }
    let audio = linked_audio(document, source, record, asset, *span, window)?;
    let minimum_delta = video_start.checked_sub(effective.start)?;
    let maximum_delta = video_support.end.checked_sub(effective.end)?;
    let minimum_delta_frames =
        i64::try_from(minimum_delta.ceil()?).map_err(|_| TimeError::Overflow)?;
    let maximum_delta_frames =
        i64::try_from(maximum_delta.floor()).map_err(|_| TimeError::Overflow)?;
    if minimum_delta_frames > 0
        || maximum_delta_frames < 0
        || minimum_delta_frames > maximum_delta_frames
    {
        return Err(invalid(
            "source slip has no valid current whole-frame handle interval",
        ));
    }
    let applied = requested.clamp(minimum_delta_frames, maximum_delta_frames);
    let delta = ExactRatio::integer(applied);
    let mut after = source.clone();
    if applied != 0 {
        let start = video_start.checked_sub(delta)?;
        let next_support = support(start, video_frames)?;
        if !contains(next_support, effective) {
            return Err(invalid("source slip moved beyond measured picture support"));
        }
        after.video_mapping = SourceVideoMapping::SelectedPlacement {
            start,
            frames: video_frames,
            selection: intersect(window_range(window), next_support),
            endpoints: EndpointPolicy::HoldAdjacent,
        };
        after
            .video_mapping
            .selection_in_source(*span, source.duration)?;
        if let Some(audio) = audio {
            let start = audio.start.checked_sub(delta)?;
            let selected = selected_audio(
                window_range(window),
                support(start.checked_add(audio.offset)?, audio.frames)?,
            );
            after.audio_mapping = SourceAudioMapping::SelectedPlacement {
                start,
                frames: audio.frames,
                selection: ExactFrameRange {
                    start: selected.start.checked_sub(audio.offset)?,
                    end: selected.end.checked_sub(audio.offset)?,
                },
            };
            after.audio_mapping.selection_frames_with_offset(
                source.duration,
                source.audio_offset,
                document.presentation_basis().frame_rate,
            )?;
        }
    }
    Ok(SourceSlipResolution {
        parent: scope.parent.clone(),
        target: scope.target.clone(),
        physical_source: scope.physical_source.clone(),
        asset: asset.clone(),
        qualification,
        requested_delta_frames: requested,
        applied_delta_frames: applied,
        minimum_delta,
        maximum_delta,
        minimum_delta_frames,
        maximum_delta_frames,
        clamp: if requested < minimum_delta_frames {
            Some(SourceSlipClamp::PictureStart)
        } else if requested > maximum_delta_frames {
            Some(SourceSlipClamp::PictureEnd)
        } else {
            None
        },
        effective_window,
        before: source.clone(),
        after,
    })
}

struct AudioPlacement {
    start: ExactRatio,
    frames: ExactRatio,
    offset: ExactRatio,
}

fn linked_audio(
    document: &ProjectDocument,
    source: &SourceNode,
    record: &AssetRecord,
    asset: &AssetId,
    video_span: SourceSpan,
    window: SourceEditWindow,
) -> Result<Option<AudioPlacement>, DocumentError> {
    let Some(audio) = &source.audio else {
        return Ok(None);
    };
    if source.link != LinkRelation::Linked
        || &audio.asset != asset
        || record.audio != Some(audio.span)
        || matches!(source.audio_mapping, SourceAudioMapping::FitBeat)
    {
        return Err(invalid(
            "source slip requires linked full audio context from the same qualified asset with an explicit map",
        ));
    }
    let start = source.audio_mapping.start_frames();
    let frames = source.audio_mapping.duration_frames(source.duration)?;
    let video_clock = affine(
        video_span,
        source.video_mapping.start_frames(),
        source.video_mapping.duration_frames(source.duration)?,
    )?;
    if affine(audio.span, start, frames)? != video_clock {
        return Err(invalid(
            "source slip requires equal exact audio/video affine clocks before the independent audio offset",
        ));
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
        return Err(invalid(
            "source slip audio selection disagrees with its editorial window and offset",
        ));
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

fn support(start: ExactRatio, frames: ExactRatio) -> Result<ExactFrameRange, TimeError> {
    crate::source_mapping::validate_placement(start, frames)?;
    Ok(ExactFrameRange {
        start,
        end: start.checked_add(frames)?,
    })
}
fn window_range(window: SourceEditWindow) -> ExactFrameRange {
    ExactFrameRange {
        start: window.start(),
        end: window.end(),
    }
}
fn intersect(a: ExactFrameRange, b: ExactFrameRange) -> ExactFrameRange {
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
fn contains(outer: ExactFrameRange, inner: ExactFrameRange) -> bool {
    outer.start.compare(inner.start).is_le() && inner.end.compare(outer.end).is_le()
}
fn selected_audio(window: ExactFrameRange, available: ExactFrameRange) -> ExactFrameRange {
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
fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::SourceRangeInvalid, message)
}
