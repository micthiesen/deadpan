use std::{error::Error, fmt, ops::Range};

/// Internal audio mix samples per second, as specified in §4.1.
pub const MIX_SAMPLE_RATE: u32 = 48_000;

/// A signed boundary on the project's presentation-frame clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectFrame(pub i64);

/// A signed boundary on the internal 48 kHz audio clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AudioSample(pub i64);

/// A nonnegative number of project frames, distinct from a frame position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrameDuration(i64);

impl FrameDuration {
    pub const ZERO: Self = Self(0);

    /// Rejects negative durations. Zero is valid for an empty sequence or gap.
    pub fn new(frames: i64) -> Result<Self, TimeError> {
        if frames < 0 {
            return Err(TimeError::NegativeDuration);
        }
        Ok(Self(frames))
    }

    pub const fn frames(self) -> i64 {
        self.0
    }

    /// Adds durations without wrapping, for example when concatenating beats.
    pub fn checked_add(self, other: Self) -> Result<Self, TimeError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(TimeError::Overflow)
    }
}

/// An exact, positive, normalized presentation rate in frames per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameRate {
    numerator: u32,
    denominator: u32,
}

impl FrameRate {
    /// Rejects a zero numerator or denominator and reduces the ratio.
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, TimeError> {
        if numerator == 0 || denominator == 0 {
            return Err(TimeError::InvalidFrameRate);
        }
        let divisor = gcd(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    /// Maps a boundary from the common origin using ties-to-even rounding.
    ///
    /// `B(f) = round_even(f × 48000 × denominator / numerator)`.
    /// Negative frame positions use the same symmetric rounding rule.
    /// Returns an error if the result cannot be represented by `AudioSample`.
    pub fn audio_boundary(self, frame: ProjectFrame) -> Result<AudioSample, TimeError> {
        let dividend = i128::from(frame.0)
            .checked_mul(i128::from(MIX_SAMPLE_RATE))
            .and_then(|value| value.checked_mul(i128::from(self.denominator)))
            .ok_or(TimeError::Overflow)?;
        let sample = round_even(dividend, i128::from(self.numerator))?;
        i64::try_from(sample)
            .map(AudioSample)
            .map_err(|_| TimeError::Overflow)
    }
}

/// A validated half-open project interval `[start, end)`.
///
/// Empty intervals are allowed. Its duration must fit in `FrameDuration`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameRange {
    start: ProjectFrame,
    end: ProjectFrame,
}

impl FrameRange {
    pub fn new(start: ProjectFrame, end: ProjectFrame) -> Result<Self, TimeError> {
        if end < start {
            return Err(TimeError::ReversedRange);
        }
        end.0.checked_sub(start.0).ok_or(TimeError::Overflow)?;
        Ok(Self { start, end })
    }

    pub const fn start(self) -> ProjectFrame {
        self.start
    }

    pub const fn end(self) -> ProjectFrame {
        self.end
    }

    pub const fn duration(self) -> FrameDuration {
        // Construction proves the subtraction fits and is nonnegative.
        FrameDuration(self.end.0 - self.start.0)
    }

    pub fn contains(self, frame: ProjectFrame) -> bool {
        self.start <= frame && frame < self.end
    }

    /// Maps each boundary independently from the common project origin.
    ///
    /// The result is `[B(start), B(end))`. Never map `duration()` and add it
    /// to the start sample, which accumulates rounding errors between beats.
    pub fn audio_range(self, rate: FrameRate) -> Result<Range<AudioSample>, TimeError> {
        Ok(rate.audio_boundary(self.start)?..rate.audio_boundary(self.end)?)
    }
}

/// An exact, positive, normalized source-stream time base in seconds per tick.
///
/// This is deliberately distinct from a project presentation frame rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceTimeBase {
    numerator: u32,
    denominator: u32,
}

