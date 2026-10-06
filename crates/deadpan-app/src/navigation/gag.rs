//! `:gag long-answer|escalator|non-sequitur|one-more-time|nothing-happens|are-we-done
//! [parameters]`: apply a built-in recipe at the cursor or selected beat as
//! one Undo.

use std::num::NonZeroU32;

use deadpan_core::{ExactRatio, GainDb, RegisterName};

use super::duration::DurationInput;

pub const USAGE: &str = "Use :gag long-answer [pause=1.5s] [creep=1.35], :gag escalator [plays=3] [gain-step=3dB] [zoom-step=0.08], :gag non-sequitur [register=r], :gag one-more-time [plays=3] [gap=500ms] [shorten=200ms] [vary=20% [seed=7]], :gag nothing-happens [register=r] [tone=1s] [silence=1s] or :gag are-we-done [register=r] [pause=1.5s].";

/// The built-in recipe names; saved presets use any other name.
pub const BUILT_IN: [&str; 6] = [
    "long-answer",
    "escalator",
    "non-sequitur",
    "one-more-time",
    "nothing-happens",
    "are-we-done",
];

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
        /// `vary=20%` with an optional `seed=N`; a new seed is chosen and
        /// pinned when none is given.
        variation: Option<(u8, Option<u64>)>,
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
                variation,
            } => deadpan_core::GagRecipe::OneMoreTime {
                version,
                plays,
                gap: gap.pause_length(rate)?,
                shorten: shorten.pause_length(rate)?,
                variation: variation.map(|(percent, seed)| deadpan_core::GagVariation {
                    percent,
                    seed: seed.unwrap_or_else(fresh_seed),
                }),
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

/// An authoring-time seed for a variation that names none. It is pinned in
/// the recipe, so playback, export and Undo/Redo never draw again.
fn fresh_seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    // Short enough to read and retype from the group label.
    (nanos as u64 ^ (nanos >> 64) as u64) % 1_000_000
}

/// `vary=20%`: a whole percentage from 1 to 50.
fn percent(value: &str) -> Result<u8, String> {
    value
        .strip_suffix('%')
        .unwrap_or(value)
        .parse::<u8>()
        .ok()
        .filter(|percent| (1..=deadpan_core::MAX_GAG_VARIATION_PERCENT).contains(percent))
        .ok_or_else(|| "vary= is a whole percentage from 1% to 50%.".to_owned())
}

/// The ordinary steps a recipe expands to, shared with the headless
/// `gag-inspect` command.
pub use deadpan_cli::gags::expansion_rows;

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
            variation: match (take("vary"), take("seed")) {
                (None, None) => None,
                (None, Some(_)) => {
                    return Err("seed= needs vary=, for example vary=20% seed=7.".into());
                }
                (Some(vary), seed) => Some((
                    percent(vary)?,
                    seed.map(|seed| {
                        seed.parse::<u64>()
                            .map_err(|_| "seed= is a whole number.".to_owned())
                    })
                    .transpose()?,
                )),
            },
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

/// The `:gag` name of a recipe.
pub fn slug(recipe: &deadpan_core::GagRecipe) -> &'static str {
    use deadpan_core::GagRecipe as G;
    match recipe {
        G::LongAnswer { .. } => "long-answer",
        G::Escalator { .. } => "escalator",
        G::NonSequitur { .. } => "non-sequitur",
        G::OneMoreTime { .. } => "one-more-time",
        G::NothingHappens { .. } => "nothing-happens",
        G::AreWeDone { .. } => "are-we-done",
    }
}

