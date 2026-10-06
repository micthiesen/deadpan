//! The Storage panel: how much space this project and Deadpan's per-user
//! caches use, what nothing references any more, explicit cleanup, and
//! portable copies (specification Sections 19.2 and 20.1, DP-19).
//!
//! `:storage` or Deadpan › Storage… opens it; File › Save Portable Copy… and
//! `:portable-copy` start a copy. The report is computed off the UI thread
//! from a read-only open of the package, so it never waits for the project
//! writer. P previews project cleanup on a read-only open off the writer;
//! R asks the project service, which owns the writer, to remove exactly the
//! previewed files that a fresh scan still finds unreferenced. Any project
//! change discards the preview. Per-user caches (C) are rebuildable.
//! Escape or Close closes it. Nothing here edits the project or its history.

use std::sync::mpsc;

use deadpan_cli::storage::{UserCleanupOutcome, UserStorage, UserStorageReport};
use deadpan_store::portable::PortableCopyReport;
use deadpan_store::storage::{CleanupOutcome, DEFAULT_GRACE, StorageReport};

use super::*;

/// One computed report.
pub(super) struct Snapshot {
    pub(super) project: Option<Result<StorageReport, String>>,
    pub(super) user: Option<UserStorageReport>,
}

/// The background preview's reply channel.
type PreviewReply = mpsc::Receiver<Result<CleanupOutcome, String>>;

#[derive(Default)]
pub(super) struct State {
    pub(super) open: bool,
    focus_pending: bool,
    return_focus: Option<(u64, Pane)>,
    loading: Option<mpsc::Receiver<Snapshot>>,
    pub(super) snapshot: Option<Snapshot>,
    ticket: u64,
    /// The service cleanup request awaiting its reply, and whether it was a
    /// dry run.
    pending: Option<u64>,
    /// A preview running on a read-only open, for its session and revision.
    previewing: Option<(u64, String, PreviewReply)>,
    /// The latest previewed cleanup and the session and revision it saw. R
    /// removes exactly these entries; any project change discards it.
    preview: Option<(u64, String, CleanupOutcome)>,
    caches: Option<mpsc::Receiver<Result<UserCleanupOutcome, String>>>,
    copy: Option<mpsc::Receiver<Result<PortableCopyReport, String>>>,
    pub(super) status: Option<String>,
    /// The per-user directories to account for; replay uses a private root
    /// so it never reports or cleans the person's real caches.
    pub(super) user: Option<UserStorage>,
}

impl State {
    fn user(&self) -> Option<UserStorage> {
        self.user.clone().or_else(UserStorage::current)
    }

    pub(super) fn copying(&self) -> bool {
        self.copy.is_some()
    }
}

pub(super) fn bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Rows of the project section: label, value.
pub(super) fn project_rows(report: &StorageReport) -> Vec<(String, String)> {
    let mut rows = vec![("Database".to_owned(), bytes(report.database_bytes))];
    for namespace in &report.namespaces {
        let label = match namespace.namespace {
            "originals" => "Originals",
            "generated" => "AI pause media",
            "render_candidates" => "Render candidates",
            other => other,
        };
        let mut value = format!("{} referenced", bytes(namespace.referenced_bytes));
        if namespace.unreferenced_bytes > 0 {
            value.push_str(&format!(
                " · {} unreferenced",
                bytes(namespace.unreferenced_bytes)
            ));
        }
        if namespace.pending_bytes > 0 {
            value.push_str(&format!(
                " · {} unfinished writes",
                bytes(namespace.pending_bytes)
            ));
        }
        if namespace.other_bytes > 0 {
            value.push_str(&format!(" · {} kept aside", bytes(namespace.other_bytes)));
        }
        rows.push((label.to_owned(), value));
    }
    rows.push((
        "Checkpoints and reports".to_owned(),
        bytes(report.auxiliary_bytes),
    ));
    rows.push(("Total".to_owned(), bytes(report.total_bytes)));
    rows.push((
        "Removable now".to_owned(),
        format!(
            "{} (unreferenced and unchanged for {} hours)",
            bytes(report.removable_bytes),
            report.grace_seconds / 3600
        ),
    ));
    rows
}

