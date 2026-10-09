//! The Storage panel's BACKUPS section: the project's verified backups,
//! what each one holds, backing up now and restoring one
//! (docs/BACKUPS.md).
//!
//! `:backups` opens the Storage panel on this section. J/K or the arrows
//! choose a backup; its revision, beats, length and edit count are read from
//! the backup on a background thread. B backs up now. O asks to restore the
//! chosen backup and a second O restores it: the project service backs up
//! the current state first, replaces the database and opens the result as
//! a new session. Listing and previews never use the project writer.

use std::sync::mpsc;

use deadpan_store::backups::{BackupInfo, BackupPreview, list_backups, preview_backup};

use super::*;
use crate::project::backups::{Request, SettingsStatus, SettingsUpdate, Update};

type Listing = mpsc::Receiver<Result<Vec<BackupInfo>, String>>;
type Previewing = mpsc::Receiver<Result<BackupPreview, String>>;

#[derive(Default)]
pub(super) struct View {
    list: Option<Result<Vec<BackupInfo>, String>>,
    loading: Option<Listing>,
    selected: usize,
    /// The chosen backup's contents, by id.
    preview: Option<(String, Result<BackupPreview, String>)>,
    previewing: Option<(String, Previewing)>,
    /// The first O: restore this backup of this session on the next O.
    confirm: Option<(u64, String)>,
    ticket: u64,
    pending: Option<u64>,
    /// The service's backup state of the current session.
    pub(super) update: Update,
    #[cfg(feature = "ui-harness")]
    pub(super) owned_workers_active_for_check: bool,
    pub(super) status: Option<String>,
    /// Opened by `:backups`: the panel shows only this section.
    pub(super) only: bool,
    /// The session the state above belongs to.
    session: Option<u64>,
    /// Current global settings, independent of the project session.
    pub(super) settings: SettingsUpdate,
    settings_ticket: u64,
    settings_expected: Option<u64>,
    pub(super) settings_draft: Option<SettingsDraft>,
    settings_error: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct SettingsDraft {
    interval: String,
    count: String,
    budget: String,
}

const SETTINGS_INTERVAL_ID: &str = "backup-settings-interval";
const SETTINGS_COUNT_ID: &str = "backup-settings-count";
const SETTINGS_BUDGET_ID: &str = "backup-settings-budget";

pub(super) fn settings_field_focused(context: &egui::Context) -> bool {
    [SETTINGS_INTERVAL_ID, SETTINGS_COUNT_ID, SETTINGS_BUDGET_ID]
        .into_iter()
        .any(|id| context.memory(|memory| memory.has_focus(egui::Id::new(id))))
}

#[cfg(feature = "ui-harness")]
impl View {
    pub(super) fn listed(&self) -> bool {
        self.list.is_some() && self.loading.is_none()
    }

    pub(super) fn count(&self) -> usize {
        match &self.list {
            Some(Ok(list)) => list.len(),
            _ => 0,
        }
    }

