//! Revision- and register-bound semantic macro requests and durable receipts.

use std::sync::Arc;

use deadpan_core::{
    NodeId, ProjectFrame, ProjectId, RevisionId, SemanticContext, SemanticInstruction,
    SemanticProgram, SemanticVisualSelection,
};

use super::{CommittedEdit, SequenceScope};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Id {
    pub session: u64,
    pub project: ProjectId,
    pub revision: RevisionId,
    pub bank_version: u64,
    pub request: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Save {
        id: Id,
        register: char,
        program: Arc<SemanticProgram>,
    },
    Run {
        id: Id,
        register: char,
        count: u32,
        scope: SequenceScope,
        context: SemanticContext,
    },
    Apply {
        id: Id,
        instruction: SemanticInstruction,
        scope: SequenceScope,
        context: SemanticContext,
    },
}

impl Operation {
    pub fn id(&self) -> &Id {
        match self {
            Self::Save { id, .. } | Self::Run { id, .. } | Self::Apply { id, .. } => id,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Outcome {
    Saved {
        register: char,
        instructions: usize,
    },
    Executed {
        register: char,
        count: u32,
        scope: SequenceScope,
        cursor: ProjectFrame,
        selected: Option<NodeId>,
        visual_selection: Option<SemanticVisualSelection>,
        committed: Option<Box<CommittedEdit>>,
        refresh_error: Option<String>,
    },
    Applied {
        scope: SequenceScope,
        cursor: ProjectFrame,
        selected: Option<NodeId>,
        visual_selection: Option<SemanticVisualSelection>,
        committed: Option<Box<CommittedEdit>>,
        refresh_error: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct Receipt {
    pub id: Id,
    pub bank_version: u64,
    pub outcome: Outcome,
}

impl Receipt {
    pub fn committed(&self) -> Option<&CommittedEdit> {
        match &self.outcome {
            Outcome::Executed { committed, .. } | Outcome::Applied { committed, .. } => {
                committed.as_deref()
            }
            Outcome::Saved { .. } => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Update {
    pub id: Id,
    pub result: Result<Receipt, String>,
}
