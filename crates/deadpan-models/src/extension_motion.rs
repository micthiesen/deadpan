//! Generated-interval motion and lighting evidence for one-sided extensions.
//! Conditioning pictures are excluded; sampled joins are a separate check.

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_analysis::generation_quality::{LumaGrid, compare};
use deadpan_core::{AssetId, FrameRate, GeneratedObjectRef, SourceFrameId};
use deadpan_jobs::{ExtensionGenerationPlan, MotionAmount};
use deadpan_media::CanonicalMedia;
use deadpan_media::protocol::VideoContract;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use serde::{Deserialize, Serialize};

use crate::QualificationError;
use crate::quality::{
    FrameObservation, MotionObservation, ObservationClock, QualityThresholds, admit_observation,
};

const PROFILE: &str = "deadpan-extension-motion-lighting-1";
const MAX_PAIRS: usize = 1024;
// Context is indexed too, but it is never added to the inspected interval.
const MAX_NATIVE_FRAMES: u32 = 4096;

/// Host measurements over every adjacent pair of generated native pictures,
/// in chronological order for either extension direction. The exact plan and
/// immutable native object are part of the evidence. Successful validation
/// means no measured rejection; unavailable motion is never a measured zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionMotionReport {
    schema_version: u32,
    profile: String,
    plan: ExtensionGenerationPlan,
    native_object: GeneratedObjectRef,
    motion: MotionAmount,
    thresholds: QualityThresholds,
    #[serde(deserialize_with = "bounded_transitions")]
    transitions: Vec<FrameObservation>,
}

impl ExtensionMotionReport {
    fn new(
        plan: &ExtensionGenerationPlan,
        native_object: &GeneratedObjectRef,
        motion: MotionAmount,
        transitions: Vec<FrameObservation>,
    ) -> Result<Self, QualificationError> {
        let report = Self {
            schema_version: 1,
            profile: PROFILE.into(),
            plan: plan.clone(),
            native_object: native_object.clone(),
            motion,
            thresholds: QualityThresholds::for_motion(motion),
            transitions,
        };
        report.validate(plan, native_object, motion)?;
        Ok(report)
    }

    pub fn transitions(&self) -> &[FrameObservation] {
        &self.transitions
    }

    pub fn thresholds(&self) -> QualityThresholds {
        self.thresholds
    }

    pub fn unavailable_motion_pairs(&self) -> usize {
        self.transitions
            .iter()
            .filter(|pair| matches!(pair.motion, MotionObservation::Unavailable))
            .count()
    }

    /// Zero pairs means there was no internal motion to measure, rather than
    /// measured stillness. Current provider plans require at least eight new
    /// pictures; the underlying accumulation also handles a single picture.
    pub fn measured_motion_pairs(&self) -> usize {
        self.transitions.len() - self.unavailable_motion_pairs()
    }

    /// FrameCentersClamped maps E generated picture centers across N authored
    /// frames, so native-adjacent spacing is N/(project_fps*E). N=1 retains this
    /// spacing even though only one sampled output picture will be inserted.
    pub fn pair_seconds(&self) -> f64 {
        pair_seconds(
            self.plan.project_frames().frames(),
            self.plan.project_frame_rate(),
            self.plan.generated_frame_count(),
        )
    }

    pub fn validate(
        &self,
        plan: &ExtensionGenerationPlan,
        native_object: &GeneratedObjectRef,
        motion: MotionAmount,
    ) -> Result<(), QualificationError> {
        let interval = inspection_interval(plan)?;
        if self.schema_version != 1
            || self.profile != PROFILE
            || self.plan != *plan
            || self.native_object != *native_object
            || self.motion != motion
            || self.thresholds != QualityThresholds::for_motion(motion)
        {
            return Err(invalid(
                "extension motion/lighting report differs from its plan, object or policy",
            ));
        }
        validate_coverage(&self.transitions, interval)?;
        for pair in &self.transitions {
            admit_observation(
                pair,
                self.thresholds,
                self.pair_seconds(),
                motion,
                PROFILE,
                ObservationClock::Native,
            )?;
        }
        Ok(())
    }
}

