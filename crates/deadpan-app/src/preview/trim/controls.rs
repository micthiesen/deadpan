//! Native amount editing, ordered background keys and a picture-first Trim panel.

#[cfg(test)]
mod tests;

use deadpan_core::{
    ExactRatio, SourceTrimGeometryConstraint, SourceTrimGeometryLimit, SourceTrimGeometryOwner,
    SourceTrimPolicy,
};

use super::*;

const FEEDBACK_FOCUS: &str = "trim-feedback-focus";
const FEEDBACK_LABEL: &str =
    "Trim feedback scroll viewport. Up/Down scroll; Page Up/Down page; Home/End ends.";

fn control_label(control: SourceTrimControl) -> &'static str {
    match control {
        SourceTrimControl::In => "In",
        SourceTrimControl::Out => "Out",
        SourceTrimControl::Slip => "Slip",
        SourceTrimControl::Roll => "Roll",
    }
}

fn ratio(value: ExactRatio) -> String {
    if value.denominator() == 1 {
        value.numerator().to_string()
    } else {
        format!("{}/{}", value.numerator(), value.denominator())
    }
}

fn limit_label(limit: SourceTrimGeometryLimit) -> String {
    let owner = match limit.owner {
        SourceTrimGeometryOwner::Target => "selected beat",
        SourceTrimGeometryOwner::Right => "right beat",
        SourceTrimGeometryOwner::Scope => "Sequence",
        SourceTrimGeometryOwner::Project => "project",
        SourceTrimGeometryOwner::Intent => "frame value",
    };
    let reason = match limit.constraint {
        SourceTrimGeometryConstraint::PictureStart => "picture start",
        SourceTrimGeometryConstraint::PictureEnd => "picture end",
        SourceTrimGeometryConstraint::MinimumSelectedDuration => "minimum selected duration",
        SourceTrimGeometryConstraint::MinimumOutputDuration => "minimum output duration",
        SourceTrimGeometryConstraint::PhysicalDuration => "physical duration",
        SourceTrimGeometryConstraint::ScopeStart => "Sequence start",
        SourceTrimGeometryConstraint::ScopeEnd => "Sequence end",
        SourceTrimGeometryConstraint::ProjectDuration => "project duration",
        SourceTrimGeometryConstraint::IntegerValue => "integer range",
    };
    format!(
        "{}f {} · {owner}: {reason}",
        ratio(limit.value),
        if limit.inclusive {
            "inclusive"
        } else {
            "exclusive"
        }
    )
}

impl Draft {
    fn status(&self) -> Vec<String> {
        let mut lines = vec![];
        if self.applying.is_some() {
            lines.push("Saving the displayed Trim…".into());
        }
        for error in [
            &self.input.text_error,
            &self.input.input_error,
            &self.input.error,
            &self.error,
        ]
        .into_iter()
        .flatten()
        {
            lines.push(error.clone());
        }
        if self.input.waiting() > 0 {
            lines.push(format!(
                "{} input events awaiting acknowledgment. Values below are accepted values.",
                self.input.waiting()
            ));
        }
        if let Some(prepared) = self.input.ready() {
            let geometry = &prepared.resolution.geometry;
            lines.push(format!(
                "Project duration {:+}f · {} silent filler{} · entry Edit cursor {}{}",
                geometry.duration_delta_frames,
                prepared.resolution.fillers.len(),
                if prepared.resolution.fillers.len() == 1 {
                    ""
                } else {
                    "s"
                },
                self.capture.target.cursor.0,
                if prepared.cursor_clamped {
                    format!(" → new end {} on Apply", prepared.cursor_after.0)
                } else {
                    " retained".into()
                }
            ));
            if prepared.accepted.is_zero() {
                lines.push("All four values are zero. No transaction will be saved.".into());
            }
            if let deadpan_core::SourceTrimRollAvailability::Unavailable { error, .. } =
                &geometry.roll_availability
            {
                lines.push(format!("Roll unavailable: {error}"));
            }
        }
        if let Some(inspection) = &self.inspection {
            lines.push(format!(
                "Cut at Edit frame {} · sample {} · incoming: {} · {}",
                inspection.identity.boundary.0,
                inspection.boundary_sample.0,
                inspection.incoming_label,
                inspection.right_label
            ));
        }
        for outcome in &self.input.feedback {
            if let Some(error) = &outcome.error {
                lines.push(format!("Input refused: {error}"));
            }
            if let Some(adjustment) = &outcome.adjustment {
                lines.push(format!("{} requested {:+}f ({:+}f step from {:+}f), accepted {:+}f · executable {:+}f to {:+}f",
                    control_label(adjustment.control), adjustment.requested_value, adjustment.requested_step,
                    adjustment.previous_value, adjustment.applied_value, adjustment.minimum_value, adjustment.maximum_value));
                lines.push(format!(
                    "Exact handles: {} to {}",
                    limit_label(adjustment.minimum),
                    limit_label(adjustment.maximum)
                ));
                if let Some(clamp) = adjustment.clamp {
                    lines.push(format!("Handle reached: {}", limit_label(clamp)));
                }
            }
        }
        lines
    }
}

