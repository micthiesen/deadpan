//! Frozen source mapping vocabulary shared by core schemas 6 and 7.

use serde::{Deserialize, Serialize};

use crate::source_mapping::validate_duration;
use crate::{EndpointPolicy, ExactRatio, SourceAudioMapping, SourceVideoMapping};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AudioMapping {
    FitBeat,
    Duration { frames: ExactRatio },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum AudioWire {
    FitBeat {},
    Duration { frames: ExactRatio },
}

impl<'de> Deserialize<'de> for AudioMapping {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match AudioWire::deserialize(deserializer)? {
            AudioWire::FitBeat {} => Ok(Self::FitBeat),
            AudioWire::Duration { frames } => {
                validate_duration(frames).map_err(serde::de::Error::custom)?;
                Ok(Self::Duration { frames })
            }
        }
    }
}

impl AudioMapping {
    pub(crate) fn upgrade(self) -> SourceAudioMapping {
        match self {
            Self::FitBeat => SourceAudioMapping::FitBeat,
            Self::Duration { frames } => SourceAudioMapping::Duration { frames },
        }
    }

    pub(crate) fn project(mapping: SourceAudioMapping) -> Option<Self> {
        match mapping {
            SourceAudioMapping::FitBeat => Some(Self::FitBeat),
            SourceAudioMapping::Duration { frames } => Some(Self::Duration { frames }),
            SourceAudioMapping::Placement { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum VideoMapping {
    FitBeat,
    Duration {
        frames: ExactRatio,
        endpoints: EndpointPolicy,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum VideoWire {
    FitBeat {},
    Duration {
        frames: ExactRatio,
        endpoints: EndpointPolicy,
    },
}

impl<'de> Deserialize<'de> for VideoMapping {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match VideoWire::deserialize(deserializer)? {
            VideoWire::FitBeat {} => Ok(Self::FitBeat),
            VideoWire::Duration { frames, endpoints } => {
                validate_duration(frames).map_err(serde::de::Error::custom)?;
                Ok(Self::Duration { frames, endpoints })
            }
        }
    }
}

impl VideoMapping {
    pub(crate) fn upgrade(self) -> SourceVideoMapping {
        match self {
            Self::FitBeat => SourceVideoMapping::FitBeat,
            Self::Duration { frames, endpoints } => {
                SourceVideoMapping::Duration { frames, endpoints }
            }
        }
    }

    pub(crate) fn project(mapping: SourceVideoMapping) -> Option<Self> {
        match mapping {
            SourceVideoMapping::FitBeat => Some(Self::FitBeat),
            SourceVideoMapping::Duration { frames, endpoints } => {
                Some(Self::Duration { frames, endpoints })
            }
            SourceVideoMapping::Placement { .. } => None,
        }
    }
}
