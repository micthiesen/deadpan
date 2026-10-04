//! Bounded browsing of stored render evidence. Selection captures immutable
//! job/attempt/publication identities before any destination picker is opened.

use super::*;
use crate::project::render_history::{Page, Query, Recovery, Request, Update};
use deadpan_jobs::{AttemptId, RequestId, render::RenderAttemptState};

// Covers the store's maximum 512 job pages followed by 8192 attempt pages.
const MAX_BACK_PAGES: usize = 16_384;

#[derive(Default)]
pub(in crate::preview) struct History {
    pub open: bool,
    pub requested: bool,
    context: Option<ProjectRenderContext>,
    query: Option<Query>,
    page: Option<Page>,
    queued: Option<Query>,
    pending: Option<(u64, Query)>,
    back: Vec<Query>,
    error: Option<String>,
    prior_focus: Option<egui::Id>,
    return_focus: Option<(u64, egui::Id)>,
}

impl History {
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn ready_for_check(&self) -> bool {
        self.open && self.page.is_some() && !self.loading()
    }

    fn queue(&mut self, query: Query, remember: bool) {
        if remember && let Some(previous) = &self.query {
            if self.back.len() == MAX_BACK_PAGES {
                self.error = Some("Return to Saved renders to start browsing again.".into());
                return;
            }
            self.back.push(previous.clone());
        }
        self.query = Some(query.clone());
        self.queued = Some(query);
        self.page = None;
        self.pending = None;
        self.error = None;
    }

    fn receive(&mut self, update: Update) {
        if self.context.as_ref() != Some(&update.context)
            || self.pending.as_ref() != Some(&(update.ticket, update.query.clone()))
        {
            return;
        }
        self.pending = None;
        match update.result {
            Ok(page) => self.page = Some(page),
            Err(error) => self.error = Some(format!("{} ({})", error.message, error.code)),
        }
    }

    fn loading(&self) -> bool {
        self.queued.is_some() || self.pending.is_some()
    }
}

#[derive(Clone)]
pub(super) struct RecoveryCapture {
    capture: Capture,
    job: RequestId,
    checkpoint: Option<AttemptId>,
}

enum Choice {
    Query(Query, bool),
    Back,
    Retry {
        job: RequestId,
        revision: RevisionId,
        checkpoint: Option<AttemptId>,
    },
    Reconcile {
        publication: RequestId,
        revision: RevisionId,
    },
    Close,
    Current,
}

impl DeadpanApp {
    pub(super) fn restore_render_history_focus(&mut self, context: &egui::Context) {
        if !self.render.blocking()
            && !self.dialogs.is_open()
            && let Some((closed_frame, focus)) = self.render.history.return_focus
            && context.cumulative_frame_nr() > closed_frame
        {
            context.memory_mut(|memory| memory.request_focus(focus));
            self.render.history.return_focus = None;
        }
    }

    pub(in crate::preview) fn receive_render_history(&mut self, update: Option<Update>) {
        if self.render.history.context.as_ref().is_some_and(|context| {
            !self.workspace.as_ref().is_some_and(|workspace| {
                workspace.session == context.session
                    && workspace.document.project_id() == &context.project
            })
        }) {
            self.render.history = History::default();
        }
        if let Some(update) = update {
            self.render.history.receive(update);
        }
    }

