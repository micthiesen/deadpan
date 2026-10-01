//! Deletion captures its own target, independently of the Original copy register.

use super::*;

#[derive(Clone)]
pub(super) struct CommandTarget {
    base: Arc<Workspace>,
    scope: SequenceScope,
    parent: NodeId,
    cursor: ProjectFrame,
    edit: ProjectEdit,
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
        let cursor = ProjectFrame(
            i64::try_from(self.sequence_cursor)
                .map_err(|_| "Edit cursor exceeds the project frame range")?,
        );
        Ok(CommandTarget {
            base,
            scope,
            parent,
            cursor,
            edit,
        })
    }

    pub(super) fn delete_captured(&mut self, captured: Result<CommandTarget, String>) {
        self.cancel_repeats("deletion was requested");
        self.bindings.clear();
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
                    "The captured deletion target changed. Open :delete again; no edit was made."
                        .into(),
                );
            }
            Ok(ProjectRequest::Edit {
                expected_session: target.base.session,
                expected_revision: target.base.document.revision_id().clone(),
                scope: target.scope,
                cursor: target.cursor,
                edit: target.edit,
            })
        });
        match result {
            Ok(request) => {
                self.submit(request);
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
