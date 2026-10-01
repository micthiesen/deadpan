//! The currently supported native command vocabulary, independent of widgets.

use deadpan_core::FrameDuration;

use super::{Action, BeatEdit, duration::DurationInput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Action(Action),
    Source,
    Sequence,
    Help,
    Renders,
    Splice,
    Slip(i64),
    RoomTone,
    HoldSilence,
    Gain(Option<deadpan_core::GainDb>),
    GainMute,
    /// Tenths of one percent, independent of authored or export gain.
    Monitor(u16),
    AuditionContext {
        lead: DurationInput,
        follow: DurationInput,
    },
    Empty,
}

pub fn parse(input: &str) -> Result<Entry, String> {
    let input = input.trim();
    let mut words = input.strip_prefix(':').unwrap_or(input).split_whitespace();
    let Some(verb) = words.next() else {
        return Ok(Entry::Empty);
    };
    let verb = verb.to_ascii_lowercase();
    if verb == "audition-context" {
        return audition_context(words);
    }
    if verb == "retime" || verb == "wrap-retime" {
        return super::retime::parse(words, verb == "wrap-retime")
            .map(|input| Entry::Action(Action::Edit(BeatEdit::Retime(input))));
    }
    if verb == "slip" {
        let amount = words
            .next()
            .ok_or("Use :slip +5f or :slip -3f with one signed whole project-frame amount.")?;
        if words.next().is_some() {
            return Err(
                "Use :slip +5f or :slip -3f with one signed whole project-frame amount.".into(),
            );
        }
        return super::slip::parse_frames(amount).map(Entry::Slip);
    }
    let argument = words.next();
    if words.next().is_some() {
        return Err("Extra arguments are not supported by this command.".into());
    }
    let action = match verb.as_str() {
        "mark" | "jump" | "unmark" => {
            let letter = argument
                .filter(|value| value.len() == 1 && value.as_bytes()[0].is_ascii_alphabetic())
                .ok_or("A mark name must be exactly one ASCII letter, a–z or A–Z.")?
                .as_bytes()[0];
            let letter = char::from(letter);
            return Ok(Entry::Action(match verb.as_str() {
                "mark" => Action::SetMark(letter),
                "jump" => Action::JumpMark(letter),
                _ => Action::DeleteMark(letter),
            }));
        }
        "marks" => Action::Marks,
        "jump-back" => Action::JumpHistory { forward: false },
        "jump-forward" => Action::JumpHistory { forward: true },
        "gain" => {
            return argument
                .map(crate::gain::parse_db)
                .transpose()
                .map(Entry::Gain);
        }
        "gain-mute" if argument.is_none() => return Ok(Entry::GainMute),
        "monitor" => return monitor(argument).map(Entry::Monitor),
        "sound-place" | "sounds" | "sound-at" | "sound-gain" | "sound-edges" | "sound-delete"
        | "sound-allow" | "sound-silence" => {
            return super::sound::parse(&verb, argument)
                .map(|sound| Entry::Action(Action::Sound(sound)));
        }
        "hold" => {
            let duration =
                DurationInput::parse(argument.ok_or(
                    "Use :hold 0.5s or :hold 12f to insert a silent freeze at the cursor.",
                )?)?;
            return Ok(Entry::Action(Action::Edit(BeatEdit::InsertHold(duration))));
        }
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
        "play" => Action::Playback,
        "audition" => Action::Audition,
        "enter" => Action::EnterGroup,
        "parent" => Action::LeaveGroup,
        "select" => Action::VisualMoment,
        "yank" => Action::CopyMoment,
        "paste" => Action::PasteMoment { before: false },
        "paste-before" => Action::PasteMoment { before: true },
        "split" => Action::Edit(BeatEdit::Split),
        "delete" => Action::Edit(BeatEdit::Delete),
        "undo" => Action::Undo,
        "redo" => Action::Redo,
        "new" => Action::New,
        "open" => Action::Open,
        "import" => Action::Import,
        "render" => Action::Render,
        "source" | "sequence" | "help" | "renders" | "splice" | "room-tone" | "hold-silence"
            if argument.is_none() =>
        {
            return Ok(match verb.as_str() {
                "source" => Entry::Source,
                "sequence" => Entry::Sequence,
                "renders" => Entry::Renders,
                "splice" => Entry::Splice,
                "room-tone" => Entry::RoomTone,
                "hold-silence" => Entry::HoldSilence,
                _ => Entry::Help,
            });
        }
        "source" | "sequence" | "help" | "renders" | "splice" | "room-tone" | "hold-silence" => {
            return Err("This command takes no arguments.".into());
        }
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

fn audition_context<'a>(arguments: impl Iterator<Item = &'a str>) -> Result<Entry, String> {
    let invalid = || {
        "Use :audition-context lead=500ms follow=750ms with both durations exactly once.".to_owned()
    };
    let mut lead = None;
    let mut follow = None;
    for argument in arguments {
        let (name, value) = argument.split_once('=').ok_or_else(invalid)?;
        let slot = match name {
            "lead" => &mut lead,
            "follow" => &mut follow,
            _ => return Err(invalid()),
        };
        if slot.is_some() {
            return Err(invalid());
        }
        *slot = Some(DurationInput::parse(value)?);
    }
    Ok(Entry::AuditionContext {
        lead: lead.ok_or_else(invalid)?,
        follow: follow.ok_or_else(invalid)?,
    })
}

fn monitor(argument: Option<&str>) -> Result<u16, String> {
    let invalid =
        || "Use :monitor 25% with a value from 0 to 100%, optionally one decimal place.".to_owned();
    let input = argument.ok_or_else(invalid)?;
    let input = input.strip_suffix('%').unwrap_or(input);
    let (whole, fraction) = input.split_once('.').unwrap_or((input, "0"));
    if whole.is_empty()
        || whole.len() > 3
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() != 1
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid());
    }
    let tenths = whole.parse::<u16>().map_err(|_| invalid())? * 10
        + fraction.parse::<u16>().map_err(|_| invalid())?;
    if tenths > 1000 {
        return Err(invalid());
    }
    Ok(tenths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slip_command_requires_one_exact_frame_amount() {
        assert_eq!(parse(":slip +5f"), Ok(Entry::Slip(5)));
        assert_eq!(parse("slip -3f"), Ok(Entry::Slip(-3)));
        assert_eq!(parse("slip 0f"), Ok(Entry::Slip(0)));
        for input in [
            "slip",
            "slip 2",
            "slip 1.5f",
            "slip 3s",
            "slip +2f extra",
            "slip 9223372036854775808f",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn gain_commands_distinguish_draft_absolute_trim_and_true_mute() {
        assert_eq!(parse(":gain"), Ok(Entry::Gain(None)));
        assert_eq!(
            parse("gain -4.125"),
            Ok(Entry::Gain(Some(deadpan_core::GainDb::new(-4125).unwrap())))
        );
        assert_eq!(parse("gain-mute"), Ok(Entry::GainMute));
        for input in [
            "gain 3 extra",
            "gain NaN",
            "gain -96.001",
            "gain 24.001",
            "gain-mute true",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn audition_context_requires_two_distinct_named_exact_durations() {
        let expected = Entry::AuditionContext {
            lead: DurationInput::parse("500ms").unwrap(),
            follow: DurationInput::parse("750ms").unwrap(),
        };
        for input in [
            ":audition-context lead=500ms follow=750ms",
            "AUDITION-CONTEXT follow=750ms lead=500ms",
        ] {
            assert_eq!(parse(input), Ok(expected));
        }
        assert_eq!(
            parse("audition-context lead=0f follow=01:02.500"),
            Ok(Entry::AuditionContext {
                lead: DurationInput::Frames(FrameDuration::ZERO),
                follow: DurationInput::parse("01:02.500").unwrap(),
            })
        );
        for input in [
            "audition-context",
            "audition-context lead=500ms",
            "audition-context follow=750ms",
            "audition-context lead=500ms lead=750ms",
            "audition-context follow=500ms follow=750ms",
            "audition-context lead=500ms follow=750ms lead=1s",
            "audition-context lead=500ms follow=750ms extra=1s",
            "audition-context lead=500ms follow=750ms extra",
            "audition-context lead=500ms tail=750ms",
            "audition-context Lead=500ms follow=750ms",
            "audition-context lead=500 follow=750ms",
            "audition-context lead=500ms follow=-1s",
            "audition-context lead=500ms follow=",
            "audition-context lead=500ms follow=750MS",
            "audition-context lead=500ms follow=750ms=1s",
            "audition-context lead =500ms follow=750ms",
            "play 2",
            "audition 2",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn monitor_command_preserves_bounded_percentages_without_authored_edits() {
        for (input, expected) in [
            ("monitor 0", 0),
            ("monitor 100%", 1000),
            ("monitor 12.5%", 125),
            ("monitor 25", 250),
        ] {
            assert_eq!(parse(input), Ok(Entry::Monitor(expected)));
        }
        for input in [
            "monitor",
            "monitor NaN",
            "monitor inf",
            "monitor -1",
            "monitor 101",
            "monitor 100.1%",
            "monitor 1.25",
            "monitor 12.%",
            "monitor 12.5% more",
            "monitor 65535",
            "monitor 1e2",
            "monitor +1",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn pause_command_keeps_exact_units_and_zero_without_committing() {
        for input in ["12f", "0f", "250ms", "1.5s", "01:02.500"] {
            assert_eq!(
                parse(&format!("hold {input}")),
                Ok(Entry::Action(Action::Edit(BeatEdit::InsertHold(
                    DurationInput::parse(input).unwrap()
                ))))
            );
        }
        for input in [
            "hold",
            "hold -1f",
            "hold 12",
            "hold 12f extra",
            "hold 1s video=ai",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn repeat_setter_and_explicit_wrapper_remain_distinct() {
        assert_eq!(parse("enter"), Ok(Entry::Action(Action::EnterGroup)));
        assert_eq!(parse(":parent"), Ok(Entry::Action(Action::LeaveGroup)));
        assert_eq!(parse("select"), Ok(Entry::Action(Action::VisualMoment)));
        assert_eq!(parse("yank"), Ok(Entry::Action(Action::CopyMoment)));
        assert_eq!(
            parse("paste"),
            Ok(Entry::Action(Action::PasteMoment { before: false }))
        );
        assert_eq!(
            parse("paste-before"),
            Ok(Entry::Action(Action::PasteMoment { before: true }))
        );
        assert!(parse("paste 2").is_err());
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
            "enter anything",
            "parent 2",
            "help extra",
            "source extra",
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
            ("play", Entry::Action(Action::Playback)),
            (":audition", Entry::Action(Action::Audition)),
            ("split", Entry::Action(Action::Edit(BeatEdit::Split))),
            ("undo", Entry::Action(Action::Undo)),
            ("redo", Entry::Action(Action::Redo)),
            ("new", Entry::Action(Action::New)),
            ("open", Entry::Action(Action::Open)),
            ("import", Entry::Action(Action::Import)),
            (":render", Entry::Action(Action::Render)),
            (":renders", Entry::Renders),
            (":splice", Entry::Splice),
            ("source", Entry::Source),
            ("sequence", Entry::Sequence),
            ("help", Entry::Help),
            ("room-tone", Entry::RoomTone),
            (":HOLD-SILENCE", Entry::HoldSilence),
            (" : ", Entry::Empty),
        ] {
            assert_eq!(parse(input), Ok(expected));
        }
        for input in [
            "room-tone auto",
            "room-tone 12f",
            "hold-silence all",
            "renders current",
            "splice 12",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }
}
