//! Built-in gag recipes (specification §8.4). A recipe expands to ordinary
//! semantic instructions and ends by grouping its result under a label that
//! pins the recipe, its version and its parameters, so the inserted gag stays
//! editable as ordinary beats: change any part directly, or ungroup to detach
//! it. A new recipe version never changes an existing expansion.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::{
    EditError, EditErrorCode, ExactRatio, Framing, FramingCurve, FramingPose, GainDb, PauseLength,
    RegisterName, RepeatEscalation, SemanticInstruction, SemanticSelector, ZoomStep,
};

/// A recipe with its pinned version and parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "recipe", rename_all = "snake_case", deny_unknown_fields)]
pub enum GagRecipe {
    /// A silent pause at the cursor with a slow creep in on it.
    LongAnswer {
        version: u32,
        pause: PauseLength,
        /// Final scale of the creep.
        scale: ExactRatio,
    },
    /// The selected beat or range, repeated with growing gain and scale.
    Escalator {
        version: u32,
        plays: NonZeroU32,
        gain_step: GainDb,
        zoom_step: ExactRatio,
    },
    /// A hard cut to a registered moment and straight back, at the cursor.
    NonSequitur {
        version: u32,
        register: RegisterName,
    },
    /// The selected beat or range played `plays` times, with a silent freeze
    /// gap after each play but the last that is `shorten` shorter than the one
    /// before it. Both lengths share one unit.
    OneMoreTime {
        version: u32,
        plays: NonZeroU32,
        gap: PauseLength,
        shorten: PauseLength,
        /// Seeded irregularity of each gap (specification §8.4). The seed is
        /// pinned with the recipe and the resolved gaps are stored as the
        /// gap Holds' exact durations, so playback and export never draw
        /// new randomness.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        variation: Option<GagVariation>,
    },
    /// A held picture whose room tone, taken from the Original moment in
    /// `register`, cuts to true silence while the picture keeps holding.
    NothingHappens {
        version: u32,
        tone: PauseLength,
        silence: PauseLength,
        register: RegisterName,
    },
    /// A pause at the cursor where the sound before it hangs on in a reverb
    /// tail while the picture cuts to a reaction from the same Original in
    /// `register`.
    AreWeDone {
        version: u32,
        pause: PauseLength,
        register: RegisterName,
    },
}

/// One exposed recipe parameter, named as `:gag` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GagParameter {
    Pause,
    Creep,
    Plays,
    GainStep,
    ZoomStep,
    Gap,
    Shorten,
    /// `vary=` and its pinned `seed=`.
    Variation,
    Tone,
    Silence,
    Register,
}

/// Seeded, bounded irregularity: each value moves by at most `percent` of
/// itself, in a direction and amount drawn deterministically from `seed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GagVariation {
    pub percent: u8,
    pub seed: u64,
}

/// The largest variation a recipe value may receive, in percent.
pub const MAX_GAG_VARIATION_PERCENT: u8 = 50;

impl GagVariation {
    /// Draws are in thousandths of the full ±`percent` swing.
    const SCALE: i64 = 1_000;

    /// The `index`-th draw in [-SCALE, SCALE], from SplitMix64: a fixed,
    /// platform-independent sequence for every seed.
    fn draw(self, index: u32) -> i64 {
        let mut z = self
            .seed
            .wrapping_add(0x9E37_79B9_7F4A_7C15_u64.wrapping_mul(u64::from(index) + 1));
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z % (2 * Self::SCALE as u64 + 1)) as i64 - Self::SCALE
    }

    /// `value` moved by its `index`-th draw, rounded half away from zero to
    /// a whole unit and kept positive.
    pub fn vary(self, value: u32, index: u32) -> u32 {
        let swing = i64::from(value) * i64::from(self.percent) * self.draw(index);
        let denominator = 100 * Self::SCALE;
        let offset = (swing + denominator / 2 * swing.signum()) / denominator;
        u32::try_from((i64::from(value) + offset).max(1)).unwrap_or(u32::MAX)
    }

    fn validate(self) -> Result<(), EditError> {
        if self.percent == 0 || self.percent > MAX_GAG_VARIATION_PERCENT {
            return Err(EditError::new(
                EditErrorCode::InvalidCommand,
                "variation is 1 to 50 percent",
            ));
        }
        Ok(())
    }
}