fn pair_seconds(output_frames: i64, project_rate: FrameRate, generated_frames: u32) -> f64 {
    output_frames as f64 * f64::from(project_rate.denominator())
        / (f64::from(project_rate.numerator()) * f64::from(generated_frames))
}

pub(crate) fn native_contract(plan: &ExtensionGenerationPlan) -> VideoContract {
    VideoContract {
        width: plan.native_dimensions().width(),
        height: plan.native_dimensions().height(),
        frames: plan.native_frame_count(),
        rate_num: plan.native_frame_rate().numerator(),
        rate_den: plan.native_frame_rate().denominator(),
    }
}

fn inspection_interval(plan: &ExtensionGenerationPlan) -> Result<Range<u32>, QualificationError> {
    let contract = native_contract(plan);
    contract.validate().map_err(invalid)?;
    if contract.frames > MAX_NATIVE_FRAMES {
        return Err(invalid(
            "extension native count exceeds the inspection budget",
        ));
    }
    let interval = plan.sampling_map().generated_interval();
    let start = u32::try_from(interval.start).map_err(invalid)?;
    let end = u32::try_from(interval.end).map_err(invalid)?;
    validate_interval(&(start..end))?;
    Ok(start..end)
}

fn validate_interval(interval: &Range<u32>) -> Result<(), QualificationError> {
    let count = interval.end.checked_sub(interval.start);
    if !matches!(count, Some(1..=1025)) || interval.end > MAX_NATIVE_FRAMES {
        return Err(invalid(
            "extension generated interval exceeds the inspection budget",
        ));
    }
    Ok(())
}

fn validate_coverage(
    transitions: &[FrameObservation],
    interval: Range<u32>,
) -> Result<(), QualificationError> {
    validate_interval(&interval)?;
    if transitions.len() != (interval.end - interval.start - 1) as usize
        || transitions.iter().enumerate().any(|(index, pair)| {
            pair.after_frame != interval.start + 1 + index as u32 || !pair.validate()
        })
    {
        return Err(invalid(
            "extension motion/lighting report has missing, reordered or invalid generated pairs",
        ));
    }
    Ok(())
}

struct Accumulator {
    interval: Range<u32>,
    next: u32,
    previous: Option<LumaGrid>,
    transitions: Vec<FrameObservation>,
}

impl Accumulator {
    fn new(interval: Range<u32>) -> Result<Self, QualificationError> {
        validate_interval(&interval)?;
        Ok(Self {
            next: interval.start,
            transitions: Vec::with_capacity((interval.end - interval.start - 1) as usize),
            interval,
            previous: None,
        })
    }

    fn push(&mut self, ordinal: u32, grid: LumaGrid) -> Result<(), QualificationError> {
        if ordinal != self.next || ordinal >= self.interval.end {
            return Err(invalid(
                "extension decoded picture is outside chronological generated coverage",
            ));
        }
        if let Some(previous) = &self.previous {
            self.transitions
                .push(FrameObservation::new(ordinal, compare(previous, &grid)));
        }
        self.previous = Some(grid);
        self.next += 1;
        Ok(())
    }

    fn finish(self) -> Result<Vec<FrameObservation>, QualificationError> {
        if self.next != self.interval.end {
            return Err(invalid(
                "extension generated pictures are incompletely inspected",
            ));
        }
        validate_coverage(&self.transitions, self.interval)?;
        Ok(self.transitions)
    }
}

fn remaining(deadline: Instant, cancelled: &AtomicBool) -> Result<Duration, QualificationError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(QualificationError::Cancelled);
    }
    let duration = deadline.saturating_duration_since(Instant::now());
    if duration.is_zero() {
        Err(QualificationError::Deadline)
    } else {
        Ok(duration)
    }
}

