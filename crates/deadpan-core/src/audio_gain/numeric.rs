//! Fixed-width arithmetic for exact owner-time comparisons and Q32 values.
//! Positive time numerators/denominators use at most 127 bits. Computing
//! (local-start)/(end-start) directly takes three-factor products of at most
//! 381 bits. Six u64 limbs leave room for the division remainder's one-bit
//! shift. No unbounded arithmetic or approximate clock comparison is used.

use std::cmp::Ordering;

use super::{GAIN_NUMERIC_SCALE, GainCurve, GainDb, GainError};
use crate::ExactRatio;

/// Continued fractions avoid cross-product overflow, including signed inputs.
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

pub(super) fn millidecibels(grid: i64) -> Result<ExactRatio, GainError> {
    Ok(ExactRatio::new(
        i128::from(grid),
        i128::from(GAIN_NUMERIC_SCALE),
    )?)
}

pub(super) fn progress(
    local: ExactRatio,
    start: ExactRatio,
    end: ExactRatio,
) -> Result<u64, GainError> {
    // local=n/d, start=a/A, end=b/B:
    // (local-start)/(end-start) = ((n*A-a*d)*B)/((b*A-a*B)*d).
    let positive = |v| u128::try_from(v).map_err(|_| GainError::Overflow);
    let n = positive(local.numerator())?;
    let d = local.denominator() as u128;
    let a = positive(start.numerator())?;
    let a_den = start.denominator() as u128;
    let b = positive(end.numerator())?;
    let b_den = end.denominator() as u128;
    let numerator = Wide::from(n)
        .mul_u128(a_den)?
        .sub(Wide::from(a).mul_u128(d)?)?
        .mul_u128(b_den)?;
    let denominator = Wide::from(b)
        .mul_u128(a_den)?
        .sub(Wide::from(a).mul_u128(b_den)?)?
        .mul_u128(d)?;
    fraction(numerator, denominator)
}