/// The only version of each recipe so far.
pub const GAG_RECIPE_VERSION: u32 = 1;

impl GagRecipe {
    pub fn name(&self) -> &'static str {
        match self {
            Self::LongAnswer { .. } => "The Long Answer",
            Self::Escalator { .. } => "The Escalator",
            Self::NonSequitur { .. } => "The Non-Sequitur",
            Self::OneMoreTime { .. } => "One More Time",
            Self::NothingHappens { .. } => "Nothing Happens",
            Self::AreWeDone { .. } => "Are We Done?",
        }
    }

    pub fn version(&self) -> u32 {
        match self {
            Self::LongAnswer { version, .. }
            | Self::Escalator { version, .. }
            | Self::NonSequitur { version, .. }
            | Self::OneMoreTime { version, .. }
            | Self::NothingHappens { version, .. }
            | Self::AreWeDone { version, .. } => *version,
        }
    }

    /// The group label pinning recipe, version and parameters.
    pub fn label(&self) -> String {
        let decimal = |value: ExactRatio| value.numerator() as f64 / value.denominator() as f64;
        let parameters = match self {
            Self::LongAnswer { pause, scale, .. } => {
                format!("pause {}, creep to {:.3}×", length(*pause), decimal(*scale))
            }
            Self::OneMoreTime {
                plays,
                gap,
                shorten,
                variation,
                ..
            } => format!(
                "{plays} plays, gap {} shortening by {}{}",
                length(*gap),
                length(*shorten),
                variation.map_or(String::new(), |variation| format!(
                    ", varied ±{}% (seed {})",
                    variation.percent, variation.seed
                ))
            ),
            Self::NothingHappens {
                tone,
                silence,
                register,
                ..
            } => format!(
                "room tone {} from register {}, then {} silence",
                length(*tone),
                register.as_char(),
                length(*silence)
            ),
            Self::Escalator {
                plays,
                gain_step,
                zoom_step,
                ..
            } => format!(
                "{plays} plays, {:+} dB and {:+.3} scale per play",
                f64::from(gain_step.millidecibels()) / 1000.0,
                decimal(*zoom_step)
            ),
            Self::NonSequitur { register, .. } => format!("register {}", register.as_char()),
            Self::AreWeDone {
                pause, register, ..
            } => format!(
                "pause {} with a reverb tail, reaction from register {}",
                length(*pause),
                register.as_char()
            ),
        };
        format!("{} · v{} · {parameters}", self.name(), self.version())
    }

    /// The recipe a gag group's pinned label names, when the label is exactly
    /// one this recipe version writes. The label is the stored form of the
    /// recipe's version and parameters (specification §8.4), so any group
    /// whose label is exactly such a label, even one typed by hand, is
    /// treated as that gag; a renamed group or an unknown version names none.
    /// Lengths, plays, gains, seeds and registers come back exactly. The
    /// label writes a creep scale and a zoom step to three decimals: a value
    /// with at most three decimals (as `:gag` accepts by default) comes back
    /// exactly, a finer one as written there (a zoom step re-quantized to the
    /// framing grid).
    pub fn from_label(label: &str) -> Option<Self> {
        let mut parts = label.splitn(3, " · ");
        let (name, version, parameters) = (parts.next()?, parts.next()?, parts.next()?);
        let version: u32 = version.strip_prefix('v')?.parse().ok()?;
        let recipe = match name {
            "The Long Answer" => {
                let rest = parameters.strip_prefix("pause ")?;
                let (pause, scale) = rest.split_once(", creep to ")?;
                Self::LongAnswer {
                    version,
                    pause: parse_length(pause)?,
                    scale: parse_decimal(scale.strip_suffix('×')?)?,
                }
            }
            "One More Time" => {
                let (plays, rest) = parameters.split_once(" plays, gap ")?;
                let (gap, rest) = rest.split_once(" shortening by ")?;
                let (shorten, variation) = match rest.split_once(", varied ±") {
                    Some((shorten, varied)) => {
                        let (percent, seed) = varied.split_once("% (seed ")?;
                        (
                            shorten,
                            Some(GagVariation {
                                percent: percent.parse().ok()?,
                                seed: seed.strip_suffix(')')?.parse().ok()?,
                            }),
                        )
                    }
                    None => (rest, None),
                };
                Self::OneMoreTime {
                    version,
                    plays: plays.parse().ok()?,
                    gap: parse_length(gap)?,
                    shorten: parse_length(shorten)?,
                    variation,
                }
            }
            "Nothing Happens" => {
                let rest = parameters.strip_prefix("room tone ")?;
                let (tone, rest) = rest.split_once(" from register ")?;
                let (register, rest) = rest.split_once(", then ")?;
                Self::NothingHappens {
                    version,
                    tone: parse_length(tone)?,
                    silence: parse_length(rest.strip_suffix(" silence")?)?,
                    register: parse_register(register)?,
                }
            }
            "The Escalator" => {
                let (plays, rest) = parameters.split_once(" plays, ")?;
                let (gain, rest) = rest.split_once(" dB and ")?;
                let gain = parse_decimal(gain)?
                    .checked_mul(ExactRatio::integer(1000))
                    .ok()?;
                if gain.denominator() != 1 {
                    return None;
                }
                Self::Escalator {
                    version,
                    plays: plays.parse().ok()?,
                    gain_step: GainDb::new(i32::try_from(gain.numerator()).ok()?).ok()?,
                    // The step is stored on the framing grid; the label
                    // writes three places of it.
                    zoom_step: crate::quantize_zoom_step(parse_decimal(
                        rest.strip_suffix(" scale per play")?,
                    )?)
                    .ok()?,
                }
            }
            "The Non-Sequitur" => Self::NonSequitur {
                version,
                register: parse_register(parameters.strip_prefix("register ")?)?,
            },
            "Are We Done?" => {
                let rest = parameters.strip_prefix("pause ")?;
                let (pause, register) =
                    rest.split_once(" with a reverb tail, reaction from register ")?;
                Self::AreWeDone {
                    version,
                    pause: parse_length(pause)?,
                    register: parse_register(register)?,
                }
            }
            _ => return None,
        };
        (recipe.label() == label).then_some(recipe)
    }

    /// These parameters with `parameters` taken from `from`, a recipe of the
    /// same kind and version. A parameter the recipe does not have refuses.
    pub fn with_parameters(
        &self,
        from: &Self,
        parameters: &[GagParameter],
    ) -> Result<Self, EditError> {
        use GagParameter as P;
        let invalid = |message: &str| EditError::new(EditErrorCode::InvalidCommand, message);
        if !self.same_recipe(from) {
            return Err(invalid("the parameters belong to another gag recipe"));
        }
        let mut result = *self;
        for parameter in parameters {
            match (&mut result, from, parameter) {
                (Self::LongAnswer { pause, .. }, Self::LongAnswer { pause: new, .. }, P::Pause)
                | (Self::AreWeDone { pause, .. }, Self::AreWeDone { pause: new, .. }, P::Pause) => {
                    *pause = *new;
                }
                (Self::LongAnswer { scale, .. }, Self::LongAnswer { scale: new, .. }, P::Creep) => {
                    *scale = *new;
                }
                (Self::Escalator { plays, .. }, Self::Escalator { plays: new, .. }, P::Plays)
                | (
                    Self::OneMoreTime { plays, .. },
                    Self::OneMoreTime { plays: new, .. },
                    P::Plays,
                ) => {
                    *plays = *new;
                }
                (
                    Self::Escalator { gain_step, .. },
                    Self::Escalator { gain_step: new, .. },
                    P::GainStep,
                ) => *gain_step = *new,
                (
                    Self::Escalator { zoom_step, .. },
                    Self::Escalator { zoom_step: new, .. },
                    P::ZoomStep,
                ) => *zoom_step = *new,
                (Self::OneMoreTime { gap, .. }, Self::OneMoreTime { gap: new, .. }, P::Gap) => {
                    *gap = *new;
                }
                (
                    Self::OneMoreTime { shorten, .. },
                    Self::OneMoreTime { shorten: new, .. },
                    P::Shorten,
                ) => *shorten = *new,
                (
                    Self::OneMoreTime { variation, .. },
                    Self::OneMoreTime { variation: new, .. },
                    P::Variation,
                ) => *variation = *new,
                (
                    Self::NothingHappens { tone, .. },
                    Self::NothingHappens { tone: new, .. },
                    P::Tone,
                ) => {
                    *tone = *new;
                }
                (
                    Self::NothingHappens { silence, .. },
                    Self::NothingHappens { silence: new, .. },
                    P::Silence,
                ) => *silence = *new,
                (
                    Self::NothingHappens { register, .. },
                    Self::NothingHappens { register: new, .. },
                    P::Register,
                )
                | (
                    Self::AreWeDone { register, .. },
                    Self::AreWeDone { register: new, .. },
                    P::Register,
                )
                | (
                    Self::NonSequitur { register, .. },
                    Self::NonSequitur { register: new, .. },
                    P::Register,
                ) => *register = *new,
                _ => {
                    return Err(invalid(&format!(
                        "{} has no {parameter:?} parameter",
                        self.name()
                    )));
                }
            }
        }
        Ok(result)
    }

    /// Whether `other` is the same recipe, so its parameters can replace
    /// these on an inserted gag.
    pub fn same_recipe(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
            && self.version() == other.version()
    }

    /// Whether the recipe edits and groups a pause it inserts, which must
    /// therefore be a direct child of the current group.
    pub fn frames_its_pause(&self) -> bool {
        matches!(
            self,
            Self::LongAnswer { .. } | Self::NothingHappens { .. } | Self::AreWeDone { .. }
        )
    }

    /// The gap lengths of One More Time, each resolved once from its exact
    /// authored value: `gap - k·shorten` after play `k + 1`, varied when a
    /// seed is pinned.
    pub fn one_more_time_gaps(
        plays: NonZeroU32,
        gap: PauseLength,
        shorten: PauseLength,
        variation: Option<GagVariation>,
    ) -> Result<Vec<PauseLength>, EditError> {
        Self::gaps(plays, gap, shorten, variation)
    }

    /// The gap lengths of One More Time, each resolved once from its exact
    /// authored value: `gap - k·shorten` after play `k + 1`.
    fn gaps(
        plays: NonZeroU32,
        gap: PauseLength,
        shorten: PauseLength,
        variation: Option<GagVariation>,
    ) -> Result<Vec<PauseLength>, EditError> {
        if let Some(variation) = variation {
            variation.validate()?;
        }
        let invalid = |message: &str| EditError::new(EditErrorCode::InvalidCommand, message);
        if plays.get() < 2 {
            return Err(invalid("One More Time needs at least two plays"));
        }
        if plays.get() - 1 > crate::MAX_SEMANTIC_REPEAT_GAPS as u32 {
            return Err(EditError::new(
                EditErrorCode::LimitExceeded,
                format!(
                    "One More Time sets at most {} gaps, so at most {} plays",
                    crate::MAX_SEMANTIC_REPEAT_GAPS,
                    crate::MAX_SEMANTIC_REPEAT_GAPS + 1
                ),
            ));
        }
        let (first, step, frames) = match (gap, shorten) {
            (PauseLength::Frames { frames: a }, PauseLength::Frames { frames: b }) => {
                (a.get(), b.get(), true)
            }
            (
                PauseLength::Milliseconds { milliseconds: a },
                PauseLength::Milliseconds { milliseconds: b },
            ) => (a.get(), b.get(), false),
            _ => return Err(invalid("give gap and shorten in the same unit")),
        };
        (0..plays.get() - 1)
            .map(|index| {
                let value = u64::from(index) * u64::from(step);
                u32::try_from(u64::from(first).saturating_sub(value))
                    .ok()
                    .and_then(NonZeroU32::new)
                    .and_then(|value| {
                        NonZeroU32::new(
                            variation.map_or(value.get(), |variation| {
                                variation.vary(value.get(), index)
                            }),
                        )
                    })
                    .map(|value| {
                        if frames {
                            PauseLength::Frames { frames: value }
                        } else {
                            PauseLength::Milliseconds {
                                milliseconds: value,
                            }
                        }
                    })
                    .ok_or_else(|| {
                        invalid("every gap must stay positive; shorten less or use fewer plays")
                    })
            })
            .collect()
    }

    /// The ordinary instructions this recipe stands for, ending with the
    /// pinning group. `visual` uses an active Visual range instead of the
    /// selected beat where the recipe acts on content; `rate` resolves pause
    /// lengths for the cursor motions some recipes make between their parts.
    pub fn expand(
        &self,
        visual: bool,
        rate: crate::FrameRate,
    ) -> Result<Vec<SemanticInstruction>, EditError> {
        if self.version() != GAG_RECIPE_VERSION {
            return Err(EditError::new(
                EditErrorCode::InvalidCommand,
                format!(
                    "{} version {} is not available; version {GAG_RECIPE_VERSION} is",
                    self.name(),
                    self.version()
                ),
            ));
        }
        let invalid = |message: &str| EditError::new(EditErrorCode::InvalidCommand, message);
        let content = if visual {
            SemanticSelector::VisualSelection
        } else {
            SemanticSelector::SelectedBeat
        };
        let mut group = SemanticSelector::SelectedBeat;
        let mut instructions = match self {
            Self::LongAnswer { pause, scale, .. } => {
                let start = FramingPose::identity();
                let end = FramingPose::new(start.center_x, start.center_y, *scale)
                    .and_then(|pose| pose.quantized())
                    .map_err(|error| invalid(&error.to_string()))?;
                vec![
                    SemanticInstruction::InsertPause {
                        length: *pause,
                        black: false,
                    },
                    SemanticInstruction::SetFraming {
                        framing: Some(Box::new(
                            Framing::creep(start, end, FramingCurve::Smoothstep)
                                .map_err(|error| invalid(&error.to_string()))?,
                        )),
                    },
                ]
            }
            Self::Escalator {
                plays,
                gain_step,
                zoom_step,
                ..
            } => vec![SemanticInstruction::Repeat {
                selector: content,
                plays: *plays,
                escalation: Some(RepeatEscalation {
                    gain_step: *gain_step,
                    zoom: (*zoom_step != ExactRatio::ZERO).then_some(ZoomStep {
                        step: *zoom_step,
                        progression: crate::ZoomProgression::Add,
                    }),
                }),
            }],
            Self::NonSequitur { register, .. } => vec![SemanticInstruction::Paste {
                register: *register,
                before: false,
            }],
            Self::OneMoreTime {
                plays,
                gap,
                shorten,
                variation,
                ..
            } => vec![
                SemanticInstruction::Repeat {
                    selector: content,
                    plays: *plays,
                    escalation: None,
                },
                SemanticInstruction::SetRepeat {
                    plays: None,
                    gaps: Some(Self::gaps(*plays, *gap, *shorten, *variation)?),
                    escalation: None,
                },
            ],
            Self::NothingHappens {
                tone,
                silence,
                register,
                ..
            } => {
                // Room tone first, then silence after it; the cursor returns
                // to the start and the group spans both pauses.
                let frames = |length: PauseLength| -> Result<NonZeroU32, EditError> {
                    let frames = length.resolve(rate)?.frames();
                    u32::try_from(frames)
                        .ok()
                        .and_then(NonZeroU32::new)
                        .ok_or_else(|| invalid("pause length exceeds the motion range"))
                };
                let tone_frames = frames(*tone)?;
                let total = tone_frames
                    .checked_add(frames(*silence)?.get())
                    .ok_or_else(|| invalid("pause length exceeds the motion range"))?;
                group = SemanticSelector::Motion {
                    motion: crate::SemanticMotion::Frames {
                        forward: true,
                        count: total,
                    },
                };
                vec![
                    SemanticInstruction::InsertPause {
                        length: *tone,
                        black: false,
                    },
                    SemanticInstruction::SetRoomTone {
                        register: *register,
                    },
                    SemanticInstruction::MoveFrames {
                        forward: true,
                        count: tone_frames,
                    },
                    SemanticInstruction::InsertPause {
                        length: *silence,
                        black: false,
                    },
                    SemanticInstruction::MoveFrames {
                        forward: false,
                        count: tone_frames,
                    },
                ]
            }
            Self::AreWeDone {
                pause, register, ..
            } => vec![
                SemanticInstruction::InsertPause {
                    length: *pause,
                    black: false,
                },
                SemanticInstruction::Tail {
                    length: None,
                    effect: crate::TailEffect::Reverb,
                },
                SemanticInstruction::SetCutaway {
                    register: *register,
                    fit: crate::CutawayFit::Hold,
                },
            ],
        };
        instructions.push(SemanticInstruction::Group {
            selector: group,
            label: self.label(),
        });
        Ok(instructions)
    }
}

