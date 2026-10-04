//! Per-play escalation of a Repeat: each play after the first adds gain and
//! changes picture scale by a fixed step.
//!
//! Specification §8.1 "Escalation" and §8.2 "Escalating crop": one editable
//! Repeat whose plays progress, without duplicating authored clips. Play `k`
//! (0-based, in current play order) receives `k · gain_step` and a centered
//! scale of `1 + k · step` (additive, the default) or `step^k`
//! (multiplicative). The first play is always unchanged. Values are exact; the
//! scale is quantized once to the framing grid. Changing the play count keeps
//! the steps, so a fourth play continues the same progression.

use serde::{Deserialize, Serialize};

use crate::{
    ExactRatio, FRAMING_NUMERIC_SCALE, FramingPose, GainDb, MAX_GAIN_MILLIDECIBELS,
    MIN_GAIN_MILLIDECIBELS,
};

/// How the scale step accumulates over plays.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoomProgression {
    /// `1 + k · step`.
    #[default]
    Add,
    /// `step^k`.
    Multiply,
}

impl ZoomProgression {
    fn is_add(&self) -> bool {
        matches!(self, Self::Add)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZoomStep {
    /// Additive scale per play, or the factor per play when multiplying.
    pub step: ExactRatio,
    #[serde(default, skip_serializing_if = "ZoomProgression::is_add")]
    pub progression: ZoomProgression,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepeatEscalation {
    /// Gain added for each play after the first.
    #[serde(default, skip_serializing_if = "is_unity")]
    pub gain_step: GainDb,
    /// Centered picture scale progression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<ZoomStep>,
}

fn is_unity(gain: &GainDb) -> bool {
    *gain == GainDb::UNITY
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscalationError {
    Empty,
    GainRange,
    ZoomGrid,
    ZoomFactor,
    ZoomUnchanged,
    ScaleRange,
}

impl std::fmt::Display for EscalationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "an escalation must change gain or scale",
            Self::GainRange => {
                "the last play's gain would leave -96 dB..=+24 dB; lower the gain step or the play count"
            }
            Self::ZoomGrid => "a zoom step must lie on the framing grid (1/4294967296)",
            Self::ZoomFactor => "a multiplying zoom step must be positive",
            Self::ZoomUnchanged => "a zoom step must change the scale (not +0 or ×1)",
            Self::ScaleRange => {
                "the last play's scale would leave 1/64..=64; lower the zoom step or the play count"
            }
        })
    }
}

impl std::error::Error for EscalationError {}

impl RepeatEscalation {
    /// Check the step and every play of a Repeat with `plays` total plays.
    pub fn validate(&self, plays: u32) -> Result<(), EscalationError> {
        if self.gain_step == GainDb::UNITY && self.zoom.is_none() {
            return Err(EscalationError::Empty);
        }
        let last = plays.saturating_sub(1);
        let gain = self.gain_millidecibels(last);
        if gain < i64::from(MIN_GAIN_MILLIDECIBELS) || gain > i64::from(MAX_GAIN_MILLIDECIBELS) {
            return Err(EscalationError::GainRange);
        }
        if let Some(zoom) = self.zoom {
            if !on_grid(zoom.step) {
                return Err(EscalationError::ZoomGrid);
            }
            if zoom.progression == ZoomProgression::Multiply
                && !zoom.step.compare_integer(0).is_gt()
            {
                return Err(EscalationError::ZoomFactor);
            }
            let identity = match zoom.progression {
                ZoomProgression::Add => ExactRatio::ZERO,
                ZoomProgression::Multiply => ExactRatio::ONE,
            };
            if zoom.step == identity {
                return Err(EscalationError::ZoomUnchanged);
            }
            // Scale is monotonic in the play index, so the first and last
            // plays bound every play.
            self.pose(last).map_err(|_| EscalationError::ScaleRange)?;
        }
        Ok(())
    }

    /// Exact gain of 0-based play `play`, in millidecibels.
    pub fn gain_millidecibels(&self, play: u32) -> i64 {
        i64::from(self.gain_step.millidecibels()) * i64::from(play)
    }

    /// Centered scale pose of 0-based play `play`, or `None` for identity.
    pub fn pose(&self, play: u32) -> Result<Option<FramingPose>, EscalationError> {
        let Some(zoom) = self.zoom else {
            return Ok(None);
        };
        if play == 0 {
            return Ok(None);
        }
        let scale = match zoom.progression {
            ZoomProgression::Add => ExactRatio::integer(i64::from(play))
                .checked_mul(zoom.step)
                .and_then(|delta| ExactRatio::ONE.checked_add(delta)),
            ZoomProgression::Multiply => power(zoom.step, play),
        }
        .map_err(|_| EscalationError::ScaleRange)?;
        if scale == ExactRatio::ONE {
            return Ok(None);
        }
        let half = ExactRatio::new(1, 2).expect("constant positive denominator");
        FramingPose::new(half, half, scale)
            .and_then(|pose| pose.quantized())
            .map(Some)
            .map_err(|_| EscalationError::ScaleRange)
    }
}

/// Round a zoom step to the nearest point of the framing grid, ties away
/// from zero, so decimal input such as 0.08 has one exact stored value.
pub fn quantize_zoom_step(value: ExactRatio) -> Result<ExactRatio, EscalationError> {
    let scale = i128::from(FRAMING_NUMERIC_SCALE);
    let scaled = value
        .numerator()
        .checked_mul(scale)
        .ok_or(EscalationError::ScaleRange)?;
    let denominator = value.denominator();
    let half = denominator / 2;
    let rounded = if scaled >= 0 {
        (scaled + half) / denominator
    } else {
        -((-scaled + half) / denominator)
    };
    ExactRatio::new(rounded, scale).map_err(|_| EscalationError::ScaleRange)
}

