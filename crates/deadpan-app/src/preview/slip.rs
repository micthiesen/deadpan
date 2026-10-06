//! Captured Source Slip with real stopped proposed pictures and one durable commit.

use deadpan_core::{ExactRatio, SourceSlipClamp};

use super::*;
use crate::navigation::slip::SlipKey;
use crate::project::slip::{Prepared, Proposal, ProposalId, ProposalUpdate, Target};

const FOCUS: &str = "slip-preview-focus";
const AMOUNT: &str = "slip-preview-amount";

#[derive(Clone)]
pub(super) struct Capture {
    target: Target,
    source_cursor: u64,
    selected_source: Option<AssetId>,
    pane: Pane,
    view: View,
    selection: edit_range::Selection,
}

pub(super) struct Draft {
    capture: Capture,
    proposal: Proposal,
    prepared: Option<Arc<Prepared>>,
    pending: Option<ProposalId>,
    issued: Option<ProposalId>,
    dirty: bool,
    applying: bool,
    invalidated: bool,
    error: Option<String>,
    amount: String,
    limits: Option<(i64, i64)>,
    before: bool,
    inspection: ProjectFrame,
    observed: Option<(ProposalId, ProjectFrame, Ticket)>,
    focus_pending: bool,
    keys: Vec<SlipKey>,
    label: String,
    scope_label: String,
}

fn inspection_frame(target: &Target) -> ProjectFrame {
    ProjectFrame(
        target
            .cursor
            .0
            .clamp(target.range.start().0, target.range.end().0 - 1),
    )
}

fn nudge(current: i64, delta: i64, limits: Option<(i64, i64)>) -> Result<i64, String> {
    // Clamp the starting point as well: one reverse key after an overshoot
    // immediately leaves the reached handle instead of consuming hidden steps.
    let current = limits.map_or(current, |(min, max)| current.clamp(min, max));
    let next = current
        .checked_add(delta)
        .ok_or("Slip amount exceeds the frame range.")?;
    Ok(limits.map_or(next, |(min, max)| next.clamp(min, max)))
}

