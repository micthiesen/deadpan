//! Fixed bounded arithmetic for one framing interpolation fraction. This is not
//! an arbitrary precision API. Admitted progress denominators are <= 10^6;
//! local numerator/denominator are <= 127 bits and duration <= 63 bits.
//! The largest product below uses at most 231 bits, leaving room for the one-bit
//! shifts used by Q32 division. Every carry is still checked.

use std::cmp::Ordering;

use crate::ExactRatio;

use super::{FRAMING_NUMERIC_SCALE, FramingError};

/// Continued fractions compare exact signed ratios without cross products.
pub(super) fn compare(a: ExactRatio, b: ExactRatio) -> Ordering {
    let order = a.floor().cmp(&b.floor());
    if order != Ordering::Equal {
        return order;
    }
    let mut a = (
        a.numerator().rem_euclid(a.denominator()) as u128,
        a.denominator() as u128,
    );
    let mut b = (
        b.numerator().rem_euclid(b.denominator()) as u128,
        b.denominator() as u128,
    );
    let mut reverse = false;
    loop {
        let order = (a.0 / a.1).cmp(&(b.0 / b.1));
        if order != Ordering::Equal {
            return if reverse { order.reverse() } else { order };
        }
        let ar = a.0 % a.1;
        let br = b.0 % b.1;
        if ar == 0 || br == 0 {
            let order = ar.cmp(&br);
            return if reverse { order.reverse() } else { order };
        }
        a = (a.1, ar);
        b = (b.1, br);
        reverse = !reverse;
    }
}

pub(super) fn quantize(value: ExactRatio) -> Result<i64, FramingError> {
    let whole = value.floor();
    let remainder = Wide::from(value.numerator().rem_euclid(value.denominator()) as u128);
    let fraction = fraction(remainder, Wide::from(value.denominator() as u128))?;
    let result = whole
        .checked_mul(i128::from(FRAMING_NUMERIC_SCALE))
        .and_then(|whole| whole.checked_add(i128::from(fraction)))
        .ok_or(FramingError::Overflow)?;
    i64::try_from(result).map_err(|_| FramingError::Overflow)
}

pub(super) fn lerp(a: i64, b: i64, t: u64) -> Result<i64, FramingError> {
    if t > FRAMING_NUMERIC_SCALE {
        return Err(FramingError::Overflow);
    }
    let q = i128::from(FRAMING_NUMERIC_SCALE);
    let numerator = i128::from(a) * (q - i128::from(t)) + i128::from(b) * i128::from(t);
    i64::try_from(ExactRatio::new(numerator, q)?.round_even()?).map_err(|_| FramingError::Overflow)
}

pub(super) fn segment_progress(
    local: ExactRatio,
    frames: i64,
    start: ExactRatio,
    end: ExactRatio,
) -> Result<u64, FramingError> {
    // (local / D - a/A) / (b/B - a/A)
    // = ((n*A - D*d*a)*B) / (D*d*(b*A-a*B)).
    let to_u64 = |v: i128| u64::try_from(v).map_err(|_| FramingError::Overflow);
    let a = to_u64(start.numerator())?;
    let a_den = to_u64(start.denominator())?;
    let b = to_u64(end.numerator())?;
    let b_den = to_u64(end.denominator())?;
    let duration = u64::try_from(frames).map_err(|_| FramingError::Overflow)?;
    let n = u128::try_from(local.numerator()).map_err(|_| FramingError::Overflow)?;
    let d = local.denominator() as u128;
    let delta = b
        .checked_mul(a_den)
        .and_then(|left| left.checked_sub(a.checked_mul(b_den)?))
        .ok_or(FramingError::Overflow)?;
    let dd = Wide::from(d).mul(duration)?;
    let numerator = Wide::from(n).mul(a_den)?.sub(dd.mul(a)?)?.mul(b_den)?;
    fraction(numerator, dd.mul(delta)?)
}

fn fraction(mut numerator: Wide, denominator: Wide) -> Result<u64, FramingError> {
    if denominator == Wide::ZERO || numerator > denominator {
        return Err(FramingError::Overflow);
    }
    if numerator == denominator {
        return Ok(FRAMING_NUMERIC_SCALE);
    }
    let mut result = 0u64;
    for _ in 0..32 {
        numerator = numerator.mul(2)?;
        result *= 2;
        if numerator >= denominator {
            numerator = numerator.sub(denominator)?;
            result += 1;
        }
    }
    let complement = denominator.sub(numerator)?;
    if numerator > complement || (numerator == complement && !result.is_multiple_of(2)) {
        result += 1;
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Wide([u64; 4]);

impl Wide {
    const ZERO: Self = Self([0; 4]);

    fn mul(self, value: u64) -> Result<Self, FramingError> {
        let mut output = [0; 4];
        let mut carry = 0u128;
        for (slot, limb) in output.iter_mut().zip(self.0) {
            let product = u128::from(limb) * u128::from(value) + carry;
            *slot = product as u64;
            carry = product >> 64;
        }
        if carry != 0 {
            return Err(FramingError::Overflow);
        }
        Ok(Self(output))
    }

    fn sub(self, other: Self) -> Result<Self, FramingError> {
        let mut output = [0; 4];
        let mut borrow = false;
        for (index, slot) in output.iter_mut().enumerate() {
            let (first, b1) = self.0[index].overflowing_sub(other.0[index]);
            let (second, b2) = first.overflowing_sub(u64::from(borrow));
            *slot = second;
            borrow = b1 || b2;
        }
        if borrow {
            return Err(FramingError::Overflow);
        }
        Ok(Self(output))
    }
}

impl From<u128> for Wide {
    fn from(value: u128) -> Self {
        Self([value as u64, (value >> 64) as u64, 0, 0])
    }
}
impl Ord for Wide {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.iter().rev().cmp(other.0.iter().rev())
    }
}
impl PartialOrd for Wide {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_carries_borrows_and_failures() {
        assert_eq!(
            Wide::from(u128::MAX).mul(u64::MAX).unwrap().0,
            [1, u64::MAX, u64::MAX - 1, 0]
        );
        assert_eq!(
            Wide([0, 0, 1, 0]).sub(Wide::from(1)).unwrap(),
            Wide::from(u128::MAX)
        );
        assert!(Wide::ZERO.sub(Wide::from(1)).is_err());
        assert!(Wide([u64::MAX; 4]).mul(2).is_err());
    }

    #[test]
    fn fraction_matches_independent_small_integer_division() {
        let q = u128::from(FRAMING_NUMERIC_SCALE);
        for d in 1..300u128 {
            for n in 0..=d {
                let quotient = n * q / d;
                let remainder = n * q % d;
                let expected = quotient
                    + u128::from(
                        remainder > d - remainder
                            || (remainder == d - remainder && !quotient.is_multiple_of(2)),
                    );
                assert_eq!(
                    u128::from(fraction(Wide::from(n), Wide::from(d)).unwrap()),
                    expected
                );
            }
        }
    }

    #[test]
    fn comparison_handles_signed_extremes_without_products() {
        let high = ExactRatio::new(i128::MAX - 1, i128::MAX).unwrap();
        let low = ExactRatio::new(i128::MAX - 2, i128::MAX - 1).unwrap();
        assert_eq!(compare(high, low), Ordering::Greater);
        assert_eq!(
            compare(
                ExactRatio::new(-1, 2).unwrap(),
                ExactRatio::new(-2, 3).unwrap()
            ),
            Ordering::Greater
        );
        assert_eq!(compare(high, ExactRatio::ONE), Ordering::Less);
    }
}
