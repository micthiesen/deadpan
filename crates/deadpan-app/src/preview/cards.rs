//! Fixed card canvases: overlay widgets must not advance their parent's flow.

use eframe::egui;

use super::thumbnails::Painted;
use super::{BeatRow, NodeId, accessibility, fit_rect, paint_cursor, style};

/// Logical height of an ordinary card thumbnail; textures render at this
/// height times the display scale and are fitted into smaller cards.
pub(super) const THUMBNAIL_HEIGHT: f32 = 76.0;
const MIN_THUMBNAIL_WIDTH: f32 = 64.0;
/// Room for a card's name, `frames · kind` and `start–end` lines.
const MIN_TEXT_WIDTH: f32 = 112.0;

/// Words a thumbnail shows while it has no picture of its own. An authored
/// Background is a black picture; loading and failure are not.
pub(super) const THUMBNAIL_LOADING: &str = "Loading…";
pub(super) const THUMBNAIL_UNAVAILABLE: &str = "No preview";

/// Letterboxed picture on black, with a hairline so dark frames keep an edge.
/// A thumbnail without a picture yet, or whose decode failed, is a panel
/// tone with words instead, so it never looks like an authored Background.
pub(super) fn paint_thumbnail(ui: &egui::Ui, rect: egui::Rect, painted: Option<Painted>) {
    let painter = ui.painter();
    let placeholder = match painted {
        None => Some(THUMBNAIL_LOADING),
        Some(Painted::Unavailable) => Some(THUMBNAIL_UNAVAILABLE),
        Some(Painted::Texture { .. } | Painted::Black { .. }) => None,
    };
    if let Some(text) = placeholder {
        painter.rect_filled(rect, 4.0, style::PANEL_RAISED);
        painter
            .with_clip_rect(rect.intersect(painter.clip_rect()))
            .text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(10.5),
                style::muted(ui),
            );
    } else {
        painter.rect_filled(rect, 4.0, egui::Color32::BLACK);
    }
    if let Some(Painted::Texture { texture, aspect }) = painted {
        painter.image(
            texture,
            fit_rect(rect, aspect),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, accessibility::border(ui.ctx())),
        egui::StrokeKind::Inside,
    );
}

#[derive(Default)]
pub(super) struct Markers {
    /// Card index and fraction of the cursor badge.
    pub cursor: Option<(usize, f32)>,
    /// The Edit frame the badge reports.
    pub frame: u64,
    pub range: Option<std::ops::Range<u64>>,
    /// Pause bands and shot starts on the Edit clock, sorted.
    pub analysis: std::sync::Arc<super::transcript::AnalysisMarks>,
}

pub(super) fn original(
    ui: &mut egui::Ui,
    label: &str,
    detail: &str,
    selected: bool,
    thumbnail: Option<Painted>,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 118.0),
        egui::Sense::hover(),
    );
    let response = workspace_card(
        ui,
        rect,
        ui.make_persistent_id("original-card"),
        selected,
        &format!("Original: {label}, {detail}. Browse the unchanged original."),
    );
    // Space was reserved above. A scope_builder would move the parent cursor
    // back to this child's last text row and overlap the following caption.
    let inner = rect.shrink(10.0);
    let picture = egui::Rect::from_min_size(inner.min, egui::vec2(inner.width(), 58.0));
    paint_thumbnail(ui, picture, thumbnail);
    let mut content = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_max(
        egui::pos2(inner.left() + 2.0, picture.bottom() + 6.0),
        inner.max,
    )));
    content.spacing_mut().item_spacing.y = 2.0;
    content.add(
        egui::Label::new(style::semibold(label))
            .truncate()
            .selectable(false),
    );
    content.add(
        egui::Label::new(egui::RichText::new(detail).size(11.0).weak())
            .truncate()
            .selectable(false),
    );
    response
}