fn summary(outcome: &CleanupOutcome) -> String {
    let mut text = format!(
        "{} {} files ({})",
        if outcome.dry_run {
            "Cleanup would remove"
        } else {
            "Removed"
        },
        outcome.removed.len(),
        bytes(outcome.removed_bytes)
    );
    if !outcome.in_use.is_empty() {
        text.push_str(&format!("; {} in use were kept", outcome.in_use.len()));
    }
    if !outcome.changed.is_empty() {
        text.push_str(&format!(
            "; {} changed meanwhile were kept",
            outcome.changed.len()
        ));
    }
    text
}

impl DeadpanApp {
    pub(super) fn open_storage(&mut self, context: &egui::Context) {
        self.bindings.clear();
        self.storage.open = true;
        self.storage.focus_pending = true;
        self.storage.preview = None;
        self.refresh_storage();
        context.request_repaint();
    }

    fn close_storage(&mut self, context: &egui::Context) {
        self.storage.open = false;
        self.storage.focus_pending = false;
        self.storage.return_focus = Some((context.cumulative_frame_nr(), self.pane));
        context.request_discard("storage closed");
        context.request_repaint();
    }

    /// Recompute both reports on a background thread.
    fn refresh_storage(&mut self) {
        let package = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.path.clone());
        let user = self.storage.user();
        let (sender, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("deadpan-storage-report".into())
            .spawn(move || {
                let project = package.map(|package| {
                    deadpan_store::ProjectStore::open(&package, deadpan_store::AccessMode::ReadOnly)
                        .and_then(|store| store.storage_report(DEFAULT_GRACE))
                        .map_err(|error| error.to_string())
                });
                let user = user.map(|user| user.report(DEFAULT_GRACE));
                let _ = sender.send(Snapshot { project, user });
            });
        match spawned {
            Ok(_) => self.storage.loading = Some(receiver),
            Err(error) => self.storage.status = Some(format!("Storage report failed: {error}")),
        }
    }

