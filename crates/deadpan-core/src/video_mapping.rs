//! Exact source-picture placement, independent of the integer timeline duration.

use serde::{Deserialize, Serialize};

use crate::source_mapping::{
    natural_duration, validate_duration, validate_placement, validate_selection,
};
use crate::{
    EndpointPolicy, ExactFrameRange, ExactRatio, ExactSourceSpan, FrameDuration, FrameRate,
    SourcePoint, SourceSpan, TimeError,
};

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
    /// Keep the full source span's affine mapping while selecting an exact
    /// half-open interval in local project frames. Endpoint holds use this
    /// selection, so hidden source context cannot change the retained picture.
    SelectedPlacement {
        start: ExactRatio,
        frames: ExactRatio,
        selection: ExactFrameRange,
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
    SelectedPlacement {
        start: ExactRatio,
        frames: ExactRatio,
        selection: ExactFrameRange,
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
            MappingWire::SelectedPlacement {
                start,
                frames,
                selection,
                endpoints,
            } => {
                validate_selection(start, frames, selection).map_err(serde::de::Error::custom)?;
                Ok(Self::SelectedPlacement {
                    start,
                    frames,
                    selection,
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
            Self::SelectedPlacement {
                start,
                frames,
                selection,
                ..
            } => {
                validate_selection(start, frames, selection)?;
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
            Self::Placement { start, .. } | Self::SelectedPlacement { start, .. } => start,
        }
    }

    /// Meaningful picture selection, distinct from the full-span affine map.
    /// Typed values receive the same validation as deserialized mappings.
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

    /// Project the selected output interval into the original source clock
    /// without rounding either endpoint or refitting the source rate.
    pub fn selection_in_source(
        self,
        span: SourceSpan,
        beat: FrameDuration,
    ) -> Result<ExactSourceSpan, TimeError> {
        let frames = self.duration_frames(beat)?;
        if !matches!(self, Self::SelectedPlacement { .. }) {
            return Ok(span.into());
        }
        let selection = self.selection_frames(beat)?;
        let source_start = ExactRatio::integer(span.start().ticks);
        let source_duration = ExactRatio::integer(span.end().ticks - span.start().ticks);
        let project = |position: ExactRatio| -> Result<SourcePoint, TimeError> {
            Ok(SourcePoint {
                ticks: position
                    .checked_sub(self.start_frames())?
                    .checked_div(frames)?
                    .checked_mul(source_duration)?
                    .checked_add(source_start)?,
                time_base: span.start().time_base,
            })
        };
        ExactSourceSpan::new(project(selection.start)?, project(selection.end)?)
    }

    pub fn endpoints(self) -> EndpointPolicy {
        match self {
            Self::FitBeat => EndpointPolicy::Reject,
            Self::Duration { endpoints, .. }
            | Self::Placement { endpoints, .. }
            | Self::SelectedPlacement { endpoints, .. } => endpoints,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SourceTimeBase, SourceTimestamp};
    use serde_json::Value;

    fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
        ExactRatio::new(numerator, denominator).unwrap()
    }

    fn selected() -> SourceVideoMapping {
        SourceVideoMapping::SelectedPlacement {
            start: ratio(-1, 3),
            frames: ratio(10, 3),
            selection: ExactFrameRange {
                start: ratio(1, 6),
                end: ratio(5, 3),
            },
            endpoints: EndpointPolicy::HoldAdjacent,
        }
    }

    fn source_span() -> SourceSpan {
        let time_base = SourceTimeBase::new(1, 30_000).unwrap();
        SourceSpan::new(
            SourceTimestamp {
                ticks: -2002,
                time_base,
            },
            SourceTimestamp {
                ticks: 5005,
                time_base,
            },
        )
        .unwrap()
    }

    #[test]
    fn selected_picture_window_projects_exact_endpoints_without_refitting_the_source() {
        let mapping = selected();
        for count in [1, 999] {
            let beat = FrameDuration::new(count).unwrap();
            assert_eq!(mapping.start_frames(), ratio(-1, 3));
            assert_eq!(mapping.duration_frames(beat).unwrap(), ratio(10, 3));
            assert_eq!(mapping.endpoints(), EndpointPolicy::HoldAdjacent);
            assert_eq!(
                mapping.selection_frames(beat).unwrap(),
                ExactFrameRange {
                    start: ratio(1, 6),
                    end: ratio(5, 3),
                }
            );
            let projected = mapping.selection_in_source(source_span(), beat).unwrap();
            // Independent affine endpoint calculations in the original 1/30000
            // clock: -2002 + (position + 1/3) * 7007 * 3/10.
            assert_eq!(projected.start().ticks, ratio(-19019, 20));
            assert_eq!(projected.end().ticks, ratio(11011, 5));
            assert_eq!(projected.start().time_base, source_span().start().time_base);
            assert_eq!(projected.end().time_base, source_span().end().time_base);
        }
    }

    #[test]
    fn existing_mappings_keep_their_entire_selected_source_span() {
        let beat = FrameDuration::new(17).unwrap();
        for mapping in [
            SourceVideoMapping::FitBeat,
            SourceVideoMapping::Duration {
                frames: ratio(13, 7),
                endpoints: EndpointPolicy::Reject,
            },
            SourceVideoMapping::Placement {
                start: ratio(-17, 3),
                frames: ratio(13, 7),
                endpoints: EndpointPolicy::HoldAdjacent,
            },
        ] {
            assert_eq!(
                mapping.selection_in_source(source_span(), beat).unwrap(),
                ExactSourceSpan::from(source_span())
            );
            let selection = mapping.selection_frames(beat).unwrap();
            assert_eq!(selection.start, mapping.start_frames());
            assert_eq!(
                selection.end.checked_sub(selection.start).unwrap(),
                mapping.duration_frames(beat).unwrap()
            );
        }
        assert!(
            SourceVideoMapping::FitBeat
                .selection_in_source(source_span(), FrameDuration::ZERO)
                .is_err()
        );
    }

    #[test]
    fn typed_and_wire_picture_windows_reject_invalid_and_unbounded_selections() {
        let beat = FrameDuration::new(1).unwrap();
        for (start, end) in [(-1, 1), (0, 0), (2, 1), (0, 3)] {
            let mapping = SourceVideoMapping::SelectedPlacement {
                start: ExactRatio::ZERO,
                frames: ExactRatio::integer(2),
                selection: ExactFrameRange {
                    start: ExactRatio::integer(start),
                    end: ExactRatio::integer(end),
                },
                endpoints: EndpointPolicy::HoldAdjacent,
            };
            assert!(mapping.duration_frames(beat).is_err());
            assert!(mapping.selection_frames(beat).is_err());
            assert!(mapping.selection_in_source(source_span(), beat).is_err());
            assert!(
                serde_json::from_value::<SourceVideoMapping>(
                    serde_json::to_value(mapping).unwrap()
                )
                .is_err()
            );
        }
        let start = ExactRatio::integer(i64::MAX);
        let overflow = SourceVideoMapping::SelectedPlacement {
            start,
            frames: ExactRatio::ONE,
            selection: ExactFrameRange {
                start,
                end: start.checked_add(ExactRatio::ONE).unwrap(),
            },
            endpoints: EndpointPolicy::Reject,
        };
        assert!(overflow.selection_frames(beat).is_err());
        assert!(
            serde_json::from_value::<SourceVideoMapping>(serde_json::to_value(overflow).unwrap())
                .is_err()
        );
    }

    #[test]
    fn selected_picture_wire_is_closed_and_requires_every_field() {
        let original = selected();
        let wire = serde_json::to_value(original).unwrap();
        assert_eq!(
            serde_json::from_value::<SourceVideoMapping>(wire.clone()).unwrap(),
            original
        );
        for field in ["start", "frames", "selection", "endpoints"] {
            for null in [false, true] {
                let mut forged = wire.clone();
                if null {
                    forged[field] = Value::Null;
                } else {
                    forged.as_object_mut().unwrap().remove(field);
                }
                assert!(serde_json::from_value::<SourceVideoMapping>(forged).is_err());
            }
            let json = serde_json::to_string(&wire).unwrap();
            let duplicated = format!(
                "{{\"{field}\":{},{}",
                serde_json::to_string(&wire[field]).unwrap(),
                &json[1..]
            );
            assert!(serde_json::from_str::<SourceVideoMapping>(&duplicated).is_err());
        }
        for nested in [false, true] {
            let mut forged = wire.clone();
            let object = if nested {
                forged["selection"].as_object_mut().unwrap()
            } else {
                forged.as_object_mut().unwrap()
            };
            object.insert("unexpected".into(), Value::Null);
            assert!(serde_json::from_value::<SourceVideoMapping>(forged).is_err());
        }
    }
}