pub(super) fn strip(
    ui: &mut egui::Ui,
    layout: style::Layout,
    beats: &[BeatRow],
    selected: Option<&NodeId>,
    markers: Markers,
    reveal: bool,
    thumbnail: &mut dyn FnMut(&BeatRow) -> Option<Painted>,
) -> Option<usize> {
    let canvas_width = beats.len() as f32 * layout.card_width;
    let geometry_id = ui.make_persistent_id("beat-strip-geometry");
    let geometry = (layout.card_width, ui.available_width());
    let resized = ui.data_mut(|data| {
        let previous = data.get_temp::<(f32, f32)>(geometry_id);
        data.insert_temp(geometry_id, geometry);
        previous.is_some_and(|previous| previous != geometry)
    });
    let mut scroll = egui::ScrollArea::horizontal().id_salt("beat-strip");
    if (reveal || resized)
        && let Some(index) = beats.iter().position(|beat| Some(&beat.id) == selected)
    {
        // egui positions this frame's content before clamping the retained
        // offset. Bound a new reveal now so a fitting strip never overscrolls
        // for one frame after an append or selection change.
        let max_offset = (canvas_width - ui.available_width()).max(0.0);
        scroll =
            scroll.horizontal_scroll_offset((index as f32 * layout.card_width).min(max_offset));
    }
    let mut clicked = None;
    scroll.show_viewport(ui, |ui, viewport| {
        // One virtual canvas includes the cursor badge and every card. Drawing
        // its overlay children must neither add another row nor rewind flow.
        let (_, canvas) = ui.allocate_space(egui::vec2(canvas_width, layout.card_height + 20.0));
        let start = (viewport.min.x / layout.card_width).floor().max(0.0) as usize;
        let end = ((viewport.max.x / layout.card_width).ceil() as usize + 1).min(beats.len());
        let origin = canvas.min + egui::vec2(0.0, 20.0);
        let mut badge = None;
        for (index, beat) in beats.iter().enumerate().take(end).skip(start) {
            let rect = egui::Rect::from_min_size(
                origin + egui::vec2(index as f32 * layout.card_width, 0.0),
                egui::vec2(layout.card_width - 8.0, layout.card_height),
            );
            let response = beat_card(ui, rect, beat, index, selected == Some(&beat.id), thumbnail);
            paint_analysis(ui, rect, beat, &markers);
            if let Some(range) = &markers.range
                && beat.frames > 0
            {
                let start = range.start.max(beat.start);
                let end = range.end.min(beat.start + beat.frames);
                if start < end {
                    let x = |at: u64| {
                        rect.left() + rect.width() * (at - beat.start) as f32 / beat.frames as f32
                    };
                    let highlight = egui::Rect::from_min_max(
                        egui::pos2(x(start), rect.bottom() - 6.0),
                        egui::pos2(x(end), rect.bottom()),
                    );
                    ui.painter().rect_filled(highlight, 1.0, style::LAVENDER);
                    ui.interact(
                        highlight,
                        ui.make_persistent_id(("edit-range", &beat.id)),
                        egui::Sense::hover(),
                    )
                    .widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Label,
                            true,
                            format!("Selected Edit range [{}..{}) in {}", start, end, beat.label),
                        )
                    });
                }
            }
            if let Some((marker_index, fraction)) = markers.cursor
                && marker_index == index
            {
                badge = Some(paint_cursor(ui, rect, fraction, markers.frame));
            }
            if response.clicked() {
                clicked = Some(index);
            }
        }
        edges(ui, canvas, origin.y, layout, beats.len(), viewport, badge);
    });
    clicked
}

/// Beats wholly outside the strip's viewport on each side. Card `i`
/// occupies `[i·pitch, (i+1)·pitch − 8)` on the strip canvas; a card the
/// edge cuts is visible, so it is faded rather than counted.
fn hidden_cards(count: usize, pitch: f32, viewport: egui::Rect) -> (usize, usize) {
    if count == 0 || pitch <= 0.0 {
        return (0, 0);
    }
    let before = ((viewport.min.x + 0.5 + 8.0) / pitch).floor().max(0.0) as usize;
    let reaching = ((viewport.max.x - 0.5) / pitch).ceil().max(0.0) as usize;
    (before.min(count), count - reaching.min(count))
}

/// Say how many beats lie past each edge of the strip's viewport, in the
/// cursor band above the cards, and fade a card the edge cuts off, so a
/// partly scrolled card never reads as the end of the edit. A count steps
/// aside from the cursor badge instead of covering it.
fn edges(
    ui: &egui::Ui,
    canvas: egui::Rect,
    cards_top: f32,
    layout: style::Layout,
    count: usize,
    viewport: egui::Rect,
    badge: Option<egui::Rect>,
) {
    let (before, after) = hidden_cards(count, layout.card_width, viewport);
    let visible = ui.clip_rect();
    let x = |canvas_x: f32| canvas.left() + canvas_x;
    let cards = egui::Rect::from_x_y_ranges(
        visible.x_range(),
        cards_top..=cards_top + layout.card_height,
    );
    // The first and last cards still in view: fade whichever the edge cuts.
    let first = x(before as f32 * layout.card_width);
    if before < count && first < visible.left() - 0.5 {
        let right = first + layout.card_width - 8.0;
        fade(ui, cards.with_max_x(right.min(visible.left() + FADE)), true);
    }
    let last = x((count - after) as f32 * layout.card_width - 8.0);
    if after < count && last > visible.right() + 0.5 {
        let left = last - layout.card_width + 8.0;
        fade(
            ui,
            cards.with_min_x(left.max(visible.right() - FADE)),
            false,
        );
    }
    if before > 0 {
        edge_count(ui, cards_top, visible, before, true, badge);
    }
    if after > 0 {
        edge_count(ui, cards_top, visible, after, false, badge);
    }
}

