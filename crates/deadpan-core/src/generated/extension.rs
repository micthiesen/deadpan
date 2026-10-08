//! Exact sampling of generated frames beside retained conditioning context.

use std::ops::Range;

use serde::{Deserialize, Deserializer, Serialize, de};

use super::{BridgeInterpolation, GeneratedError, GeneratedErrorCode};
use crate::{ExactRatio, FrameDuration, FrameRate};

pub const EXTENSION_SAMPLING_SCHEMA_VERSION: u32 = 1;

/// The available conditioning side. Context is retained in chronological order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionDirection {
    FromLeft,
    FromRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionSamplingPolicy {
    FrameCentersClamped,
}

/// Immutable operation facts, independent of any provider's legal count formula.
///
/// Only the generated interval is sampled into the authored output. The native
/// movie also retains context, which is never an extra output handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(into = "ExtensionSamplingMapWire")]
pub struct ExtensionSamplingMap {
    direction: ExtensionDirection,
    project_rate: FrameRate,
    native_rate: FrameRate,
    context_frame_count: FrameDuration,
    generated_frame_count: FrameDuration,
    output_frame_count: FrameDuration,
    interpolation: BridgeInterpolation,
    native_frame_count: FrameDuration,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtensionSamplingMapWire {
    schema_version: u32,
    direction: ExtensionDirection,
    project_rate: FrameRate,
    native_rate: FrameRate,
    context_frame_count: FrameDuration,
    generated_frame_count: FrameDuration,
    output_frame_count: FrameDuration,
    interpolation: BridgeInterpolation,
    policy: ExtensionSamplingPolicy,
}

impl ExtensionSamplingMap {
    pub fn new(
        direction: ExtensionDirection,
        project_rate: FrameRate,
        native_rate: FrameRate,
        context_frame_count: FrameDuration,
        generated_frame_count: FrameDuration,
        output_frame_count: FrameDuration,
        interpolation: BridgeInterpolation,
    ) -> Result<Self, GeneratedError> {
        if context_frame_count == FrameDuration::ZERO
            || generated_frame_count == FrameDuration::ZERO
            || output_frame_count == FrameDuration::ZERO
        {
            return Err(GeneratedError::new(
                GeneratedErrorCode::InvalidSamplingMap,
                "extension sampling requires positive context, generated and output counts",
            ));
        }
        let total = context_frame_count
            .frames()
            .checked_add(generated_frame_count.frames())
            .ok_or_else(overflow)?;
        let native_frame_count = FrameDuration::new(total).map_err(|_| overflow())?;
        Ok(Self {
            direction,
            project_rate,
            native_rate,
            context_frame_count,
            generated_frame_count,
            output_frame_count,
            interpolation,
            native_frame_count,
        })
    }

    pub const fn direction(&self) -> ExtensionDirection {
        self.direction
    }

    pub const fn project_rate(&self) -> FrameRate {
        self.project_rate
    }

    pub const fn native_rate(&self) -> FrameRate {
        self.native_rate
    }

    pub const fn context_frame_count(&self) -> FrameDuration {
        self.context_frame_count
    }

    pub const fn generated_frame_count(&self) -> FrameDuration {
        self.generated_frame_count
    }

    pub const fn output_frame_count(&self) -> FrameDuration {
        self.output_frame_count
    }

    pub const fn native_frame_count(&self) -> FrameDuration {
        self.native_frame_count
    }

    pub const fn interpolation(&self) -> BridgeInterpolation {
        self.interpolation
    }

    pub const fn policy(&self) -> ExtensionSamplingPolicy {
        ExtensionSamplingPolicy::FrameCentersClamped
    }

    /// Half-open native picture ordinals, not project-frame coordinates.
    pub fn generated_interval(&self) -> Range<i64> {
        match self.direction {
            ExtensionDirection::FromLeft => {
                self.context_frame_count.frames()..self.native_frame_count.frames()
            }
            ExtensionDirection::FromRight => 0..self.generated_frame_count.frames(),
        }
    }

    /// Half-open native picture ordinals containing only retained context.
    pub fn context_interval(&self) -> Range<i64> {
        match self.direction {
            ExtensionDirection::FromLeft => 0..self.context_frame_count.frames(),
            ExtensionDirection::FromRight => {
                self.generated_frame_count.frames()..self.native_frame_count.frames()
            }
        }
    }

    pub fn output_interval(&self) -> Range<i64> {
        0..self.output_frame_count.frames()
    }

    /// Native ordinal of the conditioning picture adjacent to generated output.
    pub const fn context_anchor_index(&self) -> i64 {
        match self.direction {
            ExtensionDirection::FromLeft => self.context_frame_count.frames() - 1,
            ExtensionDirection::FromRight => self.generated_frame_count.frames(),
        }
    }

    /// `S + clamp(((2*j+1)*E-N)/(2*N), 0, E-1)` for output `j`.
    /// Both interpolation fetches therefore remain in the generated interval.
    pub fn native_position(&self, output_index: i64) -> Result<ExactRatio, GeneratedError> {
        if !self.output_interval().contains(&output_index) {
            return Err(GeneratedError::new(
                GeneratedErrorCode::SamplingIndexOutOfRange,
                "extension output index is outside the authored frame interval",
            ));
        }
        let generated = i128::from(self.generated_frame_count.frames());
        let output = i128::from(self.output_frame_count.frames());
        let denominator = output.checked_mul(2).ok_or_else(overflow)?;
        let numerator = i128::from(output_index)
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .and_then(|value| value.checked_mul(generated))
            .and_then(|value| value.checked_sub(output))
            .ok_or_else(overflow)?;
        let last = (generated - 1)
            .checked_mul(denominator)
            .ok_or_else(overflow)?;
        let offset = numerator.clamp(0, last);
        let start = i128::from(self.generated_interval().start);
        let absolute = start
            .checked_mul(denominator)
            .and_then(|value| value.checked_add(offset))
            .ok_or_else(overflow)?;
        ExactRatio::new(absolute, denominator).map_err(|_| overflow())
    }
}

fn overflow() -> GeneratedError {
    GeneratedError::new(
        GeneratedErrorCode::SamplingOverflow,
        "extension sampling coordinate or native count overflowed",
    )
}

impl From<ExtensionSamplingMap> for ExtensionSamplingMapWire {
    fn from(value: ExtensionSamplingMap) -> Self {
        Self {
            schema_version: EXTENSION_SAMPLING_SCHEMA_VERSION,
            direction: value.direction,
            project_rate: value.project_rate,
            native_rate: value.native_rate,
            context_frame_count: value.context_frame_count,
            generated_frame_count: value.generated_frame_count,
            output_frame_count: value.output_frame_count,
            interpolation: value.interpolation,
            policy: ExtensionSamplingPolicy::FrameCentersClamped,
        }
    }
}

impl<'de> Deserialize<'de> for ExtensionSamplingMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ExtensionSamplingMapWire::deserialize(deserializer)?;
        if wire.schema_version != EXTENSION_SAMPLING_SCHEMA_VERSION {
            return Err(de::Error::custom("unsupported extension sampling schema"));
        }
        let ExtensionSamplingPolicy::FrameCentersClamped = wire.policy;
        Self::new(
            wire.direction,
            wire.project_rate,
            wire.native_rate,
            wire.context_frame_count,
            wire.generated_frame_count,
            wire.output_frame_count,
            wire.interpolation,
        )
        .map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests;
