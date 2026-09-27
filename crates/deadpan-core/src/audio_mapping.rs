//! Independent source-audio destination placement in the project frame clock.

use serde::{Deserialize, Serialize};

use crate::source_mapping::{natural_duration, validate_duration, validate_placement};
use crate::{
    AudioSample, ExactFrameRange, ExactRatio, FrameDuration, FrameRate, MIX_SAMPLE_RATE,
    SourceSpan, TimeError,
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
    /// Keep the full source span's affine mapping and select an exact half-open
    /// part of its output. The selection is in local project frames before
    /// `audio_offset`; it constrains audibility and filter support, never rate.
    SelectedPlacement {
        start: ExactRatio,
        frames: ExactRatio,
        selection: ExactFrameRange,
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
    SelectedPlacement {
        start: ExactRatio,
        frames: ExactRatio,
        selection: ExactFrameRange,
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
            MappingWire::SelectedPlacement {
                start,
                frames,
                selection,
            } => {
                validate_selection(start, frames, selection).map_err(serde::de::Error::custom)?;
                Ok(Self::SelectedPlacement {
                    start,
                    frames,
                    selection,
                })
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
            Self::SelectedPlacement {
                start,
                frames,
                selection,
            } => {
                validate_selection(start, frames, selection)?;
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
            Self::Placement { start, .. } | Self::SelectedPlacement { start, .. } => start,
        }
    }

    /// The meaningful audible interval, separate from the full-span mapping.
    /// Typed callers receive the same validation as deserialized commands.
    pub fn selection_frames(self, beat: FrameDuration) -> Result<ExactFrameRange, TimeError> {
        let frames = self.duration_frames(beat)?;
        Ok(match self {
            Self::SelectedPlacement { selection, .. } => selection,
            _ => ExactFrameRange {
                start: self.start_frames(),
                end: self.start_frames().checked_add(frames)?,
            },
        })
    }

    /// Translate both endpoints by the same exact offset as the source clock.
    pub fn selection_frames_with_offset(
        self,
        beat: FrameDuration,
        offset: AudioSample,
        rate: FrameRate,
    ) -> Result<ExactFrameRange, TimeError> {
        let selection = self.selection_frames(beat)?;
        let shift = offset_frames(offset, rate)?;
        if let Self::SelectedPlacement { start, frames, .. } = self {
            // Unlike legacy placement, this new vocabulary also closes its
            // offset-shifted signed frame endpoints before plan construction.
            validate_placement(start.checked_add(shift)?, frames)?;
        }
        Ok(ExactFrameRange {
            start: selection.start.checked_add(shift)?,
            end: selection.end.checked_add(shift)?,
        })
    }

    /// Translate the authored placement by the independent 48 kHz mix offset.
    pub fn start_frames_with_offset(
        self,
        offset: AudioSample,
        rate: FrameRate,
    ) -> Result<ExactRatio, TimeError> {
        self.start_frames()
            .checked_add(offset_frames(offset, rate)?)
    }
}

fn offset_frames(offset: AudioSample, rate: FrameRate) -> Result<ExactRatio, TimeError> {
    ExactRatio::integer(offset.0).checked_mul(ExactRatio::new(
        i128::from(rate.numerator()),
        i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
    )?)
}

fn validate_selection(
    start: ExactRatio,
    frames: ExactRatio,
    selection: ExactFrameRange,
) -> Result<(), TimeError> {
    validate_placement(start, frames)?;
    validate_duration(selection.end.checked_sub(selection.start)?)?;
    if selection
        .start
        .checked_sub(start)?
        .compare_integer(0)
        .is_lt()
        || start
            .checked_add(frames)?
            .checked_sub(selection.end)?
            .compare_integer(0)
            .is_lt()
    {
        return Err(TimeError::InvalidRatio);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn ratio(a: i128, b: i128) -> ExactRatio {
        ExactRatio::new(a, b).unwrap()
    }

    fn selected() -> SourceAudioMapping {
        SourceAudioMapping::SelectedPlacement {
            start: ratio(-1, 3),
            frames: ratio(10, 3),
            selection: ExactFrameRange {
                start: ratio(1, 7),
                end: ratio(5, 3),
            },
        }
    }

    #[test]
    fn selection_translates_without_changing_full_span_rate() {
        let mapping = selected();
        let beat = FrameDuration::new(60).unwrap();
        let rate = FrameRate::new(30000, 1001).unwrap();
        assert_eq!(mapping.duration_frames(beat).unwrap(), ratio(10, 3));
        assert_eq!(mapping.start_frames(), ratio(-1, 3));
        for offset in [AudioSample(-16016), AudioSample(16016)] {
            let shift = ExactRatio::integer(if offset.0 < 0 { -10 } else { 10 });
            let selection = mapping
                .selection_frames_with_offset(beat, offset, rate)
                .unwrap();
            assert_eq!(selection.start, ratio(1, 7).checked_add(shift).unwrap());
            assert_eq!(selection.end, ratio(5, 3).checked_add(shift).unwrap());
            assert_eq!(
                mapping.start_frames_with_offset(offset, rate).unwrap(),
                ratio(-1, 3).checked_add(shift).unwrap()
            );
        }
    }

    #[test]
    fn typed_and_wire_selection_validation_reject_the_same_invalid_intervals() {
        let beat = FrameDuration::new(1).unwrap();
        for (start, end) in [(-1, 1), (0, 0), (2, 1), (0, 3)] {
            let mapping = SourceAudioMapping::SelectedPlacement {
                start: ExactRatio::ZERO,
                frames: ExactRatio::integer(2),
                selection: ExactFrameRange {
                    start: ExactRatio::integer(start),
                    end: ExactRatio::integer(end),
                },
            };
            assert!(mapping.duration_frames(beat).is_err());
            assert!(mapping.selection_frames(beat).is_err());
            assert!(
                serde_json::from_value::<SourceAudioMapping>(
                    serde_json::to_value(mapping).unwrap()
                )
                .is_err()
            );
        }
        let mapping = SourceAudioMapping::SelectedPlacement {
            start: ExactRatio::integer(i64::MAX - 1),
            frames: ExactRatio::ONE,
            selection: ExactFrameRange {
                start: ExactRatio::integer(i64::MAX - 1),
                end: ExactRatio::integer(i64::MAX),
            },
        };
        assert!(mapping.selection_frames(beat).is_ok());
        assert!(
            mapping
                .selection_frames_with_offset(
                    beat,
                    AudioSample(48000),
                    FrameRate::new(1, 1).unwrap()
                )
                .is_err()
        );
        let overflow = SourceAudioMapping::SelectedPlacement {
            start: ExactRatio::integer(i64::MAX),
            frames: ExactRatio::ONE,
            selection: ExactFrameRange {
                start: ExactRatio::integer(i64::MAX),
                end: ExactRatio::new(i128::from(i64::MAX) + 1, 1).unwrap(),
            },
        };
        assert!(overflow.duration_frames(beat).is_err());
    }

    #[test]
    fn selected_wire_is_closed_and_requires_every_boundary() {
        let mapping = selected();
        let wire = serde_json::to_value(mapping).unwrap();
        assert_eq!(
            serde_json::from_value::<SourceAudioMapping>(wire.clone()).unwrap(),
            mapping
        );
        for field in ["start", "frames", "selection"] {
            for null in [false, true] {
                let mut value = wire.clone();
                if null {
                    value[field] = Value::Null;
                } else {
                    value.as_object_mut().unwrap().remove(field);
                }
                assert!(serde_json::from_value::<SourceAudioMapping>(value).is_err());
            }
        }
        for field in ["start", "end"] {
            let mut value = wire.clone();
            value["selection"].as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<SourceAudioMapping>(value).is_err());
        }
        let mut value = wire.clone();
        value["selection"]["future"] = Value::Null;
        assert!(serde_json::from_value::<SourceAudioMapping>(value).is_err());
        let mut value = wire;
        value["future"] = Value::Null;
        assert!(serde_json::from_value::<SourceAudioMapping>(value).is_err());
        for mut old in [
            json!({"type":"fit_beat"}),
            json!({"type":"duration", "frames": ExactRatio::ONE}),
            json!({"type":"placement", "start": ExactRatio::ZERO, "frames": ExactRatio::ONE}),
        ] {
            old["selection"] = Value::Null;
            assert!(serde_json::from_value::<SourceAudioMapping>(old).is_err());
        }
    }
}