impl DeadpanApp {
    pub(in crate::preview) fn trim_keyboard(&mut self, context: &egui::Context) {
        if context.current_pass_index() != 0 {
            return;
        }
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
                            key: egui::Key::Enter | egui::Key::Escape,
                            ..
                        }
                    )
                })
            });
            return;
        }
        if self.dialogs.is_open() || pointer_focus_transition(&events) {
            return;
        }
        let field = context.memory(|memory| memory.has_focus(egui::Id::new(AMOUNT)));
        let background = context.memory(|memory| {
            memory
                .focused()
                .is_none_or(|id| id == egui::Id::new(FOCUS) || id == pane_id(self.pane))
        });
        let Some(draft) = &mut self.trim else {
            return;
        };
        let batch = context.input_mut(|input| route_events(&mut input.events, field, background));
        draft.keys.extend(batch.keys);
        draft.accept_amount |= batch.accept_amount;
        draft.amount_events = batch.amount_events;
    }

    pub(in crate::preview) fn trim_workspace(&mut self, ui: &mut egui::Ui) {
        let keys = self
            .trim
            .as_mut()
            .map(|draft| std::mem::take(&mut draft.keys))
            .unwrap_or_default();
        for key in keys {
            self.trim_action(key, ui.ctx());
        }
        egui::CentralPanel::default().frame(style::panel()).show(ui, |ui| {
            let Some(mut draft) = self.trim.take() else { return; };
            let mut actions = Vec::new();
            let mut inspect = None;
            let mut media_changed = false;
            let mut retry = false;
            let enabled = draft.applying.is_none() && !draft.input.invalidated;
            let heading = ui.horizontal_wrapped(|ui| {
                let title = ui.heading("Trim");
                let state = ui.colored_label(style::CURSOR, "UNSAVED · Edit junction");
                title.rect.union(state.rect)
            }).inner;
            let focus = ui.interact(heading, egui::Id::new(FOCUS), egui::Sense::click());
            focus.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, "Trim keyboard controls. Tab cycles In, Out, Slip and Roll; Shift-Tab reverses. H and L adjust one project frame, Shift adjusts ten. E focuses the amount field."));
            if focus.clicked() || draft.focus_pending { focus.request_focus(); draft.focus_pending = false; }
            lock_heading(ui);
            if focus.has_focus() { ui.painter().rect_stroke(heading, 2.0, egui::Stroke::new(1.0, style::CURSOR), egui::StrokeKind::Inside); }
            ui.weak(format!("{} · {} · Entry Edit [{}..{}) · Original cursor {} retained", draft.scope_label, draft.label, draft.capture.target.range.start().0, draft.capture.target.range.end().0, u128::from(draft.capture.source_cursor) + 1));
            ui.horizontal_wrapped(|ui| {
                for control in [SourceTrimControl::In, SourceTrimControl::Out, SourceTrimControl::Slip, SourceTrimControl::Roll] {
                    if ui.add_enabled(enabled, egui::Button::new(format!("{} {:+}f", control_label(control), amount(draft.input.accepted, control))).selected(draft.control == control)).clicked() { inspect = Some(control); }
                }
                ui.weak("Tab / Shift-Tab · cycle control");
                let policy = match draft.input.accepted.policy { SourceTrimPolicy::Ripple => "Ripple · r", SourceTrimPolicy::Overwrite => "Overwrite · r" };
                if ui.add_enabled(enabled, egui::Button::new(policy)).on_hover_text("R toggles policy for all four values, preserving them or explaining why the change is refused. In and Out change duration under Ripple; Overwrite preserves it.").clicked() { actions.push(TrimKey::TogglePolicy); }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{} amount · e", control_label(draft.amount_control)));
                let amount_events = draft.amount_events.take();
                let field = with_amount_events(ui, amount_events, |ui| {
                    ui.add_enabled(enabled, egui::TextEdit::singleline(&mut draft.amount).id(egui::Id::new(AMOUNT)).desired_width(100.0))
                });
                field.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, enabled, format!("{} amount in whole project frames, for example -3f or +5f. Enter accepts the text without applying Trim. Tab uses native focus navigation.", control_label(draft.amount_control))));
                field.clone().on_hover_text("Enter a whole-frame amount such as -3f, +5f or 7f. Enter accepts the text and returns to Trim controls; it does not apply the edit. Tab moves native focus. Escape cancels Trim outside IME composition.");
                if field.changed() {
                    draft.amount_dirty = true;
                    match navigation::trim::parse_frames(&draft.amount) {
                        Ok(frames) => { draft.input.accept_text(Event::SetAmount { control: draft.amount_control, frames }); }
                        Err(_) => draft.input.set_text_error(format!("{} amount must be a whole number of frames, for example -3f or +5f.", control_label(draft.amount_control))),
                    }
                    media_changed = true;
                }
                if std::mem::take(&mut draft.accept_amount) {
                    if draft.finish_amount_entry() {
                        context_focus(ui.ctx(), FOCUS);
                    } else { field.request_focus(); }
                }
                if ui.add_enabled(enabled, egui::Button::new("−1f · h")).clicked() { actions.push(TrimKey::Nudge(-1)); }
                if ui.add_enabled(enabled, egui::Button::new("+1f · l")).clicked() { actions.push(TrimKey::Nudge(1)); }
                ui.weak("Shift · 10f");
                if ui.add_enabled(enabled, egui::Button::new(if draft.side == JunctionSide::Before { "Before · b" } else { "Proposed · b" }).selected(draft.side == JunctionSide::Proposed)).clicked() { actions.push(TrimKey::Compare); }
                if ui.add_enabled(enabled, egui::Button::new("In junction · i")).on_hover_text("Select In. While Slip is active, inspect its In junction and keep Slip active.").clicked() { actions.push(TrimKey::In); }
                if ui.add_enabled(enabled, egui::Button::new("Out junction · o")).on_hover_text("Select Out. While Slip is active, inspect its Out junction and keep Slip active.").clicked() { actions.push(TrimKey::Out); }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(enabled && draft.inspection.is_some(), egui::Button::new(if self.transport.is_some() { "Pause · Space" } else { "Audition · Space" })).on_hover_text("Audition the inspected Before or Proposed junction. Space pauses and resumes at the heard position. The picture pair and editor cursors stay fixed.").clicked() { actions.push(TrimKey::Play); }
                if ui.add_enabled(enabled && draft.inspection.is_some(), egui::Button::new("Loop · Shift-Space")).on_hover_text("Restart a loop from the beginning of this junction's context. Space pauses or resumes it; changing the inspected junction or draft resets the loop.").clicked() { actions.push(TrimKey::Loop); }
                ui.weak(format!("Context {} before / {} after", self.audition_context.lead_label(), self.audition_context.follow_label()));
                if ui.add_enabled(draft.can_apply(&self.junction_pictures) && !self.service.is_busy(), egui::Button::new("Apply · Enter").fill(style::SELECTED)).on_disabled_hover_text("Apply needs all input acknowledged and the current nonzero Proposed picture pair displayed at this window size.").clicked() { actions.push(TrimKey::Apply); }
                if ui.add_enabled(draft.applying.is_none(), egui::Button::new("Cancel · Esc")).clicked() { actions.push(TrimKey::Cancel); }
                if enabled && draft.input.error.is_some() && ui.button("Retry proposal").clicked() { retry = true; }
            });
            if media_changed || inspect.is_some() || actions.iter().any(|action| !matches!(action, TrimKey::Apply | TrimKey::Play | TrimKey::Loop | TrimKey::FocusAmount)) || retry { ui.ctx().request_discard("Trim input precedes pair submission"); }
            // Allocate the remaining body once. Fixed child rectangles keep
            // async text from resizing a submitted pair or escaping the viewport.
            let (body, _) = ui.allocate_exact_size(ui.available_size().max(egui::Vec2::ZERO), egui::Sense::hover());
            let layout = body_layout(body, ui.spacing().item_spacing.y, waveform::preferred_height(ui),
                3.0 * ui.text_style_height(&egui::TextStyle::Body) + 2.0 * ui.spacing().item_spacing.y);
            let mut pictures_ui = region_ui(ui, "trim-junction-region", layout.pictures);
            let mut waveform_ui = region_ui(ui, "trim-waveform-status", layout.waveform);
            let waveform_retry = draft.waveform.show(&mut waveform_ui, draft.position, layout.waveform.height());
            let errors: Vec<String> = [&self.error, &self.project_error].into_iter().flatten().cloned().collect();
            let mut status = errors.clone();
            for notice in draft.status().into_iter().chain(self.message.iter().cloned()) {
                if !status.contains(&notice) { status.push(notice); }
            }
            let mut feedback_ui = region_ui(ui, "trim-feedback-region", layout.feedback);
            let response = feedback_ui.interact(layout.feedback, egui::Id::new(FEEDBACK_FOCUS), egui::Sense::click());
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, FEEDBACK_LABEL));
            if response.clicked() { response.request_focus(); }
            if response.has_focus() {
                if !feedback_ui.memory(|memory| memory.had_focus_last_frame(egui::Id::new(FEEDBACK_FOCUS))) {
                    // egui cannot install a filter on the pass that gains focus.
                    // Initialize it before the next outer frame can route arrows.
                    feedback_ui.ctx().request_discard("initialize Trim feedback focus filter");
                }
                feedback_ui.painter().rect_stroke(layout.feedback, 2.0, egui::Stroke::new(1.0, style::CURSOR), egui::StrokeKind::Inside);
                feedback_ui.memory_mut(|memory| memory.set_focus_lock_filter(egui::Id::new(FEEDBACK_FOCUS), egui::EventFilter {
                    vertical_arrows: true, ..Default::default()
                }));
            }
            let hint_height = (feedback_ui.text_style_height(&egui::TextStyle::Small) + 4.0).min(layout.feedback.height());
            feedback_ui.painter().text(layout.feedback.left_top(), egui::Align2::LEFT_TOP,
                "Feedback · Up/Down scroll · PgUp/PgDn · Home/End", egui::TextStyle::Small.resolve(feedback_ui.style()), style::MUTED);
            let scroll_rect = egui::Rect::from_min_max(layout.feedback.min + egui::vec2(0.0, hint_height), layout.feedback.max);
            let mut scroll_ui = region_ui(&mut feedback_ui, "feedback-lines", scroll_rect);
            let scroll_id = scroll_ui.make_persistent_id("trim-feedback");
            let error_id = scroll_id.with("reported-errors");
            let reveal_id = scroll_id.with("revealing-error");
            let frame = scroll_ui.ctx().cumulative_frame_nr();
            let native_scrolling = scroll_ui.input(|input| input.is_scrolling()
                || input.pointer.any_down() || input.pointer.any_released());
            let (new_error, mut reveal_error) = scroll_ui.ctx().data_mut(|data| {
                let changed = !errors.is_empty() && data.get_temp::<Vec<String>>(error_id).as_ref() != Some(&errors);
                let (pending, previous_frame) = data.get_temp::<(bool, u64)>(reveal_id).unwrap_or_default();
                let reveal = !errors.is_empty() && (changed || (pending && (previous_frame == frame || native_scrolling)));
                data.insert_temp(error_id, errors);
                data.insert_temp(reveal_id, (reveal, frame));
                (changed, reveal)
            });
            // Retire this area's old kinetic velocity and animated target too.
            if new_error { egui::scroll_area::State::default().store(scroll_ui.ctx(), scroll_id); }
            let content_id = scroll_id.with("measured-content-height");
            let content_height = scroll_ui.ctx().data(|data| data.get_temp::<f32>(content_id)).unwrap_or(0.0);
            let mut area = egui::ScrollArea::vertical().id_salt("trim-feedback").auto_shrink([false, false])
                .min_scrolled_height(0.0).max_height(scroll_rect.height())
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);
            if response.has_focus() && scroll_ui.ctx().current_pass_index() == 0 && !self.ime_composing
                && !self.dialogs.is_open() && !egui::Popup::is_any_open(scroll_ui.ctx()) && !scroll_ui.ctx().any_popup_open()
                && !scroll_ui.input(|input| input.events.iter().any(|event| matches!(event, egui::Event::Ime(_)))) {
                let offset = egui::scroll_area::State::load(scroll_ui.ctx(), scroll_id).map_or(0.0, |state| state.offset.y);
                if let Some(offset) = scroll_ui.input_mut(|input| feedback_scroll_events(&mut input.events, offset, scroll_rect.height(), content_height)) {
                    area = area.vertical_scroll_offset(offset);
                    // A later deliberate key may end the reveal gesture guard.
                    if !new_error {
                        reveal_error = false;
                        scroll_ui.ctx().data_mut(|data| data.insert_temp(reveal_id, (false, frame)));
                    }
                }
            }
            // Keep the fault visible through layout retries and the active
            // native gesture, including egui's remaining wheel smoothing.
            if reveal_error { area = area.vertical_scroll_offset(0.0).scroll_source(egui::scroll_area::ScrollSource::NONE); }
            let output = area.show(&mut scroll_ui, |ui| {
                    for line in status { ui.label(line); }
                });
            if reveal_error {
                // Small wheel impulses can stop repainting before egui's
                // 150 ms scroll-idle timeout. Schedule a settling pass so the
                // reveal guard retires before later native input.
                if scroll_ui.input(|input| input.is_scrolling()) {
                    scroll_ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                }
                // An already-held scrollbar can retain its interaction ID even
                // with ScrollSource::NONE and write an offset after text paint.
                let mut state = output.state;
                state.offset = egui::Vec2::ZERO;
                state.store(scroll_ui.ctx(), output.id);
            }
            scroll_ui.ctx().data_mut(|data| data.insert_temp(content_id, output.content_size.y));
            // Every focus/layout retry must be known before picture submission.
            // The fixed, disjoint region keeps this paint order from moving or
            // covering either the waveform or feedback.
            if let Some(inspection) = &draft.inspection {
                self.junction_pictures.show(&mut pictures_ui, &mut self.renderer, &inspection.identity, layout.pictures.height());
            } else {
                self.junction_pictures.show_retained(&mut pictures_ui, layout.pictures.height());
            }
            if let Some(control) = inspect {
                draft.control = control;
                draft.slip_edge = SourceTrimEdge::In;
                draft.sync_amount();
                media_changed = true;
            }
            if retry { draft.input.retry(); media_changed = true; }
            if waveform_retry && self.transport.is_none() && let Some(inspection) = &draft.inspection {
                draft.waveform.request(&self.playback, inspection.identity.clone(), &inspection.input, inspection.samples.clone());
            }
            self.trim = Some(draft);
            if media_changed { self.invalidate_trim_media(false); }
            for action in actions { self.trim_action(action, ui.ctx()); }
        });
    }
}

