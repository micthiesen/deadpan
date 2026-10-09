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
        clean_aperture: info
            .clean_aperture
            .map(deadpan_render::CleanAperture::new)
            .transpose()?,
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
                ColorTransfer::Pq => Transfer::Pq,
                ColorTransfer::Hlg => Transfer::Hlg,
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
    // HDR sources arrive as RGBA64 so ten-bit PQ/HLG codes reach the shared
    // renderer unquantized; an eight-bit HDR picture is analysis-only.
    match (decoded.sample_bits, info.color.transfer) {
        (8, ColorTransfer::Bt709 | ColorTransfer::Srgb | ColorTransfer::Linear) => {
            Ok(Rgba8Frame::new(metadata, decoded.rgba)?)
        }
        (16, _) => Ok(Rgba8Frame::new_rgba16(metadata, decoded.rgba)?),
        _ => Err(ProjectPictureError::Limits(
            "HDR source pictures require sixteen-bit decoded samples",
        )),
    }
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

/// Crop a generated bridge picture, centered, to the aspect of `canvas`, the
/// artifact's recorded `content_aspect`.
///
/// Bridge conditioning fits each canvas-aspect boundary picture whole inside
/// the model's native raster (see `generation::conditioning`). Cropping the
/// generated picture back to the canvas aspect removes exactly those bars, so
/// the accepted Hold fills the canvas like its neighbours instead of showing
/// letterboxed inside it. Display aspect accounts for sample aspect ratio.
/// Both sides round with [`aspect_region`], so the crop and the conditioning
/// region agree to the pixel.
pub fn fill_canvas_aspect(
    frame: Rgba8Frame,
    canvas: [u32; 2],
) -> Result<Rgba8Frame, ProjectPictureError> {
    let metadata = *frame.metadata();
    let quarter_turned = matches!(
        metadata.rotation,
        Rotation::Clockwise90 | Rotation::Clockwise270
    );
    if quarter_turned || canvas[0] == 0 || canvas[1] == 0 {
        return Ok(frame);
    }
    let (width, height) = (metadata.width, metadata.height);
    let sar = metadata.sample_aspect_ratio;
    let [crop_width, crop_height] = aspect_region(
        canvas,
        [width, height],
        [sar.numerator(), sar.denominator()],
    );
    if (crop_width, crop_height) == (width, height) {
        return Ok(frame);
    }
    let (left, top) = ((width - crop_width) / 2, (height - crop_height) / 2);
    let stride = metadata.row_stride_bytes as usize;
    let mut bytes = Vec::with_capacity(crop_width as usize * crop_height as usize * 4);
    for row in frame
        .bytes()
        .chunks_exact(stride)
        .skip(top as usize)
        .take(crop_height as usize)
    {
        let start = left as usize * 4;
        bytes.extend_from_slice(&row[start..start + crop_width as usize * 4]);
    }
    Ok(Rgba8Frame::new(
        FrameMetadata {
            width: crop_width,
            height: crop_height,
            row_stride_bytes: crop_width * 4,
            ..metadata
        },
        bytes,
    )?)
}

/// The largest centered region of a `bounds` pixel raster, with pixel aspect
/// `sar` (numerator, denominator), whose display aspect is `aspect`'s. The
/// shorter side rounds half up in exact integer arithmetic and stays within
/// `1..=bound`. Bridge conditioning and the generated-picture crop share it.
pub fn aspect_region(aspect: [u32; 2], bounds: [u32; 2], sar: [u32; 2]) -> [u32; 2] {
    let [aspect_width, aspect_height] = aspect.map(|value| u128::from(value.max(1)));
    let [width, height] = bounds;
    let [sar_n, sar_d] = sar.map(|value| u128::from(value.max(1)));
    let half_up = |numerator: u128, denominator: u128, bound: u32| {
        u32::try_from((2 * numerator + denominator) / (2 * denominator))
            .unwrap_or(bound)
            .clamp(1, bound.max(1))
    };
    // Display width over height: width*sar_n / (height*sar_d) vs aspect.
    let raster = u128::from(width) * sar_n * aspect_height;
    let wanted = u128::from(height) * sar_d * aspect_width;
    if raster > wanted {
        // The raster is wider: keep its height.
        [
            half_up(
                u128::from(height) * sar_d * aspect_width,
                sar_n * aspect_height,
                width,
            ),
            height,
        ]
    } else if raster < wanted {
        [
            width,
            half_up(
                u128::from(width) * sar_n * aspect_height,
                sar_d * aspect_width,
                height,
            ),
        ]
    } else {
        bounds
    }
}
