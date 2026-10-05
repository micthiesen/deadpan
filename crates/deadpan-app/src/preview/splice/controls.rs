use super::*;

const FOOTER_SEPARATOR: f32 = 6.0;

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
            let empty_structure = draft.empty_structure();
            let keys = std::mem::take(&mut draft.keys);
            let mut action = None;
            let heading = ui.horizontal_wrapped(|ui| {
                let title = ui.heading("Place slice");
                let operation = ui.colored_label(style::LAVENDER, if empty_structure && !draft.replacing && empty::is_forest(&draft.proposal.source) { "UNSAVED · Insert empty contents · Structure only" } else if empty_structure && !draft.replacing { "UNSAVED · Insert empty group · Structure only" } else if draft.replacing { "UNSAVED · Replace · Linked picture + sound" } else if draft.proposal.operation == Operation::Move { "UNSAVED · Move · Linked picture + sound" } else { "UNSAVED · Insert · Linked picture + sound" });
                let mut rect = title.rect.union(operation.rect);
                if let Some(count) = draft.count { rect = rect.union(ui.monospace(format!("COUNT {count}")).rect); }
                rect
            }).inner;
            // The heading teaches and focuses keyboard control. Its click
            // region must exclude the adjacent native operation button.
            let focus = ui.interact(heading, egui::Id::new(FOCUS), egui::Sense::click());
            focus.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Place slice keyboard controls"));
            if draft.focus_pending {
                focus.request_focus(); draft.focus_pending = false;
            }
            if focus.has_focus() { ui.painter().rect_stroke(heading, 2.0, egui::Stroke::new(1.0, style::LAVENDER), egui::StrokeKind::Inside); }
            let enabled = !draft.invalidated && !draft.applying;
            let source_range = draft.source_range();
            ui.horizontal_wrapped(|ui| {
                if draft.edited_source() && ui.add_enabled(enabled && (draft.proposal.operation == Operation::Move || !empty_structure), egui::Button::new(if draft.proposal.operation == Operation::Move { "Copy instead · m" } else { "Move slice · m" }).selected(draft.proposal.operation == Operation::Move)).on_disabled_hover_text(empty::PLACEMENT_REASON).clicked() { action = Some(SpliceKey::Move); }
                if draft.replacement.is_some() && ui.add_enabled(enabled && (!empty_structure || draft.replacement_object.is_some()), egui::Button::new(if draft.replacing { "Insert instead · r" } else { "Replace selection · r" }).selected(draft.replacing)).on_disabled_hover_text(empty::PLACEMENT_REASON).clicked() { action = Some(SpliceKey::Replace); }
                for (label, selected, key) in [
                    (format!("In {} · i", source_range.start), draft.focus == Focus::In, SpliceKey::In),
                    (format!("Out {} exclusive · o", source_range.end), draft.focus == Focus::Out, SpliceKey::Out),
                    (format!("Destination Edit {} · d", draft.destination), draft.focus == Focus::Destination, SpliceKey::Destination),
                    ((if draft.proposal.operation == Operation::Move { "Insertion join · f" } else { "Inspect picture · f" }).into(), draft.focus == Focus::Picture && draft.site == Site::Insertion, SpliceKey::Picture),
                ] {
                    if draft.replacing && key == SpliceKey::Destination { continue; }
                    let endpoint = matches!(key, SpliceKey::In | SpliceKey::Out);
                    if ui.add_enabled(enabled && !(empty_structure && endpoint), egui::Button::new(label).selected(selected)).on_disabled_hover_text(empty::ENDPOINT_REASON).clicked() { action = Some(key); }
                }
                if draft.proposal.operation == Operation::Move && ui.add_enabled(enabled, style::action("Removal join", "s").selected(draft.focus == Focus::Picture && draft.site == Site::Removal)).clicked() { action = Some(SpliceKey::Removal); }
                if !draft.replacing && draft.proposal.operation != Operation::Move {
                    if ui.add_enabled(enabled, style::action("Previous seam", "k")).clicked() { action = Some(SpliceKey::Boundary(false)); }
                    if ui.add_enabled(enabled, style::action("Next seam", "j")).clicked() { action = Some(SpliceKey::Boundary(true)); }
                } else if draft.replacing && let Some(range) = draft.replacement {
                    ui.label(format!("Fixed {} [{}..{})", if draft.replacement_object.is_some() { "group object" } else { "Edit" }, range.start().0, range.end().0));
                }
            });
            ui.horizontal_wrapped(|ui| {
                let frames_enabled = enabled && (!empty_structure || draft.focus == Focus::Picture && draft.frames() > 0);
                if ui.add_enabled(frames_enabled, style::action("−1", "h")).on_hover_text("Previous frame in the selected control").on_disabled_hover_text(empty::PLACEMENT_REASON).clicked() { action = Some(SpliceKey::Step(false)); }
                if ui.add_enabled(frames_enabled, style::action("+1", "l")).on_hover_text("Next frame in the selected control").on_disabled_hover_text(empty::PLACEMENT_REASON).clicked() { action = Some(SpliceKey::Step(true)); }
                if draft.proposal.operation == Operation::Move {
                    if ui.add_enabled(enabled, style::action("Prev", "k")).on_hover_text("Previous Sequence seam at the destination").clicked() { action = Some(SpliceKey::Boundary(false)); }
                    if ui.add_enabled(enabled, style::action("Next", "j")).on_hover_text("Next Sequence seam at the destination").clicked() { action = Some(SpliceKey::Boundary(true)); }
                }
                let ready = enabled && draft.prepared.is_some() && !draft.dirty && draft.pending.is_none();
                let audition_ready = ready && draft.frames() > 0 && !(empty_structure && matches!(draft.focus, Focus::In | Focus::Out));
                if ui.add_enabled(audition_ready, egui::Button::new(if draft.before { "Before · b" } else { "Proposed · b" }).selected(!draft.before)).clicked() { action = Some(SpliceKey::Compare); }
                if ui.add_enabled(audition_ready || self.transport.is_some(), egui::Button::new(if self.transport.is_some() { "Pause · Space" } else { "Audition · Space" })).on_disabled_hover_text("Empty groups contain no pictures or audio. Select a nonempty destination to audition its context.").clicked() { action = Some(SpliceKey::Play); }
                if ui.add_enabled(audition_ready, egui::Button::new(if empty_structure { "Loop destination · Shift Space" } else if draft.proposal.operation == Operation::Move { "Loop this join · Shift Space" } else { "Loop both joins · Shift Space" })).clicked() { action = Some(SpliceKey::Loop); }
                if ui.add_enabled(ready, style::action("Place slice", "Enter").fill(style::SELECTED)).clicked() { action = Some(SpliceKey::Apply); }
                if ui.add_enabled(!draft.applying, style::action("Cancel", "Esc")).clicked() { action = Some(SpliceKey::Cancel); }
            });
            ui.weak(if empty_structure { "j/k chooses an exact destination slot, including slots at the same Edit frame. f then h/l inspects destination pictures. Enter inserts the group." } else if draft.proposal.operation == Operation::Move { "h/l adjusts frames; j/k chooses destination seams. Counts work: 12l. Tab / Shift Tab selects buttons; Enter activates." } else { "h/l adjusts the selected control; counts work: 12l. Tab / Shift Tab selects buttons; Enter activates." });
            if action.is_some() || !keys.is_empty() {
                ui.ctx().request_discard("Place slice controls changed before picture submission");
            }
            ui.separator();
            let footer = Footer::measure(ui, &draft, self.error.as_deref(), self.audition_context);
            let height = (ui.available_height() - footer.height - FOOTER_SEPARATOR - ui.spacing().item_spacing.y).max(120.0);
            let left_width = 220.0_f32.min(ui.available_width() * 0.29);
            let picture_width = (ui.available_width() - left_width - ui.spacing().item_spacing.x).max(100.0);
            let endpoints = draft.endpoint_identity();
            let viewing_endpoint = !empty_structure && matches!(draft.focus, Focus::In | Focus::Out) && self.transport.is_none();
            let empty_destination = draft.frames() == 0;
            let title = if empty_structure { "DESTINATION · SAVED EDIT" } else if viewing_endpoint { if draft.edited_source() { "COPIED EDIT ENDPOINT" } else { "ORIGINAL ENDPOINT" } } else if draft.before { "BEFORE · SAVED EDIT" } else if draft.prepared.is_some() { "PROPOSED · UNSAVED EDIT" } else { "DESTINATION · SAVED EDIT" };
            let label = if empty_destination { "Empty edit · no pictures or audio".into() } else { self.presentation.displayed_label().unwrap_or_else(|| "Preparing picture…".into()) };
            let in_slice = !draft.before && draft.prepared.as_ref().is_some_and(|prepared| prepared.range.start().0 <= draft.cursor as i64 && (draft.cursor as i64) < prepared.range.end().0);
            let requested = if empty_destination { "Inserting this group adds structure without picture or audio time.".into() } else if empty_structure && matches!(draft.focus, Focus::In | Focus::Out) { "Empty source group · destination picture retained".into() } else if viewing_endpoint { if draft.edited_source() { "Copied Edit clock · endpoint inspection".into() } else { "Original clock · endpoint inspection".into() } } else if draft.cursor == draft.frames() { format!("Requested terminal Edit boundary {} · displaying the final picture", draft.cursor) } else { format!("Requested Edit picture {} · {}", u128::from(draft.cursor) + 1, if in_slice { "inside inserted slice" } else if draft.site == Site::Removal { "removal context" } else { "destination context" }) };
            let picture_text = [egui::RichText::new(title).strong(), egui::RichText::new(&label), egui::RichText::new(requested).weak()].map(|text| {
                egui::WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Wrap), picture_width, egui::TextStyle::Body)
            });
            let picture_height = (height - picture_text.iter().map(|text| text.size().y).sum::<f32>() - 3.0 * ui.spacing().item_spacing.y).max(1.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(egui::vec2(left_width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                    if empty_structure {
                        egui::ScrollArea::vertical().id_salt("empty-slice-source").max_height(height).show(ui, |ui| empty::source_card(ui, &draft));
                    } else {
                        ui.strong(if draft.edited_source() { "COPIED EDIT SLICE" } else { "ORIGINAL SLICE" });
                        ui.label(format!("Boundaries [{}..{}) · {} {} frames", source_range.start, source_range.end, source_range.end - source_range.start, if draft.edited_source() { "Edit" } else { "source" }));
                        draft.endpoints.show(ui, &mut self.renderer, &endpoints, height);
                    }
                });
                ui.allocate_ui_with_layout(egui::vec2(picture_width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.add(egui::Label::new(picture_text[0].clone()));
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(picture_width, picture_height), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 3.0, egui::Color32::BLACK);
                    let canvas = self.presentation.canvas().map_or(rect, |(width, height)| fit_rect(rect, width as f32 / height as f32));
                    if !empty_destination { self.render_picture(ui.ctx(), canvas.size()); }
                    if !empty_destination && self.presentation.has_displayed() && let Some(target) = &self.target {
                        ui.painter().image(target.texture, canvas, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                    }
                    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &label));
                    ui.add(egui::Label::new(picture_text[1].clone()));
                    ui.add(egui::Label::new(picture_text[2].clone()));
                });
            });
            ui.add(egui::Separator::default().spacing(FOOTER_SEPARATOR));
            for line in footer.lines { ui.add(egui::Label::new(line)); }
            timeline(ui, &draft);
            self.splice = Some(draft);
            for action in keys.into_iter().chain(action) {
                self.splice_action(action, ui.ctx());
                ui.ctx().request_discard("Place slice action changed its controls");
            }
        });
    }
}