fn feedback_scroll_events(
    events: &mut Vec<egui::Event>,
    offset: f32,
    viewport: f32,
    content: f32,
) -> Option<f32> {
    let maximum = (content - viewport).max(0.0);
    let mut requested = None;
    events.retain(|event| {
        let egui::Event::Key {
            key,
            modifiers,
            pressed: true,
            ..
        } = event
        else {
            return true;
        };
        if !modifiers.is_none() {
            return true;
        }
        let current = requested.unwrap_or(offset);
        let next = match key {
            egui::Key::ArrowDown => current + 28.0,
            egui::Key::ArrowUp => current - 28.0,
            egui::Key::PageDown => current + viewport * 0.9,
            egui::Key::PageUp => current - viewport * 0.9,
            egui::Key::Home => 0.0,
            egui::Key::End => maximum,
            _ => return true,
        };
        requested = Some(next.clamp(0.0, maximum));
        false
    });
    requested
}

struct BodyLayout {
    pictures: egui::Rect,
    waveform: egui::Rect,
    feedback: egui::Rect,
}

fn body_layout(rect: egui::Rect, gap: f32, waveform: f32, feedback: f32) -> BodyLayout {
    let gap = gap.max(0.0).min(rect.height() / 2.0);
    let available = (rect.height() - 2.0 * gap).max(0.0);
    let waveform = waveform.max(0.0).min(available * 0.35);
    let feedback = feedback.max(0.0).min(available * 0.18);
    let pictures = available - waveform - feedback;
    let region = |top, height| {
        egui::Rect::from_min_size(
            egui::pos2(rect.left(), top),
            egui::vec2(rect.width(), height),
        )
    };
    BodyLayout {
        pictures: region(rect.top(), pictures),
        waveform: region(rect.top() + pictures + gap, waveform),
        feedback: region(rect.bottom() - feedback, feedback),
    }
}

