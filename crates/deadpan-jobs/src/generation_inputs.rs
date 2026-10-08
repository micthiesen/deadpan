//! Shared wire types for bounded generation inputs.
//!
//! These records carry observations, not admission authority. The store owns
//! capture budgets and independently recaptures inputs before granting request
//! relevance or accepting generated media.

use crate::{ConditioningMode, GenerationPlan};
use deadpan_core::{
    AttentionTarget, ExactRatio, ExtensionDirection, FrameDuration, FrameRate, GeneratedObjectRef,
    SourceFrameId, SourceQualificationId, TargetId,
};
use serde::{Deserialize, Serialize};

pub const MAX_INPUT_BINDING_BYTES: usize = 512 * 1024;

/// One measured picture identity used by generation and continuity inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GenerationPictureIdentity {
    Original {
        qualification: SourceQualificationId,
        frame: SourceFrameId,
    },
    Generated {
        sampled_object: GeneratedObjectRef,
        frame: SourceFrameId,
        /// Reduced positive ratio used by the conditioning decoder's centered
        /// fill_canvas_aspect crop. None preserves the complete decoded raster.
        /// Raster dimensions are unavailable here: different ratios which round
        /// to the same pixel crop may conservatively compare unequal. Never
        /// infer those dimensions or treat None as an assumed native aspect.
        content_aspect: Option<[u32; 2]>,
    },
    AuthoredBlack,
}

/// The operation resolved before asynchronous preparation. A policy change
/// requires a new capture; it cannot relabel earlier measurements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum GenerationCaptureSpec {
    Bridge,
    Extension {
        direction: ExtensionDirection,
        native_rate: FrameRate,
        context_frames: u32,
        policy: ExtensionCapturePolicy,
    },
}

impl<'de> Deserialize<'de> for GenerationCaptureSpec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Serde's internally tagged unit variant ignores extra fields even
        // with deny_unknown_fields. A zero-field struct variant is strict.
        #[derive(Deserialize)]
        #[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Bridge {},
            Extension {
                direction: ExtensionDirection,
                native_rate: FrameRate,
                context_frames: u32,
                policy: ExtensionCapturePolicy,
            },
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::Bridge {} => Self::Bridge,
            Wire::Extension {
                direction,
                native_rate,
                context_frames,
                policy,
            } => Self::Extension {
                direction,
                native_rate,
                context_frames,
                policy,
            },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionCapturePolicy {
    TemporalContextV1,
}

impl GenerationCaptureSpec {
    pub fn conditioning(self) -> ConditioningMode {
        match self {
            Self::Bridge => ConditioningMode::Bridge,
            Self::Extension {
                direction: ExtensionDirection::FromLeft,
                ..
            } => ConditioningMode::ExtendFromLeft,
            Self::Extension {
                direction: ExtensionDirection::FromRight,
                ..
            } => ConditioningMode::ExtendFromRight,
        }
    }

    pub fn from_plan(plan: &GenerationPlan) -> Self {
        match plan {
            GenerationPlan::Bridge(_) => Self::Bridge,
            GenerationPlan::Extension(plan) => Self::Extension {
                direction: plan.direction(),
                native_rate: plan.native_frame_rate(),
                context_frames: plan.context_frame_count(),
                policy: ExtensionCapturePolicy::TemporalContextV1,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelativeGenerationPicture {
    pub position: ExactRatio,
    pub picture: GenerationPictureIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationInputSupport {
    pub start: ExactRatio,
    pub end_exclusive: ExactRatio,
    pub first: GenerationPictureIdentity,
    pub last: GenerationPictureIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum GenerationInputs {
    Bridge {
        left: Option<GenerationPictureIdentity>,
        right: Option<GenerationPictureIdentity>,
    },
    Extension {
        capture: GenerationCaptureSpec,
        samples: Vec<RelativeGenerationPicture>,
        /// Explicitly unconditioned, even when a picture is present.
        opposite: Option<RelativeGenerationPicture>,
        support: Vec<GenerationInputSupport>,
        /// The closed endpoint is separate from half-open affine spans.
        terminal: RelativeGenerationPicture,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationRegionIdentity {
    pub id: TargetId,
    /// None retains the identity of a selected target removed by a later edit.
    pub record: Option<AttentionTarget>,
}

/// Immutable operation and region selection used when recapturing inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationInputSettings {
    pub capture: GenerationCaptureSpec,
    pub region: Option<TargetId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationInputBinding {
    pub duration: FrameDuration,
    pub frame_rate: FrameRate,
    pub canvas: [u32; 2],
    pub inputs: GenerationInputs,
    pub region: Option<GenerationRegionIdentity>,
}

impl GenerationInputBinding {
    pub fn capture_spec(&self) -> GenerationCaptureSpec {
        match &self.inputs {
            GenerationInputs::Bridge { .. } => GenerationCaptureSpec::Bridge,
            GenerationInputs::Extension { capture, .. } => *capture,
        }
    }

    pub fn settings(&self) -> GenerationInputSettings {
        GenerationInputSettings {
            capture: self.capture_spec(),
            region: self.region.as_ref().map(|region| region.id.clone()),
        }
    }
}
