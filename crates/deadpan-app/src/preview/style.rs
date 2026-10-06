//! Native workspace tokens and bounded geometry from the saved design target.

use eframe::egui::{self, Color32, FontId, RichText, Stroke};

pub(super) const CANVAS: Color32 = Color32::from_rgb(0x17, 0x19, 0x1d);
pub(super) const PANEL: Color32 = Color32::from_rgb(0x20, 0x23, 0x29);
pub(super) const TEXT: Color32 = Color32::from_rgb(0xe9, 0xeb, 0xf4);
pub(super) const MUTED: Color32 = Color32::from_rgb(0xaa, 0xb0, 0xbf);
pub(super) const BORDER: Color32 = Color32::from_rgb(0x37, 0x3d, 0x49);
/// Card and control borders under macOS Increase contrast: at least 3:1
/// against both the canvas and panel fills.
pub(super) const HIGH_CONTRAST_BORDER: Color32 = Color32::from_rgb(0x8a, 0x93, 0xa6);
pub(super) const LAVENDER: Color32 = Color32::from_rgb(0xc4, 0xb5, 0xfd);
pub(super) const SELECTED: Color32 = Color32::from_rgb(0x39, 0x37, 0x50);
pub(super) const CURSOR: Color32 = Color32::from_rgb(0xf6, 0xd3, 0x65);
pub(super) const SAVED: Color32 = Color32::from_rgb(0xa7, 0xf3, 0xd0);
pub(super) const ERROR: Color32 = Color32::from_rgb(0xf8, 0x8a, 0x8a);
pub(super) const WARNING: Color32 = Color32::from_rgb(0xf5, 0xb8, 0x6b);
/// Detected or corrected pauses under the Original range and Your edit's
/// cards: a quiet band that never competes with the selection.
pub(super) const PAUSE_BAND: Color32 = Color32::from_rgba_premultiplied(0x2e, 0x31, 0x37, 0x55);
/// Shot boundaries: hairline ticks.
pub(super) const SHOT_TICK: Color32 = Color32::from_rgb(0x8a, 0x93, 0xa6);

/// Secondary text: [`MUTED`], raised to full text colour under macOS Increase
/// contrast. Prefer `RichText::weak()`, which resolves the same colour.
pub(super) fn muted(ui: &egui::Ui) -> Color32 {
    ui.visuals().weak_text_color()
}

/// Secondary text for painters without a `Ui`.
pub(super) fn muted_in(context: &egui::Context) -> Color32 {
    context.global_style().visuals.weak_text_color()
}

/// SF Pro weight for titles and emphasized values.
pub(super) const SEMIBOLD: f32 = 600.0;

pub(super) fn semibold(text: impl Into<String>) -> RichText {
    RichText::new(text).variation("wght", SEMIBOLD)
}

/// Small uppercase pane and section title.
pub(super) fn section_title(text: &str, focused: bool) -> RichText {
    let title = RichText::new(text)
        .size(11.0)
        .extra_letter_spacing(0.8)
        .variation("wght", SEMIBOLD);
    if focused {
        title.color(LAVENDER)
    } else {
        title.weak()
    }
}

/// A word-bearing focus cue, so pane focus never depends on color alone.
pub(super) fn focus_pill(ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(SELECTED)
        .corner_radius(3)
        .inner_margin(egui::Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(
                RichText::new("FOCUS")
                    .size(9.0)
                    .extra_letter_spacing(0.6)
                    .variation("wght", SEMIBOLD)
                    .color(LAVENDER),
            );
        });
}

