//! A continuous speed ramp: one Retime whose rate changes linearly across its
//! output (specification §10.5 "variable rate"). The ramp stores only the
//! rate's shape, reduced integer weights `A:B = from:to`; the Retime's
//! integral input range and output duration fix the mean rate, so the rate at
//! output fraction `u` is
//!
//! `r(u) = (L/D) · 2(A + (B − A)u) / (A + B)`
//!
//! for input length `L` and output duration `D`. The input position is its
//! exact integral,
//!
//! `x(u) = L · (2A·u + (B − A)·u²) / (A + B)`,
//!
//! a quadratic in rationals with `x(0) = 0` and `x(1) = L`. No coordinate is
//! accumulated or rounded; picture sampling evaluates it at each output frame
//! center and audio at each output sample. Because the duration is rounded
//! once to whole frames, the realised endpoint speeds are the authored ones
//! scaled by the same factor (see [`SpeedRamp::endpoint_rates`]). Two small
//! integers keep `PitchPolicy` and every structure that embeds it compact.

use serde::{Deserialize, Serialize};

use crate::{DocumentError, DocumentErrorCode, ExactRatio, TimeError};

/// The largest reduced weight: ratios such as 1.333:0.25 fit, and the exact
/// quadratic stays inside `i128` for any admitted length and sample grid.
pub const MAX_RAMP_WEIGHT: u16 = u16::MAX;

/// Realised endpoint speeds stay within the tape resampler's 1/16..16 range.
pub const MAX_RAMP_SPEED: i64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeedRamp {
    /// Relative rate at the first output instant.
    start: u16,
    /// Relative rate at the last output instant.
    end: u16,
}

impl SpeedRamp {
    /// A ramp from `from` to `to`, two different positive speeds whose ratio
    /// reduces to weights of at most [`MAX_RAMP_WEIGHT`].
    pub fn new(from: ExactRatio, to: ExactRatio) -> Result<Self, DocumentError> {
        let invalid =
            |message: &str| DocumentError::new(DocumentErrorCode::InvalidTree, message.to_owned());
        if !from.compare_integer(0).is_gt() || !to.compare_integer(0).is_gt() {
            return Err(invalid("ramp speeds must be positive"));
        }
        if from == to {
            return Err(invalid(
                "a ramp needs two different speeds; use :speed for one speed",
            ));
        }
        let ratio = from
            .checked_div(to)
            .map_err(|_| invalid("ramp speeds are too precise"))?;
        let weight = |value: i128| {
            // MAX_RAMP_WEIGHT is u16::MAX, so the conversion is the bound.
            u16::try_from(value).map_err(|_| invalid("ramp speeds are too precise"))
        };
        Ok(Self {
            start: weight(ratio.numerator())?,
            end: weight(ratio.denominator())?,
        })
    }

    /// Reduced, distinct, positive and bounded weights, as `new` makes them.
    pub fn validate(&self) -> Result<(), DocumentError> {
        let canonical = Self::new(
            ExactRatio::integer(i64::from(self.start)),
            ExactRatio::integer(i64::from(self.end)),
        )?;
        if canonical != *self {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidTree,
                "ramp weights must be reduced".to_owned(),
            ));
        }
        Ok(())
    }

    /// Reduced positive integer weights `A:B = from:to`.
    pub fn weights(&self) -> Result<(i128, i128), TimeError> {
        Ok((i128::from(self.start), i128::from(self.end)))
    }

    /// The exact input offset (in input frames from the mapping start) at
    /// output offset `t` of a Retime with output duration `output` and input
    /// length `input`.
    pub fn input_offset(
        &self,
        t: ExactRatio,
        output: ExactRatio,
        input: ExactRatio,
    ) -> Result<ExactRatio, TimeError> {
        let (a, b) = self.weights()?;
        let u = t.checked_div(output)?;
        let linear = u.checked_mul(int(2 * a)?)?;
        let quadratic = u.checked_mul(u)?.checked_mul(int(b - a)?)?;
        input
            .checked_mul(linear.checked_add(quadratic)?)?
            .checked_div(int(a + b)?)
    }

    /// The exact instantaneous rate (input frames per output frame) at output
    /// offset `t`.
    pub fn rate_at(
        &self,
        t: ExactRatio,
        output: ExactRatio,
        input: ExactRatio,
    ) -> Result<ExactRatio, TimeError> {
        let (a, b) = self.weights()?;
        let u = t.checked_div(output)?;
        let shape = int(a)?
            .checked_add(u.checked_mul(int(b - a)?)?)?
            .checked_mul(ExactRatio::integer(2))?
            .checked_div(int(a + b)?)?;
        input.checked_div(output)?.checked_mul(shape)
    }

    /// The realised speeds at the first and last output instant.
    pub fn endpoint_rates(
        &self,
        output: ExactRatio,
        input: ExactRatio,
    ) -> Result<(ExactRatio, ExactRatio), TimeError> {
        Ok((
            self.rate_at(ExactRatio::ZERO, output, input)?,
            self.rate_at(output, output, input)?,
        ))
    }

    /// Whether both realised endpoint speeds lie in 1/16..16.
    pub fn speeds_in_range(&self, output: ExactRatio, input: ExactRatio) -> bool {
        let Ok((first, last)) = self.endpoint_rates(output, input) else {
            return false;
        };
        [first, last].into_iter().all(|rate| {
            rate.compare_integer(MAX_RAMP_SPEED).is_le()
                && rate
                    .checked_mul(ExactRatio::integer(MAX_RAMP_SPEED))
                    .is_ok_and(|scaled| scaled.compare_integer(1).is_ge())
        })
    }
}