/// Inspect the immutable host conversion, never a worker-controlled path.
/// Work, allocation, decode and the complete inspection share one deadline.
pub(crate) fn measure(
    native: &CanonicalMedia,
    plan: &ExtensionGenerationPlan,
    motion: MotionAmount,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<ExtensionMotionReport, QualificationError> {
    remaining(deadline, cancelled)?;
    let interval = inspection_interval(plan)?;
    let contract = native_contract(plan);
    if native.report().video != contract {
        return Err(invalid(
            "extension native object differs from its declared contract",
        ));
    }
    let mut accumulator = Accumulator::new(interval.clone())?;
    let mut limits = SourceSessionLimits {
        opening_timeout: remaining(deadline, cancelled)?,
        maximum_index_frames: MAX_NATIVE_FRAMES as usize,
        maximum_index_bytes: 1024 * 1024,
        maximum_seek_frames: MAX_NATIVE_FRAMES as usize,
        ..SourceSessionLimits::default()
    };
    limits.decode.max_input_bytes = native.object().byte_length();
    limits.decode.max_frames = u64::from(MAX_NATIVE_FRAMES) * 3;
    limits.decode.max_pixels = deadpan_analysis::generation_quality::MAX_PIXELS;
    let input = native.verified_source_input().map_err(invalid)?;
    let asset = AssetId::new("extension-motion-master").map_err(invalid)?;
    let session = SourceSession::open_input(input, asset, limits, cancelled);
    remaining(deadline, cancelled)?;
    let mut session = session.map_err(invalid)?;
    if session.index().index().frames().len() != contract.frames as usize
        || session.info().bwdif_fields
        || session.info().width != contract.width
        || session.info().height != contract.height
        || session.info().time_base_num != 1
        || session.info().time_base_den != 1000
    {
        return Err(invalid(
            "extension motion decoder differs from the canonical native contract",
        ));
    }
    for ordinal in interval {
        let frame = session.frame(
            SourceFrameId(u64::from(ordinal)),
            remaining(deadline, cancelled)?.min(Duration::from_secs(60)),
            cancelled,
        );
        remaining(deadline, cancelled)?;
        let frame = frame.map_err(invalid)?;
        if frame.sample_bits != 8
            || frame.width != contract.width
            || frame.height != contract.height
            || frame.metadata.pts != contract.matroska_pts(ordinal).map_err(invalid)?
        {
            return Err(invalid(
                "extension motion picture differs from its exact canonical identity",
            ));
        }
        let grid = LumaGrid::from_rgba(
            &frame.rgba,
            frame.width,
            frame.height,
            frame.row_stride_bytes,
        )
        .map_err(invalid)?;
        remaining(deadline, cancelled)?;
        accumulator.push(ordinal, grid)?;
        remaining(deadline, cancelled)?;
    }
    let report = ExtensionMotionReport::new(plan, native.object(), motion, accumulator.finish()?)?;
    remaining(deadline, cancelled)?;
    Ok(report)
}

// The shared Bridge observation's historical enum uses a unit variant.
// Extension evidence instead requires an empty object and rejects hidden fields
// on an unavailable observation, without changing older Bridge serialization.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObservationWire {
    after_frame: u32,
    mean_luma_shift: f64,
    mean_absolute_luma_change: f64,
    lighting_agreement_fraction: f64,
    textured_blocks: u32,
    matched_blocks: u32,
    motion: MotionWire,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, tag = "status", rename_all = "snake_case")]
enum MotionWire {
    Measured { maximum: f64, p95: f64 },
    Unavailable {},
}

impl From<ObservationWire> for FrameObservation {
    fn from(value: ObservationWire) -> Self {
        Self {
            after_frame: value.after_frame,
            mean_luma_shift: value.mean_luma_shift,
            mean_absolute_luma_change: value.mean_absolute_luma_change,
            lighting_agreement_fraction: value.lighting_agreement_fraction,
            textured_blocks: value.textured_blocks,
            matched_blocks: value.matched_blocks,
            motion: match value.motion {
                MotionWire::Measured { maximum, p95 } => {
                    MotionObservation::Measured { maximum, p95 }
                }
                MotionWire::Unavailable {} => MotionObservation::Unavailable,
            },
        }
    }
}

fn bounded_transitions<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<FrameObservation>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Vec<FrameObservation>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                formatter,
                "at most {MAX_PAIRS} extension motion observations"
            )
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while values.len() < MAX_PAIRS {
                let Some(value) = sequence.next_element::<ObservationWire>()? else {
                    return Ok(values);
                };
                values.push(value.into());
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom(
                    "extension motion observation count exceeds its bound",
                ));
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Visitor)
}

fn invalid(reason: impl ToString) -> QualificationError {
    QualificationError::Quality(reason.to_string())
}

#[cfg(test)]
#[path = "extension_motion_tests.rs"]
mod tests;
