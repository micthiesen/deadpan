//! Admit monotonically newer repeat receipts only in the visible session.

use crate::project::semantic::{RepeatableCut, Snapshot};
use deadpan_core::{ProjectId, SemanticMotion, SemanticSelector};

impl super::DeadpanApp {
    pub(super) fn repeat_hint(&self) -> Option<String> {
        let edit = self
            .semantic
            .snapshot()?
            .edit_for(self.workspace.as_ref()?)
            .ok()?;
        match self.edit_selection() {
            crate::navigation::EditSelection::Range => {
                return Some("repeat cut selection".into());
            }
            crate::navigation::EditSelection::Empty => {
                return Some("repeat unavailable: empty range".into());
            }
            crate::navigation::EditSelection::None => {}
        }
        Some(match &edit.operation {
            RepeatableCut::Frames(operation) => format!("repeat cut {}f", operation.count()),
            RepeatableCut::Selector(selector) => match selector {
                SemanticSelector::SelectedBeat => "repeat cut beat".into(),
                SemanticSelector::VisualSelection => "select range to repeat cut".into(),
                SemanticSelector::Motion { motion } => match motion {
                    SemanticMotion::Frames { forward, count } => format!(
                        "repeat cut {count}f {}",
                        if *forward { "forward" } else { "backward" }
                    ),
                    SemanticMotion::Beats { forward, count } => format!(
                        "repeat cut {count} beats {}",
                        if *forward { "forward" } else { "backward" }
                    ),
                    SemanticMotion::Scope { end } => {
                        format!("repeat cut to group {}", if *end { "end" } else { "start" })
                    }
                },
            },
        })
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
    fn late_snapshots_cannot_restore_cleared_edits_or_cross_sessions() {
        let project = ProjectId::new("project").unwrap();
        let mut saved = Snapshot {
            session: 1,
            project: project.clone(),
            version: 2,
            head: Some(RevisionId::new("cut").unwrap()),
            edit: Some(LastEdit {
                operation: RepeatableCut::Frames(FrameCut::new(5).unwrap()),
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
