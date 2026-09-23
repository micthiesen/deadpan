//! Fixed card canvases: overlay widgets must not advance their parent's flow.

use eframe::egui;

use super::{BeatRow, NodeId, paint_cursor, style};

pub(super) fn heading(
    ui: &mut egui::Ui,
    title: &str,
    focused: bool,
    count: usize,
    frames: u64,
    frame_rate: Option<deadpan_core::FrameRate>,
) -> egui::Response {
    ui.horizontal_wrapped(|ui| {
        let heading = ui.label(egui::RichText::new(title).size(13.0).strong());
        if focused {
            ui.label(
                egui::RichText::new("FOCUS")
                    .size(9.0)
                    .color(style::LAVENDER),
            );
        }
        ui.label(
            egui::RichText::new(format!("{count} root beats · {frames} frames"))
                .color(style::MUTED)
                .size(12.0),
        );
        if let Some(rate) = frame_rate {
            ui.label(
                egui::RichText::new(super::frame_rate_label(rate))
                    .color(style::MUTED)
                    .size(12.0),
            );
        }
        heading
    })
    .inner
}

pub(super) fn original(
    ui: &mut egui::Ui,
    label: &str,
    detail: &str,
    selected: bool,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 106.0),
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
    let mut content = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(12.0)));
    egui::Frame::new()
        .fill(style::SELECTED)
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(5, 8))
        .show(&mut content, |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new("ORIGINAL VIDEO")
                        .monospace()
                        .size(10.0)
                        .color(style::LAVENDER),
                )
                .truncate()
                .selectable(false),
            );
        });
    content.add(
        egui::Label::new(egui::RichText::new(label).strong())
            .truncate()
            .selectable(false),
    );
    content.add(
        egui::Label::new(egui::RichText::new(detail).size(11.0).color(style::MUTED))
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
    marker: Option<(usize, f32)>,
    cursor: u64,
    reveal: bool,
) -> Option<usize> {
    let canvas_width = beats.len() as f32 * layout.card_width;
    let mut scroll = egui::ScrollArea::horizontal().id_salt("beat-strip");
    if reveal && let Some(index) = beats.iter().position(|beat| Some(&beat.id) == selected) {
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
        for (index, beat) in beats.iter().enumerate().take(end).skip(start) {
            let rect = egui::Rect::from_min_size(
                origin + egui::vec2(index as f32 * layout.card_width, 0.0),
                egui::vec2(layout.card_width - 8.0, layout.card_height),
            );
            let response = beat_card(ui, rect, beat, index, selected == Some(&beat.id));
            if let Some((marker_index, fraction)) = marker
                && marker_index == index
            {
                paint_cursor(ui, rect, fraction, cursor);
            }
            if response.clicked() {
                clicked = Some(index);
            }
        }
    });
    clicked
}

fn beat_card(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    beat: &BeatRow,
    index: usize,
    selected: bool,
) -> egui::Response {
    let response = workspace_card(
        ui,
        rect,
        ui.make_persistent_id(("beat-card", &beat.id)),
        selected,
        &format!(
            "Root beat {}: {}, {}, {} frames, half-open frame boundaries {} to {}",
            index + 1,
            beat.label,
            beat.kind,
            beat.frames,
            beat.start,
            beat.start + beat.frames,
        ),
    );
    let mut content = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("beat-content", &beat.id))
            .max_rect(rect.shrink(12.0)),
    );
    content.spacing_mut().item_spacing.y = 6.0;
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
    content.add(
        egui::Label::new(
            egui::RichText::new(&beat.kind)
                .size(12.0)
                .color(style::MUTED),
        )
        .truncate()
        .selectable(false),
    );
    content.add(
        egui::Label::new(
            egui::RichText::new(format!(
                "{} f · {}–{}",
                beat.frames,
                beat.start,
                beat.start + beat.frames,
            ))
            .monospace()
            .size(11.5),
        )
        .truncate()
        .selectable(false),
    );
    response
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
                style::BORDER
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
                    heading(
                        ui,
                        "YOUR EDIT",
                        true,
                        beats.len(),
                        beats.len() as u64 * 120,
                        Some(deadpan_core::FrameRate::new(24, 1).unwrap()),
                    );
                    viewport = ui.available_rect_before_wrap();
                    strip(
                        ui,
                        layout,
                        &beats,
                        Some(&selected),
                        Some((marker, 0.0)),
                        beats[marker].start,
                        true,
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
                        || shape.galley.text() == "Source"
                        || shape.galley.text().contains(" f · "))
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
                let response = beat_card(ui, rect, &beat, index, true);
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
                            heading(
                                ui,
                                "YOUR EDIT",
                                true,
                                beats.len(),
                                beats.len() as u64 * 120,
                                None,
                            );
                            strip(
                                ui,
                                layout,
                                &beats,
                                Some(&beats.last().unwrap().id),
                                None,
                                0,
                                true,
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
        for size in [egui::vec2(960.0, 640.0), egui::vec2(1492.0, 929.0)] {
            for count in [1, 2, 4, 9] {
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
                                        title = heading(
                                            ui,
                                            "YOUR EDIT",
                                            true,
                                            count,
                                            count as u64 * 120,
                                            Some(deadpan_core::FrameRate::new(24, 1).unwrap()),
                                        )
                                        .rect;
                                        strip(
                                            ui,
                                            layout,
                                            &beats,
                                            Some(&beats[count - 1].id),
                                            Some((count - 1, 0.0)),
                                            beats[count - 1].start,
                                            reveal,
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
                        for clipped in &output.shapes {
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
                                    || shape.galley.text() == "Source"
                                    || shape.galley.text().contains(" f · "))
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
                        card =
                            original(ui, "cfr-bframes.mp4", "120 decoded video frames", true).rect;
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
                    && [
                        "ORIGINAL VIDEO",
                        "cfr-bframes.mp4",
                        "120 decoded video frames",
                    ]
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
            assert_eq!(rows, 3);
        }
    }
}