struct Footer {
    lines: Vec<Arc<egui::Galley>>,
    height: f32,
}

impl Footer {
    fn measure(
        ui: &egui::Ui,
        draft: &Draft,
        error: Option<&str>,
        context: playback::AuditionContext,
    ) -> Self {
        let destination = if draft.replacing
            && let (Some(selection), Some(range)) = (&draft.replacement_object, draft.replacement)
        {
            let path = if selection.group == draft.proposal.parent {
                draft.scope_label.clone()
            } else {
                let label = draft
                    .base
                    .document
                    .nodes()
                    .get(&selection.group)
                    .map_or("Group", |node| node.label.as_str());
                format!("{} / {label}", draft.scope_label)
            };
            let kind = match selection.kind {
                deadpan_core::SemanticTextObject::InnerGroup => "group contents",
                deadpan_core::SemanticTextObject::AroundGroup => "whole group",
            };
            format!(
                "Replace {kind}: {path} · Edit [{}..{})",
                range.start().0,
                range.end().0
            )
        } else {
            format!(
                "Destination: {} · Edit boundary {}",
                draft.scope_label, draft.destination
            )
        };
        let mut text = vec![(destination, style::TEXT)];
        if let Destination::Interior { target, at } = &draft.proposal.destination {
            let label = draft
                .base
                .document
                .nodes()
                .get(target)
                .map_or("Destination beat", |node| node.label.as_str());
            text[0].0.push_str(&format!(
                " · Inside {label} · local boundary {}",
                at.frames()
            ));
        } else if !draft.replacing
            && let Some(slot) = draft.slot
        {
            text[0].0.push_str(&format!(
                " · Sequence slot {} of {}",
                slot + 1,
                draft.seams.len()
            ));
            if draft.empty_structure() {
                let label = |index: usize| {
                    draft
                        .children
                        .get(index)
                        .and_then(|node| draft.base.document.nodes().get(node))
                        .map(|node| node.label.as_str())
                };
                if let Some(previous) = slot.checked_sub(1).and_then(label) {
                    text[0].0.push_str(&format!(" · after ‘{previous}’"));
                }
                if let Some(next) = label(slot) {
                    text[0].0.push_str(&format!(" · before ‘{next}’"));
                }
            }
        }
        if let Some(prepared) = &draft.prepared {
            let inserted = prepared.range.end().0 - prepared.range.start().0;
            if let Some(movement) = &prepared.movement {
                let source = match &draft.proposal.source {
                    Source::Edited { copied, .. } => std::iter::once("Your edit")
                        .chain(copied.scope().groups().iter().map(|id| {
                            draft
                                .base
                                .document
                                .nodes()
                                .get(id)
                                .map_or("Source group", |node| node.label.as_str())
                        }))
                        .collect::<Vec<_>>()
                        .join(" / "),
                    Source::Original { .. } => "Original".into(),
                };
                text.push((
                    format!(
                        "From {source} [{}..{}) · To {} [{}..{}) · duration unchanged",
                        movement.source_before.start().0,
                        movement.source_before.end().0,
                        draft.scope_label,
                        prepared.range.start().0,
                        prepared.range.end().0
                    ),
                    style::LAVENDER,
                ));
                if let Some(comparison) = draft.comparison() {
                    if comparison.timing_unchanged {
                        text.push((
                            "Timing unchanged · compare framing and sound at the same Edit frames"
                                .into(),
                            style::muted(ui),
                        ));
                    } else if comparison
                        .windows(
                            draft.base.document.presentation_basis().frame_rate,
                            context.lead.0,
                            context.follow.0,
                        )
                        .is_ok_and(|(_, shortened)| shortened)
                    {
                        text.push(("Context ends at other move join".into(), style::muted(ui)));
                    }
                }
            } else if let Some(removed) = prepared.removed {
                text.push((
                    format!(
                        "Remove [{}..{}) · Insert [{}..{}) · {:+} f",
                        removed.start().0,
                        removed.end().0,
                        prepared.range.start().0,
                        prepared.range.end().0,
                        inserted - (removed.end().0 - removed.start().0)
                    ),
                    style::LAVENDER,
                ));
            } else if prepared.empty_slot.is_some() {
                text[0]
                    .0
                    .push_str(if empty::is_forest(&draft.proposal.source) {
                        " · empty contents · duration unchanged"
                    } else {
                        " · empty group · duration unchanged"
                    });
            } else {
                text[0]
                    .0
                    .push_str(&format!(" · +{inserted} project frames"));
            }
        }
        if draft.dirty || draft.pending.is_some() {
            text.push((
                if draft.empty_structure() {
                    "Preparing structural placement…"
                } else {
                    "Preparing proposed picture and sound…"
                }
                .into(),
                style::muted(ui),
            ));
        }
        if draft.applying {
            text.push(("Saving the exact proposed slice…".into(), style::muted(ui)));
        }
        for note in [
            draft.comparison_note.as_deref(),
            draft.error.as_deref(),
            error,
        ]
        .into_iter()
        .flatten()
        {
            text.push((note.into(), style::LAVENDER));
        }
        // Measure all variable text before allocating the viewer, and draw the
        // same galleys. New notices and narrower windows cannot consume an
        // unmeasured extra line after picture submission.
        let lines: Vec<_> = text
            .into_iter()
            .map(|(text, color)| {
                egui::WidgetText::from(egui::RichText::new(text).color(color)).into_galley(
                    ui,
                    Some(egui::TextWrapMode::Wrap),
                    ui.available_width(),
                    egui::TextStyle::Body,
                )
            })
            .collect();
        let spacing = ui.spacing().item_spacing.y;
        let timeline = draft.prepared.as_ref().map_or(0.0, |prepared| {
            if prepared.empty_slot.is_some() {
                0.0
            } else if prepared.movement.is_some() {
                88.0 + 2.0 * spacing
            } else if prepared.removed.is_some() {
                58.0 + 2.0 * spacing
            } else {
                40.0 + spacing
            }
        });
        let height = lines
            .iter()
            .map(|line| line.size().y + spacing)
            .sum::<f32>()
            + timeline;
        Self { lines, height }
    }
}

