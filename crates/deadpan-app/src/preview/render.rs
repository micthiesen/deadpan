//! Native Render captures one revision and one optional preview edit before
//! opening a picker. Only the service's ticketed receipt can complete a commit.

use std::time::Instant;

use super::*;
use crate::project::{
    ProjectRenderContext, ProjectRenderLimits, ProjectRenderOperation, ProjectRenderRequest,
};
use deadpan_cli::encoded_render::workflow::{WorkflowOutcome, WorkflowStage};

mod history;

#[derive(Clone, Debug)]
pub(super) struct PreviewEdit {
    pub session: u64,
    pub revision: RevisionId,
    pub cursor: ProjectFrame,
    pub scope: SequenceScope,
    pub edit: ProjectEdit,
}

#[derive(Clone)]
struct Capture {
    context: ProjectRenderContext,
    revision: RevisionId,
    package: PathBuf,
}

impl Capture {
    fn same_project(&self, workspace: Option<&Workspace>) -> bool {
        workspace.is_some_and(|workspace| {
            workspace.session == self.context.session
                && workspace.document.project_id() == &self.context.project
        })
    }

    fn matches(&self, workspace: Option<&Workspace>) -> bool {
        self.same_project(workspace)
            && workspace.is_some_and(|workspace| workspace.document.revision_id() == &self.revision)
    }
}

enum Flow {
    Decision {
        capture: Capture,
        proposal: Result<Option<PreviewEdit>, String>,
    },
    Picker {
        capture: Capture,
        edit: Option<PreviewEdit>,
        discard: bool,
    },
    Submitted {
        capture: Capture,
        ticket: u64,
        close_preview: bool,
    },
    RecoveryPicker(history::RecoveryCapture),
}

#[derive(Default)]
pub(super) struct State {
    flow: Option<Flow>,
    pub requested: bool,
    pub open: bool,
    error: Option<String>,
    destination: Option<(u64, PathBuf)>,
    prior_focus: Option<egui::Id>,
    summary: Option<(ProjectRenderContext, RevisionId, String)>,
    cancel_ticket: Option<(ProjectRenderContext, u64)>,
    pub history: history::History,
}

impl State {
    pub fn blocking(&self) -> bool {
        self.flow.is_some() || self.history.open
    }
    #[cfg(feature = "ui-harness")]
    pub(super) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    #[cfg(feature = "ui-harness")]
    pub(super) fn preview_for_check(&self) -> Option<&PreviewEdit> {
        match &self.flow {
            Some(Flow::Decision {
                proposal: Ok(Some(edit)),
                ..
            })
            | Some(Flow::Picker {
                edit: Some(edit), ..
            }) => Some(edit),
            _ => None,
        }
    }
}

impl DeadpanApp {
    pub(super) fn current_render(&self) -> Option<&crate::project::ProjectRenderStatus> {
        self.render_job
            .as_ref()?
            .workflow
            .as_ref()
            .filter(|workflow| {
                self.workspace.as_ref().is_some_and(|workspace| {
                    workspace.session == workflow.context.session
                        && workspace.document.project_id() == &workflow.context.project
                })
            })
    }

    pub(super) fn render_session_changed(&mut self) {
        self.render.open = false;
        self.render.error = None;
        self.render.summary = None;
        self.render.cancel_ticket = None;
        if matches!(self.render.flow, Some(Flow::Submitted { .. })) {
            self.render.flow = None;
        }
    }

