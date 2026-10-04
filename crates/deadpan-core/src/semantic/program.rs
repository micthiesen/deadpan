//! Closed semantic intent. No input events, absolute targets or process hooks.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::{EditError, EditErrorCode, FrameCut, RegisterName, SemanticTextObject};

pub const MAX_SEMANTIC_PROGRAM_INSTRUCTIONS: usize = 1024;
pub const MAX_SEMANTIC_PROGRAM_BYTES: usize = 128 * 1024;
pub const MAX_SEMANTIC_INSTRUCTION_FUEL: usize = 4096;
pub const MAX_SEMANTIC_CALL_DEPTH: usize = 16;

/// A relative destination in the current ordinary Sequence. Beat motions use
/// the explicit selected child when present, independently of the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticMotion {
    Frames {
        forward: bool,
        count: NonZeroU32,
    },
    Beats {
        forward: bool,
        count: NonZeroU32,
    },
    Scope {
        end: bool,
    },
    /// `w`, `b` and `e`: recognized word starts, or ends when `end` is set.
    /// Backward word ends are not part of the vocabulary.
    Words {
        forward: bool,
        count: NonZeroU32,
        end: bool,
    },
    /// `W` and `B`: recognized sentence starts.
    Sentences {
        forward: bool,
        count: NonZeroU32,
    },
}

impl SemanticMotion {
    pub const fn uses_speech(self) -> bool {
        matches!(self, Self::Words { .. } | Self::Sentences { .. })
    }
}

/// Select copied or deleted content without moving the context. Motion ranges
/// run from the entry cursor to the same destination as ordinary navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticSelector {
    #[serde(deserialize_with = "deserialize_empty")]
    SelectedBeat,
    #[serde(deserialize_with = "deserialize_empty")]
    VisualSelection,
    Motion {
        motion: SemanticMotion,
    },
    TextObject {
        object: SemanticTextObject,
    },
    /// `iw`, `aw`, `is` and `as` against recognized speech at the cursor.
    Speech {
        object: super::SpeechObject,
    },
}

impl SemanticSelector {
    pub const fn uses_speech(self) -> bool {
        match self {
            Self::Motion { motion } => motion.uses_speech(),
            Self::Speech { .. } => true,
            Self::SelectedBeat | Self::VisualSelection | Self::TextObject { .. } => false,
        }
    }
}

/// The supported macro vocabulary. Selectors resolve against each preceding
/// staged document and its checked ordinary Sequence navigation context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticInstruction {
    MoveFrames {
        forward: bool,
        count: NonZeroU32,
    },
    MoveBeats {
        forward: bool,
        count: NonZeroU32,
    },
    MoveScope {
        end: bool,
    },
    MoveWords {
        forward: bool,
        count: NonZeroU32,
        end: bool,
    },
    MoveSentences {
        forward: bool,
        count: NonZeroU32,
    },
    /// Select a word or sentence object as an extending Visual time range.
    SelectSpeech {
        object: super::SpeechObject,
    },
    SelectObject {
        object: SemanticTextObject,
    },
    #[serde(deserialize_with = "deserialize_empty")]
    BeginSelection,
    #[serde(deserialize_with = "deserialize_empty")]
    FinishSelection,
    #[serde(deserialize_with = "deserialize_empty")]
    ClearSelection,
    Yank {
        selector: SemanticSelector,
        register: RegisterName,
    },
    Cut {
        selector: SemanticSelector,
        register: RegisterName,
    },
    Group {
        selector: SemanticSelector,
        label: String,
    },
    #[serde(deserialize_with = "deserialize_empty")]
    Ungroup,
    Repeat {
        selector: SemanticSelector,
        plays: NonZeroU32,
    },
    /// Set the selected direct-child Repeat's total count. Visual selection is
    /// incompatible with this node parameter edit, including an empty range.
    SetRepeatPlays {
        plays: NonZeroU32,
    },
    CutFrames {
        operation: FrameCut,
        register: RegisterName,
    },
    YankBeat {
        register: RegisterName,
    },
    YankSelection {
        register: RegisterName,
    },
    CutSelection {
        register: RegisterName,
    },
    ReplaceSelection {
        register: RegisterName,
    },
    /// Explicit seam insertion for absent or Time Visual selection. An Object
    /// Visual replaces its exact owned target; ReplaceSelection handles both
    /// Visual kinds explicitly, including empty group contents at slot zero.
    Paste {
        register: RegisterName,
        before: bool,
    },
    Call {
        register: RegisterName,
        count: NonZeroU32,
    },
}

