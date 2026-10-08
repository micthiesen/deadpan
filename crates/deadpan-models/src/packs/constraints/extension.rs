//! Pack declarations for one-sided temporal generation. This is an independent
//! operation contract, not an inference from image-to-video or Bridge support.

use deadpan_core::{ExactRatio, ExtensionDirection, FrameDuration, FrameRate};
use deadpan_jobs::{
    DimensionLimits, ExtensionCapability, ExtensionGenerationPlan, FrameCountFormula,
    MAX_HOLD_INSTRUCTION_BYTES, MotionAmount, NativeDimensions,
};
use serde::{Deserialize, Serialize};

use super::{FrameCounts, ImageAxis, PackError, distinct};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionConstraints {
    pub native_frame_rate: FrameRate,
    pub context_frame_count: u32,
    /// Counts of generated pictures E, excluding the K conditioning pictures.
    pub generated_frame_counts: FrameCounts,
    pub width: ImageAxis,
    pub height: ImageAxis,
    /// Independent output allocation bound; it does not promise this many
    /// frames at every project rate.
    pub maximum_project_frames: u32,
    /// Exact seconds of authored time, independent of the project frame rate.
    /// Capability construction floors this duration onto the selected grid.
    pub maximum_requested_duration: ExactRatio,
    pub motion_amounts: Vec<MotionAmount>,
    pub maximum_instruction_bytes: u32,
}

impl ExtensionConstraints {
    pub fn validate(&self) -> Result<(), PackError> {
        let capability = self.base_capability()?;
        if self.maximum_requested_duration.compare_integer(0).is_le()
            || self
                .maximum_requested_duration
                .compare(
                    capability
                        .maximum_requested_duration()
                        .map_err(|_| invalid("extension native duration overflowed"))?,
                )
                .is_gt()
        {
            return Err(invalid(
                "extension authored duration must be positive and fit its generated interval",
            ));
        }
        Ok(())
    }

    /// Build the finite capability for one selected project grid. Both the
    /// exact duration ceiling and the declared output allocation bound apply.
    /// The Extension operation promises both directions; the chronological
    /// inputs stay ordered identically for either selected anchor.
    pub fn capability(&self, project_rate: FrameRate) -> Result<ExtensionCapability, PackError> {
        self.validate()?;
        validate_rate(project_rate)?;
        let rate = ExactRatio::new(
            i128::from(project_rate.numerator()),
            i128::from(project_rate.denominator()),
        )
        .map_err(|_| invalid("extension project rate overflowed"))?;
        let duration_frames = self
            .maximum_requested_duration
            .checked_mul(rate)
            .map_err(|_| invalid("extension authored duration overflowed"))?
            .floor();
        let maximum = duration_frames.min(i128::from(self.maximum_project_frames));
        if maximum <= 0 {
            return Err(invalid(
                "extension duration cannot contain one whole frame at this project rate",
            ));
        }
        self.with_output_limit(
            u32::try_from(maximum)
                .map_err(|_| invalid("extension project frame bound overflowed"))?,
        )
    }

    /// Plan only the operation, clock and duration declared by this
    /// pack. Constructing a plan does not attest installed model/runtime bytes.
    pub fn plan(
        &self,
        direction: ExtensionDirection,
        frames: FrameDuration,
        project_rate: FrameRate,
        dimensions: NativeDimensions,
    ) -> Result<ExtensionGenerationPlan, PackError> {
        let capability = self.capability(project_rate)?;
        ExtensionGenerationPlan::new(direction, frames, project_rate, &capability, dimensions)
            .map_err(|_| invalid("extension request is outside the selected pack capability"))
    }

    fn base_capability(&self) -> Result<ExtensionCapability, PackError> {
        validate_rate(self.native_frame_rate)?;
        if self.context_frame_count == 0
            || self.context_frame_count > 64
            || self.generated_frame_counts.minimum == 0
            || self
                .context_frame_count
                .checked_add(self.generated_frame_counts.maximum)
                .is_none_or(|frames| frames > 65_536)
            || self.maximum_project_frames == 0
            || self.maximum_project_frames > 65_536
            || !distinct(&self.motion_amounts, 3)
            || self.maximum_instruction_bytes == 0
            || self.maximum_instruction_bytes > MAX_HOLD_INSTRUCTION_BYTES as u32
        {
            return Err(invalid(
                "extension frame or prompt limits exceed their bounds",
            ));
        }
        self.with_output_limit(self.maximum_project_frames)
    }

    fn with_output_limit(&self, maximum: u32) -> Result<ExtensionCapability, PackError> {
        let counts = self.generated_frame_counts;
        let formula =
            FrameCountFormula::new(counts.step, counts.offset, counts.minimum, counts.maximum)
                .map_err(|_| invalid("invalid extension generated frame-count formula"))?;
        ExtensionCapability::new(
            self.native_frame_rate,
            self.context_frame_count,
            formula,
            DimensionLimits::new(self.width.limits()?, self.height.limits()?),
            FrameDuration::new(i64::from(maximum))
                .map_err(|_| invalid("invalid extension project frame bound"))?,
        )
        .map_err(|_| invalid("invalid extension context or generated frame counts"))
    }
}

fn validate_rate(rate: FrameRate) -> Result<(), PackError> {
    let numerator = u64::from(rate.numerator());
    let denominator = u64::from(rate.denominator());
    if numerator < denominator || numerator > 120 * denominator {
        return Err(invalid(
            "extension frame rates must be from 1 through 120 fps",
        ));
    }
    Ok(())
}

fn invalid(reason: &'static str) -> PackError {
    PackError::Manifest(reason)
}