/// A recipe's parameters as `:gag` arguments, exactly when the value is a
/// terminating decimal: the inspector pre-fills `:gag-set` with them.
pub fn arguments(recipe: &deadpan_core::GagRecipe) -> Vec<(&'static str, String)> {
    use deadpan_core::{GagRecipe as G, PauseLength};
    let length = |length: &PauseLength| match length {
        PauseLength::Frames { frames } => format!("{frames}f"),
        PauseLength::Milliseconds { milliseconds } => format!("{milliseconds}ms"),
    };
    let register = |name: &RegisterName| name.as_char().to_string();
    match recipe {
        G::LongAnswer { pause, scale, .. } => {
            vec![("pause", length(pause)), ("creep", exact_decimal(*scale))]
        }
        G::Escalator {
            plays,
            gain_step,
            zoom_step,
            ..
        } => vec![
            ("plays", plays.to_string()),
            (
                "gain-step",
                format!(
                    "{}dB",
                    exact_decimal(
                        ExactRatio::new(i128::from(gain_step.millidecibels()), 1000)
                            .expect("nonzero denominator")
                    )
                ),
            ),
            ("zoom-step", exact_decimal(*zoom_step)),
        ],
        G::NonSequitur { register: name, .. } => vec![("register", register(name))],
        G::OneMoreTime {
            plays,
            gap,
            shorten,
            variation,
            ..
        } => {
            let mut values = vec![
                ("plays", plays.to_string()),
                ("gap", length(gap)),
                ("shorten", length(shorten)),
            ];
            if let Some(variation) = variation {
                values.push(("vary", format!("{}%", variation.percent)));
                values.push(("seed", variation.seed.to_string()));
            }
            values
        }
        G::NothingHappens {
            tone,
            silence,
            register: name,
            ..
        } => vec![
            ("register", register(name)),
            ("tone", length(tone)),
            ("silence", length(silence)),
        ],
        G::AreWeDone {
            pause,
            register: name,
            ..
        } => vec![("register", register(name)), ("pause", length(pause))],
    }
}

/// `value` written exactly in decimal when it terminates within nine
/// places, else to three places as the pinned label writes it.
fn exact_decimal(value: ExactRatio) -> String {
    let (numerator, denominator) = (value.numerator(), value.denominator());
    let Some(places) = (0..=9u32).find(|places| 10_i128.pow(*places) % denominator == 0) else {
        return format!("{:.3}", numerator as f64 / denominator as f64);
    };
    let scaled = numerator * (10_i128.pow(places) / denominator);
    let digits = scaled.unsigned_abs().to_string();
    let places = places as usize;
    let text = if places == 0 {
        digits
    } else {
        let padded = format!("{digits:0>width$}", width = places + 1);
        let (whole, fraction) = padded.split_at(padded.len() - places);
        format!("{whole}.{fraction}")
    };
    if scaled < 0 { format!("-{text}") } else { text }
}

/// The `:gag` argument keys a parameter is written with.
pub fn parameter_keys(parameter: deadpan_core::GagParameter) -> &'static [&'static str] {
    use deadpan_core::GagParameter as P;
    match parameter {
        P::Pause => &["pause"],
        P::Creep => &["creep"],
        P::Plays => &["plays"],
        P::GainStep => &["gain-step"],
        P::ZoomStep => &["zoom-step"],
        P::Gap => &["gap"],
        P::Shorten => &["shorten"],
        P::Variation => &["vary", "seed"],
        P::Tone => &["tone"],
        P::Silence => &["silence"],
        P::Register => &["register"],
    }
}

/// `:gag-set key=value …` on an inserted gag: its current recipe with only
/// the given parameters changed, each parsed by the `:gag` grammar, and the
/// changed parameters themselves (`vary=` and `seed=` are one).
pub fn merge(
    current: &deadpan_core::GagRecipe,
    parameters: &[&str],
    rate: deadpan_core::FrameRate,
) -> Result<(deadpan_core::GagRecipe, Vec<deadpan_core::GagParameter>), String> {
    let recipe = merge_recipe(current, parameters, rate)?;
    let mut changed = Vec::new();
    for parameter in parameters {
        use deadpan_core::GagParameter as P;
        let key = parameter.split_once('=').map_or(*parameter, |(key, _)| key);
        let named = match key {
            "pause" => P::Pause,
            "creep" => P::Creep,
            "plays" => P::Plays,
            "gain-step" => P::GainStep,
            "zoom-step" => P::ZoomStep,
            "gap" => P::Gap,
            "shorten" => P::Shorten,
            "vary" | "seed" => P::Variation,
            "tone" => P::Tone,
            "silence" => P::Silence,
            "register" => P::Register,
            other => return Err(format!("{other} is not a gag parameter. {USAGE}")),
        };
        if !changed.contains(&named) {
            changed.push(named);
        }
    }
    Ok((recipe, changed))
}