/// A Retime's exact child-local position at output offset `position`: the
/// affine map for an ordinary Retime, the quadratic for a ramp.
pub fn retime_child_position(
    pitch: crate::PitchPolicy,
    mapping: crate::FrameRange,
    duration: crate::FrameDuration,
    position: ExactRatio,
) -> Result<ExactRatio, TimeError> {
    let input = ExactRatio::integer(mapping.duration().frames());
    let output = ExactRatio::integer(duration.frames());
    let offset = match pitch.ramp() {
        Some(ramp) => ramp.input_offset(position, output, input)?,
        None => position.checked_mul(input)?.checked_div(output)?,
    };
    offset.checked_add(ExactRatio::integer(mapping.start().0))
}

fn int(value: i128) -> Result<ExactRatio, TimeError> {
    ExactRatio::new(value, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(n: i128, d: i128) -> ExactRatio {
        ExactRatio::new(n, d).unwrap()
    }

    #[test]
    fn endpoints_are_exact_and_the_mapping_is_strictly_increasing() {
        let ramp = SpeedRamp::new(r(1, 2), r(2, 1)).unwrap();
        let (input, output) = (r(90, 1), r(72, 1));
        assert_eq!(
            ramp.input_offset(ExactRatio::ZERO, output, input).unwrap(),
            ExactRatio::ZERO
        );
        assert_eq!(ramp.input_offset(output, output, input).unwrap(), input);
        let mut previous = ExactRatio::ZERO;
        for frame in 0..72 {
            let center = r(2 * frame + 1, 2);
            let x = ramp.input_offset(center, output, input).unwrap();
            assert!(x.compare(previous).is_gt());
            previous = x;
        }
        // The derivative of the exact integral is the rate; its mean over the
        // output is L/D and its endpoints keep the authored 1:4 shape.
        let (first, last) = ramp.endpoint_rates(output, input).unwrap();
        assert_eq!(last.checked_div(first).unwrap(), r(4, 1));
        assert_eq!(
            first.checked_add(last).unwrap(),
            input
                .checked_mul(r(2, 1))
                .unwrap()
                .checked_div(output)
                .unwrap()
        );
    }

    #[test]
    fn a_ramp_needs_two_distinct_positive_bounded_speeds() {
        assert!(SpeedRamp::new(r(1, 1), r(1, 1)).is_err());
        assert!(SpeedRamp::new(r(0, 1), r(1, 1)).is_err());
        assert!(SpeedRamp::new(r(1, 1), r(1 << 17, 1)).is_err());
        // 3/2 to 1/4 reduces to the shape 6:1; 2 to 6 is the same shape as 1 to 3.
        assert_eq!(
            SpeedRamp::new(r(3, 2), r(1, 4)).unwrap().weights().unwrap(),
            (6, 1)
        );
        assert_eq!(
            SpeedRamp::new(r(2, 1), r(6, 1)).unwrap(),
            SpeedRamp::new(r(1, 1), r(3, 1)).unwrap()
        );
        let wire = serde_json::to_string(&SpeedRamp::new(r(1, 1), r(3, 1)).unwrap()).unwrap();
        assert_eq!(wire, r#"{"start":1,"end":3}"#);
        let unreduced: SpeedRamp = serde_json::from_str(r#"{"start":2,"end":6}"#).unwrap();
        assert!(unreduced.validate().is_err());
        let ramp = SpeedRamp::new(r(1, 4), r(1, 1)).unwrap();
        assert!(ramp.speeds_in_range(r(10, 1), r(10, 1)));
        assert!(!ramp.speeds_in_range(r(10, 1), r(1_000, 1)));
    }
}