    /// Run only after the final layout pass. Browsing uses the owner thread and
    /// its own ticketed reply; the UI never opens SQLite or scans all history.
    pub(in crate::preview) fn dispatch_render_history(&mut self, context: &egui::Context) {
        if self.render.history.requested {
            self.render.history.requested = false;
            if self.render.flow.is_some() || self.dialogs.is_open() {
                return;
            }
            let Some(workspace) = &self.workspace else {
                self.error = Some("Open a project to see its saved renders.".into());
                return;
            };
            let captured = ProjectRenderContext {
                session: workspace.session,
                project: workspace.document.project_id().clone(),
            };
            if self.render.history.context.as_ref() != Some(&captured) {
                self.render.history = History::default();
            }
            self.render.history.context = Some(captured);
            self.render.history.open = true;
            self.render.history.prior_focus = Some(
                context
                    .memory(|memory| memory.focused())
                    .unwrap_or_else(|| pane_id(self.pane)),
            );
            self.render.history.return_focus = None;
            let query = self
                .render
                .history
                .query
                .clone()
                .unwrap_or(Query::Jobs { after: None });
            self.render.history.queue(query, false);
            self.pause_playback();
            self.cancel_repeats("saved renders were opened");
            self.bindings.clear();
            context.request_repaint();
        }
        if !self.render.history.open || self.service.is_busy() {
            return;
        }
        let Some(query) = self.render.history.queued.clone() else {
            return;
        };
        let Some(captured) = self.render.history.context.clone() else {
            return;
        };
        let Some(ticket) = self.next_serial() else {
            return;
        };
        match self.service.submit(ProjectRequest::RenderHistory(Request {
            ticket,
            context: captured,
            query: query.clone(),
        })) {
            Ok(()) => {
                self.render.history.queued = None;
                self.render.history.pending = Some((ticket, query));
            }
            Err(error) => {
                self.render.history.queued = None;
                self.render.history.error = Some(error);
            }
        }
    }

    fn close_render_history(&mut self, context: &egui::Context) {
        self.render.history.open = false;
        // Keep the entry focus through a suspended browser so cancelling its
        // native destination picker can reopen and later close the same scope.
        if let Some(focus) = self.render.history.prior_focus {
            // The modal still owns focus for this frame, including discarded
            // layout passes. Restore entry focus on the next outer frame.
            self.render.history.return_focus = Some((context.cumulative_frame_nr(), focus));
        }
        context.request_repaint();
        context.request_discard("saved renders closed");
    }

