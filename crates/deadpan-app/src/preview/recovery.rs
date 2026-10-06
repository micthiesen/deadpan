//! Recovery dialogs: reopening after an unclean exit, what opening a project
//! recovered, locating a missing Original, the persistent "Not saved" alert
//! and closing with unsaved previews. Every dialog is a native modal whose
//! buttons take Tab focus; Enter activates the focused button and Escape
//! takes the safe choice. None of them edits the project by itself.

use std::sync::Arc;

use super::*;
use crate::project::{OpenReport, RelinkState, RelinkStatus};
use crate::recovery::{LaunchJournal, LaunchOffer, StorageAlert};
use deadpan_store::original_media::{OriginalAvailability, OriginalContentId};

#[derive(Default)]
pub(super) struct RecoveryUi {
    journal: Option<LaunchJournal>,
    /// The project the previous launch left open, until answered.
    pub(super) launch: Option<LaunchOffer>,
    report: Option<Arc<OpenReport>>,
    /// The session whose report is showing; answered reports stay closed.
    showing: Option<u64>,
    answered: Option<u64>,
    pub(super) storage: Option<StorageAlert>,
    relink: Option<RelinkCapture>,
    relink_status: Option<RelinkStatus>,
    /// Open previews that a requested window close would discard.
    pub(super) close_prompt: Option<Vec<&'static str>>,
    close_confirmed: bool,
    /// Sessions whose retained recovery findings were acknowledged.
    acknowledged: Vec<u64>,
    /// The session the journal currently records as open.
    journal_session: Option<u64>,
    focus_pending: bool,
    ticket: u64,
}

struct RelinkCapture {
    session: u64,
    content: OriginalContentId,
    version: u64,
    ticket: u64,
}

const LAUNCH_ID: &str = "recovery-launch";
const REPORT_ID: &str = "recovery-report";
const CLOSE_ID: &str = "recovery-close";

