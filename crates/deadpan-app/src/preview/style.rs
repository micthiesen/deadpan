//! Native workspace tokens and bounded geometry from the saved design target.

use eframe::egui::{self, Color32, FontId, RichText, Stroke};

pub(super) const CANVAS: Color32 = Color32::from_rgb(0x17, 0x19, 0x1d);
pub(super) const PANEL: Color32 = Color32::from_rgb(0x20, 0x23, 0x29);
pub(super) const TEXT: Color32 = Color32::from_rgb(0xe9, 0xeb, 0xf4);
pub(super) const MUTED: Color32 = Color32::from_rgb(0xaa, 0xb0, 0xbf);
pub(super) const BORDER: Color32 = Color32::from_rgb(0x37, 0x3d, 0x49);
pub(super) const LAVENDER: Color32 = Color32::from_rgb(0xc4, 0xb5, 0xfd);
pub(super) const SELECTED: Color32 = Color32::from_rgb(0x39, 0x37, 0x50);
pub(super) const CURSOR: Color32 = Color32::from_rgb(0xf6, 0xd3, 0x65);
pub(super) const SAVED: Color32 = Color32::from_rgb(0xa7, 0xf3, 0xd0);

pub(super) fn apply(context: &egui::Context) {
    context.set_theme(egui::Theme::Dark);
    context.all_styles_mut(|style| {
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = CANVAS;
        style.visuals.window_fill = PANEL;
        style.visuals.extreme_bg_color = CANVAS;
        style.visuals.faint_bg_color = PANEL;
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.weak_text_color = Some(MUTED);
        style.visuals.selection.bg_fill = SELECTED;
        style.visuals.selection.stroke = Stroke::new(1.5, LAVENDER);
        style.visuals.window_stroke = Stroke::new(1.0, BORDER);
        for widget in [
            &mut style.visuals.widgets.noninteractive,
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.open,
        ] {
            widget.bg_fill = PANEL;
            widget.weak_bg_fill = PANEL;
            widget.bg_stroke = Stroke::new(1.0, BORDER);
            widget.fg_stroke = Stroke::new(1.0, TEXT);
            widget.corner_radius = egui::CornerRadius::same(4);
        }
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x2b, 0x2e, 0x36);
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, LAVENDER);
        style.visuals.widgets.active.bg_fill = SELECTED;
        style.visuals.widgets.active.bg_stroke = Stroke::new(1.5, LAVENDER);
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(11.5));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, FontId::monospace(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, FontId::proportional(17.0));
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.interact_size.y = 28.0;
    });
}

pub(super) fn panel() -> egui::Frame {
    egui::Frame::new()
        .fill(CANVAS)
        .inner_margin(egui::Margin::same(12))
}

pub(super) fn beat_panel(layout: Layout) -> egui::Panel {
    egui::Panel::bottom("workspace-sequence")
        .resizable(false)
        .default_size(layout.beats)
        .min_size(layout.beats)
        .max_size(layout.beats)
        .frame(panel())
}

pub(super) fn keycap(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(3)
        .inner_margin(egui::Margin::symmetric(5, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).monospace().size(11.0));
        });
}

pub(super) fn key_hint(ui: &mut egui::Ui, key: &str, label: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        keycap(ui, key);
        ui.label(RichText::new(label).color(MUTED).size(11.0));
    });
}

#[derive(Clone, Copy)]
pub(super) struct Layout {
    pub sources: f32,
    pub inspector: f32,
    pub beats: f32,
    pub card_width: f32,
    pub card_height: f32,
}

impl Layout {
    pub fn for_size(width: f32, height: f32) -> Self {
        let width = if width.is_finite() {
            width.max(1.0)
        } else {
            960.0
        };
        let height = if height.is_finite() {
            height.max(1.0)
        } else {
            640.0
        };
        let sources = (width * 0.15).clamp(144.0, 196.0);
        let inspector = (width * 0.185).clamp(192.0, 236.0);
        let card_height = if height < 700.0 { 88.0 } else { 92.0 };
        Self {
            sources,
            inspector,
            // Header, row spacing, cursor badge, horizontal scrollbar and frame
            // margins all need space outside the cards themselves.
            beats: card_height + 92.0,
            card_width: ((width - sources - 24.0) / 4.0).clamp(210.0, 340.0),
            card_height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_windows_keep_the_picture_larger_than_either_sidebar() {
        for (width, height) in [(960.0, 640.0), (1280.0, 820.0), (1440.0, 900.0)] {
            let layout = Layout::for_size(width, height);
            let picture_width = width - layout.sources - layout.inspector - 48.0;
            assert!(picture_width >= 480.0);
            assert!(picture_width > layout.sources + layout.inspector);
            assert!(height - layout.beats - 160.0 >= 300.0);
            assert!(layout.card_height + 56.0 <= layout.beats);
        }
    }

    #[test]
    fn layout_metrics_stay_finite_and_bounded_for_transient_resize_inputs() {
        for value in [0.0, -1.0, f32::NAN, f32::INFINITY, 100_000.0] {
            let layout = Layout::for_size(value, value);
            assert!((144.0..=196.0).contains(&layout.sources));
            assert!((192.0..=236.0).contains(&layout.inspector));
            assert!((210.0..=340.0).contains(&layout.card_width));
            assert!((180.0..=184.0).contains(&layout.beats));
        }
    }
}
