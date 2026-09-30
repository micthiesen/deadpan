use super::*;

impl DeadpanApp {
    pub(in crate::preview) fn splice_keyboard(&mut self, context: &egui::Context) {
        let events = context.input(|input| input.events.clone());
        let composing = self.ime_composing
            || events
                .iter()
                .any(|event| matches!(event, egui::Event::Ime(_)));
        help_scroll::observe_composition(&events, &mut self.ime_composing);
        if composing || self.ime_composing {
            context.input_mut(|input| {
                input.events.retain(|event| {
                    !matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Enter | egui::Key::Space | egui::Key::Escape,
                            ..
                        }
                    )
                })
            });
            return;
        }
        if self.dialogs.is_open()
            || pointer_focus_transition(&events)
            || events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::Tab,
                        pressed: true,
                        ..
                    }
                )
            })
        {
            return;
        }
        let background = context.memory(|memory| {
            memory
                .focused()
                .is_none_or(|id| id == egui::Id::new(FOCUS) || id == pane_id(self.pane))
        });
        let Some(draft) = &mut self.splice else {
            return;
        };
        // Keep every routed press in native order. consume_key removes all
        // matching presses, which loses duplicate count digits in one batch.
        // Native widgets receive only the events they still own.
        context.input_mut(|input| {
            input.events.retain(|event| {
                let egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    repeat,
                    ..
                } = event
                else {
                    return true;
                };
                if let Some(action) = navigation::splice::route_key(
                    *key, *modifiers, false, background, false, *repeat,
                ) {
                    draft.keys.push(action);
                    false
                } else {
                    true
                }
            });
        });
    }

    pub(in crate::preview) fn splice_workspace(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().frame(style::panel()).show(ui, |ui| {
            let Some(mut draft) = self.splice.take() else { return; };
            let keys = std::mem::take(&mut draft.keys);
            let mut action = None;
            let heading = ui.horizontal(|ui| {
                ui.heading("Place slice");
                ui.colored_label(style::LAVENDER, "UNSAVED · Insert · Linked picture + sound");
                if let Some(count) = draft.count { ui.monospace(format!("COUNT {count}")); }
            }).response;
            let focus = ui.interact(heading.rect, egui::Id::new(FOCUS), egui::Sense::click());
            focus.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Place slice keyboard controls"));
            if draft.focus_pending {
                focus.request_focus(); draft.focus_pending = false;
            }
            if focus.has_focus() { ui.painter().rect_stroke(heading.rect, 2.0, egui::Stroke::new(1.0, style::LAVENDER), egui::StrokeKind::Inside); }
            let enabled = !draft.invalidated && !draft.applying;
            ui.horizontal_wrapped(|ui| {
                for (label, selected, key) in [
                    (format!("In {} · i", draft.proposal.ordinals.start), draft.focus == Focus::In, SpliceKey::In),
                    (format!("Out {} exclusive · o", draft.proposal.ordinals.end), draft.focus == Focus::Out, SpliceKey::Out),
                    (format!("Destination Edit {} · d", draft.destination), draft.focus == Focus::Destination, SpliceKey::Destination),
                    ("Inspect picture · f".into(), draft.focus == Focus::Picture, SpliceKey::Picture),
                ] {
                    if ui.add_enabled(enabled, egui::Button::new(label).selected(selected)).clicked() { action = Some(key); }
                }
                if ui.add_enabled(enabled, egui::Button::new("Previous seam · k")).clicked() { action = Some(SpliceKey::Boundary(false)); }
                if ui.add_enabled(enabled, egui::Button::new("Next seam · j")).clicked() { action = Some(SpliceKey::Boundary(true)); }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(enabled, egui::Button::new("−1 · h")).on_hover_text("Previous frame in the selected control").clicked() { action = Some(SpliceKey::Step(false)); }
                if ui.add_enabled(enabled, egui::Button::new("+1 · l")).on_hover_text("Next frame in the selected control").clicked() { action = Some(SpliceKey::Step(true)); }
                let ready = enabled && draft.prepared.is_some() && !draft.dirty && draft.pending.is_none();
                if ui.add_enabled(ready, egui::Button::new(if draft.before { "Before · b" } else { "Proposed · b" }).selected(!draft.before)).clicked() { action = Some(SpliceKey::Compare); }
                if ui.add_enabled(ready || self.transport.is_some(), egui::Button::new(if self.transport.is_some() { "Pause · Space" } else { "Audition · Space" })).clicked() { action = Some(SpliceKey::Play); }
                if ui.add_enabled(ready, egui::Button::new("Loop both joins · Shift Space")).clicked() { action = Some(SpliceKey::Loop); }
                if ui.add_enabled(ready, egui::Button::new("Place slice · Enter").fill(style::SELECTED)).clicked() { action = Some(SpliceKey::Apply); }
                if ui.add_enabled(!draft.applying, egui::Button::new("Cancel · Esc")).clicked() { action = Some(SpliceKey::Cancel); }
            });
            ui.weak("h/l adjusts the selected control; counts work: 12l. Tab / Shift Tab selects buttons; Enter activates.");
            if action.is_some() || !keys.is_empty() {
                ui.ctx().request_discard("Place slice controls changed before picture submission");
            }
            ui.separator();
            let height = (ui.available_height() - 114.0).max(200.0);
            let left_width = 220.0_f32.min(ui.available_width() * 0.29);
            let picture_width = (ui.available_width() - left_width - ui.spacing().item_spacing.x).max(100.0);
            let endpoints = draft.endpoint_identity();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(egui::vec2(left_width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.strong("ORIGINAL SLICE");
                    ui.label(format!("Boundaries [{}..{}) · {} source frames", draft.proposal.ordinals.start, draft.proposal.ordinals.end, draft.proposal.ordinals.end - draft.proposal.ordinals.start));
                    draft.endpoints.show(ui, &mut self.renderer, &endpoints, height);
                });
                ui.allocate_ui_with_layout(egui::vec2(picture_width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                    let viewing_endpoint = matches!(draft.focus, Focus::In | Focus::Out) && self.transport.is_none();
                    let title = if viewing_endpoint { "ORIGINAL ENDPOINT" } else if draft.before { "BEFORE · SAVED EDIT" } else if draft.prepared.is_some() { "PROPOSED · UNSAVED EDIT" } else { "DESTINATION · SAVED EDIT" };
                    ui.strong(title);
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(picture_width, (height - 54.0).max(100.0)), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 3.0, egui::Color32::BLACK);
                    let canvas = self.presentation.canvas().map_or(rect, |(width, height)| fit_rect(rect, width as f32 / height as f32));
                    self.render_picture(ui.ctx(), canvas.size());
                    if self.presentation.has_displayed() && let Some(target) = &self.target {
                        ui.painter().image(target.texture, canvas, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                    }
                    let label = self.presentation.displayed_label().unwrap_or_else(|| "Preparing picture…".into());
                    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &label));
                    ui.label(&label);
                    let in_slice = !draft.before && draft.prepared.as_ref().is_some_and(|prepared| prepared.range.start().0 <= draft.cursor as i64 && (draft.cursor as i64) < prepared.range.end().0);
                    ui.weak(if viewing_endpoint { "Original clock · endpoint inspection".into() } else { format!("Requested Edit picture {} · {}", u128::from(draft.cursor) + 1, if in_slice { "inside inserted slice" } else { "destination context" }) });
                });
            });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                let destination = format!("Destination: {}", draft.scope_label);
                ui.add_sized(egui::vec2(ui.available_width().min(320.0), 20.0), egui::Label::new(&destination).truncate()).on_hover_text(&destination);
                ui.label(format!("Edit boundary {}", draft.destination));
                if let Some(slot) = draft.slot { ui.weak(format!("Sequence slot {} of {}", slot + 1, draft.seams.len())); }
                if let Destination::Interior { target, at } = &draft.proposal.destination {
                    let label = draft.base.document.nodes().get(target).map_or("Destination beat", |node| node.label.as_str());
                    let location = format!("Inside {label} · local boundary {}", at.frames());
                    ui.add_sized(egui::vec2(ui.available_width().min(280.0), 20.0), egui::Label::new(&location).truncate()).on_hover_text(location);
                }
                if let Some(prepared) = &draft.prepared { ui.colored_label(style::LAVENDER, format!("+{} project frames", prepared.range.end().0 - prepared.range.start().0)); }
            });
            timeline(ui, &draft);
            if draft.dirty || draft.pending.is_some() { ui.weak("Preparing proposed picture and sound…"); }
            if draft.applying { ui.weak("Saving the exact proposed slice…"); }
            if let Some(error) = &draft.error { ui.colored_label(style::LAVENDER, error); }
            if let Some(error) = &self.error { ui.colored_label(style::LAVENDER, error); }
            self.splice = Some(draft);
            for action in keys.into_iter().chain(action) {
                self.splice_action(action, ui.ctx());
                ui.ctx().request_discard("Place slice action changed its controls");
            }
        });
    }
}