impl RecoveryUi {
    pub(super) fn blocking(&self) -> bool {
        self.launch.is_some() || self.showing.is_some() || self.close_prompt.is_some()
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn report(&self) -> Option<&Arc<OpenReport>> {
        self.report.as_ref()
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn showing_report(&self) -> bool {
        self.showing.is_some()
    }
}

impl DeadpanApp {
    /// Installs the launch journal and offers a project the previous launch
    /// left open. Replay and tests pass a private journal.
    pub fn use_launch_journal(&mut self, journal: LaunchJournal, offer: bool) {
        self.recovery.launch = journal.unclean_offer().filter(|_| offer);
        self.recovery.focus_pending = self.recovery.launch.is_some();
        self.recovery.journal = Some(journal);
    }

    /// A normal exit: the next launch offers nothing. Waits, bounded, for
    /// the project service to release the writer first; if it has not, the
    /// journal keeps this instance's open record so the next launch offers it.
    pub(super) fn record_clean_exit_after_drain(&mut self, limit: std::time::Duration) {
        let deadline = std::time::Instant::now() + limit;
        while !self.service.is_shutdown_complete() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if !self.service.is_shutdown_complete() {
            return;
        }
        if let Some(journal) = &self.recovery.journal
            && let Err(error) = journal.record_closed()
        {
            eprintln!("{error}");
        }
        self.recovery.journal_session = None;
    }

    pub(super) fn receive_recovery(
        &mut self,
        opened: Option<Arc<OpenReport>>,
        storage: Option<StorageAlert>,
        relink: Option<RelinkStatus>,
    ) {
        let session = self.workspace.as_ref().map(|workspace| workspace.session);
        if session != self.recovery.journal_session {
            if let Some(journal) = &self.recovery.journal {
                let recorded = match &self.workspace {
                    Some(workspace) => journal.record_open(&workspace.path),
                    None => journal.record_closed(),
                };
                if let Err(error) = recorded {
                    eprintln!("{error}");
                }
            }
            self.recovery.journal_session = session;
            self.recovery.showing = None;
            self.recovery.relink = None;
        }
        if let Some(report) = opened.filter(|report| Some(report.session) == session) {
            let new_session = self
                .recovery
                .report
                .as_ref()
                .is_none_or(|current| current.session != report.session);
            if new_session && report.needs_attention() && self.recovery.answered != session {
                self.recovery.showing = session;
                self.recovery.focus_pending = true;
            }
            self.recovery.report = Some(report);
        } else if session.is_none() {
            self.recovery.report = None;
        }
        self.recovery.storage = storage;
        if let Some(status) = relink
            && self.recovery.relink_status.as_ref() != Some(&status)
        {
            if Some(status.session) == session
                && status.state == RelinkState::Restored
                && self
                    .recovery
                    .report
                    .as_ref()
                    .is_some_and(|report| report.missing().next().is_none())
            {
                // The project is whole again; decode the current picture.
                self.request_picture(true);
                self.thumbnails.reset();
            }
            if self
                .recovery
                .relink
                .as_ref()
                .is_some_and(|capture| capture.ticket == status.ticket)
                && status.state != RelinkState::Verifying
            {
                self.recovery.relink = None;
            }
            self.recovery.relink_status = Some(status);
        }
    }

    pub(super) fn show_recovery_report(&mut self) {
        match (&self.recovery.report, &self.workspace) {
            (Some(report), Some(workspace)) if report.session == workspace.session => {
                self.recovery.showing = Some(workspace.session);
                self.recovery.focus_pending = true;
            }
            _ => self.message = Some("Open a project to see what opening it recovered.".into()),
        }
    }

    /// `:relink` or the report's button: choose the file of the first
    /// missing original. The service verifies its exact content.
    pub(super) fn locate_original(&mut self, context: &egui::Context) {
        let Some(workspace) = &self.workspace else {
            self.error = Some("Open a project before locating its Original.".into());
            return;
        };
        // A missing original first; otherwise the Original itself, so a copy
        // that is present but damaged can be verified and repaired on demand.
        let target = self
            .recovery
            .report
            .as_ref()
            .filter(|report| report.session == workspace.session)
            .and_then(|report| {
                report
                    .missing()
                    .find(|status| status.primary)
                    .or_else(|| report.missing().next())
                    .or_else(|| report.originals.iter().find(|status| status.primary))
                    .cloned()
            });
        let Some(missing) = target else {
            self.message = Some("This project has no Original to locate.".into());
            return;
        };
        self.recovery.ticket += 1;
        self.recovery.relink = Some(RelinkCapture {
            session: workspace.session,
            content: missing.record.object().content().clone(),
            version: missing.record.version(),
            ticket: self.recovery.ticket,
        });
        self.recovery.showing = None;
        self.recovery.answered = Some(workspace.session);
        self.message = Some(format!(
            "Choose the file of {}. Its content must be identical; a damaged project copy is repaired from it.",
            missing.label
        ));
        if let Err(error) = self.dialogs.start(DialogKind::RelinkOriginal, context) {
            self.recovery.relink = None;
            self.error = Some(error);
        }
    }

    pub(super) fn receive_relink_dialog(&mut self, path: std::path::PathBuf) {
        let Some(capture) = self.recovery.relink.as_ref() else {
            return;
        };
        if self.workspace.as_ref().map(|workspace| workspace.session) != Some(capture.session) {
            self.recovery.relink = None;
            self.error = Some(
                "The project changed while choosing the file. Locate the Original again.".into(),
            );
            return;
        }
        let request = ProjectRequest::RelinkOriginal {
            ticket: capture.ticket,
            expected_session: capture.session,
            content: capture.content.clone(),
            expected_version: capture.version,
            path,
        };
        self.submit(request);
    }

    /// While an original is missing its decoder error is expected; say what
    /// is missing and how to fix it instead of the raw storage error.
    pub(super) fn missing_original_notice(&self) -> Option<&'static str> {
        self.presentation.error()?;
        let report = self.recovery.report.as_ref()?;
        (Some(report.session) == self.workspace.as_ref().map(|workspace| workspace.session)
            && report.missing().next().is_some())
        .then_some("The Original's file is missing, so its pictures and sound are unavailable. Locate it with :relink; your edits are unchanged.")
    }

    /// Names every open preview a close would discard.
    pub(super) fn unsaved_previews(&self) -> Vec<&'static str> {
        let mut previews = Vec::new();
        if self.camera.is_some() {
            previews.push("Camera framing");
        }
        if self.gain.is_some() {
            previews.push("Gain");
        }
        if self.room_tone.is_some() {
            previews.push("Room tone");
        }
        if self.trim.is_some() {
            previews.push("Trim");
        }
        if self.slip.is_some() {
            previews.push("Slip");
        }
        if self.splice.is_some() {
            previews.push("Place slice");
        }
        if self.command_open && !self.command.trim().is_empty() {
            previews.push("The command being typed");
        }
        if self.macros.recording() {
            previews.push("The macro being recorded");
        }
        if !self.bindings.pending().is_empty() {
            previews.push("A key sequence not yet finished");
        }
        if !self.youtube.url.trim().is_empty() && !self.youtube.active() {
            previews.push("The YouTube address being typed");
        }
        if self.youtube.active() {
            previews.push("The YouTube import in progress (it will be cancelled)");
        }
        previews
    }

