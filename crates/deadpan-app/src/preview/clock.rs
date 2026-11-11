//! Project frames shown as time with frames, for people: `2:01.04` is two
//! minutes, one second and four frames into that second. Exact integer
//! arithmetic on the rational project rate; the frame number stays the
//! secondary value wherever this appears.

use deadpan_core::FrameRate;

/// `m:ss.ff` (or `h:mm:ss.ff` from an hour), where `ff` counts the frames
/// that begin inside the displayed second.
pub(super) fn time(frame: u64, rate: FrameRate) -> String {
    let numerator = u128::from(rate.numerator().max(1));
    let denominator = u128::from(rate.denominator().max(1));
    let frame = u128::from(frame);
    // Whole seconds elapsed at the start of this frame.
    let seconds = frame * denominator / numerator;
    // The first frame starting at or after that second.
    let first = (seconds * numerator).div_ceil(denominator);
    let frames = frame - first.min(frame);
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}.{frames:02}")
    } else {
        format!("{minutes}:{seconds:02}.{frames:02}")
    }
}

/// A signed frame difference as time, such as `+0:01.12` or `-0:00.03`.
pub(super) fn signed(frames: i64, rate: FrameRate) -> String {
    let sign = if frames < 0 { '-' } else { '+' };
    format!("{sign}{}", time(frames.unsigned_abs(), rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_rates_count_frames_inside_each_second() {
        let rate = FrameRate::new(24, 1).unwrap();
        assert_eq!(time(0, rate), "0:00.00");
        assert_eq!(time(23, rate), "0:00.23");
        assert_eq!(time(24, rate), "0:01.00");
        assert_eq!(time(24 * 121 + 4, rate), "2:01.04");
        assert_eq!(time(24 * 3600, rate), "1:00:00.00");
        assert_eq!(signed(-3, rate), "-0:00.03");
    }

    #[test]
    fn drop_frame_rates_stay_exact_at_second_boundaries() {
        let rate = FrameRate::new(30000, 1001).unwrap();
        // Frame 30 starts at 1.001 s: inside second 1, its first frame.
        assert_eq!(time(29, rate), "0:00.29");
        assert_eq!(time(30, rate), "0:01.00");
        // One minute is 1798.2 frames: frame 1799 is the first of minute 1.
        assert_eq!(time(1798, rate), "0:59.29");
        assert_eq!(time(1799, rate), "1:00.00");
    }
}
