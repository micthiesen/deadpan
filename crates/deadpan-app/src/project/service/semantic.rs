//! Observe every actual head transition, including saves with failed refreshes.

use super::*;
use crate::project::semantic::{LastEdit, Snapshot};

#[derive(Clone)]
pub(super) enum Change {
    Preserve,
    Replace(LastEdit),
}

struct Proof {
    session: u64,
    project: ProjectId,
    before: RevisionId,
    after: RevisionId,
    change: Change,
}

#[derive(Default)]
pub(super) struct State {
    snapshot: Option<Snapshot>,
    version: u64,
    proof: Option<Proof>,
}

impl State {
    pub(super) fn snapshot(&self) -> Option<Snapshot> {
        self.snapshot.clone()
    }

    pub(super) fn prove(
        &mut self,
        session: u64,
        project: &ProjectId,
        before: &RevisionId,
        after: &RevisionId,
        change: Change,
    ) {
        self.proof = Some(Proof {
            session,
            project: project.clone(),
            before: before.clone(),
            after: after.clone(),
            change,
        });
    }

    fn observe(&mut self, session: u64, project: &ProjectId, head: Result<RevisionId>) {
        let proof = self.proof.take();
        let (head, mut error) = match head {
            Ok(head) => (Some(head), None),
            Err(error) => (None, Some(format!("Repeat is unavailable: {error}"))),
        };
        let previous = self
            .snapshot
            .as_ref()
            .filter(|previous| previous.session == session && &previous.project == project);
        if previous.is_some_and(|previous| previous.head == head && previous.error == error) {
            return;
        }
        let mut edit = None;
        if let (Some(previous), Some(proof), Some(head)) = (previous, proof, head.as_ref())
            && proof.session == session
            && &proof.project == project
            && previous.head.as_ref() == Some(&proof.before)
            && head == &proof.after
        {
            edit = match proof.change {
                Change::Preserve => previous.edit.clone(),
                Change::Replace(edit) => Some(edit),
            };
        }
        // Reserve the final version for a permanent unavailable state. An old
        // snapshot at that version can never contain a repeatable operation.
        self.version = self.version.saturating_add(1);
        if self.version == u64::MAX {
            edit = None;
            error = Some("Repeat state exhausted. Close and restart Deadpan.".into());
        }
        self.snapshot = Some(Snapshot {
            session,
            project: project.clone(),
            version: self.version,
            head,
            edit,
            error,
        });
    }

    fn close(&mut self) {
        self.snapshot = None;
        self.proof = None;
    }
}

impl Service {
    pub(super) fn observe_semantic(&mut self) {
        match (self.workspace.as_ref(), self.store.as_ref()) {
            (Some(workspace), Some(store)) => self.semantic.observe(
                workspace.session,
                workspace.document.project_id(),
                store.head_revision().map_err(display),
            ),
            _ => self.semantic.close(),
        }
    }

    pub(super) fn preserve_semantic(&mut self, before: &RevisionId, after: &RevisionId) {
        if let Some(workspace) = &self.workspace {
            self.semantic.prove(
                workspace.session,
                workspace.document.project_id(),
                before,
                after,
                Change::Preserve,
            );
        }
    }
}

#[cfg(test)]
mod tests;
