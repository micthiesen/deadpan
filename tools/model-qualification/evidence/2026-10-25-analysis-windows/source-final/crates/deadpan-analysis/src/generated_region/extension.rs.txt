//! One-anchor region drift checks on generated extension pictures only.
//! The retained seed describes a quiet static subject. The opposite seam is
//! never a second seed, and context pictures never contribute measurements.

use serde::{Deserialize, Serialize};

use super::{
    NormalizedRect, PixelRect, RawRegionError, RawRegionFrame, RegionBoundaryAssessment,
    RegionCheckStatus, RegionError, RegionObservation, RegionRejection, RegionThresholds,
    RegionUnavailableReason, assess_boundary, push_reason, qualified, residuals, update_max,
    validate_geometry, validate_rect,
};
use crate::generated_extension::{CoverageError, ExtensionCoverage, MAX_GENERATED_FRAMES};

pub const RAW_EXTENSION_REGION_SCHEMA_VERSION: u32 = 1;
pub const PROFILE: &str = "deadpan-extension-region-1";
const MAX_UNAVAILABLE_REASONS: usize = 7;

/// One retained anchor and exact chronological generated coverage. Tracking
/// starts at the anchor and moves outward; FromRight therefore tracks these
/// stored pictures in reverse. Context and opposite-seam observations cannot
/// be included in this batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawExtensionRegionBatch {
    pub schema_version: u32,
    pub coverage: ExtensionCoverage,
    pub seed: NormalizedRect,
    pub anchor: RegionObservation,
    #[serde(deserialize_with = "bounded_frames")]
    pub frames: Vec<RawRegionFrame>,
}

