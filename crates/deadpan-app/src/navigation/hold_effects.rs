//! `:reverse`, `:ping-pong` and `:tail`: pauses that replay the moment before
//! the cursor backwards, or let its sound ring on through an effect.

use deadpan_core::TailEffect;

use super::duration::DurationInput;

/// The reversed stretch when `:reverse` or `:ping-pong` gives no length.
pub const DEFAULT_REVERSE: &str = "8f";
pub const DEFAULT_PING_PONG: &str = "12f";
/// The new tail pause `,t` proposes when no pause is selected.
pub const DEFAULT_TAIL: &str = "1s";

const REVERSE_USAGE: &str = "Use :reverse 8f (play the 8 frames before the cursor backwards) or :ping-pong 12f (bounce back over them without repeating the last picture).";
const TAIL_USAGE: &str = "Use :tail 400ms effect=reverb|delay: a pause whose sound is the effect ringing on after what is heard just before it. On a selected pause the length is how long the tail rings.";

/// `:reverse [D]` or `:ping-pong [D]`.
pub fn parse_reverse(words: &[&str], bounce: bool) -> Result<DurationInput, String> {
    match words {
        [] => DurationInput::parse(if bounce {
            DEFAULT_PING_PONG
        } else {
            DEFAULT_REVERSE
        }),
        [length] => {
            DurationInput::parse(length).map_err(|error| format!("{error} {REVERSE_USAGE}"))
        }
        _ => Err(REVERSE_USAGE.into()),
    }
}

/// The split length `:jcut` and `:lcut` use when none is given.
pub const DEFAULT_SPLIT: &str = "6f";
const SPLIT_USAGE: &str = "Put the cursor on a cut between two source beats, then use :jcut 6f (the next beat's sound starts 6 frames early) or :lcut 200ms (this beat's sound runs on under the next picture).";

/// `:jcut [D]` or `:lcut [D]`.
pub fn parse_split(words: &[&str]) -> Result<DurationInput, String> {
    match words {
        [] => DurationInput::parse(DEFAULT_SPLIT),
        [length] => match DurationInput::parse(length) {
            Ok(DurationInput::Frames(frames)) if frames.frames() == 0 => Err(SPLIT_USAGE.into()),
            Ok(DurationInput::Seconds(seconds)) if seconds == deadpan_core::ExactRatio::ZERO => {
                Err(SPLIT_USAGE.into())
            }
            parsed => parsed.map_err(|error| format!("{error} {SPLIT_USAGE}")),
        },
        _ => Err(SPLIT_USAGE.into()),
    }
}

const BLEEP_USAGE: &str = "Use :bleep over a Visual range, optionally :bleep 880Hz level=-6dB (20 Hz to 20 kHz, at or below 0 dB).";

/// `:bleep [FREQUENCY[Hz]] [level=-10dB]`: frequency and level in
/// millidecibels, defaulting to the classic 1 kHz at -10 dB.
pub fn parse_bleep(words: &[&str]) -> Result<(u32, i32), String> {
    let mut frequency = None;
    let mut level = None;
    for word in words {
        match word.split_once('=') {
            Some(("level", value)) if level.is_none() => {
                let decibels = value
                    .strip_suffix("dB")
                    .or_else(|| value.strip_suffix("db"))
                    .unwrap_or(value);
                let parsed = super::gag::decimal(decibels.trim_start_matches('-'))
                    .map_err(|_| BLEEP_USAGE.to_owned())?;
                let millidecibels = parsed
                    .checked_mul(deadpan_core::ExactRatio::integer(1000))
                    .ok()
                    .filter(|value| value.denominator() == 1)
                    .and_then(|value| i32::try_from(value.numerator()).ok())
                    .ok_or(BLEEP_USAGE)?;
                level = Some(if decibels.starts_with('-') {
                    -millidecibels
                } else {
                    millidecibels
                });
            }
            Some(("freq" | "frequency", value)) if frequency.is_none() => {
                frequency = Some(hertz(value)?);
            }
            None if frequency.is_none() => frequency = Some(hertz(word)?),
            _ => return Err(BLEEP_USAGE.into()),
        }
    }
    let frequency = frequency.unwrap_or(deadpan_core::DEFAULT_BLEEP_FREQUENCY_HZ);
    let level = level.unwrap_or(deadpan_core::DEFAULT_BLEEP_LEVEL_MILLIDECIBELS);
    if !deadpan_core::TONE_FREQUENCY_HZ.contains(&frequency)
        || level > 0
        || deadpan_core::GainDb::new(level).is_err()
    {
        return Err(BLEEP_USAGE.into());
    }
    Ok((frequency, level))
}

