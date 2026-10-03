//! Admit monotonically newer repeat receipts only in the visible session.

use crate::project::semantic::{RepeatableCut, RepeatableEdit, Snapshot};
use deadpan_core::{ProjectId, SemanticMotion, SemanticSelector};

impl super::DeadpanApp {
    pub(super) fn last_edit_uses_register(&self) -> bool {
        self.semantic
            .snapshot()
            .and_then(|snapshot| snapshot.edit.as_ref())
            .is_none_or(|edit| edit.uses_register())
    }

    pub(super) fn repeat_hint(&self) -> Option<String> {
        let edit = self
            .semantic
            .snapshot()?
            .edit_for(self.workspace.as_ref()?)
            .ok()?;
        let selected_repeat = matches!(edit.operation, RepeatableEdit::SetRepeatPlays { .. })
            && self.workspace.as_ref().is_some_and(|workspace| {
                self.selected_beat.as_ref().is_some_and(|selected| {
                    self.sequence_scope
                        .resolve(workspace)
                        .is_ok_and(|scope| scope.children.contains(selected))
                        && matches!(
                            workspace
                                .document
                                .nodes()
                                .get(selected)
                                .map(|node| &node.kind),
                            Some(deadpan_core::NodeKind::Repeat { .. })
                        )
                })
            });
        Some(repeat_instruction_hint(
            &edit.operation,
            self.edit_selection(),
            selected_repeat,
        ))
    }
}

fn repeat_instruction_hint(
    operation: &RepeatableEdit,
    selection: crate::navigation::EditSelection,
    selected_repeat: bool,
) -> String {
    let visual_hint = |range: String| match selection {
        crate::navigation::EditSelection::Range => Some(range),
        crate::navigation::EditSelection::Empty => Some("repeat unavailable: empty range".into()),
        crate::navigation::EditSelection::None => None,
    };
    let (action, selector) = match operation {
        RepeatableEdit::Cut(RepeatableCut::Frames(operation)) => {
            return visual_hint("repeat cut selection".into())
                .unwrap_or_else(|| format!("repeat cut {}f", operation.count()));
        }
        RepeatableEdit::Cut(RepeatableCut::Selector(selector)) => {
            if let Some(hint) = visual_hint("repeat cut selection".into()) {
                return hint;
            }
            ("repeat cut".to_owned(), selector)
        }
        RepeatableEdit::Repeat { selector, plays } => {
            if let Some(hint) = visual_hint(format!("repeat selection ×{plays}")) {
                return hint;
            }
            (format!("repeat ×{plays}"), selector)
        }
        RepeatableEdit::SetRepeatPlays { plays } => {
            return if selection != crate::navigation::EditSelection::None {
                "repeat unavailable: clear Visual range".into()
            } else if selected_repeat {
                format!("set Repeat to {plays} plays")
            } else {
                format!("select Repeat to set {plays} plays")
            };
        }
    };
    match selector {
        SemanticSelector::SelectedBeat => format!("{action} beat"),
        SemanticSelector::VisualSelection => format!("select range to {action}"),
        SemanticSelector::Motion { motion } => match motion {
            SemanticMotion::Frames { forward, count } => format!(
                "{action} {count}f {}",
                if *forward { "forward" } else { "backward" }
            ),
            SemanticMotion::Beats { forward, count } => format!(
                "{action} {count} beats {}",
                if *forward { "forward" } else { "backward" }
            ),
            SemanticMotion::Scope { end } => {
                format!("{action} to group {}", if *end { "end" } else { "start" })
            }
        },
    }
}

#[derive(Default)]
pub(super) struct Mirror {
    snapshot: Option<Snapshot>,
}

