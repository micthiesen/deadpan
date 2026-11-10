//! The live context command-line argument guidance reads: what a command
//! would act on now and the names the project and library hold.

use super::*;
use crate::navigation::command::schema;

impl DeadpanApp {
    pub(super) fn command_context(&mut self) -> schema::Context {
        let presets = self
            .command_gag_presets
            .get_or_insert_with(|| {
                // Read once per command entry, never per frame.
                self.gag_presets
                    .load()
                    .ok()
                    .map(|loaded| loaded.presets.into_keys().collect())
            })
            .clone();
        let workspace = self.workspace.as_deref();
        let beat = self
            .beat_rows
            .iter()
            .find(|beat| Some(&beat.id) == self.selected_beat.as_ref())
            .map(|beat| schema::Beat {
                label: beat.label.clone(),
                kind: beat.kind.clone(),
                frames: beat.frames,
                gag: deadpan_core::GagRecipe::from_label(&beat.label).is_some(),
            });
        let visual = (self.edit_selection() != navigation::EditSelection::None)
            .then(|| self.selected_edit_range())
            .flatten()
            .map(|range| {
                format!(
                    "[{}..{}) {} f",
                    range.start().0,
                    range.end().0,
                    range.duration().frames()
                )
            });
        let sound = self
            .selected_event
            .as_ref()
            .and_then(|id| Some(workspace?.document.sounds().get(id)?.label.clone()));
        schema::Context {
            original: self.view == View::Source,
            visual,
            beat,
            cursor: Some(format!("frame {}", self.sequence_cursor)),
            contents: self.scoped.is_some().then(|| self.beat_scope_label()),
            sound,
            registers: self
                .copied
                .entries()
                .filter(|(name, _)| *name != '"')
                .map(|(name, content)| {
                    (
                        name,
                        match content {
                            copied::Content::Original(_) => "Original",
                            copied::Content::Edited(_) | copied::Content::EditedSet(_) => "edited",
                            copied::Content::Macro(_) => "macro",
                        },
                    )
                })
                .collect(),
            targets: workspace
                .map(|workspace| {
                    workspace
                        .document
                        .targets()
                        .iter()
                        .map(|(id, target)| (id.as_str().to_owned(), target.label.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            gag_presets: presets,
            marks: workspace
                .map(|workspace| {
                    workspace
                        .document
                        .marks()
                        .keys()
                        .filter_map(|id| {
                            let mut letters = id.as_str().strip_prefix("native-mark-")?.chars();
                            match (letters.next(), letters.next()) {
                                (Some(letter), None) => Some(letter),
                                _ => None,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

impl DeadpanApp {
    /// Tab, Shift+Tab, Down and Up complete in the focused command line,
    /// never during IME composition (the composition owns those keys).
    /// Down and Up step only while there is something to complete, so they
    /// keep their native meaning otherwise. Returns whether the step goes
    /// backwards.
    pub(super) fn command_tab_key(
        &mut self,
        context: &egui::Context,
        key: egui::Key,
        modifiers: egui::Modifiers,
        ime: bool,
    ) -> Option<bool> {
        let focused = context.memory(|memory| memory.has_focus(egui::Id::new(COMMAND_ID)));
        let reverse = command_tab_admitted(self.command_open, focused, ime, key, modifiers)?;
        if key == egui::Key::Tab {
            return Some(reverse);
        }
        let cycling = self
            .command_cycle
            .as_ref()
            .is_some_and(|cycle| cycle.current(&self.command).is_some());
        (cycling || {
            let schema_context = self.command_context();
            navigation::command::complete::candidates(&self.command, &schema_context).is_some()
        })
        .then_some(reverse)
    }

    /// Applies a pending Tab step after the field processed this frame's
    /// text. Puts the caret at the end of the completed text. Returns whether
    /// the command text changed.
    pub(super) fn apply_command_tab(&mut self, context: &egui::Context) -> bool {
        let Some(reverse) = self.command_tab.take() else {
            return false;
        };
        let schema_context = self.command_context();
        let Some(text) = navigation::command::complete::step(
            &mut self.command_cycle,
            &self.command,
            &schema_context,
            reverse,
        ) else {
            return false;
        };
        self.command = text;
        let id = egui::Id::new(COMMAND_ID);
        let mut state = egui::text_edit::TextEditState::load(context, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(self.command.chars().count()),
            )));
        state.store(context, id);
        // The field painted the text before this step; show the result.
        context.request_repaint();
        true
    }
}

/// The completion step a command-line key asks for: forwards, or backwards.
pub(super) fn command_tab_step(key: egui::Key, modifiers: egui::Modifiers) -> Option<bool> {
    match (key, modifiers) {
        (egui::Key::Tab, egui::Modifiers::NONE) | (egui::Key::ArrowDown, egui::Modifiers::NONE) => {
            Some(false)
        }
        (egui::Key::Tab, egui::Modifiers::SHIFT) | (egui::Key::ArrowUp, egui::Modifiers::NONE) => {
            Some(true)
        }
        _ => None,
    }
}

/// A completion step, only for the open and focused command line outside
/// IME composition.
pub(super) fn command_tab_admitted(
    open: bool,
    focused: bool,
    composing: bool,
    key: egui::Key,
    modifiers: egui::Modifiers,
) -> Option<bool> {
    (open && focused && !composing)
        .then(|| command_tab_step(key, modifiers))
        .flatten()
}
