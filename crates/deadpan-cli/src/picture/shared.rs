//! Small pure host adapters shared by native preview and fixed-revision workers.

use deadpan_core::{SourceFrameIndex, SourceTimeBase, SourceTimestamp};
use deadpan_plan::PictureFraming;
use deadpan_render::{
    FrameMetadata, FramingLayer, Primaries, Rgba8Frame, Rotation, SampleAspectRatio, SourceColor,
    Transfer,
};
use deadpan_source::{ColorPrimaries, ColorTransfer, DecodedRgbaFrame, SourceStreamInfo};

use super::ProjectPictureError;

/// Compare complete measured mappings while deliberately ignoring the authored
/// asset alias. Callers separately check that alias against their fixed revision.
/// Cancellation is observed between bounded chunks, including on cache hits.
pub fn same_index_mapping(
    left: &SourceFrameIndex,
    right: &SourceFrameIndex,
    mut cancelled: impl FnMut() -> bool,
) -> Result<bool, ProjectPictureError> {
    if cancelled() {
        return Err(ProjectPictureError::Cancelled);
    }
    if left.time_base() != right.time_base()
        || left.frames().len() != right.frames().len()
        || left.terminal_end() != right.terminal_end()
        || left.terminal_provenance() != right.terminal_provenance()
    {
        return Ok(false);
    }
    for (left, right) in left.frames().chunks(1024).zip(right.frames().chunks(1024)) {
        if cancelled() {
            return Err(ProjectPictureError::Cancelled);
        }
        if left != right {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Preserve decoded transfer, primaries, coded orientation, SAR and signed
/// original PTS. This adapter neither retimes nor applies editorial geometry.
pub fn source_to_render_frame(
    decoded: DecodedRgbaFrame,
    info: &SourceStreamInfo,
) -> Result<Rgba8Frame, ProjectPictureError> {
    if decoded.width != info.width || decoded.height != info.height {
        return Err(ProjectPictureError::DecodedDimensions);
    }
    let metadata = FrameMetadata {
        width: decoded.width,
        height: decoded.height,
        row_stride_bytes: u32::try_from(decoded.row_stride_bytes).map_err(|_| {
            ProjectPictureError::Limits("source row stride exceeds the renderer limit")
        })?,
        sample_aspect_ratio: SampleAspectRatio::new(
            info.sample_aspect_num,
            info.sample_aspect_den,
        )?,
        rotation: match info.rotation_quarter_turns {
            0 => Rotation::None,
            1 => Rotation::Clockwise90,
            2 => Rotation::Clockwise180,
            3 => Rotation::Clockwise270,
            _ => return Err(ProjectPictureError::Orientation),
        },
        color: SourceColor {
            transfer: match info.color.transfer {
                ColorTransfer::Bt709 => Transfer::Rec709,
                ColorTransfer::Srgb => Transfer::Srgb,
                ColorTransfer::Linear => Transfer::Linear,
            },
            primaries: match info.color.primaries {
                ColorPrimaries::Bt709 => Primaries::Rec709,
                ColorPrimaries::Bt2020 => Primaries::Rec2020,
                ColorPrimaries::DisplayP3 => Primaries::DisplayP3D65,
            },
        },
        pts: SourceTimestamp {
            ticks: decoded.metadata.pts,
            time_base: SourceTimeBase::new(info.time_base_num, info.time_base_den)?,
        },
    };
    Ok(Rgba8Frame::new(metadata, decoded.rgba)?)
}

/// Preserve every sampled provider-to-root scope, including identities and the
/// extra provider clip preceding a default Repeat gap's authored scopes.
pub fn render_layers(
    framing: &[PictureFraming],
    framing_gap: bool,
) -> Result<Vec<FramingLayer>, ProjectPictureError> {
    let count = framing
        .len()
        .checked_add(usize::from(framing_gap))
        .filter(|count| *count <= deadpan_render::MAX_FRAMING_SCOPES)
        .ok_or(ProjectPictureError::Limits(
            "framing scopes exceed the shared renderer bound",
        ))?;
    let mut layers = Vec::new();
    layers
        .try_reserve_exact(count)
        .map_err(|_| ProjectPictureError::Limits("framing allocation failed"))?;
    if framing_gap {
        layers.push(FramingLayer::identity());
    }
    for layer in framing {
        layers.push(match layer.pose {
            Some(pose) => FramingLayer::new([pose.center_x, pose.center_y], pose.scale)?,
            None => FramingLayer::identity(),
        });
    }
    Ok(layers)
}
