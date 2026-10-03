//! Prepare branch isolation and its exact continuation before publishing once.

use deadpan_core::{MarkId, OccurrenceIdentities, ScopedNodeEdit};

use super::*;
use crate::project::scoped::{Commit, Target};

pub(super) struct Prepared {
    pub request: CommandRequest,
    pub receipt: Commit,
}

pub(super) fn prepare(
    workspace: &Workspace,
    target: &Target,
    edit: &ScopedNodeEdit,
) -> Result<Prepared> {
    target.validate(workspace)?;
    let requirements = workspace
        .document
        .scoped_edit_requirements(&target.target, edit)
        .map_err(display)?;
    let request = CommandRequest {
        project_id: target.project.clone(),
        expected_revision: target.revision.clone(),
        new_revision: revision(),
        command: Command::EditScoped {
            target: target.target.clone(),
            edit: edit.clone(),
            identities: OccurrenceIdentities {
                nodes: (0..requirements.nodes).map(|_| node()).collect(),
                marks: (0..requirements.marks)
                    .map(|_| {
                        MarkId::new(uuid::Uuid::new_v4().to_string())
                            .expect("UUID is a valid mark identity")
                    })
                    .collect(),
            },
        },
    };
    let prepared =
        deadpan_core::prepare_scoped_edit(&workspace.document, &request).map_err(display)?;
    let presentation = target
        .presentation
        .as_ref()
        .map(|instance| prepared.map_instance(&workspace.document, instance))
        .transpose()
        .map_err(display)?;
    if let Some(instance) = &presentation
        && !prepared
            .target
            .matches_instance(&prepared.document, instance)
            .map_err(display)?
    {
        return Err("Prepared scoped presentation does not match its edited owner.".into());
    }
    let receipt = Commit {
        before: target.clone(),
        revision: request.new_revision.clone(),
        target: prepared.target.clone(),
        presentation,
    };
    Ok(Prepared { request, receipt })
}

impl Service {
    pub(super) fn check_scoped_head(&self, target: &Target) -> Result<()> {
        let head = self
            .store
            .as_ref()
            .ok_or("Open a project first")?
            .head_revision()
            .map_err(display)?;
        if head != target.revision {
            return Err("The saved project changed; reopen this inspector before editing.".into());
        }
        Ok(())
    }

    pub(super) fn edit_scoped(&mut self, target: Target, edit: ScopedNodeEdit) -> Result<()> {
        self.check_scoped_head(&target)?;
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        target.validate(workspace)?;
        if workspace
            .document
            .scoped_edit_requirements(&target.target, &edit)
            .map_err(display)?
            .unchanged
        {
            self.message = Some("Scoped value is unchanged. No edit was made.".into());
            return Ok(());
        }
        let prepared = prepare(workspace, &target, &edit)?;
        let outcome = self.writer()?.commit(&prepared.request).map_err(display)?;
        self.committed = Some(CommittedEdit {
            revision: outcome.revision_id,
            selected_node: Some(target.root),
            preserve_cursor: true,
            cursor: Some(target.cursor),
            scope: target.scope,
            sound: None,
            range_selection: None,
            scoped: Some(prepared.receipt),
        });
        self.refresh_saved("Scoped edit saved")?;
        self.message = Some("Scoped edit saved".into());
        Ok(())
    }
}
