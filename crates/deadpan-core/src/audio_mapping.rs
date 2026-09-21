//! Independent source-audio destination placement in the project frame clock.

use serde::{Deserialize, Serialize};

use crate::source_mapping::{natural_duration, validate_duration, validate_placement};
use crate::{
    AudioSample, ExactRatio, FrameDuration, FrameRate, MIX_SAMPLE_RATE, SourceSpan, TimeError,
};

/// The selected original audio span maps linearly over this destination extent.
/// Its start adds the Source node's signed mix-clock `audio_offset`. Enclosing
/// structural retimes compose with this mapping without intermediate rounding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceAudioMapping {
    /// Preserve the historical behavior of fitting audio to the entire beat.
    FitBeat,
    /// An independently authored positive duration in exact project frames.
    Duration { frames: ExactRatio },
    /// An exact signed start and positive extent, independent of picture timing.
    Placement {
        start: ExactRatio,
        frames: ExactRatio,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum MappingWire {
    // A struct variant rejects extra fields; serde's tagged unit variant does not.
    FitBeat {},
    Duration {
        frames: ExactRatio,
    },
    Placement {
        start: ExactRatio,
        frames: ExactRatio,
    },
}

impl<'de> Deserialize<'de> for SourceAudioMapping {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match MappingWire::deserialize(deserializer)? {
            MappingWire::FitBeat {} => Ok(Self::FitBeat),
            MappingWire::Duration { frames } => {
                validate_duration(frames).map_err(serde::de::Error::custom)?;
                Ok(Self::Duration { frames })
            }
            MappingWire::Placement { start, frames } => {
                validate_placement(start, frames).map_err(serde::de::Error::custom)?;
                Ok(Self::Placement { start, frames })
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
            Self::Placement { start, frames } => {
                validate_placement(start, frames)?;
                frames
            }
        };
        validate_duration(frames)?;
        Ok(frames)
    }

    /// Destination start before the independent mix offset and structural mappings.
    pub fn start_frames(self) -> ExactRatio {
        match self {
            Self::FitBeat | Self::Duration { .. } => ExactRatio::ZERO,
            Self::Placement { start, .. } => start,
        }
    }

    /// Translate the authored placement by the independent 48 kHz mix offset.
    pub fn start_frames_with_offset(
        self,
        offset: AudioSample,
        rate: FrameRate,
    ) -> Result<ExactRatio, TimeError> {
        self.start_frames()
            .checked_add(ExactRatio::integer(offset.0).checked_mul(ExactRatio::new(
                i128::from(rate.numerator()),
                i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
            )?)?)
    }
}