    /// The chosen backup's contents have been read.
    pub(super) fn previewed(&self) -> bool {
        match (&self.list, &self.preview) {
            (Some(Ok(list)), Some((id, Ok(_)))) => {
                list.get(self.selected).is_some_and(|info| info.id == *id)
            }
            _ => false,
        }
    }
}

/// Seconds as `m:ss.s` or `h:mm:ss`.
fn length(frames: i64, rate: deadpan_core::FrameRate) -> String {
    let seconds = frames as f64 * f64::from(rate.denominator()) / f64::from(rate.numerator());
    let whole = seconds as u64;
    if whole >= 3600 {
        format!("{}:{:02}:{:02}", whole / 3600, whole / 60 % 60, whole % 60)
    } else {
        format!("{}:{:04.1}", whole / 60, seconds - (whole / 60 * 60) as f64)
    }
}

/// One line describing what a backup holds.
pub(super) fn describe(preview: &BackupPreview) -> String {
    let count = |count: u64, one: &str, many: &str| {
        format!("{count} {}", if count == 1 { one } else { many })
    };
    format!(
        "Revision {} · {} · {} long · {}",
        preview.revision_id.as_str(),
        count(preview.beats as u64, "beat", "beats"),
        length(preview.duration_frames, preview.frame_rate),
        count(preview.edits, "edit", "edits")
    )
}

fn row(info: &BackupInfo) -> String {
    format!(
        "{} · {} · {}",
        crate::project::backups::age(info.created_unix_ms),
        info.reason.label(),
        storage::bytes(info.database_bytes)
    )
}

impl DeadpanApp {
    /// Refresh the per-user settings off the UI thread through the bounded
    /// project-service worker. The ticket admits only this request's reply.
    pub(super) fn reload_backup_settings(&mut self) {
        let view = &mut self.storage.backups;
        view.settings_ticket = view.settings_ticket.saturating_add(1);
        let ticket = view.settings_ticket;
        view.settings_expected = Some(ticket);
        view.settings_draft = None;
        view.settings_error = None;
        match self.service.submit(crate::project::ProjectRequest::Backup(
            Request::LoadSettings { ticket },
        )) {
            Ok(()) => view.settings.status = SettingsStatus::Loading,
            Err(error) => {
                view.settings_expected = None;
                view.settings_error = Some(error);
            }
        }
    }

    /// List backups again, off the UI thread.
    pub(super) fn refresh_backups(&mut self) {
        let Some(package) = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.path.clone())
        else {
            self.storage.backups.list = None;
            return;
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("deadpan-backup-list".into())
            .spawn(move || {
                let _ = sender.send(list_backups(&package).map_err(|error| error.to_string()));
            });
        match spawned {
            Ok(_) => self.storage.backups.loading = Some(receiver),
            Err(error) => self.storage.backups.status = Some(error.to_string()),
        }
    }

    /// `:backups`: the Storage panel, on its backups.
    pub(super) fn open_backups(&mut self, context: &egui::Context) {
        self.open_storage(context);
        self.storage.backups.only = true;
        self.storage.backups.status =
            Some("J/K choose a backup, B backs up now, O restores the chosen one.".into());
    }

    /// Admit the service's backup state for the current session.
    pub(super) fn receive_backups(&mut self, update: Update) {
        let session = self.workspace.as_ref().map(|workspace| workspace.session);
        let view = &mut self.storage.backups;
        #[cfg(feature = "ui-harness")]
        {
            view.owned_workers_active_for_check = update.owned_workers_active_for_check;
        }
        let settings_ticket = update.settings.ticket;
        let settings_is_current = view
            .settings_expected
            .is_some_and(|expected| expected == settings_ticket)
            || (view.settings_expected.is_none() && settings_ticket >= view.settings.ticket);
        if settings_is_current {
            view.settings = update.settings.clone();
            view.settings_ticket = settings_ticket;
            if view.settings_expected == Some(settings_ticket) {
                match &view.settings.status {
                    SettingsStatus::Ready { .. } | SettingsStatus::Saved { .. } => {
                        view.settings_expected = None;
                        view.settings_draft = None;
                        view.settings_error = None;
                    }
                    SettingsStatus::Failed(_) => {
                        view.settings_expected = None;
                    }
                    SettingsStatus::Loading | SettingsStatus::Saving => {}
                }
            }
        }
        let changed = view.session != session;
        view.session = session;
        if changed {
            // A restore's reply arrives with its new session; anything else
            // pending belonged to a session that is gone.
            view.confirm = None;
            if update.reply.as_ref().map(|reply| reply.ticket) != view.pending {
                view.pending = None;
            }
        }
        if Some(update.session) != session {
            return;
        }
        let published = update.latest.as_ref().map(|(info, _)| info.id.clone())
            != view.update.latest.as_ref().map(|(info, _)| info.id.clone());
        if let Some(reply) = &update.reply
            && view.pending == Some(reply.ticket)
        {
            view.pending = None;
            view.confirm = None;
            view.status = Some(match &reply.result {
                Ok(message) => message.clone(),
                Err(error) => error.clone(),
            });
        }
        view.update = update;
        if published && self.storage.open {
            self.refresh_backups();
        }
    }

