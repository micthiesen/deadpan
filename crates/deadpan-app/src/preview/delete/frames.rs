//! Cursor cuts retain their entry boundary independently of selected beats.

use super::*;

#[derive(Clone)]
pub(in crate::preview) struct FrameTarget {
    base: Arc<Workspace>,
    scope: SequenceScope,
    parent: NodeId,
    cursor: u64,
    end: u64,
    register: Option<char>,
}

impl FrameTarget {
    fn range(&self, count: u32) -> Result<deadpan_core::FrameRange, String> {
        if count == 0 {
            return Err("A frame cut count must be positive; no edit was made.".into());
        }
        let end = self
            .cursor
            .checked_add(u64::from(count))
            .ok_or("The frame cut exceeds the project frame range.")?
            .min(self.end);
        let frame = |value| {
            i64::try_from(value)
                .map(ProjectFrame)
                .map_err(|_| "The frame cut exceeds the project frame range.".to_owned())
        };
        deadpan_core::FrameRange::new(frame(self.cursor)?, frame(end)?)
            .map_err(|error| error.to_string())
    }

    fn command(self, count: u32, repeat_version: Option<u64>) -> Result<CommandTarget, String> {
        let range = self.range(count)?;
        Ok(CommandTarget {
            base: self.base,
            scope: self.scope,
            register: self.register,
            attempt: Some(crate::project::semantic::CutAttempt {
                operation: deadpan_core::FrameCut::new(count).map_err(|error| error.to_string())?,
                repeat_version,
            }),
            parent: self.parent.clone(),
            edit: ProjectEdit::DeleteRange {
                parent: self.parent,
                range,
            },
        })
    }
}

impl DeadpanApp {
    pub(in crate::preview) fn frame_delete_blocked(&self) -> Option<&'static str> {
        if self.close_pending || self.dialogs.is_open() || self.render.blocking() || self.help_open
        {
            Some("Finish the current dialog or help before cutting frames.")
        } else if self.camera.is_some()
            || self.trim.is_some()
            || self.slip.is_some()
            || self.splice.is_some()
            || self.gain.is_some()
            || self.room_tone.is_some()
        {
            Some("Finish or cancel the current preview before cutting frames.")
        } else {
            None
        }
    }

    pub(in crate::preview) fn capture_frame_delete_target(&self) -> Result<FrameTarget, String> {
        // A previous event in this batch can have opened a modal after the
        // outer keyboard guard. Capture absence instead of an editor target.
        if let Some(error) = self.frame_delete_blocked() {
            return Err(error.into());
        }
        if self.view != View::Sequence
            || self.sound_focused()
            || matches!(self.pane, Pane::Sources | Pane::Sounds)
            || self.event_focused()
        {
            return Err("Return to Your edit and focus Beats before cutting frames. Use :sound-delete for a placed sound.".into());
        }
        if self.edit_selection() != navigation::EditSelection::None {
            return Err(format!(
                "Use {} to cut the Edit selection, or {} then {} to cut at the cursor.",
                self.editor_key(EditorKey::CutRange),
                self.editor_key(EditorKey::Escape),
                self.editor_key(EditorKey::CutFrames)
            ));
        }
        let base = self
            .workspace
            .clone()
            .ok_or("Open a project before cutting frames.")?;
        let scope = self.sequence_scope.clone();
        let view = scope.resolve(&base)?;
        let cursor = self.sequence_cursor;
        if cursor < view.start || cursor > view.end {
            return Err("The Edit cursor is outside this group. Move into the group before cutting frames; no edit was made.".into());
        }
        if cursor == view.end {
            return Err("The Edit cursor is at this group's end. Move left before cutting frames; no edit was made.".into());
        }
        let parent = view.owner.clone();
        let end = view.end;
        Ok(FrameTarget {
            base,
            scope,
            parent,
            cursor,
            end,
            register: self.copied.selected(),
        })
    }

    pub(in crate::preview) fn delete_frames_captured(
        &mut self,
        target: Result<FrameTarget, String>,
        count: u32,
    ) {
        let target = match self.frame_delete_blocked() {
            Some(error) => Err(error.into()),
            None => target,
        };
        self.delete_captured(target.and_then(|target| target.command(count, None)));
    }

    pub(in crate::preview) fn repeat_last_edit(&mut self) {
        let target = (|| {
            let workspace = self
                .workspace
                .as_ref()
                .ok_or("Open a project before repeating an edit.")?;
            let snapshot = self
                .semantic
                .snapshot()
                .ok_or("Cut frames first to make an edit available for repeat.")?;
            let edit = snapshot.edit_for(workspace)?;
            let mut target = self.capture_frame_delete_target()?;
            target.register = self.copied.selected_override().unwrap_or(edit.register);
            target.command(edit.operation.count(), Some(snapshot.version))
        })();
        self.delete_captured(target);
    }

    pub(in crate::preview) fn repeat_hint(&self) -> Option<String> {
        let edit = self
            .semantic
            .snapshot()?
            .edit_for(self.workspace.as_ref()?)
            .ok()?;
        Some(format!("repeat cut {}f", edit.operation.count()))
    }

    pub(in crate::preview) fn frame_delete_hint(&self) -> Option<String> {
        let Ok(navigation::command::Entry::Action(Action::DeleteFrames(count))) =
            navigation::command::parse(&self.command)
        else {
            return None;
        };
        Some(match self.frame_delete_command_target.as_ref()? {
            Err(error) => error.clone(),
            Ok(target) => match target.range(count) {
                Err(error) => error,
                Ok(range) => format!(
                    "Cut captured Edit [{}..{}) · {} f{} · linked picture + sound · one undo",
                    range.start().0,
                    range.end().0,
                    range.duration().frames(),
                    if range.duration().frames() < i64::from(count) {
                        " · stopped at group end"
                    } else {
                        ""
                    },
                ),
            },
        })
    }
}
