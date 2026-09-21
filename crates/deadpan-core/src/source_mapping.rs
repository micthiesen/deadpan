//! Shared exact duration arithmetic for independently mapped source streams.

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