impl SemanticInstruction {
    /// True when resolving this instruction needs recognized speech.
    pub fn uses_speech(&self) -> bool {
        match self {
            Self::MoveWords { .. } | Self::MoveSentences { .. } | Self::SelectSpeech { .. } => true,
            Self::Yank { selector, .. }
            | Self::Cut { selector, .. }
            | Self::Group { selector, .. }
            | Self::Repeat { selector, .. } => selector.uses_speech(),
            _ => false,
        }
    }

    fn backward_word_end(&self) -> bool {
        let backward_end = |motion: &SemanticMotion| {
            matches!(
                motion,
                SemanticMotion::Words {
                    forward: false,
                    end: true,
                    ..
                }
            )
        };
        match self {
            Self::MoveWords {
                forward: false,
                end: true,
                ..
            } => true,
            Self::Yank { selector, .. }
            | Self::Cut { selector, .. }
            | Self::Group { selector, .. }
            | Self::Repeat { selector, .. } => {
                matches!(selector, SemanticSelector::Motion { motion } if backward_end(motion))
            }
            _ => false,
        }
    }
}

// Internally tagged unit variants otherwise ignore unknown fields in Serde.
pub(super) fn deserialize_empty<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<(), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Empty {}

    Empty::deserialize(deserializer).map(|_| ())
}

/// Validated nonempty macro body. The immutable private body makes every
/// register-held program safe to expand without repeatedly scanning its wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProgram {
    instructions: Vec<SemanticInstruction>,
}

impl SemanticProgram {
    pub fn new(instructions: Vec<SemanticInstruction>) -> Result<Self, EditError> {
        let result = Self { instructions };
        result.validate()?;
        Ok(result)
    }

    pub fn instructions(&self) -> &[SemanticInstruction] {
        &self.instructions
    }

    pub fn validate(&self) -> Result<(), EditError> {
        if self.instructions.is_empty()
            || self.instructions.len() > MAX_SEMANTIC_PROGRAM_INSTRUCTIONS
        {
            return Err(EditError::new(
                EditErrorCode::LimitExceeded,
                "a semantic program needs 1..=1024 instructions",
            ));
        }
        for instruction in &self.instructions {
            if instruction.backward_word_end() {
                return Err(EditError::new(
                    EditErrorCode::InvalidCommand,
                    "word-end motions move forward only",
                ));
            }
            if let SemanticInstruction::Group { label, .. } = instruction {
                crate::validate_group_label(label)?;
            }
            if let SemanticInstruction::Call { register, .. } = instruction
                && *register == RegisterName::unnamed()
            {
                return Err(EditError::new(
                    EditErrorCode::InvalidCommand,
                    "a macro call requires a named register a-z",
                ));
            }
        }
        crate::compound::wire::size(self, MAX_SEMANTIC_PROGRAM_BYTES)?;
        Ok(())
    }
}

impl<'de> Deserialize<'de> for SemanticProgram {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            instructions: Vec<SemanticInstruction>,
        }
        // Bound decoded bytes, depth, duplicate fields and the body array before
        // internally tagged instructions allocate their typed payloads. Hosts
        // still bound raw envelopes, including whitespace and escaped strings.
        let value = crate::compound::wire::read_bounded(deserializer, MAX_SEMANTIC_PROGRAM_BYTES)?;
        let wire = Wire::deserialize(value).map_err(serde::de::Error::custom)?;
        Self::new(wire.instructions).map_err(serde::de::Error::custom)
    }
}
