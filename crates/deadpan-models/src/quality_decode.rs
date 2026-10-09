//! Bounded inspection of the private canonical master. No worker path is opened.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_core::{AssetId, SourceFrameId};
use deadpan_jobs::{BridgeGenerationPlan, MotionAmount};
use deadpan_media::CanonicalMedia;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};

use super::{Accumulator, BridgeQualityReport, LumaGrid, MAX_FRAMES, invalid};
use crate::QualificationError;

pub(crate) fn measure(
    native: &CanonicalMedia,
    plan: &BridgeGenerationPlan,
    motion: MotionAmount,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<BridgeQualityReport, QualificationError> {
    let mut accumulator = Accumulator::new(plan, motion)?;
    let remaining = || {
        if cancelled.load(Ordering::Acquire) {
            return Err(QualificationError::Cancelled);
        }
        let duration = deadline.saturating_duration_since(Instant::now());
        if duration.is_zero() {
            Err(QualificationError::Deadline)
        } else {
            Ok(duration)
        }
    };
    let mut limits = SourceSessionLimits {
        opening_timeout: remaining()?,
        maximum_index_frames: MAX_FRAMES as usize,
        maximum_index_bytes: 1024 * 1024,
        maximum_seek_frames: MAX_FRAMES as usize,
        ..SourceSessionLimits::default()
    };
    limits.decode.max_input_bytes = native.object().byte_length();
    limits.decode.max_frames = u64::from(MAX_FRAMES) * 3;
    limits.decode.max_pixels = deadpan_analysis::generation_quality::MAX_PIXELS;
    limits.decode.progressive_only = true;
    let input = native.verified_source_input().map_err(invalid)?;
    let asset = AssetId::new("bridge-quality-master").map_err(invalid)?;
    let mut session =
        SourceSession::open_input(input, asset, limits, cancelled).map_err(invalid)?;
    remaining()?;
    let contract = super::native_contract(plan);
    if session.index().index().frames().len() != contract.frames as usize
        || session.info().bwdif_fields
        || session.info().width != contract.width
        || session.info().height != contract.height
        || session.info().time_base_num != 1
        || session.info().time_base_den != 1000
    {
        return Err(invalid(
            "quality decoder differs from the canonical native contract",
        ));
    }
    for ordinal in 0..contract.frames {
        let frame = session
            .frame(
                SourceFrameId(u64::from(ordinal)),
                remaining()?.min(Duration::from_secs(60)),
                cancelled,
            )
            .map_err(invalid)?;
        remaining()?;
        if frame.sample_bits != 8
            || frame.width != contract.width
            || frame.height != contract.height
            || frame.metadata.pts != contract.matroska_pts(ordinal).map_err(invalid)?
        {
            return Err(invalid(
                "quality picture differs from its exact canonical frame identity",
            ));
        }
        let grid = LumaGrid::from_rgba(
            &frame.rgba,
            frame.width,
            frame.height,
            frame.row_stride_bytes,
        )
        .map_err(invalid)?;
        accumulator.push(grid)?;
        remaining()?;
    }
    accumulator.finish(plan, motion)
}