/// A command label and its key as one text run: the label in normal text and
/// the key in grey monospace. One run keeps the painted text identical to the
/// accessible label, `"{label}  {key}"`.
pub(super) fn action_text(label: &str, key: &str, size: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let proportional = egui::TextFormat {
        font_id: FontId::proportional(size),
        color: TEXT,
        ..Default::default()
    };
    job.append(label, 0.0, proportional.clone());
    job.append("  ", 0.0, proportional);
    job.append(
        key,
        0.0,
        egui::TextFormat {
            font_id: FontId::monospace(size - 2.0),
            color: MUTED,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job
}

/// A pre-measured command label, for controls that reserve their height from
/// the same galley they paint.
pub(super) fn action_galley(
    ui: &egui::Ui,
    label: &str,
    key: &str,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    egui::WidgetText::from(action_text(label, key, 13.0)).into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        width,
        egui::TextStyle::Button,
    )
}

/// A command button showing its key.
pub(super) fn action<'a>(label: impl AsRef<str>, key: impl AsRef<str>) -> egui::Button<'a> {
    egui::Button::new(action_text(label.as_ref(), key.as_ref(), 13.0))
}

/// A full-width, left-aligned command row for side panes.
pub(super) fn row_action<'a>(
    ui: &egui::Ui,
    label: impl AsRef<str>,
    key: impl AsRef<str>,
) -> egui::Button<'a> {
    egui::Button::new((
        action_text(label.as_ref(), key.as_ref(), 13.0),
        egui::Atom::grow(),
    ))
    .min_size(egui::vec2(ui.available_width(), 28.0))
}

/// Read-only label/value pairs in aligned columns. Values are plain text,
/// never field-shaped, so they cannot be mistaken for editable inputs.
pub(super) fn value_grid<'a>(
    ui: &mut egui::Ui,
    id: &str,
    rows: impl IntoIterator<Item = (&'a str, &'a str)>,
) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing(egui::vec2(12.0, 6.0))
        .min_row_height(18.0)
        .show(ui, |ui| {
            for (label, value) in rows {
                ui.label(RichText::new(label).size(12.0).weak());
                super::accessibility::full_text(
                    ui.add(
                        egui::Label::new(RichText::new(value).monospace().size(11.5)).truncate(),
                    ),
                    value,
                )
                .on_hover_text(format!("{label}: {value}"));
                ui.end_row();
            }
        });
}

pub(super) fn apply(context: &egui::Context) {
    context.set_theme(egui::Theme::Dark);
    // TextEdit must consume its final input before command mode closes. Allow
    // one pass for that transition and two to measure/reanchor the new footer.
    // Stable frames still use one pass; external updates are read only once.
    context.options_mut(|options| options.max_passes = std::num::NonZeroUsize::new(3).unwrap());
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
        style.visuals.hyperlink_color = LAVENDER;
        style.visuals.error_fg_color = ERROR;
        style.visuals.warn_fg_color = WARNING;
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

pub(super) fn compact_panel() -> egui::Frame {
    panel().inner_margin(egui::Margin::symmetric(12, 6))
}

pub(super) fn beat_panel(layout: Layout) -> egui::Panel {
    egui::Panel::bottom("workspace-sequence")
        .resizable(false)
        .default_size(layout.beats)
        .min_size(layout.beats)
        .max_size(layout.beats)
        .frame(compact_panel())
}

/// A small quality-tier chip in the picture's top-right corner while the
/// viewer shows preview-proxy pixels. It disappears once the exact Original
/// picture replaces them.
pub(super) fn proxy_badge(painter: &egui::Painter, picture: egui::Rect) {
    if picture.width() < 80.0 || picture.height() < 40.0 {
        return;
    }
    let galley = painter.layout_no_wrap(
        "Proxy".to_owned(),
        egui::FontId::proportional(11.0),
        muted_in(painter.ctx()),
    );
    let padding = egui::vec2(6.0, 2.0);
    let size = galley.size() + padding * 2.0;
    let chip = egui::Rect::from_min_size(
        egui::pos2(picture.right() - size.x - 8.0, picture.top() + 8.0),
        size,
    );
    painter.rect(
        chip,
        4.0,
        Color32::from_rgba_unmultiplied(0x17, 0x19, 0x1d, 0xd0),
        egui::Stroke::new(1.0, BORDER),
        egui::StrokeKind::Inside,
    );
    painter.galley(chip.min + padding, galley, muted_in(painter.ctx()));
}

/// The viewing condition of an HDR-bearing project, in the picture's
/// top-left corner: HDR output shown tone-mapped on this SDR viewer, or SDR
/// output whose HDR sources are tone-mapped (spec 17.5 requires the label).
pub(super) fn color_badge(painter: &egui::Painter, picture: egui::Rect, label: &str) {
    if picture.width() < 160.0 || picture.height() < 40.0 {
        return;
    }
    let galley = painter.layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(11.0),
        muted_in(painter.ctx()),
    );
    let padding = egui::vec2(6.0, 2.0);
    let size = galley.size() + padding * 2.0;
    let chip =
        egui::Rect::from_min_size(egui::pos2(picture.left() + 8.0, picture.top() + 8.0), size);
    painter.rect(
        chip,
        4.0,
        Color32::from_rgba_unmultiplied(0x17, 0x19, 0x1d, 0xd0),
        egui::Stroke::new(1.0, BORDER),
        egui::StrokeKind::Inside,
    );
    painter.galley(chip.min + padding, galley, muted_in(painter.ctx()));
}

