//! Typed placed-sound actions and exact command arguments. Selection ownership,
//! source admission and reversible writes belong to the project service.

use deadpan_core::{
    AudioEdgePolicy, AudioSample, MAX_SOUND_GAIN_MILLIDECIBELS, MIN_SOUND_GAIN_MILLIDECIBELS,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundAction {
    Place,
    Focus,
    /// Absolute onset on the project-origin 48 kHz sample clock.
    Move(AudioSample),
    /// Absolute authored millidecibels, independent of monitoring volume.
    Gain(i32),
    /// Relative authored millidecibels for the focused placed sound.
    GainStep(i32),
    /// Apply the same policy to both event endpoints.
    Edges(AudioEdgePolicy),
    Delete,
}

pub(super) fn parse(verb: &str, argument: Option<&str>) -> Result<SoundAction, String> {
    match verb {
        "sound-place" | "sounds" | "sound-delete" => {
            if argument.is_some() {
                return Err("This command takes no arguments.".into());
            }
            Ok(match verb {
                "sound-place" => SoundAction::Place,
                "sounds" => SoundAction::Focus,
                _ => SoundAction::Delete,
            })
        }
        "sound-at" => onset(argument).map(SoundAction::Move),
        "sound-gain" => gain(argument).map(SoundAction::Gain),
        "sound-edges" => match argument {
            Some("soft") => Ok(SoundAction::Edges(AudioEdgePolicy::Automatic)),
            Some("hard") => Ok(SoundAction::Edges(AudioEdgePolicy::Hard)),
            _ => Err("Use :sound-edges soft or :sound-edges hard for both sound endpoints.".into()),
        },
        _ => Err("Unknown sound command. Use :help for available commands.".into()),
    }
}

fn onset(argument: Option<&str>) -> Result<AudioSample, String> {
    let invalid = || {
        "Use :sound-at N with a nonnegative whole number of 48 kHz samples, for example 274000."
            .to_owned()
    };
    let input = argument.ok_or_else(invalid)?;
    if input.is_empty() || !input.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid());
    }
    input.parse::<i64>().map(AudioSample).map_err(|_| invalid())
}

fn gain(argument: Option<&str>) -> Result<i32, String> {
    let invalid = || {
        "Use :sound-gain -3 with exact dB from -96 to 24 and at most three decimal places."
            .to_owned()
    };
    let input = argument.ok_or_else(invalid)?;
    let (negative, magnitude) = if let Some(magnitude) = input.strip_prefix('-') {
        (true, magnitude)
    } else {
        (false, input.strip_prefix('+').unwrap_or(input))
    };
    let (whole, fraction) = match magnitude.split_once('.') {
        Some((whole, fraction)) if !fraction.is_empty() => (whole, fraction),
        Some(_) => return Err(invalid()),
        None => (magnitude, ""),
    };
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 3
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid());
    }
    // Integer thousandths preserve user intent without floating-point parsing
    // or rounding. The fractional part has at most three ASCII digits.
    let fractional = if fraction.is_empty() {
        0
    } else {
        let scale = match fraction.len() {
            1 => 100,
            2 => 10,
            _ => 1,
        };
        fraction.parse::<i32>().map_err(|_| invalid())? * scale
    };
    let magnitude = whole
        .parse::<i32>()
        .ok()
        .and_then(|whole| whole.checked_mul(1000))
        .and_then(|whole| whole.checked_add(fractional))
        .ok_or_else(invalid)?;
    let value = if negative {
        magnitude.checked_neg().ok_or_else(invalid)?
    } else {
        magnitude
    };
    if !(MIN_SOUND_GAIN_MILLIDECIBELS..=MAX_SOUND_GAIN_MILLIDECIBELS).contains(&value) {
        return Err(invalid());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::super::{
        Action,
        command::{self, Entry},
    };
    use super::*;

    fn action(input: &str, expected: SoundAction) {
        assert_eq!(
            command::parse(input),
            Ok(Entry::Action(Action::Sound(expected))),
            "{input}"
        );
    }

    #[test]
    fn sound_commands_are_explicit_and_reject_extraneous_arguments() {
        action(":sound-place", SoundAction::Place);
        action("SOUNDS", SoundAction::Focus);
        action(":sound-delete", SoundAction::Delete);
        action(
            ":sound-edges soft",
            SoundAction::Edges(AudioEdgePolicy::Automatic),
        );
        action(
            ":sound-edges hard",
            SoundAction::Edges(AudioEdgePolicy::Hard),
        );
        for input in [
            "sound-place 2",
            "sounds now",
            "sound-delete all",
            "sound-edges",
            "sound-edges automatic",
            "sound-edges hard soft",
            "sound-gain -3 extra",
            "sound-at 5 6",
        ] {
            assert!(command::parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn sound_onset_uses_whole_sample_labels_without_frame_rounding() {
        for (input, value) in [
            ("sound-at 0", 0),
            ("sound-at 274000", 274000),
            ("sound-at 0007", 7),
            ("sound-at 9223372036854775807", i64::MAX),
        ] {
            action(input, SoundAction::Move(AudioSample(value)));
        }
        for input in [
            "sound-at",
            "sound-at -1",
            "sound-at +1",
            "sound-at 1.0",
            "sound-at 12f",
            "sound-at 1s",
            "sound-at 1e3",
            "sound-at １２",
            "sound-at 9223372036854775808",
        ] {
            assert!(command::parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn sound_gain_is_exact_bounded_millidecibels() {
        for (input, value) in [
            ("sound-gain -96", -96000),
            ("sound-gain 24.000", 24000),
            ("sound-gain -3", -3000),
            ("sound-gain +3.125", 3125),
            ("sound-gain -0.001", -1),
            ("sound-gain 0.01", 10),
            ("sound-gain 0.1", 100),
            ("sound-gain -0", 0),
            ("sound-gain 023.999", 23999),
        ] {
            action(input, SoundAction::Gain(value));
        }
        for input in [
            "sound-gain",
            "sound-gain -96.001",
            "sound-gain 24.001",
            "sound-gain 0.0001",
            "sound-gain 3.",
            "sound-gain .5",
            "sound-gain --3",
            "sound-gain +-3",
            "sound-gain 3dB",
            "sound-gain NaN",
            "sound-gain inf",
            "sound-gain 1e1",
            "sound-gain １",
            "sound-gain 2147483647",
            "sound-gain 2147483648",
            "sound-gain 1.2.3",
        ] {
            assert!(command::parse(input).is_err(), "{input}");
        }
    }
}