impl Draft {
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn proposal_for_check(&self) -> &Proposal {
        &self.proposal
    }
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn prepared_for_check(&self) -> Option<&Arc<Prepared>> {
        self.prepared.as_ref()
    }
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn before_for_check(&self) -> bool {
        self.before
    }
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn inspection_for_check(&self) -> ProjectFrame {
        self.inspection
    }
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn ready_for_check(&self) -> bool {
        self.can_apply()
    }
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn applying_for_check(&self) -> bool {
        self.applying
    }
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn error_for_check(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn ready(&self) -> Option<&Arc<Prepared>> {
        (!self.dirty && self.pending.is_none() && !self.invalidated && self.error.is_none())
            .then_some(self.prepared.as_ref())
            .flatten()
    }

    fn changed(&mut self, value: Result<i64, String>) {
        self.prepared = None;
        self.observed = None;
        self.before = false;
        self.error = None;
        self.dirty = false;
        let Some(change) = self.proposal.change.checked_add(1) else {
            self.invalidated = true;
            self.error = Some("Slip preview counter exhausted. Cancel and reopen Slip.".into());
            return;
        };
        self.proposal.change = change;
        match value {
            Ok(value) => {
                self.proposal.delta_frames = value;
                self.dirty = true;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn receive(&mut self, update: ProposalUpdate) -> bool {
        if self.pending.as_ref() != Some(&update.id) {
            return false;
        }
        self.pending = None;
        if self.invalidated || self.proposal.id() != update.id {
            return false;
        }
        let result = update.result.and_then(|prepared| {
            self.proposal.target.validate(&prepared.base)?;
            if prepared.target != self.proposal.target
                || prepared.resolution.requested_delta_frames != self.proposal.delta_frames
                || (prepared.resolution.applied_delta_frames == 0) != prepared.snapshot.is_none()
            {
                return Err(
                    "Slip returned a different target or amount; reopen the preview.".into(),
                );
            }
            if let Some(snapshot) = &prepared.snapshot {
                let expected = deadpan_playback::ContentIdentity::Proposed {
                    base_revision: update.id.base_revision.clone(),
                    draft: update.id.draft,
                    change: update.id.change,
                };
                if snapshot.session != update.id.session
                    || snapshot.document.project_id() != &update.id.project
                    || snapshot.content != expected
                {
                    return Err(
                        "Slip returned a different proposed picture; reopen the preview.".into(),
                    );
                }
            }
            Ok(prepared)
        });
        self.observed = None;
        match result {
            Ok(prepared) => {
                self.limits = Some((
                    prepared.resolution.minimum_delta_frames,
                    prepared.resolution.maximum_delta_frames,
                ));
                self.prepared = Some(prepared);
                self.error = None;
            }
            Err(error) => {
                self.prepared = None;
                self.error = Some(error);
            }
        }
        true
    }

    fn can_apply(&self) -> bool {
        !self.applying
            && !self.before
            && self
                .ready()
                .is_some_and(|prepared| prepared.snapshot.is_some())
            && self.observed.as_ref().is_some_and(|(id, frame, _)| {
                *id == self.proposal.id() && *frame == self.inspection
            })
    }

    fn status(&self) -> String {
        if self.applying {
            return "Saving Slip…".into();
        }
        if let Some(error) = &self.error {
            return error.clone();
        }
        let Some(prepared) = self.ready() else {
            return "Preparing Slip… Previous displayed picture is retained.".into();
        };
        let result = &prepared.resolution;
        let clamp = match result.clamp {
            Some(SourceSlipClamp::PictureStart) => " · Picture start handle reached",
            Some(SourceSlipClamp::PictureEnd) => " · Picture end handle reached",
            None => "",
        };
        format!(
            "Requested {:+}f · Applied {:+}f{clamp}{}",
            result.requested_delta_frames,
            result.applied_delta_frames,
            if result.applied_delta_frames == 0 {
                " · No movement; no edit will be saved."
            } else {
                ""
            }
        )
    }
}

impl DeadpanApp {
    pub(super) fn capture_slip_target(&self) -> Result<Capture, String> {
        if self.view != View::Sequence
            || matches!(self.pane, Pane::Sources | Pane::Sounds)
            || self.event_focused()
            || self.sound_focused()
        {
            return Err("Select a picture beat in Your edit before opening :slip.".into());
        }
        if self.edit_range.active || self.edit_selection() != navigation::EditSelection::None {
            return Err("Slip edits the selected beat. Clear the Visual Edit selection with Esc before opening :slip.".into());
        }
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open a project before Slip.")?;
        let target = Target::capture(
            workspace,
            self.sequence_scope.clone(),
            self.selected_beat.as_ref(),
            ProjectFrame(i64::try_from(self.sequence_cursor).map_err(|e| e.to_string())?),
        )?;
        Ok(Capture {
            target,
            source_cursor: self.source_cursor,
            selected_source: self.selected_source.clone(),
            pane: self.pane,
            view: self.view,
            selection: self.edit_range.clone(),
        })
    }

    fn slip_context_matches(&self, capture: &Capture, entering: bool) -> bool {
        self.workspace
            .as_ref()
            .is_some_and(|workspace| capture.target.validate(workspace).is_ok())
            && self.selected_beat.as_ref() == Some(&capture.target.node)
            && self.sequence_scope == capture.target.scope
            && i64::try_from(self.sequence_cursor).ok() == Some(capture.target.cursor.0)
            && self.source_cursor == capture.source_cursor
            && self.selected_source == capture.selected_source
            && self.view == capture.view
            && self.edit_range == capture.selection
            && !self.sound_focused()
            && !self.event_focused()
            && (!entering || self.pane == capture.pane)
    }

    pub(super) fn open_slip(
        &mut self,
        captured: Option<Result<Capture, String>>,
        amount: i64,
        context: &egui::Context,
    ) {
        let capture = match captured
            .unwrap_or_else(|| Err("Open :slip again to capture its target.".into()))
        {
            Ok(capture) if self.slip_context_matches(&capture, true) => capture,
            Ok(_) => {
                self.error = Some(
                    "The selected beat, scope or cursor changed. Reopen :slip; no edit was made."
                        .into(),
                );
                return;
            }
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(token) = self.next_serial() else {
            return;
        };
        let label = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.document.nodes().get(&capture.target.node))
            .map(|node| node.label.clone())
            .unwrap_or_else(|| "Selected beat".into());
        let inspection = inspection_frame(&capture.target);
        let proposal = Proposal {
            target: capture.target.clone(),
            draft: token,
            change: 1,
            delta_frames: amount,
        };
        self.stop_playback();
        self.cancel_camera();
        self.cancel_repeats("Slip preview opened");
        self.help_open = false;
        self.bindings.clear();
        self.error = None;
        self.message = None;
        self.slip = Some(Draft {
            capture,
            proposal,
            prepared: None,
            pending: None,
            issued: None,
            dirty: true,
            applying: false,
            invalidated: false,
            error: None,
            amount: format!("{amount:+}f"),
            limits: None,
            before: false,
            inspection,
            observed: None,
            focus_pending: true,
            keys: vec![],
            label,
            scope_label: if self.scope_labels.is_empty() {
                "Your edit".into()
            } else {
                format!("Your edit / {}", self.scope_labels.join(" / "))
            },
        });
        self.pane = Pane::Viewer;
        self.invalidate_slip_picture();
        self.service.set_preview_active(true);
        context.request_discard("Slip preview opened");
        context.request_repaint();
    }

    pub(super) fn finish_slip_update(&mut self, update: &mut crate::project::ProjectUpdate) {
        // A durable receipt may remain in every later publication. Consume it
        // only for this exact applying draft, never as a new selection event or
        // a repeated historical refresh warning after navigation/Undo.
        if let Some(receipt) = &update.saved_slip
            && self
                .slip
                .as_ref()
                .is_some_and(|draft| draft.applying && draft.proposal.id() == receipt.id)
        {
            let same_project = update.workspace.as_ref().is_some_and(|workspace| {
                workspace.session == receipt.id.session
                    && workspace.document.project_id() == &receipt.id.project
            });
            if same_project
                && update.workspace.as_ref().is_some_and(|workspace| {
                    workspace.document.revision_id() == &receipt.committed.revision
                })
                && update.committed.is_none()
                && self.last_committed.as_ref() != Some(&receipt.committed.revision)
            {
                update.committed = Some(receipt.committed.clone());
            }
            if same_project
                && update.workspace.as_ref().is_some_and(|workspace| {
                    workspace.document.revision_id() == &receipt.id.base_revision
                })
                && let Some(error) = &receipt.refresh_error
            {
                update.message = Some(format!(
                    "Slip saved, but the preview could not refresh: {error}. Reopen the project to view the saved edit."
                ));
            }
            self.slip = None;
            self.bindings.clear();
            self.invalidate_slip_picture();
        }
        if let Some(commit) = &update.slip_commit
            && let Some(draft) = &mut self.slip
            && draft.applying
            && draft.proposal.id() == commit.id
        {
            match &commit.result {
                Ok(_) => {
                    self.slip = None;
                    self.bindings.clear();
                    self.invalidate_slip_picture();
                }
                Err(error) => {
                    draft.applying = false;
                    draft.prepared = None;
                    draft.observed = None;
                    draft.error = Some(error.clone());
                }
            }
        }
    }

    pub(super) fn receive_slip(&mut self, update: Option<ProposalUpdate>) {
        if let Some(update) = update
            && let Some(draft) = &mut self.slip
            && draft.receive(update)
        {
            self.slip_picture_pending = true;
        }
    }

    pub(super) fn reconcile_slip(&mut self, context: &egui::Context) {
        let stale = self.slip.as_ref().is_some_and(|draft| {
            !draft.invalidated && !self.slip_context_matches(&draft.capture, false)
        });
        if stale && let Some(draft) = &mut self.slip {
            draft.invalidated = true;
            draft.prepared = None;
            draft.observed = None;
            draft.dirty = false;
            draft.error = Some(
                "The saved edit or captured selection changed. Cancel and reopen Slip.".into(),
            );
            self.slip_abandon = draft.issued.clone();
            self.invalidate_slip_picture();
            context.request_repaint();
        }
    }

    fn invalidate_slip_picture(&mut self) {
        // Revoke the ticket at the logical boundary. The next final layout
        // pass can request a replacement without admitting an old reply first.
        self.worker.cancel();
        self.presentation.invalidate_pending();
        self.slip_picture_pending = true;
    }

    pub(super) fn dispatch_slip(&mut self, context: &egui::Context) {
        if !self.service.is_busy() {
            if let Some(id) = self.slip_abandon.clone() {
                if self.service.submit(ProjectRequest::AbandonSlip(id)).is_ok() {
                    self.slip_abandon = None;
                }
            } else if let Some(draft) = &mut self.slip
                && draft.dirty
                && draft.pending.is_none()
                && !draft.invalidated
                && !draft.applying
            {
                match self
                    .service
                    .submit(ProjectRequest::PrepareSlip(draft.proposal.clone()))
                {
                    Ok(()) => {
                        draft.pending = Some(draft.proposal.id());
                        draft.issued = draft.pending.clone();
                        draft.dirty = false;
                    }
                    Err(error) => draft.error = Some(error),
                }
                context.request_repaint();
            }
        }
        if std::mem::take(&mut self.slip_picture_pending) {
            self.request_picture(false);
        }
    }

    pub(super) fn slip_picture_work(&self) -> Option<Work> {
        let draft = self.slip.as_ref()?;
        if !self.slip_context_matches(&draft.capture, false) {
            return None;
        }
        let prepared = draft.ready()?;
        let view = ProjectView::Sequence {
            frame: draft.inspection,
        };
        if draft.before || prepared.snapshot.is_none() {
            Some(Work::Project {
                workspace: prepared.base.clone(),
                view,
            })
        } else {
            Some(Work::Proposed {
                base: prepared.base.clone(),
                snapshot: prepared.snapshot.as_ref()?.clone(),
                view,
            })
        }
    }

    fn close_slip(&mut self, context: &egui::Context) {
        if self.slip.as_ref().is_some_and(|draft| draft.applying) {
            return;
        }
        if let Some(draft) = self.slip.take() {
            self.slip_abandon = draft.issued;
            if self
                .workspace
                .as_ref()
                .is_some_and(|workspace| draft.capture.target.validate(workspace).is_ok())
            {
                self.sequence_cursor = u64::try_from(draft.capture.target.cursor.0)
                    .expect("captured nonnegative cursor");
                self.source_cursor = draft.capture.source_cursor;
                self.selected_source = draft.capture.selected_source;
                self.sequence_scope = draft.capture.target.scope;
                self.selected_beat = Some(draft.capture.target.node);
                self.edit_range = draft.capture.selection;
                self.pane = draft.capture.pane;
                self.view = draft.capture.view;
            }
        }
        self.bindings.clear();
        self.invalidate_slip_picture();
        context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
        context.request_discard("Slip preview closed");
        context.request_repaint();
    }

    fn slip_action(&mut self, action: SlipKey, context: &egui::Context) {
        if action == SlipKey::Cancel {
            self.close_slip(context);
            return;
        }
        let Some(draft) = &mut self.slip else {
            return;
        };
        if draft.invalidated || draft.applying {
            return;
        }
        match action {
            SlipKey::Apply => {
                if !draft.can_apply() || self.service.is_busy() {
                    return;
                }
                match self
                    .service
                    .submit(ProjectRequest::CommitSlip(draft.proposal.id()))
                {
                    Ok(()) => {
                        draft.applying = true;
                        draft.keys.clear();
                    }
                    Err(error) => draft.error = Some(error),
                }
            }
            SlipKey::Nudge(delta) => {
                let base = draft
                    .ready()
                    .map_or(draft.proposal.delta_frames, |prepared| {
                        prepared.resolution.applied_delta_frames
                    });
                let value = nudge(base, delta, draft.limits);
                if let Ok(value) = value {
                    draft.amount = format!("{value:+}f");
                }
                draft.changed(value);
                self.invalidate_slip_picture();
            }
            SlipKey::Compare => {
                draft.before = !draft.before;
                draft.observed = None;
                self.invalidate_slip_picture();
            }
            SlipKey::First | SlipKey::Last | SlipKey::Inspect(_) => {
                let range = draft.proposal.target.range;
                draft.inspection = ProjectFrame(match action {
                    SlipKey::First => range.start().0,
                    SlipKey::Last => range.end().0 - 1,
                    SlipKey::Inspect(delta) => draft
                        .inspection
                        .0
                        .saturating_add(delta)
                        .clamp(range.start().0, range.end().0 - 1),
                    _ => unreachable!(),
                });
                draft.observed = None;
                self.invalidate_slip_picture();
            }
            SlipKey::Cancel => unreachable!(),
        }
        context.request_discard("Slip controls changed");
        context.request_repaint();
    }

    pub(super) fn slip_keyboard(&mut self, context: &egui::Context) {
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
        let field = context.memory(|memory| memory.has_focus(egui::Id::new(AMOUNT)));
        let background = context.memory(|memory| {
            memory
                .focused()
                .is_none_or(|id| id == egui::Id::new(FOCUS) || id == pane_id(self.pane))
        });
        let Some(draft) = &mut self.slip else {
            return;
        };
        context.input_mut(|input| {
            let mut events = std::mem::take(&mut input.events).into_iter().peekable();
            while let Some(event) = events.next() {
                if let egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    repeat,
                    ..
                } = &event
                    // The companion text names the typed key: a character no
                    // Slip key names stays native input.
                    && let Some(action) = navigation::mode_key(
                        *key,
                        *modifiers,
                        super::editor_input::companion_text(*key, events.peek()),
                    )
                    .and_then(|(key, modifiers)| {
                        navigation::slip::route_key(key, modifiers, field, background, false, *repeat)
                    })
                {
                    draft.keys.push(action);
                    continue;
                }
                input.events.push(event);
            }
        });
    }

    pub(super) fn slip_workspace(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().frame(style::panel()).show(ui, |ui| {
            let Some(mut draft) = self.slip.take() else { return; };
            let keys = std::mem::take(&mut draft.keys);
            let mut actions = Vec::new();
            let heading = ui.horizontal_wrapped(|ui| {
                let title = ui.heading("Slip preview");
                let status = ui.colored_label(style::CURSOR, "UNSAVED · Stopped picture");
                title.rect.union(status.rect)
            }).inner;
            let focus = ui.interact(heading, egui::Id::new(FOCUS), egui::Sense::click());
            focus.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Slip preview keyboard controls"));
            if draft.focus_pending { focus.request_focus(); draft.focus_pending = false; }
            if focus.has_focus() { ui.painter().rect_stroke(heading, 2.0, egui::Stroke::new(1.0, style::CURSOR), egui::StrokeKind::Inside); }
            ui.label(format!("{} · {} · Edit frames [{}..{})", draft.scope_label, draft.label, draft.proposal.target.range.start().0, draft.proposal.target.range.end().0));
            let editable = !draft.invalidated && !draft.applying;
            ui.horizontal_wrapped(|ui| {
                ui.label("Slip amount");
                let amount = ui.add_enabled(editable, egui::TextEdit::singleline(&mut draft.amount).id(egui::Id::new(AMOUNT)).desired_width(115.0));
                amount.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, editable, "Signed Slip amount in project frames, for example +5f"));
                if amount.changed() {
                    let value = navigation::slip::parse_frames(&draft.amount);
                    draft.changed(value); self.invalidate_slip_picture();
                    ui.ctx().request_discard("Slip amount text changed");
                }
                if ui.add_enabled(editable, style::action("−1f", "h")).clicked() { actions.push(SlipKey::Nudge(-1)); }
                if ui.add_enabled(editable, style::action("+1f", "l")).clicked() { actions.push(SlipKey::Nudge(1)); }
                if ui.add_enabled(editable, egui::Button::new(if draft.before { "Before · b" } else { "Proposed · b" }).selected(!draft.before)).clicked() { actions.push(SlipKey::Compare); }
                if ui.add_enabled(editable, style::action("First picture", "i")).clicked() { actions.push(SlipKey::First); }
                if ui.add_enabled(editable, style::action("Last picture", "o")).clicked() { actions.push(SlipKey::Last); }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(editable, style::action("Previous picture", "Left")).clicked() { actions.push(SlipKey::Inspect(-1)); }
                if ui.add_enabled(editable, style::action("Next picture", "Right")).clicked() { actions.push(SlipKey::Inspect(1)); }
                if ui.add_enabled(draft.can_apply() && !self.service.is_busy(), style::action("Apply Slip", crate::navigation::registry::mode_label("slip.apply")).fill(style::SELECTED)).on_disabled_hover_text("Apply requires a nonzero current proposal displayed at this inspection frame. Choose Proposed and wait for its picture.").clicked() { actions.push(SlipKey::Apply); }
                if ui.add_enabled(!draft.applying, style::action("Cancel", crate::navigation::registry::mode_label("slip.cancel"))).clicked() { actions.push(SlipKey::Cancel); }
            });
            ui.weak("h/l changes Slip; Shift gives 10 frames. Arrows inspect pictures. Tab selects controls. Playback is stopped.");
            if !keys.is_empty() || !actions.is_empty() { ui.ctx().request_discard("Slip input precedes picture submission"); }
            let mut lines = vec![draft.status()];
            // Keep a status row in every state so readiness alone cannot
            // resize the viewer and invalidate the display that enabled Apply.
            lines.push(if draft.applying { "Saving the displayed Slip." }
                else if draft.before { "Before comparison. Choose Proposed to apply." }
                else if draft.can_apply() { "Current Proposed picture displayed. Enter applies one Slip." }
                else if draft.ready().is_some_and(|prepared| prepared.snapshot.is_none()) { "Unchanged picture. No transaction will be saved." }
                else { "Apply waits for the current Proposed picture; prior display is retained." }.into());
            if let Some(error) = &self.error { lines.push(error.clone()); }
            if let Some(prepared) = draft.ready() {
                let resolution = &prepared.resolution;
                lines.push(format!("Handles: {}f to {}f exact · whole-frame moves {:+}f to {:+}f", ratio_label(resolution.minimum_delta), ratio_label(resolution.maximum_delta), resolution.minimum_delta_frames, resolution.maximum_delta_frames));
            }
            lines.push(format!("Inspecting Edit picture {} · real Edit cursor {} unchanged · Original cursor {} unchanged", i128::from(draft.inspection.0) + 1, draft.capture.target.cursor.0, u128::from(draft.capture.source_cursor) + 1));
            if let Some(error) = self.presentation.error() { lines.push(format!("Picture unavailable: {error}")); }
            let footer: Vec<_> = lines.into_iter().map(|line| egui::WidgetText::from(egui::RichText::new(line).color(style::CURSOR)).into_galley(ui, Some(egui::TextWrapMode::Wrap), ui.available_width(), egui::TextStyle::Body)).collect();
            let reserved = footer.iter().map(|line| line.size().y + ui.spacing().item_spacing.y).sum::<f32>() + 48.0;
            let size = egui::vec2(ui.available_width().max(1.0), (ui.available_height() - reserved).max(1.0));
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter().rect_filled(rect, 3.0, egui::Color32::BLACK);
            let canvas = self.presentation.canvas().map_or(rect, |(width, height)| fit_rect(rect, width as f32 / height as f32));
            self.render_picture(ui.ctx(), canvas.size());
            if self.presentation.has_displayed() && let Some(target) = &self.target {
                ui.painter().image(target.texture, canvas, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
            }
            let raster = target_size(canvas.size(), ui.ctx().pixels_per_point());
            draft.observed = None;
            if !draft.before && let Some(prepared) = draft.ready() && let Some(snapshot) = &prepared.snapshot
                && self.target.as_ref().is_some_and(|target| target.target.width() == raster.0 && target.target.height() == raster.1)
                && let Some(ticket) = self.presentation.stable_proposed_ticket(snapshot.session, snapshot.document.project_id(), snapshot.document.revision_id(), &snapshot.content, draft.inspection) {
                draft.observed = Some((draft.proposal.id(), draft.inspection, ticket));
            }
            let label = self.presentation.displayed_label().unwrap_or_else(|| "No picture displayed".into());
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &label));
            ui.label(label);
            ui.weak(if draft.before { "Requested: Before · saved edit" }
                else if draft.ready().is_some_and(|prepared| prepared.snapshot.is_none()) { "Unchanged · saved edit" }
                else { "Requested: Proposed · unsaved edit" });
            for line in footer { ui.add(egui::Label::new(line)); }
            self.slip = Some(draft);
            for action in keys.into_iter().chain(actions) { self.slip_action(action, ui.ctx()); }
        });
    }
}

fn ratio_label(value: ExactRatio) -> String {
    if value.denominator() == 1 {
        value.numerator().to_string()
    } else {
        format!("{}/{}", value.numerator(), value.denominator())
    }
}

#[cfg(test)]
mod tests;
