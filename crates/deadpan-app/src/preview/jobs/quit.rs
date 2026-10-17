//! Quitting while background work the person asked for (a render, AI pause
//! pictures, tracking, an import or an install) is still running or queued.
//!
//! A window close or system Quit asks first: **Quit when they finish** keeps
//! the window open and quits once exactly those jobs have finished, **Cancel
//! them and quit** cancels and quits now, and **Go back** (Escape) stays.
//! Automatic work (transcription, shot and pause detection, proxies) never
//! asks: it stops on quit and resumes next time.
//!
//! Every choice ends in the existing close path: `Jobs::shutdown`, then each
//! job's own drain and the project writer's release only after its cleanup
//! is known. Waiting to quit changes nothing about how jobs run.

use crate::jobs::JobId;

use super::*;

#[derive(Default)]
pub(in crate::preview) struct State {
    /// The question is open.
    pub(in crate::preview) open: bool,
    focus_pending: bool,
    /// Quit once none of these jobs remains.
    pub(in crate::preview) after: Option<Vec<JobId>>,
    /// The person chose to quit (or the awaited jobs finished): the close
    /// path proceeds without asking again.
    confirmed: bool,
    /// The job rows the question names, captured when it opened.
    rows: Vec<String>,
}

/// What a close request does now.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::preview) enum CloseChoice {
    Proceed,
    Ask,
}

/// Ask only when work the person asked for is live and they have not
/// already chosen to quit.
pub(in crate::preview) fn close_choice(confirmed: bool, requested: usize) -> CloseChoice {
    if confirmed || requested == 0 {
        CloseChoice::Proceed
    } else {
        CloseChoice::Ask
    }
}

/// "1 job" / "2 jobs".
fn jobs_count(count: usize) -> String {
    format!("{count} job{}", if count == 1 { "" } else { "s" })
}

impl DeadpanApp {
    /// Native launches: sample battery, Low Power Mode, thermal and memory
    /// conditions for the coordinator. Only explicitly opted-in real-model
    /// qualification replays use this production probe.
    pub fn follow_system_conditions(&self) {
        self.service
            .jobs()
            .set_probe(Some(crate::jobs::system_probe()));
    }

    /// On a close request: whether to hold it for the jobs question.
    pub(in crate::preview) fn hold_close_for_jobs(&mut self, context: &egui::Context) -> bool {
        let requested = self.service.jobs().requested();
        if close_choice(self.jobs.quit.confirmed, requested.len()) == CloseChoice::Proceed {
            return false;
        }
        context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        let current = self.workspace.as_ref().map(|workspace| workspace.session);
        self.jobs.quit.rows = requested.iter().map(|row| job_text(row, current)).collect();
        self.jobs.quit.open = true;
        self.jobs.quit.focus_pending = true;
        context.request_repaint();
        true
    }