    /// `:close`: the same readiness as File › Close Project. Unsaved previews
    /// are never discarded silently; the user applies or cancels them first.
    pub(super) fn close_command(&mut self) {
        let previews = self.unsaved_previews();
        let refusal = close_refusal(&CloseState {
            project: self.workspace.is_some(),
            previews: &previews,
            busy: self.service.is_busy(),
            importing: self.importing(),
            render_decision: self.render.blocking(),
            dialog: self.dialogs.is_open(),
            macro_recording: self.macros.recording() || self.macros.is_pending(),
            panel: self.marks.open
                || self.models.open
                || self.diagnostics.open
                || self.storage.open
                || self.jobs.open
                || self.help_open,
        });
        match refusal {
            Some(refusal) => self.error = Some(refusal),
            None => {
                self.stop_playback();
                self.submit(ProjectRequest::Close);
            }
        }
    }

    /// Returns true when a requested window close must wait for an answer.
    pub(super) fn hold_close_for_previews(&mut self, context: &egui::Context) -> bool {
        if self.recovery.close_confirmed {
            return false;
        }
        let previews = self.unsaved_previews();
        if previews.is_empty() {
            return false;
        }
        context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        self.recovery.close_prompt = Some(previews);
        self.recovery.focus_pending = true;
        true
    }