fn timeline(ui: &mut egui::Ui, draft: &Draft) {
    let Some(prepared) = &draft.prepared else {
        return;
    };
    let range = prepared.range;
    let duration = range.end().0 - range.start().0;
    let radius = duration.clamp(1, 120);
    let start = range.start().0.saturating_sub(radius).max(0);
    let end = range
        .end()
        .0
        .saturating_add(radius)
        .min(prepared.plan.duration().frames());
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::hover());
    let x =
        |at: i64| rect.left() + rect.width() * (at - start) as f32 / (end - start).max(1) as f32;
    for (from, to, label, color) in [
        (start, range.start().0, "Before destination", style::PANEL),
        (
            range.start().0,
            range.end().0,
            "PROVISIONAL SLICE",
            style::SELECTED,
        ),
        (range.end().0, end, "Following material", style::PANEL),
    ] {
        if from >= to {
            continue;
        }
        let span = egui::Rect::from_min_max(
            egui::pos2(x(from), rect.top()),
            egui::pos2(x(to), rect.bottom()),
        );
        ui.painter().rect_filled(span, 2.0, color);
        if span.width() > 70.0 {
            ui.painter().with_clip_rect(span).text(
                span.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(10.5),
                style::TEXT,
            );
        }
    }
    ui.painter().line_segment(
        [
            egui::pos2(x(range.start().0), rect.top()),
            egui::pos2(x(range.start().0), rect.bottom()),
        ],
        egui::Stroke::new(2.0, style::CURSOR),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Label,
            true,
            format!(
                "Provisional insertion at Edit {}, slice through exclusive {}, context {} to {}",
                range.start().0,
                range.end().0,
                start,
                end
            ),
        )
    });
}