impl SourceTimeBase {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, TimeError> {
        if numerator == 0 || denominator == 0 {
            return Err(TimeError::InvalidSourceTimeBase);
        }
        let divisor = gcd(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    pub const fn denominator(self) -> u32 {
        self.denominator
    }
}

/// Original signed source PTS with its own stream's time base.
///
/// No project-frame conversion or implicit origin normalization is performed.
/// Ordering timestamps across streams requires explicit time-base conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceTimestamp {
    pub ticks: i64,
    pub time_base: SourceTimeBase,
}

/// Computes `plays × child + (plays − 1) × gap`, with no trailing gap.
///
/// `plays` is the total number of plays. It must be at least one, the child
/// must be positive, and the gap may be zero. Every arithmetic step is checked.
pub fn repeat_duration(
    child: FrameDuration,
    plays: u32,
    gap: FrameDuration,
) -> Result<FrameDuration, TimeError> {
    if plays == 0 {
        return Err(TimeError::ZeroPlays);
    }
    if child == FrameDuration::ZERO {
        return Err(TimeError::EmptyRepeatChild);
    }
    let child_total = child
        .0
        .checked_mul(i64::from(plays))
        .ok_or(TimeError::Overflow)?;
    let gap_total = gap
        .0
        .checked_mul(i64::from(plays - 1))
        .ok_or(TimeError::Overflow)?;
    child_total
        .checked_add(gap_total)
        .map(FrameDuration)
        .ok_or(TimeError::Overflow)
}

/// Invalid timing inputs or an unrepresentable exact arithmetic result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeError {
    InvalidFrameRate,
    InvalidSourceTimeBase,
    NegativeDuration,
    ReversedRange,
    ZeroPlays,
    EmptyRepeatChild,
    Overflow,
}

impl fmt::Display for TimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidFrameRate => "frame rate numerator and denominator must be positive",
            Self::InvalidSourceTimeBase => {
                "source time-base numerator and denominator must be positive"
            }
            Self::NegativeDuration => "frame duration must be nonnegative",
            Self::ReversedRange => "frame range end must not precede its start",
            Self::ZeroPlays => "repeat must have at least one total play",
            Self::EmptyRepeatChild => "repeat child duration must be positive",
            Self::Overflow => "timing arithmetic exceeds the representable range",
        })
    }
}

impl Error for TimeError {}

fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

