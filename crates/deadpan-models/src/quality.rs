//! Versioned gross motion/lighting rejection heuristics over every native frame.
//! They do not establish identity, silence or perceptual acceptability.

use deadpan_analysis::generation_quality::{FrameChange, LumaGrid, compare};
use deadpan_core::FrameRate;
use deadpan_jobs::{BridgeGenerationPlan, MotionAmount};
use deadpan_media::protocol::VideoContract;
use serde::{Deserialize, Serialize};

use crate::QualificationError;

#[path = "quality_decode.rs"]
mod decode;
pub(crate) use decode::measure;

const PROFILE: &str = "deadpan-motion-lighting-1";
pub(crate) const MAX_FRAMES: u32 = 1025;
const BLOCKS: u32 = 36;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityThresholds {
    /// P95 normalized displacement per authored second. X and Y are divided
    /// by width and height respectively before taking Euclidean magnitude.
    pub maximum_motion_per_second: f64,
    pub abrupt_luma_shift: f64,
    pub minimum_lighting_agreement: f64,
}

impl QualityThresholds {
    pub(crate) fn for_motion(motion: MotionAmount) -> Self {
        Self {
            maximum_motion_per_second: match motion {
                MotionAmount::Still => 0.5,
                MotionAmount::Subtle => 1.0,
                MotionAmount::Moderate => 2.0,
            },
            abrupt_luma_shift: 32.0,
            minimum_lighting_agreement: 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "status", rename_all = "snake_case")]
pub enum MotionObservation {
    Measured {
        maximum: f64,
        p95: f64,
    },
    /// Includes poor/ambiguous matches and motion beyond the bounded search.
    /// This is never a measured zero or a successful motion check.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameObservation {
    /// Zero-based picture coordinate in the enclosing report's clock. Native
    /// reports name the later picture; sampled-join reports name the endpoint.
    pub after_frame: u32,
    pub mean_luma_shift: f64,
    pub mean_absolute_luma_change: f64,
    pub lighting_agreement_fraction: f64,
    pub textured_blocks: u32,
    pub matched_blocks: u32,
    pub motion: MotionObservation,
}

impl FrameObservation {
    pub(crate) fn new(after_frame: u32, change: FrameChange) -> Self {
        Self {
            after_frame,
            mean_luma_shift: change.mean_luma_shift,
            mean_absolute_luma_change: change.mean_absolute_luma_change,
            lighting_agreement_fraction: change.lighting_agreement_fraction,
            // The pure fixed-grid matcher has exactly 36 block positions.
            textured_blocks: change.motion.textured_blocks as u32,
            matched_blocks: change.motion.matched_blocks as u32,
            motion: change
                .motion
                .displacement
                .map_or(MotionObservation::Unavailable, |value| {
                    MotionObservation::Measured {
                        maximum: value.maximum,
                        p95: value.p95,
                    }
                }),
        }
    }

    pub(crate) fn validate(&self) -> bool {
        if !(-255.0..=255.0).contains(&self.mean_luma_shift)
            || !(0.0..=255.0).contains(&self.mean_absolute_luma_change)
            || self.mean_absolute_luma_change + 1e-10 < self.mean_luma_shift.abs()
            || !(0.0..=1.0).contains(&self.lighting_agreement_fraction)
            || self.textured_blocks > BLOCKS
            || self.matched_blocks > self.textured_blocks
        {
            return false;
        }
        let enough = self.matched_blocks >= 9;
        match self.motion {
            MotionObservation::Unavailable => !enough,
            MotionObservation::Measured { maximum, p95 } => {
                enough && (0.0..=0.1).contains(&maximum) && (0.0..=maximum).contains(&p95)
            }
        }
    }
}

/// Host evidence, bound to the native master and exact authored sampling clock.
/// Reports retain unavailable motion coverage explicitly. Successful admission
/// means no measured rejection, not that every region was measurable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeQualityReport {
    schema_version: u32,
    profile: String,
    native: VideoContract,
    project_frames: i64,
    project_rate: FrameRate,
    motion: MotionAmount,
    thresholds: QualityThresholds,
    transitions: Vec<FrameObservation>,
}

impl BridgeQualityReport {
    fn new(plan: &BridgeGenerationPlan, motion: MotionAmount) -> Result<Self, QualificationError> {
        let native = native_contract(plan);
        if !(2..=MAX_FRAMES).contains(&native.frames) {
            return Err(invalid(
                "native frame count exceeds the quality inspection budget",
            ));
        }
        Ok(Self {
            schema_version: 1,
            profile: PROFILE.into(),
            native,
            project_frames: plan.project_frames().frames(),
            project_rate: plan.project_frame_rate(),
            motion,
            thresholds: QualityThresholds::for_motion(motion),
            transitions: Vec::with_capacity((native.frames - 1) as usize),
        })
    }

    pub fn transitions(&self) -> &[FrameObservation] {
        &self.transitions
    }

    pub fn unavailable_motion_pairs(&self) -> usize {
        self.transitions
            .iter()
            .filter(|pair| matches!(pair.motion, MotionObservation::Unavailable))
            .count()
    }

    pub fn thresholds(&self) -> QualityThresholds {
        self.thresholds
    }

    /// Pair spacing in the authored clock follows `(N+1)/(M-1)` project
    /// frames, including the two conditioning boundaries. Native frame rate
    /// alone would miss speed changes introduced by exact Hold resampling.
    fn pair_seconds(&self) -> f64 {
        (self.project_frames as f64 + 1.0) * f64::from(self.project_rate.denominator())
            / (f64::from(self.native.frames - 1) * f64::from(self.project_rate.numerator()))
    }

