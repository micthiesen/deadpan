//! Explicit data/host requirements in a signed pack, checked against the
//! shipped runtime before an update can be selected.

use deadpan_core::FrameRate;
use deadpan_jobs::{
    AxisLimits, BridgeCapability, DimensionLimits, FrameCountFormula, MAX_HOLD_INSTRUCTION_BYTES,
    MotionAmount,
};
use serde::{Deserialize, Serialize};

use super::{Operation, PackError};

mod extension;
pub use extension::ExtensionConstraints;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    AppleSilicon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accelerator {
    Cpu,
    Metal,
}

/// Stored weight precisions, including unquantized components in a mixed pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Precision {
    Quantized4,
    Float16,
    Bfloat16,
    Float32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacOsVersion {
    pub major: u32,
    pub minor: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HardwareRequirements {
    pub architecture: Architecture,
    pub minimum_macos: MacOsVersion,
    pub accelerators: Vec<Accelerator>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Conditioning {
    MonoAudio,
    LeftBoundaryImage,
    RightBoundaryImage,
    /// Chronological same-shot pictures ending or beginning at one anchor.
    /// An optional opposite seam is inspection evidence, never conditioning.
    ChronologicalVideo,
    FixedPrompt,
    UserInstructions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameCounts {
    pub step: u32,
    pub offset: u32,
    pub minimum: u32,
    pub maximum: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageAxis {
    pub minimum: u32,
    pub maximum: u32,
    pub multiple: u32,
}

impl ImageAxis {
    fn limits(self) -> Result<AxisLimits, PackError> {
        if self.maximum > 16_384 {
            return Err(PackError::Manifest("image axis exceeds the manifest bound"));
        }
        AxisLimits::new(self.minimum, self.maximum, self.multiple)
            .map_err(|_| PackError::Manifest("invalid image axis constraints"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeConstraints {
    pub native_frame_rate: FrameRate,
    pub frame_counts: FrameCounts,
    pub width: ImageAxis,
    pub height: ImageAxis,
    pub maximum_project_frames: u32,
    pub motion_amounts: Vec<MotionAmount>,
    pub maximum_instruction_bytes: u32,
}

impl BridgeConstraints {
    pub fn capability(&self) -> Result<BridgeCapability, PackError> {
        if self.frame_counts.minimum < 2
            || self.frame_counts.maximum > 65_536
            || self.maximum_project_frames == 0
            || self.maximum_project_frames > 65_536
            || !distinct(&self.motion_amounts, 3)
            || self.maximum_instruction_bytes == 0
            || self.maximum_instruction_bytes > MAX_HOLD_INSTRUCTION_BYTES as u32
        {
            return Err(PackError::Manifest(
                "bridge frame or prompt limits exceed their bounds",
            ));
        }
        let counts = self.frame_counts;
        let formula =
            FrameCountFormula::new(counts.step, counts.offset, counts.minimum, counts.maximum)
                .map_err(|_| PackError::Manifest("invalid bridge frame-count formula"))?;
        Ok(BridgeCapability::new(
            true,
            self.native_frame_rate,
            formula,
            DimensionLimits::new(self.width.limits()?, self.height.limits()?),
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioConstraints {
    pub sample_rate: u32,
    pub channels: u32,
    pub maximum_samples: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackConstraints {
    pub hardware: HardwareRequirements,
    pub weight_precisions: Vec<Precision>,
    pub conditioning: Vec<Conditioning>,
    pub audio: Option<AudioConstraints>,
    pub bridge: Option<BridgeConstraints>,
    /// Omitted from existing manifests to preserve their signed wire bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<ExtensionConstraints>,
}

fn distinct<T: PartialEq>(values: &[T], maximum: usize) -> bool {
    !values.is_empty()
        && values.len() <= maximum
        && values
            .iter()
            .enumerate()
            .all(|(i, value)| !values[..i].contains(value))
}

impl PackConstraints {
    pub(super) fn validate(&self, operations: &[Operation]) -> Result<(), PackError> {
        let invalid = |reason| PackError::Manifest(reason);
        if !(15..=99).contains(&self.hardware.minimum_macos.major)
            || self.hardware.minimum_macos.minor > 99
            || !distinct(&self.hardware.accelerators, 2)
            || !distinct(&self.weight_precisions, 4)
            || !distinct(&self.conditioning, 6)
        {
            return Err(invalid(
                "invalid hardware, precision or conditioning constraints",
            ));
        }
        let bridge = operations.contains(&Operation::BridgeHold);
        let extension = operations.contains(&Operation::ExtensionHold);
        let audio = operations.iter().any(|operation| {
            matches!(operation, Operation::Transcribe | Operation::SpeechActivity)
        });
        if self.bridge.is_some() != bridge
            || self.extension.is_some() != extension
            || self.audio.is_some() != audio
        {
            return Err(invalid("operation and media constraints disagree"));
        }
        for (input, supported) in [
            (Conditioning::MonoAudio, audio),
            (Conditioning::LeftBoundaryImage, bridge),
            (Conditioning::RightBoundaryImage, bridge),
            (Conditioning::ChronologicalVideo, extension),
            (Conditioning::FixedPrompt, bridge || extension),
            (Conditioning::UserInstructions, bridge || extension),
        ] {
            if self.conditioning.contains(&input) != supported {
                return Err(invalid("conditioning does not match supported operations"));
            }
        }
        if let Some(bridge) = &self.bridge {
            bridge.capability()?;
            if !self.hardware.accelerators.contains(&Accelerator::Metal) {
                return Err(invalid("the bridge runtime requires Metal"));
            }
        }
        if let Some(extension) = &self.extension {
            extension.validate()?;
            if !self.hardware.accelerators.contains(&Accelerator::Metal) {
                return Err(invalid("the extension runtime requires Metal"));
            }
        }
        if let Some(audio) = self.audio
            && (audio.sample_rate != deadpan_jobs::transcription::SAMPLE_RATE
                || audio.channels != 1
                || audio.maximum_samples == 0
                || audio.maximum_samples > deadpan_jobs::transcription::MAX_ANALYSIS_FRAMES)
        {
            return Err(invalid("unsupported analysis audio constraints"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) mod extension_tests;
