//! The canonical schedule of a pitch-preserving speed ramp.
//!
//! A ramp's input position at output sample `n` is the exact rational
//! `q(n) = (linear·n + quadratic·n²) / denominator` on `0 ≤ n ≤ N` (the
//! stage's output count), extended linearly with its endpoint slopes outside
//! so priming before the start and flushing after the end keep a positive
//! rate. The native renderer keeps the fixed 256-sample output quantum of
//! the constant schedule; each call consumes exactly the input between two
//! rounded boundaries `round_even(q(k·256 + latency))`. The cumulative input
//! position therefore equals the exact mapping (to the nearest sample) at
//! every quantum boundary, independently of how callers size their reads.
//! Inside one call the stretcher holds the call's mean rate, so its time
//! position follows the chord of `q` instead of the curve; see
//! [`RampSchedule::linearisation_bound`].

use crate::{DspError, QUANTUM, StretchRate};

/// Identity of the ramp schedule and its pinned native implementation.
pub const RAMP_ENGINE_ID: &str = "deadpan-canonical-stretch-ramp-1";

/// Exact output samples beyond each end that the schedule must evaluate
/// without overflow: priming at the slowest admitted rate stays below
/// 65,536 output samples, and flushing below one quantum plus latency.
const MARGIN: i128 = 1 << 17;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RampSchedule {
    linear: i128,
    quadratic: i128,
    denominator: i128,
    output_frames: u32,
}

impl RampSchedule {
    /// The reduced schedule of an output of `output_frames` samples. The
    /// starting slope `linear/denominator` must lie in 1/8..8. The slope at
    /// `output_frames` may overshoot the authored end speed by the fraction
    /// of a sample past the exact end, so it is admitted in 1/9..9.
    pub fn new(
        linear: i128,
        quadratic: i128,
        denominator: i128,
        output_frames: u32,
    ) -> Result<Self, DspError> {
        if output_frames == 0 {
            return Err(DspError::OutputLength);
        }
        if denominator <= 0 {
            return Err(DspError::Rate);
        }
        let divisor = gcd(gcd(linear, quadratic), denominator);
        let schedule = Self {
            linear: linear / divisor,
            quadratic: quadratic / divisor,
            denominator: denominator / divisor,
            output_frames,
        };
        let first = schedule.slope_numerator(0)?;
        let last = schedule.slope_numerator(i128::from(output_frames))?;
        let within = |slope: i128, bound: i128| {
            slope
                .checked_mul(bound)
                .is_some_and(|scaled| scaled >= schedule.denominator)
                && schedule
                    .denominator
                    .checked_mul(bound)
                    .is_some_and(|scaled| slope <= scaled)
        };
        if !within(first, 8) || !within(last, 9) {
            return Err(DspError::Rate);
        }
        // Prove every position the renderer can reach evaluates exactly.
        schedule.numerator(-MARGIN)?;
        schedule.numerator(i128::from(output_frames) + MARGIN)?;
        Ok(schedule)
    }

    pub fn output_frames(self) -> u32 {
        self.output_frames
    }

    pub fn coefficients(self) -> (i128, i128, i128) {
        (self.linear, self.quadratic, self.denominator)
    }

    /// The exact starting rate, which also sets the stretcher's priming
    /// latency, reduced.
    pub fn starting_rate(self) -> Result<StretchRate, DspError> {
        StretchRate::new(
            u64::try_from(self.linear).map_err(|_| DspError::Rate)?,
            u64::try_from(self.denominator).map_err(|_| DspError::Rate)?,
        )
    }

    fn slope_numerator(&self, n: i128) -> Result<i128, DspError> {
        self.quadratic
            .checked_mul(2)
            .and_then(|q| q.checked_mul(n))
            .and_then(|q| q.checked_add(self.linear))
            .ok_or(DspError::Rate)
    }

    /// The exact numerator of `q(n)` over [`Self::coefficients`]'s
    /// denominator, including both linear extensions.
    pub fn numerator(&self, n: i128) -> Result<i128, DspError> {
        let end = i128::from(self.output_frames);
        let quadratic = |n: i128| {
            self.linear
                .checked_mul(n)?
                .checked_add(self.quadratic.checked_mul(n)?.checked_mul(n)?)
        };
        if n <= 0 {
            self.linear.checked_mul(n)
        } else if n <= end {
            quadratic(n)
        } else {
            let slope = self.slope_numerator(end).ok();
            quadratic(end).and_then(|at_end| at_end.checked_add(slope?.checked_mul(n - end)?))
        }
        .ok_or(DspError::Rate)
    }

    /// `q(n)` rounded half-even: the native renderer's input boundary for
    /// output sample `n` before its analysis latency is added.
    pub fn boundary(&self, n: i64) -> Result<i64, DspError> {
        let numerator = self.numerator(i128::from(n))?;
        let mut whole = numerator.div_euclid(self.denominator);
        let remainder = numerator.rem_euclid(self.denominator);
        let rest = self.denominator - remainder;
        if remainder > rest || (remainder == rest && whole % 2 != 0) {
            whole += 1;
        }
        i64::try_from(whole).map_err(|_| DspError::Rate)
    }