    pub(super) fn render_history_window(&mut self, context: &egui::Context) {
        if !self.render.history.open || self.render.flow.is_some() {
            return;
        }
        let mut choice = None;
        let busy = self.render.history.loading() || self.service.is_busy();
        let active = self.current_render().is_some_and(|workflow| {
            !workflow.status.cleanup_confirmed
                || !matches!(
                    workflow.status.stage,
                    WorkflowStage::Idle | WorkflowStage::Finished
                )
        });
        let modal = egui::Modal::new(egui::Id::new("saved-renders")).show(context, |ui| {
            ui.set_width(620.0_f32.min(context.content_rect().width() - 80.0));
            ui.heading("Saved renders");
            ui.weak("Tab / Shift Tab move between controls · Enter activates · Esc returns to the editor");
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!busy, egui::Button::new("Saved edits")).clicked() {
                    choice = Some(Choice::Query(Query::Jobs { after: None }, false));
                }
                if ui.add_enabled(!busy, egui::Button::new("Destinations")).clicked() {
                    choice = Some(Choice::Query(Query::Publications { after: None }, false));
                }
                if ui.add_enabled(!busy && !self.render.history.back.is_empty(), egui::Button::new("Back")).clicked() {
                    choice = Some(Choice::Back);
                }
                if ui.add_enabled(!busy, egui::Button::new("Refresh saved renders")).clicked()
                    && let Some(query) = &self.render.history.query {
                    choice = Some(Choice::Query(query.clone(), false));
                }
                if ui.add(style::action("Back to editor", "Esc")).clicked() { choice = Some(Choice::Close); }
                if self.current_render().is_some()
                    && ui.button("View current render").clicked() { choice = Some(Choice::Current); }
            });
            if let Some(error) = &self.render.history.error { ui.colored_label(style::LAVENDER, error); }
            if busy { ui.label("Loading saved renders…"); }
            if active { ui.weak("A render is active. Return to the editor to view its status or cancel it."); }
            if self.camera.is_some() || self.gain.is_some() || self.room_tone.is_some() {
                ui.weak("These actions use saved edits. Your current preview remains unsaved.");
            }
            let height = (context.content_rect().height() - ui.min_rect().height() - 100.0).max(140.0);
            egui::ScrollArea::vertical().max_height(height).min_scrolled_height(height).show(ui, |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    if let Some(page) = &self.render.history.page {
                        show_page(ui, page, !active, &mut choice);
                    }
                });
            });
        });
        if !self.ime_composing
            && !context.input(|input| {
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Ime(_)))
            })
            && modal.should_close()
        {
            choice = Some(Choice::Close);
        }
        match choice {
            Some(Choice::Close) => self.close_render_history(context),
            Some(Choice::Current) => {
                self.close_render_history(context);
                self.render.open = true;
            }
            Some(Choice::Query(query, remember)) => {
                if !remember && self.render.history.query.as_ref() != Some(&query) {
                    self.render.history.back.clear();
                }
                self.render.history.queue(query, remember);
            }
            Some(Choice::Back) => {
                if let Some(query) = self.render.history.back.pop() {
                    self.render.history.queue(query, false);
                }
            }
            Some(Choice::Retry {
                job,
                revision,
                checkpoint,
            }) => {
                if let Some(capture) = self.history_capture(revision) {
                    self.close_render_history(context);
                    self.render.error = None;
                    self.render.open = true;
                    let save = self.render_movie_suggestion(&capture);
                    match self.dialogs.save_movie(save, context) {
                        Ok(()) => {
                            self.render.flow = Some(Flow::RecoveryPicker(RecoveryCapture {
                                capture,
                                job,
                                checkpoint,
                            }))
                        }
                        Err(error) => self.render.error = Some(error),
                    }
                }
            }
            Some(Choice::Reconcile {
                publication,
                revision,
            }) => {
                if let Some(capture) = self.history_capture(revision) {
                    self.close_render_history(context);
                    self.submit_render_recovery(
                        capture,
                        Recovery::Reconcile {
                            publication_id: publication,
                        },
                    );
                }
            }
            None => {}
        }
    }

    fn history_capture(&self, revision: RevisionId) -> Option<Capture> {
        let context = self.render.history.context.as_ref()?;
        let workspace = self.workspace.as_ref()?;
        (workspace.session == context.session
            && workspace.document.project_id() == &context.project)
            .then(|| Capture {
                context: context.clone(),
                revision,
                package: workspace.path.clone(),
            })
    }

    pub(super) fn receive_recovery_dialog(
        &mut self,
        captured: RecoveryCapture,
        path: Option<PathBuf>,
    ) {
        if !captured.capture.same_project(self.workspace.as_deref()) {
            self.render.error = Some(
                "The project changed while choosing a destination. Open Saved renders again."
                    .into(),
            );
            return;
        }
        let Some(path) = path else {
            self.render.history.open = true;
            return;
        };
        self.render.destination = Some((captured.capture.context.session, path.clone()));
        self.submit_render_recovery(
            captured.capture,
            Recovery::Retry {
                job_id: captured.job,
                checkpoint_attempt_id: captured.checkpoint,
                destination: path,
            },
        );
    }

    fn submit_render_recovery(&mut self, capture: Capture, recovery: Recovery) {
        self.render.error = None;
        self.render.open = true;
        let Some(ticket) = self.next_serial() else {
            return;
        };
        match self
            .service
            .submit(ProjectRequest::Render(ProjectRenderRequest {
                ticket,
                context: capture.context.clone(),
                operation: ProjectRenderOperation::Recover(recovery),
            })) {
            Ok(()) => {
                self.render.flow = Some(Flow::Submitted {
                    capture,
                    ticket,
                    close_preview: false,
                })
            }
            Err(error) => self.render.error = Some(error),
        }
    }
}

