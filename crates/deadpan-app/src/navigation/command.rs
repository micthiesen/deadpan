//! The currently supported native command vocabulary, independent of widgets.

use deadpan_core::FrameDuration;

use super::{Action, BeatEdit, duration::DurationInput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeChoice {
    All,
    Play(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Action(Action),
    Group {
        label: String,
    },
    Source,
    Sequence,
    Help,
    Renders,
    /// `:relink`: locate a missing Original by choosing its file.
    Relink,
    /// `:recovery`: show what opening this project recovered.
    Recovery,
    /// `:models`: open the model pack panel.
    Models,
    Splice,
    Slip(i64),
    Trim(super::trim::TrimInput),
    RoomTone,
    HoldSilence,
    Gain(Option<deadpan_core::GainDb>),
    /// `:gain +=3dB` / `:gain -=3dB`: change the trim by a signed amount.
    GainStep(i32),
    GainMute,
    /// `:saturate 12dB` sets the selected beat's drive; `:saturate off`
    /// removes the stage.
    Saturate(Option<deadpan_core::Saturation>),
    Scope(ScopeChoice),
    /// Tenths of one percent, independent of authored or export gain.
    Monitor(u16),
    /// `:proxies on|off|retry`: automatic seek proxies.
    Proxies(ProxyCommand),
    AuditionContext {
        lead: DurationInput,
        follow: DurationInput,
    },
    /// Track a saved target by id or label, or the one the selected beat
    /// follows. Without `through_shots` tracking stops at the first stored
    /// shot boundary and needs a shot analysis.
    Track {
        target: Option<String>,
        through_shots: bool,
    },
    TrackCancel,
    /// `:zoom` and `:creep`: authored framing on the selected beat.
    Zoom(super::zoom::ZoomInput),
    /// `:gag-inspect NAME [parameters]`: list a recipe's expansion without
    /// applying it.
    GagInspect(super::gag::GagInput),
    /// `:select role=audio|video|linked`: the media role the next Visual
    /// delete acts on.
    SelectRole(deadpan_core::MediaRole),
    /// `:delete role=audio|video`: delete one role over the Visual range.
    DeleteRole(deadpan_core::MediaRole),
    /// `:recipe-save a`: keep the selected group, with its parts and
    /// attachments, as local recipe a in this project.
    RecipeSave(char),
    /// `:recipe a`: insert a copy of local recipe a at the cursor.
    Recipe(char),
    /// `:recipe-inspect a`: list what local recipe a inserts.
    RecipeInspect(char),
    /// `:caption`: a line of text over the selected beat or Edit range.
    Caption(super::caption::CaptionInput),
    Empty,
}

/// `:track [ID or label] [through-shots]`. Labels may contain spaces.
fn track<'a>(words: impl Iterator<Item = &'a str>) -> Result<Entry, String> {
    let mut words: Vec<&str> = words.collect();
    let through_shots = words
        .last()
        .is_some_and(|last| last.eq_ignore_ascii_case("through-shots"));
    if through_shots {
        words.pop();
    }
    if words
        .iter()
        .any(|word| word.eq_ignore_ascii_case("through-shots"))
    {
        return Err("Put through-shots last: :track target-1 through-shots.".into());
    }
    let target = (!words.is_empty()).then(|| words.join(" "));
    Ok(Entry::Track {
        target,
        through_shots,
    })
}

const HOLD_USAGE: &str = "Use :hold 0.5s or :hold 12f to insert a silent freeze at the cursor; video=black inserts black picture instead (audio=silence).";

/// `:hold DURATION [video=freeze|black] [audio=silence]`.
fn hold<'a>(mut words: impl Iterator<Item = &'a str>) -> Result<Entry, String> {
    let duration = DurationInput::parse(words.next().ok_or(HOLD_USAGE)?)?;
    let mut video = None;
    let mut audio = false;
    for word in words {
        match word.split_once('=') {
            Some(("video", value)) if video.is_none() => {
                video = Some(match value {
                    "freeze" => false,
                    "black" => true,
                    "ai" => return Err(
                        "Insert the pause first, then request an AI picture with ,a or :generate."
                            .into(),
                    ),
                    _ => return Err("video is freeze or black.".into()),
                });
            }
            Some(("audio", value)) if !audio => {
                if value != "silence" {
                    return Err("A new pause is silent (audio=silence); choose room tone afterwards with :room-tone.".into());
                }
                audio = true;
            }
            Some(("video" | "audio", _)) => return Err("Each parameter can be given once.".into()),
            _ => return Err(HOLD_USAGE.into()),
        }
    }
    Ok(Entry::Action(Action::Edit(if video == Some(true) {
        BeatEdit::InsertBlack(duration)
    } else {
        BeatEdit::InsertHold(duration)
    })))
}

