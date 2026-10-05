//! `:gag long-answer|escalator|non-sequitur|one-more-time|nothing-happens|are-we-done
//! [parameters]`: apply a built-in recipe at the cursor or selected beat as
//! one Undo.

use std::num::NonZeroU32;

use deadpan_core::{ExactRatio, GainDb, RegisterName};

use super::duration::DurationInput;

pub const USAGE: &str = "Use :gag long-answer [pause=1.5s] [creep=1.35], :gag escalator [plays=3] [gain-step=3dB] [zoom-step=0.08], :gag non-sequitur [register=r], :gag one-more-time [plays=3] [gap=500ms] [shorten=200ms], :gag nothing-happens [register=r] [tone=1s] [silence=1s] or :gag are-we-done [register=r] [pause=1.5s].";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GagInput {
    LongAnswer {
        pause: DurationInput,
        scale: ExactRatio,
    },
    Escalator {
        plays: NonZeroU32,
        gain_step: GainDb,
        zoom_step: ExactRatio,
    },
    NonSequitur {
        register: RegisterName,
    },
    OneMoreTime {
        plays: NonZeroU32,
        gap: DurationInput,
        shorten: DurationInput,
    },
    NothingHappens {
        tone: DurationInput,
        silence: DurationInput,
        register: RegisterName,
    },
    AreWeDone {
        pause: DurationInput,
        register: RegisterName,
    },
}

impl GagInput {
    /// The pinned recipe, resolving a non-millisecond pause at `rate`.
    pub fn recipe(self, rate: deadpan_core::FrameRate) -> Result<deadpan_core::GagRecipe, String> {
        let version = deadpan_core::GAG_RECIPE_VERSION;
        Ok(match self {
            Self::LongAnswer { pause, scale } => deadpan_core::GagRecipe::LongAnswer {
                version,
                pause: pause.pause_length(rate)?,
                scale,
            },
            Self::Escalator {
                plays,
                gain_step,
                zoom_step,
            } => deadpan_core::GagRecipe::Escalator {
                version,
                plays,
                gain_step,
                zoom_step,
            },
            Self::NonSequitur { register } => {
                deadpan_core::GagRecipe::NonSequitur { version, register }
            }
            Self::OneMoreTime {
                plays,
                gap,
                shorten,
            } => deadpan_core::GagRecipe::OneMoreTime {
                version,
                plays,
                gap: gap.pause_length(rate)?,
                shorten: shorten.pause_length(rate)?,
            },
            Self::NothingHappens {
                tone,
                silence,
                register,
            } => deadpan_core::GagRecipe::NothingHappens {
                version,
                tone: tone.pause_length(rate)?,
                silence: silence.pause_length(rate)?,
                register,
            },
            Self::AreWeDone { pause, register } => deadpan_core::GagRecipe::AreWeDone {
                version,
                pause: pause.pause_length(rate)?,
                register,
            },
        })
    }
}

pub fn parse(arguments: &[&str]) -> Result<GagInput, String> {
    let (name, parameters) = arguments.split_first().ok_or(USAGE)?;
    let mut values = std::collections::BTreeMap::new();
    for parameter in parameters {
        let (key, value) = parameter.split_once('=').ok_or(USAGE)?;
        if values.insert(key, value).is_some() {
            return Err(format!("{key} is given twice."));
        }
    }
    let mut take = |key: &str| values.remove(key);
    let input = match *name {
        "long-answer" => GagInput::LongAnswer {
            pause: take("pause")
                .map_or(Ok(DurationInput::half_seconds(3)), DurationInput::parse)?,
            scale: match take("creep") {
                Some(value) => decimal(value)?,
                None => ExactRatio::new(27, 20).expect("constant ratio"),
            },
        },
        "escalator" => {
            // Reuse the :repeat step grammar for the two steps.
            let steps: Vec<String> = ["gain-step", "zoom-step"]
                .iter()
                .filter_map(|key| take(key).map(|value| format!("{key}={value}")))
                .collect();
            let steps: Vec<&str> = steps.iter().map(String::as_str).collect();
            let escalation = super::escalation::parse(&steps)?;
            let plays = take("plays")
                .map(|value| {
                    value
                        .parse::<u32>()
                        .ok()
                        .and_then(NonZeroU32::new)
                        .ok_or("plays must be a positive integer.")
                })
                .transpose()?
                .unwrap_or(NonZeroU32::new(3).expect("constant plays"));
            GagInput::Escalator {
                plays,
                gain_step: escalation
                    .and_then(|input| input.gain_step)
                    .unwrap_or(GainDb::new(3_000).expect("constant gain")),
                zoom_step: match escalation.and_then(|input| input.zoom_step) {
                    Some(Some(step)) => step,
                    Some(None) => ExactRatio::ZERO,
                    None => deadpan_core::quantize_zoom_step(
                        ExactRatio::new(8, 100).expect("constant ratio"),
                    )
                    .expect("constant step"),
                },
            }
        }
        "non-sequitur" => GagInput::NonSequitur {
            register: register(take("register"))?,
        },
        "one-more-time" => GagInput::OneMoreTime {
            plays: take("plays")
                .map(|value| {
                    value
                        .parse::<u32>()
                        .ok()
                        .and_then(NonZeroU32::new)
                        .filter(|plays| plays.get() >= 2)
                        .ok_or("plays must be an integer of at least 2.")
                })
                .transpose()?
                .unwrap_or(NonZeroU32::new(3).expect("constant plays")),
            gap: take("gap").map_or(Ok(DurationInput::half_seconds(1)), DurationInput::parse)?,
            shorten: take("shorten").map_or(
                Ok(DurationInput::Seconds(
                    ExactRatio::new(1, 5).expect("constant ratio"),
                )),
                DurationInput::parse,
            )?,
        },
        "nothing-happens" => GagInput::NothingHappens {
            register: register(take("register"))?,
            tone: take("tone").map_or(Ok(DurationInput::half_seconds(2)), DurationInput::parse)?,
            silence: take("silence")
                .map_or(Ok(DurationInput::half_seconds(2)), DurationInput::parse)?,
        },
        "are-we-done" => GagInput::AreWeDone {
            register: register(take("register"))?,
            pause: take("pause")
                .map_or(Ok(DurationInput::half_seconds(3)), DurationInput::parse)?,
        },
        other => return Err(format!("Unknown gag {other}. {USAGE}")),
    };
    if let Some(key) = values.keys().next() {
        return Err(format!("{key} does not apply to this gag. {USAGE}"));
    }
    Ok(input)
}

