use deadpan_jobs::{GenerationOptions, HoldInstructions};

use super::Entry;
use crate::navigation::{Action, AiAction};

pub(super) fn parse(argument: Option<&str>) -> Result<Entry, String> {
    let usage = "Use :generate [1-4] [motion=still|subtle|moderate] [text=guidance]. Put text last. Explicit controls replace the previous choices.";
    let mut rest = argument.unwrap_or("").trim();
    let mut variants = 1;
    let mut controls = GenerationOptions::default();
    let mut count_seen = false;
    let mut motion_seen = false;
    let mut explicit = false;
    while !rest.is_empty() {
        if let Some(text) = rest.strip_prefix("text=") {
            controls.instructions =
                Some(HoldInstructions::new(text).map_err(|error| error.to_string())?);
            explicit = true;
            break;
        }
        let (word, tail) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        if let Some(motion) = word.strip_prefix("motion=") {
            if motion_seen {
                return Err(usage.into());
            }
            controls.motion = motion.parse().map_err(str::to_owned)?;
            motion_seen = true;
            explicit = true;
        } else {
            if count_seen || explicit {
                return Err(usage.into());
            }
            variants = word
                .parse::<u8>()
                .ok()
                .filter(|count| (1..=crate::project::generation::MAX_VARIANTS).contains(count))
                .ok_or(usage)?;
            count_seen = true;
        }
        rest = tail.trim_start();
    }
    Ok(if explicit {
        Entry::Generate {
            variants,
            options: controls,
        }
    } else {
        Entry::Action(Action::Ai(AiAction::Generate { variants }))
    })
}

/// A complete, editable command. Omitting text clears old guidance when run.
pub(crate) fn command(options: &GenerationOptions) -> String {
    let mut command = format!("generate motion={}", options.motion.name());
    if let Some(text) = &options.instructions {
        command.push_str(" text=");
        command.push_str(text.as_str());
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_jobs::MotionAmount;

    #[test]
    fn controls_preserve_plain_text_and_round_trip_prefills() {
        let options = GenerationOptions {
            motion: MotionAmount::Subtle,
            instructions: Some(
                HoldInstructions::new("Keep eyes open, 目線 unchanged. motion=still is text")
                    .unwrap(),
            ),
        };
        assert_eq!(
            super::super::parse(&command(&options)).unwrap(),
            Entry::Generate {
                variants: 1,
                options: options.clone()
            }
        );
        assert_eq!(
            parse(Some("3 motion=subtle text=Keep hands still.")).unwrap(),
            Entry::Generate {
                variants: 3,
                options: GenerationOptions {
                    motion: MotionAmount::Subtle,
                    instructions: Some(HoldInstructions::new("Keep hands still.").unwrap())
                }
            }
        );
        assert_eq!(
            parse(Some("motion=still")).unwrap(),
            Entry::Generate {
                variants: 1,
                options: GenerationOptions::default()
            }
        );
        for text in [
            "0",
            "5",
            "motion=fast",
            "motion=still motion=subtle",
            "text=",
            "2 3",
            "motion=still 2",
            "text=a\tb",
        ] {
            assert!(parse(Some(text)).is_err(), "{text}");
        }
        assert!(parse(Some(&format!("text={}", "界".repeat(171)))).is_err());
        assert_eq!(
            parse(None).unwrap(),
            Entry::Action(Action::Ai(AiAction::Generate { variants: 1 }))
        );
    }
}
