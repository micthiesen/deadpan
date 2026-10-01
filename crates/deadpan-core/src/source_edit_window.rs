//! Exact selected editorial time in a physical Source owner's local clock.

use serde::{Deserialize, Serialize};

use crate::{ExactRatio, FrameDuration, TimeError};

/// A positive half-open editorial interval, separate from integral allocation,
/// full affine media context, and independently selected picture/audio support.
/// Coordinates are physical-local frames after the independent audio offset;
/// SourceAudioMapping stores its selected audio support before that offset.
/// This records intent; qualification and linked-clock agreement belong to the
/// command resolver. Stream mappings remain authoritative for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "WindowWire")]
pub struct SourceEditWindow {
    start: ExactRatio,
    end: ExactRatio,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowWire {
    start: ExactRatio,
    end: ExactRatio,
}

impl TryFrom<WindowWire> for SourceEditWindow {
    type Error = TimeError;

    fn try_from(value: WindowWire) -> Result<Self, Self::Error> {
        Self::new(value.start, value.end)
    }
}

impl SourceEditWindow {
    /// Keep exact fractional endpoints within the nonnegative local frame clock.
    /// The owning Source's duration is checked separately by `validate`.
    pub fn new(start: ExactRatio, end: ExactRatio) -> Result<Self, TimeError> {
        if start.compare_integer(0).is_lt() {
            return Err(TimeError::InvalidRatio);
        }
        crate::source_mapping::validate_placement(start, end.checked_sub(start)?)?;
        Ok(Self { start, end })
    }

    pub const fn start(self) -> ExactRatio {
        self.start
    }

    pub const fn end(self) -> ExactRatio {
        self.end
    }

    /// A selected interval must fit its positive physical owner's allocation.
    pub fn validate(self, duration: FrameDuration) -> Result<(), TimeError> {
        Self::new(self.start, self.end)?;
        if self.end.compare_integer(duration.frames()).is_gt() {
            return Err(TimeError::InvalidRatio);
        }
        Ok(())
    }

    /// Translate retained intent when the physical Source gains a prefix.
    /// The caller must also grow the Source and translate its other local data
    /// atomically. This pure helper cannot make a negative prefix or mutate input.
    pub fn prepend_owner_frames(self, prefix: FrameDuration) -> Result<Self, TimeError> {
        let prefix = ExactRatio::integer(prefix.frames());
        Self::new(
            self.start.checked_add(prefix)?,
            self.end.checked_add(prefix)?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::{Value, json};

    fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
        ExactRatio::new(numerator, denominator).unwrap()
    }

    #[test]
    fn exact_window_is_positive_bounded_and_strict_at_serde_ingress() {
        let window = SourceEditWindow::new(ratio(1, 3), ratio(17, 3)).unwrap();
        let wire = serde_json::to_value(window).unwrap();
        assert_eq!(
            serde_json::from_value::<SourceEditWindow>(wire.clone()).unwrap(),
            window
        );
        assert_eq!(window.start(), ratio(1, 3));
        assert_eq!(window.end(), ratio(17, 3));
        assert!(window.validate(FrameDuration::new(6).unwrap()).is_ok());
        assert!(window.validate(FrameDuration::new(5).unwrap()).is_err());
        assert!(window.validate(FrameDuration::ZERO).is_err());
        for (start, end) in [
            (ratio(-1, 3), ratio(1, 3)),
            (ratio(1, 3), ratio(1, 3)),
            (ratio(2, 3), ratio(1, 3)),
            (
                ExactRatio::ZERO,
                ExactRatio::integer(i64::MAX)
                    .checked_add(ratio(1, 3))
                    .unwrap(),
            ),
        ] {
            assert!(SourceEditWindow::new(start, end).is_err());
            assert!(
                serde_json::from_value::<SourceEditWindow>(json!({"start":start,"end":end}))
                    .is_err()
            );
        }
        for field in ["start", "end"] {
            let mut missing = wire.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<SourceEditWindow>(missing).is_err());
            let mut null = wire.clone();
            null[field] = Value::Null;
            assert!(serde_json::from_value::<SourceEditWindow>(null).is_err());
        }
        let mut unknown = wire.clone();
        unknown["future"] = Value::Null;
        assert!(serde_json::from_value::<SourceEditWindow>(unknown).is_err());
        let raw = serde_json::to_string(&window).unwrap();
        let duplicate = raw.replacen(
            '{',
            "{\"st\\u0061rt\":{\"numerator\":\"0\",\"denominator\":\"1\"},",
            1,
        );
        assert!(serde_json::from_str::<SourceEditWindow>(&duplicate).is_err());
    }

    #[test]
    fn prefix_translation_is_exact_and_overflow_leaves_original_usable() {
        let window = SourceEditWindow::new(ratio(1, 7), ratio(11, 7)).unwrap();
        assert_eq!(
            window.prepend_owner_frames(FrameDuration::ZERO).unwrap(),
            window
        );
        let shifted = window
            .prepend_owner_frames(FrameDuration::new(3).unwrap())
            .unwrap();
        assert_eq!(shifted.start(), ratio(22, 7));
        assert_eq!(shifted.end(), ratio(32, 7));
        assert!(shifted.validate(FrameDuration::new(5).unwrap()).is_ok());
        assert!(
            window
                .prepend_owner_frames(FrameDuration::new(i64::MAX).unwrap())
                .is_err()
        );
        assert_eq!(window.start(), ratio(1, 7));
        assert_eq!(window.end(), ratio(11, 7));
        let maximum =
            SourceEditWindow::new(ExactRatio::ZERO, ExactRatio::integer(i64::MAX)).unwrap();
        assert!(
            maximum
                .validate(FrameDuration::new(i64::MAX).unwrap())
                .is_ok()
        );
        assert!(
            maximum
                .prepend_owner_frames(FrameDuration::new(1).unwrap())
                .is_err()
        );
    }

    proptest! {
        #[test]
        fn prefix_composition_retains_fractional_width(
            start in 0_i64..10_000,
            width in 1_i64..10_000,
            denominator in 1_i64..100,
            first in 0_i64..10_000,
            second in 0_i64..10_000,
        ) {
            let window = SourceEditWindow::new(
                ratio(i128::from(start), i128::from(denominator)),
                ratio(i128::from(start + width), i128::from(denominator)),
            ).unwrap();
            let composed = window
                .prepend_owner_frames(FrameDuration::new(first).unwrap()).unwrap()
                .prepend_owner_frames(FrameDuration::new(second).unwrap()).unwrap();
            prop_assert_eq!(composed, window.prepend_owner_frames(FrameDuration::new(first + second).unwrap()).unwrap());
            prop_assert_eq!(composed.end().checked_sub(composed.start()).unwrap(), window.end().checked_sub(window.start()).unwrap());
        }
    }
}