fn length(value: PauseLength) -> String {
    match value {
        PauseLength::Frames { frames } => format!("{frames}f"),
        PauseLength::Milliseconds { milliseconds } => format!("{milliseconds}ms"),
    }
}

/// `12f` or `500ms`, as `length` writes them.
fn parse_length(text: &str) -> Option<PauseLength> {
    if let Some(milliseconds) = text.strip_suffix("ms") {
        return Some(PauseLength::Milliseconds {
            milliseconds: milliseconds.parse().ok()?,
        });
    }
    Some(PauseLength::Frames {
        frames: text.strip_suffix('f')?.parse().ok()?,
    })
}

/// A signed decimal such as `+3`, `-0.25` or `1.350`, exactly.
fn parse_decimal(text: &str) -> Option<ExactRatio> {
    let (negative, digits) = match text.as_bytes().first()? {
        b'+' => (false, &text[1..]),
        b'-' => (true, &text[1..]),
        _ => (false, text),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty()
        || whole.len() + fraction.len() > 18
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let magnitude: i128 = format!("{whole}{fraction}").parse().ok()?;
    let numerator = if negative { -magnitude } else { magnitude };
    ExactRatio::new(numerator, 10_i128.pow(u32::try_from(fraction.len()).ok()?)).ok()
}

fn parse_register(text: &str) -> Option<RegisterName> {
    let mut chars = text.chars();
    let name = chars.next()?;
    chars.next().is_none().then_some(())?;
    RegisterName::new(name).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipes_expand_to_ordinary_instructions_under_a_pinning_group() {
        let recipe = GagRecipe::LongAnswer {
            version: 1,
            pause: PauseLength::Milliseconds {
                milliseconds: NonZeroU32::new(1500).unwrap(),
            },
            scale: ExactRatio::new(27, 20).unwrap(),
        };
        let rate = crate::FrameRate::new(30, 1).unwrap();
        let expanded = recipe.expand(false, rate).unwrap();
        assert!(matches!(
            expanded[0],
            SemanticInstruction::InsertPause { .. }
        ));
        assert!(matches!(
            expanded[1],
            SemanticInstruction::SetFraming { .. }
        ));
        assert!(matches!(
            &expanded[2],
            SemanticInstruction::Group { label, .. }
                if label == "The Long Answer · v1 · pause 1500ms, creep to 1.350×"
        ));
        let future = GagRecipe::NonSequitur {
            version: 2,
            register: RegisterName::new('r').unwrap(),
        };
        assert!(
            future.expand(false, rate).is_err(),
            "unknown versions never expand"
        );
        let wire = serde_json::to_string(&recipe).unwrap();
        assert_eq!(serde_json::from_str::<GagRecipe>(&wire).unwrap(), recipe);
    }

    #[test]
    fn every_recipe_label_parses_back_to_exactly_its_recipe() {
        let millis = |value| PauseLength::Milliseconds {
            milliseconds: NonZeroU32::new(value).unwrap(),
        };
        let frames = |value| PauseLength::Frames {
            frames: NonZeroU32::new(value).unwrap(),
        };
        let r = |name| RegisterName::new(name).unwrap();
        let recipes = [
            GagRecipe::LongAnswer {
                version: 1,
                pause: millis(1500),
                scale: ExactRatio::new(27, 20).unwrap(),
            },
            GagRecipe::Escalator {
                version: 1,
                plays: NonZeroU32::new(4).unwrap(),
                gain_step: GainDb::new(-1_250).unwrap(),
                zoom_step: crate::quantize_zoom_step(ExactRatio::new(2, 25).unwrap()).unwrap(),
            },
            GagRecipe::Escalator {
                version: 1,
                plays: NonZeroU32::new(3).unwrap(),
                gain_step: GainDb::new(3_000).unwrap(),
                zoom_step: ExactRatio::ZERO,
            },
            GagRecipe::NonSequitur {
                version: 1,
                register: r('"'),
            },
            GagRecipe::OneMoreTime {
                version: 1,
                plays: NonZeroU32::new(5).unwrap(),
                gap: frames(12),
                shorten: frames(3),
                variation: Some(GagVariation {
                    percent: 20,
                    seed: 7,
                }),
            },
            GagRecipe::OneMoreTime {
                version: 1,
                plays: NonZeroU32::new(3).unwrap(),
                gap: millis(500),
                shorten: millis(200),
                variation: None,
            },
            GagRecipe::NothingHappens {
                version: 1,
                tone: millis(1000),
                silence: frames(30),
                register: r('t'),
            },
            GagRecipe::AreWeDone {
                version: 1,
                pause: millis(1500),
                register: r('r'),
            },
        ];
        for recipe in recipes {
            assert_eq!(
                GagRecipe::from_label(&recipe.label()),
                Some(recipe),
                "{}",
                recipe.label()
            );
            assert!(recipe.same_recipe(&recipe));
        }
        // A renamed group, an edited label and a stranger name none.
        for label in [
            "Renamed",
            "The Long Answer · v1 · pause 1500ms, creep to 1.35×",
            "The Long Answer · v1 · pause 1500 ms, creep to 1.350×",
            "One More Time · v1 · 3 plays, gap 500ms shortening by 200ms, varied ±20% (seed x)",
            "The Shrug · v1 · register r",
        ] {
            assert_eq!(GagRecipe::from_label(label), None, "{label}");
        }
        // A rounded creep scale comes back as written.
        let thirds = GagRecipe::LongAnswer {
            version: 1,
            pause: frames(12),
            scale: ExactRatio::new(4, 3).unwrap(),
        };
        assert_eq!(
            GagRecipe::from_label(&thirds.label()),
            Some(GagRecipe::LongAnswer {
                version: 1,
                pause: frames(12),
                scale: ExactRatio::new(1333, 1000).unwrap(),
            })
        );
        assert!(!thirds.same_recipe(&GagRecipe::AreWeDone {
            version: 1,
            pause: frames(12),
            register: r('r'),
        }));
    }

    #[test]
    fn seeded_variation_resolves_fixed_bounded_gaps_and_pins_its_seed() {
        let millis = |value| PauseLength::Milliseconds {
            milliseconds: NonZeroU32::new(value).unwrap(),
        };
        let recipe = |variation| GagRecipe::OneMoreTime {
            version: 1,
            plays: NonZeroU32::new(5).unwrap(),
            gap: millis(1000),
            shorten: millis(100),
            variation,
        };
        let gaps = |recipe: GagRecipe| -> Vec<u32> {
            let rate = crate::FrameRate::new(30, 1).unwrap();
            match &recipe.expand(false, rate).unwrap()[1] {
                SemanticInstruction::SetRepeat {
                    gaps: Some(gaps), ..
                } => gaps
                    .iter()
                    .map(|gap| match gap {
                        PauseLength::Milliseconds { milliseconds } => milliseconds.get(),
                        PauseLength::Frames { .. } => unreachable!(),
                    })
                    .collect(),
                other => panic!("{other:?}"),
            }
        };
        let plain = gaps(recipe(None));
        assert_eq!(plain, vec![1000, 900, 800, 700]);
        let seeded = Some(GagVariation {
            percent: 20,
            seed: 7,
        });
        let varied = gaps(recipe(seeded));
        // The same seed always resolves the same gaps; another seed differs.
        assert_eq!(gaps(recipe(seeded)), varied);
        assert_ne!(
            gaps(recipe(Some(GagVariation {
                percent: 20,
                seed: 8
            }))),
            varied
        );
        assert_ne!(varied, plain);
        for (varied, plain) in varied.iter().zip(&plain) {
            assert!(varied.abs_diff(*plain) <= plain / 5, "{varied} vs {plain}");
        }
        assert!(
            recipe(seeded).label().ends_with("varied ±20% (seed 7)"),
            "{}",
            recipe(seeded).label()
        );
        let wire = serde_json::to_value(recipe(seeded)).unwrap();
        assert_eq!(
            wire["variation"],
            serde_json::json!({"percent":20,"seed":7})
        );
        assert!(
            serde_json::to_value(recipe(None))
                .unwrap()
                .get("variation")
                .is_none()
        );
        let rate = crate::FrameRate::new(30, 1).unwrap();
        assert!(
            recipe(Some(GagVariation {
                percent: 51,
                seed: 1
            }))
            .expand(false, rate)
            .is_err()
        );
    }
}