pub(super) fn keycap(ui: &mut egui::Ui, text: &str) {
    keycap_frame().show(ui, |ui| {
        ui.label(RichText::new(text).monospace().size(11.0));
    });
}

fn keycap_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(3)
        .inner_margin(egui::Margin::symmetric(5, 2))
}

pub(super) fn key_hint(ui: &mut egui::Ui, key: &str, label: &str) {
    let galley = |text: RichText| {
        egui::WidgetText::from(text).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Body,
        )
    };
    let key = galley(RichText::new(key).monospace().size(11.0));
    let label = galley(RichText::new(label).weak().size(11.0));
    let frame = keycap_frame();
    let key_size = key.size() + frame.total_margin().sum();
    let size = egui::vec2(
        key_size.x + 4.0 + label.size().x,
        key_size
            .y
            .max(label.size().y)
            .max(ui.spacing().interact_size.y),
    );
    // The wrapping parent needs the whole pair's width before allocating it.
    // A nested horizontal UI otherwise grows past the right edge after layout.
    ui.allocate_ui_with_layout(
        size,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            frame.show(ui, |ui| {
                ui.label(key);
            });
            ui.label(label);
        },
    );
}

/// Reserve fallback glyphs' vertical overhang in a top-down text column.
/// Label allocates the logical row box; its actual ink can extend beyond it.
/// Reuse the measured galley so wrapping, selection and accessibility stay on
/// the standard Label path, without enlarging the surrounding scroll clip.
pub(super) fn ink_padded_label(
    ui: &mut egui::Ui,
    text: RichText,
    wrap: Option<egui::TextWrapMode>,
) -> egui::Response {
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        wrap,
        ui.available_width(),
        egui::TextStyle::Body,
    );
    let top = (-galley.mesh_bounds.top()).max(0.0).ceil();
    let bottom = (galley.mesh_bounds.bottom() - galley.rect.bottom())
        .max(0.0)
        .ceil();
    ui.add_space(top);
    let response = ui.label(galley);
    ui.add_space(bottom);
    response
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
        let (card_height, beats) = if height < 700.0 {
            (64.0, 128.0)
        } else {
            (92.0, 164.0)
        };
        Self {
            sources,
            inspector,
            // Compact: 28 px heading, 4 px row gap, 20 px cursor band and
            // 12 px panel margins. The scrollbar floats over the canvas.
            beats,
            card_width: ((width - sources - 24.0) / 4.0).clamp(210.0, 340.0),
            card_height,
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod ink_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_shortcuts_keep_every_key_and_label_inside_the_panel() {
        let context = egui::Context::default();
        apply(&context);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 200.0));
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (key, label) in [
                        ("h l", "frame"),
                        (":sequence", "Your edit"),
                        (",i", "reuse Original"),
                        ("Tab", "pane"),
                        (":", "command"),
                        ("?", "keys"),
                    ] {
                        key_hint(ui, key, label);
                    }
                });
            },
        );
        output.textures_delta.clear();
        let mut text_count = 0;
        for clipped in output.shapes {
            if let egui::Shape::Text(text) = clipped.shape {
                text_count += 1;
                let bounds = text.galley.rect.translate(text.pos.to_vec2());
                assert!(
                    screen.contains_rect(bounds) && clipped.clip_rect.contains_rect(bounds),
                    "Clipped hint {:?} at {bounds:?}",
                    text.galley.text()
                );
            }
        }
        assert_eq!(text_count, 12);
    }

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
            assert!((128.0..=164.0).contains(&layout.beats));
        }
    }
}
