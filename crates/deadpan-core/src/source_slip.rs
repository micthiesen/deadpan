//! Atomic source movement on an unchanged physical and delivery clock.

use serde::Serialize;

use crate::source_edit::{
    Admission, admit, contains, intersect, invalid, selected_audio, support, window_range,
};
use crate::{
    AssetId, DocumentError, EditError, EndpointPolicy, ExactFrameRange, ExactRatio, NodeId,
    ProjectDocument, SourceAudioMapping, SourceEditWindow, SourceNode, SourceQualificationId,
    SourceVideoMapping, TimeError,
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
        let admission = admit(self, parent, node, "source slip")?;
        resolve(self, admission, delta_frames).map_err(Into::into)
    }
}

fn resolve(
    document: &ProjectDocument,
    admission: Admission<'_>,
    requested: i64,
) -> Result<SourceSlipResolution, DocumentError> {
    let source = admission.source;
    let window = admission.window;
    let effective = window_range(admission.effective);
    let video_start = admission.video_start;
    let video_frames = admission.video_frames;
    let video_support = admission.video_support;
    let audio = admission.audio;
    let asset = admission.asset;
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
            .selection_in_source(admission.video_span, source.duration)?;
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
        parent: admission.parent.clone(),
        target: admission.target.clone(),
        physical_source: admission.physical_source.clone(),
        asset: asset.clone(),
        qualification: admission.qualification.clone(),
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
        effective_window: admission.effective,
        before: source.clone(),
        after,
    })
}
