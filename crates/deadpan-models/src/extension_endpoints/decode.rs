use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_analysis::endpoint_quality::{PixelChannels, RgbView, compare as compare_rgb};
use deadpan_analysis::generation_quality::{LumaGrid, compare as compare_luma};
use deadpan_core::{AssetId, SourceFrameId};
use deadpan_media::CanonicalMedia;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};

use super::*;
use crate::RetainedExtensionConditioning;
use crate::quality_input::{Control, read_png};

pub(crate) fn measure(
    sampled: &CanonicalMedia,
    conditioning: &mut RetainedExtensionConditioning,
    plan: &ExtensionGenerationPlan,
    motion: MotionAmount,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<ExtensionEndpointReport, QualificationError> {
    let control = Control {
        deadline,
        cancelled,
    };
    control.remaining()?;
    validate_binding(plan, conditioning.context(), conditioning.receipt())?;
    let contract = sampled_contract(plan)?;
    let presentation = conditioning.context().presentation();
    let mut limits = SourceSessionLimits {
        opening_timeout: control.remaining()?,
        maximum_index_frames: crate::endpoints::MAX_SAMPLED_FRAMES as usize,
        maximum_index_bytes: 16 * 1024 * 1024,
        maximum_seek_frames: crate::endpoints::MAX_SAMPLED_FRAMES as usize,
        ..SourceSessionLimits::default()
    };
    limits.decode.max_input_bytes = sampled.object().byte_length();
    limits.decode.max_frames = u64::from(crate::endpoints::MAX_SAMPLED_FRAMES) * 3;
    limits.decode.max_pixels = deadpan_analysis::generation_quality::MAX_PIXELS;
    limits.decode.progressive_only = true;
    let input = sampled.verified_source_input().map_err(invalid)?;
    let asset = AssetId::new("extension-endpoint-master").map_err(invalid)?;
    let opened = SourceSession::open_input(input, asset, limits, cancelled);
    control.remaining()?;
    let mut session = opened.map_err(invalid)?;
    if session.index().index().frames().len() != contract.frames as usize
        || session.info().bwdif_fields
        || session.info().width != contract.width
        || session.info().height != contract.height
        || session.info().time_base_num != 1
        || session.info().time_base_den != 1000
    {
        return Err(invalid(
            "extension endpoint decoder differs from its sampled contract",
        ));
    }
    for (ordinal, frame) in session.index().index().frames().iter().enumerate() {
        control.remaining()?;
        if frame.pts
            != contract
                .matroska_pts(u32::try_from(ordinal).map_err(invalid)?)
                .map_err(invalid)?
        {
            return Err(invalid(
                "extension sampled index differs from its exact clock",
            ));
        }
    }
    let thresholds = EndpointThresholds::policy();
    let mut observations = Vec::with_capacity(2);
    let mut decoded = None;
    for (entry, ordinal) in [(true, 0), (false, contract.frames - 1)] {
        control.remaining()?;
        let expected = expected_join(conditioning.context(), conditioning.receipt(), entry)?;
        let Some(expected) = expected else {
            observations.push(ExtensionEndpointJoin::Absent {});
            continue;
        };
        let role = expected.role;
        let picture = Box::new(expected.picture.clone());
        let object = expected.object.clone();
        let content = expected.content;
        // Decode at most twice. With N=1 both present joins share this actual
        // sampled picture, which may be an interpolated generated center.
        if decoded
            .as_ref()
            .is_none_or(|(previous, _)| *previous != ordinal)
        {
            drop(decoded.take());
            let result = session.frame(
                SourceFrameId(u64::from(ordinal)),
                control.remaining()?.min(Duration::from_secs(60)),
                cancelled,
            );
            control.remaining()?;
            decoded = Some((ordinal, result.map_err(invalid)?));
        }
        let frame = &decoded.as_ref().expect("present join decodes a picture").1;
        let pts = contract.matroska_pts(ordinal).map_err(invalid)?;
        if frame.sample_bits != 8
            || frame.width != contract.width
            || frame.height != contract.height
            || frame.metadata.pts != pts
        {
            return Err(invalid(
                "extension endpoint picture differs from its exact sampled identity",
            ));
        }
        let retained = if is_conditioned(plan, entry) {
            conditioning.anchor_mut()
        } else {
            conditioning
                .opposite_mut()
                .ok_or_else(|| invalid("opposite PNG is missing"))?
        };
        if retained.object() != &object {
            return Err(invalid("extension retained PNG differs from its receipt"));
        }
        let png = read_png(retained, contract.width, contract.height, &control)?;
        let png_stride = usize::try_from(contract.width)
            .map_err(invalid)?
            .checked_mul(3)
            .ok_or_else(|| invalid("PNG stride overflow"))?;
        let original = RgbView::new(
            png.as_raw(),
            contract.width,
            contract.height,
            png_stride,
            PixelChannels::Rgb,
        )
        .map_err(invalid)?;
        let generated = RgbView::new(
            &frame.rgba,
            frame.width,
            frame.height,
            frame.row_stride_bytes,
            PixelChannels::Rgba,
        )
        .map_err(invalid)?;
        let difference = compare_rgb(
            original,
            generated,
            [
                presentation.x,
                presentation.y,
                presentation.width,
                presentation.height,
            ],
            thresholds.gross_cell_difference,
        )
        .map_err(invalid)?;
        control.remaining()?;
        let boundary_grid = presentation_grid(png.as_raw(), png_stride, 3, presentation, &control)?;
        let sampled_grid = presentation_grid(
            &frame.rgba,
            frame.row_stride_bytes,
            4,
            presentation,
            &control,
        )?;
        let change = if entry {
            compare_luma(&boundary_grid, &sampled_grid)
        } else {
            compare_luma(&sampled_grid, &boundary_grid)
        };
        control.remaining()?;
        observations.push(ExtensionEndpointJoin::Measured {
            role,
            picture,
            object,
            content,
            endpoint: EndpointObservation {
                sampled_frame: ordinal,
                sampled_pts: pts,
                mean_absolute_rgb_difference: difference.mean_absolute_rgb_difference,
                gross_cell_fraction: difference.gross_cell_fraction,
            },
            quality: FrameObservation::new(ordinal, change),
        });
    }
    let mut observations = observations.into_iter();
    let report = ExtensionEndpointReport {
        schema_version: 1,
        profile: PROFILE.into(),
        plan: plan.clone(),
        sampled: contract,
        sampled_object: sampled.object().clone(),
        conditioning: conditioning.receipt().clone(),
        presentation,
        motion,
        thresholds,
        quality_thresholds: QualityThresholds::for_motion(motion),
        entry: observations.next().expect("entry was visited"),
        exit: observations.next().expect("exit was visited"),
    };
    report.validate(
        plan,
        sampled.object(),
        conditioning.context(),
        conditioning.receipt(),
        motion,
    )?;
    control.remaining()?;
    Ok(report)
}

/// Convert only the presentation rectangle to the shared fixed luma grid.
/// Callers first validate the full raster with RgbView and retained geometry.
fn presentation_grid(
    bytes: &[u8],
    stride: usize,
    channels: usize,
    crop: RasterRect,
    control: &Control<'_>,
) -> Result<LumaGrid, QualificationError> {
    control.remaining()?;
    let width = usize::try_from(crop.width).map_err(invalid)?;
    let height = usize::try_from(crop.height).map_err(invalid)?;
    let x = usize::try_from(crop.x).map_err(invalid)?;
    let y = usize::try_from(crop.y).map_err(invalid)?;
    let row_bytes = width
        .checked_mul(4)
        .ok_or_else(|| invalid("presentation stride overflow"))?;
    let length = row_bytes
        .checked_mul(height)
        .ok_or_else(|| invalid("presentation size overflow"))?;
    if u64::from(crop.width) * u64::from(crop.height)
        > deadpan_analysis::generation_quality::MAX_PIXELS
    {
        return Err(invalid("presentation exceeds the bounded luma input"));
    }
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(length).map_err(invalid)?;
    for row in 0..height {
        control.remaining()?;
        let start = y
            .checked_add(row)
            .and_then(|row| row.checked_mul(stride))
            .and_then(|base| {
                x.checked_mul(channels)
                    .and_then(|offset| base.checked_add(offset))
            })
            .ok_or_else(|| invalid("presentation offset overflow"))?;
        let end = width
            .checked_mul(channels)
            .and_then(|length| start.checked_add(length))
            .ok_or_else(|| invalid("presentation row overflow"))?;
        let pixels = bytes
            .get(start..end)
            .ok_or_else(|| invalid("presentation exceeds its raster"))?;
        for pixel in pixels.chunks_exact(channels) {
            rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
    }
    let grid = LumaGrid::from_rgba(&rgba, crop.width, crop.height, row_bytes).map_err(invalid)?;
    control.remaining()?;
    Ok(grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luma_checks_preserve_presentation_crop_and_ignore_padding() {
        let cancelled = AtomicBool::new(false);
        let control = Control {
            deadline: Instant::now() + Duration::from_secs(10),
            cancelled: &cancelled,
        };
        let crop = RasterRect::new(1, 1, 2, 2).unwrap();
        let mut rgb = vec![255; 4 * 4 * 3];
        let mut rgba = vec![0; 4 * 4 * 4];
        for y in 1..3 {
            for x in 1..3 {
                rgb[(y * 4 + x) * 3..][..3].copy_from_slice(&[10, 20, 30]);
                rgba[(y * 4 + x) * 4..][..4].copy_from_slice(&[10, 20, 30, 255]);
            }
        }
        let before = presentation_grid(&rgb, 12, 3, crop, &control).unwrap();
        let after = presentation_grid(&rgba, 16, 4, crop, &control).unwrap();
        assert_eq!(before, after);
        assert_eq!(compare_luma(&before, &after).mean_absolute_luma_change, 0.0);
        assert!(presentation_grid(&rgba[..10], 16, 4, crop, &control).is_err());
    }

    #[test]
    fn presentation_checks_stop_for_cancellation_and_deadline() {
        let cancelled = AtomicBool::new(true);
        let crop = RasterRect::new(0, 0, 1, 1).unwrap();
        let control = Control {
            deadline: Instant::now() + Duration::from_secs(10),
            cancelled: &cancelled,
        };
        assert!(matches!(
            presentation_grid(&[0, 0, 0], 3, 3, crop, &control),
            Err(QualificationError::Cancelled)
        ));
        cancelled.store(false, std::sync::atomic::Ordering::Release);
        let control = Control {
            deadline: Instant::now(),
            cancelled: &cancelled,
        };
        assert!(matches!(
            presentation_grid(&[0, 0, 0], 3, 3, crop, &control),
            Err(QualificationError::Deadline)
        ));
    }
}
