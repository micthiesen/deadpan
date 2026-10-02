//! Editor teaching is derived from the same map that routes Normal and Visual.
//! Native widgets and temporary editing modes keep their own key ownership.

use super::*;

fn frame() -> egui::Frame {
    egui::Frame::new()
        .fill(style::PANEL)
        .stroke(egui::Stroke::new(1.0, style::BORDER))
        .corner_radius(3)
        .inner_margin(egui::Margin::symmetric(5, 2))
}

fn text(ui: &egui::Ui, value: egui::RichText, width: f32) -> Arc<egui::Galley> {
    egui::WidgetText::from(value).into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        width,
        egui::TextStyle::Body,
    )
}

/// A whole path remains visible, including paths wider than a footer/help row.
pub(super) fn keycap(ui: &mut egui::Ui, key: &str) {
    let frame = frame();
    let width = ui.max_rect().width().max(1.0);
    let galley = text(
        ui,
        egui::RichText::new(key).monospace().size(11.0),
        (width - frame.total_margin().sum().x).max(1.0),
    );
    let size = galley.size() + frame.total_margin().sum();
    ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
        frame.show(ui, |ui| {
            ui.label(galley);
        });
    });
}

/// Preserve the compact default layout; measure an oversized path and its
/// description before allocating a wrapped, stacked hint in the same pass.
pub(super) fn hint(ui: &mut egui::Ui, key: &str, description: &str) {
    let frame = frame();
    let margin = frame.total_margin().sum();
    let width = ui.max_rect().width().max(1.0);
    let key_text = egui::RichText::new(key).monospace().size(11.0);
    let description_text = egui::RichText::new(description)
        .color(style::MUTED)
        .size(11.0);
    let natural_key = text(ui, key_text.clone(), f32::INFINITY);
    let natural_description = text(ui, description_text.clone(), f32::INFINITY);
    if natural_key.size().x + margin.x + 4.0 + natural_description.size().x <= width {
        style::key_hint(ui, key, description);
        return;
    }
    let key = text(ui, key_text, (width - margin.x).max(1.0));
    let description = text(ui, description_text, width);
    let height = key.size().y + margin.y + 4.0 + description.size().y;
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            frame.show(ui, |ui| {
                ui.label(key);
            });
            ui.label(description);
        },
    );
}

pub(super) type Hints = Vec<(String, String)>;

fn hint_size(ui: &egui::Ui, width: f32, key: &str, description: &str) -> egui::Vec2 {
    let margin = frame().total_margin().sum();
    let key_text = egui::RichText::new(key).monospace().size(11.0);
    let description_text = egui::RichText::new(description).size(11.0);
    let key = text(ui, key_text.clone(), f32::INFINITY);
    let description = text(ui, description_text.clone(), f32::INFINITY);
    let natural_width = key.size().x + margin.x + 4.0 + description.size().x;
    if natural_width <= width {
        egui::vec2(
            natural_width,
            (key.size().y + margin.y)
                .max(description.size().y)
                .max(ui.spacing().interact_size.y),
        )
    } else {
        let key = text(ui, key_text, (width - margin.x).max(1.0));
        let description = text(ui, description_text, width);
        egui::vec2(width, key.size().y + margin.y + 4.0 + description.size().y)
    }
}

/// Footer teaching yields to one complete reference shortcut when the entire
/// contextual set would consume the viewer. The reference and action controls
/// retain every binding; no individual key path is shortened.
pub(super) fn footer_hints(ui: &mut egui::Ui, hints: &Hints, help: &str, budget: f32) {
    let width = ui.max_rect().width().max(1.0);
    let mut used = (width - ui.available_width()).max(0.0);
    let mut height = 0.0;
    let mut row = ui.spacing().interact_size.y;
    for (key, description) in hints {
        let size = hint_size(ui, width, key, description);
        if used > 0.0 && used + size.x > width {
            height += row + ui.spacing().item_spacing.y;
            row = 0.0;
            used = 0.0;
        }
        row = row.max(size.y);
        used += size.x + ui.spacing().item_spacing.x;
    }
    if height + row > budget {
        hint(ui, help, "all editor keys");
    } else {
        for (key, description) in hints {
            hint(ui, key, description);
        }
    }
}

