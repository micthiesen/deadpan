//! The two closed numeric domains of the shared compact Repeat index.
//! Integer documents keep their original i128 prefix arithmetic. Derived
//! clocks use checked ratios and never round a prefix or a query coordinate.

use std::{cmp::Ordering, fmt::Debug};

use crate::{ExactRatio, FrameDuration, TimeError};

pub(super) trait Clock: Clone + Debug {
    type Duration: Copy + Debug;
    type Position: Copy + Debug;
    const ZERO: Self::Duration;

    fn start(duration: Self::Duration) -> Self::Position;
    fn duration(position: Self::Position) -> Result<Self::Duration, TimeError>;
    fn coordinate(position: Self::Position) -> ExactRatio;
    fn length(duration: Self::Duration) -> ExactRatio;
    fn compare(position: ExactRatio, boundary: Self::Position) -> Ordering;
    fn prefix(
        start: Self::Position,
        child: Self::Duration,
        gap: Self::Duration,
        plays: u32,
        gaps: u32,
    ) -> Result<Self::Position, TimeError>;
    fn period(child: Self::Duration, gap: Self::Duration) -> Result<ExactRatio, TimeError>;
}

#[derive(Debug, Clone)]
pub(super) struct IntegerClock;

impl Clock for IntegerClock {
    type Duration = FrameDuration;
    type Position = i64;
    const ZERO: FrameDuration = FrameDuration::ZERO;

    fn start(duration: FrameDuration) -> i64 {
        duration.frames()
    }
    fn duration(position: i64) -> Result<FrameDuration, TimeError> {
        FrameDuration::new(position)
    }
    fn coordinate(position: i64) -> ExactRatio {
        ExactRatio::integer(position)
    }
    fn length(duration: FrameDuration) -> ExactRatio {
        ExactRatio::integer(duration.frames())
    }
    fn compare(position: ExactRatio, boundary: i64) -> Ordering {
        position.compare_integer(boundary)
    }
    fn prefix(
        start: i64,
        child: FrameDuration,
        gap: FrameDuration,
        plays: u32,
        gaps: u32,
    ) -> Result<i64, TimeError> {
        // Nonnegative i64 extents and u32 counts fit in this intermediate.
        let end = i128::from(start)
            + i128::from(child.frames()) * i128::from(plays)
            + i128::from(gap.frames()) * i128::from(gaps);
        i64::try_from(end).map_err(|_| TimeError::Overflow)
    }
    fn period(child: FrameDuration, gap: FrameDuration) -> Result<ExactRatio, TimeError> {
        ExactRatio::new(i128::from(child.frames()) + i128::from(gap.frames()), 1)
    }
}

#[derive(Debug, Clone)]
pub(super) struct ExactClock;

impl Clock for ExactClock {
    type Duration = ExactRatio;
    type Position = ExactRatio;
    const ZERO: ExactRatio = ExactRatio::ZERO;

    fn start(duration: ExactRatio) -> ExactRatio {
        duration
    }
    fn duration(position: ExactRatio) -> Result<ExactRatio, TimeError> {
        Ok(position)
    }
    fn coordinate(position: ExactRatio) -> ExactRatio {
        position
    }
    fn length(duration: ExactRatio) -> ExactRatio {
        duration
    }
    fn compare(position: ExactRatio, boundary: ExactRatio) -> Ordering {
        position.compare(boundary)
    }
    fn prefix(
        start: ExactRatio,
        child: ExactRatio,
        gap: ExactRatio,
        plays: u32,
        gaps: u32,
    ) -> Result<ExactRatio, TimeError> {
        start
            .checked_add(child.checked_mul(ExactRatio::integer(i64::from(plays)))?)?
            .checked_add(gap.checked_mul(ExactRatio::integer(i64::from(gaps)))?)
    }
    fn period(child: ExactRatio, gap: ExactRatio) -> Result<ExactRatio, TimeError> {
        child.checked_add(gap)
    }
}
