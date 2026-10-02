//! A cut captures one exact deletion target and updates the register after save.

use super::*;

mod frames;
pub(super) use frames::FrameTarget;

#[derive(Clone)]
pub(super) struct CommandTarget {
    base: Arc<Workspace>,
    scope: SequenceScope,
    parent: NodeId,
    edit: ProjectEdit,
    register: Option<char>,
}

impl DeadpanApp {
    pub(super) fn capture_delete_target(&self) -> Result<CommandTarget, String> {
        if self.view != View::Sequence
            || self.sound_focused()
            || self.pane == Pane::Sounds
            || self.event_focused()
        {
            return Err("Return to Your edit and focus Beats before deleting picture time. Use :sound-delete for a placed sound.".into());
        }
        let base = self
            .workspace
            .clone()
            .ok_or("Open a project before deleting time.")?;
        let scope = self.sequence_scope.clone();
        let parent = scope.resolve(&base)?.owner.clone();
        let edit = match self.edit_selection() {
            navigation::EditSelection::Empty => return Err("The Edit selection is empty. Move its boundary or press Esc before deleting a beat; no edit was made.".into()),
            navigation::EditSelection::Range => ProjectEdit::DeleteRange {
                parent: parent.clone(),
                range: self.selected_edit_range().ok_or("The Edit selection changed; select it again.")?,
            },
            navigation::EditSelection::None => {
                let node = self.selected_beat.clone().ok_or("Select a beat or nonempty Edit range before opening :delete; no edit was made.")?;
                if !scope.resolve(&base)?.children.contains(&node) {
                    return Err("Select a direct child of the displayed group before deleting.".into());
                }
                ProjectEdit::Delete { node }
            }
        };
        Ok(CommandTarget {
            base,
            scope,
            parent,
            edit,
            register: self.copied.selected(),
        })
    }

    pub(super) fn delete_captured(&mut self, captured: Result<CommandTarget, String>) {
        self.cancel_repeats("deletion was requested");
        self.bindings.clear();
        self.copied.supersede();
        self.copied.clear_selection();
        let result = captured.and_then(|target| {
            let workspace = self
                .workspace
                .as_ref()
                .ok_or("The captured deletion project is closed.")?;
            if workspace.session != target.base.session
                || workspace.document.project_id() != target.base.document.project_id()
                || workspace.document.revision_id() != target.base.document.revision_id()
                || self.sequence_scope != target.scope
                || target.scope.resolve(workspace)?.owner != &target.parent
            {
                return Err(
                    "The captured deletion target changed. Start the cut again; no edit was made."
                        .into(),
                );
            }
            let selection = match target.edit {
                ProjectEdit::Delete { node } => deadpan_core::SliceCaptureSelection::Child { node },
                ProjectEdit::DeleteRange { range, .. } => {
                    deadpan_core::SliceCaptureSelection::Range { range }
                }
                _ => return Err("The captured command is not a picture cut.".into()),
            };
            Ok((
                target.register,
                crate::project::slice::CaptureRequest {
                    id: crate::project::slice::CopyId {
                        session: target.base.session,
                        project: target.base.document.project_id().clone(),
                        source_revision: target.base.document.revision_id().clone(),
                        request: 0,
                        persisted_version: None,
                    },
                    register: target.register,
                    scope: target.scope,
                    parent: target.parent,
                    selection,
                },
            ))
        });
        match result {
            Ok((_, mut request)) => {
                let Some(serial) = self.next_serial() else {
                    return;
                };
                request.id.request = serial;
                if self.submit(ProjectRequest::CutEditSlice(request.clone())) {
                    self.copied.expect_cut_to(request);
                    self.message = Some("Saving cut and copy…".into());
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn delete_hint(&self) -> Option<String> {
        if !matches!(
            navigation::command::parse(&self.command),
            Ok(navigation::command::Entry::Action(Action::Edit(
                BeatEdit::Delete
            )))
        ) {
            return None;
        }
        Some(match self.delete_command_target.as_ref()? {
            Err(error) => error.clone(),
            Ok(target) => match &target.edit {
                ProjectEdit::DeleteRange { range, .. } => format!(
                    "Cut captured Edit [{}..{}) · {} f · linked picture + sound · one undo",
                    range.start().0,
                    range.end().0,
                    range.duration().frames()
                ),
                ProjectEdit::Delete { node } => format!(
                    "Cut captured beat: {} · linked picture + sound · one undo",
                    target.base.document.nodes()[node].label
                ),
                _ => unreachable!("deletion captures only deletion operations"),
            },
        })
    }
}
