//! Inspect exact sampled endpoints and retained PNGs through private inputs.

use std::sync::atomic::AtomicBool;
#[cfg(test)]
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use deadpan_analysis::endpoint_quality::{PixelChannels, RgbView, compare};
use deadpan_core::{AssetId, SourceFrameId};
use deadpan_jobs::BridgeGenerationPlan;
use deadpan_media::CanonicalMedia;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};

use super::{
    BridgeEndpointReport, EndpointObservation, EndpointThresholds, MAX_SAMPLED_FRAMES, PROFILE,
    invalid, sampled_contract,
};
#[cfg(test)]
use crate::quality_input::decode_png;
use crate::quality_input::{Control, read_png};
use crate::{QualificationError, RetainedConditioning};

const MAX_PIXELS: u64 = 4096 * 4096;

pub(crate) fn measure(
    sampled: &CanonicalMedia,
    conditioning: &mut RetainedConditioning,
    plan: &BridgeGenerationPlan,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<BridgeEndpointReport, QualificationError> {
    let control = Control {
        deadline,
        cancelled,
    };
    let contract = sampled_contract(plan)?;
    let geometry = conditioning
        .context()
        .geometry()
        .copied()
        .ok_or_else(|| invalid("fresh candidates require captured conditioning geometry"))?;
    let mut limits = SourceSessionLimits {
        opening_timeout: control.remaining()?,
        maximum_index_frames: MAX_SAMPLED_FRAMES as usize,
        maximum_index_bytes: 16 * 1024 * 1024,
        maximum_seek_frames: MAX_SAMPLED_FRAMES as usize,
        ..SourceSessionLimits::default()
    };
    limits.decode.max_input_bytes = sampled.object().byte_length();
    limits.decode.max_frames = u64::from(MAX_SAMPLED_FRAMES) * 3;
    limits.decode.max_pixels = MAX_PIXELS;
    limits.decode.progressive_only = true;
    let input = sampled.verified_source_input().map_err(invalid)?;
    let asset = AssetId::new("bridge-endpoint-master").map_err(invalid)?;
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
            "endpoint decoder differs from the sampled contract",
        ));
    }
    let thresholds = EndpointThresholds::policy();
    let mut observations = Vec::with_capacity(2);
    let mut decoded = None;
    for (left, ordinal) in [(true, 0), (false, contract.frames - 1)] {
        if left || ordinal != 0 {
            drop(decoded.take());
            let result = session.frame(
                SourceFrameId(u64::from(ordinal)),
                control.remaining()?.min(Duration::from_secs(60)),
                cancelled,
            );
            control.remaining()?;
            decoded = Some(result.map_err(invalid)?);
        }
        // A one-frame Hold uses the same decoded picture for both joins.
        let frame = decoded.as_ref().expect("entry always decodes a picture");
        let pts = contract.matroska_pts(ordinal).map_err(invalid)?;
        if frame.sample_bits != 8
            || frame.width != contract.width
            || frame.height != contract.height
            || frame.metadata.pts != pts
        {
            return Err(invalid(
                "endpoint picture differs from its exact sampled identity",
            ));
        }
        let png = read_png(
            conditioning.boundary_mut(left),
            contract.width,
            contract.height,
            &control,
        )?;
        let original = RgbView::new(
            png.as_raw(),
            contract.width,
            contract.height,
            usize::try_from(contract.width)
                .map_err(invalid)?
                .checked_mul(3)
                .ok_or_else(|| invalid("PNG stride overflow"))?,
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
        let crop = geometry.presentation;
        let difference = compare(
            original,
            generated,
            [crop.x, crop.y, crop.width, crop.height],
            thresholds.gross_cell_difference,
        )
        .map_err(invalid)?;
        control.remaining()?;
        observations.push(EndpointObservation {
            sampled_frame: ordinal,
            sampled_pts: pts,
            mean_absolute_rgb_difference: difference.mean_absolute_rgb_difference,
            gross_cell_fraction: difference.gross_cell_fraction,
        });
    }
    let report = BridgeEndpointReport {
        schema_version: 1,
        profile: PROFILE.into(),
        sampled: contract,
        sampled_object: sampled.object().clone(),
        context_object: conditioning.receipt().manifest().object().clone(),
        left_object: conditioning.receipt().left().object().clone(),
        right_object: conditioning.receipt().right().object().clone(),
        geometry,
        thresholds,
        entry: observations[0],
        exit: observations[1],
    };
    report.validate(plan, sampled.object(), conditioning.receipt())?;
    report.validate_context(conditioning.context())?;
    Ok(report)
}

#[cfg(test)]
#[path = "endpoint_decode_tests.rs"]
mod tests;