fn region_ui(ui: &mut egui::Ui, id: &str, rect: egui::Rect) -> egui::Ui {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    fn key(key: egui::Key, modifiers: egui::Modifiers, repeat: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers,
        }
    }

    #[test]
    fn focused_feedback_scrolls_from_the_actual_offset_and_preserves_other_keys() {
        let mut events = vec![
            key(egui::Key::End, egui::Modifiers::NONE, false),
            key(egui::Key::ArrowUp, egui::Modifiers::NONE, true),
            key(egui::Key::Tab, egui::Modifiers::NONE, false),
            key(egui::Key::Escape, egui::Modifiers::NONE, false),
            key(egui::Key::PageDown, egui::Modifiers::COMMAND, false),
        ];
        assert_eq!(
            feedback_scroll_events(&mut events, 19.0, 40.0, 120.0),
            Some(52.0)
        );
        assert_eq!(events.len(), 3);
        let mut page = vec![key(egui::Key::PageUp, egui::Modifiers::NONE, false)];
        assert_eq!(
            feedback_scroll_events(&mut page, 19.0, 40.0, 120.0),
            Some(0.0)
        );
        assert!(page.is_empty());
    }

    #[test]
    fn feedback_keeps_native_modifier_and_activation_ownership() {
        let mut events = vec![
            key(egui::Key::Home, egui::Modifiers::SHIFT, false),
            key(egui::Key::End, egui::Modifiers::CTRL, false),
            key(egui::Key::ArrowDown, egui::Modifiers::ALT, true),
            key(egui::Key::Enter, egui::Modifiers::NONE, false),
            key(egui::Key::H, egui::Modifiers::NONE, false),
        ];
        let original = events.clone();
        assert_eq!(feedback_scroll_events(&mut events, 0.0, 40.0, 120.0), None);
        assert_eq!(events, original);
    }

    #[test]
    fn body_regions_fit_short_viewports_without_a_minimum_picture_overflow() {
        for height in [0.0, 8.0, 80.0, 388.0, 568.0, 1200.0] {
            let rect =
                egui::Rect::from_min_size(egui::pos2(12.0, 240.0), egui::vec2(936.0, height));
            let layout = body_layout(rect, 8.0, 176.0, 58.0);
            for region in [layout.pictures, layout.waveform, layout.feedback] {
                assert!(rect.contains_rect(region));
                assert!(region.height() >= 0.0);
            }
            assert!(layout.pictures.bottom() <= layout.waveform.top());
            assert!(layout.waveform.bottom() <= layout.feedback.top());
            assert_eq!(layout.feedback.bottom(), rect.bottom());
            if height >= 388.0 {
                assert!(layout.pictures.height() >= 180.0);
                assert!(layout.waveform.height() >= 128.0);
                assert_eq!(layout.feedback.height(), 58.0);
            }
        }
    }
}

