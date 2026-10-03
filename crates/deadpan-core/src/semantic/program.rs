//! Closed semantic intent. No input events, absolute targets or process hooks.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::{EditError, EditErrorCode, FrameCut, RegisterName};

pub const MAX_SEMANTIC_PROGRAM_INSTRUCTIONS: usize = 1024;
pub const MAX_SEMANTIC_PROGRAM_BYTES: usize = 128 * 1024;
pub const MAX_SEMANTIC_INSTRUCTION_FUEL: usize = 4096;
pub const MAX_SEMANTIC_CALL_DEPTH: usize = 16;

/// The supported macro vocabulary. Selectors resolve against each preceding
/// staged document, with the same ordinary Sequence scope throughout the run.
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
    #[serde(deserialize_with = "deserialize_empty")]
    BeginSelection,
    #[serde(deserialize_with = "deserialize_empty")]
    FinishSelection,
    #[serde(deserialize_with = "deserialize_empty")]
    ClearSelection,
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
    Paste {
        register: RegisterName,
        before: bool,
    },
    Call {
        register: RegisterName,
        count: NonZeroU32,
    },
}

// Internally tagged unit variants otherwise ignore unknown fields in Serde.
fn deserialize_empty<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<(), D::Error> {
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
