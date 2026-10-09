//! Native, keyboard-accessible named snapshots. All writes go to the owner.

use super::*;
use crate::project::takes::{Operation, Request, Update};
use deadpan_store::takes::{Action, TakeCatalog, TakeId, TakeName};

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    pub requested: bool,
    session: Option<u64>,
    catalog: Option<TakeCatalog>,
    selected: Option<TakeId>,
    name: String,
    queued: Option<Request>,
    pending: Option<(u64, u64)>,
    error: Option<String>,
    message: Option<String>,
    prior_focus: Option<egui::Id>,
    return_focus: Option<(u64, egui::Id)>,
    focus_name: bool,
}

impl State {
    fn loading(&self) -> bool {
        self.queued.is_some() || self.pending.is_some()
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn ready_for_check(&self) -> bool {
        self.open && self.catalog.is_some() && !self.loading()
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn selected_name_for_check(&self) -> Option<&str> {
        self.catalog
            .as_ref()?
            .entries
            .iter()
            .find(|take| Some(&take.id) == self.selected.as_ref())
            .map(|take| take.name.as_str())
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn error_for_check(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn receive(&mut self, update: Update) {
        if self.pending != Some((update.session, update.ticket)) {
            return;
        }
        self.pending = None;
        match update.result {
            Ok(receipt) => {
                self.catalog = Some(receipt.catalog);
                self.error = receipt.refresh_error;
                self.message = Some(
                    if receipt.committed_revision.is_some() {
                        "Take opened. Undo returns to the previous edit."
                    } else if receipt.changed {
                        "Saved takes updated. The current edit is unchanged."
                    } else {
                        "Showing saved takes."
                    }
                    .into(),
                );
            }
            Err(error) => self.error = Some(error),
        }
    }
}

enum Choice {
    Refresh,
    Create,
    Update,
    Rename,
    Delete,
    Open,
    Close,
}

impl DeadpanApp {
    fn takes_preview_active(&self) -> bool {
        self.camera.is_some()
            || self.camera_pending.is_some()
            || self.gain.is_some()
            || self.room_tone.is_some()
            || self.splice.is_some()
            || self.slip.is_some()
            || self.trim.is_some()
    }

    pub(super) fn receive_takes(
        &mut self,
        update: Option<Update>,
        catalog: Option<TakeCatalog>,
        service_error: Option<&str>,
    ) {
        if self.takes.session.is_some_and(|session| {
            self.workspace
                .as_ref()
                .is_none_or(|workspace| workspace.session != session)
        }) {
            self.takes = State::default();
        }
        if let Some(update) = update {
            self.takes.receive(update);
        }
        if let Some(catalog) = catalog
            && self.takes.open
            && self.workspace.as_ref().is_some_and(|workspace| {
                workspace.document.project_id() == &catalog.project_id
                    && workspace.document.revision_id() == &catalog.revision_id
            })
            && self
                .takes
                .catalog
                .as_ref()
                .is_none_or(|current| catalog.version >= current.version && current != &catalog)
        {
            self.takes.catalog = Some(catalog);
        }
        if self.takes.open
            && !self.takes.loading()
            && self.takes.error.is_none()
            && self.workspace.as_ref().is_some_and(|workspace| {
                self.takes
                    .catalog
                    .as_ref()
                    .is_some_and(|catalog| &catalog.revision_id != workspace.document.revision_id())
            })
        {
            if let Some(error) = service_error {
                self.takes.error = Some(error.into());
            } else {
                self.queue_take(Operation::List);
            }
        }
    }

    pub(super) fn takes_keyboard(&mut self, context: &egui::Context) -> bool {
        if !self.takes.open
            && let Some((closed, focus)) = self.takes.return_focus
            && context.cumulative_frame_nr() > closed
        {
            context.memory_mut(|memory| memory.request_focus(focus));
            self.takes.return_focus = None;
        }
        if !self.takes.open {
            return false;
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
                            key: egui::Key::Enter | egui::Key::Space | egui::Key::Escape,
                            ..
                        }
                    )
                })
            });
        }
        true
    }

    pub(super) fn dispatch_takes(&mut self, context: &egui::Context) {
        if self.takes.requested {
            self.takes.requested = false;
            if self.render.blocking() || self.dialogs.is_open() || self.takes_preview_active() {
                self.error =
                    Some("Finish or cancel the current preview before opening takes.".into());
                return;
            }
            let Some(workspace) = &self.workspace else {
                self.error = Some("Open a project to see its takes.".into());
                return;
            };
            self.takes = State {
                open: true,
                session: Some(workspace.session),
                prior_focus: Some(
                    context
                        .memory(|memory| memory.focused())
                        .unwrap_or_else(|| pane_id(self.pane)),
                ),
                focus_name: true,
                ..Default::default()
            };
            self.queue_take(Operation::List);
            self.pause_playback();
            self.cancel_repeats("named takes were opened");
            self.bindings.clear();
            context.request_repaint();
        }
        if !self.takes.open || self.service.is_busy() {
            return;
        }
        let Some(request) = self.takes.queued.take() else {
            return;
        };
        let identity = (request.session, request.ticket);
        match self.service.submit(ProjectRequest::Takes(request)) {
            Ok(()) => self.takes.pending = Some(identity),
            Err(error) => self.takes.error = Some(error),
        }
    }

    fn queue_take(&mut self, operation: Operation) {
        let Some(ticket) = self.next_serial() else {
            return;
        };
        let Some(workspace) = &self.workspace else {
            return;
        };
        self.takes.queued = Some(Request {
            ticket,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            operation,
        });
        self.takes.error = None;
        self.takes.message = None;
    }

    pub(super) fn takes_window(&mut self, context: &egui::Context) {
        if !self.takes.open {
            return;
        }
        let busy = self.takes.loading() || self.service.is_busy();
        let writable = self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.read_only.is_none())
            && !self.takes_preview_active();
        let selected = self
            .takes
            .catalog
            .as_ref()
            .and_then(|catalog| {
                catalog
                    .entries
                    .iter()
                    .find(|take| Some(&take.id) == self.takes.selected.as_ref())
            })
            .cloned();
        let mut choice = None;
        let modal = egui::Modal::new(egui::Id::new("named-takes")).show(context, |ui| {
            accessibility::dialog(ui, "Named takes");
            ui.set_width(600.0_f32.min(context.content_rect().width() - 64.0));
            ui.heading("Named takes");
            ui.weak("Tab / Shift Tab move between controls · Enter activates · Esc returns to the editor");
            ui.label("Keep versions of your edit. Opening a take is one edit you can undo.");
            if let Some(reason) = self.workspace.as_ref().and_then(|workspace| workspace.read_only.as_ref()) { ui.label(reason.as_ref()); }
            ui.horizontal(|ui| {
                let label = ui.label("Take name");
                let field = ui.add(egui::TextEdit::singleline(&mut self.takes.name).id(egui::Id::new("take-name")).hint_text("Name this version").char_limit(128));
                let field = field.labelled_by(label.id);
                if self.takes.focus_name { field.request_focus(); self.takes.focus_name = false; }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!busy && writable && self.takes.catalog.as_ref().is_some_and(|catalog| catalog.entries.len() < deadpan_store::takes::MAX_TAKES), egui::Button::new("Save new take")).clicked() { choice = Some(Choice::Create); }
                if ui.add_enabled(!busy, egui::Button::new("Refresh takes")).clicked() { choice = Some(Choice::Refresh); }
            });
            ui.separator();
            egui::ScrollArea::vertical().id_salt("take-list").max_height((context.content_rect().height() - 355.0).max(90.0)).show(ui, |ui| {
                if let Some(catalog) = &self.takes.catalog {
                    if catalog.entries.is_empty() { ui.weak("No named takes yet."); }
                    let mut entries: Vec<_> = catalog.entries.iter().collect();
                    entries.sort_by(|left, right| left.name.cmp(&right.name));
                    for take in entries {
                        let selected = self.takes.selected.as_ref() == Some(&take.id);
                        let clicked = ui.push_id(take.id.as_str(), |ui| {
                            let response = ui.add_enabled(!busy, egui::Button::selectable(selected, take.name.as_str()).wrap());
                            if response.gained_focus() {
                                response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
                            }
                            response.clicked()
                        }).inner;
                        if clicked {
                            self.takes.selected = Some(take.id.clone());
                            self.takes.name = take.name.as_str().into();
                        }
                    }
                } else { ui.weak(if busy { "Loading takes…" } else { "Refresh to load takes." }); }
            });
            ui.horizontal_wrapped(|ui| {
                let enabled = !busy && writable && selected.is_some();
                for (label, action) in [("Open selected take", Choice::Open), ("Update to current edit", Choice::Update), ("Rename selected take", Choice::Rename), ("Delete selected take", Choice::Delete)] {
                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() { choice = Some(action); }
                }
            });
            ui.weak("Update replaces the selected take with the current saved edit. Delete removes only its name; history and media stay retained.");
            if let Some(message) = &self.takes.message { ui.label(message); }
            if let Some(error) = &self.takes.error { ui.colored_label(style::ERROR, error); }
            if ui.button("Back to editor  Esc").clicked() { choice = Some(Choice::Close); }
        });
        if modal.should_close() && !self.ime_composing {
            choice = Some(Choice::Close);
        }
        if let Some(choice) = choice {
            self.take_choice(choice, context);
        }
    }

    fn take_choice(&mut self, choice: Choice, context: &egui::Context) {
        match choice {
            Choice::Close => {
                self.takes.open = false;
                self.takes.queued = None;
                if let Some(focus) = self.takes.prior_focus {
                    self.takes.return_focus = Some((context.cumulative_frame_nr(), focus));
                }
                context.request_discard("takes closed");
                context.request_repaint();
                return;
            }
            Choice::Refresh => {
                self.queue_take(Operation::List);
                return;
            }
            _ => {}
        }
        let result = self.take_action(choice);
        match result {
            Ok(operation) => self.queue_take(Operation::Apply(operation)),
            Err(error) => self.takes.error = Some(error),
        }
    }

    fn take_action(&self, choice: Choice) -> Result<deadpan_store::takes::Request, String> {
        let catalog = self.takes.catalog.as_ref().ok_or("Refresh takes first")?;
        let selected = || {
            catalog
                .entries
                .iter()
                .find(|take| Some(&take.id) == self.takes.selected.as_ref())
                .ok_or_else(|| "Select a take first".to_owned())
        };
        let name =
            || TakeName::new(self.takes.name.trim().to_owned()).map_err(|error| error.to_string());
        let action = match choice {
            Choice::Create => Action::Create {
                id: TakeId::new(uuid::Uuid::new_v4().to_string())
                    .map_err(|error| error.to_string())?,
                name: name()?,
            },
            Choice::Update => {
                let take = selected()?;
                Action::Update {
                    id: take.id.clone(),
                    expected_snapshot: take.revision_id.clone(),
                }
            }
            Choice::Rename => {
                let take = selected()?;
                Action::Rename {
                    id: take.id.clone(),
                    expected_snapshot: take.revision_id.clone(),
                    name: name()?,
                }
            }
            Choice::Delete => {
                let take = selected()?;
                Action::Delete {
                    id: take.id.clone(),
                    expected_snapshot: take.revision_id.clone(),
                }
            }
            Choice::Open => {
                let take = selected()?;
                Action::Restore {
                    id: take.id.clone(),
                    expected_snapshot: take.revision_id.clone(),
                    new_revision: RevisionId::new(uuid::Uuid::new_v4().to_string())
                        .map_err(|error| error.to_string())?,
                }
            }
            Choice::Refresh | Choice::Close => unreachable!(),
        };
        Ok(deadpan_store::takes::Request {
            project_id: catalog.project_id.clone(),
            expected_revision: catalog.revision_id.clone(),
            expected_version: catalog.version,
            action,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::takes::Receipt;
    use deadpan_core::ProjectId;

    fn catalog() -> TakeCatalog {
        TakeCatalog {
            project_id: ProjectId::new("project").unwrap(),
            revision_id: RevisionId::new("head").unwrap(),
            version: 4,
            entries: Vec::new(),
        }
    }

    #[test]
    fn only_the_captured_session_and_ticket_can_complete_a_take_request() {
        let mut state = State {
            open: true,
            session: Some(3),
            pending: Some((3, 19)),
            ..Default::default()
        };
        for (session, ticket) in [(4, 19), (3, 18)] {
            state.receive(Update {
                session,
                ticket,
                result: Err("stale".into()),
            });
            assert_eq!(state.pending, Some((3, 19)));
            assert!(state.error.is_none());
        }
        state.receive(Update {
            session: 3,
            ticket: 19,
            result: Ok(Receipt {
                catalog: catalog(),
                committed_revision: Some(RevisionId::new("saved").unwrap()),
                changed: true,
                refresh_error: Some("saved, but refresh failed".into()),
            }),
        });
        assert!(state.pending.is_none());
        assert_eq!(state.catalog.unwrap().version, 4);
        assert!(state.message.unwrap().contains("Take opened"));
        assert_eq!(state.error.as_deref(), Some("saved, but refresh failed"));
    }
}
