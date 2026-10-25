//! Exact one-sided extension planning. No model, allocation or timeline edits.

use deadpan_core::{
    BridgeInterpolation, ExactRatio, ExtensionDirection, ExtensionSamplingMap, FrameDuration,
    FrameRate,
};
use serde::{Deserialize, Deserializer, Serialize, de};
use thiserror::Error;

use crate::{DimensionLimits, FrameCountFormula, NativeDimensions};

pub const EXTENSION_GENERATION_PLAN_SCHEMA_VERSION: u32 = 1;

/// A host-qualified finite envelope for the extension provider.
///
/// Context length is fixed, generated counts are positive multiples of eight,
/// and the longest requested interval is the maximum legal generated count at
/// the native rate. Constructing this value does not qualify a model or device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtensionCapability {
    native_rate: FrameRate,
    context_frame_count: u32,
    generated_counts: FrameCountFormula,
    dimensions: DimensionLimits,
    maximum_output_frames: FrameDuration,
    first_generated_count: u32,
    last_generated_count: u32,
}

impl ExtensionCapability {
    pub fn new(
        native_rate: FrameRate,
        context_frame_count: u32,
        generated_counts: FrameCountFormula,
        dimensions: DimensionLimits,
        maximum_output_frames: FrameDuration,
    ) -> Result<Self, ExtensionPlanError> {
        if context_frame_count == 0
            || !(context_frame_count - 1).is_multiple_of(8)
            || generated_counts.step() != 8
            || generated_counts.offset() != 0
            || maximum_output_frames == FrameDuration::ZERO
        {
            return Err(ExtensionPlanError::InvalidCapability);
        }
        let first_generated_count = generated_counts
            .minimum()
            .div_ceil(8)
            .checked_mul(8)
            .ok_or(ExtensionPlanError::InvalidCapability)?;
        let last_generated_count = generated_counts.maximum() / 8 * 8;
        if first_generated_count == 0
            || first_generated_count > last_generated_count
            || context_frame_count
                .checked_add(last_generated_count)
                .is_none()
        {
            return Err(ExtensionPlanError::InvalidCapability);
        }
        Ok(Self {
            native_rate,
            context_frame_count,
            generated_counts,
            dimensions,
            maximum_output_frames,
            first_generated_count,
            last_generated_count,
        })
    }

    pub const fn native_frame_rate(&self) -> FrameRate {
        self.native_rate
    }

    pub const fn context_frame_count(&self) -> u32 {
        self.context_frame_count
    }

    pub const fn generated_frame_counts(&self) -> FrameCountFormula {
        self.generated_counts
    }

    pub const fn dimensions(&self) -> DimensionLimits {
        self.dimensions
    }

    pub const fn maximum_output_frames(&self) -> FrameDuration {
        self.maximum_output_frames
    }

    pub fn maximum_requested_duration(&self) -> Result<ExactRatio, ExtensionPlanError> {
        duration(i64::from(self.last_generated_count), self.native_rate)
    }

    pub const fn maximum_native_frame_count(&self) -> u32 {
        // Proved at construction, including for an unaligned advertised maximum.
        self.context_frame_count + self.last_generated_count
    }