fn timeline(ui: &mut egui::Ui, draft: &Draft) {
    let Some(prepared) = &draft.prepared else {
        return;
    };
    if prepared.empty_slot.is_some() {
        return;
    }
    if prepared.movement.is_some() {
        for site in [Site::Removal, Site::Insertion] {
            move_timeline(ui, draft, site);
        }
        return;
    }
    let range = prepared.range;
    let longest_end = prepared
        .removed
        .map_or(range.end().0, |removed| removed.end().0.max(range.end().0));
    let duration = longest_end - range.start().0;
    let radius = duration.clamp(1, 120);
    let start = range.start().0.saturating_sub(radius).max(0);
    let end = longest_end.saturating_add(radius).min(
        prepared
            .plan
            .duration()
            .frames()
            .max(draft.base.plan.duration().frames()),
    );
    // Both rows use the same project-frame scale, so unequal durations are
    // visible geometrically as well as in their exact numeric labels.
    if let Some(removed) = prepared.removed {
        timeline_interval(
            ui,
            removed,
            draft.base.plan.duration().frames(),
            (start, end),
            true,
        );
    }
    timeline_interval(
        ui,
        range,
        prepared.plan.duration().frames(),
        (start, end),
        false,
    );
}

fn move_timeline(ui: &mut egui::Ui, draft: &Draft, site: Site) {
    let prepared = draft.prepared.as_ref().expect("prepared move");
    let comparison = Comparison::new(
        (prepared.range.start().0, prepared.range.end().0),
        None,
        prepared.movement.as_ref(),
        site,
        draft.base.plan.duration().frames(),
        prepared.plan.duration().frames(),
    );
    let saved = comparison.affected.saved;
    let proposed = comparison.affected.proposed;
    let name = match site {
        Site::Removal => "Removal · s",
        Site::Insertion => "Insertion · f",
    };
    let label = format!(
        "{name} · Saved [{}..{}) to Proposed [{}..{}){}",
        saved.0,
        saved.1,
        proposed.0,
        proposed.1,
        if comparison.timing_unchanged {
            " · same timing"
        } else {
            ""
        }
    );
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 3.0, style::PANEL);
    if draft.site == site {
        ui.painter().rect_stroke(
            rect,
            3.0,
            egui::Stroke::new(1.0, style::LAVENDER),
            egui::StrokeKind::Inside,
        );
    }
    ui.painter().with_clip_rect(rect).text(
        rect.left_top() + egui::vec2(6.0, 3.0),
        egui::Align2::LEFT_TOP,
        &label,
        egui::FontId::proportional(11.0),
        style::TEXT,
    );
    // Align the before/after intervals at their local joins, not at distant
    // global positions. The exact clocks remain in each row's label.
    let longest = (saved.1 - saved.0).max(proposed.1 - proposed.0);
    let radius = longest.clamp(1, 120);
    let contexts = comparison.frame_context(radius);
    let width = (rect.width() - 16.0).max(1.0);
    let x = |offset: i64| {
        rect.left() + 8.0 + width * offset as f32 / longest.saturating_add(2 * radius) as f32
    };
    for (index, range) in [saved, proposed].into_iter().enumerate() {
        let y = rect.top() + 23.0 + index as f32 * 9.0;
        let context = contexts.side(index == 0);
        ui.painter().line_segment(
            [
                egui::pos2(x((context.0 - range.0).saturating_add(radius)), y),
                egui::pos2(x((context.1 - range.0).saturating_add(radius)), y),
            ],
            egui::Stroke::new(3.0, style::MUTED.gamma_multiply(0.25)),
        );
        let selected = egui::Rect::from_min_max(
            egui::pos2(x(radius), y - 2.5),
            egui::pos2(
                x((range.1 - range.0).saturating_add(radius)).max(x(radius) + 2.0),
                y + 2.5,
            ),
        );
        ui.painter().rect_filled(
            selected,
            1.0,
            if index == 0 {
                style::LAVENDER
            } else {
                style::CURSOR
            },
        );
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &label));
}

fn timeline_interval(
    ui: &mut egui::Ui,
    range: FrameRange,
    total: i64,
    context: (i64, i64),
    removed: bool,
) {
    let (start, end) = context;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), if removed { 18.0 } else { 40.0 }),
        egui::Sense::hover(),
    );
    let x =
        |at: i64| rect.left() + rect.width() * (at - start) as f32 / (end - start).max(1) as f32;
    for (from, to, label, color) in [
        (start, range.start().0, "Before destination", style::PANEL),
        (
            range.start().0,
            range.end().0,
            if removed {
                "REMOVED FROM SAVED EDIT"
            } else {
                "PROVISIONAL SLICE"
            },
            if removed {
                style::LAVENDER.gamma_multiply(0.3)
            } else {
                style::SELECTED
            },
        ),
        (
            range.end().0,
            end.min(total),
            "Following material",
            style::PANEL,
        ),
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
                "{} at Edit {}, through exclusive {}, context {} to {}",
                if removed {
                    "Removed selection"
                } else {
                    "Provisional insertion"
                },
                range.start().0,
                range.end().0,
                start,
                end
            ),
        )
    });
}