    pub(crate) fn validate(
        &self,
        plan: &BridgeGenerationPlan,
        motion: MotionAmount,
    ) -> Result<(), QualificationError> {
        if self.schema_version != 1
            || self.profile != PROFILE
            || self.native != native_contract(plan)
            || !(2..=MAX_FRAMES).contains(&self.native.frames)
            || self.project_frames != plan.project_frames().frames()
            || self.project_rate != plan.project_frame_rate()
            || self.motion != motion
            || self.thresholds != QualityThresholds::for_motion(motion)
            || self.transitions.len() != (self.native.frames - 1) as usize
            || self
                .transitions
                .iter()
                .enumerate()
                .any(|(index, pair)| pair.after_frame as usize != index + 1 || !pair.validate())
        {
            return Err(invalid(
                "motion/lighting report is incomplete or differs from its plan/policy",
            ));
        }
        self.admit()
    }

    fn admit(&self) -> Result<(), QualificationError> {
        for pair in &self.transitions {
            admit_observation(
                pair,
                self.thresholds,
                self.pair_seconds(),
                self.motion,
                PROFILE,
                ObservationClock::Native,
            )?;
        }
        Ok(())
    }
}

/// Shared rejection policy. Callers separately prove exact pair coverage and
/// clock identity; this helper never infers those from measurements.
#[derive(Clone, Copy)]
pub(crate) enum ObservationClock {
    Native,
    Sampled,
}

impl ObservationClock {
    const fn name(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Sampled => "sampled",
        }
    }
}

pub(crate) fn admit_observation(
    pair: &FrameObservation,
    thresholds: QualityThresholds,
    pair_seconds: f64,
    motion: MotionAmount,
    profile: &str,
    clock: ObservationClock,
) -> Result<(), QualificationError> {
    if !pair.validate() || !pair_seconds.is_finite() || pair_seconds <= 0.0 {
        return Err(invalid(
            "invalid motion/lighting observation or pair spacing",
        ));
    }
    if pair.mean_luma_shift.abs() >= thresholds.abrupt_luma_shift
        && pair.lighting_agreement_fraction >= thresholds.minimum_lighting_agreement
    {
        return Err(invalid(format!(
            "{profile}: abrupt lighting at {} frame {}: luma shift {:.3}, agreement {:.3}; limits {:.3}/{:.3}",
            clock.name(),
            pair.after_frame,
            pair.mean_luma_shift,
            pair.lighting_agreement_fraction,
            thresholds.abrupt_luma_shift,
            thresholds.minimum_lighting_agreement,
        )));
    }
    if let MotionObservation::Measured { p95, .. } = pair.motion {
        let speed = p95 / pair_seconds;
        if speed > thresholds.maximum_motion_per_second {
            return Err(invalid(format!(
                "{profile}: excessive motion at {} frame {}: p95 {:.6}/s, {} of {BLOCKS} blocks; limit {:.6}/s for {}",
                clock.name(),
                pair.after_frame,
                speed,
                pair.matched_blocks,
                thresholds.maximum_motion_per_second,
                motion.name(),
            )));
        }
    }
    Ok(())
}

fn native_contract(plan: &BridgeGenerationPlan) -> VideoContract {
    let dimensions = plan.native_dimensions();
    VideoContract {
        width: dimensions.width(),
        height: dimensions.height(),
        frames: plan.native_frame_count(),
        rate_num: plan.native_frame_rate().numerator(),
        rate_den: plan.native_frame_rate().denominator(),
    }
}

fn invalid(reason: impl ToString) -> QualificationError {
    QualificationError::Quality(reason.to_string())
}

struct Accumulator {
    report: BridgeQualityReport,
    previous: Option<LumaGrid>,
    frames: u32,
}

impl Accumulator {
    fn new(plan: &BridgeGenerationPlan, motion: MotionAmount) -> Result<Self, QualificationError> {
        Ok(Self {
            report: BridgeQualityReport::new(plan, motion)?,
            previous: None,
            frames: 0,
        })
    }
    fn push(&mut self, grid: LumaGrid) -> Result<(), QualificationError> {
        if self.frames >= self.report.native.frames {
            return Err(invalid(
                "decoded more quality pictures than the declared native count",
            ));
        }
        if let Some(previous) = &self.previous {
            self.report
                .transitions
                .push(FrameObservation::new(self.frames, compare(previous, &grid)));
        }
        self.previous = Some(grid);
        self.frames += 1;
        Ok(())
    }
    fn finish(
        self,
        plan: &BridgeGenerationPlan,
        motion: MotionAmount,
    ) -> Result<BridgeQualityReport, QualificationError> {
        self.report.validate(plan, motion)?;
        Ok(self.report)
    }
}

#[cfg(test)]
pub(crate) fn test_report(
    plan: &BridgeGenerationPlan,
    motion: MotionAmount,
) -> BridgeQualityReport {
    let mut report = BridgeQualityReport::new(plan, motion).unwrap();
    report.transitions = (1..plan.native_frame_count())
        .map(|after_frame| FrameObservation {
            after_frame,
            mean_luma_shift: 0.0,
            mean_absolute_luma_change: 0.0,
            lighting_agreement_fraction: 1.0,
            textured_blocks: 0,
            matched_blocks: 0,
            motion: MotionObservation::Unavailable,
        })
        .collect();
    report
}

#[cfg(test)]
#[path = "quality_tests.rs"]
mod tests;