/// `base^exponent` by squaring, so a late play costs O(log k).
fn power(base: ExactRatio, exponent: u32) -> Result<ExactRatio, crate::TimeError> {
    let (mut result, mut base, mut exponent) = (ExactRatio::ONE, base, exponent);
    while exponent > 0 {
        if exponent & 1 == 1 {
            result = result.checked_mul(base)?;
        }
        exponent >>= 1;
        if exponent > 0 {
            base = base.checked_mul(base)?;
        }
    }
    Ok(result)
}

fn on_grid(value: ExactRatio) -> bool {
    i128::from(FRAMING_NUMERIC_SCALE) % value.denominator() == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
        ExactRatio::new(numerator, denominator).unwrap()
    }

    fn escalation(gain: i32, step: Option<(ExactRatio, ZoomProgression)>) -> RepeatEscalation {
        RepeatEscalation {
            gain_step: GainDb::new(gain).unwrap(),
            zoom: step.map(|(step, progression)| ZoomStep { step, progression }),
        }
    }

    #[test]
    fn plays_add_gain_and_scale_and_the_first_play_is_unchanged() {
        // :repeat 3 gain-step=3dB zoom-step=0.08
        let escalation = escalation(3_000, Some((ratio(8, 100), ZoomProgression::Add)));
        assert_eq!(escalation.gain_millidecibels(0), 0);
        assert_eq!(escalation.gain_millidecibels(2), 6_000);
        assert_eq!(escalation.pose(0).unwrap(), None);
        let third = escalation.pose(2).unwrap().unwrap();
        // 1.16 on the 2^-32 grid.
        assert_eq!(
            third.scale,
            FramingPose::new(ratio(1, 2), ratio(1, 2), ratio(116, 100))
                .unwrap()
                .quantized()
                .unwrap()
                .scale
        );
        assert_eq!(third.center_x, ratio(1, 2));
    }

    #[test]
    fn multiplying_scale_compounds() {
        let escalation = escalation(0, Some((ratio(3, 2), ZoomProgression::Multiply)));
        assert_eq!(escalation.pose(2).unwrap().unwrap().scale, ratio(9, 4));
        assert!(escalation.validate(10).is_ok(), "1.5^9 ≈ 38");
        assert_eq!(
            escalation.validate(12),
            Err(EscalationError::ScaleRange),
            "1.5^11 ≈ 86"
        );
    }

    #[test]
    fn every_play_must_stay_in_range() {
        let loud = escalation(6_000, None);
        assert!(loud.validate(5).is_ok(), "+24 dB on the fifth play");
        assert_eq!(loud.validate(6), Err(EscalationError::GainRange));
        let shrinking = escalation(0, Some((ratio(-1, 4), ZoomProgression::Add)));
        assert!(shrinking.validate(3).is_ok(), "0.5 on the third play");
        assert_eq!(shrinking.validate(5), Err(EscalationError::ScaleRange));
        assert_eq!(escalation(0, None).validate(3), Err(EscalationError::Empty));
        assert_eq!(
            escalation(0, Some((ratio(1, 3), ZoomProgression::Add))).validate(2),
            Err(EscalationError::ZoomGrid)
        );
        assert_eq!(
            escalation(0, Some((ratio(-1, 2), ZoomProgression::Multiply))).validate(2),
            Err(EscalationError::ZoomFactor)
        );
        for unchanged in [
            (ExactRatio::ZERO, ZoomProgression::Add),
            (ExactRatio::ONE, ZoomProgression::Multiply),
        ] {
            assert_eq!(
                escalation(3_000, Some(unchanged)).validate(2),
                Err(EscalationError::ZoomUnchanged)
            );
        }
        // Squaring keeps a huge play count cheap and still refuses it.
        let doubling = escalation(0, Some((ratio(2, 1), ZoomProgression::Multiply)));
        assert_eq!(
            doubling.validate(u32::MAX),
            Err(EscalationError::ScaleRange)
        );
        assert_eq!(doubling.pose(6).unwrap().unwrap().scale, ratio(64, 1));
    }

    #[test]
    fn decimal_steps_round_once_to_the_grid() {
        let step = quantize_zoom_step(ratio(8, 100)).unwrap();
        assert!(on_grid(step));
        assert_eq!(step, ratio(343_597_384, 4_294_967_296));
        assert_eq!(quantize_zoom_step(ratio(1, 4)).unwrap(), ratio(1, 4));
        assert_eq!(
            quantize_zoom_step(ratio(-8, 100)).unwrap(),
            ratio(-343_597_384, 4_294_967_296)
        );
    }

    #[test]
    fn the_wire_omits_defaults() {
        let escalation = escalation(3_000, Some((ratio(1, 4), ZoomProgression::Add)));
        let json = serde_json::to_string(&escalation).unwrap();
        assert!(!json.contains("progression"), "{json}");
        assert_eq!(
            serde_json::from_str::<RepeatEscalation>(&json).unwrap(),
            escalation
        );
        assert!(serde_json::from_str::<RepeatEscalation>(r#"{"gain_step":0,"extra":1}"#).is_err());
    }
}
