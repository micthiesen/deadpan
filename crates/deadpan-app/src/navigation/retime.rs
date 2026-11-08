//! Exact dimensionless playback speed and explicit sound policy.

use deadpan_core::{ExactRatio, PitchPolicy};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetimeInput {
    pub speed: ExactRatio,
    pub pitch: PitchPolicy,
    /// Explicit nesting; otherwise an ordinary Retime is adjusted in place.
    pub wrap: bool,
}

pub fn parse<'a>(
    mut words: impl Iterator<Item = &'a str>,
    wrap: bool,
) -> Result<RetimeInput, String> {
    let invalid = || {
        "Use :retime 0.75 pitch=preserve or :retime 3/4 pitch=tape. Speed must be positive; choose a pitch policy explicitly.".to_owned()
    };
    let value = words.next().ok_or_else(invalid)?;
    let speed = if let Some((numerator, denominator)) = value.split_once('/') {
        let integer = |text: &str| {
            if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(invalid());
            }
            text.parse::<i128>().map_err(|_| invalid())
        };
        ExactRatio::new(integer(numerator)?, integer(denominator)?).map_err(|_| invalid())?
    } else {
        super::duration::decimal(value).map_err(|_| invalid())?
    };
    if !speed.compare_integer(0).is_gt() {
        return Err(invalid());
    }
    let pitch = match words.next() {
        Some("pitch=preserve") => PitchPolicy::Preserve,
        Some("pitch=tape") => PitchPolicy::FollowSpeed,
        Some(value) if value.starts_with("pitch=") => {
            PitchPolicy::shifted(parse_semitones(&value["pitch=".len()..])?)
                .map_err(|error| error.to_string())?
        }
        _ => return Err(invalid()),
    };
    if words.next().is_some() {
        return Err(invalid());
    }
    Ok(RetimeInput { speed, pitch, wrap })
}

/// `:ramp FROM TO [steps=N] pitch=tape|preserve`: a stepped speed ramp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RampInput {
    pub from: ExactRatio,
    pub to: ExactRatio,
    pub steps: u32,
    pub pitch: PitchPolicy,
}

pub const RAMP_USAGE: &str = "Use :ramp 1 0.25 pitch=tape (or 3/4, steps=8 by default, at most 32) to ramp the selected beat's speed from the first to the second; choose a pitch policy explicitly.";

pub fn parse_ramp<'a>(words: impl Iterator<Item = &'a str>) -> Result<RampInput, String> {
    let invalid = || RAMP_USAGE.to_owned();
    let mut speeds = Vec::new();
    let mut steps = 8_u32;
    let mut pitch = None;
    for word in words {
        if let Some(value) = word.strip_prefix("steps=") {
            steps = value.parse().map_err(|_| invalid())?;
        } else if word.starts_with("pitch=") {
            let input = parse(["1", word].into_iter(), false).map_err(|_| invalid())?;
            pitch = Some(input.pitch);
        } else {
            speeds.push(
                parse([word, "pitch=tape"].into_iter(), false)
                    .map_err(|_| invalid())?
                    .speed,
            );
        }
    }
    let ([from, to], Some(pitch)) = (speeds.as_slice(), pitch) else {
        return Err(invalid());
    };
    if !(2..=deadpan_core::MAX_RAMP_STEPS).contains(&steps) {
        return Err(invalid());
    }
    Ok(RampInput {
        from: *from,
        to: *to,
        steps,
        pitch,
    })
}

pub fn pitch_name(pitch: PitchPolicy) -> String {
    match pitch {
        PitchPolicy::Preserve => "preserve".into(),
        PitchPolicy::FollowSpeed => "tape".into(),
        PitchPolicy::Shift { semitones } => format!("{semitones:+}st"),
    }
}

const PITCH_USAGE: &str = "Use :pitch +3st or :pitch -2st (whole semitones within ±24) to shift the selected beat's pitch at its current speed; :pitch 0 removes the shift.";

/// Signed whole semitones with an optional `st` unit, within ±24.
pub fn parse_semitones(value: &str) -> Result<i8, String> {
    let number = value.strip_suffix("st").unwrap_or(value);
    let (negative, digits) = match number.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, number.strip_prefix('+').unwrap_or(number)),
    };
    if digits.is_empty() || digits.len() > 2 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(PITCH_USAGE.into());
    }
    let magnitude: i8 = digits.parse().map_err(|_| PITCH_USAGE.to_owned())?;
    if magnitude > deadpan_core::MAX_PITCH_SHIFT_SEMITONES {
        return Err(PITCH_USAGE.into());
    }
    Ok(if negative { -magnitude } else { magnitude })
}

