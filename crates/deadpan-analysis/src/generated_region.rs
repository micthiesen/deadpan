//! Conservative drift checks for one explicitly selected generated region.
//!
//! Seeds come from the same selected subject in the two retained conditioning
//! pictures. Crop and content rectangles are not subject seeds. Observations
//! retain the native tracker's output, without held or interpolated boxes.
//! Once tracking loses confidence, later observations cannot restart it.

use serde::{Deserialize, Serialize};

use crate::NormalizedRect;

pub const RAW_REGION_SCHEMA_VERSION: u32 = 1;
pub const PROFILE: &str = "deadpan-generated-region-1";
pub const MAX_NATIVE_FRAMES: usize = 1025;
const MAX_RASTER_EDGE: u32 = 4096;

/// The captured subject boxes in the actual left and right conditioning PNGs.
/// The host binds their source identities and capture geometry separately.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionSeeds {
    pub left: NormalizedRect,
    pub right: NormalizedRect,
}

impl RegionSeeds {
    pub fn validate(&self) -> Result<(), RawRegionError> {
        validate_rect(self.left)?;
        validate_rect(self.right)
    }
}

/// One raw result from a continuous tracker seeded on the left input.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum RegionObservation {
    Tracked {
        region: NormalizedRect,
        confidence: f32,
    },
    Unavailable {
        reason: RegionObservationUnavailableReason,
    },
}

impl RegionObservation {
    pub fn validate(&self) -> Result<(), RawRegionError> {
        if let Self::Tracked { region, confidence } = self {
            validate_rect(*region)?;
            if !confidence.is_finite() || !(0.0..=1.0).contains(confidence) {
                return Err(RawRegionError::Confidence);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionObservationUnavailableReason {
    Missing,
    InvalidGeometry,
    LowConfidence,
    LostTrack,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRegionFrame {
    pub ordinal: u32,
    pub pts: i64,
    pub observation: RegionObservation,
}

/// Exact coverage of left PNG, every native picture, and right PNG, in that
/// order. The right observation is the continuing track, never a new seed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRegionBatch {
    pub schema_version: u32,
    pub seeds: RegionSeeds,
    pub left: RegionObservation,
    pub frames: Vec<RawRegionFrame>,
    pub right: RegionObservation,
}

impl RawRegionBatch {
    pub fn validate(&self, expected_native_pts: &[i64]) -> Result<(), RawRegionError> {
        if self.schema_version != RAW_REGION_SCHEMA_VERSION {
            return Err(RawRegionError::SchemaVersion);
        }
        if !(2..=MAX_NATIVE_FRAMES).contains(&expected_native_pts.len()) {
            return Err(RawRegionError::NativeFrameLimit);
        }
        if expected_native_pts
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err(RawRegionError::ExpectedPtsOrder);
        }
        if self.frames.len() != expected_native_pts.len() {
            return Err(RawRegionError::FrameCount);
        }
        self.seeds.validate()?;
        self.left.validate()?;
        self.right.validate()?;
        for (index, frame) in self.frames.iter().enumerate() {
            if usize::try_from(frame.ordinal).ok() != Some(index) {
                return Err(RawRegionError::FrameOrdinal);
            }
            if frame.pts != expected_native_pts[index] {
                return Err(RawRegionError::FramePts);
            }
            frame.observation.validate()?;
        }
        Ok(())
    }
}

fn validate_rect(rect: NormalizedRect) -> Result<(), RawRegionError> {
    // NormalizedRect checks these at construction/deserialization. Check the
    // exact raster edges again because its rotation tolerance is not padding.
    if ![rect.x(), rect.y(), rect.width(), rect.height()]
        .iter()
        .all(|value| value.is_finite())
        || rect.x() < 0.0
        || rect.y() < 0.0
        || rect.width() <= 0.0
        || rect.height() <= 0.0
        || rect.x() + rect.width() > 1.0
        || rect.y() + rect.height() > 1.0
    {
        return Err(RawRegionError::RegionBounds);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawRegionError {
    SchemaVersion,
    NativeFrameLimit,
    ExpectedPtsOrder,
    FrameCount,
    FrameOrdinal,
    FramePts,
    RegionBounds,
    Confidence,
}

impl std::fmt::Display for RawRegionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SchemaVersion => "unsupported raw-region schema",
            Self::NativeFrameLimit => "native frame count is outside the region observation bound",
            Self::ExpectedPtsOrder => "expected native PTS values are not strictly increasing",
            Self::FrameCount => "raw region observations do not cover every native frame",
            Self::FrameOrdinal => "raw region frame ordinals are not contiguous",
            Self::FramePts => "raw region frame PTS differs from the canonical frame",
            Self::RegionBounds => "raw region is non-finite or outside the raster",
            Self::Confidence => "region confidence is outside [0, 1]",
        })
    }
}

impl std::error::Error for RawRegionError {}

/// Retained engineering thresholds, not guarantees of subject identity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionThresholds {
    /// Center displacement divided by the presentation-crop diagonal.
    pub center_residual: f64,
    /// Largest absolute log width, height, or aspect ratio residual.
    pub log_size_residual: f64,
    pub minimum_confidence: f64,
    pub sustained_frames: u32,
}