    pub(super) fn recovery_keyboard(&mut self, context: &egui::Context) -> bool {
        if !self.recovery.blocking() {
            return false;
        }
        let composing = self.ime_composing
            || context.input(|input| {
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Ime(_)))
            });
        if composing {
            // IME confirmation must never answer a recovery question.
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
        self.bindings.clear();
        true
    }

    pub(super) fn recovery_windows(&mut self, context: &egui::Context) {
        if self.recovery.close_prompt.is_some() {
            self.close_prompt_window(context);
        } else if self.recovery.launch.is_some() {
            self.launch_window(context);
        } else if self.recovery.showing.is_some() {
            self.report_window(context);
        }
        self.storage_banner(context);
    }

    fn focus_first(&mut self, ui: &egui::Ui, response: &egui::Response) {
        if std::mem::take(&mut self.recovery.focus_pending) {
            ui.memory_mut(|memory| memory.request_focus(response.id));
        }
    }

    fn launch_window(&mut self, context: &egui::Context) {
        let Some(offer) = self.recovery.launch.clone() else {
            return;
        };
        let name = project_name(&offer.project);
        let mut reopen = false;
        let modal = egui::Modal::new(egui::Id::new(LAUNCH_ID)).show(context, |ui| {
            super::accessibility::dialog(ui, "Deadpan did not close normally");
            ui.set_max_width(460.0);
            ui.heading("Deadpan did not close normally");
            ui.label(format!(
                "{name} was open when Deadpan last stopped, after a crash, force quit or power loss. Every edit that was saved is still in the project."
            ));
            ui.label("Reopen it to continue from its last saved edit. Previews that were never saved (Camera, Gain, Trim, Slip, Room tone and text being typed) are gone.");
            ui.weak(offer.project.display().to_string());
            ui.add_space(6.0);
            let primary = ui.add(style::action(format!("Reopen {name}"), "Enter"));
            self.focus_first(ui, &primary);
            reopen = primary.clicked();
            if ui.add(style::action("Not now", "Esc")).clicked() {
                self.recovery.launch = None;
            }
        });
        if reopen {
            self.recovery.launch = None;
            self.submit(ProjectRequest::Open(offer.project.clone()));
        } else if modal.should_close() && !self.ime_composing {
            self.recovery.launch = None;
            self.message = Some(format!(
                "{name} was not reopened. Open it any time with ⌘O; its saved edits are intact."
            ));
        }
        // Answered either way: the crashed instance's record is consumed.
        if self.recovery.launch.is_none()
            && let Some(journal) = &self.recovery.journal
            && let Err(error) = journal.dismiss(&offer)
        {
            eprintln!("{error}");
        }
    }

    fn report_window(&mut self, context: &egui::Context) {
        let Some(report) = self.recovery.report.clone() else {
            self.recovery.showing = None;
            return;
        };
        let recovery = &report.recovery;
        let missing: Vec<_> = report.missing().cloned().collect();
        let mut locate = false;
        let mut renders = false;
        let modal = egui::Modal::new(egui::Id::new(REPORT_ID)).show(context, |ui| {
            super::accessibility::dialog(ui, "Project recovery report");
            ui.set_max_width(500.0);
            ui.heading(if recovery.unclean_previous_writer.is_some() {
                "Project recovered"
            } else if !missing.is_empty() {
                "Original missing"
            } else {
                "Interrupted work"
            });
            if recovery.unclean_previous_writer.is_some() {
                ui.label("The last session with this project ended without closing it. It opened at its last saved edit; nothing that was saved was lost.");
            }
            if recovery.interrupted_render_count > 0 {
                let retained = recovery
                    .interrupted_renders
                    .iter()
                    .filter(|render| render.checkpoint_attempt_id.is_some())
                    .count();
                ui.label(format!(
                    "{} render{} stopped before finishing. No movie was saved by {}. {}",
                    recovery.interrupted_render_count,
                    plural(recovery.interrupted_render_count),
                    if recovery.interrupted_render_count == 1 { "it" } else { "them" },
                    if retained > 0 {
                        "A finished encoding was kept: in Renders choose Save movie from that attempt to verify and save it without encoding again."
                    } else {
                        "In Renders choose Render this saved edit again."
                    }
                ));
            }
            if recovery.interrupted_publication_count > 0 {
                let committed = recovery
                    .interrupted_publications
                    .iter()
                    .any(|publication| publication.movie_committed);
                ui.label(format!(
                    "{} movie save{} stopped while writing to {} destination. {}",
                    recovery.interrupted_publication_count,
                    plural(recovery.interrupted_publication_count),
                    if recovery.interrupted_publication_count == 1 { "its" } else { "their" },
                    if committed {
                        "A movie may be at the destination without its report: in Renders choose Check previous destination."
                    } else {
                        "No destination file was changed during recovery."
                    }
                ));
            }
            if recovery.interrupted_generation_count > 0 {
                ui.label(format!(
                    "{} AI pause generation{} stopped. Accepted pauses are unaffected; select the pause and generate again.",
                    recovery.interrupted_generation_count,
                    plural(recovery.interrupted_generation_count),
                ));
            }
            for status in &missing {
                let what = if status.primary { "The Original" } else { "The sound" };
                let reason = match &status.availability {
                    OriginalAvailability::Missing => "is missing (moved, renamed or deleted)".to_owned(),
                    OriginalAvailability::Unreadable { reason } => format!("cannot be used: {reason}"),
                    OriginalAvailability::Present => continue,
                };
                ui.label(format!(
                    "{what} {} {reason}. Its pictures and sound are unavailable until you locate the file; the project and its edits are unchanged.",
                    status.label
                ));
            }
            if let Some(error) = &recovery.record_error {
                ui.weak(format!("Recovery evidence was not fully recorded: {error}"));
            }
            ui.add_space(6.0);
            let mut first = None;
            if !missing.is_empty() {
                let button = ui.add(style::action("Locate Original…", ":relink"));
                locate = button.clicked();
                first.get_or_insert(button);
            }
            if recovery.interrupted_render_count > 0 || recovery.interrupted_publication_count > 0 {
                let button = ui.add(style::action("Open Renders", ":renders"));
                renders = button.clicked();
                first.get_or_insert(button);
            }
            let close = ui.add(style::action("Continue", "Esc"));
            if close.clicked() {
                self.recovery.showing = None;
            }
            let first = first.unwrap_or(close);
            self.focus_first(ui, &first);
        });
        let session = self.recovery.showing;
        if locate {
            self.locate_original(context);
        } else if renders {
            self.recovery.showing = None;
            // One interrupted job opens on its attempts; several open the
            // saved-edit list, where each job shows its own.
            let mut jobs: Vec<_> = recovery
                .interrupted_renders
                .iter()
                .map(|render| render.job_id.as_str())
                .collect();
            jobs.dedup();
            match (jobs.as_slice(), recovery.interrupted_render_count) {
                ([job], 1..)
                    if recovery.interrupted_render_count == recovery.interrupted_renders.len() =>
                {
                    match deadpan_jobs::RequestId::new(*job) {
                        Ok(job) => self.render.history.request_attempts(job),
                        Err(_) => self.render.history.requested = true,
                    }
                }
                _ => self.render.history.requested = true,
            }
        } else if modal.should_close() && !self.ime_composing {
            self.recovery.showing = None;
        }
        if self.recovery.showing.is_none() {
            self.recovery.answered = session.or(self.recovery.answered);
            // Shown and answered: the store may forget the retained findings.
            if let Some(session) = session
                && !report.recovery.is_clean()
                && !self.recovery.acknowledged.contains(&session)
            {
                self.recovery.acknowledged.push(session);
                self.submit_now(ProjectRequest::AcknowledgeRecovery {
                    expected_session: session,
                });
            }
        }
    }

    fn close_prompt_window(&mut self, context: &egui::Context) {
        let Some(previews) = self.recovery.close_prompt.clone() else {
            return;
        };
        let mut discard = false;
        let modal = egui::Modal::new(egui::Id::new(CLOSE_ID)).show(context, |ui| {
            super::accessibility::dialog(ui, "Close with unsaved previews?");
            ui.set_max_width(440.0);
            ui.heading("Close with unsaved previews?");
            ui.label("Every saved edit is already in the project. These open previews were never saved and will be discarded:");
            for preview in &previews {
                ui.label(format!("• {preview}"));
            }
            ui.add_space(6.0);
            let keep = ui.add(style::action("Keep editing", "Esc"));
            self.focus_first(ui, &keep);
            if keep.clicked() {
                self.recovery.close_prompt = None;
            }
            discard = ui.button("Discard previews and close").clicked();
        });
        if discard {
            self.recovery.close_prompt = None;
            self.recovery.close_confirmed = true;
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if modal.should_close() && !self.ime_composing {
            self.recovery.close_prompt = None;
        }
    }

    fn storage_banner(&self, context: &egui::Context) {
        let Some(alert) = &self.recovery.storage else {
            return;
        };
        egui::Area::new(egui::Id::new("recovery-storage-alert"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 48.0))
            .interactable(false)
            .show(context, |ui| {
                egui::Frame::new()
                    .fill(style::PANEL)
                    .stroke(egui::Stroke::new(1.0, style::ERROR))
                    .corner_radius(4)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.set_max_width(560.0);
                        ui.label(style::semibold(alert.headline()).color(style::ERROR));
                        ui.label(
                            egui::RichText::new(match alert.code {
                                "DiskFull" => "The last action was not saved because the disk is full. Your last saved edit is intact. Free space, then repeat the action; this notice clears when a save succeeds.",
                                "ProjectReadOnly" => "The last action was not saved because the project cannot be written. Your last saved edit is intact. Make it writable or copy it to Documents/Deadpan and reopen it.",
                                _ => "The last action was not saved because macOS denied access. Your last saved edit is intact. Restore access to the project folder, then repeat the action.",
                            })
                            .size(12.0)
                            .color(style::TEXT),
                        );
                    });
            });
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn project_name(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "The last project".into())
}

