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
    let request = CommandRequest {
        project_id: target.project.clone(),
        expected_revision: target.revision.clone(),
        new_revision: new_revision.clone(),
        command: Command::EditScopedMany { edits, identities },
    };
    let prepared =
        deadpan_core::prepare_scoped_edit(&workspace.document, &request).map_err(display)?;
    let presentation = target
        .presentation
        .as_ref()
        .map(|instance| prepared.map_instance(&workspace.document, instance))
        .transpose()
        .map_err(display)?;
    let also = target
        .also
        .iter()
        .map(|other| prepared.map_target(&workspace.document, other))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(display)?;
    Ok(Prepared {
        request,
        receipt: Commit {
            before: target.clone(),
            revision: new_revision,
            target: prepared.target,
            presentation,
            also: Some(also),
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
        also: None,
    };
    Ok(Prepared { request, receipt })
}

/// The occurrence's exact root interval. A speed change or crop between the
/// play and the beat would make its local clock differ from the Edit clock.
fn occurrence_interval(
    document: &deadpan_core::ProjectDocument,
    instance: &deadpan_core::InstancePath,
) -> Result<(i64, i64)> {
    let durations = document.durations().map_err(display)?;
    let length = durations
        .get(&instance.node)
        .ok_or("The selected beat no longer exists.")?
        .frames();
    let index = deadpan_core::AnchorIndex::new(document).map_err(display)?;
    let boundary = |position: i64, bias| {
        index
            .resolve_target(&deadpan_core::AnchorTarget {
                boundary: deadpan_core::BoundaryAnchor {
                    coordinate: deadpan_core::Anchor::Occurrence {
                        instance: instance.clone(),
                        position: deadpan_core::ExactRatio::integer(position),
                    },
                    bias,
                },
                occurrence: None,
            })
            .map_err(display)
    };
    let start = boundary(0, deadpan_core::InsertionBias::Right)?;
    let end = boundary(length, deadpan_core::InsertionBias::Left)?;
    if start.exact_frame.denominator() != 1
        || end.exact_frame.denominator() != 1
        || end.frame.0 - start.frame.0 != length
    {
        return Err("This beat plays inside a speed change or crop in this play; splitting, deleting or repeating it there is not supported yet.".into());
    }
    Ok((start.frame.0, end.frame.0))
}

impl Service {
    pub(super) fn insert_scoped_pause(
        &mut self,
        target: Target,
        duration: deadpan_core::FrameDuration,
        black: bool,
        ai: bool,
    ) -> Result<()> {
        self.check_scoped_head(&target)?;
        let bank = if target.also.is_empty() {
            None
        } else {
            Some(self.writer()?.register_version().map_err(display)?)
        };
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        target.validate(workspace)?;
        let shared = target
            .target
            .repeats
            .iter()
            .any(|step| matches!(step.branch, deadpan_core::RepeatEditBranch::Default));
        if black && ai {
            return Err("An AI pause requires its captured freeze fallback.".into());
        }
        if duration == deadpan_core::FrameDuration::ZERO {
            self.message = Some("Pause resolves to 0 frames; no edit was made.".into());
            return Ok(());
        }
        let (request, document, mapped, presentation, boundary, also) = if let Some(bank) = bank {
            let prepared = crate::project::pause::batch::prepare(
                workspace, &target, duration, black, ai, bank,
            )?;
            (
                prepared.request,
                prepared.document,
                prepared.target,
                prepared.presentation,
                prepared.boundary.frame,
                Some(prepared.also),
            )
        } else {
            let request =
                crate::project::pause::prepare_scoped(workspace, &target, duration, black, ai)?;
            let prepared = deadpan_core::prepare_occurrence_time(&workspace.document, &request)
                .map_err(display)?;
            (
                request,
                prepared.document,
                prepared.target,
                prepared.presentation,
                prepared.boundary.frame,
                None,
            )
        };
        let receipt = Commit {
            before: target.clone(),
            revision: request.new_revision.clone(),
            target: mapped,
            presentation: Some(presentation),
            also,
        };
        receipt.target.validate(&document).map_err(display)?;
        let outcome = self.writer()?.commit(&request).map_err(display)?;
        self.capture_preparation_notices(&outcome.generation_preparation_notices);
        self.committed = Some(CommittedEdit {
            revision: outcome.revision_id,
            selected_node: Some(target.root),
            preserve_cursor: true,
            cursor: Some(boundary),
            scope: target.scope,
            sound: None,
            range_selection: None,
            scoped: Some(receipt),
        });
        let saved = if !target.also.is_empty() {
            format!(
                "Pause inserted in {} selected plays and saved",
                target.also.len() + 1
            )
        } else if shared {
            "Pause inserted in the selected shared scope and saved".into()
        } else {
            "Pause inserted in This play and saved".into()
        };
        self.refresh_saved(&saved)?;
        self.message = Some(saved);
        Ok(())
    }

    /// Split, delete or repeat one beat, or set a Repeat's plays, in the
    /// captured explicit play. The inspector continues on the right fragment,
    /// the new or recounted Repeat, or the deleted beat's neighbor, in the
    /// same play.
    pub(super) fn edit_scoped_structure(
        &mut self,
        target: Target,
        edit: crate::project::ScopedStructure,
    ) -> Result<()> {
        use crate::project::ScopedStructure;
        self.check_scoped_head(&target)?;
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        target.validate(workspace)?;
        if !target.also.is_empty() {
            return Err("Several selected plays take values and pauses only. Choose one play with :scope play N to split, delete or repeat a beat in it.".into());
        }
        if target.target.repeats.is_empty() || !target.has_single_play_authority() {
            return Err("Splitting, deleting or repeating a beat inside Repeat contents needs This play (]r) at every enclosing Repeat. All plays keeps the shared definition; edit it from the parent.".into());
        }
        let document = &workspace.document;
        let instance = deadpan_core::InstancePath {
            node: target.target.node.clone(),
            repeats: target
                .target
                .repeats
                .iter()
                .map(|step| match &step.branch {
                    deadpan_core::RepeatEditBranch::Play { iteration } => {
                        Ok(deadpan_core::RepeatInstance {
                            node: step.repeat.clone(),
                            iteration: iteration.clone(),
                        })
                    }
                    deadpan_core::RepeatEditBranch::Default => {
                        Err("A one-play edit lost its explicit play.".to_owned())
                    }
                })
                .collect::<Result<Vec<_>>>()?,
        };
        if target
            .presentation
            .as_ref()
            .is_some_and(|shown| shown != &instance)
        {
            return Err(
                "The shown play differs from the selected beat; choose the play again.".into(),
            );
        }
        let (start, end) = occurrence_interval(document, &instance)?;
        let new_revision = revision();
        let timing = deadpan_core::AudioTimingId {
            allocation: new_revision.clone(),
            ordinal: 0,
        };
        let (edit, cursor, saved) = match edit {
            ScopedStructure::Split => {
                if target.cursor.0 <= start || target.cursor.0 >= end {
                    return Err("Move inside the selected beat to split it; existing boundaries need no split.".into());
                }
                // The split pool covers the beat's subtree, both Partitions,
                // a root context and a container; unused names are dropped.
                let mut count = 3_usize;
                let mut pending = vec![instance.node.clone()];
                while let Some(id) = pending.pop() {
                    count = count
                        .checked_add(1)
                        .filter(|count| *count <= deadpan_core::MAX_DOCUMENT_NODES)
                        .ok_or("Split exceeds the document node limit")?;
                    pending.extend(document.children(&id).cloned());
                }
                (
                    deadpan_core::OccurrenceEdit::Split {
                        at: deadpan_core::FrameDuration::new(target.cursor.0 - start)
                            .map_err(display)?,
                        identities: deadpan_core::SplitIdentities {
                            nodes: (0..count).map(|_| node()).collect(),
                        },
                    },
                    target.cursor,
                    "Beat split in this play only and saved".to_owned(),
                )
            }
            ScopedStructure::Delete => (
                deadpan_core::OccurrenceEdit::DeleteRipple { timing },
                deadpan_core::ProjectFrame(start),
                "Beat deleted from this play only and saved (not copied to a register)".to_owned(),
            ),
            ScopedStructure::Repeat { plays } => {
                if plays == 0 {
                    return Err("A Repeat needs at least one total play.".into());
                }
                (
                    deadpan_core::OccurrenceEdit::RepeatChild {
                        id: node(),
                        plays,
                        timing,
                    },
                    deadpan_core::ProjectFrame(start),
                    format!("Beat repeated {plays} times in this play only and saved"),
                )
            }
            ScopedStructure::SetPlays { plays } => (
                deadpan_core::OccurrenceEdit::SetRepeatPlays { plays, timing },
                deadpan_core::ProjectFrame(start),
                format!("Repeat set to {plays} total plays in this play only and saved"),
            ),
        };
        let needs = document
            .occurrence_structure_requirements(&instance, &edit)
            .map_err(display)?;
        let identities = pool(needs.nodes, needs.marks);
        let (selected, presentation) = document
            .occurrence_structure_selection(&instance, &edit, &identities)
            .map_err(display)?;
        let request = CommandRequest {
            project_id: target.project.clone(),
            expected_revision: target.revision.clone(),
            new_revision: new_revision.clone(),
            command: Command::EditOccurrence {
                instance,
                edit,
                identities,
            },
        };
        let receipt = Commit {
            before: target.clone(),
            revision: new_revision,
            target: selected,
            presentation: Some(presentation),
            also: None,
        };
        let outcome = self.writer()?.commit(&request).map_err(display)?;
        self.capture_preparation_notices(&outcome.generation_preparation_notices);
        self.committed = Some(CommittedEdit {
            revision: outcome.revision_id,
            selected_node: Some(target.root),
            preserve_cursor: true,
            cursor: Some(cursor),
            scope: target.scope,
            sound: None,
            range_selection: None,
            scoped: Some(receipt),
        });
        self.refresh_saved(&saved)?;
        self.message = Some(saved);
        Ok(())
    }

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
        self.capture_preparation_notices(&outcome.generation_preparation_notices);
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
