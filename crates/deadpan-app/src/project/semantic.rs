//! Session-local semantic edit receipts, independent of visible refreshes.

use deadpan_core::{
    FrameCut, ProjectId, RegisterName, RevisionId, SemanticContext, SemanticInstruction,
    SemanticSelector,
};

/// Unresolved cut intent. FrameCut retains its checked-add overflow behavior.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepeatableCut {
    Frames(FrameCut),
    Selector(SemanticSelector),
}

impl RepeatableCut {
    pub fn instruction(&self, register: RegisterName) -> SemanticInstruction {
        match self {
            Self::Frames(operation) => SemanticInstruction::CutFrames {
                operation: operation.clone(),
                register,
            },
            Self::Selector(selector) => SemanticInstruction::Cut {
                selector: *selector,
                register,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CutAttempt {
    pub operation: RepeatableCut,
    /// Dot-repeat binds the exact service snapshot visible at input time.
    pub repeat_version: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastEdit {
    pub operation: RepeatableCut,
    pub register: Option<char>,
}

impl LastEdit {
    /// Predict intent only. Core resolves and validates the new target, including
    /// refusing explicit empty Visual selections without falling back to a beat.
    pub fn instruction(
        &self,
        context: &SemanticContext,
        register: RegisterName,
    ) -> SemanticInstruction {
        if context.visual_selection.is_some() {
            RepeatableCut::Selector(SemanticSelector::VisualSelection).instruction(register)
        } else {
            self.operation.instruction(register)
        }
    }

    pub fn from_instruction(instruction: &SemanticInstruction) -> Option<Self> {
        let (operation, register) = match instruction {
            SemanticInstruction::Cut { selector, register } => {
                (RepeatableCut::Selector(*selector), *register)
            }
            SemanticInstruction::CutFrames {
                operation,
                register,
            } => (RepeatableCut::Frames(operation.clone()), *register),
            SemanticInstruction::CutSelection { register } => (
                RepeatableCut::Selector(SemanticSelector::VisualSelection),
                *register,
            ),
            _ => return None,
        };
        Some(Self {
            operation,
            register: (register != RegisterName::unnamed()).then_some(register.as_char()),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub session: u64,
    pub project: ProjectId,
    pub version: u64,
    pub head: Option<RevisionId>,
    pub edit: Option<LastEdit>,
    pub error: Option<String>,
}

impl Snapshot {
    pub fn edit_for(&self, workspace: &super::Workspace) -> Result<&LastEdit, String> {
        if self.session != workspace.session || &self.project != workspace.document.project_id() {
            return Err("The repeat belongs to another project session.".into());
        }
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if self.head.as_ref() != Some(workspace.document.revision_id()) {
            return Err("The saved project changed. Reopen it before repeating an edit.".into());
        }
        self.edit.as_ref().ok_or_else(|| {
            "No repeatable edit is available. Cut content first; other edits cannot be repeated yet."
                .into()
        })
    }
}
