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
        _ => return Err(invalid()),
    };
    if words.next().is_some() {
        return Err(invalid());
    }
    Ok(RetimeInput { speed, pitch, wrap })
}

pub fn pitch_name(pitch: PitchPolicy) -> &'static str {
    match pitch {
        PitchPolicy::Preserve => "preserve",
        PitchPolicy::FollowSpeed => "tape",
    }
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