fn context_focus(context: &egui::Context, id: &str) {
    context.memory_mut(|memory| memory.request_focus(egui::Id::new(id)));
    if id == FOCUS {
        context.request_discard("initialize Trim heading focus filter");
    }
}

fn lock_heading(ui: &egui::Ui) {
    if ui.memory(|memory| {
        memory.has_focus(egui::Id::new(FOCUS)) && !memory.had_focus_last_frame(egui::Id::new(FOCUS))
    }) {
        ui.ctx()
            .request_discard("initialize Trim heading focus filter");
    }
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            egui::Id::new(FOCUS),
            egui::EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: true,
            },
        )
    });
}

#[derive(Default)]
struct KeyBatch {
    keys: Vec<TrimKey>,
    accept_amount: bool,
    amount_events: Option<Vec<egui::Event>>,
}

fn route_events(events: &mut Vec<egui::Event>, mut field: bool, mut background: bool) -> KeyBatch {
    let mut batch = KeyBatch::default();
    let mut shortcut_text: Option<egui::Key> = None;
    let mut focus_boundary = None;
    let mut retained = Vec::with_capacity(events.len());
    for event in std::mem::take(events) {
        if let egui::Event::Text(text) = &event {
            // Text sent before the heading's focus shortcut belonged to the
            // heading. It cannot be replayed into a field focused later in
            // this same native event batch, including unbound number keys.
            if background && !field {
                shortcut_text = None;
                continue;
            }
            let suppress = shortcut_text.is_some_and(|key| match key {
                egui::Key::H => text.eq_ignore_ascii_case("h"),
                egui::Key::L => text.eq_ignore_ascii_case("l"),
                egui::Key::R => text.eq_ignore_ascii_case("r"),
                egui::Key::B => text.eq_ignore_ascii_case("b"),
                egui::Key::I => text.eq_ignore_ascii_case("i"),
                egui::Key::O => text.eq_ignore_ascii_case("o"),
                egui::Key::E => text.eq_ignore_ascii_case("e"),
                egui::Key::Space => text == " ",
                _ => false,
            });
            shortcut_text = None;
            if !suppress {
                retained.push(event);
            }
            continue;
        }
        let egui::Event::Key {
            key,
            modifiers,
            pressed: true,
            repeat,
            ..
        } = &event
        else {
            retained.push(event);
            continue;
        };
        shortcut_text = None;
        if field && *key == egui::Key::Enter && modifiers.is_none() && !repeat {
            batch.accept_amount = true;
            continue;
        }
        if let Some(action) =
            navigation::trim::route_key(*key, *modifiers, field, background, false, *repeat)
        {
            batch.keys.push(action);
            shortcut_text = Some(*key);
            if action == TrimKey::FocusAmount {
                focus_boundary = Some(retained.len());
                field = true;
                background = false;
            }
        } else {
            retained.push(event);
        }
    }
    if let Some(start) = focus_boundary {
        // Only events after E belong to the newly focused native field. Moving
        // this suffix also prevents another widget consuming it before the
        // amount field and avoids ambiguous matching of repeated global keys.
        batch.amount_events = Some(retained.split_off(start));
    }
    *events = retained;
    batch
}

fn with_amount_events<R>(
    ui: &mut egui::Ui,
    events: Option<Vec<egui::Event>>,
    show: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let Some(events) = events else {
        return show(ui);
    };
    let prefix = ui.input_mut(|input| std::mem::replace(&mut input.events, events));
    let result = show(ui);
    ui.input_mut(|input| {
        // Preserve earlier unclaimed global keys, but restore only suffix
        // events still present after the native widget. Never resurrect a
        // consumed key. No event storage survives this widget invocation.
        let mut remaining = std::mem::replace(&mut input.events, prefix);
        input.events.append(&mut remaining);
    });
    result
}
