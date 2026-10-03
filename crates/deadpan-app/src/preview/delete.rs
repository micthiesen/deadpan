//! A cut captures one exact deletion target and updates the register after save.

use super::*;

mod frames;
pub(super) use frames::FrameTarget;

#[derive(Clone)]
pub(super) struct CommandTarget {
    base: Arc<Workspace>,
    scope: SequenceScope,
    parent: NodeId,
    edit: CapturedEdit,
    register: Option<char>,
    attempt: Option<crate::project::semantic::CutAttempt>,
}

#[derive(Clone)]
enum CapturedEdit {
    TimeOrBeat(Box<ProjectEdit>),
    Object {
        capture: macros::Capture,
        label: String,
    },
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
        let selection = self.edit_selection();
        if selection == navigation::EditSelection::Object {
            return Ok(CommandTarget {
                base,
                scope,
                parent,
                edit: CapturedEdit::Object {
                    capture: self.capture_macro_target()?,
                    label: self
                        .edit_range_label()
                        .ok_or("The group selection changed.")?,
                },
                register: self.copied.selected(),
                attempt: None,
            });
        }
        let edit = match selection {
            navigation::EditSelection::Object => unreachable!("object captured above"),
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
            edit: CapturedEdit::TimeOrBeat(Box::new(edit)),
            register: self.copied.selected(),
            attempt: Some(crate::project::semantic::CutAttempt {
                operation: crate::project::semantic::RepeatableCut::Selector(match selection {
                    navigation::EditSelection::Range => {
                        deadpan_core::SemanticSelector::VisualSelection
                    }
                    navigation::EditSelection::None => deadpan_core::SemanticSelector::SelectedBeat,
                    navigation::EditSelection::Empty | navigation::EditSelection::Object => {
                        unreachable!("empty deletion rejected above")
                    }
                }),
                repeat_version: None,
            }),
        })
    }

    pub(super) fn delete_captured(&mut self, captured: Result<CommandTarget, String>) {
        let captured = match captured {
            Ok(CommandTarget {
                edit: CapturedEdit::Object { capture, .. },
                register,
                ..
            }) => {
                self.copied.begin_write();
                let instruction = register
                    .map_or(
                        Ok(deadpan_core::RegisterName::unnamed()),
                        deadpan_core::RegisterName::new,
                    )
                    .map(|register| deadpan_core::SemanticInstruction::CutSelection { register })
                    .map_err(|error| error.to_string());
                self.apply_recorded_instruction(Ok(capture), instruction);
                return;
            }
            other => other,
        };
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
            let CapturedEdit::TimeOrBeat(edit) = target.edit else {
                return Err("The captured object needs a semantic cut.".into());
            };
            let selection = match *edit {
                ProjectEdit::Delete { node } => deadpan_core::SliceCaptureSelection::Child { node },
                ProjectEdit::DeleteRange { range, .. } => {
                    deadpan_core::SliceCaptureSelection::Range { range }
                }
                _ => return Err("The captured command is not a picture cut.".into()),
            };
            Ok((
                target.attempt,
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
            Ok((attempt, mut request)) => {
                let Some(serial) = self.next_serial() else {
                    return;
                };
                request.id.request = serial;
                let recorded_attempt = attempt.clone();
                let command = match attempt {
                    Some(attempt) => ProjectRequest::CutFrames {
                        capture: request.clone(),
                        attempt,
                    },
                    None => ProjectRequest::CutEditSlice(request.clone()),
                };
                if self.submit(command) {
                    self.record_macro_cut(&request, recorded_attempt.as_ref());
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
                CapturedEdit::TimeOrBeat(edit) => match edit.as_ref() {
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
                CapturedEdit::Object { label, .. } => {
                    format!("Cut captured {label} · linked contents · one undo")
                }
            },
        })
    }
}