fn hertz(value: &str) -> Result<u32, String> {
    value
        .strip_suffix("Hz")
        .or_else(|| value.strip_suffix("hz"))
        .unwrap_or(value)
        .parse::<u32>()
        .map_err(|_| BLEEP_USAGE.to_owned())
}

/// `:tail [D] [effect=reverb|delay]`.
pub fn parse_tail(words: &[&str]) -> Result<(Option<DurationInput>, TailEffect), String> {
    let mut length = None;
    let mut effect = None;
    for word in words {
        match word.split_once('=') {
            Some(("effect", value)) if effect.is_none() => {
                effect = Some(match value.to_ascii_lowercase().as_str() {
                    "reverb" => TailEffect::Reverb,
                    "delay" | "echo" => TailEffect::Delay,
                    _ => return Err("effect is reverb or delay.".into()),
                });
            }
            Some(("effect", _)) => return Err("Give effect= once.".into()),
            Some(_) => return Err(TAIL_USAGE.into()),
            None if length.is_none() => {
                length = Some(
                    DurationInput::parse(word).map_err(|error| format!("{error} {TAIL_USAGE}"))?,
                );
            }
            None => return Err(TAIL_USAGE.into()),
        }
    }
    Ok((length, effect.unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_lengths_default_and_parse_exact_units() {
        assert_eq!(
            parse_reverse(&[], false),
            DurationInput::parse(DEFAULT_REVERSE)
        );
        assert_eq!(
            parse_reverse(&[], true),
            DurationInput::parse(DEFAULT_PING_PONG)
        );
        assert_eq!(
            parse_reverse(&["250ms"], false),
            DurationInput::parse("250ms")
        );
        assert!(parse_reverse(&["8f", "9f"], false).is_err());
        assert!(parse_reverse(&["fast"], true).is_err());
    }

    #[test]
    fn command_verbs_reach_their_actions() {
        use crate::navigation::{Action, command::Entry, command::parse};
        assert_eq!(
            parse(":reverse 6f"),
            Ok(Entry::Action(Action::Reverse {
                length: DurationInput::parse("6f").unwrap(),
                bounce: false
            }))
        );
        assert_eq!(
            parse("ping-pong"),
            Ok(Entry::Action(Action::Reverse {
                length: DurationInput::parse(DEFAULT_PING_PONG).unwrap(),
                bounce: true
            }))
        );
        assert_eq!(
            parse("tail effect=delay"),
            Ok(Entry::Action(Action::Tail {
                length: None,
                effect: TailEffect::Delay
            }))
        );
        assert_eq!(
            parse("bleep 880Hz"),
            Ok(Entry::Action(Action::Bleep {
                frequency_hz: 880,
                level_millidecibels: -10_000
            }))
        );
        assert_eq!(parse("lift"), Ok(Entry::Action(Action::Lift)));
        assert!(parse("lift now").is_err());
        assert!(matches!(
            parse("caption Are we done? at=top"),
            Ok(Entry::Caption(_))
        ));
    }

    #[test]
    fn bleeps_default_to_one_kilohertz_at_minus_ten_decibels() {
        assert_eq!(parse_bleep(&[]), Ok((1_000, -10_000)));
        assert_eq!(parse_bleep(&["880Hz", "level=-6dB"]), Ok((880, -6_000)));
        assert_eq!(
            parse_bleep(&["level=-3.5dB", "freq=440"]),
            Ok((440, -3_500))
        );
        for bad in [&["10Hz"][..], &["level=3dB"], &["loud"], &["440", "880"]] {
            assert!(parse_bleep(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn tail_takes_an_optional_length_and_effect_in_any_order() {
        assert_eq!(parse_tail(&[]), Ok((None, TailEffect::Reverb)));
        assert_eq!(
            parse_tail(&["400ms", "effect=reverb"]),
            Ok((
                Some(DurationInput::parse("400ms").unwrap()),
                TailEffect::Reverb
            ))
        );
        assert_eq!(
            parse_tail(&["effect=delay", "12f"]),
            Ok((
                Some(DurationInput::parse("12f").unwrap()),
                TailEffect::Delay
            ))
        );
        for bad in [
            &["effect=chorus"][..],
            &["1s", "2s"],
            &["effect=delay", "effect=reverb"],
            &["mix=50%"],
        ] {
            assert!(parse_tail(bad).is_err(), "{bad:?}");
        }
    }
}