// The divisor comes from a validated positive frame-rate numerator. Integer
// division truncates toward zero, so a rounding increment follows the dividend's
// sign. The remainder's magnitude is strictly smaller than the u32 divisor.
fn round_even(dividend: i128, divisor: i128) -> Result<i128, TimeError> {
    let quotient = dividend / divisor;
    let twice_remainder = (dividend % divisor)
        .abs()
        .checked_mul(2)
        .ok_or(TimeError::Overflow)?;
    if twice_remainder > divisor || (twice_remainder == divisor && quotient % 2 != 0) {
        quotient
            .checked_add(dividend.signum())
            .ok_or(TimeError::Overflow)
    } else {
        Ok(quotient)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn rates_and_time_bases_are_positive_and_normalized() -> Result<(), TimeError> {
        assert_eq!(
            FrameRate::new(60_000, 2_002)?,
            FrameRate::new(30_000, 1_001)?
        );
        assert_eq!(FrameRate::new(30_000, 1_001)?.numerator(), 30_000);
        assert_eq!(FrameRate::new(30_000, 1_001)?.denominator(), 1_001);
        assert_eq!(
            SourceTimeBase::new(2, 96_000)?,
            SourceTimeBase::new(1, 48_000)?
        );
        assert_eq!(SourceTimeBase::new(2, 96_000)?.numerator(), 1);
        assert_eq!(SourceTimeBase::new(2, 96_000)?.denominator(), 48_000);
        for (numerator, denominator) in [(0, 1), (1, 0), (0, 0)] {
            assert_eq!(
                FrameRate::new(numerator, denominator),
                Err(TimeError::InvalidFrameRate)
            );
            assert_eq!(
                SourceTimeBase::new(numerator, denominator),
                Err(TimeError::InvalidSourceTimeBase)
            );
        }
        let source = SourceTimestamp {
            ticks: -1_024,
            time_base: SourceTimeBase::new(1, 48_000)?,
        };
        assert_eq!(source.ticks, -1_024);
        Ok(())
    }

    #[test]
    fn ties_round_to_even_on_both_sides_of_zero() -> Result<(), TimeError> {
        let rate = FrameRate::new(96_000, 1)?;
        for (frame, sample) in [
            (-7, -4),
            (-5, -2),
            (-3, -2),
            (-1, 0),
            (0, 0),
            (1, 0),
            (3, 2),
            (5, 2),
            (7, 4),
        ] {
            assert_eq!(
                rate.audio_boundary(ProjectFrame(frame))?,
                AudioSample(sample)
            );
        }
        let fractional = FrameRate::new(30_000, 1_001)?;
        for (frame, sample) in [(-2, -3_203), (-1, -1_602), (1, 1_602), (2, 3_203)] {
            assert_eq!(
                fractional.audio_boundary(ProjectFrame(frame))?,
                AudioSample(sample)
            );
        }
        Ok(())
    }

    #[test]
    fn half_open_ranges_reject_reversal_and_duration_overflow() -> Result<(), TimeError> {
        let range = FrameRange::new(ProjectFrame(-2), ProjectFrame(3))?;
        assert_eq!(range.duration().frames(), 5);
        assert!(range.contains(ProjectFrame(-2)));
        assert!(range.contains(ProjectFrame(2)));
        assert!(!range.contains(ProjectFrame(3)));
        let empty = FrameRange::new(ProjectFrame(2), ProjectFrame(2))?;
        assert_eq!(empty.duration(), FrameDuration::ZERO);
        assert!(!empty.contains(ProjectFrame(2)));
        assert_eq!(
            FrameRange::new(ProjectFrame(2), ProjectFrame(1)),
            Err(TimeError::ReversedRange)
        );
        assert_eq!(
            FrameRange::new(ProjectFrame(i64::MIN), ProjectFrame(0)),
            Err(TimeError::Overflow)
        );
        assert_eq!(
            FrameRange::new(ProjectFrame(i64::MIN), ProjectFrame(-1))?
                .duration()
                .frames(),
            i64::MAX
        );
        Ok(())
    }

    #[test]
    fn ten_thousand_single_frame_edits_have_no_fractional_rate_drift() -> Result<(), TimeError> {
        let rate = FrameRate::new(30_000, 1_001)?;
        let mut sample_count = 0;
        let mut previous_boundary = AudioSample(0);
        for frame in 0..10_000 {
            let samples =
                FrameRange::new(ProjectFrame(frame), ProjectFrame(frame + 1))?.audio_range(rate)?;
            assert_eq!(samples.start, previous_boundary);
            sample_count += samples.end.0 - samples.start.0;
            previous_boundary = samples.end;
        }
        assert_eq!(sample_count, 16_016_000);
        assert_eq!(
            rate.audio_boundary(ProjectFrame(10_000))?,
            AudioSample(sample_count)
        );
        // Rounding each one-frame duration and then adding would be wrong.
        assert_ne!(
            sample_count,
            rate.audio_boundary(ProjectFrame(1))?.0 * 10_000
        );
        Ok(())
    }

    #[test]
    fn sample_boundaries_report_overflow_without_restricting_signed_positions()
    -> Result<(), TimeError> {
        let identity = FrameRate::new(MIX_SAMPLE_RATE, 1)?;
        assert_eq!(
            identity.audio_boundary(ProjectFrame(i64::MIN))?,
            AudioSample(i64::MIN)
        );
        assert_eq!(
            identity.audio_boundary(ProjectFrame(i64::MAX))?,
            AudioSample(i64::MAX)
        );
        let large = FrameRate::new(1, u32::MAX)?;
        assert_eq!(
            large.audio_boundary(ProjectFrame(i64::MIN)),
            Err(TimeError::Overflow)
        );
        assert_eq!(
            large.audio_boundary(ProjectFrame(i64::MAX)),
            Err(TimeError::Overflow)
        );
        Ok(())
    }

    #[test]
    fn repeats_count_total_plays_and_have_no_trailing_gap() -> Result<(), TimeError> {
        let child = FrameDuration::new(5)?;
        let gap = FrameDuration::new(2)?;
        assert_eq!(repeat_duration(child, 1, gap)?, child);
        assert_eq!(repeat_duration(child, 3, gap)?.frames(), 19);
        assert_eq!(repeat_duration(child, 3, FrameDuration::ZERO)?.frames(), 15);
        assert_eq!(repeat_duration(child, 0, gap), Err(TimeError::ZeroPlays));
        assert_eq!(
            repeat_duration(FrameDuration::ZERO, 1, gap),
            Err(TimeError::EmptyRepeatChild)
        );
        assert_eq!(FrameDuration::new(-1), Err(TimeError::NegativeDuration));
        Ok(())
    }

    #[test]
    fn duration_arithmetic_checks_each_overflow_path() -> Result<(), TimeError> {
        let one = FrameDuration::new(1)?;
        let max = FrameDuration::new(i64::MAX)?;
        assert_eq!(max.checked_add(one), Err(TimeError::Overflow));
        assert_eq!(
            repeat_duration(max, 2, FrameDuration::ZERO),
            Err(TimeError::Overflow)
        );
        assert_eq!(repeat_duration(one, 3, max), Err(TimeError::Overflow));
        assert_eq!(repeat_duration(one, 2, max), Err(TimeError::Overflow));
        assert_eq!(repeat_duration(max, 1, max)?, max);
        Ok(())
    }

    proptest! {
        #[test]
        fn boundaries_are_monotonic_and_symmetric(
            frame in -1_000_000_i64..1_000_000,
            numerator in 1_u32..100_000,
            denominator in 1_u32..10_000,
        ) {
            let rate = FrameRate::new(numerator, denominator)?;
            let sample = rate.audio_boundary(ProjectFrame(frame))?;
            let opposite = rate.audio_boundary(ProjectFrame(-frame))?;
            let next = rate.audio_boundary(ProjectFrame(frame + 1))?;
            prop_assert_eq!(sample.0, -opposite.0);
            prop_assert!(sample <= next);
        }

        #[test]
        fn partition_boundaries_telescope(
            start in -10_000_i64..10_000,
            lengths in proptest::collection::vec(0_i64..100, 0..200),
            numerator in 1_u32..100_000,
            denominator in 1_u32..10_000,
        ) {
            let rate = FrameRate::new(numerator, denominator)?;
            let mut cursor = start;
            let mut total = 0_i64;
            for length in lengths {
                let samples = FrameRange::new(ProjectFrame(cursor), ProjectFrame(cursor + length))?.audio_range(rate)?;
                total += samples.end.0 - samples.start.0;
                cursor += length;
            }
            prop_assert_eq!(total, rate.audio_boundary(ProjectFrame(cursor))?.0 - rate.audio_boundary(ProjectFrame(start))?.0);
        }

        #[test]
        fn equivalent_rational_rates_produce_identical_boundaries(
            numerator in 1_u32..100_000,
            denominator in 1_u32..10_000,
            factor in 1_u32..1_000,
            frame in -1_000_000_i64..1_000_000,
        ) {
            let rate = FrameRate::new(numerator, denominator)?;
            let scaled = FrameRate::new(numerator * factor, denominator * factor)?;
            prop_assert_eq!(rate, scaled);
            prop_assert_eq!(rate.audio_boundary(ProjectFrame(frame))?, scaled.audio_boundary(ProjectFrame(frame))?);
        }
    }
}