    pub(super) fn reconcile_backups(&mut self, context: &egui::Context) {
        let view = &mut self.storage.backups;
        if view.confirm.as_ref().is_some_and(|(session, _)| {
            Some(*session) != self.workspace.as_ref().map(|w| w.session)
        }) {
            view.confirm = None;
        }
        if let Some(receiver) = &view.loading {
            match receiver.try_recv() {
                Ok(list) => {
                    view.loading = None;
                    if let Ok(list) = &list {
                        view.selected = view.selected.min(list.len().saturating_sub(1));
                    }
                    view.list = Some(list);
                    self.preview_selected_backup();
                }
                Err(mpsc::TryRecvError::Empty) => {
                    context.request_repaint_after(std::time::Duration::from_millis(100));
                }
                Err(mpsc::TryRecvError::Disconnected) => view.loading = None,
            }
        }
        let view = &mut self.storage.backups;
        if let Some((id, receiver)) = &view.previewing {
            match receiver.try_recv() {
                Ok(preview) => {
                    let id = id.clone();
                    view.previewing = None;
                    view.preview = Some((id, preview));
                }
                Err(mpsc::TryRecvError::Empty) => {
                    context.request_repaint_after(std::time::Duration::from_millis(100));
                }
                Err(mpsc::TryRecvError::Disconnected) => view.previewing = None,
            }
        }
    }

    fn selected_backup(&self) -> Option<&BackupInfo> {
        match &self.storage.backups.list {
            Some(Ok(list)) => list.get(self.storage.backups.selected),
            _ => None,
        }
    }

