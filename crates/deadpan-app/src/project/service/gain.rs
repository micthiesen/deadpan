//! Produce a validated uncommitted document with committed media capabilities.

use super::*;

impl Service {
    pub(super) fn prepare_gain(
        &mut self,
        proposal: &super::super::gain::Proposal,
    ) -> Result<Arc<deadpan_playback::Snapshot>> {
        let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
        proposal.target.validate(workspace)?;
        if proposal.draft == 0 || proposal.change == 0 {
            return Err("Gain proposal identities must be nonzero.".into());
        }
        if let Some(target) = &proposal.target.scoped {
            self.check_scoped_head(target)?;
        }
        let request = if let Some(target) = &proposal.target.scoped
            && proposal.treatments != proposal.target.entry
        {
            scoped::prepare(
                workspace,
                target,
                &deadpan_core::ScopedNodeEdit::SetAudioTreatments {
                    treatments: proposal.treatments.clone(),
                },
            )?
            .request
        } else {
            // An unchanged proposal still needs a private preview revision,
            // but must not isolate a play. This identical value command leaves
            // every authored field intact and is never committed here.
            CommandRequest {
                project_id: proposal.target.project.clone(),
                expected_revision: proposal.target.revision.clone(),
                new_revision: revision(),
                command: Command::SetAudioTreatments {
                    node: proposal.target.node.clone(),
                    treatments: proposal.treatments.clone(),
                },
            }
        };
        let edit = self
            .store
            .as_ref()
            .ok_or("Open a project first.")?
            .preview(&request)
            .map_err(display)?;
        let document = Arc::new(edit.forward.apply(&workspace.document).map_err(display)?);
        let snapshot = deadpan_playback::Snapshot::proposed(
            &workspace.playback_snapshot(),
            document,
            proposal.draft,
            proposal.change,
        )
        .map_err(display)?;
        Ok(Arc::new(snapshot))
    }
}
