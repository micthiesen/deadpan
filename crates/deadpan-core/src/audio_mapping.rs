//! Independent source-audio destination duration in the project frame clock.

use serde::{Deserialize, Serialize};

use crate::source_mapping::{natural_duration, validate_duration};
use crate::{ExactRatio, FrameDuration, FrameRate, SourceSpan, TimeError};

/// The selected original audio span maps linearly over this destination extent.
/// Its start is the Source node's signed mix-clock `audio_offset`. Enclosing
/// structural retimes compose with this mapping without intermediate rounding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceAudioMapping {
    /// Preserve the historical behavior of fitting audio to the entire beat.
    FitBeat,
    /// An independently authored positive duration in exact project frames.
    Duration { frames: ExactRatio },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum MappingWire {
    // A struct variant rejects extra fields; serde's tagged unit variant does not.
    FitBeat {},
    Duration { frames: ExactRatio },
}

impl<'de> Deserialize<'de> for SourceAudioMapping {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match MappingWire::deserialize(deserializer)? {
            MappingWire::FitBeat {} => Ok(Self::FitBeat),
            MappingWire::Duration { frames } => {
                validate_duration(frames).map_err(serde::de::Error::custom)?;
                Ok(Self::Duration { frames })
            }
        }
    }
}

impl SourceAudioMapping {
    /// Preserve the selected span's original rate, independently of picture
    /// duration. Timestamp origins do not affect duration or imply alignment.
    pub fn natural_rate(span: SourceSpan, rate: FrameRate) -> Result<Self, TimeError> {
        Ok(Self::Duration {
            frames: natural_duration(span, rate)?,
        })
    }

    pub fn duration_frames(self, beat: FrameDuration) -> Result<ExactRatio, TimeError> {
        let frames = match self {
            Self::FitBeat => ExactRatio::integer(beat.frames()),
            Self::Duration { frames } => frames,
        };
        validate_duration(frames)?;
        Ok(frames)
    }
}