    pub(super) fn begin_render(&mut self, context: &egui::Context) {
        self.render.requested = false;
        if self.trim.is_some() {
            self.error =
                Some("Finish or cancel Trim preview before rendering the saved edit.".into());
            return;
        }
        if self.slip.is_some() {
            self.error =
                Some("Finish or cancel Slip preview before rendering the saved edit.".into());
            return;
        }
        context.request_repaint();
        if self.render.flow.is_some() || self.dialogs.is_open() {
            return;
        }
        self.render.open = true;
        if self.service.is_busy() {
            self.render.error =
                Some("Wait for the current edit to finish, then choose Render again.".into());
            return;
        }
        if self.current_render().is_some_and(|workflow| {
            !workflow.status.cleanup_confirmed
                || !matches!(
                    workflow.status.stage,
                    WorkflowStage::Idle | WorkflowStage::Finished
                )
        }) {
            return;
        }
        let Some(workspace) = &self.workspace else {
            self.render.error = Some("Open a project before rendering.".into());
            return;
        };
        if workspace.plan.duration().frames() == 0 {
            self.render.error =
                Some("Your edit is empty. Add picture time before rendering.".into());
            return;
        }
        let capture = Capture {
            context: ProjectRenderContext {
                session: workspace.session,
                project: workspace.document.project_id().clone(),
            },
            revision: workspace.document.revision_id().clone(),
            package: workspace.path.clone(),
        };
        match deadpan_cli::render::output_summary(&workspace.document) {
            Ok(summary) => {
                self.render.summary = Some((
                    capture.context.clone(),
                    capture.revision.clone(),
                    format!(
                        "{} × {} · {} · {} frames · SDR H.264 / AAC",
                        summary.raster[0],
                        summary.raster[1],
                        frame_rate_label(summary.frame_rate),
                        summary.frame_count,
                    ),
                ))
            }
            Err(error) => {
                self.render.error = Some(error.to_string());
                return;
            }
        }
        self.cancel_repeats("Render was requested");
        self.pause_playback();
        self.bindings.clear();
        self.render.error = None;
        let proposal = if self.camera.is_some() {
            Some(self.camera_render_edit())
        } else if self.gain.is_some() {
            Some(self.gain_render_edit())
        } else if self.room_tone.is_some() {
            Some(self.room_tone_render_edit())
        } else if self.camera_pending.is_some() {
            Some(Err(
                "Wait for Camera to open before committing its preview.".into(),
            ))
        } else {
            None
        };
        if let Some(proposal) = proposal {
            self.render.flow = Some(Flow::Decision { capture, proposal });
            self.render.prior_focus = context.memory(|memory| memory.focused());
            context.memory_mut(|memory| {
                if let Some(focus) = memory.focused() {
                    memory.surrender_focus(focus);
                }
            });
        } else {
            self.render_picker(capture, None, false, context);
        }
    }

    fn render_picker(
        &mut self,
        capture: Capture,
        edit: Option<PreviewEdit>,
        discard: bool,
        context: &egui::Context,
    ) {
        if !capture.matches(self.workspace.as_deref()) {
            self.render.error = Some(
                "The project changed. Choose Render again to capture the current edit.".into(),
            );
            self.render.flow = None;
            return;
        }
        let save = self.render_movie_suggestion(&capture);
        match self.dialogs.save_movie(save, context) {
            Ok(()) => {
                self.render.flow = Some(Flow::Picker {
                    capture,
                    edit,
                    discard,
                })
            }
            Err(error) => {
                self.render.error = Some(error);
                self.render.flow = None;
            }
        }
    }

