//! Session-local semantic edit receipts, independent of visible refreshes.

use deadpan_core::{
    FrameCut, ProjectId, RegisterName, RevisionId, SemanticContext, SemanticInstruction,
    SemanticSelector,
};
use std::num::NonZeroU32;

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
pub enum RepeatableEdit {
    Cut(RepeatableCut),
    Repeat {
        selector: SemanticSelector,
        plays: NonZeroU32,
    },
    SetRepeatPlays {
        plays: NonZeroU32,
    },
    Group {
        selector: SemanticSelector,
        label: String,
    },
    Ungroup,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastEdit {
    pub operation: RepeatableEdit,
    /// Only cuts use a register. Other edits preserve pending overrides.
    pub register: Option<char>,
}

impl LastEdit {
    pub fn uses_register(&self) -> bool {
        matches!(self.operation, RepeatableEdit::Cut(_))
    }

    /// Predict intent only. Core resolves and validates the new target, including
    /// refusing explicit empty Visual selections without falling back to a beat.
    pub fn instruction(
        &self,
        context: &SemanticContext,
        register: RegisterName,
    ) -> SemanticInstruction {
        match &self.operation {
            RepeatableEdit::Cut(operation) => {
                if context.visual_selection.is_some() {
                    RepeatableCut::Selector(SemanticSelector::VisualSelection).instruction(register)
                } else {
                    operation.instruction(register)
                }
            }
            RepeatableEdit::Repeat { selector, plays } => SemanticInstruction::Repeat {
                selector: if context.visual_selection.is_some() {
                    SemanticSelector::VisualSelection
                } else {
                    *selector
                },
                plays: *plays,
            },
            RepeatableEdit::SetRepeatPlays { plays } => {
                SemanticInstruction::SetRepeatPlays { plays: *plays }
            }
            RepeatableEdit::Group { selector, label } => SemanticInstruction::Group {
                selector: if context.visual_selection.is_some() {
                    SemanticSelector::VisualSelection
                } else {
                    *selector
                },
                label: label.clone(),
            },
            RepeatableEdit::Ungroup => SemanticInstruction::Ungroup,
        }
    }

    pub fn from_instruction(instruction: &SemanticInstruction) -> Option<Self> {
        if let SemanticInstruction::Group { selector, label } = instruction {
            return Some(Self {
                operation: RepeatableEdit::Group {
                    selector: *selector,
                    label: label.clone(),
                },
                register: None,
            });
        }
        if let SemanticInstruction::Ungroup = instruction {
            return Some(Self {
                operation: RepeatableEdit::Ungroup,
                register: None,
            });
        }
        if let SemanticInstruction::SetRepeatPlays { plays } = instruction {
            return Some(Self {
                operation: RepeatableEdit::SetRepeatPlays { plays: *plays },
                register: None,
            });
        }
        if let SemanticInstruction::Repeat { selector, plays } = instruction {
            return Some(Self {
                operation: RepeatableEdit::Repeat {
                    selector: *selector,
                    plays: *plays,
                },
                register: None,
            });
        }
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
            operation: RepeatableEdit::Cut(operation),
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
            "No repeatable edit is available. Cut, group, ungroup, wrap a Repeat or set its play count first.".into()
        })
    }
}
