//! Closed source-picture vocabulary predating schema-35 selected placements.

use serde::{Deserialize, Serialize};

use crate::legacy_audio_mapping_v35::AudioMapping;
use crate::source_mapping::{validate_duration, validate_placement};
use crate::{
    AudioSample, EndpointPolicy, ExactRatio, FrameDuration, LinkRelation, SourceAudio, SourceNode,
    SourceVideo, SourceVideoMapping,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum VideoMapping {
    FitBeat,
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

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Wire {
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

impl<'de> Deserialize<'de> for VideoMapping {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Wire::deserialize(deserializer)? {
            Wire::FitBeat {} => Ok(Self::FitBeat),
            Wire::Duration { frames, endpoints } => {
                validate_duration(frames).map_err(serde::de::Error::custom)?;
                Ok(Self::Duration { frames, endpoints })
            }
            Wire::Placement {
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

impl VideoMapping {
    pub(crate) fn upgrade(self) -> SourceVideoMapping {
        match self {
            Self::FitBeat => SourceVideoMapping::FitBeat,
            Self::Duration { frames, endpoints } => {
                SourceVideoMapping::Duration { frames, endpoints }
            }
            Self::Placement {
                start,
                frames,
                endpoints,
            } => SourceVideoMapping::Placement {
                start,
                frames,
                endpoints,
            },
        }
    }

    pub(crate) fn project(mapping: SourceVideoMapping) -> Option<Self> {
        match mapping {
            SourceVideoMapping::FitBeat => Some(Self::FitBeat),
            SourceVideoMapping::Duration { frames, endpoints } => {
                Some(Self::Duration { frames, endpoints })
            }
            SourceVideoMapping::Placement {
                start,
                frames,
                endpoints,
            } => Some(Self::Placement {
                start,
                frames,
                endpoints,
            }),
            SourceVideoMapping::SelectedPlacement { .. } => None,
        }
    }
}

/// Schemas 20 onward admit selected audio but retain the older picture mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacySourceNode {
    duration: FrameDuration,
    video: SourceVideo,
    audio: Option<SourceAudio>,
    link: LinkRelation,
    audio_offset: AudioSample,
    audio_mapping: AudioMapping,
    video_mapping: VideoMapping,
}

impl LegacySourceNode {
    pub(crate) fn upgrade(self) -> SourceNode {
        SourceNode {
            duration: self.duration,
            video: self.video,
            audio: self.audio,
            link: self.link,
            audio_offset: self.audio_offset,
            audio_mapping: self.audio_mapping.upgrade(),
            video_mapping: self.video_mapping.upgrade(),
        }
    }

    pub(crate) fn project(source: &SourceNode) -> Option<Self> {
        Some(Self {
            duration: source.duration,
            video: source.video.clone(),
            audio: source.audio.clone(),
            link: source.link,
            audio_offset: source.audio_offset,
            audio_mapping: AudioMapping::project(source.audio_mapping)?,
            video_mapping: VideoMapping::project(source.video_mapping)?,
        })
    }
}