    fn render_movie_suggestion(&self, capture: &Capture) -> crate::dialogs::SaveMovie {
        let previous = self
            .render
            .destination
            .as_ref()
            .filter(|(session, _)| *session == capture.context.session)
            .map(|(_, path)| path);
        let directory = previous
            .and_then(|path| path.parent())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                capture
                    .package
                    .parent()
                    .unwrap_or(&capture.package)
                    .join("Exports")
            });
        let stem = capture
            .package
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let name = format!("{stem}-{}.mp4", &suffix[..8]);
        crate::dialogs::SaveMovie { directory, name }
    }

    pub(super) fn receive_render_dialog(&mut self, path: Option<PathBuf>) {
        let flow = self.render.flow.take();
        if let Some(Flow::RecoveryPicker(captured)) = flow {
            self.receive_recovery_dialog(captured, path);
            return;
        }
        let Some(Flow::Picker {
            capture,
            edit,
            discard,
        }) = flow
        else {
            return;
        };
        let Some(path) = path else {
            return;
        };
        if !capture.matches(self.workspace.as_deref())
            || edit.as_ref().is_some_and(|edit| {
                edit.session != capture.context.session || edit.revision != capture.revision
            })
        {
            self.render.error = Some(
                "The project changed while choosing a destination. Choose Render again.".into(),
            );
            return;
        }
        let prepared = (|| {
            let request = deadpan_cli::render::start_request(
                &deadpan_cli::render::RenderContext {
                    project_id: capture.context.project.clone(),
                    revision_id: capture.revision.clone(),
                },
                path.clone(),
                Instant::now(),
            )
            .map_err(|error| error.to_string())?;
            let limits =
                deadpan_cli::render::default_limits().map_err(|error| error.to_string())?;
            let limits = ProjectRenderLimits {
                encode: limits.encode,
                verification: limits.verification,
                media: limits.media,
            };
            let operation = match edit {
                Some(edit) => ProjectRenderOperation::CommitAndStart {
                    edit: Box::new(edit.edit),
                    scope: edit.scope,
                    cursor: edit.cursor,
                    request,
                    limits,
                },
                None => ProjectRenderOperation::Start { request, limits },
            };
            Ok::<_, String>(operation)
        })();
        let operation = match prepared {
            Ok(operation) => operation,
            Err(error) => {
                self.render.error = Some(error);
                return;
            }
        };
        let close_preview =
            discard || self.camera.is_some() || self.gain.is_some() || self.room_tone.is_some();
        let Some(ticket) = self.next_serial() else {
            return;
        };
        match self
            .service
            .submit(ProjectRequest::Render(ProjectRenderRequest {
                ticket,
                context: capture.context.clone(),
                operation,
            })) {
            Ok(()) => {
                self.render.destination = Some((capture.context.session, path));
                self.render.flow = Some(Flow::Submitted {
                    capture,
                    ticket,
                    close_preview,
                });
            }
            Err(error) => self.render.error = Some(error),
        }
    }

    pub(super) fn render_dialog_failed(&mut self, error: String) {
        self.render.flow = None;
        self.render.error = Some(error);
    }

    pub(super) fn reconcile_render(&mut self, context: &egui::Context) {
        if self.current_render().is_some()
            && let Some(error) = self
                .render_job
                .as_ref()
                .and_then(|update| update.service_error.as_ref())
        {
            self.render.error = Some(format!("{} ({})", error.message, error.code));
        }
        if let Some((capture, ticket)) = &self.render.cancel_ticket
            && let Some(outcome) = self
                .render_job
                .as_ref()
                .and_then(|update| update.command.as_ref())
            && outcome.context == *capture
            && outcome.ticket == *ticket
        {
            self.render.error = outcome
                .result
                .as_ref()
                .err()
                .map(|error| format!("{} ({})", error.message, error.code));
            self.render.cancel_ticket = None;
        }
        let Some(Flow::Submitted {
            capture,
            ticket,
            close_preview,
        }) = &self.render.flow
        else {
            return;
        };
        let Some(outcome) = self
            .render_job
            .as_ref()
            .and_then(|update| update.command.as_ref())
        else {
            return;
        };
        if outcome.ticket != *ticket || outcome.context != capture.context {
            return;
        }
        let close =
            *close_preview && (outcome.result.is_ok() || outcome.committed_revision.is_some());
        if let Some(revision) = &outcome.committed_revision
            && let Some((_, summary_revision, _)) = &mut self.render.summary
        {
            summary_revision.clone_from(revision);
        }
        self.render.error = outcome.result.as_ref().err().map(|error| {
            if let Some(revision) = &outcome.committed_revision {
                format!(
                    "Preview saved as {}. Render did not start: {} ({})",
                    revision.as_str(),
                    error.message,
                    error.code
                )
            } else {
                format!("{} ({})", error.message, error.code)
            }
        });
        self.render.flow = None;
        if close {
            self.close_render_preview(context);
        }
    }

    fn close_render_preview(&mut self, context: &egui::Context) {
        self.cancel_camera();
        if self.gain.is_some() {
            self.close_gain(context);
        }
        if self.room_tone.is_some() {
            self.close_room_tone(context);
        }
        self.stop_playback();
        self.request_picture(false);
    }

    /// Called before preview-specific key routers. Native fields and IME keep
    /// their keys; Render never implicitly submits a focused field.
    pub(super) fn render_keyboard(&mut self, context: &egui::Context) -> bool {
        self.restore_render_history_focus(context);
        if self.render.blocking() {
            let events = context.input(|input| input.events.clone());
            let composition_owned = self.ime_composing
                || events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Ime(_)));
            help_scroll::observe_composition(&events, &mut self.ime_composing);
            if composition_owned || self.ime_composing {
                // egui buttons activate directly from raw Enter/Space. IME
                // confirmation must not turn into a recovery or preview action.
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
            return true;
        }
        false
    }

    pub(super) fn render_windows(&mut self, context: &egui::Context) {
        if self.render.history.open && self.render.flow.is_none() {
            self.render_history_window(context);
            return;
        }
        if let Some(Flow::Decision { capture, proposal }) = &self.render.flow {
            let capture = capture.clone();
            let proposal = proposal.clone();
            let mut decision = None;
            let modal = egui::Modal::new(egui::Id::new("render-preview-decision")).show(context, |ui| {
                ui.set_max_width(440.0);
                ui.heading("Render your preview?");
                ui.label("Render uses a saved revision. Choose what happens to this unsaved preview.");
                if let Err(error) = &proposal { ui.colored_label(style::LAVENDER, error); }
                if ui.add_enabled(proposal.is_ok(), egui::Button::new("Commit preview and render")).clicked() { decision = Some(true); }
                if ui.button("Discard preview and render").clicked() { decision = Some(false); }
                if ui.button("Keep editing  ·  Esc").clicked() { self.render.flow = None; }
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
                self.render.flow = None;
            }
            if self.render.flow.is_none() {
                if let Some(focus) = self.render.prior_focus.take() {
                    context.memory_mut(|memory| memory.request_focus(focus));
                }
                context.request_discard("Render preview decision closed");
            }
            if let Some(commit) = decision {
                self.render_picker(
                    capture,
                    if commit {
                        proposal.ok().flatten()
                    } else {
                        None
                    },
                    !commit,
                    context,
                );
            }
            return;
        }
        if self.render.blocking() {
            return;
        }
        if !self.render.open {
            return;
        }
        let mut open = true;
        let mut cancel = None;
        egui::Window::new("Render").id(egui::Id::new("render-status")).open(&mut open)
            .default_width(430.0).resizable(true).scroll([false, true]).show(context, |ui| {
                ui.label("Full edit · automatic MP4");
                if let Some(error) = &self.render.error { ui.colored_label(style::LAVENDER, error); }
                let Some(workflow) = self.current_render() else {
                    ui.weak("Choose Render to save your complete edit as a movie.");
                    return;
                };
                let status = &workflow.status;
                ui.label(format!("Saved revision {}", workflow.revision.as_str()));
                if let Some((capture, revision, summary)) = &self.render.summary
                    && capture == &workflow.context && revision == &workflow.revision { ui.label(summary); }
                ui.heading(stage_label(status.stage, status.outcome, status.observed_movie_commit));
                if let Some(progress) = &status.progress {
                    use deadpan_cli::encoded_render::workflow::WorkflowProgress;
                    match progress {
                        WorkflowProgress::Qualification { completed_frames, total_frames, .. } => { ui.label(format!("Encoder check: {completed_frames} / {total_frames} frames")); }
                        WorkflowProgress::Encoding { completed_frames, total_frames, .. } => { ui.label(format!("{completed_frames} / {total_frames} frames")); }
                        _ => {}
                    }
                }
                for diagnostic in [&status.diagnostic, &status.journal_diagnostic].into_iter().flatten() {
                    ui.label(format!("{} ({})", diagnostic.detail, diagnostic.code));
                }
                if let Some(receipt) = &status.receipt {
                    ui.label(format!("Movie: {}", receipt.movie.display()));
                    ui.label(format!("Local report: {}", receipt.report.display()));
                    if receipt.contains_generated_pictures {
                        ui.weak("This movie contains generated pictures. Review the upload platform's synthetic-content disclosure requirements.");
                    }
                }
                if status.outcome.is_some() && status.outcome != Some(WorkflowOutcome::Published) {
                    for (label, path) in [
                        ("Movie staging path", &status.retained.partial_movie),
                        ("Report staging path", &status.retained.partial_report),
                        ("Published report", &status.retained.published_report),
                    ] {
                        if let Some(path) = path { ui.label(format!("{label}: {}", path.display())); }
                    }
                }
                if status.outcome.is_none() && !status.cancellation_requested && status.stage != WorkflowStage::Unresolved
                    && ui.button("Cancel render").clicked() {
                    cancel = status.identity.clone().map(|identity| (workflow.context.clone(), identity));
                }
                if !status.cleanup_confirmed { ui.weak("Finishing worker cleanup; this render still owns its slot."); }
            });
        self.render.open = open;
        if let Some((context, identity)) = cancel
            && let Some(ticket) = self.next_serial()
        {
            match self
                .service
                .submit(ProjectRequest::Render(ProjectRenderRequest {
                    ticket,
                    context: context.clone(),
                    operation: ProjectRenderOperation::Cancel(identity),
                })) {
                Ok(()) => self.render.cancel_ticket = Some((context, ticket)),
                Err(error) => self.render.error = Some(error),
            }
        }
    }
}

