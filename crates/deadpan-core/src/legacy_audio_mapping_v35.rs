//! Closed source-audio mapping semantics before schema-36 dormant support.

use serde::{Deserialize, Serialize};

use crate::source_mapping::{validate_duration, validate_placement, validate_selection};
use crate::{ExactFrameRange, ExactRatio, SourceAudioMapping};

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
    SelectedPlacement {
        start: ExactRatio,
        frames: ExactRatio,
        selection: ExactFrameRange,
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
    SelectedPlacement {
        start: ExactRatio,
        frames: ExactRatio,
        selection: ExactFrameRange,
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
            Wire::SelectedPlacement {
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

impl AudioMapping {
    pub(crate) fn upgrade(self) -> SourceAudioMapping {
        match self {
            Self::FitBeat => SourceAudioMapping::FitBeat,
            Self::Duration { frames } => SourceAudioMapping::Duration { frames },
            Self::Placement { start, frames } => SourceAudioMapping::Placement { start, frames },
            Self::SelectedPlacement {
                start,
                frames,
                selection,
            } => SourceAudioMapping::SelectedPlacement {
                start,
                frames,
                selection,
            },
        }
    }

    #[cfg(test)]
    pub(crate) fn project(mapping: SourceAudioMapping) -> Option<Self> {
        match mapping {
            SourceAudioMapping::FitBeat => Some(Self::FitBeat),
            SourceAudioMapping::Duration { frames } => Some(Self::Duration { frames }),
            SourceAudioMapping::Placement { start, frames } => {
                Some(Self::Placement { start, frames })
            }
            SourceAudioMapping::SelectedPlacement {
                start,
                frames,
                selection,
            } => {
                validate_selection(start, frames, selection).ok()?;
                Some(Self::SelectedPlacement {
                    start,
                    frames,
                    selection,
                })
            }
        }
    }
}

/// Catalog sounds require audible support in every supported schema.
pub(crate) fn sound_mapping<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<SourceAudioMapping, D::Error> {
    AudioMapping::deserialize(deserializer).map(AudioMapping::upgrade)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positive_selected_support_round_trips_but_empty_support_cannot_project() {
        let selected = |end| SourceAudioMapping::SelectedPlacement {
            start: ExactRatio::ZERO,
            frames: ExactRatio::integer(30),
            selection: ExactFrameRange {
                start: ExactRatio::integer(2),
                end: ExactRatio::integer(end),
            },
        };
        let positive = selected(8);
        assert_eq!(AudioMapping::project(positive).unwrap().upgrade(), positive);
        assert_eq!(
            serde_json::from_str::<AudioMapping>(&serde_json::to_string(&positive).unwrap())
                .unwrap()
                .upgrade(),
            positive
        );
        let empty = selected(2);
        assert!(AudioMapping::project(empty).is_none());
        let wire = serde_json::to_string(&empty).unwrap();
        assert!(serde_json::from_str::<AudioMapping>(&wire).is_err());
        assert!(
            serde_json::from_str::<AudioMapping>(
                &wire.replace("selected_placement", "selected_\\u0070lacement")
            )
            .is_err()
        );
    }
}