/// `register=r`: one register letter, or the unnamed register when absent.
fn register(value: Option<&str>) -> Result<RegisterName, String> {
    value.map_or(Ok(RegisterName::unnamed()), |value| {
        let mut chars = value.chars();
        match (chars.next(), chars.next()) {
            (Some(name), None) => RegisterName::new(name).map_err(|error| error.message),
            _ => Err("register= takes one letter.".into()),
        }
    })
}

pub(super) fn decimal(value: &str) -> Result<ExactRatio, String> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty() && fraction.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || whole.len() + fraction.len() > 12
    {
        return Err(format!("{value} is not a positive decimal number."));
    }
    let numerator: i128 = format!("{whole}{fraction}")
        .parse()
        .map_err(|_| format!("{value} is not a number."))?;
    ExactRatio::new(numerator, 10_i128.pow(fraction.len() as u32))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gags_parse_with_defaults_and_refuse_strangers() {
        assert_eq!(
            parse(&["long-answer"]).unwrap(),
            GagInput::LongAnswer {
                pause: DurationInput::half_seconds(3),
                scale: ExactRatio::new(27, 20).unwrap()
            }
        );
        assert!(matches!(
            parse(&["long-answer", "pause=12f", "creep=1.5"]).unwrap(),
            GagInput::LongAnswer { scale, .. } if scale == ExactRatio::new(3, 2).unwrap()
        ));
        assert!(matches!(
            parse(&["escalator", "plays=4", "gain-step=2dB"]).unwrap(),
            GagInput::Escalator { plays, gain_step, .. }
                if plays.get() == 4 && gain_step.millidecibels() == 2000
        ));
        assert_eq!(
            parse(&["non-sequitur", "register=r"]).unwrap(),
            GagInput::NonSequitur {
                register: RegisterName::new('r').unwrap()
            }
        );
        assert_eq!(
            parse(&["one-more-time"]).unwrap(),
            GagInput::OneMoreTime {
                plays: NonZeroU32::new(3).unwrap(),
                gap: DurationInput::parse("500ms").unwrap(),
                shorten: DurationInput::parse("200ms").unwrap(),
            }
        );
        let rate = deadpan_core::FrameRate::new(30, 1).unwrap();
        assert!(matches!(
            parse(&["one-more-time", "plays=4", "gap=12f", "shorten=3f"])
                .unwrap()
                .recipe(rate)
                .unwrap(),
            deadpan_core::GagRecipe::OneMoreTime { plays, .. } if plays.get() == 4
        ));
        assert_eq!(
            parse(&["nothing-happens", "register=t", "tone=12f"]).unwrap(),
            GagInput::NothingHappens {
                tone: DurationInput::parse("12f").unwrap(),
                silence: DurationInput::parse("1s").unwrap(),
                register: RegisterName::new('t').unwrap(),
            }
        );
        assert_eq!(
            parse(&["are-we-done", "register=r", "pause=20f"]).unwrap(),
            GagInput::AreWeDone {
                pause: DurationInput::parse("20f").unwrap(),
                register: RegisterName::new('r').unwrap(),
            }
        );
        assert!(matches!(
            parse(&["are-we-done"]).unwrap().recipe(rate).unwrap(),
            deadpan_core::GagRecipe::AreWeDone { pause, .. }
                if pause == deadpan_core::PauseLength::Milliseconds {
                    milliseconds: NonZeroU32::new(1500).unwrap()
                }
        ));
        for bad in [
            vec![],
            vec!["shrug"],
            vec!["are-we-done", "tone=1s"],
            vec!["one-more-time", "plays=1"],
            vec!["one-more-time", "creep=1.2"],
            vec!["nothing-happens", "register=tt"],
            vec!["long-answer", "plays=3"],
            vec!["long-answer", "pause=1s", "pause=2s"],
            vec!["escalator", "plays=0"],
            vec!["non-sequitur", "register=R"],
        ] {
            assert!(parse(&bad).is_err(), "{bad:?}");
        }
    }
}
