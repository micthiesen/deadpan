//! Shared exact placement arithmetic for independently mapped source streams.

use crate::{ExactRatio, FrameRate, SourceSpan, TimeError};

pub(crate) fn natural_duration(span: SourceSpan, rate: FrameRate) -> Result<ExactRatio, TimeError> {
    let clock = span.start().time_base;
    let frames = ExactRatio::integer(span.end().ticks - span.start().ticks)
        .checked_mul(ExactRatio::new(
            i128::from(clock.numerator()),
            i128::from(clock.denominator()),
        )?)?
        .checked_mul(ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(rate.denominator()),
        )?)?;
    validate_duration(frames)?;
    Ok(frames)
}

pub(crate) fn validate_duration(frames: ExactRatio) -> Result<(), TimeError> {
    if frames.compare_integer(0) != std::cmp::Ordering::Greater {
        return Err(TimeError::InvalidRatio);
    }
    if frames.compare_integer(i64::MAX) == std::cmp::Ordering::Greater {
        return Err(TimeError::Overflow);
    }
    Ok(())
}

pub(crate) fn validate_placement(start: ExactRatio, frames: ExactRatio) -> Result<(), TimeError> {
    validate_duration(frames)?;
    for boundary in [start, start.checked_add(frames)?] {
        if boundary.compare_integer(i64::MIN).is_lt() || boundary.compare_integer(i64::MAX).is_gt()
        {
            return Err(TimeError::Overflow);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EndpointPolicy, SourceAudioMapping, SourceVideoMapping};
    use serde_json::{Value, json};

    #[test]
    fn placement_wire_requires_every_field_and_rejects_unknown_even_null_fields() {
        let audio = SourceAudioMapping::Placement {
            start: ExactRatio::new(-5, 3).unwrap(),
            frames: ExactRatio::new(61, 2).unwrap(),
        };
        let video = SourceVideoMapping::Placement {
            start: ExactRatio::new(7, 3).unwrap(),
            frames: ExactRatio::new(73, 2).unwrap(),
            endpoints: EndpointPolicy::HoldAdjacent,
        };
        let audio_wire = serde_json::to_value(audio).unwrap();
        let video_wire = serde_json::to_value(video).unwrap();
        assert_eq!(
            serde_json::from_value::<SourceAudioMapping>(audio_wire.clone()).unwrap(),
            audio
        );
        assert_eq!(
            serde_json::from_value::<SourceVideoMapping>(video_wire.clone()).unwrap(),
            video
        );
        for field in ["start", "frames", "endpoints"] {
            for null in [false, true] {
                let mut forged = video_wire.clone();
                if null {
                    forged[field] = Value::Null;
                } else {
                    forged.as_object_mut().unwrap().remove(field);
                }
                assert!(serde_json::from_value::<SourceVideoMapping>(forged).is_err());
                if field != "endpoints" {
                    let mut forged = audio_wire.clone();
                    if null {
                        forged[field] = Value::Null;
                    } else {
                        forged.as_object_mut().unwrap().remove(field);
                    }
                    assert!(serde_json::from_value::<SourceAudioMapping>(forged).is_err());
                }
            }
        }
        for original in [
            json!({"type":"fit_beat"}),
            json!({"type":"duration", "frames":{"numerator":"60","denominator":"1"}}),
            audio_wire,
        ] {
            let mut forged = original;
            forged["unexpected"] = Value::Null;
            assert!(serde_json::from_value::<SourceAudioMapping>(forged).is_err());
        }
        for original in [
            json!({"type":"fit_beat"}),
            json!({"type":"duration", "frames":{"numerator":"60","denominator":"1"}, "endpoints":"reject"}),
            video_wire,
        ] {
            let mut forged = original;
            forged["unexpected"] = Value::Null;
            assert!(serde_json::from_value::<SourceVideoMapping>(forged).is_err());
        }
        assert_eq!(SourceAudioMapping::FitBeat.start_frames(), ExactRatio::ZERO);
        assert_eq!(SourceVideoMapping::FitBeat.start_frames(), ExactRatio::ZERO);
        assert_eq!(
            SourceAudioMapping::Duration {
                frames: ExactRatio::ONE
            }
            .start_frames(),
            ExactRatio::ZERO
        );
        assert_eq!(
            SourceVideoMapping::Duration {
                frames: ExactRatio::ONE,
                endpoints: EndpointPolicy::Reject
            }
            .start_frames(),
            ExactRatio::ZERO
        );
    }
}