/// What `:close` must wait for; mirrors the menu's Close readiness.
pub(super) struct CloseState<'a> {
    pub project: bool,
    pub previews: &'a [&'static str],
    pub busy: bool,
    pub importing: bool,
    pub render_decision: bool,
    pub dialog: bool,
    pub macro_recording: bool,
    pub panel: bool,
}

/// Why `:close` cannot close now, or `None` when it can.
pub(super) fn close_refusal(state: &CloseState<'_>) -> Option<String> {
    if !state.project {
        return Some("No project is open.".into());
    }
    // The command being typed has already been submitted.
    let previews = state
        .previews
        .iter()
        .filter(|preview| **preview != "The command being typed")
        .copied()
        .collect::<Vec<_>>();
    if !previews.is_empty() {
        return Some(format!(
            "Close would discard unsaved previews: {}. Apply them with Enter or cancel them with Esc, then :close. Saved edits are already in the project.",
            previews.join(", ")
        ));
    }
    if state.render_decision {
        return Some("Finish or cancel the Render decision first, then :close.".into());
    }
    if state.macro_recording {
        return Some("Save (q) or cancel (Esc) the macro recording first, then :close.".into());
    }
    if state.busy || state.importing {
        return Some(
            "The project is still saving or importing; :close again when it finishes.".into(),
        );
    }
    if state.dialog || state.panel {
        return Some("Close the open panel or dialog first, then :close.".into());
    }
    None
}