impl RawExtensionRegionBatch {
    pub fn validate(&self, expected_native_pts: &[i64]) -> Result<(), ExtensionRegionError> {
        if self.schema_version != RAW_EXTENSION_REGION_SCHEMA_VERSION {
            return Err(RawRegionError::SchemaVersion.into());
        }
        self.coverage.validate(expected_native_pts)?;
        if self.frames.len() != (self.coverage.end - self.coverage.start) as usize {
            return Err(RawRegionError::FrameCount.into());
        }
        validate_rect(self.seed)?;
        self.anchor.validate()?;
        for (frame, ordinal) in self.frames.iter().zip(self.coverage.ordinals()) {
            if frame.ordinal != ordinal {
                return Err(RawRegionError::FrameOrdinal.into());
            }
            if frame.pts != expected_native_pts[ordinal as usize] {
                return Err(RawRegionError::FramePts.into());
            }
            frame.observation.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionRegionReport {
    pub schema_version: u32,
    pub profile: String,
    pub coverage: ExtensionCoverage,
    /// The full canonical movie count, including unmeasured context.
    pub native_frames: u32,
    pub generated_frames: u32,
    pub thresholds: RegionThresholds,
    pub status: RegionCheckStatus,
    pub measured_frames: u32,
    pub unavailable_frames: u32,
    /// First lost picture in outward tracking order, using its canonical
    /// chronological ordinal. FromRight stays lost toward lower ordinals.
    #[serde(deserialize_with = "required_option")]
    pub first_unavailable_ordinal: Option<u32>,
    #[serde(deserialize_with = "strict_boundary")]
    pub anchor: RegionBoundaryAssessment,
    #[serde(deserialize_with = "required_option")]
    pub maximum_center_residual: Option<f64>,
    #[serde(deserialize_with = "required_option")]
    pub maximum_log_size_residual: Option<f64>,
    /// `first_ordinal` is the chronological low ordinal of the earliest
    /// measured sustained run, even when tracking reaches it in reverse.
    #[serde(deserialize_with = "required_option")]
    pub rejection: Option<RegionRejection>,
    #[serde(deserialize_with = "bounded_reasons")]
    pub unavailable_reasons: Vec<RegionUnavailableReason>,
}

impl ExtensionRegionReport {
    /// Host code independently binds the selected target, anchor input and
    /// output object. Recompute every measurement and decision from the raw
    /// observations rather than trusting a serialized acceptance claim.
    pub fn validate_recomputed(
        &self,
        batch: &RawExtensionRegionBatch,
        expected_native_pts: &[i64],
        raster: [u32; 2],
        presentation: [u32; 4],
    ) -> Result<(), ExtensionRegionError> {
        if *self != analyze(batch, expected_native_pts, raster, presentation)? {
            return Err(RegionError::PolicyMismatch.into());
        }
        Ok(())
    }
}

/// Evaluate an uninterrupted track outward from the sole retained anchor.
/// Losing confidence or leaving the presentation ends coverage permanently.
/// A later loss cannot erase an already measured rejection.
pub fn analyze(
    batch: &RawExtensionRegionBatch,
    expected_native_pts: &[i64],
    raster: [u32; 2],
    presentation: [u32; 4],
) -> Result<ExtensionRegionReport, ExtensionRegionError> {
    batch.validate(expected_native_pts)?;
    validate_geometry(raster, presentation)?;
    let seed = PixelRect::inside(batch.seed, raster, presentation)
        .ok_or(RegionError::SeedsOutsidePresentation)?;
    let thresholds = RegionThresholds::policy();
    let anchor = assess_boundary(batch.anchor, seed, raster, presentation, thresholds);
    let mut active = anchor.measured;
    let mut report = ExtensionRegionReport {
        schema_version: 1,
        profile: PROFILE.into(),
        coverage: batch.coverage,
        native_frames: u32::try_from(expected_native_pts.len()).map_err(|_| RegionError::Raster)?,
        generated_frames: batch.coverage.end - batch.coverage.start,
        thresholds,
        status: RegionCheckStatus::Unavailable,
        measured_frames: 0,
        unavailable_frames: 0,
        first_unavailable_ordinal: None,
        anchor,
        maximum_center_residual: None,
        maximum_log_size_residual: None,
        rejection: None,
        unavailable_reasons: Vec::new(),
    };
    if let Some(reason) = anchor.unavailable_reason {
        push_reason(&mut report.unavailable_reasons, reason);
    }
    let mut excess_run = 0;
    let mut run_start = 0;
    for ordinal in batch.coverage.tracking_ordinals() {
        // Validation proved exact ordered coverage and bounded subtraction.
        let frame = &batch.frames[(ordinal - batch.coverage.start) as usize];
        let observed = qualified(frame.observation, raster, presentation, thresholds);
        if let Err(reason) = observed {
            push_reason(&mut report.unavailable_reasons, reason);
            active = false;
        }
        if !active {
            report.unavailable_frames += 1;
            report.first_unavailable_ordinal.get_or_insert(ordinal);
            push_reason(
                &mut report.unavailable_reasons,
                RegionUnavailableReason::LostTrack,
            );
            excess_run = 0;
            continue;
        }
        let actual = observed.map_err(|_| RegionError::PolicyMismatch)?;
        let (center, log_size) = residuals(actual, seed, presentation);
        report.measured_frames += 1;
        update_max(&mut report.maximum_center_residual, center);
        update_max(&mut report.maximum_log_size_residual, log_size);
        if center > thresholds.center_residual || log_size > thresholds.log_size_residual {
            if excess_run == 0 {
                run_start = ordinal;
            }
            excess_run += 1;
            if excess_run >= thresholds.sustained_frames {
                let candidate = RegionRejection {
                    first_ordinal: run_start.min(ordinal),
                    consecutive_frames: excess_run,
                    center_residual: center,
                    log_size_residual: log_size,
                };
                if report
                    .rejection
                    .is_none_or(|previous| candidate.first_ordinal < previous.first_ordinal)
                {
                    report.rejection = Some(candidate);
                }
            }
        } else {
            excess_run = 0;
        }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionRegionError {
    Coverage(CoverageError),
    RawObservation(RawRegionError),
    Region(RegionError),
}

impl From<CoverageError> for ExtensionRegionError {
    fn from(value: CoverageError) -> Self {
        Self::Coverage(value)
    }
}
impl From<RawRegionError> for ExtensionRegionError {
    fn from(value: RawRegionError) -> Self {
        Self::RawObservation(value)
    }
}
impl From<RegionError> for ExtensionRegionError {
    fn from(value: RegionError) -> Self {
        Self::Region(value)
    }
}
impl std::fmt::Display for ExtensionRegionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coverage(error) => error.fmt(f),
            Self::RawObservation(error) => error.fmt(f),
            Self::Region(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for ExtensionRegionError {}

fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

fn strict_boundary<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<RegionBoundaryAssessment, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Wire {
        measured: bool,
        #[serde(deserialize_with = "required_option")]
        center_residual: Option<f64>,
        #[serde(deserialize_with = "required_option")]
        log_size_residual: Option<f64>,
        #[serde(deserialize_with = "required_option")]
        unavailable_reason: Option<RegionUnavailableReason>,
    }
    let value = Wire::deserialize(deserializer)?;
    Ok(RegionBoundaryAssessment {
        measured: value.measured,
        center_residual: value.center_residual,
        log_size_residual: value.log_size_residual,
        unavailable_reason: value.unavailable_reason,
    })
}

fn bounded_frames<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<RawRegionFrame>, D::Error> {
    bounded_list::<D, RawRegionFrame, MAX_GENERATED_FRAMES>(deserializer)
}

fn bounded_reasons<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<RegionUnavailableReason>, D::Error> {
    bounded_list::<D, RegionUnavailableReason, MAX_UNAVAILABLE_REASONS>(deserializer)
}

fn bounded_list<'de, D, T, const MAX: usize>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Visitor<T, const MAX: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const MAX: usize> serde::de::Visitor<'de> for Visitor<T, MAX> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "at most {MAX} region observations")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while values.len() < MAX {
                let Some(value) = sequence.next_element()? else {
                    return Ok(values);
                };
                values.push(value);
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom(
                    "region observation count exceeds its bound",
                ));
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Visitor::<T, MAX>(std::marker::PhantomData))
}

#[cfg(test)]
mod tests;
