//! `:reverse`, `:ping-pong`, `:tail` and `,t`: pauses that replay the moment
//! before the cursor backwards or let its sound ring on. Each is one recorded
//! semantic instruction, so macros, dot-free replay and the headless path
//! author the same Hold as the keys.

use super::*;
use crate::navigation::duration::DurationInput;
use deadpan_core::{NodeKind, SemanticInstruction, TailEffect};

impl DeadpanApp {
    /// `,t`: open `:tail` with its length ready to change. On a selected
    /// pause the length is how long the tail rings (the whole pause); with
    /// none selected a new tail pause is inserted at the cursor.
    pub(super) fn pick_tail(&mut self, context: &egui::Context) {
        if self.view != View::Sequence {
            self.error = Some(
                "Add a tail in Your edit: place the cursor after the sound, then press ,t.".into(),
            );
            return;
        }
        let selected_hold = self
            .workspace
            .as_ref()
            .zip(self.selected_beat.as_ref())
            .and_then(
                |(workspace, node)| match &workspace.document.nodes().get(node)?.kind {
                    NodeKind::Hold { recipe } => Some(recipe.duration.frames()),
                    _ => None,
                },
            );
        let length = selected_hold.map_or_else(
            || crate::navigation::hold_effects::DEFAULT_TAIL.to_owned(),
            |frames| format!("{frames}f"),
        );
        self.open_command(format!("tail {length} effect=reverb"), context);
    }

    pub(super) fn apply_reverse(&mut self, length: DurationInput, bounce: bool) {
        self.cancel_repeats("a reverse was requested");
        let target = self.capture_macro_target();
        let instruction = self.pause_instruction(|rate| {
            Ok(SemanticInstruction::InsertReverse {
                length: length.pause_length(rate)?,
                bounce,
            })
        });
        self.apply_recorded_instruction(target, instruction);
    }

    pub(super) fn apply_tail(&mut self, length: Option<DurationInput>, effect: TailEffect) {
        self.cancel_repeats("a tail was requested");
        let target = self.capture_macro_target();
        let on_hold = self
            .workspace
            .as_ref()
            .zip(self.selected_beat.as_ref())
            .is_some_and(|(workspace, node)| {
                matches!(
                    workspace.document.nodes().get(node).map(|node| &node.kind),
                    Some(NodeKind::Hold { .. })
                )
            });
        let instruction = if on_hold {
            self.workspace
                .as_ref()
                .ok_or_else(|| "Open a project first.".to_owned())
                .and_then(|workspace| {
                    let rate = workspace.document.presentation_basis().frame_rate;
                    Ok(SemanticInstruction::Tail {
                        length: length.map(|length| length.pause_length(rate)).transpose()?,
                        effect,
                    })
                })
        } else {
            self.pause_instruction(|rate| {
                Ok(SemanticInstruction::Tail {
                    length: Some(
                        length
                            .ok_or("Give the new tail pause a length, for example :tail 400ms.")?
                            .pause_length(rate)?,
                    ),
                    effect,
                })
            })
        };
        self.apply_recorded_instruction(target, instruction);
    }

    /// `:lift`: cut the Visual range into the selected register (or the
    /// unnamed one) and refill its time with a silent black pause, as one
    /// recorded instruction and one Undo.
    pub(super) fn lift_selection(&mut self) {
        self.cancel_repeats("a lift was requested");
        let target = self.capture_macro_target();
        let instruction = if self.view != View::Sequence {
            Err("Select a range in Your edit with v, then :lift it.".to_owned())
        } else if self.edit_selection() == crate::navigation::EditSelection::None {
            Err("Select the range to lift with v first.".to_owned())
        } else {
            deadpan_core::RegisterName::new(self.copied.selected().unwrap_or('"'))
                .map(|register| SemanticInstruction::Lift { register })
                .map_err(|error| error.message)
        };
        self.apply_recorded_instruction(target, instruction);
    }

    /// `,b` / `:bleep`: replace the Visual range's sound with a tone while its
    /// pictures keep playing, as one recorded instruction and one Undo. The
    /// cut content goes to the selected (or unnamed) register.
    pub(super) fn bleep_selection(&mut self, frequency_hz: u32, level_millidecibels: i32) {
        self.cancel_repeats("a bleep was requested");
        let target = self.capture_macro_target();
        let instruction = if self.view != View::Sequence {
            Err("Select a range in Your edit with v, then bleep it with ,b.".to_owned())
        } else if self.edit_selection() == crate::navigation::EditSelection::None {
            Err("Select the range to bleep with v (or a word with viw) first.".to_owned())
        } else {
            deadpan_core::RegisterName::new(self.copied.selected().unwrap_or('"'))
                .map_err(|error| error.message)
                .and_then(|register| {
                    Ok(SemanticInstruction::Bleep {
                        register,
                        frequency_hz,
                        level: deadpan_core::GainDb::new(level_millidecibels)
                            .map_err(|error| error.to_string())?,
                    })
                })
        };
        self.apply_recorded_instruction(target, instruction);
    }

    /// An instruction that inserts a pause at the Edit cursor, after the
    /// same scope check as `,h`.
    fn pause_instruction(
        &self,
        build: impl FnOnce(deadpan_core::FrameRate) -> Result<SemanticInstruction, String>,
    ) -> Result<SemanticInstruction, String> {
        if self.view != View::Sequence {
            return Err("Place the Edit cursor in Your edit first.".into());
        }
        let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
        self.sequence_scope
            .check_pause(workspace, ProjectFrame(self.sequence_cursor as i64))?;
        build(workspace.document.presentation_basis().frame_rate)
    }
}