/// Large branches keep their exact next strokes visible without repeating all
/// action descriptions in the footer. Help retains the full action reference.
pub(super) fn pending_guidance(ui: &mut egui::Ui, full: &str, next_keys: &str) {
    let width = ui.max_rect().width().max(1.0);
    let budget = ui
        .ctx()
        .input(|input| (input.content_rect().height() * 0.08).clamp(32.0, 64.0));
    let rich = |value: &str| egui::RichText::new(value).size(11.0).color(style::LAVENDER);
    let full_text = text(ui, rich(full), width);
    if full_text.size().y <= budget {
        ui.label(full_text);
    } else {
        ui.label(text(ui, rich(next_keys), width))
            .on_hover_text(full);
    }
}

pub(super) fn pair(
    bindings: &Bindings,
    first: EditorKey,
    second: EditorKey,
    separator: &str,
) -> String {
    format!(
        "{}{separator}{}",
        bindings.key_label(first),
        bindings.key_label(second)
    )
}

pub(super) fn aliases_pair(
    bindings: &Bindings,
    first: EditorKey,
    second: EditorKey,
    separator: &str,
) -> String {
    format!(
        "{}{separator}{}",
        bindings.key_labels(first),
        bindings.key_labels(second)
    )
}

impl DeadpanApp {
    pub(super) fn editor_key(&self, id: EditorKey) -> String {
        self.bindings.key_label(id)
    }

    pub(super) fn editor_keys(&self, id: EditorKey) -> String {
        self.bindings.key_labels(id)
    }

    pub(super) fn editor_pair(
        &self,
        first: EditorKey,
        second: EditorKey,
        separator: &str,
    ) -> String {
        pair(&self.bindings, first, second, separator)
    }

    pub(super) fn editor_counted(&self, id: EditorKey, count: u32) -> String {
        self.bindings.counted_label(id, count)
    }

    pub(super) fn editor_copy_recipe(&self) -> String {
        format!(
            "{}, {}, {}",
            self.editor_key(EditorKey::Visual),
            self.editor_pair(EditorKey::FramePrevious, EditorKey::FrameNext, "/"),
            self.editor_key(EditorKey::Copy)
        )
    }

    pub(super) fn add_editor_hint(&self, hints: &mut Hints, id: EditorKey, description: &str) {
        hints.push((self.editor_key(id), description.to_owned()));
    }

