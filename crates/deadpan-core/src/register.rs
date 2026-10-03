//! Pure typed copied contents; persistence and media admission belong to the host.

use std::{ops::Range, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    AssetId, CapturedEditSlice, EditError, EditErrorCode, RevisionId, SemanticProgram,
    SourceQualificationId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct RegisterName(char);

impl RegisterName {
    pub fn new(name: char) -> Result<Self, EditError> {
        if name == '"' || name.is_ascii_lowercase() {
            Ok(Self(name))
        } else {
            Err(EditError::new(
                EditErrorCode::InvalidCommand,
                "register name must be a-z or the unnamed register (\")",
            ))
        }
    }
    pub const fn unnamed() -> Self {
        Self('"')
    }
    pub const fn as_char(self) -> char {
        self.0
    }
}

impl<'de> Deserialize<'de> for RegisterName {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(char::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RegisterValue {
    Original {
        revision: RevisionId,
        asset: AssetId,
        qualification: SourceQualificationId,
        ordinals: Range<u64>,
    },
    Edited {
        slice: Arc<CapturedEditSlice>,
    },
    Macro {
        program: Arc<SemanticProgram>,
    },
}

impl RegisterValue {
    /// Authored copies retain capture provenance. A semantic program has none.
    pub fn capture_revision(&self) -> Option<&RevisionId> {
        match self {
            Self::Original { revision, .. } => Some(revision),
            Self::Edited { slice } => Some(slice.revision_id()),
            Self::Macro { .. } => None,
        }
    }
}