fn stage_label(
    stage: WorkflowStage,
    outcome: Option<WorkflowOutcome>,
    movie_committed: bool,
) -> &'static str {
    if movie_committed && outcome != Some(WorkflowOutcome::Published) {
        return "Movie saved; confirmation needs recovery";
    }
    match outcome {
        Some(WorkflowOutcome::Published) => "Movie exported",
        Some(WorkflowOutcome::PublishedUnconfirmed) => "Movie saved; confirmation needs recovery",
        Some(WorkflowOutcome::Cancelled) => "Render cancelled",
        Some(WorkflowOutcome::NotPublished | WorkflowOutcome::Failed) => {
            "Render failed; movie not published"
        }
        Some(WorkflowOutcome::Unresolved) => "Render needs recovery",
        None => match stage {
            WorkflowStage::Idle => "Ready",
            WorkflowStage::Capturing => "Preparing saved edit",
            WorkflowStage::Qualifying => "Checking automatic encoder",
            WorkflowStage::Encoding => "Rendering movie",
            WorkflowStage::Verifying => "Verifying finished movie",
            WorkflowStage::PreparingPublication
            | WorkflowStage::CommittingReport
            | WorkflowStage::CommittingMovie => "Saving verified movie",
            WorkflowStage::Reconciling => "Checking previous publication",
            WorkflowStage::Cancelling => "Cancelling render",
            WorkflowStage::Releasing => "Finishing worker cleanup",
            WorkflowStage::Finished => "Render finished",
            WorkflowStage::Unresolved => "Render needs recovery",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movie_commit_survives_later_failure_in_the_visible_status() {
        assert_eq!(
            stage_label(
                WorkflowStage::Unresolved,
                Some(WorkflowOutcome::Failed),
                true
            ),
            "Movie saved; confirmation needs recovery"
        );
        assert_eq!(
            stage_label(WorkflowStage::Releasing, None, false),
            "Finishing worker cleanup"
        );
        assert_eq!(
            stage_label(
                WorkflowStage::Finished,
                Some(WorkflowOutcome::Published),
                true
            ),
            "Movie exported"
        );
    }
}