    pub(super) fn add_editor_pair_hint(
        &self,
        hints: &mut Hints,
        first: EditorKey,
        second: EditorKey,
        separator: &str,
        description: &str,
    ) {
        hints.push((
            self.editor_pair(first, second, separator),
            description.to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn many_legal_long_paths_keep_footer_teaching_within_one_reference_row() {
        let actions = [
            EditorKey::FramePrevious,
            EditorKey::FrameNext,
            EditorKey::BeatPrevious,
            EditorKey::BeatNext,
            EditorKey::Visual,
            EditorKey::Copy,
            EditorKey::EnterGroup,
            EditorKey::LeaveGroup,
            EditorKey::Split,
            EditorKey::GainUp,
            EditorKey::GainDown,
            EditorKey::Hold,
            EditorKey::Camera,
            EditorKey::Trim,
            EditorKey::Repeat,
            EditorKey::CutFrames,
            EditorKey::CutBeat,
            EditorKey::PasteAfter,
            EditorKey::PasteBefore,
            EditorKey::Undo,
            EditorKey::Help,
        ];
        let entries = actions
            .iter()
            .zip('a'..='z')
            .map(|(action, suffix)| {
                let mut path = vec!["Shift+F11".to_owned(); 15];
                path.push(suffix.to_string());
                serde_json::json!({ "action": action.as_str(), "keys": [path] })
            })
            .collect::<Vec<_>>();
        let bindings = Bindings::from_json(
            &serde_json::to_vec(&serde_json::json!({
                "version": 1, "key_mode": "logical", "bindings": entries,
            }))
            .unwrap(),
        )
        .unwrap();
        let hints = actions
            .iter()
            .map(|id| (bindings.key_label(*id), id.as_str().to_owned()))
            .collect();
        let help = bindings.key_label(EditorKey::Help);
        let context = egui::Context::default();
        style::apply(&context);
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
        let mut row = egui::Rect::NOTHING;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                ..Default::default()
            },
            |ui| {
                row = ui
                    .horizontal_wrapped(|ui| {
                        ui.label("Edit 123 / 456 f");
                        footer_hints(ui, &hints, &help, 102.4);
                    })
                    .response
                    .rect;
            },
        );
        output.textures_delta.clear();
        assert!(
            row.height() <= 102.4,
            "Footer hint row grew to {}",
            row.height()
        );
        let painted = output
            .shapes
            .iter()
            .filter_map(|clipped| {
                let egui::Shape::Text(text) = &clipped.shape else {
                    return None;
                };
                assert!(clipped.clip_rect.contains_rect(text.visual_bounding_rect()));
                assert!(viewport.contains_rect(text.visual_bounding_rect()));
                Some(text.galley.text())
            })
            .collect::<Vec<_>>();
        assert!(painted.contains(&help.as_str()));
        assert!(painted.contains(&"all editor keys"));
        assert!(!painted.contains(&bindings.key_label(EditorKey::FramePrevious).as_str()));
    }

    #[test]
    fn short_paths_keep_the_complete_contextual_footer() {
        let context = egui::Context::default();
        style::apply(&context);
        let bindings = Bindings::default();
        let ids = [
            EditorKey::FramePrevious,
            EditorKey::FrameNext,
            EditorKey::BeatPrevious,
            EditorKey::BeatNext,
            EditorKey::Visual,
            EditorKey::Copy,
            EditorKey::EnterGroup,
            EditorKey::LeaveGroup,
            EditorKey::Split,
            EditorKey::GainUp,
            EditorKey::GainDown,
            EditorKey::Hold,
            EditorKey::Camera,
            EditorKey::Trim,
            EditorKey::Repeat,
            EditorKey::CutFrames,
            EditorKey::CutBeat,
            EditorKey::PasteAfter,
            EditorKey::PasteBefore,
            EditorKey::Undo,
            EditorKey::Help,
        ];
        let hints = ids
            .iter()
            .map(|id| (bindings.key_label(*id), id.as_str().to_owned()))
            .collect();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 640.0),
                )),
                ..Default::default()
            },
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Edit 123 / 456 f");
                    footer_hints(ui, &hints, "?", 102.4);
                });
            },
        );
        output.textures_delta.clear();
        let painted = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for id in ids {
            assert!(painted.contains(&id.as_str()), "Missing {}", id.as_str());
        }
        assert!(!painted.contains(&"all editor keys"));
    }

    #[test]
    fn long_path_keycap_and_hint_paint_the_complete_path_within_the_first_viewport() {
        let key = ["Shift+F12"; 16].join(" ");
        for with_description in [false, true] {
            let context = egui::Context::default();
            style::apply(&context);
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 300.0));
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport),
                    ..Default::default()
                },
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if with_description {
                            hint(ui, &key, "move one frame");
                        } else {
                            keycap(ui, &key);
                        }
                    });
                },
            );
            output.textures_delta.clear();
            let mut painted = Vec::new();
            for clipped in &output.shapes {
                let egui::Shape::Text(text) = &clipped.shape else {
                    continue;
                };
                if text.galley.text() == key || text.galley.text() == "move one frame" {
                    assert!(clipped.clip_rect.contains_rect(text.visual_bounding_rect()));
                    assert!(viewport.contains_rect(text.visual_bounding_rect()));
                    painted.push(text.galley.text());
                }
            }
            assert!(painted.contains(&key.as_str()));
            assert_eq!(painted.contains(&"move one frame"), with_description);
        }
    }
}
