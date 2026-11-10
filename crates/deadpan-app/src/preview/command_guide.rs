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
