//! Exact text durations. Quantization happens once on the requested clock.

use deadpan_core::{
    AudioSample, ExactRatio, FrameDuration, FrameRate, MIX_SAMPLE_RATE, ProjectFrame,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurationInput {
    Frames(FrameDuration),
    Seconds(ExactRatio),
}

impl DurationInput {
    pub fn half_seconds(count: u32) -> Self {
        Self::Seconds(ExactRatio::new(i128::from(count), 2).expect("bounded count"))
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        if let Some(frames) = value.strip_suffix('f') {
            let frames = digits(frames)?;
            return FrameDuration::new(i64::try_from(frames).map_err(|_| range_error())?)
                .map(Self::Frames)
                .map_err(|error| error.to_string());
        }
        let seconds = if let Some(milliseconds) = value.strip_suffix("ms") {
            decimal(milliseconds)?
                .checked_div(ExactRatio::integer(1000))
                .map_err(|error| error.to_string())?
        } else if let Some(seconds) = value.strip_suffix('s') {
            decimal(seconds)?
        } else if value.contains(':') {
            let parts: Vec<_> = value.split(':').collect();
            let (hours, minutes, seconds) = match parts.as_slice() {
                [minutes, seconds] => (0, digits(minutes)?, decimal(seconds)?),
                [hours, minutes, seconds] => {
                    let minutes = digits(minutes)?;
                    if minutes >= 60 {
                        return Err("Clock minutes must be below 60.".into());
                    }
                    (digits(hours)?, minutes, decimal(seconds)?)
                }
                _ => return Err("Use MM:SS.sss or HH:MM:SS.sss.".into()),
            };
            if seconds.compare_integer(60) != std::cmp::Ordering::Less {
                return Err("Clock seconds must be below 60.".into());
            }
            let whole = hours
                .checked_mul(3600)
                .and_then(|hours| minutes.checked_mul(60)?.checked_add(hours))
                .ok_or_else(range_error)?;
            seconds
                .checked_add(ExactRatio::new(whole, 1).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?
        } else {
            return Err("Use a duration with units: 12f, 250ms, 1.5s, or 01:02.500.".into());
        };
        Ok(Self::Seconds(seconds))
    }

    /// The macro form: exact milliseconds when the input has them, otherwise
    /// the frames it resolves to now.
    pub fn pause_length(self, rate: FrameRate) -> Result<deadpan_core::PauseLength, String> {
        use std::num::NonZeroU32;
        if let Self::Seconds(seconds) = self
            && let Ok(milliseconds) = seconds.checked_mul(ExactRatio::integer(1000))
            && milliseconds.denominator() == 1
            && let Some(milliseconds) = u32::try_from(milliseconds.numerator())
                .ok()
                .and_then(NonZeroU32::new)
        {
            return Ok(deadpan_core::PauseLength::Milliseconds { milliseconds });
        }
        let frames = u32::try_from(self.resolve(rate)?.frames())
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or("The pause resolves to no whole project frame.")?;
        Ok(deadpan_core::PauseLength::Frames { frames })
    }

    pub fn resolve(self, rate: FrameRate) -> Result<FrameDuration, String> {
        match self {
            Self::Frames(frames) => Ok(frames),
            Self::Seconds(seconds) => {
                let frames = seconds
                    .checked_mul(
                        ExactRatio::new(
                            i128::from(rate.numerator()),
                            i128::from(rate.denominator()),
                        )
                        .map_err(|error| error.to_string())?,
                    )
                    .and_then(ExactRatio::round_even)
                    .map_err(|error| error.to_string())?;
                FrameDuration::new(i64::try_from(frames).map_err(|_| range_error())?)
                    .map_err(|error| error.to_string())
            }
        }
    }

    /// Quantize once on the mix clock, without an intermediate frame boundary.
    pub fn samples(self, rate: FrameRate) -> Result<AudioSample, String> {
        match self {
            Self::Frames(frames) => rate
                .audio_boundary(ProjectFrame(frames.frames()))
                .map_err(|error| error.to_string()),
            Self::Seconds(seconds) => {
                if seconds.compare_integer(0).is_lt() {
                    return Err("Duration must be nonnegative, with explicit units.".into());
                }
                let samples = seconds
                    .checked_mul(ExactRatio::integer(i64::from(MIX_SAMPLE_RATE)))
                    .and_then(ExactRatio::round_even)
                    .map_err(|error| error.to_string())?;
                i64::try_from(samples)
                    .map(AudioSample)
                    .map_err(|_| "Duration exceeds the supported sample range.".into())
            }
        }
    }
}

fn range_error() -> String {
    "Duration exceeds the supported frame range.".into()
}

fn digits(value: &str) -> Result<i128, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Duration must be nonnegative, with explicit units.".into());
    }
    value.parse().map_err(|_| range_error())
}