    /// Worst-case distance, in input samples, between the time position the
    /// stretcher follows inside one 256-sample call (the chord between two
    /// rounded boundaries) and the exact quadratic. The chord of a parabola
    /// over `h` samples departs from it by at most `|q''|·h²/8`, with
    /// `q'' = 2·quadratic/denominator`; rounding each boundary to a whole
    /// sample adds at most one half. Equivalently `|r₁ − r₀|·256²/(8N) + ½`
    /// for endpoint rates `r₀`, `r₁` over `N` output samples.
    pub fn linearisation_bound(&self) -> f64 {
        let quantum = QUANTUM as f64;
        self.quadratic.unsigned_abs() as f64 * quantum * quantum / (4.0 * self.denominator as f64)
            + 0.5
    }
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    // A zero quadratic with a positive denominator still divides by one.
    i128::try_from(a.max(1)).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quarter_to_double(output: u32) -> RampSchedule {
        // Rates 1/2 at n = 0 to 2 at n = output: q' = (2·output + 6n)/(4·output).
        RampSchedule::new(2 * i128::from(output), 3, 4 * i128::from(output), output).unwrap()
    }

    #[test]
    fn boundaries_round_the_exact_quadratic_and_extend_linearly() {
        let output = 48_000;
        let schedule = quarter_to_double(output);
        let (linear, quadratic, denominator) = schedule.coefficients();
        let mut previous = schedule.boundary(-70_000).unwrap();
        for n in (-70_000_i64..=i64::from(output) + 70_000).step_by(7) {
            let at = schedule.boundary(n).unwrap();
            assert!(at >= previous, "monotonic at {n}");
            previous = at;
            if (0..=i64::from(output)).contains(&n) {
                let exact = (linear as f64 * n as f64 + quadratic as f64 * (n * n) as f64)
                    / denominator as f64;
                assert!(
                    (at as f64 - exact).abs() <= 0.5 + 1e-9,
                    "{n}: {at} vs {exact}"
                );
            }
        }
        // q(N) = 5N/4 exactly: the mean rate is 5/4.
        assert_eq!(schedule.boundary(0).unwrap(), 0);
        assert_eq!(schedule.boundary(i64::from(output)).unwrap(), 60_000);
        // Ties round to even, symmetrically below zero.
        let half = RampSchedule::new(1, 0, 2, 16).unwrap();
        assert_eq!(
            (-3..=3)
                .map(|n| half.boundary(n).unwrap())
                .collect::<Vec<_>>(),
            [-2, -1, 0, 0, 0, 1, 2]
        );
    }

    #[test]
    fn admission_reduces_and_bounds_both_slopes() {
        let schedule = RampSchedule::new(10, 6, 20, 10).unwrap();
        assert_eq!(schedule.coefficients(), (5, 3, 10));
        assert_eq!(
            schedule.starting_rate().unwrap(),
            StretchRate::new(1, 2).unwrap()
        );
        // Starting slope 1/9 or ending slope above 9 refuse.
        assert_eq!(RampSchedule::new(1, 0, 9, 10), Err(DspError::Rate));
        assert_eq!(RampSchedule::new(8, 1, 1, 1), Err(DspError::Rate));
        assert_eq!(RampSchedule::new(1, 0, 0, 10), Err(DspError::Rate));
        assert_eq!(RampSchedule::new(1, 0, 1, 0), Err(DspError::OutputLength));
        // Coefficients whose reach overflows refuse before native work.
        assert_eq!(
            RampSchedule::new(i128::MAX / 4, 0, i128::MAX / 8, 10),
            Err(DspError::Rate)
        );
    }

    #[test]
    fn linearisation_bound_follows_the_rate_change_per_sample() {
        // 1/2x to 2x over one second: |r1 - r0|·256²/(8N) + 1/2.
        let schedule = quarter_to_double(48_000);
        let expected = 1.5 * 65_536.0 / (8.0 * 48_000.0) + 0.5;
        assert!((schedule.linearisation_bound() - expected).abs() < 1e-12);
        // The chord between exact quadratic values never departs further.
        let (linear, quadratic, denominator) = schedule.coefficients();
        let q = |n: f64| (linear as f64 * n + quadratic as f64 * n * n) / denominator as f64;
        let mut worst = 0.0_f64;
        for block in 0..(48_000 / 256) {
            let start = f64::from(block * 256);
            let (a, b) = (
                schedule.boundary(block as i64 * 256).unwrap() as f64,
                schedule.boundary(block as i64 * 256 + 256).unwrap() as f64,
            );
            for k in 0..=256 {
                let chord = a + (b - a) * f64::from(k) / 256.0;
                worst = worst.max((chord - q(start + f64::from(k))).abs());
            }
        }
        assert!(worst <= schedule.linearisation_bound(), "{worst}");
    }
}
