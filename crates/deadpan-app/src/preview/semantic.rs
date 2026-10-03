//! Admit monotonically newer repeat receipts only in the visible session.

use crate::project::semantic::Snapshot;
use deadpan_core::ProjectId;

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
                operation: FrameCut::new(5).unwrap(),
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
