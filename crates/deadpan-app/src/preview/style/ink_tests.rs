use super::*;

struct TitlePaint {
    text: egui::epaint::TextShape,
    clip: egui::Rect,
    viewport: egui::Rect,
    following: egui::Rect,
    response: egui::Rect,
}

fn title_paint(label: &str, width: f32, scale: f32, padded: bool, scoped: bool) -> TitlePaint {
    let context = egui::Context::default();
    apply(&context);
    super::super::fonts::install(&context);
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 320.0));
    let mut input = egui::RawInput {
        screen_rect: Some(viewport),
        ..Default::default()
    };
    let native = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    native.native_pixels_per_point = Some(scale);
    native.inner_rect = Some(viewport);
    let mut following = egui::Rect::NOTHING;
    let mut response = egui::Rect::NOTHING;
    let mut output = context.run_ui(input, |ui| {
        ui.label("INSPECTOR");
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut title = |ui: &mut egui::Ui| {
                    let rich = if scoped {
                        RichText::new(label).heading()
                    } else {
                        RichText::new(label).size(17.0)
                    };
                    response = if padded {
                        ink_padded_label(
                            ui,
                            rich,
                            (!scoped).then_some(egui::TextWrapMode::Truncate),
                        )
                    } else if scoped {
                        ui.label(rich)
                    } else {
                        ui.add(egui::Label::new(rich).truncate())
                    }
                    .rect;
                    following = ui.label("Sequence").rect;
                };
                if scoped {
                    title(ui);
                } else {
                    ui.horizontal(|ui| {
                        egui::Frame::new()
                            .fill(SELECTED)
                            .corner_radius(6)
                            .inner_margin(10)
                            .show(ui, |ui| {
                                ui.label(RichText::new("≡").size(24.0).color(LAVENDER));
                            });
                        ui.vertical(&mut title);
                    });
                }
            });
    });
    assert_eq!(context.pixels_per_point(), scale);
    assert_eq!(context.input(|input| input.content_rect()), viewport);
    context.fonts_mut(|fonts| {
        assert!(fonts.has_glyphs(&egui::FontId::proportional(17.0), "答え日本語"));
    });
    output.textures_delta.clear();
    let matching = output
        .shapes
        .into_iter()
        .filter_map(|clipped| {
            let egui::Shape::Text(text) = clipped.shape else {
                return None;
            };
            (text.galley.text() == label).then_some((text, clipped.clip_rect))
        })
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1, "Missing painted title {label:?}");
    let (text, clip) = matching.into_iter().next().unwrap();
    TitlePaint {
        text,
        clip,
        viewport,
        following,
        response,
    }
}

#[test]
fn real_cjk_inspector_title_overflows_unpadded_first_scroll_row() {
    let label = "l'été 答え \"oui\"";
    let mut clipped_scales = Vec::new();
    for scale in [1.0, 1.25, 2.0] {
        let paint = title_paint(label, 236.0, scale, false, false);
        if paint.text.visual_bounding_rect().top() < paint.clip.top() {
            clipped_scales.push(scale);
        }
    }
    assert!(
        !clipped_scales.is_empty(),
        "The installed fonts no longer reproduce the unpadded inspector clipping"
    );
}

#[test]
fn padded_ascii_and_cjk_titles_fit_actual_scroll_clip_at_supported_scales() {
    for scoped in [false, true] {
        for width in [192.0, 236.0] {
            for scale in [1.0, 1.25, 2.0] {
                for label in ["Answer", "l'été 答え \"oui\"", "答え 日本語 日本語 日本語"]
                {
                    let paint = title_paint(label, width, scale, true, scoped);
                    let bounds = paint.text.visual_bounding_rect();
                    assert!(
                        paint.clip.contains_rect(bounds) && paint.viewport.contains_rect(bounds),
                        "{label:?}, width {width}, scale {scale}, scoped {scoped}: \
                         painted {bounds:?}, clip {:?}, viewport {:?}",
                        paint.clip,
                        paint.viewport
                    );
                    assert!(bounds.bottom() <= paint.following.top());
                    assert_eq!(
                        paint.text.galley.job.wrap.max_rows,
                        if scoped { usize::MAX } else { 1 }
                    );
                }
            }
        }
    }
}

#[test]
fn ascii_inspector_title_keeps_its_existing_geometry_when_ink_fits() {
    for scale in [1.0, 1.25, 2.0] {
        let old = title_paint("Answer", 236.0, scale, false, false);
        let new = title_paint("Answer", 236.0, scale, true, false);
        assert!(old.text.galley.mesh_bounds.top() >= old.text.galley.rect.top());
        assert!(old.text.galley.mesh_bounds.bottom() <= old.text.galley.rect.bottom());
        assert_eq!(old.text.pos, new.text.pos);
        assert_eq!(old.text.galley.as_ref(), new.text.galley.as_ref());
        assert_eq!(old.response, new.response);
        assert_eq!(old.following, new.following);
        assert_eq!(old.clip, new.clip);
    }
}