fn merge_recipe(
    current: &deadpan_core::GagRecipe,
    parameters: &[&str],
    rate: deadpan_core::FrameRate,
) -> Result<deadpan_core::GagRecipe, String> {
    use deadpan_core::GagRecipe as G;
    if parameters.is_empty() {
        return Err(format!(
            "Give the parameters to change, for example :gag-set {}.",
            arguments(current)
                .first()
                .map_or(String::new(), |(key, value)| format!("{key}={value}"))
        ));
    }
    let mut given = std::collections::BTreeSet::new();
    let mut words: Vec<String> = vec![slug(current).to_owned()];
    for parameter in parameters {
        let (key, _) = parameter.split_once('=').ok_or(USAGE)?;
        given.insert(key.to_owned());
        words.push((*parameter).to_owned());
    }
    // A new seed alone keeps the pinned percentage.
    if let G::OneMoreTime {
        variation: Some(variation),
        ..
    } = current
        && given.contains("seed")
        && !given.contains("vary")
    {
        words.push(format!("vary={}%", variation.percent));
        given.insert("vary".to_owned());
    }
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let changed = parse(&words)?.recipe(rate)?;
    let has = |key: &str| given.contains(key);
    Ok(match (current, changed) {
        (
            G::LongAnswer {
                version,
                pause,
                scale,
            },
            G::LongAnswer {
                pause: new_pause,
                scale: new_scale,
                ..
            },
        ) => G::LongAnswer {
            version: *version,
            pause: if has("pause") { new_pause } else { *pause },
            scale: if has("creep") { new_scale } else { *scale },
        },
        (
            G::Escalator {
                version,
                plays,
                gain_step,
                zoom_step,
            },
            G::Escalator {
                plays: new_plays,
                gain_step: new_gain,
                zoom_step: new_zoom,
                ..
            },
        ) => G::Escalator {
            version: *version,
            plays: if has("plays") { new_plays } else { *plays },
            gain_step: if has("gain-step") {
                new_gain
            } else {
                *gain_step
            },
            zoom_step: if has("zoom-step") {
                new_zoom
            } else {
                *zoom_step
            },
        },
        (G::NonSequitur { version, .. }, G::NonSequitur { register, .. }) => G::NonSequitur {
            version: *version,
            register,
        },
        (
            G::OneMoreTime {
                version,
                plays,
                gap,
                shorten,
                variation,
            },
            G::OneMoreTime {
                plays: new_plays,
                gap: new_gap,
                shorten: new_shorten,
                variation: new_variation,
                ..
            },
        ) => G::OneMoreTime {
            version: *version,
            plays: if has("plays") { new_plays } else { *plays },
            gap: if has("gap") { new_gap } else { *gap },
            shorten: if has("shorten") {
                new_shorten
            } else {
                *shorten
            },
            variation: if has("vary") {
                new_variation
            } else {
                *variation
            },
        },
        (
            G::NothingHappens {
                version,
                tone,
                silence,
                register,
            },
            G::NothingHappens {
                tone: new_tone,
                silence: new_silence,
                register: new_register,
                ..
            },
        ) => G::NothingHappens {
            version: *version,
            tone: if has("tone") { new_tone } else { *tone },
            silence: if has("silence") {
                new_silence
            } else {
                *silence
            },
            register: if has("register") {
                new_register
            } else {
                *register
            },
        },
        (
            G::AreWeDone {
                version,
                pause,
                register,
            },
            G::AreWeDone {
                pause: new_pause,
                register: new_register,
                ..
            },
        ) => G::AreWeDone {
            version: *version,
            pause: if has("pause") { new_pause } else { *pause },
            register: if has("register") {
                new_register
            } else {
                *register
            },
        },
        _ => return Err("The parameters belong to another gag.".into()),
    })
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
    fn gag_set_changes_only_the_given_parameters_and_prefills_exact_values() {
        let rate = deadpan_core::FrameRate::new(30, 1).unwrap();
        let current = parse(&[
            "one-more-time",
            "plays=4",
            "gap=12f",
            "shorten=3f",
            "vary=20%",
            "seed=7",
        ])
        .unwrap()
        .recipe(rate)
        .unwrap();
        let (merged, changed) = merge(&current, &["plays=5"], rate).unwrap();
        assert_eq!(changed, [deadpan_core::GagParameter::Plays]);
        assert!(matches!(
            merged,
            deadpan_core::GagRecipe::OneMoreTime {
                plays,
                gap: deadpan_core::PauseLength::Frames { frames },
                variation: Some(deadpan_core::GagVariation { percent: 20, seed: 7 }),
                ..
            } if plays.get() == 5 && frames.get() == 12
        ));
        // A new seed alone keeps the pinned percentage.
        assert!(matches!(
            merge(&current, &["seed=9"], rate).unwrap().0,
            deadpan_core::GagRecipe::OneMoreTime {
                variation: Some(deadpan_core::GagVariation {
                    percent: 20,
                    seed: 9
                }),
                ..
            }
        ));
        assert!(merge(&current, &[], rate).is_err());
        assert!(merge(&current, &["creep=1.5"], rate).is_err());
        let long = parse(&["long-answer", "pause=1500ms", "creep=1.35"])
            .unwrap()
            .recipe(rate)
            .unwrap();
        assert_eq!(
            arguments(&long),
            vec![("pause", "1500ms".to_owned()), ("creep", "1.35".to_owned())]
        );
        let escalator = parse(&["escalator", "gain-step=-1.5dB", "zoom-step=0.08"])
            .unwrap()
            .recipe(rate)
            .unwrap();
        assert_eq!(
            arguments(&escalator),
            vec![
                ("plays", "3".to_owned()),
                ("gain-step", "-1.5dB".to_owned()),
                ("zoom-step", "0.080".to_owned())
            ]
        );
        // The pre-filled arguments parse back to the same recipe.
        for recipe in [long, escalator, current] {
            let words: Vec<String> = std::iter::once(slug(&recipe).to_owned())
                .chain(
                    arguments(&recipe)
                        .into_iter()
                        .map(|(key, value)| format!("{key}={value}")),
                )
                .collect();
            let words: Vec<&str> = words.iter().map(String::as_str).collect();
            assert_eq!(
                parse(&words).unwrap().recipe(rate).unwrap(),
                recipe,
                "{words:?}"
            );
        }
    }

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
                variation: None,
            }
        );
        let varied = parse(&["one-more-time", "vary=20%", "seed=7"]).unwrap();
        assert!(matches!(
            varied,
            GagInput::OneMoreTime {
                variation: Some((20, Some(7))),
                ..
            }
        ));
        let rows = expansion_rows(
            &varied
                .recipe(deadpan_core::FrameRate::new(30, 1).unwrap())
                .unwrap(),
            false,
            deadpan_core::FrameRate::new(30, 1).unwrap(),
        )
        .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], "Repeat the selected beat for 3 plays");
        assert!(rows[1].starts_with("Silent freeze gaps after each play but the last: "));
        assert!(rows[2].contains("varied ±20% (seed 7)"), "{rows:?}");
        // A seed is drawn once and pinned when none is given.
        assert!(matches!(
            parse(&["one-more-time", "vary=10"])
                .unwrap()
                .recipe(deadpan_core::FrameRate::new(30, 1).unwrap())
                .unwrap(),
            deadpan_core::GagRecipe::OneMoreTime {
                variation: Some(deadpan_core::GagVariation { percent: 10, .. }),
                ..
            }
        ));
        for bad in [
            ["one-more-time", "seed=7"],
            ["one-more-time", "vary=0%"],
            ["one-more-time", "vary=51%"],
        ] {
            assert!(parse(&bad).is_err(), "{bad:?}");
        }
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