pub fn parse(input: &str) -> Result<Entry, String> {
    let input = input.trim();
    let input = input.strip_prefix(':').unwrap_or(input).trim_start();
    let mut words = input.split_whitespace();
    let Some(verb) = words.next() else {
        return Ok(Entry::Empty);
    };
    if verb.eq_ignore_ascii_case("group") {
        return super::group::parse(&input[verb.len()..]);
    }
    if verb.eq_ignore_ascii_case("zoom") {
        return super::zoom::parse_zoom(&input[verb.len()..]).map(Entry::Zoom);
    }
    if verb.eq_ignore_ascii_case("creep") {
        return super::zoom::parse_creep(&input[verb.len()..]).map(Entry::Zoom);
    }
    if verb.eq_ignore_ascii_case("caption") {
        return super::caption::parse(&input[verb.len()..]).map(Entry::Caption);
    }
    let verb = verb.to_ascii_lowercase();
    if verb == "scope" {
        let choice = match words.next() {
            Some("all") => ScopeChoice::All,
            Some("play") => {
                let play = words
                    .next()
                    .filter(|value| {
                        !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                    })
                    .and_then(|value| value.parse::<u32>().ok())
                    .filter(|value| *value > 0)
                    .ok_or("Use :scope play N with a positive one-based play number.")?;
                ScopeChoice::Play(play)
            }
            _ => return Err("Use :scope all or :scope play N inside a Repeat.".into()),
        };
        if words.next().is_some() {
            return Err("Extra arguments are not supported by this command.".into());
        }
        return Ok(Entry::Scope(choice));
    }
    if verb == "audition-context" {
        return audition_context(words);
    }
    if verb == "track" {
        return track(words);
    }
    if verb == "track-cancel" {
        return match words.next() {
            None => Ok(Entry::TrackCancel),
            Some(_) => Err("Use :track-cancel without arguments.".into()),
        };
    }
    if verb == "trim" {
        return super::trim::parse(words).map(Entry::Trim);
    }
    if verb == "roll" {
        return super::trim::parse_roll(words).map(Entry::Trim);
    }
    if verb == "pitch" {
        return super::retime::parse_pitch(words)
            .map(|semitones| Entry::Action(Action::Edit(BeatEdit::Pitch(semitones))));
    }
    if verb == "retime" || verb == "wrap-retime" {
        return super::retime::parse(words, verb == "wrap-retime")
            .map(|input| Entry::Action(Action::Edit(BeatEdit::Retime(input))));
    }
    if verb == "record" || verb == "macro" {
        let register = words
            .next()
            .filter(|name| name.len() == 1 && name.as_bytes()[0].is_ascii_alphabetic())
            .ok_or("A macro name must be exactly one ASCII letter, a–z or A–Z.")?;
        let register = char::from(register.as_bytes()[0]).to_ascii_lowercase();
        let action = if verb == "record" {
            Action::MacroRecord(register)
        } else {
            let count = words.next().unwrap_or("1");
            let count = (!count.is_empty() && count.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| count.parse::<u32>().ok())
                .flatten()
                .filter(|count| *count > 0)
                .ok_or("Use :macro a [count] with a positive count in 1..4294967295.")?;
            Action::MacroExecute { register, count }
        };
        if words.next().is_some() {
            return Err("Extra arguments are not supported by this command.".into());
        }
        return Ok(Entry::Action(action));
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
    if verb == "gag" {
        let arguments: Vec<&str> = words.collect();
        return super::gag::parse(&arguments).map(|input| Entry::Action(Action::Gag(input)));
    }
    if verb == "gag-inspect" {
        let arguments: Vec<&str> = words.collect();
        return super::gag::parse(&arguments).map(Entry::GagInspect);
    }
    if verb == "cutaway" {
        let arguments: Vec<&str> = words.collect();
        return super::cutaway::parse(&arguments)
            .map(|input| Entry::Action(Action::Edit(BeatEdit::Cutaway(input))));
    }
    if verb == "hold" {
        return hold(words);
    }
    if verb == "reverse" || verb == "ping-pong" {
        let arguments: Vec<&str> = words.collect();
        let bounce = verb == "ping-pong";
        return super::hold_effects::parse_reverse(&arguments, bounce)
            .map(|length| Entry::Action(Action::Reverse { length, bounce }));
    }
    if verb == "jcut" || verb == "lcut" {
        let arguments: Vec<&str> = words.collect();
        let kind = if verb == "jcut" {
            deadpan_core::SplitEditKind::J
        } else {
            deadpan_core::SplitEditKind::L
        };
        return super::hold_effects::parse_split(&arguments)
            .map(|length| Entry::Action(Action::SplitEdit { kind, length }));
    }
    if verb == "bleep" {
        let arguments: Vec<&str> = words.collect();
        return super::hold_effects::parse_bleep(&arguments).map(
            |(frequency_hz, level_millidecibels)| {
                Entry::Action(Action::Bleep {
                    frequency_hz,
                    level_millidecibels,
                })
            },
        );
    }
    if verb == "tail" {
        let arguments: Vec<&str> = words.collect();
        return super::hold_effects::parse_tail(&arguments)
            .map(|(length, effect)| Entry::Action(Action::Tail { length, effect }));
    }
    if verb == "repeat" && words.clone().any(|word| word.starts_with("role=")) {
        return role_repeat(words);
    }
    if verb == "repeat" {
        let arguments: Vec<&str> = words.clone().collect();
        if let Some(input) = super::escalation::parse(&arguments)? {
            return Ok(Entry::Action(Action::Edit(BeatEdit::Escalate(input))));
        }
    }
    let argument = words.next();
    if words.next().is_some() {
        return Err("Extra arguments are not supported by this command.".into());
    }
    if let Some(role) = argument.and_then(|argument| argument.strip_prefix("role="))
        && (verb == "select" || verb == "delete")
    {
        let role = match role {
            "audio" => deadpan_core::MediaRole::Audio,
            "video" => deadpan_core::MediaRole::Video,
            "linked" if verb == "select" => deadpan_core::MediaRole::Linked,
            _ => {
                return Err(if verb == "select" {
                    "Use :select role=audio, role=video or role=linked."
                } else {
                    "Use :delete role=audio or :delete role=video over a Visual range; plain :delete removes linked time."
                }
                .into());
            }
        };
        return Ok(if verb == "select" {
            Entry::SelectRole(role)
        } else {
            Entry::DeleteRole(role)
        });
    }
    let action = match verb.as_str() {
        "register" => {
            let name = argument
                .filter(|value| {
                    value.len() == 1
                        && (value.as_bytes()[0].is_ascii_alphabetic() || *value == "\"")
                })
                .ok_or("Use :register a–z or :register \" to select the unnamed register.")?;
            return Ok(Entry::Action(Action::SelectRegister(
                char::from(name.as_bytes()[0]).to_ascii_lowercase(),
            )));
        }
        "recipe-save" | "recipe" | "recipe-inspect" => {
            let name = argument
                .filter(|value| value.len() == 1 && value.as_bytes()[0].is_ascii_alphabetic())
                .ok_or("Use :recipe-save a, :recipe a or :recipe-inspect a with one register letter a–z.")?;
            let name = char::from(name.as_bytes()[0]).to_ascii_lowercase();
            return Ok(match verb.as_str() {
                "recipe-save" => Entry::RecipeSave(name),
                "recipe" => Entry::Recipe(name),
                _ => Entry::RecipeInspect(name),
            });
        }
        "framing-save" => {
            let name = argument
                .filter(|value| value.len() == 1 && value.as_bytes()[0].is_ascii_alphabetic())
                .ok_or("Use :framing-save a to keep the selected beat's framing in register a; apply it with @a.")?;
            return Ok(Entry::Action(Action::SaveFraming(
                char::from(name.as_bytes()[0]).to_ascii_lowercase(),
            )));
        }
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
        "generate" | "generate-ai" => Action::Ai(super::AiAction::Generate),
        "cancel-ai" => Action::Ai(super::AiAction::Cancel),
        "preview-ai" => Action::Ai(super::AiAction::Preview),
        "accept-ai" => Action::Ai(super::AiAction::Accept),
        "discard-ai" => Action::Ai(super::AiAction::Discard),
        "record-stop" => Action::MacroStop,
        "record-cancel" => Action::MacroCancel,
        "delete-frames" => {
            let frames = argument.unwrap_or("1f")
                .strip_suffix('f')
                .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|frames| *frames > 0)
                .ok_or("Use :delete-frames Nf with 1..4294967295 whole project frames, for example 12f.")?;
            return Ok(Entry::Action(Action::DeleteFrames(frames)));
        }
        "jump-back" => Action::JumpHistory { forward: false },
        "jump-forward" => Action::JumpHistory { forward: true },
        "gain" => {
            let Some(argument) = argument else {
                return Ok(Entry::Gain(None));
            };
            if let Some((negative, amount)) = argument
                .strip_prefix("+=")
                .map(|amount| (false, amount))
                .or_else(|| argument.strip_prefix("-=").map(|amount| (true, amount)))
            {
                let step = crate::gain::parse_db(strip_db(amount))?.millidecibels();
                return Ok(Entry::GainStep(if negative { -step } else { step }));
            }
            return crate::gain::parse_db(strip_db(argument)).map(|value| Entry::Gain(Some(value)));
        }
        "saturate" => {
            const USAGE: &str = "Use :saturate 12dB to drive the selected beat into a soft clipper (0 to 24 dB), or :saturate off to remove it.";
            let argument = argument.ok_or(USAGE)?;
            if argument.eq_ignore_ascii_case("off") {
                return Ok(Entry::Saturate(None));
            }
            let drive = crate::gain::parse_db(strip_db(argument)).map_err(|_| USAGE.to_owned())?;
            return deadpan_core::Saturation::new(drive)
                .map(|stage| Entry::Saturate(Some(stage)))
                .map_err(|_| USAGE.to_owned());
        }
        "gain-mute" if argument.is_none() => return Ok(Entry::GainMute),
        "monitor" => return monitor(argument).map(Entry::Monitor),
        "proxies" => {
            return match argument {
                Some("on") => Ok(Entry::Proxies(ProxyCommand::On)),
                Some("off") => Ok(Entry::Proxies(ProxyCommand::Off)),
                Some("retry") => Ok(Entry::Proxies(ProxyCommand::Retry)),
                _ => Err("Use :proxies on, :proxies off or :proxies retry.".into()),
            };
        }
        "sound-place" | "sounds" | "sound-at" | "sound-gain" | "sound-edges" | "sound-delete"
        | "sound-allow" | "sound-silence" | "sound-cut" => {
            return super::sound::parse(&verb, argument)
                .map(|sound| Entry::Action(Action::Sound(sound)));
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
        "audio-lag" => {
            return audio_lag(argument).map(|(earlier, amount)| {
                Entry::Action(Action::Edit(BeatEdit::AudioLag { earlier, amount }))
            });
        }
        "insert" => Action::Insert,
        "play" => Action::Playback,
        "audition" => Action::Audition,
        "enter" => Action::EnterGroup,
        "parent" => Action::LeaveGroup,
        "ungroup" => Action::Ungroup,
        "select" => Action::VisualMoment,
        "yank" => Action::CopyMoment,
        "paste" => Action::PasteMoment { before: false },
        "paste-before" => Action::PasteMoment { before: true },
        "split" => Action::Edit(BeatEdit::Split),
        "lift" => Action::Lift,
        "delete" => Action::Edit(BeatEdit::Delete),
        "undo" => Action::Undo,
        "redo" => Action::Redo,
        "new" => Action::New,
        "youtube" | "new-url" => Action::NewFromUrl,
        "open" => Action::Open,
        "import" => Action::Import,
        "render" => Action::Render,
        "source" | "sequence" | "help" | "registers" | "renders" | "splice" | "room-tone"
        | "hold-silence" | "relink" | "recovery" | "models"
            if argument.is_none() =>
        {
            return Ok(match verb.as_str() {
                "source" => Entry::Source,
                "sequence" => Entry::Sequence,
                "renders" => Entry::Renders,
                "relink" => Entry::Relink,
                "recovery" => Entry::Recovery,
                "models" => Entry::Models,
                "splice" => Entry::Splice,
                "room-tone" => Entry::RoomTone,
                "hold-silence" => Entry::HoldSilence,
                _ => Entry::Help,
            });
        }
        "source" | "sequence" | "help" | "registers" | "renders" | "splice" | "room-tone"
        | "hold-silence" | "relink" | "recovery" | "models" => {
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

const ROLE_REPEAT_USAGE: &str = "Use :repeat 3 role=audio or :repeat 3 role=video over a Visual range inside one beat; add overflow=trim to cut repeats at the beat's end.";

/// `:repeat [N] role=audio|video [overflow=trim]`.
fn role_repeat<'a>(words: impl Iterator<Item = &'a str>) -> Result<Entry, String> {
    let mut plays = None;
    let mut role = None;
    let mut trim = None;
    for word in words {
        match word.split_once('=') {
            Some(("role", value)) if role.is_none() => {
                role = Some(match value {
                    "audio" => deadpan_core::MediaRole::Audio,
                    "video" => deadpan_core::MediaRole::Video,
                    _ => return Err(ROLE_REPEAT_USAGE.into()),
                });
            }
            Some(("overflow", "trim")) if trim.is_none() => trim = Some(true),
            Some(("extend", "hold")) => {
                return Err("extend=hold is not available yet: a role repeat cannot add picture time. Use overflow=trim, fewer plays, or a linked :repeat.".into());
            }
            None if plays.is_none()
                && !word.is_empty()
                && word.bytes().all(|b| b.is_ascii_digit()) =>
            {
                plays = Some(
                    word.parse::<u32>()
                        .ok()
                        .and_then(std::num::NonZeroU32::new)
                        .ok_or(ROLE_REPEAT_USAGE)?,
                );
            }
            _ => return Err(ROLE_REPEAT_USAGE.into()),
        }
    }
    Ok(Entry::Action(Action::RoleRepeat {
        role: role.ok_or(ROLE_REPEAT_USAGE)?,
        plays: plays.unwrap_or(std::num::NonZeroU32::new(2).expect("constant plays")),
        trim: trim.unwrap_or(false),
    }))
}

/// Typed decibel values accept an optional `dB` unit (specification §6.4).
fn strip_db(value: &str) -> &str {
    value
        .strip_suffix("dB")
        .or_else(|| value.strip_suffix("db"))
        .or_else(|| value.strip_suffix("DB"))
        .unwrap_or(value)
}

const AUDIO_LAG_USAGE: &str = "Use :audio-lag +80ms (sound later), :audio-lag -2f (sound earlier) or :audio-lag 0 to realign the selected beat's sound with its picture.";

/// `:audio-lag [+|-]DURATION` or `0`: the signed sound offset of a Source.
fn audio_lag(argument: Option<&str>) -> Result<(bool, Option<DurationInput>), String> {
    let value = argument.ok_or(AUDIO_LAG_USAGE)?;
    if value == "0" {
        return Ok((false, None));
    }
    let (earlier, magnitude) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value.strip_prefix('+').unwrap_or(value)),
    };
    let amount =
        DurationInput::parse(magnitude).map_err(|error| format!("{error} {AUDIO_LAG_USAGE}"))?;
    Ok((earlier, Some(amount)))
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

/// Automatic seek proxies: on, off (remembered per user), or retry a build
/// that failed for this Original.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyCommand {
    On,
    Off,
    Retry,
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
mod track_tests {
    use super::*;

    #[test]
    fn track_commands_name_a_target_and_explicit_shot_crossing() {
        assert_eq!(
            parse(":track"),
            Ok(Entry::Track {
                target: None,
                through_shots: false
            })
        );
        assert_eq!(
            parse("track target-2"),
            Ok(Entry::Track {
                target: Some("target-2".into()),
                through_shots: false
            })
        );
        assert_eq!(
            parse(":track Target 2 through-shots"),
            Ok(Entry::Track {
                target: Some("Target 2".into()),
                through_shots: true
            })
        );
        assert_eq!(
            parse(":track through-shots"),
            Ok(Entry::Track {
                target: None,
                through_shots: true
            })
        );
        assert!(parse(":track through-shots target-1").is_err());
        assert_eq!(parse(":track-cancel"), Ok(Entry::TrackCancel));
        assert!(parse(":track-cancel now").is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_play_commands_are_explicit_and_bounded() {
        assert_eq!(parse(":scope all"), Ok(Entry::Scope(ScopeChoice::All)));
        assert_eq!(
            parse(":scope play 4294967295"),
            Ok(Entry::Scope(ScopeChoice::Play(u32::MAX)))
        );
        for input in [
            "scope",
            "scope play",
            "scope play 0",
            "scope play -1",
            "scope play 1.5",
            "scope play 4294967296",
            "scope all 1",
            "scope play 2 extra",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
        assert_eq!(parse("play"), Ok(Entry::Action(Action::Playback)));
    }

    #[test]
    fn macro_commands_require_named_registers_and_positive_bounded_counts() {
        for name in 'a'..='z' {
            for entered in [name, name.to_ascii_uppercase()] {
                assert_eq!(
                    parse(&format!(":record {entered}")),
                    Ok(Entry::Action(Action::MacroRecord(name)))
                );
                assert_eq!(
                    parse(&format!(":macro {entered}")),
                    Ok(Entry::Action(Action::MacroExecute {
                        register: name,
                        count: 1
                    }))
                );
                assert_eq!(
                    parse(&format!(":macro {entered} 4294967295")),
                    Ok(Entry::Action(Action::MacroExecute {
                        register: name,
                        count: u32::MAX
                    }))
                );
            }
        }
        assert_eq!(parse(":record-stop"), Ok(Entry::Action(Action::MacroStop)));
        assert_eq!(
            parse(":record-cancel"),
            Ok(Entry::Action(Action::MacroCancel))
        );
        for input in [
            "record",
            "record ab",
            "record a 1",
            "record a a",
            "record \"",
            "record é",
            "record 0",
            "record-stop a",
            "record-cancel a",
            "macro",
            "macro \"",
            "macro ab",
            "macro a 0",
            "macro a -1",
            "macro a +1",
            "macro a 1.0",
            "macro a 4294967296",
            "macro a 1 2",
            "macro é",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn register_commands_select_only_one_ascii_letter_or_the_unnamed_slot() {
        for name in 'a'..='z' {
            for entered in [name, name.to_ascii_uppercase()] {
                assert_eq!(
                    parse(&format!(":register {entered}")),
                    Ok(Entry::Action(Action::SelectRegister(name)))
                );
            }
        }
        assert_eq!(
            parse(" :REGISTER \" "),
            Ok(Entry::Action(Action::SelectRegister('"')))
        );
        assert_eq!(parse(":registers"), Ok(Entry::Help));
        for input in [
            "register",
            "register ab",
            "register a b",
            "register 0",
            "register 'a",
            "register é",
            "register 🐈",
            "registers a",
            "register a\"",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn frame_cut_command_requires_positive_typed_frames() {
        for (input, frames) in [
            ("delete-frames", 1),
            (":delete-frames 1f", 1),
            ("delete-frames 12f", 12),
            ("delete-frames 4294967295f", u32::MAX),
        ] {
            assert_eq!(
                parse(input),
                Ok(Entry::Action(Action::DeleteFrames(frames)))
            );
        }
        for input in [
            "delete-frames 0f",
            "delete-frames 12",
            "delete-frames -1f",
            "delete-frames +1f",
            "delete-frames 1.5f",
            "delete-frames 1s",
            "delete-frames f",
            "delete-frames 4294967296f",
            "delete-frames 1f extra",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

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
        assert_eq!(
            parse(":gain +6dB"),
            Ok(Entry::Gain(Some(deadpan_core::GainDb::new(6000).unwrap())))
        );
        assert_eq!(parse(":gain +=3dB"), Ok(Entry::GainStep(3000)));
        assert_eq!(parse(":gain -=1.5"), Ok(Entry::GainStep(-1500)));
        assert_eq!(
            parse(":saturate 12dB"),
            Ok(Entry::Saturate(Some(
                deadpan_core::Saturation::new(deadpan_core::GainDb::new(12_000).unwrap()).unwrap()
            )))
        );
        assert_eq!(parse(":saturate off"), Ok(Entry::Saturate(None)));
        for input in [
            "saturate",
            "saturate -3dB",
            "saturate 24.001",
            "gain +=x",
            "gain 6dBx",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
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
    fn role_repeats_name_their_role_and_overflow_explicitly() {
        use deadpan_core::MediaRole;
        let three = std::num::NonZeroU32::new(3).unwrap();
        assert_eq!(
            parse(":repeat 3 role=audio"),
            Ok(Entry::Action(Action::RoleRepeat {
                role: MediaRole::Audio,
                plays: three,
                trim: false
            }))
        );
        assert_eq!(
            parse(":repeat role=video overflow=trim 3"),
            Ok(Entry::Action(Action::RoleRepeat {
                role: MediaRole::Video,
                plays: three,
                trim: true
            }))
        );
        for input in [
            "repeat 3 role=both",
            "repeat 3 role=audio extend=hold",
            "repeat 0 role=audio",
            "repeat 3 role=audio overflow=wrap",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn roles_are_selected_and_deleted_explicitly() {
        use deadpan_core::MediaRole;
        assert_eq!(
            parse(":select role=audio"),
            Ok(Entry::SelectRole(MediaRole::Audio))
        );
        assert_eq!(
            parse(":select role=linked"),
            Ok(Entry::SelectRole(MediaRole::Linked))
        );
        assert_eq!(
            parse(":delete role=video"),
            Ok(Entry::DeleteRole(MediaRole::Video))
        );
        assert_eq!(parse(":select"), Ok(Entry::Action(Action::VisualMoment)));
        for input in [
            "select role=both",
            "delete role=linked",
            "delete role=audio extra",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn local_recipes_name_one_register() {
        assert_eq!(parse(":recipe-save A"), Ok(Entry::RecipeSave('a')));
        assert_eq!(parse(":recipe b"), Ok(Entry::Recipe('b')));
        assert_eq!(parse(":recipe-inspect c"), Ok(Entry::RecipeInspect('c')));
        for input in ["recipe", "recipe ab", "recipe-save 1", "recipe a b"] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn roll_opens_trim_with_its_whole_frame_amount() {
        let Ok(Entry::Trim(input)) = parse(":roll +2f") else {
            panic!("roll parses as Trim")
        };
        assert_eq!(input.control, deadpan_core::SourceTrimControl::Roll);
        assert_eq!(input.intent.roll_frames, 2);
        assert_eq!(
            parse(":roll +2f"),
            parse("trim edge=roll delta=+2f mode=ripple")
        );
        for input in ["roll", "roll 2s", "roll +2f extra"] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn split_edits_take_one_optional_length() {
        use deadpan_core::SplitEditKind;
        assert_eq!(
            parse(":jcut"),
            Ok(Entry::Action(Action::SplitEdit {
                kind: SplitEditKind::J,
                length: DurationInput::parse("6f").unwrap(),
            }))
        );
        assert_eq!(
            parse(":lcut 200ms"),
            Ok(Entry::Action(Action::SplitEdit {
                kind: SplitEditKind::L,
                length: DurationInput::parse("200ms").unwrap(),
            }))
        );
        for input in ["jcut 0f", "lcut 6f extra", "jcut soon"] {
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
            assert_eq!(parse(input), Ok(expected.clone()));
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
    fn proxies_command_accepts_on_off_and_retry_only() {
        assert_eq!(parse("proxies on"), Ok(Entry::Proxies(ProxyCommand::On)));
        assert_eq!(parse("proxies off"), Ok(Entry::Proxies(ProxyCommand::Off)));
        assert_eq!(
            parse("proxies retry"),
            Ok(Entry::Proxies(ProxyCommand::Retry))
        );
        for input in ["proxies", "proxies maybe", "proxies on now"] {
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
            "hold 1s video=black video=black",
            "hold 1s audio=room-tone",
            "hold video=black",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
        assert_eq!(
            parse("hold 12f video=freeze audio=silence"),
            Ok(Entry::Action(Action::Edit(BeatEdit::InsertHold(
                DurationInput::parse("12f").unwrap()
            ))))
        );
        assert_eq!(
            parse("hold 0.5s video=black"),
            Ok(Entry::Action(Action::Edit(BeatEdit::InsertBlack(
                DurationInput::parse("0.5s").unwrap()
            ))))
        );
    }

    #[test]
    fn zoom_and_creep_commands_parse_through_the_command_line() {
        assert!(matches!(
            parse(":zoom 1.35 target=current curve=step"),
            Ok(Entry::Zoom(input)) if input.target == super::super::zoom::TargetChoice::Current
        ));
        assert!(matches!(
            parse("zoom 1.35 target=face:2 curve=step"),
            Ok(Entry::Zoom(input)) if input.target == super::super::zoom::TargetChoice::Face(2)
        ));
        assert!(parse("zoom 1.35 target=face:0").is_err());
        assert!(matches!(parse("creep from=1 to=1.4"), Ok(Entry::Zoom(_))));
        assert!(matches!(parse("ZOOM off"), Ok(Entry::Zoom(_))));
        assert!(parse("zoom").is_err());
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
            "repeat 3 gap=120",
            "repeat 3 volume=2",
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
    fn framing_presets_save_to_one_named_register() {
        assert_eq!(
            parse(":framing-save S"),
            Ok(Entry::Action(Action::SaveFraming('s')))
        );
        for input in [
            "framing-save",
            "framing-save ab",
            "framing-save \"",
            "framing-save 1",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn audio_lag_takes_one_signed_duration_or_zero() {
        assert_eq!(
            parse(":audio-lag +80ms"),
            Ok(Entry::Action(Action::Edit(BeatEdit::AudioLag {
                earlier: false,
                amount: Some(DurationInput::parse("80ms").unwrap())
            })))
        );
        assert_eq!(
            parse("audio-lag -2f"),
            Ok(Entry::Action(Action::Edit(BeatEdit::AudioLag {
                earlier: true,
                amount: Some(DurationInput::parse("2f").unwrap())
            })))
        );
        assert_eq!(
            parse("audio-lag 0"),
            Ok(Entry::Action(Action::Edit(BeatEdit::AudioLag {
                earlier: false,
                amount: None
            })))
        );
        for input in [
            "audio-lag",
            "audio-lag 80",
            "audio-lag +-2f",
            "audio-lag 2f 3f",
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
            (":youtube", Entry::Action(Action::NewFromUrl)),
            ("new-url", Entry::Action(Action::NewFromUrl)),
            ("open", Entry::Action(Action::Open)),
            ("import", Entry::Action(Action::Import)),
            (":render", Entry::Action(Action::Render)),
            (":renders", Entry::Renders),
            (":relink", Entry::Relink),
            ("recovery", Entry::Recovery),
            (":models", Entry::Models),
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
            "relink /tmp/clip.mp4",
            "models ltx",
            "splice 12",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }
}
