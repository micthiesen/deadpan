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
        escalation: Option<deadpan_core::RepeatEscalation>,
    },
    SetRepeatPlays {
        plays: NonZeroU32,
    },
    Group {
        selector: SemanticSelector,
        label: String,
    },
    Ungroup,
    /// A creative edit of the selected beat or at the cursor that keeps its
    /// exact parameters: gain and saturation, reverse and ping-pong pauses,
    /// tails and gags. Dot applies the same instruction to the current
    /// selection, which resolves it afresh.
    Parameter(SemanticInstruction),
}

impl RepeatableEdit {
    /// Instructions whose parameters alone define the repeated edit.
    fn parameter(instruction: &SemanticInstruction) -> bool {
        matches!(
            instruction,
            SemanticInstruction::SetAudio { .. }
                | SemanticInstruction::SplitEdit { .. }
                | SemanticInstruction::DeleteRole { .. }
                | SemanticInstruction::RoleRepeat { .. }
                | SemanticInstruction::InsertReverse { .. }
                | SemanticInstruction::Tail { .. }
                | SemanticInstruction::InsertPause { .. }
                | SemanticInstruction::SetFraming { .. }
                | SemanticInstruction::Gag { .. }
        )
    }
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

    /// Predict intent only. A saved text-object selector stays unresolved until
    /// core sees the current group; an active Object Visual overrides it through
    /// the same typed Visual selector without becoming a time range.
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
            RepeatableEdit::Repeat {
                selector,
                plays,
                escalation,
            } => SemanticInstruction::Repeat {
                selector: if context.visual_selection.is_some() {
                    SemanticSelector::VisualSelection
                } else {
                    *selector
                },
                plays: *plays,
                escalation: *escalation,
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
            RepeatableEdit::Parameter(instruction) => instruction.clone(),
        }
    }

    pub fn from_instruction(instruction: &SemanticInstruction) -> Option<Self> {
        if RepeatableEdit::parameter(instruction) {
            return Some(Self {
                operation: RepeatableEdit::Parameter(instruction.clone()),
                register: None,
            });
        }
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
        if let SemanticInstruction::Repeat {
            selector,
            plays,
            escalation,
        } = instruction
        {
            return Some(Self {
                operation: RepeatableEdit::Repeat {
                    selector: *selector,
                    plays: *plays,
                    escalation: *escalation,
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