impl RegionThresholds {
    pub const fn policy() -> Self {
        Self {
            center_residual: 0.10,
            log_size_residual: 0.40,
            minimum_confidence: 0.70,
            sustained_frames: 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionCheckStatus {
    Measured,
    Rejected,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionUnavailableReason {
    Missing,
    InvalidGeometry,
    LowConfidence,
    LostTrack,
    Unsupported,
    OutsidePresentation,
    BoundarySeedMismatch,
}

impl From<RegionObservationUnavailableReason> for RegionUnavailableReason {
    fn from(value: RegionObservationUnavailableReason) -> Self {
        match value {
            RegionObservationUnavailableReason::Missing => Self::Missing,
            RegionObservationUnavailableReason::InvalidGeometry => Self::InvalidGeometry,
            RegionObservationUnavailableReason::LowConfidence => Self::LowConfidence,
            RegionObservationUnavailableReason::LostTrack => Self::LostTrack,
            RegionObservationUnavailableReason::Unsupported => Self::Unsupported,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionRejection {
    /// The start of the earliest run, not the frame that completed the run.
    pub first_ordinal: u32,
    pub consecutive_frames: u32,
    /// Residuals at the picture that first completed the sustained run.
    pub center_residual: f64,
    pub log_size_residual: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionBoundaryAssessment {
    pub measured: bool,
    pub center_residual: Option<f64>,
    pub log_size_residual: Option<f64>,
    pub unavailable_reason: Option<RegionUnavailableReason>,
}

impl RegionBoundaryAssessment {
    fn unavailable(reason: RegionUnavailableReason) -> Self {
        Self {
            measured: false,
            center_residual: None,
            log_size_residual: None,
            unavailable_reason: Some(reason),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedRegionReport {
    pub schema_version: u32,
    pub profile: String,
    pub native_frames: u32,
    pub thresholds: RegionThresholds,
    pub status: RegionCheckStatus,
    pub measured_frames: u32,
    pub unavailable_frames: u32,
    /// Tracking remains unavailable from this ordinal to the end.
    pub first_unavailable_ordinal: Option<u32>,
    pub left_boundary: RegionBoundaryAssessment,
    pub right_boundary: RegionBoundaryAssessment,
    pub maximum_center_residual: Option<f64>,
    pub maximum_log_size_residual: Option<f64>,
    pub rejection: Option<RegionRejection>,
    pub unavailable_reasons: Vec<RegionUnavailableReason>,
}

impl GeneratedRegionReport {
    /// Object hashes and captured seeds are independently bound by the host.
    /// This rejects alterations to the retained policy or derived coverage.
    pub fn validate_recomputed(
        &self,
        batch: &RawRegionBatch,
        expected_native_pts: &[i64],
        raster: [u32; 2],
        presentation: [u32; 4],
    ) -> Result<(), RegionError> {
        if *self != analyze(batch, expected_native_pts, raster, presentation)? {
            return Err(RegionError::PolicyMismatch);
        }
        Ok(())
    }
}

/// Evaluate the uninterrupted, qualified native prefix. The expected subject
/// path interpolates the captured endpoint seeds on the exact native PTS
/// clock, with the first and last native pictures at those endpoints. Loss or
/// a weak observation ends coverage permanently, preserving earlier rejection.
pub fn analyze(
    batch: &RawRegionBatch,
    expected_native_pts: &[i64],
    raster: [u32; 2],
    presentation: [u32; 4],
) -> Result<GeneratedRegionReport, RegionError> {
    batch.validate(expected_native_pts)?;
    validate_geometry(raster, presentation)?;
    let left = PixelRect::inside(batch.seeds.left, raster, presentation)
        .ok_or(RegionError::SeedsOutsidePresentation)?;
    let right = PixelRect::inside(batch.seeds.right, raster, presentation)
        .ok_or(RegionError::SeedsOutsidePresentation)?;
    let thresholds = RegionThresholds::policy();
    let left_boundary = assess_boundary(batch.left, left, raster, presentation, thresholds);
    let mut active = left_boundary.measured;
    let native_frames = u32::try_from(batch.frames.len()).map_err(|_| RegionError::Raster)?;
    let mut report = GeneratedRegionReport {
        schema_version: 1,
        profile: PROFILE.into(),
        native_frames,
        thresholds,
        status: RegionCheckStatus::Unavailable,
        measured_frames: 0,
        unavailable_frames: 0,
        first_unavailable_ordinal: None,
        left_boundary,
        right_boundary: RegionBoundaryAssessment::unavailable(RegionUnavailableReason::LostTrack),
        maximum_center_residual: None,
        maximum_log_size_residual: None,
        rejection: None,
        unavailable_reasons: Vec::new(),
    };
    if let Some(reason) = left_boundary.unavailable_reason {
        push_reason(&mut report.unavailable_reasons, reason);
    }
    // i128 subtraction preserves the exact difference even across i64 ends.
    let first_pts = i128::from(expected_native_pts[0]);
    let pts_span = i128::from(expected_native_pts[expected_native_pts.len() - 1]) - first_pts;
    let mut excess_run = 0;
    let mut run_start = 0;
    for frame in &batch.frames {
        let observed = qualified(frame.observation, raster, presentation, thresholds);
        if let Err(reason) = observed {
            push_reason(&mut report.unavailable_reasons, reason);
            active = false;
        }
        if !active {
            report.unavailable_frames += 1;
            report
                .first_unavailable_ordinal
                .get_or_insert(frame.ordinal);
            push_reason(
                &mut report.unavailable_reasons,
                RegionUnavailableReason::LostTrack,
            );
            excess_run = 0;
            continue;
        }
        let actual = observed.map_err(|_| RegionError::PolicyMismatch)?;
        let fraction = (i128::from(frame.pts) - first_pts) as f64 / pts_span as f64;
        let expected = left.interpolate(right, fraction);
        let (center, log_size) = residuals(actual, expected, presentation);
        report.measured_frames += 1;
        update_max(&mut report.maximum_center_residual, center);
        update_max(&mut report.maximum_log_size_residual, log_size);
        if center > thresholds.center_residual || log_size > thresholds.log_size_residual {
            if excess_run == 0 {
                run_start = frame.ordinal;
            }
            excess_run += 1;
            if excess_run >= thresholds.sustained_frames && report.rejection.is_none() {
                report.rejection = Some(RegionRejection {
                    first_ordinal: run_start,
                    consecutive_frames: excess_run,
                    center_residual: center,
                    log_size_residual: log_size,
                });
            }
        } else {
            excess_run = 0;
        }
    }
    // The terminal PNG can qualify the continued endpoint but cannot erase a
    // measured native prefix or authorize resumption after interrupted tracking.
    if active {
        report.right_boundary =
            assess_boundary(batch.right, right, raster, presentation, thresholds);
    } else if let Err(reason) = qualified(batch.right, raster, presentation, thresholds) {
        push_reason(&mut report.unavailable_reasons, reason);
    }
    if let Some(reason) = report.right_boundary.unavailable_reason {
        push_reason(&mut report.unavailable_reasons, reason);
    }
    report.status = if report.rejection.is_some() {
        RegionCheckStatus::Rejected
    } else if report.measured_frames != 0 {
        RegionCheckStatus::Measured
    } else {
        RegionCheckStatus::Unavailable
    };
    Ok(report)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionError {
    RawObservation(RawRegionError),
    Raster,
    Presentation,
    SeedsOutsidePresentation,
    PolicyMismatch,
}

impl From<RawRegionError> for RegionError {
    fn from(value: RawRegionError) -> Self {
        Self::RawObservation(value)
    }
}

impl std::fmt::Display for RegionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RawObservation(error) => error.fmt(f),
            Self::Raster => f.write_str("region raster dimensions are outside the bound"),
            Self::Presentation => {
                f.write_str("presentation rectangle is outside the region raster")
            }
            Self::SeedsOutsidePresentation => {
                f.write_str("selected region seeds extend outside the presentation crop")
            }
            Self::PolicyMismatch => {
                f.write_str("retained region result differs from recomputed policy")
            }
        }
    }
}

impl std::error::Error for RegionError {}

fn validate_geometry(raster: [u32; 2], presentation: [u32; 4]) -> Result<(), RegionError> {
    if raster
        .iter()
        .any(|edge| !(1..=MAX_RASTER_EDGE).contains(edge))
    {
        return Err(RegionError::Raster);
    }
    let [x, y, width, height] = presentation;
    if width == 0
        || height == 0
        || x.checked_add(width).is_none_or(|end| end > raster[0])
        || y.checked_add(height).is_none_or(|end| end > raster[1])
    {
        return Err(RegionError::Presentation);
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct PixelRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl PixelRect {
    fn inside(rect: NormalizedRect, raster: [u32; 2], crop: [u32; 4]) -> Option<Self> {
        let value = Self {
            x: rect.x() * f64::from(raster[0]) - f64::from(crop[0]),
            y: rect.y() * f64::from(raster[1]) - f64::from(crop[1]),
            width: rect.width() * f64::from(raster[0]),
            height: rect.height() * f64::from(raster[1]),
        };
        // Only floating-point roundoff at an exact crop edge is tolerated.
        let tolerance = 1e-9;
        (value.x >= -tolerance
            && value.y >= -tolerance
            && value.x + value.width <= f64::from(crop[2]) + tolerance
            && value.y + value.height <= f64::from(crop[3]) + tolerance)
            .then_some(value)
    }

    fn interpolate(self, other: Self, fraction: f64) -> Self {
        Self {
            x: lerp(self.x, other.x, fraction),
            y: lerp(self.y, other.y, fraction),
            width: lerp(self.width, other.width, fraction),
            height: lerp(self.height, other.height, fraction),
        }
    }
}

fn qualified(
    observation: RegionObservation,
    raster: [u32; 2],
    presentation: [u32; 4],
    thresholds: RegionThresholds,
) -> Result<PixelRect, RegionUnavailableReason> {
    match observation {
        RegionObservation::Tracked { region, confidence } => {
            // The native value is f32. Compare in that same domain so the
            // representable 0.70 boundary is admitted by a >= 0.70 policy.
            if confidence < thresholds.minimum_confidence as f32 {
                return Err(RegionUnavailableReason::LowConfidence);
            }
            PixelRect::inside(region, raster, presentation)
                .ok_or(RegionUnavailableReason::OutsidePresentation)
        }
        RegionObservation::Unavailable { reason } => Err(reason.into()),
    }
}

fn assess_boundary(
    observation: RegionObservation,
    seed: PixelRect,
    raster: [u32; 2],
    presentation: [u32; 4],
    thresholds: RegionThresholds,
) -> RegionBoundaryAssessment {
    let actual = match qualified(observation, raster, presentation, thresholds) {
        Ok(actual) => actual,
        Err(reason) => return RegionBoundaryAssessment::unavailable(reason),
    };
    let (center, log_size) = residuals(actual, seed, presentation);
    let measured = center <= thresholds.center_residual && log_size <= thresholds.log_size_residual;
    RegionBoundaryAssessment {
        measured,
        center_residual: Some(center),
        log_size_residual: Some(log_size),
        unavailable_reason: (!measured).then_some(RegionUnavailableReason::BoundarySeedMismatch),
    }
}

fn residuals(actual: PixelRect, expected: PixelRect, crop: [u32; 4]) -> (f64, f64) {
    let dx = actual.x + actual.width * 0.5 - expected.x - expected.width * 0.5;
    let dy = actual.y + actual.height * 0.5 - expected.y - expected.height * 0.5;
    let center = dx.hypot(dy) / f64::from(crop[2]).hypot(f64::from(crop[3]));
    let width = (actual.width / expected.width).ln().abs();
    let height = (actual.height / expected.height).ln().abs();
    let aspect = ((actual.width / actual.height) / (expected.width / expected.height))
        .ln()
        .abs();
    (center, width.max(height).max(aspect))
}

fn lerp(left: f64, right: f64, fraction: f64) -> f64 {
    left + (right - left) * fraction
}

fn update_max(value: &mut Option<f64>, next: f64) {
    *value = Some(value.unwrap_or(next).max(next));
}

fn push_reason(reasons: &mut Vec<RegionUnavailableReason>, reason: RegionUnavailableReason) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

#[cfg(test)]
mod tests;
