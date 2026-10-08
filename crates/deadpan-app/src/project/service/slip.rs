//! One exact qualified Slip proposal, with durable success independent of refresh.

use super::*;
use crate::project::slip::{
    CommitReceipt, CommitUpdate, Prepared, Proposal, ProposalId, ProposalUpdate,
};

pub(super) struct Draft {
    proposal: Proposal,
    request: CommandRequest,
    prepared: Arc<Prepared>,
}

impl Service {
    fn check_slip_context(&self, id: &ProposalId) -> Result<()> {
        if id.session == 0 || id.draft == 0 || id.change == 0 {
            return Err("Slip proposal identities must be nonzero.".into());
        }
        self.check_context(id.session, &id.base_revision)?;
        if self.pending_session_change.is_some()
            || self
                .workspace
                .as_ref()
                .is_none_or(|workspace| workspace.document.project_id() != &id.project)
        {
            return Err("Slip project session changed; reopen Slip.".into());
        }
        Ok(())
    }

    pub(super) fn prepare_slip_command(&mut self, proposal: Proposal) {
        let id = proposal.id();
        let result = self.prepare_slip(proposal);
        self.slip = Some(ProposalUpdate { id, result });
    }

    fn prepare_slip(&mut self, proposal: Proposal) -> Result<Arc<Prepared>> {
        let id = proposal.id();
        self.check_slip_context(&id)?;
        if self.slip_seen.as_ref().is_some_and(|seen| {
            seen.session == id.session
                && (id.draft < seen.draft
                    || (id.draft == seen.draft
                        && (id.change <= seen.change || id.base_revision != seen.base_revision)))
        }) {
            return Err("Slip proposal identity was already used or superseded.".into());
        }
        // Failed newer refinements must never leave a preceding amount ready.
        self.slip_seen = Some(id);
        self.slip_draft = None;
        let base = self
            .workspace
            .as_ref()
            .ok_or("Open a project first.")?
            .clone();
        proposal.target.validate(&base)?;
        let request = CommandRequest {
            project_id: proposal.target.project.clone(),
            expected_revision: proposal.target.base_revision.clone(),
            new_revision: revision(),
            command: Command::SlipSource {
                parent: proposal.target.parent.clone(),
                node: proposal.target.node.clone(),
                delta_frames: proposal.delta_frames,
            },
        };
        let preview = self
            .store
            .as_ref()
            .ok_or("Open a project first.")?
            .preview_source_slip(&request)
            .map_err(display)?;
        let snapshot = match preview.edit {
            Some(edit) => {
                let document = Arc::new(edit.forward.apply(&base.document).map_err(display)?);
                Some(Arc::new(
                    deadpan_playback::Snapshot::proposed(
                        &base.playback_snapshot(),
                        document,
                        proposal.draft,
                        proposal.change,
                    )
                    .map_err(display)?,
                ))
            }
            None => None,
        };
        let prepared = Arc::new(Prepared {
            base,
            target: proposal.target.clone(),
            resolution: preview.resolution,
            snapshot,
        });
        self.slip_draft = Some(Draft {
            proposal,
            request,
            prepared: prepared.clone(),
        });
        Ok(prepared)
    }

    pub(super) fn commit_slip_command(&mut self, id: ProposalId) {
        // Latest success is retained separately from failure feedback. Exact
        // retransmission observes it even after a failed refresh or later Undo.
        if self.pending_session_change.is_none()
            && self.workspace.as_ref().is_some_and(|workspace| {
                workspace.session == id.session && workspace.document.project_id() == &id.project
            })
            && let Some(saved) = &self.saved_slip
            && saved.id == id
        {
            self.slip_commit = Some(CommitUpdate {
                id,
                result: Ok(saved.committed.clone()),
            });
            return;
        }
        let result = self.commit_slip(&id);
        match result {
            Ok(committed) => {
                self.committed = Some(committed.clone());
                // The durable receipt exists before rebuilding any preview state.
                self.saved_slip = Some(CommitReceipt {
                    id: id.clone(),
                    committed: committed.clone(),
                    refresh_error: None,
                });
                self.slip_commit = Some(CommitUpdate {
                    id,
                    result: Ok(committed),
                });
                #[cfg(test)]
                {
                    self.render_preview_refresh_failure = self
                        .shared
                        .slip_commit_refresh_failure
                        .swap(false, Ordering::AcqRel);
                }
                let refresh_error = self.refresh().err();
                self.message = Some(match &refresh_error {
                    Some(error) => format!(
                        "Slip saved, but the preview could not refresh: {error}. Reopen the project to view the saved edit."
                    ),
                    None => "Slip saved. Placement and duration unchanged. Undo with u.".into(),
                });
                self.saved_slip
                    .as_mut()
                    .expect("successful Slip receipt")
                    .refresh_error = refresh_error;
            }
            Err(error) => {
                self.slip_commit = Some(CommitUpdate {
                    id,
                    result: Err(error),
                })
            }
        }
    }

    fn commit_slip(&mut self, id: &ProposalId) -> Result<CommittedEdit> {
        self.check_slip_context(id)?;
        let draft = self
            .slip_draft
            .as_ref()
            .ok_or("Slip proposal is no longer available.")?;
        if draft.proposal.id() != *id {
            return Err("Slip proposal was superseded; preview the latest amount.".into());
        }
        draft
            .proposal
            .target
            .validate(self.workspace.as_ref().ok_or("Open a project first.")?)?;
        // Consume a matching attempt once. Store failure cannot silently retry
        // this draft, and zero movement never authors a revision.
        let draft = self.slip_draft.take().expect("matching Slip draft checked");
        if draft.prepared.resolution.applied_delta_frames == 0 {
            return Err("source slip resolves to no change".into());
        }
        let outcome = self.writer()?.commit(&draft.request).map_err(display)?;
        self.capture_preparation_notices(&outcome.generation_preparation_notices);
        let target = draft.proposal.target;
        Ok(CommittedEdit {
            scoped: None,
            revision: outcome.revision_id,
            selected_node: Some(target.node),
            preserve_cursor: true,
            cursor: Some(target.cursor),
            scope: target.scope,
            sound: None,
            range_selection: None,
        })
    }

    pub(super) fn abandon_slip(&mut self, id: &ProposalId) {
        if self
            .slip_draft
            .as_ref()
            .is_some_and(|draft| draft.proposal.id() == *id)
        {
            self.invalidate_slip("Slip proposal was abandoned.");
        }
    }

    pub(super) fn invalidate_slip(&mut self, error: &str) {
        if let Some(draft) = self.slip_draft.take() {
            self.slip = Some(ProposalUpdate {
                id: draft.proposal.id(),
                result: Err(error.into()),
            });
        }
    }

    pub(super) fn reconcile_slip(&mut self) {
        let session = self
            .workspace
            .as_ref()
            .map(|workspace| (workspace.session, workspace.document.project_id().clone()));
        if self.slip_session != session {
            // Drop old admitted snapshots and session-local receipts together.
            // Wrong-context failure replies within the current session still
            // retain their supplied IDs so callers can complete their requests.
            self.slip_session = session;
            self.slip_draft = None;
            self.slip_seen = None;
            self.slip = None;
            self.slip_commit = None;
            self.saved_slip = None;
        } else if let Some(draft) = &self.slip_draft
            && self.check_slip_context(&draft.proposal.id()).is_err()
        {
            self.invalidate_slip("Project changed; reopen Slip to capture its target.");
        }
    }
}
