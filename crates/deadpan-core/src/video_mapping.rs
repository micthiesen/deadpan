//! Exact source-picture placement, independent of the integer timeline duration.

use serde::{Deserialize, Serialize};

use crate::source_mapping::{natural_duration, validate_duration, validate_placement};
use crate::{EndpointPolicy, ExactRatio, FrameDuration, FrameRate, SourceSpan, TimeError};

/// A selected original video span maps over this exact project-frame extent.
/// Enclosing retimes compose with it. The beat's integer duration determines
/// timeline occupancy, never an implicit rate conversion for explicit mappings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceVideoMapping {
    /// Historical fitting of the entire span over the beat, with no endpoint hold.
    FitBeat,
    Duration {
        frames: ExactRatio,
        /// Explicit behavior outside the selected half-open source span.
        endpoints: EndpointPolicy,
    },
    /// Signed destination start and positive extent in exact project frames.
    Placement {
        start: ExactRatio,
        frames: ExactRatio,
        endpoints: EndpointPolicy,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum MappingWire {
    // Struct variants reject unknown fields, including fields with null values.
    FitBeat {},
    Duration {
        frames: ExactRatio,
        endpoints: EndpointPolicy,
    },
    Placement {
        start: ExactRatio,
        frames: ExactRatio,
        endpoints: EndpointPolicy,
    },
}

impl<'de> Deserialize<'de> for SourceVideoMapping {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match MappingWire::deserialize(deserializer)? {
            MappingWire::FitBeat {} => Ok(Self::FitBeat),
            MappingWire::Duration { frames, endpoints } => {
                validate_duration(frames).map_err(serde::de::Error::custom)?;
                Ok(Self::Duration { frames, endpoints })
            }
            MappingWire::Placement {
                start,
                frames,
                endpoints,
            } => {
                validate_placement(start, frames).map_err(serde::de::Error::custom)?;
                Ok(Self::Placement {
                    start,
                    frames,
                    endpoints,
                })
            }
        }
    }
}

impl SourceVideoMapping {
    /// Keep original elapsed time at the project's rational frame rate. The host
    /// chooses the beat duration and endpoint policy explicitly; neither the PTS
    /// origin nor quantization of the beat can change this rate.
    pub fn natural_rate(
        span: SourceSpan,
        rate: FrameRate,
        endpoints: EndpointPolicy,
    ) -> Result<Self, TimeError> {
        Ok(Self::Duration {
            frames: natural_duration(span, rate)?,
            endpoints,
        })
    }

    pub fn duration_frames(self, beat: FrameDuration) -> Result<ExactRatio, TimeError> {
        let frames = match self {
            Self::FitBeat => ExactRatio::integer(beat.frames()),
            Self::Duration { frames, .. } => frames,
            Self::Placement { start, frames, .. } => {
                validate_placement(start, frames)?;
                frames
            }
        };
        validate_duration(frames)?;
        Ok(frames)
    }

    /// Destination start before enclosing structural mappings; legacy modes start at zero.
    pub fn start_frames(self) -> ExactRatio {
        match self {
            Self::FitBeat | Self::Duration { .. } => ExactRatio::ZERO,
            Self::Placement { start, .. } => start,
        }
    }

    pub fn endpoints(self) -> EndpointPolicy {
        match self {
            Self::FitBeat => EndpointPolicy::Reject,
            Self::Duration { endpoints, .. } | Self::Placement { endpoints, .. } => endpoints,
        }
    }
}
