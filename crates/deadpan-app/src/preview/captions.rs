//! Resolve `:caption` against the selected beat and the Edit range into one
//! caption list for the project service, or one recorded semantic caption.
//!
//! Like a cutaway, a caption belongs to a Source or Hold, through any unity
//! Partitions of a Split fragment, in that host's own clock.

use super::*;
use crate::navigation::caption::CaptionInput;
use deadpan_core::{Caption, FrameRange};

impl DeadpanApp {
    pub(super) fn caption_command(&mut self, input: CaptionInput) {
        if self.view != View::Sequence {
            self.error = Some("Select a beat in Your edit, then caption it.".into());
            return;
        }
        let target = self.capture_macro_target();
        // Placing a caption is one semantic instruction over the selected
        // beat or the Edit range inside it, so a macro records it and `.`
        // repeats it. Clearing stays a direct edit outside recordings.
        if self.macros.recording() || matches!(input, CaptionInput::Place { .. }) {
            let instruction = match input {
                CaptionInput::Clear => {
                    Err("Recording places captions; clear them outside a macro.".to_owned())
                }
                CaptionInput::Place {
                    text,
                    placement,
                    delay,
                    reveal,
                } => self
                    .workspace
                    .as_ref()
                    .ok_or_else(|| "Open a project first.".to_owned())
                    .and_then(|workspace| {
                        let rate = workspace.document.presentation_basis().frame_rate;
                        Ok(deadpan_core::SemanticInstruction::SetCaption {
                            text,
                            placement,
                            delay: delay.map(|delay| delay.pause_length(rate)).transpose()?,
                            reveal,
                        })
                    }),
            };
            self.apply_recorded_instruction(target, instruction);
            return;
        }
        let (Some(workspace), Some(node)) = (&self.workspace, self.selected_beat.clone()) else {
            self.error = Some("Select a beat in the current group before captioning it.".into());
            return;
        };
        let edit = match self.caption_edit(&node, input) {
            Ok(edit) => edit,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        self.submit(ProjectRequest::Edit {
            expected_session: workspace.session,
            expected_revision: workspace.document.revision_id().clone(),
            scope: self.sequence_scope.clone(),
            cursor: ProjectFrame(self.sequence_cursor as i64),
            edit,
        });
    }

    fn caption_edit(&self, node: &NodeId, input: CaptionInput) -> Result<ProjectEdit, String> {
        let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
        let row = self
            .beat_rows
            .iter()
            .find(|row| &row.id == node)
            .ok_or("Select a beat in the current group first.")?;
        let (host_id, offset) = deadpan_core::cutaway_host(&workspace.document, node).ok_or(
            "Captions belong to a source or pause beat. Open a group with Enter and select one there.",
        )?;
        // The Edit range, or the whole beat after its delay, in the beat's clock.
        let (start, end) = match self.selected_edit_range() {
            Some(range) => {
                let (start, end) = (range.start().0 as u64, range.end().0 as u64);
                if start < row.start || end > row.start + row.frames || start == end {
                    return Err(
                        "Select a range inside one beat; a caption belongs to that beat.".into(),
                    );
                }
                if matches!(input, CaptionInput::Place { delay: Some(_), .. }) {
                    return Err(
                        "An Edit range already sets where the caption starts; leave out delay=."
                            .into(),
                    );
                }
                (start - row.start, end - row.start)
            }
            None => {
                let delay = match &input {
                    CaptionInput::Place {
                        delay: Some(delay), ..
                    } => delay
                        .resolve(workspace.document.presentation_basis().frame_rate)?
                        .frames() as u64,
                    _ => 0,
                };
                if delay >= row.frames {
                    return Err("The caption delay reaches past the end of the beat.".into());
                }
                (delay, row.frames)
            }
        };
        let range = FrameRange::new(
            ProjectFrame(start as i64 + offset),
            ProjectFrame(end as i64 + offset),
        )
        .map_err(|error| error.to_string())?;
        let existing = &workspace.document.nodes()[&host_id].captions;
        let mut captions = existing.clone();
        match input {
            CaptionInput::Clear => {
                captions.retain(|caption| {
                    !(caption.range.start() < range.end() && range.start() < caption.range.end())
                });
                if captions.len() == existing.len() {
                    return Err("There is no caption here to clear.".into());
                }
            }
            CaptionInput::Place {
                text,
                placement,
                reveal,
                ..
            } => {
                let position =
                    captions.partition_point(|caption| caption.range.start() <= range.start());
                captions.insert(
                    position,
                    Caption {
                        range,
                        text,
                        placement,
                        reveal,
                    },
                );
                if captions.iter().enumerate().any(|(index, caption)| {
                    captions[..index].iter().any(|earlier| {
                        earlier.placement == caption.placement
                            && earlier.range.end() > caption.range.start()
                            && caption.range.end() > earlier.range.start()
                    })
                }) {
                    return Err("This range already has a caption at that placement; :caption clear removes it first.".into());
                }
            }
        }
        Ok(ProjectEdit::SetCaptions {
            node: node.clone(),
            host: host_id,
            captions,
        })
    }
}