    /// Poll background work. Call once per outer frame.
    pub(super) fn reconcile_storage(&mut self, context: &egui::Context) {
        // A preview describes one session and revision; any change, such as
        // an edit, Undo or another project, discards it.
        let current = self.current_context();
        if self
            .storage
            .preview
            .as_ref()
            .is_some_and(|(session, revision, _)| {
                current.as_ref() != Some(&(*session, revision.clone()))
            })
        {
            self.storage.preview = None;
            if self.storage.open {
                self.storage.status =
                    Some("The project changed; preview the cleanup again with P.".into());
            }
        }
        if let Some((session, revision, receiver)) = &self.storage.previewing {
            match receiver.try_recv() {
                Ok(result) => {
                    let (session, revision) = (*session, revision.clone());
                    self.storage.previewing = None;
                    match result {
                        Ok(outcome) if current.as_ref() == Some(&(session, revision.clone())) => {
                            self.storage.status = Some(if outcome.removed.is_empty() {
                                "Nothing is removable now.".to_owned()
                            } else {
                                format!(
                                    "{}. R removes exactly these if they are still unreferenced.",
                                    summary(&outcome)
                                )
                            });
                            self.storage.preview = Some((session, revision, outcome));
                        }
                        Ok(_) => {
                            self.storage.status = Some(
                                "The project changed during the preview; press P again.".into(),
                            );
                        }
                        Err(error) => self.storage.status = Some(error),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    context.request_repaint_after(std::time::Duration::from_millis(100))
                }
                Err(mpsc::TryRecvError::Disconnected) => self.storage.previewing = None,
            }
        }
        if let Some(receiver) = &self.storage.loading {
            match receiver.try_recv() {
                Ok(snapshot) => {
                    self.storage.snapshot = Some(snapshot);
                    self.storage.loading = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    context.request_repaint_after(std::time::Duration::from_millis(100))
                }
                Err(mpsc::TryRecvError::Disconnected) => self.storage.loading = None,
            }
        }
        if let Some(receiver) = &self.storage.caches {
            match receiver.try_recv() {
                Ok(result) => {
                    self.storage.caches = None;
                    self.storage.status = Some(match result {
                        Ok(outcome) => format!(
                            "Removed {} cache entries ({}){}",
                            outcome.removed.len(),
                            bytes(outcome.removed_bytes),
                            if outcome.kept_in_use.is_empty() {
                                String::new()
                            } else {
                                format!("; {} in use were kept", outcome.kept_in_use.len())
                            }
                        ),
                        Err(error) => format!("Cache cleanup failed: {error}"),
                    });
                    self.refresh_storage();
                }
                Err(mpsc::TryRecvError::Empty) => {
                    context.request_repaint_after(std::time::Duration::from_millis(100))
                }
                Err(mpsc::TryRecvError::Disconnected) => self.storage.caches = None,
            }
        }
        if let Some(receiver) = &self.storage.copy {
            match receiver.try_recv() {
                Ok(result) => {
                    self.storage.copy = None;
                    let text = match result {
                        Ok(report) => format!(
                            "Saved a portable copy at {} ({}, verified)",
                            report.destination.display(),
                            bytes(report.total_bytes)
                        ),
                        Err(error) => format!("The portable copy failed: {error}"),
                    };
                    self.message = Some(text.clone());
                    self.storage.status = Some(text);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    context.request_repaint_after(std::time::Duration::from_millis(200))
                }
                Err(mpsc::TryRecvError::Disconnected) => self.storage.copy = None,
            }
        }
    }

    /// Admit the project service's reply to this panel's removal request.
    pub(super) fn receive_storage_cleanup(
        &mut self,
        status: Option<crate::project::StorageCleanupStatus>,
    ) {
        let Some(status) = status else { return };
        if self.storage.pending != Some(status.ticket) {
            return;
        }
        self.storage.pending = None;
        self.storage.preview = None;
        match status.result {
            Ok(outcome) => {
                self.storage.status = Some(summary(&outcome));
                // A reply from an earlier project leaves the new one's report.
                if self.current_context().map(|(session, _)| session) == Some(status.session) {
                    self.refresh_storage();
                }
            }
            Err(error) => self.storage.status = Some(error),
        }
    }

    fn current_context(&self) -> Option<(u64, String)> {
        self.workspace.as_ref().map(|workspace| {
            (
                workspace.session,
                workspace.document.revision_id().as_str().to_owned(),
            )
        })
    }

    /// P: list what cleanup would remove, on a read-only open in a worker
    /// thread, never on the project writer.
    fn preview_storage_cleanup(&mut self) {
        let Some((session, revision)) = self.current_context() else {
            self.storage.status = Some("Open a project to clean its storage.".into());
            return;
        };
        if self.storage.previewing.is_some() || self.storage.pending.is_some() {
            return;
        }
        let Some(package) = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.path.clone())
        else {
            return;
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("deadpan-storage-preview".into())
            .spawn(move || {
                let _ = sender.send(
                    deadpan_store::ProjectStore::open(
                        &package,
                        deadpan_store::AccessMode::ReadOnly,
                    )
                    .and_then(|store| store.preview_storage_cleanup(DEFAULT_GRACE))
                    .map_err(|error| error.to_string()),
                );
            });
        match spawned {
            Ok(_) => {
                self.storage.preview = None;
                self.storage.previewing = Some((session, revision, receiver));
                self.storage.status = Some("Finding unreferenced files…".into());
            }
            Err(error) => self.storage.status = Some(error.to_string()),
        }
    }

    /// R: remove exactly the previewed entries, on the writer.
    fn confirm_storage_cleanup(&mut self) {
        let Some((session, revision)) = self.current_context() else {
            self.storage.status = Some("Open a project to clean its storage.".into());
            return;
        };
        if self.storage.pending.is_some() || self.storage.previewing.is_some() {
            return;
        }
        let previewed = match &self.storage.preview {
            Some((previewed_session, previewed_revision, outcome))
                if *previewed_session == session && *previewed_revision == revision =>
            {
                if outcome.removed.is_empty() {
                    self.storage.status = Some("Nothing is removable now.".into());
                    return;
                }
                outcome.removed.clone()
            }
            _ => {
                self.storage.status = Some("Preview the cleanup with P first.".into());
                return;
            }
        };
        self.storage.ticket += 1;
        let ticket = self.storage.ticket;
        match self
            .service
            .submit(crate::project::ProjectRequest::CleanStorage {
                ticket,
                expected_session: session,
                previewed,
            }) {
            Ok(()) => {
                self.storage.pending = Some(ticket);
                self.storage.status = Some("Removing the previewed files…".into());
            }
            Err(error) => self.storage.status = Some(error),
        }
    }

    fn clean_user_caches(&mut self) {
        if self.storage.caches.is_some() {
            return;
        }
        let Some(user) = self.storage.user() else {
            self.storage.status = Some("No home directory is available.".into());
            return;
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("deadpan-cache-cleanup".into())
            .spawn(move || {
                let _ = sender.send(
                    user.clean(DEFAULT_GRACE, false, &[])
                        .map_err(|error| error.to_string()),
                );
            });
        match spawned {
            Ok(_) => {
                self.storage.caches = Some(receiver);
                self.storage.status = Some("Cleaning caches…".into());
            }
            Err(error) => self.storage.status = Some(error.to_string()),
        }
    }

    /// File › Save Portable Copy…: choose where the copy goes.
    pub(super) fn start_portable_copy(&mut self, context: &egui::Context) {
        if self.workspace.is_none() {
            self.message = Some("Open a project to save a portable copy.".into());
            return;
        }
        if self.storage.copy.is_some() {
            self.message = Some("A portable copy is already being saved.".into());
            return;
        }
        if let Err(error) = self.dialogs.start(DialogKind::PortableCopy, context) {
            self.message = Some(error);
        }
    }

    /// Copy the open project to the chosen location on a background thread.
    /// The source is read through a read-only open, so editing continues.
    pub(super) fn receive_portable_copy_dialog(&mut self, path: PathBuf) {
        let Some(source) = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.path.clone())
        else {
            return;
        };
        let destination = if path
            .extension()
            .is_some_and(|extension| extension == "deadpan")
        {
            path
        } else {
            path.with_extension("deadpan")
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("deadpan-portable-copy".into())
            .spawn(move || {
                let _ = sender.send(
                    deadpan_store::portable::copy_portable(
                        &source,
                        &destination,
                        &std::sync::atomic::AtomicBool::new(false),
                    )
                    .map_err(|error| error.to_string()),
                );
            });
        match spawned {
            Ok(_) => {
                self.storage.copy = Some(receiver);
                self.message = Some("Saving a portable copy…".into());
                self.storage.status = Some("Saving a portable copy…".into());
            }
            Err(error) => self.message = Some(error.to_string()),
        }
    }

    /// The panel owns the keyboard while open: editor bindings never see
    /// keys, Tab and Space/Enter stay native, Escape closes it and P, R, C,
    /// S and U are its actions.
    pub(super) fn storage_keyboard(&mut self, context: &egui::Context) -> bool {
        if let Some((frame, pane)) = self.storage.return_focus
            && context.cumulative_frame_nr() > frame
        {
            context.memory_mut(|memory| memory.request_focus(pane_id(pane)));
            self.storage.return_focus = None;
        }
        if !self.storage.open {
            return false;
        }
        let composing = &mut self.ime_composing;
        context.input(|input| help_scroll::observe_composition(&input.events, composing));
        self.bindings.clear();
        if !self.ime_composing {
            let pressed =
                |key| context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key));
            if pressed(egui::Key::P) {
                self.preview_storage_cleanup();
            } else if pressed(egui::Key::R) {
                self.confirm_storage_cleanup();
            } else if pressed(egui::Key::C) {
                self.clean_user_caches();
            } else if pressed(egui::Key::S) {
                self.start_portable_copy(context);
            } else if pressed(egui::Key::U) {
                self.refresh_storage();
            }
        }
        true
    }