pub(super) fn decimal(value: &str) -> Result<ExactRatio, String> {
    let Some((whole, fraction)) = value.split_once('.') else {
        return ExactRatio::new(digits(value)?, 1).map_err(|error| error.to_string());
    };
    let whole = digits(whole)?;
    let fractional = digits(fraction)?;
    let denominator = 10_i128
        .checked_pow(u32::try_from(fraction.len()).map_err(|_| range_error())?)
        .ok_or_else(range_error)?;
    let numerator = whole
        .checked_mul(denominator)
        .and_then(|whole| whole.checked_add(fractional))
        .ok_or_else(range_error)?;
    ExactRatio::new(numerator, denominator).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_durations_keep_subframe_precision_and_exact_frame_boundaries() {
        for (input, numerator, denominator, expected) in [
            ("0f", 30, 1, 0),
            ("0ms", 30, 1, 0),
            ("500ms", 30_000, 1001, 24_000),
            ("750ms", 24, 1, 36_000),
            ("1f", 30_000, 1001, 1602),
            ("5f", 30_000, 1001, 8008),
            ("01:02.500", 25, 1, 3_000_000),
            ("0.00003125s", 30, 1, 2),
            ("0.00009375s", 30, 1, 4),
            ("0.00001s", 30, 1, 0),
        ] {
            assert_eq!(
                DurationInput::parse(input)
                    .unwrap()
                    .samples(FrameRate::new(numerator, denominator).unwrap()),
                Ok(AudioSample(expected)),
                "{input}"
            );
        }
    }

    #[test]
    fn sample_durations_check_negative_values_and_all_overflow_boundaries() {
        let rate = FrameRate::new(48_000, 1).unwrap();
        assert_eq!(
            DurationInput::Frames(FrameDuration::new(i64::MAX).unwrap()).samples(rate),
            Ok(AudioSample(i64::MAX))
        );
        assert!(
            DurationInput::Frames(FrameDuration::new(i64::MAX).unwrap())
                .samples(FrameRate::new(24, 1).unwrap())
                .is_err()
        );
        let maximum =
            DurationInput::Seconds(ExactRatio::new(i128::from(i64::MAX), 48_000).unwrap());
        assert_eq!(maximum.samples(rate), Ok(AudioSample(i64::MAX)));
        for seconds in [
            ExactRatio::new(i128::from(i64::MAX) + 1, 48_000).unwrap(),
            ExactRatio::new(i128::MAX, 1).unwrap(),
            ExactRatio::new(-1, 1).unwrap(),
        ] {
            assert!(DurationInput::Seconds(seconds).samples(rate).is_err());
        }
    }

    #[test]
    fn duration_inputs_quantize_once_with_ties_to_even() {
        for (input, numerator, denominator, expected) in [
            ("12f", 30, 1, 12),
            ("250ms", 30, 1, 8),
            ("1.5s", 30_000, 1001, 45),
            ("01:02.500", 30, 1, 1875),
            ("01:01:02.500", 30, 1, 109875),
            ("0.5s", 25, 1, 12),
            ("1.5s", 25, 1, 38),
            ("0.001s", 30, 1, 0),
            ("0f", 30, 1, 0),
        ] {
            let rate = FrameRate::new(numerator, denominator).unwrap();
            assert_eq!(
                DurationInput::parse(input)
                    .unwrap()
                    .resolve(rate)
                    .unwrap()
                    .frames(),
                expected,
                "{input}"
            );
        }
    }

    #[test]
    fn malformed_or_unbounded_durations_are_not_approximated() {
        for input in [
            "",
            "1",
            "-1f",
            "+1s",
            "1.5f",
            "1F",
            "1e3s",
            ".5s",
            "1.s",
            "1.2.3s",
            "00:60",
            "01:60:00",
            "1:2:3:4",
            "9223372036854775808f",
            "0.123456789012345678901234567890123456789s",
        ] {
            assert!(DurationInput::parse(input).is_err(), "{input}");
        }
    }
}
