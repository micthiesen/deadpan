//! Operation identity retained with an accepted, materialized sampled master.

use serde::{Deserialize, Serialize};

use super::{BridgeInterpolation, BridgeSamplingMap, ExtensionSamplingMap, GeneratedError};
use crate::{ExactRatio, FrameDuration, FrameRate};

/// Accepted sampling keeps each operation's complete, validated mapping.
/// Native Extension context is retained evidence, never authored output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    content = "sampling",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum GeneratedSamplingMap {
    Bridge(BridgeSamplingMap),
    Extension(ExtensionSamplingMap),
}

impl GeneratedSamplingMap {
    pub const fn project_rate(&self) -> FrameRate {
        match self {
            Self::Bridge(map) => map.project_rate(),
            Self::Extension(map) => map.project_rate(),
        }
    }

    pub const fn native_rate(&self) -> FrameRate {
        match self {
            Self::Bridge(map) => map.native_rate(),
            Self::Extension(map) => map.native_rate(),
        }
    }

    /// Complete native movie count, including conditioning frames.
    pub const fn native_frame_count(&self) -> FrameDuration {
        match self {
            Self::Bridge(map) => map.native_frame_count(),
            Self::Extension(map) => map.native_frame_count(),
        }
    }

    pub const fn output_frame_count(&self) -> FrameDuration {
        match self {
            Self::Bridge(map) => map.output_frame_count(),
            Self::Extension(map) => map.output_frame_count(),
        }
    }

    pub const fn interpolation(&self) -> BridgeInterpolation {
        match self {
            Self::Bridge(map) => map.interpolation(),
            Self::Extension(map) => map.interpolation(),
        }
    }

    /// Original native position used to materialize an output picture.
    /// Accepted playback reads that picture from the sampled master directly.
    pub fn native_position(&self, output_index: i64) -> Result<ExactRatio, GeneratedError> {
        match self {
            Self::Bridge(map) => map.native_position(output_index),
            Self::Extension(map) => map.native_position(output_index),
        }
    }
}

impl From<BridgeSamplingMap> for GeneratedSamplingMap {
    fn from(map: BridgeSamplingMap) -> Self {
        Self::Bridge(map)
    }
}

impl From<ExtensionSamplingMap> for GeneratedSamplingMap {
    fn from(map: ExtensionSamplingMap) -> Self {
        Self::Extension(map)
    }
}
