//! Pure, exact planning for endpoint-conditioned bridge generation.
//!
//! The plan contains no model/runtime choice and allocates no per-output-frame
//! table. A worker can reconstruct every sample from the serialized counts.

use std::cmp::Ordering;

use deadpan_core::{ExactRatio, FrameDuration, FrameRate, TimeError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::protocol::ConditioningMode;

pub const GENERATION_PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisLimits {
    minimum: u32,
    maximum: u32,
    multiple: u32,
}

impl AxisLimits {
    pub fn new(minimum: u32, maximum: u32, multiple: u32) -> Result<Self, GenerationPlanError> {
        if minimum == 0 || maximum < minimum || multiple == 0 {
            return Err(GenerationPlanError::InvalidDimensionLimits);
        }
        let remainder = minimum % multiple;
        let first_supported = if remainder == 0 {
            Some(minimum)
        } else {
            minimum.checked_add(multiple - remainder)
        };
        if first_supported.is_none_or(|value| value > maximum) {
            return Err(GenerationPlanError::InvalidDimensionLimits);
        }
        Ok(Self {
            minimum,
            maximum,
            multiple,
        })
    }

    pub const fn minimum(self) -> u32 {
        self.minimum
    }

    pub const fn maximum(self) -> u32 {
        self.maximum
    }

    pub const fn multiple(self) -> u32 {
        self.multiple
    }

    fn contains(self, value: u32) -> bool {
        (self.minimum..=self.maximum).contains(&value) && value.is_multiple_of(self.multiple)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DimensionLimits {
    width: AxisLimits,
    height: AxisLimits,
}

impl DimensionLimits {
    pub const fn new(width: AxisLimits, height: AxisLimits) -> Self {
        Self { width, height }
    }

    pub const fn width(self) -> AxisLimits {
        self.width
    }

    pub const fn height(self) -> AxisLimits {
        self.height
    }

    fn validate(self, dimensions: NativeDimensions) -> Result<(), GenerationPlanError> {
        if !self.width.contains(dimensions.width) || !self.height.contains(dimensions.height) {
            return Err(GenerationPlanError::UnsupportedDimensions {
                width: dimensions.width,
                height: dimensions.height,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeDimensions {
    width: u32,
    height: u32,
}

impl NativeDimensions {
    pub fn new(width: u32, height: u32) -> Result<Self, GenerationPlanError> {
        if width == 0 || height == 0 {
            return Err(GenerationPlanError::UnsupportedDimensions { width, height });
        }
        Ok(Self { width, height })
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }
}

/// Legal native counts are `step * k + offset`, restricted to `[minimum, maximum]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameCountFormula {
    step: u32,
    offset: u32,
    minimum: u32,
    maximum: u32,
}

impl FrameCountFormula {
    pub fn new(
        step: u32,
        offset: u32,
        minimum: u32,
        maximum: u32,
    ) -> Result<Self, GenerationPlanError> {
        if step == 0 || offset >= step || minimum == 0 || maximum < minimum {
            return Err(GenerationPlanError::InvalidFrameCountFormula);
        }
        let formula = Self {
            step,
            offset,
            minimum,
            maximum,
        };
        if formula.first_bridge_count().is_none() {
            return Err(GenerationPlanError::NoLegalBridgeFrameCount);
        }
        Ok(formula)
    }

    pub const fn step(self) -> u32 {
        self.step
    }

    pub const fn offset(self) -> u32 {
        self.offset
    }

    pub const fn minimum(self) -> u32 {
        self.minimum
    }

    pub const fn maximum(self) -> u32 {
        self.maximum
    }

    pub fn contains(self, count: u32) -> bool {
        (self.minimum..=self.maximum).contains(&count)
            && count >= self.offset
            && (count - self.offset).is_multiple_of(self.step)
    }

    fn first_bridge_count(self) -> Option<u32> {
        self.align_up(self.minimum.max(2))
            .filter(|count| *count <= self.maximum)
    }

    fn last_count(self) -> Option<u32> {
        self.align_down(self.maximum)
            .filter(|count| *count >= self.minimum.max(2))
    }

    fn align_up(self, value: u32) -> Option<u32> {
        let value = u64::from(value);
        let step = u64::from(self.step);
        let offset = u64::from(self.offset);
        let count = if value <= offset {
            offset
        } else {
            let delta = value - offset;
            offset.checked_add(delta.div_ceil(step).checked_mul(step)?)?
        };
        u32::try_from(count).ok()
    }

    fn align_down(self, value: u32) -> Option<u32> {
        let value = u64::from(value);
        let offset = u64::from(self.offset);
        if value < offset {
            return None;
        }
        let step = u64::from(self.step);
        let count = offset.checked_add(((value - offset) / step).checked_mul(step)?)?;
        u32::try_from(count).ok()
    }

    fn nearest(self, ideal: ExactRatio) -> Result<u32, GenerationPlanError> {
        let first = self
            .first_bridge_count()
            .ok_or(GenerationPlanError::NoLegalBridgeFrameCount)?;
        let last = self
            .last_count()
            .ok_or(GenerationPlanError::NoLegalBridgeFrameCount)?;
        if ideal.compare_integer(i64::from(first)).is_lt()
            || ideal.compare_integer(i64::from(last)).is_gt()
        {
            return Err(GenerationPlanError::FrameCountOutsideCapability);
        }

        let floor = u32::try_from(ideal.floor())
            .map_err(|_| GenerationPlanError::FrameCountOutsideCapability)?;
        let lower = self
            .align_down(floor)
            .filter(|count| *count >= first)
            .ok_or(GenerationPlanError::FrameCountOutsideCapability)?;
        if ideal.compare_integer(i64::from(lower)).is_eq() {
            return Ok(lower);
        }
        let upper = lower
            .checked_add(self.step)
            .filter(|count| *count <= last)
            .ok_or(GenerationPlanError::FrameCountOutsideCapability)?;
        let below = ideal
            .checked_sub(ExactRatio::integer(i64::from(lower)))
            .map_err(arithmetic)?;
        let above = ExactRatio::integer(i64::from(upper))
            .checked_sub(ideal)
            .map_err(arithmetic)?;
        let difference = below.checked_sub(above).map_err(arithmetic)?;
        // A tie selects the larger legal native count.
        Ok(if difference.compare_integer(0).is_lt() {
            lower
        } else {
            upper
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeCapability {
    supported: bool,
    native_frame_rate: FrameRate,
    frame_counts: FrameCountFormula,
    dimensions: DimensionLimits,
}

impl BridgeCapability {
    pub const fn new(
        supported: bool,
        native_frame_rate: FrameRate,
        frame_counts: FrameCountFormula,
        dimensions: DimensionLimits,
    ) -> Self {
        Self {
            supported,
            native_frame_rate,
            frame_counts,
            dimensions,
        }
    }

    pub const fn supported(self) -> bool {
        self.supported
    }

    pub const fn native_frame_rate(self) -> FrameRate {
        self.native_frame_rate
    }

    pub const fn frame_counts(self) -> FrameCountFormula {
        self.frame_counts
    }

    pub const fn dimensions(self) -> DimensionLimits {
        self.dimensions
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationOperation {
    Bridge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameInterpolation {
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointPolicy {
    InteriorOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectPlan {
    interior_frames: FrameDuration,
    frame_rate: FrameRate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativePlan {
    frame_count: u32,
    frame_rate: FrameRate,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanTiming {
    requested_boundary_duration: ExactRatio,
    actual_boundary_duration: ExactRatio,
    /// Signed seconds: actual native boundary duration minus requested duration.
    retime_deviation: ExactRatio,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SamplingPlan {
    endpoint_policy: EndpointPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BridgeGenerationPlan {
    schema_version: u32,
    operation: GenerationOperation,
    interpolation: FrameInterpolation,
    project: ProjectPlan,
    native: NativePlan,
    timing: PlanTiming,
    sampling: SamplingPlan,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BridgeGenerationPlanWire {
    schema_version: u32,
    operation: GenerationOperation,
    interpolation: FrameInterpolation,
    project: ProjectPlan,
    native: NativePlan,
    timing: PlanTiming,
    sampling: SamplingPlan,
}

impl BridgeGenerationPlan {
    /// Routes only true two-endpoint conditioning into this planner.
    pub fn for_conditioning(
        conditioning: ConditioningMode,
        project_frames: FrameDuration,
        project_frame_rate: FrameRate,
        capability: &BridgeCapability,
        dimensions: NativeDimensions,
    ) -> Result<Self, GenerationPlanError> {
        if conditioning != ConditioningMode::Bridge {
            return Err(GenerationPlanError::UnsupportedConditioning(conditioning));
        }
        Self::new(project_frames, project_frame_rate, capability, dimensions)
    }

    pub fn new(
        project_frames: FrameDuration,
        project_frame_rate: FrameRate,
        capability: &BridgeCapability,
        dimensions: NativeDimensions,
    ) -> Result<Self, GenerationPlanError> {
        if !capability.supported {
            return Err(GenerationPlanError::UnsupportedBridge);
        }
        if project_frames == FrameDuration::ZERO {
            return Err(GenerationPlanError::ZeroProjectFrames);
        }
        capability.dimensions.validate(dimensions)?;

        let requested_boundary_duration = boundary_duration(project_frames, project_frame_rate)?;
        let ideal_count = requested_boundary_duration
            .checked_mul(rate_ratio(capability.native_frame_rate)?)
            .and_then(|value| value.checked_add(ExactRatio::ONE))
            .map_err(arithmetic)?;
        let native_frame_count = capability.frame_counts.nearest(ideal_count)?;
        let actual_boundary_duration =
            native_boundary_duration(native_frame_count, capability.native_frame_rate)?;
        let retime_deviation = actual_boundary_duration
            .checked_sub(requested_boundary_duration)
            .map_err(arithmetic)?;
        let plan = Self {
            schema_version: GENERATION_PLAN_SCHEMA_VERSION,
            operation: GenerationOperation::Bridge,
            interpolation: FrameInterpolation::Linear,
            project: ProjectPlan {
                interior_frames: project_frames,
                frame_rate: project_frame_rate,
            },
            native: NativePlan {
                frame_count: native_frame_count,
                frame_rate: capability.native_frame_rate,
                width: dimensions.width,
                height: dimensions.height,
            },
            timing: PlanTiming {
                requested_boundary_duration,
                actual_boundary_duration,
                retime_deviation,
            },
            sampling: SamplingPlan {
                endpoint_policy: EndpointPolicy::InteriorOnly,
            },
        };
        plan.validate_intrinsic()?;
        Ok(plan)
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub const fn project_frames(&self) -> FrameDuration {
        self.project.interior_frames
    }

    pub const fn project_frame_rate(&self) -> FrameRate {
        self.project.frame_rate
    }

    pub const fn native_frame_count(&self) -> u32 {
        self.native.frame_count
    }

    pub const fn native_frame_rate(&self) -> FrameRate {
        self.native.frame_rate
    }

    pub const fn native_dimensions(&self) -> NativeDimensions {
        NativeDimensions {
            width: self.native.width,
            height: self.native.height,
        }
    }

    pub const fn interpolation(&self) -> FrameInterpolation {
        self.interpolation
    }

    pub const fn requested_boundary_duration(&self) -> ExactRatio {
        self.timing.requested_boundary_duration
    }

    pub const fn actual_boundary_duration(&self) -> ExactRatio {
        self.timing.actual_boundary_duration
    }

    /// Signed seconds: actual native boundary duration minus requested duration.
    pub const fn retime_deviation(&self) -> ExactRatio {
        self.timing.retime_deviation
    }

    pub fn validate_for(&self, capability: &BridgeCapability) -> Result<(), GenerationPlanError> {
        self.validate_intrinsic()?;
        if !capability.supported {
            return Err(GenerationPlanError::UnsupportedBridge);
        }
        if self.native.frame_rate != capability.native_frame_rate {
            return Err(GenerationPlanError::NativeFrameRateMismatch);
        }
        if !capability.frame_counts.contains(self.native.frame_count) || self.native.frame_count < 2
        {
            return Err(GenerationPlanError::UnsupportedNativeFrameCount(
                self.native.frame_count,
            ));
        }
        let ideal_count = self
            .timing
            .requested_boundary_duration
            .checked_mul(rate_ratio(capability.native_frame_rate)?)
            .and_then(|value| value.checked_add(ExactRatio::ONE))
            .map_err(arithmetic)?;
        let expected = capability.frame_counts.nearest(ideal_count)?;
        if self.native.frame_count != expected {
            return Err(GenerationPlanError::NativeFrameCountNotNearest {
                actual: self.native.frame_count,
                expected,
            });
        }
        capability.dimensions.validate(self.native_dimensions())
    }

    pub fn sample(&self, output_index: u64) -> Result<BridgeSample, GenerationPlanError> {
        let output_count = u64::try_from(self.project.interior_frames.frames())
            .map_err(|_| GenerationPlanError::ArithmeticOverflow)?;
        if output_index >= output_count {
            return Err(GenerationPlanError::SampleOutOfRange {
                index: output_index,
                count: output_count,
            });
        }
        let one_based = i128::from(output_index)
            .checked_add(1)
            .ok_or(GenerationPlanError::ArithmeticOverflow)?;
        let native_span = i128::from(self.native.frame_count - 1);
        let boundary_count = i128::from(self.project.interior_frames.frames())
            .checked_add(1)
            .ok_or(GenerationPlanError::ArithmeticOverflow)?;
        let position = ExactRatio::new(
            one_based
                .checked_mul(native_span)
                .ok_or(GenerationPlanError::ArithmeticOverflow)?,
            boundary_count,
        )
        .map_err(arithmetic)?;
        let lower =
            u32::try_from(position.floor()).map_err(|_| GenerationPlanError::ArithmeticOverflow)?;
        let upper = u32::try_from(position.ceil().map_err(arithmetic)?)
            .map_err(|_| GenerationPlanError::ArithmeticOverflow)?;
        let upper_weight = position
            .checked_sub(ExactRatio::integer(i64::from(lower)))
            .map_err(arithmetic)?;
        Ok(BridgeSample {
            output_index,
            native_position: position,
            lower_index: lower,
            upper_index: upper,
            upper_weight,
        })
    }

    pub fn samples(&self) -> BridgeSamples<'_> {
        BridgeSamples {
            plan: self,
            next: 0,
            count: u64::try_from(self.project.interior_frames.frames())
                .expect("validated positive frame duration fits u64"),
        }
    }

    fn validate_intrinsic(&self) -> Result<(), GenerationPlanError> {
        if self.schema_version != GENERATION_PLAN_SCHEMA_VERSION {
            return Err(GenerationPlanError::UnsupportedSchema(self.schema_version));
        }
        if self.operation != GenerationOperation::Bridge
            || self.interpolation != FrameInterpolation::Linear
            || self.sampling.endpoint_policy != EndpointPolicy::InteriorOnly
        {
            return Err(GenerationPlanError::InconsistentPlan);
        }
        if self.project.interior_frames == FrameDuration::ZERO {
            return Err(GenerationPlanError::ZeroProjectFrames);
        }
        if self.native.frame_count < 2 || self.native.width == 0 || self.native.height == 0 {
            return Err(GenerationPlanError::InconsistentPlan);
        }
        let requested = boundary_duration(self.project.interior_frames, self.project.frame_rate)?;
        let actual = native_boundary_duration(self.native.frame_count, self.native.frame_rate)?;
        let deviation = actual.checked_sub(requested).map_err(arithmetic)?;
        if self.timing.requested_boundary_duration != requested
            || self.timing.actual_boundary_duration != actual
            || self.timing.retime_deviation != deviation
        {
            return Err(GenerationPlanError::InconsistentPlan);
        }
        // Prove the endpoint-exclusion arithmetic on both extremes.
        let first = self.sample(0)?;
        let last = self.sample(
            u64::try_from(self.project.interior_frames.frames() - 1)
                .map_err(|_| GenerationPlanError::ArithmeticOverflow)?,
        )?;
        if first.native_position.compare_integer(0) != Ordering::Greater
            || last
                .native_position
                .compare_integer(i64::from(self.native.frame_count - 1))
                != Ordering::Less
        {
            return Err(GenerationPlanError::InconsistentPlan);
        }
        Ok(())
    }
}

impl TryFrom<BridgeGenerationPlanWire> for BridgeGenerationPlan {
    type Error = GenerationPlanError;

    fn try_from(value: BridgeGenerationPlanWire) -> Result<Self, Self::Error> {
        let plan = Self {
            schema_version: value.schema_version,
            operation: value.operation,
            interpolation: value.interpolation,
            project: value.project,
            native: value.native,
            timing: value.timing,
            sampling: value.sampling,
        };
        plan.validate_intrinsic()?;
        Ok(plan)
    }
}

impl<'de> Deserialize<'de> for BridgeGenerationPlan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = BridgeGenerationPlanWire::deserialize(deserializer)?;
        Self::try_from(wire).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeSample {
    output_index: u64,
    native_position: ExactRatio,
    lower_index: u32,
    upper_index: u32,
    upper_weight: ExactRatio,
}

impl BridgeSample {
    pub const fn output_index(self) -> u64 {
        self.output_index
    }

    pub const fn native_position(self) -> ExactRatio {
        self.native_position
    }

    pub const fn lower_index(self) -> u32 {
        self.lower_index
    }

    pub const fn upper_index(self) -> u32 {
        self.upper_index
    }

    /// Linear interpolation weight assigned to `upper_index`.
    pub const fn upper_weight(self) -> ExactRatio {
        self.upper_weight
    }

    pub fn lower_weight(self) -> Result<ExactRatio, GenerationPlanError> {
        ExactRatio::ONE
            .checked_sub(self.upper_weight)
            .map_err(arithmetic)
    }
}

pub struct BridgeSamples<'a> {
    plan: &'a BridgeGenerationPlan,
    next: u64,
    count: u64,
}

impl Iterator for BridgeSamples<'_> {
    type Item = BridgeSample;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next == self.count {
            return None;
        }
        let sample = self
            .plan
            .sample(self.next)
            .expect("validated plan samples are representable");
        self.next += 1;
        Some(sample)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.count - self.next;
        let lower = usize::try_from(remaining).unwrap_or(usize::MAX);
        (lower, usize::try_from(remaining).ok())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GenerationPlanError {
    #[error("conditioning mode {0:?} is not a two-endpoint bridge")]
    UnsupportedConditioning(ConditioningMode),
    #[error("provider does not support endpoint-conditioned bridge generation")]
    UnsupportedBridge,
    #[error("bridge output must contain at least one project frame")]
    ZeroProjectFrames,
    #[error("invalid native frame-count formula")]
    InvalidFrameCountFormula,
    #[error("frame-count capability contains no legal bridge count")]
    NoLegalBridgeFrameCount,
    #[error("ideal native frame count lies outside the provider capability")]
    FrameCountOutsideCapability,
    #[error("invalid native dimension limits")]
    InvalidDimensionLimits,
    #[error("native dimensions {width}x{height} are unsupported")]
    UnsupportedDimensions { width: u32, height: u32 },
    #[error("native frame rate does not match the selected capability")]
    NativeFrameRateMismatch,
    #[error("native frame count {0} is unsupported")]
    UnsupportedNativeFrameCount(u32),
    #[error("native frame count {actual} is legal but nearest count is {expected}")]
    NativeFrameCountNotNearest { actual: u32, expected: u32 },
    #[error("generation plan arithmetic overflow")]
    ArithmeticOverflow,
    #[error("unsupported generation plan schema {0}")]
    UnsupportedSchema(u32),
    #[error("generation plan fields are inconsistent")]
    InconsistentPlan,
    #[error("output sample {index} is outside 0..{count}")]
    SampleOutOfRange { index: u64, count: u64 },
}

fn boundary_duration(
    project_frames: FrameDuration,
    project_rate: FrameRate,
) -> Result<ExactRatio, GenerationPlanError> {
    let boundaries = i128::from(project_frames.frames())
        .checked_add(1)
        .ok_or(GenerationPlanError::ArithmeticOverflow)?;
    ExactRatio::new(boundaries, 1)
        .and_then(|value| {
            value.checked_mul(ExactRatio::new(
                i128::from(project_rate.denominator()),
                i128::from(project_rate.numerator()),
            )?)
        })
        .map_err(arithmetic)
}

fn native_boundary_duration(
    native_frame_count: u32,
    native_rate: FrameRate,
) -> Result<ExactRatio, GenerationPlanError> {
    ExactRatio::new(i128::from(native_frame_count - 1), 1)
        .and_then(|value| {
            value.checked_mul(ExactRatio::new(
                i128::from(native_rate.denominator()),
                i128::from(native_rate.numerator()),
            )?)
        })
        .map_err(arithmetic)
}

fn rate_ratio(rate: FrameRate) -> Result<ExactRatio, GenerationPlanError> {
    ExactRatio::new(i128::from(rate.numerator()), i128::from(rate.denominator()))
        .map_err(arithmetic)
}

fn arithmetic(_: TimeError) -> GenerationPlanError {
    GenerationPlanError::ArithmeticOverflow
}
