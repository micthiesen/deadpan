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
}

/// The only version of each recipe so far.
pub const GAG_RECIPE_VERSION: u32 = 1;

impl GagRecipe {
    pub fn name(&self) -> &'static str {
        match self {
            Self::LongAnswer { .. } => "The Long Answer",
            Self::Escalator { .. } => "The Escalator",
            Self::NonSequitur { .. } => "The Non-Sequitur",
        }
    }

    fn version(&self) -> u32 {
        match self {
            Self::LongAnswer { version, .. }
            | Self::Escalator { version, .. }
            | Self::NonSequitur { version, .. } => *version,
        }
    }

    /// The group label pinning recipe, version and parameters.
    pub fn label(&self) -> String {
        let decimal = |value: ExactRatio| value.numerator() as f64 / value.denominator() as f64;
        let parameters = match self {
            Self::LongAnswer { pause, scale, .. } => {
                let pause = match pause {
                    PauseLength::Frames { frames } => format!("{frames}f"),
                    PauseLength::Milliseconds { milliseconds } => format!("{milliseconds}ms"),
                };
                format!("pause {pause}, creep to {:.3}×", decimal(*scale))
            }
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
        };
        format!("{} · v{} · {parameters}", self.name(), self.version())
    }

    /// Whether the recipe frames and groups a pause it inserts, which must
    /// therefore be a direct child of the current group.
    pub fn frames_its_pause(&self) -> bool {
        matches!(self, Self::LongAnswer { .. })
    }

    /// The ordinary instructions this recipe stands for, ending with the
    /// pinning group. `visual` uses an active Visual range instead of the
    /// selected beat where the recipe acts on content.
    pub fn expand(&self, visual: bool) -> Result<Vec<SemanticInstruction>, EditError> {
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
                selector: if visual {
                    SemanticSelector::VisualSelection
                } else {
                    SemanticSelector::SelectedBeat
                },
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
        };
        instructions.push(SemanticInstruction::Group {
            selector: SemanticSelector::SelectedBeat,
            label: self.label(),
        });
        Ok(instructions)
    }
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
        let expanded = recipe.expand(false).unwrap();
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
            future.expand(false).is_err(),
            "unknown versions never expand"
        );
        let wire = serde_json::to_string(&recipe).unwrap();
        assert_eq!(serde_json::from_str::<GagRecipe>(&wire).unwrap(), recipe);
    }
}