    /// Once per outer frame: quit when the awaited jobs have finished, hand
    /// a deferred system Quit to the close path, and tell the native quit
    /// guard whether to defer.
    pub(in crate::preview) fn reconcile_quit(&mut self, context: &egui::Context) {
        let board = self.service.jobs().clone();
        if let Some(ids) = &self.jobs.quit.after
            && !board.any_live(ids)
        {
            self.jobs.quit.after = None;
            self.jobs.quit.confirmed = true;
            self.message = Some("The jobs finished; quitting.".into());
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        #[cfg(target_os = "macos")]
        if let Some(guard) = &self.quit_guard {
            if guard.take_deferred() && !self.close_pending {
                // A system Quit while jobs run: ask through the window's close.
                context.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            guard.set_background(
                !self.close_pending && !self.jobs.quit.confirmed && !board.requested().is_empty(),
            );
        }
        if self.jobs.quit.after.is_some() {
            // Notice the jobs finishing even while nothing else repaints.
            context.request_repaint_after(Duration::from_millis(250));
        }
    }

    /// The question owns input above every draft while it is open.
    pub(in crate::preview) fn quit_keyboard(&mut self, context: &egui::Context) -> bool {
        if !self.jobs.quit.open {
            return false;
        }
        let composing = &mut self.ime_composing;
        context.input(|input| help_scroll::observe_composition(&input.events, composing));
        if self.ime_composing {
            // IME confirmation never answers the question.
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

    pub(in crate::preview) fn quit_window(&mut self, context: &egui::Context) {
        self.quit_waiting_banner(context);
        if !self.jobs.quit.open {
            return;
        }
        #[derive(PartialEq)]
        enum Choice {
            Wait,
            Cancel,
            Back,
        }
        let mut choice = None;
        let rows = self.jobs.quit.rows.clone();
        let modal = egui::Modal::new(egui::Id::new("quit-with-jobs")).show(context, |ui| {
            accessibility::dialog(ui, "Quit while jobs run?");
            ui.set_max_width(460.0);
            ui.heading("Quit while jobs run?");
            ui.label(format!(
                "{} you started {} still running or waiting:",
                jobs_count(rows.len()),
                if rows.len() == 1 { "is" } else { "are" }
            ));
            for row in &rows {
                let response = ui.add(
                    egui::Label::new(egui::RichText::new(format!("• {row}")).size(12.0)).wrap(),
                );
                accessibility::full_text(response, row);
            }
            ui.label(
                egui::RichText::new(
                    "Your saved edits are kept either way. When Deadpan quits, automatic work (transcription, shot and pause detection, seek proxies) stops and resumes next time.",
                )
                .size(11.5)
                .weak(),
            );
            ui.add_space(6.0);
            let wait = ui.button("Quit when they finish");
            accessibility::name(
                &wait,
                "Quit when they finish: keep the window open and quit automatically",
            );
            if std::mem::take(&mut self.jobs.quit.focus_pending) {
                wait.request_focus();
            }
            if wait.clicked() {
                choice = Some(Choice::Wait);
            }
            let cancel = ui.button("Cancel them and quit");
            if cancel.clicked() {
                choice = Some(Choice::Cancel);
            }
            if ui.add(style::action("Go back", "Esc")).clicked() {
                choice = Some(Choice::Back);
            }
        });
        if choice.is_none() && modal.should_close() && !self.ime_composing {
            choice = Some(Choice::Back);
        }
        let Some(choice) = choice else {
            return;
        };
        self.jobs.quit.open = false;
        match choice {
            Choice::Wait => {
                let ids: Vec<JobId> = self
                    .service
                    .jobs()
                    .requested()
                    .into_iter()
                    .map(|row| row.id)
                    .collect();
                self.message = Some(format!(
                    "Deadpan quits when {} finish{}. Quit again to cancel them or stay.",
                    jobs_count(ids.len()),
                    if ids.len() == 1 { "es" } else { "" }
                ));
                self.jobs.quit.after = Some(ids);
            }
            Choice::Cancel => {
                self.jobs.quit.after = None;
                self.jobs.quit.confirmed = true;
                context.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Choice::Back => {
                if self.jobs.quit.after.take().is_some() {
                    self.message = Some("Staying open; the jobs keep running.".into());
                }
            }
        }
        context.request_repaint();
    }

    /// While waiting to quit: say so, above everything, without taking input.
    fn quit_waiting_banner(&self, context: &egui::Context) {
        let Some(ids) = &self.jobs.quit.after else {
            return;
        };
        let board = self.service.jobs();
        let left = board
            .snapshot()
            .into_iter()
            .filter(|row| ids.contains(&row.id))
            .count();
        let text = format!(
            "Quitting when {} finish{} · :jobs shows them · Quit again to cancel them or stay",
            jobs_count(left),
            if left == 1 { "es" } else { "" }
        );
        egui::Area::new(egui::Id::new("quit-waiting"))
            .order(egui::Order::Foreground)
            // Below the header, so the project name and Saved state stay visible.
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
            .interactable(false)
            .show(context, |ui| {
                egui::Frame::new()
                    .fill(style::PANEL)
                    .stroke(egui::Stroke::new(1.0, style::LAVENDER))
                    .corner_radius(4)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        let response = ui.label(egui::RichText::new(&text).size(12.0));
                        accessibility::full_text(response, &text);
                    });
            });
    }

    /// Whether Deadpan quits once the awaited jobs finish (replays).
    #[cfg(feature = "ui-harness")]
    pub(crate) fn quit_waiting(&self) -> bool {
        self.jobs.quit.after.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_close_asks_only_for_requested_work_not_yet_confirmed() {
        assert_eq!(close_choice(false, 0), CloseChoice::Proceed);
        assert_eq!(close_choice(false, 2), CloseChoice::Ask);
        assert_eq!(close_choice(true, 2), CloseChoice::Proceed);
        assert_eq!(jobs_count(1), "1 job");
        assert_eq!(jobs_count(3), "3 jobs");
    }
}