fn show_page(ui: &mut egui::Ui, page: &Page, available: bool, choice: &mut Option<Choice>) {
    match page {
        Page::Jobs { items, next_after } => {
            if items.is_empty() {
                ui.label("No saved renders on this page.");
            }
            for job in items {
                ui.push_id(job.job_id.as_str(), |ui| {
                    ui.separator();
                    ui.label(format!("Saved revision {}", job.revision_id.as_str()));
                    ui.weak(format!(
                        "Edit frames {} to {} · {}",
                        job.range.start().0,
                        job.range.end().0,
                        if job.automatic {
                            "Automatic MP4"
                        } else {
                            "Engineering render"
                        }
                    ));
                    if history_button(ui, true, "View render attempts") {
                        *choice = Some(Choice::Query(
                            Query::Attempts {
                                job: job.job_id.clone(),
                                after_ordinal: 0,
                            },
                            true,
                        ));
                    }
                });
            }
            if let Some(after) = next_after
                && history_button(ui, true, "Next saved edits")
            {
                *choice = Some(Choice::Query(
                    Query::Jobs {
                        after: Some(after.clone()),
                    },
                    true,
                ));
            }
        }
        Page::Attempts {
            job,
            items,
            next_after_ordinal,
        } => {
            ui.label(format!("Saved revision {}", job.revision_id.as_str()));
            ui.label(format!(
                "Edit frames {} to {}",
                job.range.start().0,
                job.range.end().0
            ));
            if history_button(
                ui,
                available && job.automatic,
                "Render this saved edit again…",
            ) {
                *choice = Some(Choice::Retry {
                    job: job.job_id.clone(),
                    revision: job.revision_id.clone(),
                    checkpoint: None,
                });
            }
            ui.weak("Rendering again uses this saved edit. Saving a retained movie checks the movie again without encoding it again.");
            if !job.automatic {
                ui.weak("Engineering renders are available for inspection only.");
            }
            if items.is_empty() {
                ui.label("No attempts on this page.");
            }
            for attempt in items {
                ui.push_id(attempt.attempt_id.as_str(), |ui| {
                    ui.separator();
                    ui.strong(format!(
                        "Attempt {} · {}",
                        attempt.ordinal,
                        attempt_label(attempt.state)
                    ));
                    if let Some(diagnostic) = &attempt.diagnostic {
                        ui.label(&diagnostic.detail);
                    }
                    if let Some(checkpoint) = &attempt.checkpoint_attempt_id {
                        let label = format!("Save movie from attempt {}…", attempt.ordinal);
                        if history_button(ui, available && job.automatic, label) {
                            *choice = Some(Choice::Retry {
                                job: job.job_id.clone(),
                                revision: job.revision_id.clone(),
                                checkpoint: Some(checkpoint.clone()),
                            });
                        }
                    }
                });
            }
            if let Some(after) = next_after_ordinal
                && history_button(ui, true, "Next attempts")
            {
                *choice = Some(Choice::Query(
                    Query::Attempts {
                        job: job.job_id.clone(),
                        after_ordinal: *after,
                    },
                    true,
                ));
            }
        }
        Page::Publications { items, next_after } => {
            ui.weak("Check the original destination without choosing a new path. A saved movie remains saved if later confirmation failed.");
            if items.is_empty() {
                ui.label("No destinations on this page.");
            }
            for publication in items {
                ui.push_id(publication.publication_id.as_str(), |ui| {
                    ui.separator();
                    ui.label(format!("Movie: {}", publication.destination.display()));
                    ui.label(format!(
                        "Saved revision {}",
                        publication.revision_id.as_str()
                    ));
                    ui.strong(publication_label(
                        publication.outcome,
                        publication.observed_movie_commit,
                    ));
                    if let Some(diagnostic) = &publication.diagnostic {
                        ui.label(&diagnostic.detail);
                    }
                    if history_button(
                        ui,
                        available && publication.can_reconcile(),
                        "Check previous destination",
                    ) {
                        *choice = Some(Choice::Reconcile {
                            publication: publication.publication_id.clone(),
                            revision: publication.revision_id.clone(),
                        });
                    }
                });
            }
            if let Some(after) = next_after
                && history_button(ui, true, "Next destinations")
            {
                *choice = Some(Choice::Query(
                    Query::Publications {
                        after: Some(after.clone()),
                    },
                    true,
                ));
            }
        }
    }
}

