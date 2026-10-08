//! Exact native coverage for quality checks of a one-sided generated interval.
//!
//! Context is indexed but never counted as generated evidence. Wire observations
//! stay chronological; a tracker travels outward from the one retained anchor.

use deadpan_core::ExtensionDirection;
use serde::{Deserialize, Serialize};

pub const MAX_NATIVE_FRAMES: usize = 4096;
pub const MAX_GENERATED_FRAMES: usize = 1025;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionCoverage {
    pub direction: ExtensionDirection,
    /// Inclusive native ordinal of the first generated picture.
    pub start: u32,
    /// Exclusive native ordinal after the last generated picture.
    pub end: u32,
}

impl ExtensionCoverage {
    pub fn validate(&self, expected_native_pts: &[i64]) -> Result<(), CoverageError> {
        if !(2..=MAX_NATIVE_FRAMES).contains(&expected_native_pts.len()) {
            return Err(CoverageError::NativeFrameLimit);
        }
        if expected_native_pts
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err(CoverageError::ExpectedPtsOrder);
        }
        let Some(count) = self.end.checked_sub(self.start) else {
            return Err(CoverageError::GeneratedInterval);
        };
        if !(1..=MAX_GENERATED_FRAMES as u32).contains(&count)
            || self.end as usize > expected_native_pts.len()
        {
            return Err(CoverageError::GeneratedInterval);
        }
        let valid = match self.direction {
            ExtensionDirection::FromLeft => {
                self.start > 0 && self.end as usize == expected_native_pts.len()
            }
            ExtensionDirection::FromRight => {
                self.start == 0 && (self.end as usize) < expected_native_pts.len()
            }
        };
        if !valid {
            return Err(CoverageError::GeneratedInterval);
        }
        Ok(())
    }

    pub fn ordinals(&self) -> std::ops::Range<u32> {
        self.start..self.end
    }

    /// Validate before use. This allocation is bounded even for malformed input.
    pub fn tracking_ordinals(&self) -> Vec<u32> {
        if self
            .end
            .checked_sub(self.start)
            .is_none_or(|count| count == 0 || count > MAX_GENERATED_FRAMES as u32)
        {
            return Vec::new();
        }
        match self.direction {
            ExtensionDirection::FromLeft => self.ordinals().collect(),
            ExtensionDirection::FromRight => self.ordinals().rev().collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CoverageError {
    #[error("extension native frame count exceeds the inspection bound")]
    NativeFrameLimit,
    #[error("extension native PTS values are not strictly increasing")]
    ExpectedPtsOrder,
    #[error("generated interval must be bounded, touch its directional edge and leave context")]
    GeneratedInterval,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_keeps_canonical_ordinals_and_separate_tracking_order() {
        let pts = [i64::MIN, -4, 0, 9, i64::MAX];
        let left = ExtensionCoverage {
            direction: ExtensionDirection::FromLeft,
            start: 2,
            end: 5,
        };
        let right = ExtensionCoverage {
            direction: ExtensionDirection::FromRight,
            start: 0,
            end: 3,
        };
        left.validate(&pts).unwrap();
        right.validate(&pts).unwrap();
        assert_eq!(left.tracking_ordinals(), [2, 3, 4]);
        assert_eq!(right.tracking_ordinals(), [2, 1, 0]);
        assert_eq!(right.ordinals().collect::<Vec<_>>(), [0, 1, 2]);
    }

    #[test]
    fn rejects_contextless_interior_empty_and_unbounded_coverage() {
        let pts: Vec<_> = (0..16).collect();
        for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
            for (start, end) in [(0, 16), (1, 15), (2, 2), (5, 3), (0, 17), (0, u32::MAX)] {
                assert!(
                    ExtensionCoverage {
                        direction,
                        start,
                        end
                    }
                    .validate(&pts)
                    .is_err()
                );
            }
        }
        let coverage = ExtensionCoverage {
            direction: ExtensionDirection::FromLeft,
            start: 1,
            end: 2,
        };
        assert!(coverage.validate(&[1, 1]).is_err());
        assert!(coverage.validate(&[2, 1]).is_err());
        assert!(coverage.validate(&[0]).is_err());
        let pts: Vec<_> = (0..MAX_NATIVE_FRAMES as i64 + 1).collect();
        assert!(coverage.validate(&pts).is_err());
        let excessive = ExtensionCoverage {
            direction: ExtensionDirection::FromLeft,
            start: 1,
            end: MAX_GENERATED_FRAMES as u32 + 2,
        };
        assert!(excessive.validate(&pts[..MAX_NATIVE_FRAMES]).is_err());
        assert!(excessive.tracking_ordinals().is_empty());
    }

    #[test]
    fn admits_single_generated_picture_without_fabricating_an_endpoint() {
        for (direction, start, end) in [
            (ExtensionDirection::FromLeft, 1, 2),
            (ExtensionDirection::FromRight, 0, 1),
        ] {
            let coverage = ExtensionCoverage {
                direction,
                start,
                end,
            };
            coverage.validate(&[0, 41]).unwrap();
            assert_eq!(coverage.tracking_ordinals(), [start]);
        }
    }
}