const FADE: f32 = 36.0;

/// A canvas-coloured gradient, opaque at the strip edge.
fn fade(ui: &egui::Ui, rect: egui::Rect, leading: bool) {
    let (outer, inner) = if leading {
        (rect.left(), rect.right())
    } else {
        (rect.right(), rect.left())
    };
    let mut mesh = egui::Mesh::default();
    for (x, color) in [(outer, style::CANVAS), (inner, egui::Color32::TRANSPARENT)] {
        mesh.colored_vertex(egui::pos2(x, rect.top()), color);
        mesh.colored_vertex(egui::pos2(x, rect.bottom()), color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    ui.painter().add(mesh);
}

fn edge_count(
    ui: &egui::Ui,
    cards_top: f32,
    visible: egui::Rect,
    hidden: usize,
    leading: bool,
    badge: Option<egui::Rect>,
) {
    let text = if leading {
        format!("‹ {hidden}")
    } else {
        format!("{hidden} ›")
    };
    let galley =
        ui.painter()
            .layout_no_wrap(text, egui::FontId::proportional(11.0), style::muted(ui));
    let size = galley.size() + egui::vec2(10.0, 2.0);
    let center_y = cards_top - 10.0;
    let mut chip = if leading {
        egui::Rect::from_min_size(egui::pos2(visible.left(), center_y - size.y / 2.0), size)
    } else {
        egui::Rect::from_min_size(
            egui::pos2(visible.right() - size.x, center_y - size.y / 2.0),
            size,
        )
    };
    if let Some(badge) = badge.filter(|badge| badge.expand(2.0).intersects(chip)) {
        chip = chip.translate(egui::vec2(
            if leading {
                badge.right() + 4.0 - chip.left()
            } else {
                badge.left() - 4.0 - chip.right()
            },
            0.0,
        ));
    }
    ui.painter().rect(
        chip,
        4.0,
        style::PANEL,
        egui::Stroke::new(1.0, accessibility::border(ui.ctx())),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(chip.min + egui::vec2(5.0, 1.0), galley, style::muted(ui));
    let label = format!(
        "{hidden} {} {} outside the visible strip",
        if hidden == 1 { "beat" } else { "beats" },
        if leading { "before" } else { "after" },
    );
    let response = ui.interact(
        chip,
        ui.make_persistent_id(("beat-strip-edge", leading)),
        egui::Sense::hover(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &label));
    response.on_hover_text(label);
}

/// Pause bands along a card's top edge and shot ticks below them, mapped
/// through the card's own frames. The top stays visible when a compact
/// window clips the card's bottom.
fn paint_analysis(ui: &egui::Ui, rect: egui::Rect, beat: &BeatRow, markers: &Markers) {
    if beat.frames == 0 {
        return;
    }
    let end = beat.start + beat.frames;
    let x = |at: u64| rect.left() + rect.width() * (at - beat.start) as f32 / beat.frames as f32;
    let first = markers
        .analysis
        .pauses
        .partition_point(|pause| pause.end <= beat.start);
    for pause in markers.analysis.pauses[first..]
        .iter()
        .take_while(|pause| pause.start < end)
    {
        let band = egui::Rect::from_min_max(
            egui::pos2(x(pause.start.max(beat.start)), rect.top() + 1.0),
            egui::pos2(x(pause.end.min(end)), rect.top() + 4.0),
        );
        ui.painter().rect_filled(band, 0.0, style::PAUSE_BAND);
    }
    let first = markers
        .analysis
        .shots
        .partition_point(|shot| *shot <= beat.start);
    for shot in markers.analysis.shots[first..]
        .iter()
        .take_while(|shot| **shot < end)
    {
        ui.painter().line_segment(
            [
                egui::pos2(x(*shot), rect.top() + 1.0),
                egui::pos2(x(*shot), rect.top() + 10.0),
            ],
            egui::Stroke::new(1.0, style::SHOT_TICK),
        );
    }
}

fn beat_card(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    beat: &BeatRow,
    index: usize,
    selected: bool,
    thumbnail: &mut dyn FnMut(&BeatRow) -> Option<Painted>,
) -> egui::Response {
    let response = workspace_card(
        ui,
        rect,
        ui.make_persistent_id(("beat-card", &beat.id)),
        selected,
        &format!(
            "Beat {}: {}, {}, {} frames, half-open frame boundaries {} to {}",
            index + 1,
            beat.label,
            beat.kind,
            beat.frames,
            beat.start,
            beat.start + beat.frames,
        ),
    );
    let compact = rect.height() <= 64.0;
    let mut inner = rect.shrink(if compact { 6.0 } else { 10.0 });
    if let Some(width) = thumbnail_width(beat.frames, inner.size(), compact) {
        let picture = egui::Rect::from_min_size(inner.min, egui::vec2(width, inner.height()));
        paint_thumbnail(ui, picture, thumbnail(beat));
        inner.min.x = picture.right() + thumbnail_gap(compact);
    }
    let mut content = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("beat-content", &beat.id))
            .max_rect(inner),
    );
    content.spacing_mut().item_spacing.y = if compact { 2.0 } else { 6.0 };
    content.add(
        egui::Label::new(
            egui::RichText::new(format!("{}. {}", index + 1, beat.label))
                .size(14.0)
                .color(if selected {
                    style::LAVENDER
                } else {
                    style::TEXT
                }),
        )
        .truncate()
        .selectable(false),
    );
    // Duration leads its line so a long kind truncates first; the half-open
    // boundaries have a line of their own.
    let mut duration = egui::text::LayoutJob::default();
    duration.append(
        &format!("{} f", beat.frames),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::monospace(11.5),
            color: style::TEXT,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    duration.append(
        &format!(" · {}", beat.kind),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(12.0),
            color: style::muted(ui),
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    content.add(egui::Label::new(duration).truncate().selectable(false));
    content.add(
        egui::Label::new(
            egui::RichText::new(format!("{}–{}", beat.start, beat.start + beat.frames))
                .monospace()
                .size(11.5)
                .weak(),
        )
        .truncate()
        .selectable(false),
    );
    response
}

fn thumbnail_gap(compact: bool) -> f32 {
    if compact { 8.0 } else { 10.0 }
}

/// A card's thumbnail width, or none. Exact durations and boundaries outrank
/// the picture: the text keeps at least its minimum width and the picture
/// narrows to the rest, down to its own minimum. Zero-duration beats have no
/// picture of their own.
fn thumbnail_width(frames: u64, inner: egui::Vec2, compact: bool) -> Option<f32> {
    let width = (inner.y * 16.0 / 9.0)
        .min(inner.x * 0.42)
        .min(inner.x - thumbnail_gap(compact) - MIN_TEXT_WIDTH);
    (frames > 0 && width >= MIN_THUMBNAIL_WIDTH).then_some(width)
}

fn workspace_card(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id: egui::Id,
    selected: bool,
    label: &str,
) -> egui::Response {
    // The reserved canvas is authoritative for paint, hit testing and AX.
    // An empty Button adds an unnecessary intrinsic-size layout; content-based
    // button IDs also change when the same beat moves to a different position.
    let response = ui.interact(rect, id, egui::Sense::click());
    ui.painter().rect(
        rect,
        6.0,
        if selected {
            style::SELECTED
        } else {
            style::PANEL
        },
        egui::Stroke::new(
            if selected { 1.5 } else { 1.0 },
            if selected {
                style::LAVENDER
            } else {
                accessibility::border(ui.ctx())
            },
        ),
        egui::StrokeKind::Inside,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label)
    });
    if response.has_focus() || response.hovered() {
        ui.painter().rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.0, style::LAVENDER),
            egui::StrokeKind::Inside,
        );
    }
    response.on_hover_text(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Text and fills one thumbnail paints.
    fn thumbnail_paint(painted: Option<Painted>) -> (Vec<String>, Vec<egui::Color32>) {
        let context = egui::Context::default();
        style::apply(&context);
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let rect = egui::Rect::from_min_size(ui.min_rect().min, egui::vec2(70.0, 52.0));
                paint_thumbnail(ui, rect, painted);
            });
        });
        output.textures_delta.clear();
        let mut texts = Vec::new();
        let mut fills = Vec::new();
        for clipped in output.shapes {
            match clipped.shape {
                egui::Shape::Text(text) => texts.push(text.galley.text().to_owned()),
                egui::Shape::Rect(rect) => fills.push(rect.fill),
                _ => {}
            }
        }
        (texts, fills)
    }

    #[test]
    fn loading_and_failed_thumbnails_never_look_like_an_authored_background() {
        let (loading, loading_fills) = thumbnail_paint(None);
        let (failed, failed_fills) = thumbnail_paint(Some(Painted::Unavailable));
        let (background, background_fills) =
            thumbnail_paint(Some(Painted::Black { aspect: 16.0 / 9.0 }));
        assert_eq!(loading, [THUMBNAIL_LOADING]);
        assert_eq!(failed, [THUMBNAIL_UNAVAILABLE]);
        assert!(background.is_empty());
        assert!(loading_fills.contains(&style::PANEL_RAISED));
        assert!(failed_fills.contains(&style::PANEL_RAISED));
        assert!(!loading_fills.contains(&egui::Color32::BLACK));
        assert!(background_fills.contains(&egui::Color32::BLACK));
        assert!(!background_fills.contains(&style::PANEL_RAISED));
    }

    #[test]
    fn compact_cards_keep_their_text_width_and_gain_a_thumbnail() {
        // A 960-point window's 64-point card: 202 × 64 less 6-point padding.
        let compact = thumbnail_width(120, egui::vec2(190.0, 52.0), true);
        assert!(compact.is_some_and(
            |width| width >= MIN_THUMBNAIL_WIDTH && 190.0 - width - 8.0 >= MIN_TEXT_WIDTH
        ));
        // Ordinary cards keep their previous widths.
        assert_eq!(
            thumbnail_width(120, egui::vec2(238.0, 72.0), false),
            Some(238.0 * 0.42)
        );
        assert_eq!(thumbnail_width(120, egui::vec2(182.0, 72.0), false), None);
        assert_eq!(thumbnail_width(0, egui::vec2(190.0, 52.0), true), None);
    }

    #[test]
    fn hidden_cards_count_only_wholly_hidden_beats_on_each_side() {
        let viewport = |left: f32, width: f32| {
            egui::Rect::from_min_size(egui::pos2(left, 0.0), egui::vec2(width, 84.0))
        };
        // Seven 210-point cards in a 600-point strip. Card 2 at [420, 622)
        // is cut, not hidden.
        assert_eq!(hidden_cards(7, 210.0, viewport(0.0, 600.0)), (0, 4));
        assert_eq!(hidden_cards(7, 210.0, viewport(870.0, 600.0)), (4, 0));
        assert_eq!(hidden_cards(7, 210.0, viewport(420.0, 600.0)), (2, 2));
        // A viewport starting inside the gap after card 3 hides it wholly.
        assert_eq!(hidden_cards(7, 210.0, viewport(836.0, 600.0)), (4, 0));
        assert_eq!(hidden_cards(0, 210.0, viewport(0.0, 600.0)), (0, 0));
        assert_eq!(hidden_cards(2, 210.0, viewport(0.0, 600.0)), (0, 0));
    }

    #[test]
    fn resize_keeps_the_selected_card_visible_when_card_pitch_changes() {
        let context = egui::Context::default();
        style::apply(&context);
        let beats = (0..50)
            .map(|index| BeatRow {
                id: NodeId::new(format!("beat-{index}")).unwrap(),
                label: format!("Beat {index}"),
                kind: "Hold".into(),
                start: index * 12,
                frames: 12,
            })
            .collect::<Vec<_>>();
        for (size, reveal) in [
            (egui::vec2(1280.0, 820.0), true),
            (egui::vec2(960.0, 640.0), false),
        ] {
            let layout = style::Layout::for_size(size.x, size.y);
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        strip(
                            ui,
                            layout,
                            &beats,
                            Some(&beats[30].id),
                            Markers {
                                frame: 360,
                                ..Markers::default()
                            },
                            reveal,
                            &mut |_| None,
                        );
                    });
                },
            );
            output.textures_delta.clear();
            assert!(
                output.shapes.iter().any(|clipped| {
                    matches!(&clipped.shape, egui::Shape::Rect(shape)
                    if shape.fill == style::SELECTED && clipped.clip_rect.contains_rect(shape.rect))
                }),
                "selection disappeared after resize to {size:?}"
            );
        }
    }

    #[test]
    fn undo_then_insert_after_first_preserves_each_card_and_its_text() {
        let context = egui::Context::default();
        style::apply(&context);
        let size = egui::vec2(1492.0, 929.0);
        let layout = style::Layout::for_size(size.x, size.y);
        let steps: &[(&[u8], u8)] = &[
            (&[0, 1, 2, 3], 0),
            (&[0, 1, 2, 3], 0),
            (&[0, 1, 2], 0),
            (&[0, 1, 2], 0),
            (&[0, 4, 1, 2], 4),
            (&[0, 4, 1, 2], 2),
            (&[0, 4, 1, 2], 1),
        ];
        for (order, selected) in steps {
            let beats: Vec<_> = order
                .iter()
                .enumerate()
                .map(|(index, id)| BeatRow {
                    id: NodeId::new(format!("beat-{id}")).unwrap(),
                    label: "cfr-bframes.mp4".into(),
                    kind: "Source".into(),
                    start: index as u64 * 120,
                    frames: 120,
                })
                .collect();
            let selected = NodeId::new(format!("beat-{selected}")).unwrap();
            let marker = beats.iter().position(|beat| beat.id == selected).unwrap();
            let mut viewport = egui::Rect::NOTHING;
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events: vec![egui::Event::PointerMoved(egui::pos2(999.0, 765.0))],
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .native_pixels_per_point = Some(2.0);
            let mut output = context.run_ui(input, |ui| {
                egui::Panel::top("header").exact_size(72.0).show(ui, |_| {});
                egui::Panel::bottom("footer")
                    .exact_size(88.0)
                    .show(ui, |_| {});
                egui::Panel::left("sources")
                    .exact_size(layout.sources)
                    .show(ui, |_| {});
                style::beat_panel(layout).show(ui, |ui| {
                    super::super::scope::draw_heading(
                        ui,
                        true,
                        true,
                        &[],
                        beats.len(),
                        beats.len() as u64 * 120,
                        &crate::navigation::Bindings::default(),
                    );
                    viewport = ui.available_rect_before_wrap();
                    strip(
                        ui,
                        layout,
                        &beats,
                        Some(&selected),
                        Markers {
                            cursor: Some((marker, 0.0)),
                            frame: beats[marker].start,
                            range: None,
                            ..Markers::default()
                        },
                        true,
                        &mut |_| None,
                    );
                });
                egui::CentralPanel::default().show(ui, |_| {});
            });
            output.textures_delta.clear();
            let mut drawn = 0;
            let mut text_rows = 0;
            let mut card_fills = Vec::new();
            for clipped in &output.shapes {
                if let egui::Shape::Rect(shape) = &clipped.shape
                    && (shape.rect.height() - layout.card_height).abs() < 0.1
                    && [style::PANEL, style::SELECTED].contains(&shape.fill)
                {
                    assert!(
                        (shape.rect.width() - (layout.card_width - 8.0)).abs() < 0.1,
                        "order {order:?}: card {:?}",
                        shape.rect
                    );
                    assert!(clipped.clip_rect.contains_rect(shape.rect));
                    card_fills.push((shape.rect, shape.fill));
                    drawn += 1;
                }
                if let egui::Shape::Text(shape) = &clipped.shape
                    && (shape.galley.text().contains(". cfr-bframes.mp4")
                        || shape.galley.text().contains(" f · ")
                        || shape.galley.text().contains('–'))
                {
                    assert!(viewport.contains_rect(shape.visual_bounding_rect()));
                    assert!(
                        clipped
                            .clip_rect
                            .contains_rect(shape.visual_bounding_rect())
                    );
                    text_rows += 1;
                }
            }
            assert_eq!(drawn, beats.len(), "order {order:?}");
            assert_eq!(text_rows, beats.len() * 3, "order {order:?}");
            let primitives = context.tessellate(output.shapes, output.pixels_per_point);
            for (rect, color) in card_fills {
                for fraction in [0.1, 0.5, 0.9] {
                    let point = egui::pos2(egui::lerp(rect.x_range(), fraction), rect.center().y);
                    assert!(
                        mesh_covers(&primitives, point, color),
                        "missing card mesh at {point:?}, order {order:?}"
                    );
                }
            }
        }
    }

    fn mesh_covers(
        primitives: &[egui::ClippedPrimitive],
        point: egui::Pos2,
        color: egui::Color32,
    ) -> bool {
        primitives.iter().any(|clipped| {
            if !clipped.clip_rect.contains(point) {
                return false;
            }
            let egui::epaint::Primitive::Mesh(mesh) = &clipped.primitive else {
                return false;
            };
            mesh.indices.chunks_exact(3).any(|triangle| {
                let vertices = [triangle[0], triangle[1], triangle[2]]
                    .map(|index| mesh.vertices[index as usize]);
                if !vertices.iter().all(|vertex| vertex.color == color) {
                    return false;
                }
                let [a, b, c] = [vertices[0].pos, vertices[1].pos, vertices[2].pos];
                let cross =
                    |a: egui::Pos2, b: egui::Pos2, p: egui::Pos2| (b - a).rot90().dot(p - a);
                if cross(a, b, c).abs() < f32::EPSILON {
                    return false;
                }
                let signs = [cross(a, b, point), cross(b, c, point), cross(c, a, point)];
                signs.iter().all(|sign| *sign >= 0.0) || signs.iter().all(|sign| *sign <= 0.0)
            })
        })
    }

    #[test]
    fn card_identity_and_keyboard_focus_follow_its_node_after_a_move() {
        let context = egui::Context::default();
        style::apply(&context);
        let mut id = None;
        for index in 0..2 {
            let beat = BeatRow {
                id: NodeId::new("stable-beat").unwrap(),
                label: "Original".into(),
                kind: "Source".into(),
                start: index as u64 * 120,
                frames: 120,
            };
            let input = egui::RawInput {
                events: if index == 1 {
                    vec![egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }]
                } else {
                    Vec::new()
                },
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                let rect = egui::Rect::from_min_size(
                    egui::pos2(10.0 + index as f32 * 220.0, 10.0),
                    egui::vec2(210.0, 92.0),
                );
                let response = beat_card(ui, rect, &beat, index, true, &mut |_| None);
                assert_eq!(response.rect, rect);
                if index == 0 {
                    id = Some(response.id);
                    response.request_focus();
                } else {
                    assert_eq!(Some(response.id), id);
                    assert!(response.has_focus());
                    assert!(response.clicked());
                }
            });
            output.textures_delta.clear();
        }
    }

    #[test]
    fn appending_beats_keeps_every_fitting_card_visible_on_the_first_frame() {
        for size in [egui::vec2(1280.0, 820.0), egui::vec2(1492.0, 929.0)] {
            let context = egui::Context::default();
            style::apply(&context);
            let layout = style::Layout::for_size(size.x, size.y);
            let mut beats = Vec::new();
            for index in 0..4 {
                beats.push(BeatRow {
                    id: NodeId::new(format!("beat-{index}")).unwrap(),
                    label: format!("Original {index}"),
                    kind: "Source".into(),
                    start: index * 120,
                    frames: 120,
                });
                // One frame per append, with the same retained scroll state.
                // Waiting for a second frame would hide an invalid reveal offset.
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| {
                        egui::Panel::top("header").exact_size(72.0).show(ui, |_| {});
                        egui::Panel::bottom("footer")
                            .exact_size(88.0)
                            .show(ui, |_| {});
                        egui::Panel::left("sources")
                            .exact_size(layout.sources)
                            .show(ui, |_| {});
                        style::beat_panel(layout).show(ui, |ui| {
                            super::super::scope::draw_heading(
                                ui,
                                true,
                                true,
                                &[],
                                beats.len(),
                                beats.len() as u64 * 120,
                                &crate::navigation::Bindings::default(),
                            );
                            strip(
                                ui,
                                layout,
                                &beats,
                                Some(&beats.last().unwrap().id),
                                Markers::default(),
                                true,
                                &mut |_| None,
                            );
                        });
                        egui::CentralPanel::default().show(ui, |_| {});
                    },
                );
                output.textures_delta.clear();
                let mut drawn = 0;
                for clipped in &output.shapes {
                    if let egui::Shape::Rect(shape) = &clipped.shape
                        && (shape.rect.height() - layout.card_height).abs() < 0.1
                        && [style::PANEL, style::SELECTED].contains(&shape.fill)
                    {
                        assert!(
                            clipped.clip_rect.contains_rect(shape.rect),
                            "append {} at {size:?}: card {:?}, clip {:?}",
                            beats.len(),
                            shape.rect,
                            clipped.clip_rect
                        );
                        drawn += 1;
                    }
                }
                assert_eq!(drawn, beats.len(), "append {} at {size:?}", beats.len());
            }
        }
    }

    #[test]
    fn beat_headers_and_card_rows_remain_visible_below_the_viewer() {
        for size in [
            egui::vec2(960.0, 640.0),
            egui::vec2(960.0, 699.0),
            egui::vec2(960.0, 700.0),
            egui::vec2(1492.0, 929.0),
        ] {
            for (count, group_labels) in [
                (1, vec![]),
                (2, vec![]),
                (4, vec!["Outer group".into(), "Inner group".into()]),
                (
                    9,
                    vec!["A long group name that needs horizontal scrolling".into(); 16],
                ),
            ] {
                for reveal in [false, true] {
                    let context = egui::Context::default();
                    style::apply(&context);
                    let layout = style::Layout::for_size(size.x, size.y);
                    let beats: Vec<_> = (0..count)
                        .map(|index| BeatRow {
                            id: NodeId::new(format!("beat-{index}")).unwrap(),
                            label: format!("Original {index}"),
                            kind: "Source".into(),
                            start: index as u64 * 120,
                            frames: 120,
                        })
                        .collect();
                    // Repeated frames exercise the retained horizontal offset as
                    // well as the first-frame panel/scroll-area sizing pass.
                    for _ in 0..3 {
                        let mut panel = egui::Rect::NOTHING;
                        let mut viewer = egui::Rect::NOTHING;
                        let mut title = egui::Rect::NOTHING;
                        let mut output = context.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    size,
                                )),
                                ..Default::default()
                            },
                            |ui| {
                                egui::Panel::top("header").exact_size(72.0).show(ui, |_| {});
                                egui::Panel::bottom("footer")
                                    .exact_size(88.0)
                                    .show(ui, |_| {});
                                egui::Panel::left("sources")
                                    .exact_size(layout.sources)
                                    .show(ui, |_| {});
                                panel = style::beat_panel(layout)
                                    .show(ui, |ui| {
                                        // Match the production timeline's heading-to-strip gap.
                                        ui.spacing_mut().item_spacing.y = 4.0;
                                        title = super::super::scope::draw_heading(
                                            ui,
                                            true,
                                            true,
                                            &group_labels,
                                            count,
                                            count as u64 * 120,
                                            &crate::navigation::Bindings::default(),
                                        )
                                        .0
                                        .rect;
                                        strip(
                                            ui,
                                            layout,
                                            &beats,
                                            Some(&beats[count - 1].id),
                                            Markers {
                                                cursor: Some((count - 1, 0.0)),
                                                frame: beats[count - 1].start,
                                                range: None,
                                                ..Markers::default()
                                            },
                                            reveal,
                                            &mut |_| None,
                                        );
                                    })
                                    .response
                                    .rect;
                                viewer = egui::CentralPanel::default()
                                    .show(ui, |ui| {
                                        ui.painter().rect_filled(ui.max_rect(), 0.0, style::CANVAS);
                                    })
                                    .response
                                    .rect;
                            },
                        );
                        output.textures_delta.clear();
                        assert!(
                            panel.contains_rect(title),
                            "title {title:?}, panel {panel:?}, size {size:?}"
                        );
                        assert!(
                            viewer.bottom() <= title.top(),
                            "viewer {viewer:?} covers title {title:?}"
                        );
                        assert!(panel.height() <= layout.beats + 0.1);
                        let mut card_labels = 0;
                        let mut fully_visible_cards = 0;
                        let mut cards = Vec::new();
                        let mut cursor_badges = 0;
                        for clipped in &output.shapes {
                            if let egui::Shape::Rect(shape) = &clipped.shape
                                && shape.fill == style::CURSOR
                            {
                                assert!(panel.contains_rect(shape.rect));
                                assert!(shape.rect.top() >= title.bottom());
                                assert!(clipped.clip_rect.contains_rect(shape.rect));
                                cursor_badges += 1;
                            }
                            if let egui::Shape::Rect(shape) = &clipped.shape
                                && (shape.rect.height() - layout.card_height).abs() < 0.1
                                && [style::PANEL, style::SELECTED].contains(&shape.fill)
                            {
                                assert!(shape.rect.top() >= title.bottom());
                                assert!(shape.rect.bottom() <= panel.bottom());
                                assert!(shape.rect.top() >= clipped.clip_rect.top());
                                assert!(shape.rect.bottom() <= clipped.clip_rect.bottom());
                                if clipped.clip_rect.contains_rect(shape.rect) {
                                    fully_visible_cards += 1;
                                }
                                cards.push(shape.rect);
                            }
                        }
                        for clipped in &output.shapes {
                            if let egui::Shape::Text(shape) = &clipped.shape
                                && (shape.galley.text().contains(". Original")
                                    || shape.galley.text().contains(" f · ")
                                    || shape.galley.text().contains('–'))
                            {
                                let text = shape.visual_bounding_rect();
                                assert!(text.top() >= title.bottom());
                                assert!(text.top() >= viewer.bottom());
                                assert!(
                                    text.top() >= clipped.clip_rect.top(),
                                    "clipped text: {}",
                                    shape.galley.text()
                                );
                                assert!(
                                    text.bottom() <= clipped.clip_rect.bottom(),
                                    "clipped text: {}",
                                    shape.galley.text()
                                );
                                assert!(
                                    cards.iter().any(|card| card.contains_rect(text)),
                                    "text {text:?} outside cards {cards:?}: {}",
                                    shape.galley.text()
                                );
                                card_labels += 1;
                            }
                        }
                        assert!(
                            !cards.is_empty(),
                            "no cards at size {size:?}, count {count}, reveal {reveal}"
                        );
                        // One extra virtualized card may lie wholly outside the
                        // horizontal viewport; egui culls that card's labels.
                        assert!(fully_visible_cards > 0);
                        assert!(card_labels >= fully_visible_cards * 3);
                        if reveal || count == 1 {
                            assert_eq!(cursor_badges, 1);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn original_caption_follows_the_whole_card_at_both_sidebar_widths() {
        for width in [144.0, 196.0] {
            let context = egui::Context::default();
            style::apply(&context);
            let mut card = egui::Rect::NOTHING;
            let mut caption = egui::Rect::NOTHING;
            let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                egui::Panel::left("sources")
                    .exact_size(width)
                    .frame(style::panel())
                    .show(ui, |ui| {
                        card = original(
                            ui,
                            "cfr-bframes.mp4",
                            "120 decoded video frames",
                            true,
                            None,
                        )
                        .rect;
                        caption = ui
                            .label(
                                egui::RichText::new("Your starting point stays intact.").size(12.0),
                            )
                            .rect;
                    });
            });
            output.textures_delta.clear();
            assert!(
                caption.top() >= card.bottom() + 8.0,
                "caption {caption:?} overlaps card {card:?}"
            );
            let mut rows = 0;
            for clipped in output.shapes {
                if let egui::Shape::Text(shape) = clipped.shape
                    && ["cfr-bframes.mp4", "120 decoded video frames"]
                        .contains(&shape.galley.text())
                {
                    rows += 1;
                    assert!(
                        card.contains_rect(shape.visual_bounding_rect()),
                        "{} outside Original card",
                        shape.galley.text()
                    );
                }
            }
            assert_eq!(rows, 2);
        }
    }
}
