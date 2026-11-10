//! Sounds attached to a beat (`:sound-attach` stings): a card badge, the
//! inspector's Attached sounds list and its selection, which the existing
//! `:sound-gain`, `:sound-edges` and `:sound-delete` commands then address.
//! The list comes from the shared headless derivation, never a parallel copy.

use deadpan_cli::sound_events::{AttachedSound, attached_sounds};
use std::collections::BTreeMap;

use deadpan_core::{ExactRatio, FrameRate, SoundId};

use super::*;
use crate::navigation::SoundAction;

impl DeadpanApp {
    /// The attached sounds of the selected beat in Your edit.
    pub(super) fn attached_rows(&self) -> Vec<AttachedSound> {
        match (&self.workspace, &self.selected_beat) {
            (Some(workspace), Some(beat)) if self.view == View::Sequence => {
                attached_sounds(&workspace.document, beat)
            }
            _ => Vec::new(),
        }
    }

    /// The selected attached sound, only while it still belongs to the
    /// selected beat in the current workspace.
    pub(super) fn attached_target(&self) -> Option<(NodeId, SoundId)> {
        let selected = self.selected_attached.as_ref()?;
        self.attached_rows()
            .into_iter()
            .any(|sound| (&sound.owner, &sound.id) == (&selected.0, &selected.1))
            .then(|| selected.clone())
    }

    /// The beat's one attached sound, when it has exactly one.
    pub(super) fn only_attached(&self) -> Option<(NodeId, SoundId)> {
        match self.attached_rows().as_slice() {
            [only] => Some((only.owner.clone(), only.id.clone())),
            _ => None,
        }
    }

    /// Audible attached sounds per shown card, for the card badge.
    pub(super) fn attached_badges(&self, beats: &[BeatRow]) -> BTreeMap<NodeId, usize> {
        let Some(workspace) = &self.workspace else {
            return BTreeMap::new();
        };
        beats
            .iter()
            .filter_map(|beat| {
                let count = attached_sounds(&workspace.document, &beat.id)
                    .iter()
                    .filter(|sound| sound.audible.is_some())
                    .count();
                (count > 0).then(|| (beat.id.clone(), count))
            })
            .collect()
    }

    pub(super) fn attached_inspector(&mut self, ui: &mut egui::Ui, ready: bool) {
        let rows = self.attached_rows();
        if rows.is_empty() {
            return;
        }
        let Some(workspace) = &self.workspace else {
            return;
        };
        let rate = workspace.document.presentation_basis().frame_rate;
        let selected = self.attached_target();
        ui.add_space(8.0);
        ui.label(style::section_title("ATTACHED SOUNDS", false));
        let mut select = None;
        let mut detach = None;
        for (index, sound) in rows.iter().enumerate() {
            let key = (sound.owner.clone(), sound.id.clone());
            let is_selected = selected.as_ref() == Some(&key);
            let detail = attached_detail(sound, rate);
            let text = format!("♫  {}", sound.label);
            let response = ui.add_sized(
                [ui.available_width(), 26.0],
                egui::Button::new(&text).selected(is_selected),
            );
            reveal(&response);
            let spoken = format!(
                "Attached sound {} of {}: {}, {}{}",
                index + 1,
                rows.len(),
                sound.label,
                detail,
                if is_selected { ", selected" } else { "" }
            );
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::Button, true, is_selected, &spoken)
            });
            if response.clicked() {
                select = Some(key.clone());
            }
            ui.add(egui::Label::new(egui::RichText::new(&detail).size(12.0).weak()).wrap());
            if is_selected {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let response = ui.add_enabled(ready, style::action("Detach", ":sound-detach"));
                    reveal(&response);
                    let spoken = format!("Detach {} from this beat", sound.label);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, ready, &spoken)
                    });
                    if response.clicked() {
                        detach = Some(key.clone());
                    }
                });
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            ":sound-gain, :sound-edges and :sound-delete apply to this sound.",
                        )
                        .size(12.0)
                        .weak(),
                    )
                    .wrap(),
                );
            }
        }
        if let Some(key) = select {
            self.focus_scoped_pane(Pane::Inspector);
            self.message = Some(
                "Attached sound selected. :sound-gain, :sound-edges and :sound-detach apply to it."
                    .into(),
            );
            self.selected_attached = Some(key);
        }
        if let Some(key) = detach {
            self.focus_scoped_pane(Pane::Inspector);
            self.selected_attached = Some(key);
            self.sound_action(SoundAction::Detach, ui.ctx());
        }
    }
}

/// A newly focused row scrolls into view at once (see the inspector scroll).
fn reveal(response: &egui::Response) {
    if response.gained_focus() {
        response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
    }
}

/// Exact beat-local frames, trimmed to two decimals for display.
fn frames_label(value: ExactRatio) -> String {
    let frames = value.numerator() as f64 / value.denominator() as f64;
    let text = format!("{frames:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text} f")
}

/// Where the sound starts in the beat, how long it lasts and where it plays.
pub(super) fn attached_detail(sound: &AttachedSound, rate: FrameRate) -> String {
    let length = sound.end.checked_sub(sound.start).map_or(0.0, |frames| {
        frames.numerator() as f64 * f64::from(rate.denominator())
            / (frames.denominator() as f64 * f64::from(rate.numerator()))
    });
    let place = match (&sound.audible, sound.plays) {
        (None, _) => " · cut away in this beat".to_owned(),
        (Some(_), Some(plays)) => format!(" · in every play ({plays})"),
        (Some((start, end)), None) if (*start, *end) != (sound.start, sound.end) => {
            format!(
                " · plays {}–{} here",
                frames_label(*start),
                frames_label(*end)
            )
        }
        (Some(_), None) => String::new(),
    };
    format!(
        "Starts {} into the beat · {length:.2} s{place}",
        frames_label(sound.start)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sound(
        start: ExactRatio,
        end: ExactRatio,
        audible: Option<(ExactRatio, ExactRatio)>,
        plays: Option<u32>,
    ) -> AttachedSound {
        AttachedSound {
            owner: NodeId::new("beat").unwrap(),
            id: SoundId::new("sting").unwrap(),
            label: "Boing".into(),
            start,
            end,
            audible,
            plays,
        }
    }

    #[test]
    fn inspector_rows_describe_start_length_and_where_the_sound_plays() {
        let rate = FrameRate::new(30_000, 1001).unwrap();
        let (start, end) = (ExactRatio::integer(12), ExactRatio::new(27, 2).unwrap());
        assert_eq!(
            attached_detail(&sound(start, end, Some((start, end)), None), rate),
            "Starts 12 f into the beat · 0.05 s"
        );
        assert_eq!(
            attached_detail(&sound(start, end, Some((start, end)), Some(3)), rate),
            "Starts 12 f into the beat · 0.05 s · in every play (3)"
        );
        assert_eq!(
            attached_detail(
                &sound(start, end, Some((start, ExactRatio::integer(13))), None),
                rate
            ),
            "Starts 12 f into the beat · 0.05 s · plays 12 f–13 f here"
        );
        assert_eq!(
            attached_detail(&sound(start, end, None, None), rate),
            "Starts 12 f into the beat · 0.05 s · cut away in this beat"
        );
    }
}