fn fraction(mut numerator: Wide, denominator: Wide) -> Result<u64, GainError> {
    if denominator == Wide::ZERO || numerator > denominator {
        return Err(GainError::Overflow);
    }
    if numerator == denominator {
        return Ok(GAIN_NUMERIC_SCALE);
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

fn lerp(a: i64, b: i64, t: u64) -> Result<i64, GainError> {
    if t > GAIN_NUMERIC_SCALE {
        return Err(GainError::Overflow);
    }
    let q = i128::from(GAIN_NUMERIC_SCALE);
    // Each input is a bounded authored millidecibel value on Q32; convex
    // interpolation retains that bound, including all de Casteljau steps.
    let value = i128::from(a) * (q - i128::from(t)) + i128::from(b) * i128::from(t);
    i64::try_from(ExactRatio::new(value, q)?.round_even()?).map_err(|_| GainError::Overflow)
}

pub(super) fn interpolate(
    from: GainDb,
    to: GainDb,
    curve: GainCurve,
    t: u64,
) -> Result<i64, GainError> {
    let from = from.grid();
    let to = to.grid();
    match curve {
        GainCurve::Step => Ok(from),
        GainCurve::Linear => lerp(from, to, t),
        GainCurve::Smoothstep => {
            let q = i128::from(GAIN_NUMERIC_SCALE);
            let t = i128::from(t);
            let shaped = ExactRatio::new(t * t * (3 * q - 2 * t), q * q)?.round_even()?;
            lerp(
                from,
                to,
                u64::try_from(shaped).map_err(|_| GainError::Overflow)?,
            )
        }
        GainCurve::Cubic { control1, control2 } => {
            let a = lerp(from, control1.grid(), t)?;
            let b = lerp(control1.grid(), control2.grid(), t)?;
            let c = lerp(control2.grid(), to, t)?;
            lerp(lerp(a, b, t)?, lerp(b, c, t)?, t)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Wide([u64; 6]);
impl Wide {
    const ZERO: Self = Self([0; 6]);
    fn mul(self, value: u64) -> Result<Self, GainError> {
        let mut output = [0; 6];
        let mut carry = 0u128;
        for (slot, limb) in output.iter_mut().zip(self.0) {
            let product = u128::from(limb) * u128::from(value) + carry;
            *slot = product as u64;
            carry = product >> 64;
        }
        if carry != 0 {
            return Err(GainError::Overflow);
        }
        Ok(Self(output))
    }
    fn mul_u128(self, value: u128) -> Result<Self, GainError> {
        let low = self.mul(value as u64)?;
        let high = self.mul((value >> 64) as u64)?;
        if high.0[5] != 0 {
            return Err(GainError::Overflow);
        }
        let mut output = low.0;
        let mut carry = false;
        for (slot, addend) in output[1..].iter_mut().zip(high.0) {
            let (first, c1) = slot.overflowing_add(addend);
            let (second, c2) = first.overflowing_add(u64::from(carry));
            *slot = second;
            carry = c1 || c2;
        }
        if carry {
            return Err(GainError::Overflow);
        }
        Ok(Self(output))
    }
    fn sub(self, other: Self) -> Result<Self, GainError> {
        let mut output = [0; 6];
        let mut borrow = false;
        for (index, slot) in output.iter_mut().enumerate() {
            let (first, b1) = self.0[index].overflowing_sub(other.0[index]);
            let (second, b2) = first.overflowing_sub(u64::from(borrow));
            *slot = second;
            borrow = b1 || b2;
        }
        if borrow {
            return Err(GainError::Overflow);
        }
        Ok(Self(output))
    }
}
impl From<u128> for Wide {
    fn from(value: u128) -> Self {
        Self([value as u64, (value >> 64) as u64, 0, 0, 0, 0])
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
    fn six_limb_carries_borrows_and_rejection_are_checked() {
        assert_eq!(
            Wide::from(u128::MAX).mul_u128(u128::MAX).unwrap().0,
            [1, 0, u64::MAX - 1, u64::MAX, 0, 0]
        );
        assert_eq!(
            Wide([0, 0, 1, 0, 0, 0]).sub(Wide::from(1)).unwrap(),
            Wide::from(u128::MAX)
        );
        assert!(Wide::ZERO.sub(Wide::from(1)).is_err());
        assert!(Wide([u64::MAX; 6]).mul(2).is_err());
        assert!(Wide([0, 0, 0, 0, 0, 1]).mul_u128(1 << 64).is_err());
    }

    #[test]
    fn fraction_has_independent_integer_round_even_oracle() {
        let q = u128::from(GAIN_NUMERIC_SCALE);
        for d in 1..180u128 {
            for n in 0..=d {
                let whole = n * q / d;
                let remainder = n * q % d;
                let expected = whole
                    + u128::from(
                        remainder > d - remainder
                            || (remainder == d - remainder && !whole.is_multiple_of(2)),
                    );
                assert_eq!(
                    u128::from(fraction(Wide::from(n), Wide::from(d)).unwrap()),
                    expected
                );
            }
        }
        for (n, expected) in [(1, 0), (3, 2), (5, 2), (7, 4)] {
            assert_eq!(
                fraction(Wide::from(n), Wide::from(2 * q)).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn full_width_owner_coordinates_need_more_than_five_limbs() {
        let p = 1_i128 << 124;
        let start = ExactRatio::new(p, 4 * p + 3).unwrap();
        let local = ExactRatio::new(2 * p + 1, 4 * p + 7).unwrap();
        let end = ExactRatio::new(3 * p + 1, 4 * p + 11).unwrap();
        let numerator = Wide::from(local.numerator() as u128)
            .mul_u128(start.denominator() as u128)
            .unwrap()
            .sub(
                Wide::from(start.numerator() as u128)
                    .mul_u128(local.denominator() as u128)
                    .unwrap(),
            )
            .unwrap()
            .mul_u128(end.denominator() as u128)
            .unwrap();
        assert_ne!(
            numerator.0[5], 0,
            "the sixth limb must actually be exercised"
        );
        assert!(local.checked_sub(start).is_err());
        // These are within 2^-120 of 1/4, 1/2 and 3/4 respectively.
        assert_eq!(progress(local, start, end).unwrap(), GAIN_NUMERIC_SCALE / 2);
    }
}