    fn nearest(&self, ideal: ExactRatio) -> Result<u32, ExtensionPlanError> {
        // Check the advertised duration before rounding. Small positive requests
        // may use the minimum legal internal count, but long ones never clamp.
        if ideal.compare_integer(0).is_le()
            || ideal
                .compare_integer(i64::from(self.last_generated_count))
                .is_gt()
        {
            return Err(ExtensionPlanError::DurationOutsideCapability);
        }
        if ideal
            .compare_integer(i64::from(self.first_generated_count))
            .is_le()
        {
            return Ok(self.first_generated_count);
        }
        let floor =
            u32::try_from(ideal.floor()).map_err(|_| ExtensionPlanError::ArithmeticOverflow)?;
        let lower = floor / 8 * 8;
        let midpoint = i64::from(lower) + 4;
        // Ties select the shorter duration. Equality with the last legal count
        // takes this branch without asking for an unsupported upper neighbor.
        if ideal.compare_integer(midpoint).is_le() {
            Ok(lower)
        } else {
            lower
                .checked_add(8)
                .ok_or(ExtensionPlanError::ArithmeticOverflow)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExtensionOperation {
    Extension,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DimensionsWire {
    width: u32,
    height: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtensionGenerationPlanWire {
    schema_version: u32,
    operation: ExtensionOperation,
    sampling: ExtensionSamplingMap,
    native_dimensions: DimensionsWire,
}

/// A retained operation contract. Capability validation remains a host step;
/// decoding proves intrinsic count, timing and generated-only sampling rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(into = "ExtensionGenerationPlanWire")]
pub struct ExtensionGenerationPlan {
    sampling: ExtensionSamplingMap,
    dimensions: NativeDimensions,
    requested_duration: ExactRatio,
    generated_duration: ExactRatio,
    native_movie_duration: ExactRatio,
    context_duration: ExactRatio,
    context_anchor_span: ExactRatio,
    speed: ExactRatio,
    retime_deviation: ExactRatio,
}

impl ExtensionGenerationPlan {
    pub fn new(
        direction: ExtensionDirection,
        project_frames: FrameDuration,
        project_rate: FrameRate,
        capability: &ExtensionCapability,
        dimensions: NativeDimensions,
    ) -> Result<Self, ExtensionPlanError> {
        if project_frames == FrameDuration::ZERO {
            return Err(ExtensionPlanError::ZeroProjectFrames);
        }
        if project_frames.frames() > capability.maximum_output_frames.frames() {
            return Err(ExtensionPlanError::OutputCountOutsideCapability);
        }
        validate_dimensions(capability.dimensions, dimensions)?;
        let ideal = duration(project_frames.frames(), project_rate)?
            .checked_mul(rate_ratio(capability.native_rate)?)
            .map_err(|_| ExtensionPlanError::ArithmeticOverflow)?;
        let generated = capability.nearest(ideal)?;
        let sampling = ExtensionSamplingMap::new(
            direction,
            project_rate,
            capability.native_rate,
            FrameDuration::new(i64::from(capability.context_frame_count))
                .map_err(|_| ExtensionPlanError::ArithmeticOverflow)?,
            FrameDuration::new(i64::from(generated))
                .map_err(|_| ExtensionPlanError::ArithmeticOverflow)?,
            project_frames,
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )
        .map_err(|_| ExtensionPlanError::InconsistentPlan)?;
        Self::from_parts(sampling, dimensions)
    }

    fn from_parts(
        sampling: ExtensionSamplingMap,
        dimensions: NativeDimensions,
    ) -> Result<Self, ExtensionPlanError> {
        let context = u32::try_from(sampling.context_frame_count().frames())
            .map_err(|_| ExtensionPlanError::InconsistentPlan)?;
        let generated = u32::try_from(sampling.generated_frame_count().frames())
            .map_err(|_| ExtensionPlanError::InconsistentPlan)?;
        if !(context - 1).is_multiple_of(8)
            || !generated.is_multiple_of(8)
            || context.checked_add(generated).is_none()
        {
            return Err(ExtensionPlanError::InconsistentPlan);
        }
        let requested_duration = duration(
            sampling.output_frame_count().frames(),
            sampling.project_rate(),
        )?;
        let generated_duration = duration(i64::from(generated), sampling.native_rate())?;
        let native_movie_duration = duration(
            sampling.native_frame_count().frames(),
            sampling.native_rate(),
        )?;
        let context_duration = duration(i64::from(context), sampling.native_rate())?;
        let context_anchor_span = duration(i64::from(context - 1), sampling.native_rate())?;
        let speed = generated_duration
            .checked_div(requested_duration)
            .map_err(|_| ExtensionPlanError::ArithmeticOverflow)?;
        let retime_deviation = generated_duration
            .checked_sub(requested_duration)
            .map_err(|_| ExtensionPlanError::ArithmeticOverflow)?;
        Ok(Self {
            sampling,
            dimensions,
            requested_duration,
            generated_duration,
            native_movie_duration,
            context_duration,
            context_anchor_span,
            speed,
            retime_deviation,
        })
    }

    pub const fn schema_version(&self) -> u32 {
        EXTENSION_GENERATION_PLAN_SCHEMA_VERSION
    }

    pub const fn sampling_map(&self) -> &ExtensionSamplingMap {
        &self.sampling
    }

    pub const fn direction(&self) -> ExtensionDirection {
        self.sampling.direction()
    }

    pub const fn project_frames(&self) -> FrameDuration {
        self.sampling.output_frame_count()
    }

    pub const fn project_frame_rate(&self) -> FrameRate {
        self.sampling.project_rate()
    }

    pub const fn native_frame_rate(&self) -> FrameRate {
        self.sampling.native_rate()
    }

    pub fn context_frame_count(&self) -> u32 {
        u32::try_from(self.sampling.context_frame_count().frames()).expect("validated native count")
    }

    pub fn generated_frame_count(&self) -> u32 {
        u32::try_from(self.sampling.generated_frame_count().frames())
            .expect("validated native count")
    }

    pub fn native_frame_count(&self) -> u32 {
        u32::try_from(self.sampling.native_frame_count().frames()).expect("validated native count")
    }

    /// The upstream extension argument counts new video latents, not pictures.
    pub fn generated_latent_frame_count(&self) -> u32 {
        self.generated_frame_count() / 8
    }

    pub const fn native_dimensions(&self) -> NativeDimensions {
        self.dimensions
    }

    /// Inserted interval N/P; never includes native conditioning context.
    pub const fn requested_duration(&self) -> ExactRatio {
        self.requested_duration
    }

    pub const fn generated_duration(&self) -> ExactRatio {
        self.generated_duration
    }

    pub const fn native_movie_duration(&self) -> ExactRatio {
        self.native_movie_duration
    }

    pub const fn context_duration(&self) -> ExactRatio {
        self.context_duration
    }

    /// Elapsed time between the first and last conditioning picture centers.
    pub const fn context_anchor_span(&self) -> ExactRatio {
        self.context_anchor_span
    }

    /// Nominal generated-interval speed `(E/R)/(N/P)`.
    pub const fn speed(&self) -> ExactRatio {
        self.speed
    }

    /// Signed seconds: generated interval minus requested inserted interval.
    pub const fn retime_deviation(&self) -> ExactRatio {
        self.retime_deviation
    }

    pub fn validate_for(&self, capability: &ExtensionCapability) -> Result<(), ExtensionPlanError> {
        let expected = Self::new(
            self.direction(),
            self.project_frames(),
            self.project_frame_rate(),
            capability,
            self.dimensions,
        )?;
        if *self != expected {
            return Err(ExtensionPlanError::CapabilityMismatch);
        }
        Ok(())
    }

    pub fn sample(&self, output_index: u64) -> Result<ExtensionSample, ExtensionPlanError> {
        let index =
            i64::try_from(output_index).map_err(|_| ExtensionPlanError::SampleOutOfRange)?;
        let position = self
            .sampling
            .native_position(index)
            .map_err(|_| ExtensionPlanError::SampleOutOfRange)?;
        let lower_index =
            u32::try_from(position.floor()).map_err(|_| ExtensionPlanError::ArithmeticOverflow)?;
        let upper_index = u32::try_from(
            position
                .ceil()
                .map_err(|_| ExtensionPlanError::ArithmeticOverflow)?,
        )
        .map_err(|_| ExtensionPlanError::ArithmeticOverflow)?;
        let upper_weight = position
            .checked_sub(ExactRatio::integer(i64::from(lower_index)))
            .map_err(|_| ExtensionPlanError::ArithmeticOverflow)?;
        Ok(ExtensionSample {
            output_index,
            native_position: position,
            lower_index,
            upper_index,
            upper_weight,
        })
    }
}

impl From<ExtensionGenerationPlan> for ExtensionGenerationPlanWire {
    fn from(value: ExtensionGenerationPlan) -> Self {
        Self {
            schema_version: EXTENSION_GENERATION_PLAN_SCHEMA_VERSION,
            operation: ExtensionOperation::Extension,
            sampling: value.sampling,
            native_dimensions: DimensionsWire {
                width: value.dimensions.width(),
                height: value.dimensions.height(),
            },
        }
    }
}

impl<'de> Deserialize<'de> for ExtensionGenerationPlan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ExtensionGenerationPlanWire::deserialize(deserializer)?;
        if wire.schema_version != EXTENSION_GENERATION_PLAN_SCHEMA_VERSION {
            return Err(de::Error::custom(
                "unsupported extension generation plan schema",
            ));
        }
        let ExtensionOperation::Extension = wire.operation;
        let dimensions =
            NativeDimensions::new(wire.native_dimensions.width, wire.native_dimensions.height)
                .map_err(de::Error::custom)?;
        Self::from_parts(wire.sampling, dimensions).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtensionSample {
    output_index: u64,
    native_position: ExactRatio,
    lower_index: u32,
    upper_index: u32,
    upper_weight: ExactRatio,
}

impl ExtensionSample {
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
    pub const fn upper_weight(self) -> ExactRatio {
        self.upper_weight
    }
    pub fn lower_weight(self) -> Result<ExactRatio, ExtensionPlanError> {
        ExactRatio::ONE
            .checked_sub(self.upper_weight)
            .map_err(|_| ExtensionPlanError::ArithmeticOverflow)
    }
}

fn validate_dimensions(
    limits: DimensionLimits,
    dimensions: NativeDimensions,
) -> Result<(), ExtensionPlanError> {
    for (axis, value) in [
        (limits.width(), dimensions.width()),
        (limits.height(), dimensions.height()),
    ] {
        if !(axis.minimum()..=axis.maximum()).contains(&value)
            || !value.is_multiple_of(axis.multiple())
        {
            return Err(ExtensionPlanError::UnsupportedDimensions);
        }
    }
    Ok(())
}

fn duration(count: i64, rate: FrameRate) -> Result<ExactRatio, ExtensionPlanError> {
    let numerator = i128::from(count)
        .checked_mul(i128::from(rate.denominator()))
        .ok_or(ExtensionPlanError::ArithmeticOverflow)?;
    ExactRatio::new(numerator, i128::from(rate.numerator()))
        .map_err(|_| ExtensionPlanError::ArithmeticOverflow)
}

fn rate_ratio(rate: FrameRate) -> Result<ExactRatio, ExtensionPlanError> {
    ExactRatio::new(i128::from(rate.numerator()), i128::from(rate.denominator()))
        .map_err(|_| ExtensionPlanError::ArithmeticOverflow)
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExtensionPlanError {
    #[error(
        "extension capability must advertise finite legal context, generated and output counts"
    )]
    InvalidCapability,
    #[error("extension output must contain at least one project frame")]
    ZeroProjectFrames,
    #[error("requested output count exceeds the extension capability")]
    OutputCountOutsideCapability,
    #[error("requested duration exceeds the extension capability")]
    DurationOutsideCapability,
    #[error("native dimensions are outside the extension capability")]
    UnsupportedDimensions,
    #[error("extension plan does not match the selected capability")]
    CapabilityMismatch,
    #[error("extension plan has inconsistent native counts")]
    InconsistentPlan,
    #[error("extension timing arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("extension output sample is outside the authored interval")]
    SampleOutOfRange,
}

#[cfg(test)]
mod tests;
