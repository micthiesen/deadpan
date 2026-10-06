//! Prepare branch isolation and its exact continuation before publishing once.

use deadpan_core::{MarkId, OccurrenceIdentities, ScopedNodeEdit};

use super::*;
use crate::project::scoped::{Commit, Target};

pub(super) struct Prepared {
    pub request: CommandRequest,
    pub receipt: Commit,
}

fn pool(nodes: usize, marks: usize) -> OccurrenceIdentities {
    OccurrenceIdentities {
        nodes: (0..nodes).map(|_| node()).collect(),
        marks: (0..marks)
            .map(|_| {
                MarkId::new(uuid::Uuid::new_v4().to_string())
                    .expect("UUID is a valid mark identity")
            })
            .collect(),
    }
}

/// One value per selected play: the chosen value for the inspected play and
/// its carried-over counterpart for every further play (`:scope plays`).
pub(super) fn many_edits(
    workspace: &Workspace,
    target: &Target,
    edit: &ScopedNodeEdit,
) -> Result<Vec<deadpan_core::ScopedTargetEdit>> {
    let document = &workspace.document;
    let entry = &document.nodes()[&target.target.node];
    let mut edits = vec![deadpan_core::ScopedTargetEdit {
        target: target.target.clone(),
        edit: edit.clone(),
    }];
    for also in &target.also {
        let number = target
            .target
            .repeats
            .iter()
            .zip(&also.repeats)
            .find_map(|(mine, theirs)| match (&mine.branch, &theirs.branch) {
                (_, deadpan_core::RepeatEditBranch::Play { iteration })
                    if mine.branch != theirs.branch =>
                {
                    match &document.nodes()[&theirs.repeat].kind {
                        NodeKind::Repeat { iterations, .. } => iterations
                            .position(iteration)
                            .and_then(|index| index.checked_add(1)),
                        _ => None,
                    }
                }
                _ => None,
            })
            .unwrap_or(0);
        let other = &document.nodes()[&also.node];
        edits.push(deadpan_core::ScopedTargetEdit {
            target: also.clone(),
            edit: crate::project::scoped::transfer(edit, entry, other, number)?,
        });
    }
    Ok(edits)
}

/// Whether no selected play would change.
pub(super) fn unchanged(
    workspace: &Workspace,
    target: &Target,
    edit: &ScopedNodeEdit,
) -> Result<bool> {
    if target.also.is_empty() {
        return Ok(workspace
            .document
            .scoped_edit_requirements(&target.target, edit)
            .map_err(display)?
            .unchanged);
    }
    let edits = many_edits(workspace, target, edit)?;
    Ok(workspace
        .document
        .scoped_many_requirements(&edits)
        .map_err(display)?
        .iter()
        .all(|needs| needs.unchanged))
}

fn prepare_many(workspace: &Workspace, target: &Target, edit: &ScopedNodeEdit) -> Result<Prepared> {
    let edits = many_edits(workspace, target, edit)?;
    let needs = workspace
        .document
        .scoped_many_requirements(&edits)
        .map_err(display)?;
    let identities: Vec<_> = needs
        .iter()
        .map(|needs| {
            if needs.unchanged {
                OccurrenceIdentities::default()
            } else {
                pool(needs.nodes, needs.marks)
            }
        })
        .collect();
    let new_revision = revision();
    // The inspected play is edited first with its own pool, exactly as a
    // single scoped edit would be, so its mapped target is the receipt's.
    let (mapped, presentation) = if needs[0].unchanged {
        (target.target.clone(), target.presentation.clone())
    } else {
        let single = CommandRequest {
            project_id: target.project.clone(),
            expected_revision: target.revision.clone(),
            new_revision: new_revision.clone(),
            command: Command::EditScoped {
                target: target.target.clone(),
                edit: edit.clone(),
                identities: identities[0].clone(),
            },
        };
        let prepared =
            deadpan_core::prepare_scoped_edit(&workspace.document, &single).map_err(display)?;
        let presentation = target
            .presentation
            .as_ref()
            .map(|instance| prepared.map_instance(&workspace.document, instance))
            .transpose()
            .map_err(display)?;
        (prepared.target, presentation)
    };
    let request = CommandRequest {
        project_id: target.project.clone(),
        expected_revision: target.revision.clone(),
        new_revision: new_revision.clone(),
        command: Command::EditScopedMany { edits, identities },
    };
    deadpan_core::apply(&workspace.document, &request).map_err(display)?;
    Ok(Prepared {
        request,
        receipt: Commit {
            before: target.clone(),
            revision: new_revision,
            target: mapped,
            presentation,
        },
    })
}

pub(super) fn prepare(
    workspace: &Workspace,
    target: &Target,
    edit: &ScopedNodeEdit,
) -> Result<Prepared> {
    target.validate(workspace)?;
    if !target.also.is_empty() {
        return prepare_many(workspace, target, edit);
    }
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
        if unchanged(workspace, &target, &edit)? {
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
        let saved = if target.also.is_empty() {
            "Scoped edit saved".to_owned()
        } else {
            format!("Scoped edit saved in {} plays", target.also.len() + 1)
        };
        self.refresh_saved(&saved)?;
        self.message = Some(saved);
        Ok(())
    }
}