impl Mirror {
    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_ref()
    }

    pub fn receive(&mut self, incoming: Option<Snapshot>, context: Option<(u64, &ProjectId)>) {
        let matches = |snapshot: &Snapshot| {
            context.is_some_and(|(session, project)| {
                snapshot.session == session && &snapshot.project == project
            })
        };
        if self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| !matches(snapshot))
        {
            self.snapshot = None;
        }
        if let Some(incoming) = incoming
            && matches(&incoming)
            && self
                .snapshot
                .as_ref()
                .is_none_or(|previous| incoming.version > previous.version)
        {
            self.snapshot = Some(incoming);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::semantic::LastEdit;
    use deadpan_core::{FrameCut, RevisionId};

    #[test]
    fn repeat_count_hint_requires_an_explicit_repeat_and_no_visual_state() {
        use crate::navigation::EditSelection;
        let operation = RepeatableEdit::SetRepeatPlays {
            plays: std::num::NonZeroU32::new(3).unwrap(),
        };
        assert_eq!(
            repeat_instruction_hint(&operation, EditSelection::None, true),
            "set Repeat to 3 plays"
        );
        assert_eq!(
            repeat_instruction_hint(&operation, EditSelection::None, false),
            "select Repeat to set 3 plays"
        );
        for selection in [EditSelection::Empty, EditSelection::Range] {
            for selected_repeat in [false, true] {
                assert_eq!(
                    repeat_instruction_hint(&operation, selection, selected_repeat),
                    "repeat unavailable: clear Visual range"
                );
            }
        }
    }

    #[test]
    fn selector_and_frame_repeat_hints_keep_visual_override_teaching() {
        use crate::navigation::EditSelection;
        let cut = RepeatableEdit::Cut(RepeatableCut::Frames(FrameCut::new(5).unwrap()));
        assert_eq!(
            repeat_instruction_hint(&cut, EditSelection::None, false),
            "repeat cut 5f"
        );
        assert_eq!(
            repeat_instruction_hint(&cut, EditSelection::Range, false),
            "repeat cut selection"
        );
        assert_eq!(
            repeat_instruction_hint(&cut, EditSelection::Empty, false),
            "repeat unavailable: empty range"
        );
        let wrapped = RepeatableEdit::Repeat {
            selector: SemanticSelector::VisualSelection,
            plays: std::num::NonZeroU32::new(3).unwrap(),
        };
        assert_eq!(
            repeat_instruction_hint(&wrapped, EditSelection::None, true),
            "select range to repeat ×3"
        );
        assert_eq!(
            repeat_instruction_hint(&wrapped, EditSelection::Range, true),
            "repeat selection ×3"
        );
        assert_eq!(
            repeat_instruction_hint(&wrapped, EditSelection::Empty, true),
            "repeat unavailable: empty range"
        );
    }

    #[test]
    fn late_snapshots_cannot_restore_cleared_edits_or_cross_sessions() {
        let project = ProjectId::new("project").unwrap();
        let mut saved = Snapshot {
            session: 1,
            project: project.clone(),
            version: 2,
            head: Some(RevisionId::new("cut").unwrap()),
            edit: Some(LastEdit {
                operation: RepeatableEdit::Cut(RepeatableCut::Frames(FrameCut::new(5).unwrap())),
                register: None,
            }),
            error: None,
        };
        let mut mirror = Mirror::default();
        mirror.receive(Some(saved.clone()), Some((1, &project)));
        let mut cleared = saved.clone();
        cleared.version = 3;
        cleared.edit = None;
        mirror.receive(Some(cleared.clone()), Some((1, &project)));
        mirror.receive(Some(saved.clone()), Some((1, &project)));
        assert_eq!(mirror.snapshot(), Some(&cleared));
        mirror.receive(Some(saved.clone()), Some((2, &project)));
        assert!(mirror.snapshot().is_none());
        saved.session = 2;
        mirror.receive(Some(saved.clone()), Some((2, &project)));
        mirror.receive(None, None);
        assert!(mirror.snapshot().is_none());
        mirror.receive(Some(saved), None);
        assert!(mirror.snapshot().is_none());
    }
}
