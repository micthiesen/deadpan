//! The currently supported native command vocabulary, independent of widgets.

use deadpan_core::FrameDuration;

use super::{Action, BeatEdit};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Action(Action),
    Source,
    Sequence,
    Help,
    Empty,
}

pub fn parse(input: &str) -> Result<Entry, String> {
    let input = input.trim();
    let mut words = input.strip_prefix(':').unwrap_or(input).split_whitespace();
    let Some(verb) = words.next() else {
        return Ok(Entry::Empty);
    };
    let verb = verb.to_ascii_lowercase();
    let argument = words.next();
    if words.next().is_some() {
        return Err("Extra arguments are not supported by this command.".into());
    }
    let action = match verb.as_str() {
        "repeat" | "wrap-repeat" => {
            let count = argument
                .ok_or("Use :repeat N or :wrap-repeat N with a positive total play count.")?;
            if !count.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err("Total plays must be a positive integer.".into());
            }
            let plays = count
                .parse::<u32>()
                .map_err(|_| "Total plays must fit in 1..4294967295.")?;
            if plays == 0 {
                return Err("Total plays must be positive.".into());
            }
            return Ok(Entry::Action(Action::Edit(if verb == "repeat" {
                BeatEdit::Repeat(plays)
            } else {
                BeatEdit::WrapRepeat(plays)
            })));
        }
        "hold-duration" => {
            let frames = argument
                .and_then(|value| value.strip_suffix('f'))
                .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
                .ok_or("Use :hold-duration Nf with a positive whole number of project frames, for example 11f. Seconds and milliseconds are not supported yet.")?;
            let frames = frames
                .parse::<i64>()
                .map_err(|_| "Hold duration exceeds the supported frame range.")?;
            if frames == 0 {
                return Err("Hold duration must be positive; no edit was made.".into());
            }
            let duration = FrameDuration::new(frames).map_err(|error| error.to_string())?;
            return Ok(Entry::Action(Action::Edit(BeatEdit::HoldDuration(
                duration,
            ))));
        }
        "insert" => Action::Insert,
        "split" => Action::Edit(BeatEdit::Split),
        "delete" => Action::Edit(BeatEdit::Delete),
        "undo" => Action::Undo,
        "redo" => Action::Redo,
        "new" => Action::New,
        "open" => Action::Open,
        "import" => Action::Import,
        "source" | "sequence" | "help" if argument.is_none() => {
            return Ok(match verb.as_str() {
                "source" => Entry::Source,
                "sequence" => Entry::Sequence,
                _ => Entry::Help,
            });
        }
        "source" | "sequence" | "help" => return Err("This command takes no arguments.".into()),
        _ => {
            return Err(format!(
                "Unknown command: {verb}. Use :help for available commands."
            ));
        }
    };
    if argument.is_some() {
        return Err("This command takes no arguments.".into());
    }
    Ok(Entry::Action(action))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_setter_and_explicit_wrapper_remain_distinct() {
        assert_eq!(
            parse(" :RePeAt 3 "),
            Ok(Entry::Action(Action::Edit(BeatEdit::Repeat(3))))
        );
        assert_eq!(
            parse("wrap-repeat 1"),
            Ok(Entry::Action(Action::Edit(BeatEdit::WrapRepeat(1))))
        );
        assert_eq!(
            parse("delete"),
            Ok(Entry::Action(Action::Edit(BeatEdit::Delete)))
        );
        assert_eq!(
            parse("repeat 4294967295"),
            Ok(Entry::Action(Action::Edit(BeatEdit::Repeat(u32::MAX))))
        );
    }

    #[test]
    fn invalid_counts_and_unimplemented_parameters_never_become_edits() {
        for input in [
            "repeat",
            "repeat 0",
            "repeat -1",
            "repeat +2",
            "repeat 2.5",
            "repeat 4294967296",
            "repeat 3 gap=120ms",
            "wrap-repeat 2 gain-step=3dB",
            "delete 2",
            "split 12f",
            "undo anything",
            "help extra",
            "source extra",
            "hold 11f",
            "::delete",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn hold_duration_requires_exact_frame_units_without_lowercasing_arguments() {
        assert_eq!(
            parse("HOLD-DURATION 11f"),
            Ok(Entry::Action(Action::Edit(BeatEdit::HoldDuration(
                FrameDuration::new(11).unwrap()
            ))))
        );
        for input in [
            "hold-duration",
            "hold-duration 11",
            "hold-duration 0f",
            "hold-duration -1f",
            "hold-duration 1.5f",
            "hold-duration 1s",
            "hold-duration 100ms",
            "hold-duration 11F",
            "hold-duration 9223372036854775808f",
            "hold-duration 11f extra",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn existing_commands_keep_their_explicit_actions() {
        for (input, expected) in [
            ("insert", Entry::Action(Action::Insert)),
            ("split", Entry::Action(Action::Edit(BeatEdit::Split))),
            ("undo", Entry::Action(Action::Undo)),
            ("redo", Entry::Action(Action::Redo)),
            ("new", Entry::Action(Action::New)),
            ("open", Entry::Action(Action::Open)),
            ("import", Entry::Action(Action::Import)),
            ("source", Entry::Source),
            ("sequence", Entry::Sequence),
            ("help", Entry::Help),
            (" : ", Entry::Empty),
        ] {
            assert_eq!(parse(input), Ok(expected));
        }
    }
}
