//! Closed source-audio mapping vocabulary for core schemas 8 through 19.

use serde::{Deserialize, Serialize};

use crate::source_mapping::{validate_duration, validate_placement};
use crate::{ExactRatio, SourceAudioMapping};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AudioMapping {
    FitBeat,
    Duration {
        frames: ExactRatio,
    },
    Placement {
        start: ExactRatio,
        frames: ExactRatio,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Wire {
    FitBeat {},
    Duration {
        frames: ExactRatio,
    },
    Placement {
        start: ExactRatio,
        frames: ExactRatio,
    },
}

impl<'de> Deserialize<'de> for AudioMapping {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Wire::deserialize(deserializer)? {
            Wire::FitBeat {} => Ok(Self::FitBeat),
            Wire::Duration { frames } => {
                validate_duration(frames).map_err(serde::de::Error::custom)?;
                Ok(Self::Duration { frames })
            }
            Wire::Placement { start, frames } => {
                validate_placement(start, frames).map_err(serde::de::Error::custom)?;
                Ok(Self::Placement { start, frames })
            }
        }
    }
}

impl AudioMapping {
    pub(crate) fn upgrade(self) -> SourceAudioMapping {
        match self {
            Self::FitBeat => SourceAudioMapping::FitBeat,
            Self::Duration { frames } => SourceAudioMapping::Duration { frames },
            Self::Placement { start, frames } => SourceAudioMapping::Placement { start, frames },
        }
    }

    pub(crate) fn project(mapping: SourceAudioMapping) -> Option<Self> {
        match mapping {
            SourceAudioMapping::FitBeat => Some(Self::FitBeat),
            SourceAudioMapping::Duration { frames } => Some(Self::Duration { frames }),
            SourceAudioMapping::Placement { start, frames } => {
                Some(Self::Placement { start, frames })
            }
            SourceAudioMapping::SelectedPlacement { .. } => None,
        }
    }
}