    /// Read what the chosen backup holds, unless already read or reading.
    fn preview_selected_backup(&mut self) {
        let Some(info) = self.selected_backup().cloned() else {
            return;
        };
        let view = &mut self.storage.backups;
        if view.preview.as_ref().is_some_and(|(id, _)| *id == info.id)
            || view
                .previewing
                .as_ref()
                .is_some_and(|(id, _)| *id == info.id)
        {
            return;
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let id = info.id.clone();
        let spawned = std::thread::Builder::new()
            .name("deadpan-backup-preview".into())
            .spawn(move || {
                let _ = sender.send(preview_backup(&info).map_err(|error| error.to_string()));
            });
        if spawned.is_ok() {
            view.previewing = Some((id, receiver));
        }
    }

    pub(super) fn move_backup_selection(&mut self, forward: bool) {
        let count = match &self.storage.backups.list {
            Some(Ok(list)) => list.len(),
            _ => 0,
        };
        if count == 0 {
            return;
        }
        let view = &mut self.storage.backups;
        view.selected = if forward {
            (view.selected + 1).min(count - 1)
        } else {
            view.selected.saturating_sub(1)
        };
        if view.confirm.take().is_some() {
            view.status = Some("Restore cancelled.".into());
        }
        self.preview_selected_backup();
    }

    /// B: back up now, on the service's backup thread.
    pub(super) fn back_up_now(&mut self) {
        let Some(session) = self.workspace.as_ref().map(|workspace| workspace.session) else {
            self.storage.backups.status = Some("Open a project to back it up.".into());
            return;
        };
        if let Some(reason) = self.workspace.as_ref().and_then(|w| w.read_only.clone()) {
            self.storage.backups.status = Some(format!("Not backed up: {reason}"));
            return;
        }
        if self.storage.backups.pending.is_some() {
            return;
        }
        self.storage.backups.ticket += 1;
        let ticket = self.storage.backups.ticket;
        match self
            .service
            .submit(crate::project::ProjectRequest::Backup(Request::Now {
                ticket,
                expected_session: session,
            })) {
            Ok(()) => {
                self.storage.backups.pending = Some(ticket);
                self.storage.backups.status = Some("Backing up…".into());
            }
            Err(error) => self.storage.backups.status = Some(error),
        }
    }

    /// O: the first press asks, the second restores the same backup.
    pub(super) fn restore_selected_backup(&mut self) {
        let Some(session) = self.workspace.as_ref().map(|workspace| workspace.session) else {
            self.storage.backups.status = Some("Open a project to restore a backup.".into());
            return;
        };
        if let Some(reason) = self.workspace.as_ref().and_then(|w| w.read_only.clone()) {
            self.storage.backups.status = Some(format!("Not restored: {reason}"));
            return;
        }
        if self.storage.backups.pending.is_some() {
            return;
        }
        let Some(info) = self.selected_backup().cloned() else {
            self.storage.backups.status = Some("There is no backup to restore yet.".into());
            return;
        };
        let confirmed = self.storage.backups.confirm.as_ref() == Some(&(session, info.id.clone()));
        if !confirmed {
            let contents = match &self.storage.backups.preview {
                Some((id, Ok(preview))) if *id == info.id => describe(preview),
                _ => "its contents are being read".into(),
            };
            self.storage.backups.confirm = Some((session, info.id.clone()));
            self.storage.backups.status = Some(format!(
                "Restore the backup from {} ({contents})? It replaces the project, history included; what you have now is backed up first. Press O again to restore, or J/K to cancel.",
                crate::project::backups::age(info.created_unix_ms)
            ));
            return;
        }
        self.storage.backups.ticket += 1;
        let ticket = self.storage.backups.ticket;
        match self
            .service
            .submit(crate::project::ProjectRequest::Backup(Request::Restore {
                ticket,
                expected_session: session,
                id: info.id,
            })) {
            Ok(()) => {
                self.storage.backups.pending = Some(ticket);
                self.storage.backups.status =
                    Some("Backing up the current state, then restoring…".into());
            }
            Err(error) => self.storage.backups.status = Some(error),
        }
    }

    pub(super) fn begin_backup_settings_edit(&mut self) {
        let settings = &self.storage.backups.settings.settings;
        self.storage.backups.settings_draft = Some(SettingsDraft {
            interval: settings.interval_minutes().to_string(),
            count: settings.max_count().to_string(),
            budget: settings.budget_mib().to_string(),
        });
        self.storage.backups.settings_error = None;
    }

    pub(super) fn cancel_backup_settings_edit(&mut self) {
        self.storage.backups.settings_draft = None;
        self.storage.backups.settings_error = None;
    }

    pub(super) fn save_backup_settings(&mut self) {
        let Some(draft) = self.storage.backups.settings_draft.as_ref() else {
            return;
        };
        let parsed = (|| {
            let interval = draft
                .interval
                .parse::<u32>()
                .map_err(|_| "Enter a whole number of minutes from 1 to 1440.".to_owned())?;
            let count = draft
                .count
                .parse::<u32>()
                .map_err(|_| "Enter a whole number of backups from 8 to 256.".to_owned())?;
            let budget = draft
                .budget
                .parse::<u32>()
                .map_err(|_| "Enter a whole number of MiB from 256 to 65536.".to_owned())?;
            deadpan_cli::backup_settings::Settings::new(interval, count, budget)
                .map_err(|error| error.to_string())
        })();
        let settings = match parsed {
            Ok(settings) => settings,
            Err(error) => {
                self.storage.backups.settings_error = Some(error);
                return;
            }
        };
        let view = &mut self.storage.backups;
        view.settings_ticket = view.settings_ticket.saturating_add(1);
        let ticket = view.settings_ticket;
        view.settings_expected = Some(ticket);
        view.settings.status = SettingsStatus::Saving;
        view.settings_error = None;
        match self.service.submit(crate::project::ProjectRequest::Backup(
            Request::SaveSettings { ticket, settings },
        )) {
            Ok(()) => {}
            Err(error) => {
                view.settings_expected = None;
                view.settings.status = SettingsStatus::Failed(error.clone());
                view.settings_error = Some(error);
            }
        }
    }

    /// The BACKUPS section of the Storage panel. Returns a clicked action.
    pub(super) fn backups_section(&mut self, ui: &mut egui::Ui, focus: bool) -> Option<char> {
        let mut action = None;
        ui.label(style::section_title("BACKUPS", false));
        let read_only = self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.read_only.is_some());
        let (list, selected, preview, backup_update, status, settings) = {
            let view = &self.storage.backups;
            (
                view.list.clone(),
                view.selected,
                view.preview.clone(),
                view.update.clone(),
                view.status.clone(),
                view.settings.clone(),
            )
        };
        if read_only {
            ui.label(
                egui::RichText::new("This project is open read-only; it is not backed up here.")
                    .size(11.5)
                    .weak(),
            );
        }
        let line = |ui: &mut egui::Ui, text: String, selected: bool| {
            let rich = egui::RichText::new(&text).monospace().size(11.5);
            let response = ui.add(
                egui::Label::new(if selected {
                    rich.background_color(style::SELECTED).strong()
                } else {
                    rich
                })
                .wrap(),
            );
            accessibility::full_text(
                response,
                &if selected {
                    format!("Chosen backup: {text}")
                } else {
                    text.clone()
                },
            );
        };
        match &list {
            None => line(ui, "Reading backups…".into(), false),
            Some(Err(error)) => line(ui, format!("Backups unavailable: {error}"), false),
            Some(Ok(list)) if list.is_empty() => line(
                ui,
                "None yet. Deadpan backs up while you edit, when you close, and before a restore."
                    .into(),
                false,
            ),
            Some(Ok(list)) => {
                for (index, info) in list.iter().enumerate() {
                    line(ui, row(info), index == selected);
                }
                let contents = match (&preview, list.get(selected)) {
                    (Some((id, Ok(preview))), Some(info)) if *id == info.id => describe(preview),
                    (Some((id, Err(error))), Some(info)) if *id == info.id => {
                        format!("Cannot be read: {error}")
                    }
                    _ => "Reading what it holds…".into(),
                };
                ui.label(egui::RichText::new(contents).size(11.5).weak());
            }
        }
        if let Some(reason) = backup_update.running {
            ui.label(
                egui::RichText::new(format!("Backing up now ({})…", reason.label())).size(11.5),
            );
        }
        if let Some(failure) = &backup_update.failure {
            ui.colored_label(
                style::WARNING,
                format!("The last automatic backup failed: {failure}"),
            );
        }
        ui.separator();
        ui.label(style::section_title("AUTOMATIC BACKUPS", false));
        match &settings.status {
            SettingsStatus::Loading => {
                ui.label(egui::RichText::new("Reading backup settings…").weak());
            }
            SettingsStatus::Ready { source, warning } => {
                let origin = match source {
                    deadpan_cli::backup_settings::Source::Default => "Defaults are in use.",
                    deadpan_cli::backup_settings::Source::File => "Saved per-user settings.",
                };
                ui.label(egui::RichText::new(origin).weak());
                if let Some(warning) = warning {
                    ui.colored_label(style::WARNING, warning);
                }
            }
            SettingsStatus::Saving => {
                ui.label(egui::RichText::new("Saving backup settings…").weak());
            }
            SettingsStatus::Saved { warning } => {
                ui.label(egui::RichText::new("Backup settings saved.").weak());
                if let Some(warning) = warning {
                    ui.colored_label(style::WARNING, warning);
                }
            }
            SettingsStatus::Failed(error) => {
                ui.colored_label(style::WARNING, format!("Backup settings: {error}"));
                if !settings.trusted {
                    ui.colored_label(
                        style::WARNING,
                        "Using the default interval and keeping every backup until settings are repaired.",
                    );
                }
            }
        }
        if let Some(draft) = &mut self.storage.backups.settings_draft {
            egui::Grid::new("backup-settings-grid")
                .num_columns(2)
                .min_col_width(112.0)
                .spacing(egui::vec2(8.0, 4.0))
                .show(ui, |ui| {
                    ui.label("Interval (minutes)");
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut draft.interval)
                            .id(egui::Id::new(SETTINGS_INTERVAL_ID))
                            .return_key(None)
                            .desired_width(80.0),
                    );
                    accessibility::name(&field, "Automatic backup interval in minutes");
                    ui.end_row();
                    ui.label("Backups kept");
                    let count = ui.add(
                        egui::TextEdit::singleline(&mut draft.count)
                            .id(egui::Id::new(SETTINGS_COUNT_ID))
                            .return_key(None)
                            .desired_width(80.0),
                    );
                    accessibility::name(&count, "Maximum backup count");
                    ui.end_row();
                    ui.label("Storage budget (MiB)");
                    let budget = ui.add(
                        egui::TextEdit::singleline(&mut draft.budget)
                            .id(egui::Id::new(SETTINGS_BUDGET_ID))
                            .return_key(None)
                            .desired_width(100.0),
                    );
                    accessibility::name(&budget, "Maximum backup storage in MiB");
                    ui.end_row();
                });
            ui.label(
                egui::RichText::new("Limits: 1–1440 minutes, 8–256 backups, 256–65536 MiB.")
                    .size(11.0)
                    .weak(),
            );
            if let Some(error) = &self.storage.backups.settings_error {
                let response = ui.colored_label(style::WARNING, error);
                accessibility::full_text(response, error);
            }
            let saving = matches!(
                &self.storage.backups.settings.status,
                SettingsStatus::Saving
            ) || self.storage.backups.settings_expected.is_some();
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(!saving, egui::Button::new("Save settings"))
                    .clicked()
                {
                    action = Some('w');
                }
                if ui
                    .add_enabled(!saving, egui::Button::new("Cancel settings"))
                    .clicked()
                {
                    action = Some('q');
                }
            });
        } else {
            let summary = format!(
                "Every {} min · up to {} backups · {} MiB",
                settings.settings.interval_minutes(),
                settings.settings.max_count(),
                settings.settings.budget_mib()
            );
            ui.label(egui::RichText::new(summary).monospace().size(11.5));
            let loading = matches!(&settings.status, SettingsStatus::Loading);
            if ui
                .add_enabled(!loading, egui::Button::new("Change backup settings…"))
                .clicked()
            {
                action = Some('i');
            }
        }
        ui.horizontal_wrapped(|ui| {
            let project = self.workspace.is_some() && !read_only;
            let back_up = ui.add_enabled(project, style::action("Back up now", "B"));
            if focus {
                back_up.request_focus();
            }
            if back_up.clicked() {
                action = Some('b');
            }
            let any = matches!(&list, Some(Ok(list)) if !list.is_empty());
            if ui
                .add_enabled(project && any, style::action("Restore…", "O"))
                .clicked()
            {
                action = Some('o');
            }
        });
        if let Some(status) = &status {
            let response = ui.add(egui::Label::new(egui::RichText::new(status).size(12.0)).wrap());
            accessibility::full_text(response, status);
        }
        ui.add_space(6.0);
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_read_as_clock_times() {
        let rate = deadpan_core::FrameRate::new(30, 1).unwrap();
        assert_eq!(length(0, rate), "0:00.0");
        assert_eq!(length(45, rate), "0:01.5");
        assert_eq!(length(30 * 75, rate), "1:15.0");
        assert_eq!(length(30 * 3725, rate), "1:02:05");
    }
}