/// `:pitch +3st`.
pub fn parse_pitch<'a>(mut words: impl Iterator<Item = &'a str>) -> Result<i8, String> {
    let value = words.next().ok_or(PITCH_USAGE)?;
    if words.next().is_some() {
        return Err(PITCH_USAGE.into());
    }
    parse_semitones(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::{
        Action, BeatEdit,
        command::{self, Entry},
    };

    #[test]
    fn speeds_are_exact_and_pitch_is_chosen_explicitly() {
        for speed in ["0.75", "3/4", "000.7500", "6/8"] {
            assert_eq!(
                command::parse(&format!(":retime {speed} pitch=preserve")),
                Ok(Entry::Action(Action::Edit(BeatEdit::Retime(RetimeInput {
                    speed: ExactRatio::new(3, 4).unwrap(),
                    pitch: PitchPolicy::Preserve,
                    wrap: false,
                }))))
            );
        }
        assert_eq!(
            command::parse("wrap-retime 2 pitch=tape"),
            Ok(Entry::Action(Action::Edit(BeatEdit::Retime(RetimeInput {
                speed: ExactRatio::integer(2),
                pitch: PitchPolicy::FollowSpeed,
                wrap: true,
            }))))
        );
    }

    #[test]
    fn pitch_shifts_are_whole_semitones_on_pitch_preserving_processing() {
        assert_eq!(
            command::parse(":retime 1 pitch=+3st"),
            Ok(Entry::Action(Action::Edit(BeatEdit::Retime(RetimeInput {
                speed: ExactRatio::ONE,
                pitch: PitchPolicy::Shift { semitones: 3 },
                wrap: false,
            }))))
        );
        assert_eq!(
            command::parse(":pitch -2st"),
            Ok(Entry::Action(Action::Edit(BeatEdit::Pitch(-2))))
        );
        assert_eq!(
            command::parse(":pitch 24"),
            Ok(Entry::Action(Action::Edit(BeatEdit::Pitch(24))))
        );
        assert_eq!(
            command::parse(":retime 0.5 pitch=0st"),
            Ok(Entry::Action(Action::Edit(BeatEdit::Retime(RetimeInput {
                speed: ExactRatio::new(1, 2).unwrap(),
                pitch: PitchPolicy::Preserve,
                wrap: false,
            }))))
        );
        for input in [
            "pitch",
            "pitch 25st",
            "pitch 1.5",
            "pitch +3st extra",
            "retime 1 pitch=x",
        ] {
            assert!(command::parse(input).is_err(), "{input}");
        }
        assert_eq!(pitch_name(PitchPolicy::Shift { semitones: -5 }), "-5st");
    }

    #[test]
    fn invalid_or_ambiguous_speed_never_becomes_an_edit() {
        for value in [
            "0",
            "0/1",
            "-1",
            "1/-1",
            "-1/-1",
            "1/0",
            "1/2/3",
            "1e2",
            "NaN",
            "inf",
            ".75",
            "1.",
            "2x",
            "1f",
            "+1",
            "1.2.3",
            "9999999999999999999999999999999999999999999",
        ] {
            assert!(
                command::parse(&format!("retime {value} pitch=preserve")).is_err(),
                "{value}"
            );
        }
        for command in [
            "retime",
            "retime 1",
            "retime 1 preserve",
            "retime 1 pitch=unknown",
            "retime pitch=tape 1",
            "retime 1 pitch=tape pitch=preserve",
            "retime 1 pitch=tape extra",
        ] {
            assert!(command::parse(command).is_err(), "{command}");
        }
    }
}

#[cfg(test)]
mod ramp_tests {
    use super::*;

    #[test]
    fn ramps_take_two_speeds_optional_steps_and_an_explicit_pitch() {
        let parsed = parse_ramp("1 3/4 steps=4 pitch=tape".split_whitespace()).unwrap();
        assert_eq!(
            parsed,
            RampInput {
                from: ExactRatio::ONE,
                to: ExactRatio::new(3, 4).unwrap(),
                steps: 4,
                pitch: PitchPolicy::FollowSpeed,
            }
        );
        assert_eq!(
            parse_ramp("2 0.5 pitch=preserve".split_whitespace())
                .unwrap()
                .steps,
            8
        );
        for input in [
            "1 2",
            "1 pitch=tape",
            "1 2 steps=1 pitch=tape",
            "1 2 steps=33 pitch=tape",
            "0 2 pitch=tape",
            "1 2 3 pitch=tape",
        ] {
            assert!(parse_ramp(input.split_whitespace()).is_err(), "{input}");
        }
    }
}