#[cfg(test)]
mod close_tests {
    use super::*;

    fn ready<'a>() -> CloseState<'a> {
        CloseState {
            project: true,
            previews: &[],
            busy: false,
            importing: false,
            render_decision: false,
            dialog: false,
            macro_recording: false,
            panel: false,
        }
    }

    #[test]
    fn close_waits_for_every_condition_the_menu_waits_for() {
        assert_eq!(close_refusal(&ready()), None);
        let refused = |state: CloseState<'_>, needle: &str| {
            let refusal = close_refusal(&state).expect("refused");
            assert!(refusal.contains(needle), "{refusal}");
        };
        refused(
            CloseState {
                project: false,
                ..ready()
            },
            "No project",
        );
        refused(
            CloseState {
                previews: &["Trim", "Gain"],
                ..ready()
            },
            "unsaved previews: Trim, Gain",
        );
        refused(
            CloseState {
                render_decision: true,
                ..ready()
            },
            "Render decision",
        );
        refused(
            CloseState {
                busy: true,
                ..ready()
            },
            "still saving",
        );
        refused(
            CloseState {
                importing: true,
                ..ready()
            },
            "still saving",
        );
        refused(
            CloseState {
                macro_recording: true,
                ..ready()
            },
            "macro recording",
        );
        refused(
            CloseState {
                dialog: true,
                ..ready()
            },
            "panel or dialog",
        );
        refused(
            CloseState {
                panel: true,
                ..ready()
            },
            "panel or dialog",
        );
        // The already-submitted command text is not a preview to protect.
        assert_eq!(
            close_refusal(&CloseState {
                previews: &["The command being typed"],
                ..ready()
            }),
            None
        );
    }
}