    pub(super) fn storage_window(&mut self, context: &egui::Context) {
        if !self.storage.open {
            return;
        }
        let content = context.content_rect();
        let width = (content.width() - 32.0).clamp(300.0, 440.0);
        let height = (content.height() - 270.0).max(160.0);
        let mut close = false;
        let mut action = None;
        let project = self.workspace.is_some();
        let previewed = self
            .storage
            .preview
            .as_ref()
            .is_some_and(|(_, _, outcome)| !outcome.removed.is_empty());
        let modal = egui::Modal::new(egui::Id::new("storage-window"))
            .backdrop_color(egui::Color32::TRANSPARENT)
            .area(
                egui::Modal::default_area(egui::Id::new("storage-window"))
                    .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 56.0)),
            )
            .show(context, |ui| {
                accessibility::dialog(ui, "Storage");
                ui.set_width(width);
                ui.label(style::section_title("STORAGE", true));
                ui.label(
                    egui::RichText::new(
                        "What this project and Deadpan's caches use. P lists files no retained revision, register, checkpoint or offered AI variant references and that have been unchanged for a day; R removes exactly those.",
                    )
                    .size(11.5)
                    .weak(),
                );
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("storage-rows")
                    .max_height(height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let Some(snapshot) = &self.storage.snapshot else {
                            ui.label(egui::RichText::new("Measuring…").weak());
                            return;
                        };
                        let section = |ui: &mut egui::Ui, title: &str, rows: Vec<(String, String)>| {
                            ui.label(style::section_title(title, false));
                            egui::Grid::new(("storage-section", title.to_owned()))
                                .num_columns(2)
                                .min_col_width(120.0)
                                .spacing(egui::vec2(10.0, 3.0))
                                .min_row_height(16.0)
                                .show(ui, |ui| {
                                    for (label, value) in &rows {
                                        ui.label(egui::RichText::new(label).size(12.0).weak());
                                        let response = ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(value).monospace().size(11.5),
                                            )
                                            .wrap(),
                                        );
                                        accessibility::full_text(response, &format!("{label}: {value}"));
                                        ui.end_row();
                                    }
                                });
                            ui.add_space(6.0);
                        };
                        match &snapshot.project {
                            Some(Ok(report)) => section(ui, "THIS PROJECT", project_rows(report)),
                            Some(Err(error)) => section(
                                ui,
                                "THIS PROJECT",
                                vec![("Unavailable".into(), error.clone())],
                            ),
                            None => section(
                                ui,
                                "THIS PROJECT",
                                vec![("No project".into(), "Open one to see its storage".into())],
                            ),
                        }
                        if let Some(user) = &snapshot.user {
                            let mut rows: Vec<(String, String)> = user
                                .directories
                                .iter()
                                .map(|directory| {
                                    let mut value = format!("{} · {}", bytes(directory.bytes), directory.kind);
                                    if directory.removable_bytes > 0 {
                                        value.push_str(&format!(
                                            " · {} removable",
                                            bytes(directory.removable_bytes)
                                        ));
                                    }
                                    (directory.name.clone(), value)
                                })
                                .collect();
                            rows.push((
                                "Not on disk".into(),
                                "decoded audio, pictures and thumbnails stay in memory".into(),
                            ));
                            section(ui, "DEADPAN CACHES", rows);
                        }
                    });
                if let Some(status) = &self.storage.status {
                    let response = ui.add(egui::Label::new(egui::RichText::new(status).size(12.0)).wrap());
                    accessibility::full_text(response, status);
                }
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    let preview = ui.add_enabled(project, style::action("Preview cleanup", "P"));
                    if std::mem::take(&mut self.storage.focus_pending) {
                        preview.request_focus();
                    }
                    if preview.clicked() {
                        action = Some('p');
                    }
                    if ui.add_enabled(previewed, style::action("Remove", "R")).clicked() {
                        action = Some('r');
                    }
                    if ui.add(style::action("Clean caches", "C")).clicked() {
                        action = Some('c');
                    }
                    if ui
                        .add_enabled(project && !self.storage.copying(), style::action("Save portable copy…", "S"))
                        .clicked()
                    {
                        action = Some('s');
                    }
                    if ui.add(style::action("Close", "Esc")).clicked() {
                        close = true;
                    }
                });
            });
        match action {
            Some('p') => self.preview_storage_cleanup(),
            Some('r') => self.confirm_storage_cleanup(),
            Some('c') => self.clean_user_caches(),
            Some('s') => self.start_portable_copy(context),
            _ => {}
        }
        close |= modal.should_close() && !self.ime_composing;
        if close {
            self.close_storage(context);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_cleanup_summaries_read_plainly() {
        assert_eq!(bytes(900), "900 B");
        assert_eq!(bytes(5 * 1024 * 1024), "5.0 MiB");
        let mut outcome = CleanupOutcome {
            dry_run: true,
            ..CleanupOutcome::default()
        };
        assert_eq!(summary(&outcome), "Cleanup would remove 0 files (0 B)");
        outcome.dry_run = false;
        outcome.removed_bytes = 2048;
        outcome
            .removed
            .push(deadpan_store::storage::RemovedEntry::for_test(
                "generated",
                "blake3-x",
                2048,
            ));
        outcome
            .in_use
            .push(deadpan_store::storage::RemovedEntry::for_test(
                "generated",
                "blake3-y",
                1,
            ));
        assert_eq!(
            summary(&outcome),
            "Removed 1 files (2.0 KiB); 1 in use were kept"
        );
    }
}