fn history_button(ui: &mut egui::Ui, enabled: bool, label: impl Into<egui::WidgetText>) -> bool {
    let response = ui.add_enabled(enabled, egui::Button::new(label));
    if response.gained_focus() {
        response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
    }
    response.clicked()
}

fn publication_label(
    outcome: deadpan_jobs::render::publication::PublicationOutcome,
    committed: bool,
) -> &'static str {
    use deadpan_jobs::render::publication::PublicationOutcome;
    if outcome == PublicationOutcome::Published {
        return "Movie saved and confirmed";
    }
    if committed || outcome == PublicationOutcome::PublishedUnconfirmed {
        return "Movie saved; confirmation needs recovery";
    }
    match outcome {
        PublicationOutcome::InProgress => "Saving movie",
        PublicationOutcome::Interrupted => "Saving was interrupted",
        PublicationOutcome::Unresolved => "Destination needs recovery",
        PublicationOutcome::Failed => "Movie was not published",
        PublicationOutcome::Cancelled => "Saving was cancelled",
        PublicationOutcome::Published | PublicationOutcome::PublishedUnconfirmed => {
            unreachable!("saved states handled above")
        }
    }
}

fn attempt_label(state: RenderAttemptState) -> &'static str {
    match state {
        RenderAttemptState::Queued => "Queued",
        RenderAttemptState::Encoding => "Rendering",
        RenderAttemptState::EncodedRetained => "Movie retained",
        RenderAttemptState::Verifying => "Checking movie",
        RenderAttemptState::Verified => "Movie verified",
        RenderAttemptState::Cancelling => "Finishing cancellation",
        RenderAttemptState::Cancelled => "Cancelled",
        RenderAttemptState::Failed => "Failed",
        RenderAttemptState::Interrupted => "Interrupted",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_pending_query_ticket_and_session_can_complete_history() {
        let context = ProjectRenderContext {
            session: 7,
            project: deadpan_core::ProjectId::new("history-project").unwrap(),
        };
        let query = Query::Jobs { after: None };
        let mut history = History {
            context: Some(context.clone()),
            pending: Some((11, query.clone())),
            ..Default::default()
        };
        let update = Update {
            ticket: 11,
            context,
            query,
            result: Ok(Page::Jobs {
                items: vec![],
                next_after: None,
            }),
        };
        let mut wrong_ticket = update.clone();
        wrong_ticket.ticket = 10;
        let mut wrong_session = update.clone();
        wrong_session.context.session = 8;
        let mut wrong_query = update.clone();
        wrong_query.query = Query::Publications { after: None };
        for stale in [wrong_ticket, wrong_session, wrong_query] {
            history.receive(stale);
            assert!(history.pending.is_some());
            assert!(history.page.is_none());
        }
        history.receive(update.clone());
        assert!(history.pending.is_none());
        assert_eq!(history.page, Some(update.result.unwrap()));
    }

    #[test]
    fn a_durable_movie_commit_survives_every_later_failure_label() {
        use deadpan_jobs::render::publication::PublicationOutcome;
        for outcome in [
            PublicationOutcome::Interrupted,
            PublicationOutcome::Unresolved,
            PublicationOutcome::Failed,
            PublicationOutcome::Cancelled,
        ] {
            assert_eq!(
                publication_label(outcome, true),
                "Movie saved; confirmation needs recovery"
            );
        }
        assert_eq!(
            publication_label(PublicationOutcome::Published, true),
            "Movie saved and confirmed"
        );
    }
}
