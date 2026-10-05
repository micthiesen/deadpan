//! `:caption TEXT [at=bottom|top|center] [delay=12f] [reveal=3]` and
//! `:caption clear`: a line of text over the selected beat or the Edit range
//! inside it.

use std::num::NonZeroU32;

use deadpan_core::CaptionPlacement;

use super::duration::DurationInput;

pub const USAGE: &str = "Use :caption Are we done? to caption the selected beat (or the Edit range inside it); \"quotes\" keep words like at= in the text. at=bottom|top|center places it, delay=12f starts it later in the beat and reveal=3 waits for the third play of a Repeat, counted in its current play order. :caption clear removes captions there.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptionInput {
    Place {
        text: String,
        placement: CaptionPlacement,
        delay: Option<DurationInput>,
        reveal: Option<NonZeroU32>,
    },
    Clear,
}

/// Parse everything after the verb. Quoted text is kept exactly; unquoted
/// text is its words joined by single spaces, before any trailing options.
pub fn parse(arguments: &str) -> Result<CaptionInput, String> {
    let arguments = arguments.trim();
    if arguments == "clear" {
        return Ok(CaptionInput::Clear);
    }
    let (text, options): (String, Vec<&str>) = if let Some(rest) = arguments.strip_prefix('"') {
        let (text, options) = rest
            .split_once('"')
            .ok_or("Close the quoted caption text with \".")?;
        (text.to_owned(), options.split_whitespace().collect())
    } else {
        let words: Vec<&str> = arguments.split_whitespace().collect();
        let first_option = words
            .iter()
            .rposition(|word| !is_option(word))
            .map_or(0, |last_text| last_text + 1);
        (
            words[..first_option].join(" "),
            words[first_option..].to_vec(),
        )
    };
    if text.trim().is_empty() {
        return Err(USAGE.into());
    }
    deadpan_core::validate_caption_text(&text).map_err(|error| error.message)?;
    let mut placement = None;
    let mut delay = None;
    let mut reveal = None;
    for option in options {
        match option.split_once('=') {
            Some(("at", value)) if placement.is_none() => {
                placement = Some(match value {
                    "bottom" => CaptionPlacement::Bottom,
                    "top" => CaptionPlacement::Top,
                    "center" | "middle" => CaptionPlacement::Center,
                    _ => return Err("at is bottom, top or center.".into()),
                });
            }
            Some(("delay", value)) if delay.is_none() => {
                delay = Some(DurationInput::parse(value)?);
            }
            Some(("reveal", value)) if reveal.is_none() => {
                reveal = Some(
                    value
                        .parse::<u32>()
                        .ok()
                        .and_then(NonZeroU32::new)
                        .ok_or("reveal is the first play to show it, from 1.")?,
                );
            }
            Some(("at" | "delay" | "reveal", _)) => {
                return Err("Each option can be given once.".into());
            }
            _ => return Err(USAGE.into()),
        }
    }
    Ok(CaptionInput::Place {
        text,
        placement: placement.unwrap_or_default(),
        delay,
        reveal,
    })
}

fn is_option(word: &str) -> bool {
    word.split_once('=')
        .is_some_and(|(key, _)| matches!(key, "at" | "delay" | "reveal"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caption_text_keeps_its_words_and_takes_trailing_options() {
        assert_eq!(
            parse("Are we done?  at=top delay=12f reveal=3"),
            Ok(CaptionInput::Place {
                text: "Are we done?".into(),
                placement: CaptionPlacement::Top,
                delay: Some(DurationInput::parse("12f").unwrap()),
                reveal: NonZeroU32::new(3),
            })
        );
        assert_eq!(
            parse("\"x=1  means  at=top\" at=center"),
            Ok(CaptionInput::Place {
                text: "x=1  means  at=top".into(),
                placement: CaptionPlacement::Center,
                delay: None,
                reveal: None,
            })
        );
        assert_eq!(parse(" clear "), Ok(CaptionInput::Clear));
        for bad in [
            "",
            "at=top",
            "\"unclosed",
            "hello at=left",
            "hello reveal=0",
            "hello at=top at=bottom",
            "\"  \"",
            "\"hello\" font=serif",
            &"x".repeat(121),
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }
}
