//! Borrowed final policy intervals on one typed output grid.
//! Validation completes before any emission or PCM zeroing.

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Support {
    Complete,
    Unsupported,
}

/// Checked local offsets shared by numeric and metadata policy consumers.
pub(super) fn offsets<T: Copy>(
    start: T,
    frames: usize,
    range: &Range<T>,
    index: impl Fn(T) -> i64,
) -> Result<Range<usize>, StageAudioError> {
    let start = index(start);
    let left = index(range.start)
        .checked_sub(start)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(StageAudioError::Range)?;
    let right = index(range.end)
        .checked_sub(start)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(StageAudioError::Range)?;
    if left >= right || right > frames {
        return Err(StageAudioError::Range);
    }
    Ok(left..right)
}

/// Visit existing policy, without preparing queries, sources or PCM. The cold
/// caller supplies a billed hook before each validation and emission, retaining
/// spent work on errors. The production replacement supplies a no-op: it keeps
/// its original control/work boundary and all-or-nothing policy zeroing.
/// Consumers must discard partial observations if their callback fails.
pub(super) fn visit<T: Copy>(
    start: T,
    frames: usize,
    ranges: &[Range<T>],
    index: impl Fn(T) -> i64,
    mut charge: impl FnMut() -> Result<(), StageAudioError>,
    mut emit: impl FnMut(&Range<T>, Range<usize>) -> Result<(), StageAudioError>,
) -> Result<(), StageAudioError> {
    for range in ranges {
        charge()?;
        offsets(start, frames, range, &index)?;
    }
    for range in ranges {
        charge()?;
        emit(range, offsets(start, frames, range, &index)?)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "policy_ranges/tests.rs"]
mod tests;
