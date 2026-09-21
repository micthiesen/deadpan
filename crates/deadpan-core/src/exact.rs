//! Exact intermediate coordinates for nested retiming and source sampling.
//! Authored durations remain integer project frames; no rounded intermediate
//! frame is introduced while mapping an output frame center through a plan.

use serde::{Deserialize, Serialize};

use crate::TimeError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "RatioWire", into = "RatioWire")]
pub struct ExactRatio {
    numerator: i128,
    denominator: i128,
}

// Decimal strings keep the JSON contract exact beyond JavaScript/u64 integers.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RatioWire {
    numerator: String,
    denominator: String,
}

impl TryFrom<RatioWire> for ExactRatio {
    type Error = TimeError;
    fn try_from(value: RatioWire) -> Result<Self, Self::Error> {
        Self::new(
            value.numerator.parse().map_err(|_| TimeError::Overflow)?,
            value.denominator.parse().map_err(|_| TimeError::Overflow)?,
        )
    }
}
impl From<ExactRatio> for RatioWire {
    fn from(value: ExactRatio) -> Self {
        Self {
            numerator: value.numerator.to_string(),
            denominator: value.denominator.to_string(),
        }
    }
}

impl ExactRatio {
    pub const ZERO: Self = Self {
        numerator: 0,
        denominator: 1,
    };
    pub const ONE: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    pub fn new(numerator: i128, denominator: i128) -> Result<Self, TimeError> {
        if denominator <= 0 {
            return Err(TimeError::InvalidRatio);
        }
        let divisor = gcd(numerator.unsigned_abs(), denominator as u128) as i128;
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }
    pub const fn integer(value: i64) -> Self {
        Self {
            numerator: value as i128,
            denominator: 1,
        }
    }
    pub const fn numerator(self) -> i128 {
        self.numerator
    }
    pub const fn denominator(self) -> i128 {
        self.denominator
    }
    pub fn floor(self) -> i128 {
        self.numerator.div_euclid(self.denominator)
    }
    pub fn ceil(self) -> Result<i128, TimeError> {
        self.floor()
            .checked_add(i128::from(self.numerator.rem_euclid(self.denominator) != 0))
            .ok_or(TimeError::Overflow)
    }
    pub fn round_even(self) -> Result<i128, TimeError> {
        let floor = self.floor();
        let remainder = self.numerator.rem_euclid(self.denominator);
        let round_up = remainder > self.denominator - remainder
            || (remainder == self.denominator - remainder && floor % 2 != 0);
        floor
            .checked_add(i128::from(round_up))
            .ok_or(TimeError::Overflow)
    }
    pub fn checked_add(self, other: Self) -> Result<Self, TimeError> {
        let divisor = gcd(self.denominator as u128, other.denominator as u128) as i128;
        let left_scale = other.denominator / divisor;
        let right_scale = self.denominator / divisor;
        let numerator = self
            .numerator
            .checked_mul(left_scale)
            .and_then(|left| {
                other
                    .numerator
                    .checked_mul(right_scale)
                    .and_then(|right| left.checked_add(right))
            })
            .ok_or(TimeError::Overflow)?;
        Self::new(
            numerator,
            self.denominator
                .checked_mul(left_scale)
                .ok_or(TimeError::Overflow)?,
        )
    }
    pub fn checked_sub(self, other: Self) -> Result<Self, TimeError> {
        self.checked_add(Self {
            numerator: other.numerator.checked_neg().ok_or(TimeError::Overflow)?,
            ..other
        })
    }
    pub fn checked_mul(self, other: Self) -> Result<Self, TimeError> {
        let left_cancel = gcd(self.numerator.unsigned_abs(), other.denominator as u128) as i128;
        let right_cancel = gcd(other.numerator.unsigned_abs(), self.denominator as u128) as i128;
        Self::new(
            (self.numerator / left_cancel)
                .checked_mul(other.numerator / right_cancel)
                .ok_or(TimeError::Overflow)?,
            (self.denominator / right_cancel)
                .checked_mul(other.denominator / left_cancel)
                .ok_or(TimeError::Overflow)?,
        )
    }
    pub fn checked_div(self, other: Self) -> Result<Self, TimeError> {
        let magnitude =
            i128::try_from(other.numerator.unsigned_abs()).map_err(|_| TimeError::Overflow)?;
        if magnitude == 0 {
            return Err(TimeError::InvalidRatio);
        }
        self.checked_mul(Self {
            numerator: if other.numerator < 0 {
                -other.denominator
            } else {
                other.denominator
            },
            denominator: magnitude,
        })
    }
    /// Compare with an integer without multiplying potentially large coordinates.
    pub fn compare_integer(self, value: i64) -> std::cmp::Ordering {
        self.floor().cmp(&i128::from(value)).then_with(|| {
            if self.numerator.rem_euclid(self.denominator) == 0 {
                std::cmp::Ordering::Equal
            } else {
                std::cmp::Ordering::Greater
            }
        })
    }
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_mapping_and_negative_boundaries_do_not_round_intermediate_frames() {
        let center = ExactRatio::new(3, 2).unwrap();
        let child = center.checked_mul(ExactRatio::new(7, 15).unwrap()).unwrap();
        let source = child
            .checked_mul(ExactRatio::new(1001, 3).unwrap())
            .unwrap();
        assert_eq!(source, ExactRatio::new(7007, 30).unwrap());
        for (numerator, rounded) in [
            (-7, -4),
            (-5, -2),
            (-3, -2),
            (-1, 0),
            (1, 0),
            (3, 2),
            (5, 2),
            (7, 4),
        ] {
            assert_eq!(
                ExactRatio::new(numerator, 2).unwrap().round_even().unwrap(),
                rounded
            );
        }
        let negative = ExactRatio::new(-1, 3).unwrap();
        assert_eq!(negative.floor(), -1);
        assert_eq!(negative.ceil().unwrap(), 0);
        assert!(negative.compare_integer(0).is_lt());
        assert!(negative.compare_integer(-1).is_gt());
    }

    #[test]
    fn cross_cancellation_and_wire_format_preserve_wide_values() {
        let huge = ExactRatio::new(i128::MAX, 2).unwrap();
        assert_eq!(
            huge.checked_mul(ExactRatio::new(2, i128::MAX).unwrap())
                .unwrap(),
            ExactRatio::ONE
        );
        let json = serde_json::to_string(&huge).unwrap();
        assert_eq!(serde_json::from_str::<ExactRatio>(&json).unwrap(), huge);
        assert!(
            huge.checked_add(huge).is_err(),
            "wide intermediate overflow is rejected"
        );
        assert!(
            ExactRatio::new(i128::MAX, 1)
                .unwrap()
                .checked_add(ExactRatio::ONE)
                .is_err()
        );
        assert!(ExactRatio::ONE.checked_div(ExactRatio::ZERO).is_err());
        assert!(ExactRatio::new(1, 0).is_err());
    }
}
