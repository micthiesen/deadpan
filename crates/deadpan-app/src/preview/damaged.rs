//! Native backup selection after an Open failure. No file work runs on the UI.

use super::*;
use crate::project::damaged::{Action, Offer, Outcome, Request, Update};
use deadpan_store::backups::BackupPreview;

fn reveal_focus(response: &egui::Response) {
    if response.gained_focus() {
        response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
    }
}

#[derive(Default)]
pub(super) struct State {
    offer: Option<Arc<Offer>>,
    dismissed: Option<u64>,
    selected: Option<String>,
    inspected: Option<(BackupPreview, bool)>,
    confirmation: String,
    pending: Option<u64>,
    error: Option<String>,
    warnings: Vec<String>,
    restored: Option<std::path::PathBuf>,
    focus_first: bool,
    return_focus: Option<(u64, egui::Id)>,
}

impl State {
    pub(super) fn open(&self) -> bool {
        self.offer.is_some()
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn inspected_for_check(&self) -> bool {
        self.inspected.is_some() && self.pending.is_none()
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn restored_for_check(&self) -> bool {
        self.restored.is_some() && self.pending.is_none()
    }
}

impl DeadpanApp {
    pub(super) fn receive_damaged(&mut self, update: Option<Update>) {
        let Some(update) = update else {
            self.damaged.offer = None;
            return;
        };
        if self.damaged.dismissed == Some(update.offer.id) {
            return;
        }
        if self
            .damaged
            .offer
            .as_ref()
            .is_none_or(|offer| offer.id != update.offer.id)
        {
            self.damaged = State {
                selected: update.offer.backups.first().map(|info| info.id.clone()),
                offer: Some(Arc::clone(&update.offer)),
                focus_first: true,
                ..Default::default()
            };
            self.bindings.clear();
            self.stop_playback();
        }
        let Some(reply) = update
            .reply
            .filter(|reply| Some(reply.ticket) == self.damaged.pending)
        else {
            return;
        };
        self.damaged.pending = None;
        match reply.result {
            Ok(Outcome::Inspected {
                preview,
                requires_project_confirmation,
            }) => {
                self.damaged.inspected = Some((preview, requires_project_confirmation));
                self.damaged.confirmation.clear();
                // The disabled checking button temporarily leaves the focus
                // chain. Restore its place so Tab reaches the next action.
                self.damaged.focus_first = true;
            }
            Ok(Outcome::Restored {
                quarantine,
                open_error,
                warnings,
            }) => {
                self.damaged.inspected = None;
                self.damaged.restored = Some(quarantine);
                self.damaged.error = open_error;
                self.damaged.warnings = warnings;
            }
            Err(error) => self.damaged.error = Some(error),
        }
    }

    pub(super) fn damaged_keyboard(&mut self, context: &egui::Context) -> bool {
        if !self.damaged.open() {
            if let Some((closed, focus)) = self.damaged.return_focus
                && context.cumulative_frame_nr() > closed
            {
                context.memory_mut(|memory| memory.request_focus(focus));
                self.damaged.return_focus = None;
            }
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

    pub(super) fn damaged_window(&mut self, context: &egui::Context) {
        let Some(offer) = self.damaged.offer.clone() else {
            return;
        };
        let busy = self.damaged.pending.is_some() || self.service.is_busy();
        let mut action = None;
        let modal = egui::Modal::new(egui::Id::new("damaged-project-recovery")).show(context, |ui| {
            accessibility::dialog(ui, "Recover a project from backup");
            ui.set_width(640.0_f32.min(context.content_rect().width() - 64.0));
            ui.set_height((context.content_rect().height() - 80.0).clamp(200.0, 620.0));
            ui.heading("Recover a project from backup");
            ui.weak("Tab / Shift Tab move between controls · Enter activates · Esc closes");
            egui::ScrollArea::vertical().max_height((ui.available_height() - 40.0).max(80.0)).auto_shrink([false, false]).show(ui, |ui| {
                ui.label(offer.path.display().to_string());
                if let Some(quarantine) = &self.damaged.restored {
                    ui.label("The backup was restored. Previous database files were kept at:");
                    ui.label(quarantine.display().to_string());
                    if self.damaged.error.is_none() { ui.label("The recovered project is open."); }
                } else {
                    ui.label(format!("This project could not open: {}", offer.error));
                    ui.label("Choose a backup and check it first. Restoring replaces this project's database and keeps the damaged files in a separate folder. Media files are retained.");
                    egui::ScrollArea::vertical().id_salt("damaged-backups").max_height(150.0).show(ui, |ui| {
                        for info in &offer.backups {
                            let label = format!("{} · {} · {}", crate::project::backups::age(info.created_unix_ms), info.reason.label(), storage::bytes(info.database_bytes));
                            let response = ui.push_id(&info.id, |ui| ui.add_enabled(!busy, egui::Button::selectable(self.damaged.selected.as_ref() == Some(&info.id), label).wrap())).inner;
                            reveal_focus(&response);
                            if response.clicked() {
                                self.damaged.selected = Some(info.id.clone());
                                self.damaged.inspected = None;
                                self.damaged.confirmation.clear();
                                self.damaged.error = None;
                            }
                        }
                    });
                    let check = ui.add_enabled(!busy && self.damaged.selected.is_some(), egui::Button::new("Check selected backup"));
                    reveal_focus(&check);
                    if self.damaged.focus_first { check.request_focus(); self.damaged.focus_first = false; }
                    if check.clicked() && let Some(backup) = &self.damaged.selected { action = Some(Action::Inspect { backup: backup.clone() }); }
                    if let Some((preview, requires_confirmation)) = &self.damaged.inspected {
                        ui.label("Backup verified");
                        ui.label(backups::describe(preview));
                        let confirmed = if *requires_confirmation {
                            ui.label(format!("The project manifest cannot confirm its identity. To use this backup, type its project ID exactly: {}", preview.project_id));
                            let label = ui.label("Confirm project ID");
                            let field = ui.add(egui::TextEdit::singleline(&mut self.damaged.confirmation).id_salt("recovery-project-id").char_limit(128)).labelled_by(label.id);
                            reveal_focus(&field);
                            self.damaged.confirmation == preview.project_id.as_str()
                        } else { true };
                        let restore = ui.add_enabled(!busy && confirmed, egui::Button::new("Restore checked backup"));
                        reveal_focus(&restore);
                        if restore.clicked() {
                            action = Some(Action::Restore {
                                backup: preview.info.id.clone(),
                                confirmed_project: requires_confirmation.then(|| preview.project_id.clone()),
                            });
                        }
                    }
                }
                if busy { ui.label("Checking or restoring the backup…"); }
                if let Some(error) = &self.damaged.error { ui.colored_label(style::ERROR, error); }
                for warning in &self.damaged.warnings { ui.colored_label(style::ERROR, warning); }
            });
            if ui.add_enabled(!busy, egui::Button::new("Close recovery  Esc")).clicked() { action = Some(Action::Dismiss); }
        });
        if modal.should_close() && !busy && !self.ime_composing {
            action = Some(Action::Dismiss);
        }
        let Some(action) = action else { return };
        let Some(ticket) = self.next_serial() else {
            return;
        };
        let dismiss = matches!(action, Action::Dismiss);
        let request = Request {
            offer: offer.id,
            ticket,
            action,
        };
        match self.service.submit(ProjectRequest::Damaged(request)) {
            Ok(()) if dismiss => {
                self.damaged.offer = None;
                self.damaged.dismissed = Some(offer.id);
                self.damaged.return_focus =
                    Some((context.cumulative_frame_nr(), pane_id(self.pane)));
                context.request_discard("damaged recovery closed");
                context.request_repaint();
            }
            Ok(()) => {
                self.damaged.pending = Some(ticket);
                self.damaged.error = None;
            }
            Err(error) => self.damaged.error = Some(error),
        }
    }
}
