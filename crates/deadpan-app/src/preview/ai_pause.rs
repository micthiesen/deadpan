//! AI pause pictures in the native workspace: generate for the selected
//! pause, follow progress, preview a Ready candidate in the viewer, then
//! accept or discard it. Ready never edits; Accept is one undoable edit.

use std::sync::Arc;
use std::time::Duration;

use deadpan_core::{NodeId, NodeKind, ProjectFrame};
use deadpan_jobs::RequestId;

use super::*;
use crate::navigation::AiAction;
use crate::project::generation::{
    Candidate, CandidatePreview, GenerationOperation, Job, Outcome, Update,
};

#[derive(Default)]
pub(super) struct State {
    update: Option<Update>,
    ticket: u64,
    /// The preview command awaiting its reply.
    pending_preview: Option<(u64, RequestId)>,
    /// The admitted preview the viewer shows instead of the committed edit.
    preview: Option<Arc<CandidatePreview>>,
    /// Independent command tickets whose refusal the editor should show.
    awaiting: Option<u64>,
    /// Context captured at the first `,a` ancestor.
    pub prefix: Option<Result<Target, String>>,
    /// Context captured when command entry opened.
    pub command: Option<Result<Target, String>>,
}

/// The session, revision and selection an AI command was entered with.
#[derive(Clone, Debug)]
pub(super) struct Target {
    session: u64,
    revision: deadpan_core::RevisionId,
    hold: Option<NodeId>,
    candidate: Option<(NodeId, RequestId)>,
    cursor: ProjectFrame,
    scope: SequenceScope,
}

impl State {
    fn next_ticket(&mut self) -> u64 {
        self.ticket = self.ticket.wrapping_add(1).max(1);
        self.ticket
    }

    /// A generation job is still running for this session.
    pub(super) fn generation_running(&self) -> bool {
        self.update
            .as_ref()
            .and_then(|update| update.job.as_ref())
            .is_some_and(|job| job.running())
    }

    #[cfg(feature = "ui-harness")]
    pub(crate) fn job(&self) -> Option<&Job> {
        self.update.as_ref()?.job.as_ref()
    }

    #[cfg(feature = "ui-harness")]
    pub(crate) fn candidate_count(&self) -> usize {
        self.update
            .as_ref()
            .map_or(0, |update| update.candidates.len())
    }

    #[cfg(feature = "ui-harness")]
    pub(crate) fn preview_request(&self) -> Option<&RequestId> {
        self.preview.as_ref().map(|preview| preview.request())
    }
}

fn elapsed(job: &Job) -> String {
    let seconds = job.started.elapsed().as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

impl DeadpanApp {
    /// The selected pause in Your edit, when AI pictures can target it.
    pub(super) fn ai_hold(&self) -> Option<NodeId> {
        if self.view != View::Sequence
            || self.event_focused()
            || self.sound_focused()
            || self.scoped.is_some()
        {
            return None;
        }
        let workspace = self.workspace.as_ref()?;
        let node = self.selected_beat.as_ref()?;
        matches!(
            workspace.document.nodes().get(node)?.kind,
            NodeKind::Hold { .. }
        )
        .then(|| node.clone())
    }

    fn ai_running(&self) -> Option<&Job> {
        self.ai
            .update
            .as_ref()?
            .job
            .as_ref()
            .filter(|job| job.running())
    }

    fn ai_candidate(&self, hold: &NodeId) -> Option<&Candidate> {
        self.ai.update.as_ref()?.candidates.get(hold)
    }

    /// The context an AI command acts on, captured at command entry or at the
    /// first `,a` ancestor. Absence is a real result: a later selection or
    /// service reply cannot supply a target that was missing on entry.
    pub(super) fn ai_capture(&self) -> Result<Target, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open a project before using AI pictures.")?;
        let hold = self.ai_hold();
        let candidate = hold.as_ref().and_then(|hold| {
            self.ai_candidate(hold)
                .map(|candidate| (hold.clone(), candidate.request.clone()))
        });
        Ok(Target {
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            hold,
            candidate,
            cursor: ProjectFrame(i64::try_from(self.sequence_cursor).unwrap_or(i64::MAX)),
            scope: self.sequence_scope.clone(),
        })
    }

    /// Run an AI action with its captured context.
    pub(super) fn ai_action(&mut self, action: AiAction, target: Option<Result<Target, String>>) {
        let result = (|| {
            if action == AiAction::Cancel {
                return self.ai_cancel();
            }
            if action == AiAction::Preview
                && (self.ai.preview.is_some() || self.ai.pending_preview.is_some())
            {
                self.ai_stop_preview();
                self.message = Some("Showing your edit again.".into());
                return Ok(());
            }
            let target = target.ok_or("Enter the AI command again to capture its pause.")??;
            match action {
                AiAction::Generate => self.ai_generate(target),
                AiAction::Preview => self.ai_preview(target),
                AiAction::Accept => self.ai_accept(target),
                AiAction::Discard => self.ai_discard(target),
                AiAction::Cancel => unreachable!("cancel handled above"),
            }
        })();
        if let Err(error) = result {
            self.error = Some(error);
        }
    }

    /// Non-editing AI commands leave Camera, Repeat chains, playback and
    /// pending keys untouched.
    fn ai_submit(&mut self, operation: GenerationOperation) -> Option<u64> {
        let ticket = self.ai.next_ticket();
        let operation = match operation {
            GenerationOperation::Start {
                session,
                revision,
                hold,
                ..
            } => GenerationOperation::Start {
                ticket,
                session,
                revision,
                hold,
            },
            GenerationOperation::Cancel { session, job, .. } => GenerationOperation::Cancel {
                ticket,
                session,
                job,
            },
            GenerationOperation::Preview {
                session,
                revision,
                request,
                ..
            } => GenerationOperation::Preview {
                ticket,
                session,
                revision,
                request,
            },
            GenerationOperation::Discard {
                session, request, ..
            } => GenerationOperation::Discard {
                ticket,
                session,
                request,
            },
            accept @ GenerationOperation::Accept { .. } => accept,
        };
        match self.service.submit(ProjectRequest::Generation(operation)) {
            Ok(()) => {
                self.error = None;
                self.ai.awaiting = Some(ticket);
                Some(ticket)
            }
            Err(error) => {
                self.error = Some(error);
                None
            }
        }
    }

    fn ai_generate(&mut self, target: Target) -> Result<(), String> {
        let hold = target.hold.ok_or(
            "AI pictures fill a pause. Select a pause beat in Your edit, then press the key again.",
        )?;
        if self.ai_running().is_some() {
            return Err(
                "An AI pause is already generating. Cancel it with :cancel-ai first.".into(),
            );
        }
        if !self.macro_request_allowed(&ProjectRequest::Generation(GenerationOperation::Start {
            ticket: 0,
            session: target.session,
            revision: target.revision.clone(),
            hold: hold.clone(),
        })) {
            return Ok(());
        }
        self.ai_stop_preview();
        self.ai_submit(GenerationOperation::Start {
            ticket: 0,
            session: target.session,
            revision: target.revision,
            hold,
        });
        Ok(())
    }

    fn ai_cancel(&mut self) -> Result<(), String> {
        let job = self
            .ai_running()
            .ok_or("No AI pause is generating.")?
            .ticket;
        let session = self
            .workspace
            .as_ref()
            .ok_or("Open a project first.")?
            .session;
        self.ai_submit(GenerationOperation::Cancel {
            ticket: 0,
            session,
            job,
        });
        Ok(())
    }

    fn ai_target_candidate(target: &Target) -> Result<(NodeId, RequestId), String> {
        target.candidate.clone().ok_or_else(|| {
            if target.hold.is_some() {
                "This pause has no Ready AI pictures. Generate them first.".into()
            } else {
                "Select the pause with Ready AI pictures first.".into()
            }
        })
    }

    fn ai_preview(&mut self, target: Target) -> Result<(), String> {
        let (_, request) = Self::ai_target_candidate(&target)?;
        if let Some(ticket) = self.ai_submit(GenerationOperation::Preview {
            ticket: 0,
            session: target.session,
            revision: target.revision,
            request: request.clone(),
        }) {
            self.ai.pending_preview = Some((ticket, request));
        }
        Ok(())
    }

    fn ai_accept(&mut self, target: Target) -> Result<(), String> {
        let (hold, request) = Self::ai_target_candidate(&target)?;
        self.ai_stop_preview();
        // Acceptance is an ordinary edit with the ordinary command path.
        self.submit(ProjectRequest::Generation(GenerationOperation::Accept {
            session: target.session,
            revision: target.revision,
            request,
            hold,
            cursor: target.cursor,
            scope: target.scope,
        }));
        Ok(())
    }

    fn ai_discard(&mut self, target: Target) -> Result<(), String> {
        let (_, request) = Self::ai_target_candidate(&target)?;
        self.ai_stop_preview();
        self.ai_submit(GenerationOperation::Discard {
            ticket: 0,
            session: target.session,
            request,
        });
        Ok(())
    }

    /// Leave preview without an edit. True when the viewer must change.
    pub(super) fn ai_stop_preview(&mut self) -> bool {
        self.ai.pending_preview = None;
        if self.ai.preview.take().is_some() {
            self.request_picture(false);
            true
        } else {
            false
        }
    }

    /// Whether Escape belongs to another editor state that it should clear
    /// first: command/help entry, pending keys, a register choice, a macro
    /// recording, or a Visual selection.
    pub(super) fn escape_owned_elsewhere(&self) -> bool {
        self.command_open
            || self.help_open
            || !self.bindings.pending().is_empty()
            || self.copied.selected().is_some()
            || self.macros.recording()
            || self.moment.active
            || self.edit_selection() != navigation::EditSelection::None
    }

    /// Admit the service's generation state. True when the viewer must
    /// change because a preview started or ended.
    pub(super) fn receive_ai(&mut self, update: Option<Update>) -> bool {
        let workspace = self
            .workspace
            .as_ref()
            .map(|workspace| (workspace.session, workspace.document.revision_id().clone()));
        self.ai.update = update.filter(|update| {
            workspace
                .as_ref()
                .is_some_and(|(session, _)| *session == update.session)
        });
        let mut changed = false;
        let reply = self
            .ai
            .update
            .as_ref()
            .and_then(|update| update.reply.clone());
        // Commands are serialized through one service mailbox, so a later
        // ticket's reply means every earlier command was already answered.
        if let Some((answered, refusal)) = &reply {
            if self
                .ai
                .awaiting
                .is_some_and(|awaiting| *answered >= awaiting)
            {
                if self.ai.awaiting == Some(*answered)
                    && let Some(refusal) = refusal
                {
                    self.error = Some(refusal.clone());
                }
                self.ai.awaiting = None;
            }
            if let Some((ticket, request)) = self.ai.pending_preview.clone()
                && *answered >= ticket
            {
                self.ai.pending_preview = None;
                let issued = self.ai.update.as_ref().and_then(|update| {
                    update
                        .preview
                        .clone()
                        .filter(|preview| preview.request() == &request)
                });
                if let Some(preview) = issued {
                    // Show the pause itself when the cursor is elsewhere.
                    let range = preview.range();
                    if let (Ok(start), Ok(end)) =
                        (u64::try_from(range.start().0), u64::try_from(range.end().0))
                        && !(start..end).contains(&self.sequence_cursor)
                    {
                        self.sequence_cursor = start;
                    }
                    self.ai.preview = Some(preview);
                    self.message = Some(
                        "Previewing AI pictures. Nothing is saved; accept with :accept-ai or press Esc to return."
                            .into(),
                    );
                    changed = true;
                }
            }
        }
        // A preview belongs to one session, revision and offered candidate.
        let valid = self.ai.preview.as_ref().is_none_or(|preview| {
            workspace.as_ref().is_some_and(|(session, revision)| {
                preview.session() == *session && preview.base() == revision
            }) && self.ai.update.as_ref().is_some_and(|update| {
                update
                    .candidates
                    .get(preview.hold())
                    .is_some_and(|candidate| &candidate.request == preview.request())
            })
        });
        if !valid {
            self.ai.preview = None;
            changed = true;
        }
        if workspace.is_none() {
            self.ai.pending_preview = None;
            self.ai.awaiting = None;
        }
        changed
    }

    /// The candidate picture at the edit cursor while previewing.
    pub(super) fn ai_picture_work(&self, picture: Option<u64>) -> Option<crate::worker::Work> {
        let preview = self.ai.preview.as_ref()?;
        if self.view != View::Sequence {
            return None;
        }
        let workspace = self.workspace.as_ref()?;
        let frame = picture
            .unwrap_or(self.sequence_cursor)
            .min(self.sequence_length().saturating_sub(1));
        Some(crate::worker::Work::Candidate {
            base: Arc::clone(workspace),
            candidate: Arc::clone(preview),
            frame: ProjectFrame(i64::try_from(frame).ok()?),
        })
    }

    /// The inspector section for a selected pause.
    pub(super) fn ai_inspector(&mut self, ui: &mut egui::Ui, ready: bool) {
        let Some(hold) = self.ai_hold() else {
            return;
        };
        ui.add_space(8.0);
        ui.label(style::section_title("AI PICTURES", false));
        let generate_key = self.editor_key(EditorKey::GenerateAi);
        let job = self
            .ai
            .update
            .as_ref()
            .and_then(|update| update.job.clone())
            .filter(|job| job.hold == hold);
        let other_running = self
            .ai_running()
            .is_some_and(|running| running.hold != hold);
        if let Some(job) = job.as_ref().filter(|job| job.running()) {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(style::semibold("In progress"));
                ui.label(
                    egui::RichText::new(elapsed(job))
                        .monospace()
                        .color(style::MUTED),
                );
            });
            match job.phase.steps().filter(|(_, total)| *total > 0) {
                Some((completed, total)) => {
                    ui.label(format!(
                        "{} · step {completed} of {total}",
                        job.phase.label()
                    ));
                    ui.add(
                        egui::ProgressBar::new(completed as f32 / total as f32)
                            .desired_height(4.0)
                            .fill(style::LAVENDER),
                    );
                }
                None => {
                    ui.label(job.phase.label());
                }
            }
            ui.label(
                egui::RichText::new("The current picture stays until you accept.")
                    .size(12.0)
                    .color(style::MUTED),
            );
            // Escape never cancels a long generation; cancellation is explicit.
            if ui
                .add_enabled(
                    !self.service.is_busy(),
                    style::row_action(ui, "Cancel generation", ":cancel-ai"),
                )
                .clicked()
            {
                self.ai_action(AiAction::Cancel, None);
            }
            ui.ctx().request_repaint_after(Duration::from_millis(250));
            return;
        }
        if let Some(candidate) = self.ai_candidate(&hold).cloned() {
            ui.colored_label(
                style::LAVENDER,
                format!("Ready · {} f of generated pictures", candidate.frames),
            );
            ui.label(
                egui::RichText::new("Ready does not change your edit.")
                    .size(12.0)
                    .color(style::MUTED),
            );
            let previewing = self.ai.preview.is_some();
            let (label, key) = if previewing {
                ("Stop preview", "Esc")
            } else {
                ("Preview", ":preview-ai")
            };
            if ui
                .add_enabled(ready, style::row_action(ui, label, key))
                .on_hover_text("Show the generated pictures in the viewer at the edit cursor. Move through the pause to see every frame; nothing is saved.")
                .clicked()
            {
                self.ai_action(AiAction::Preview, Some(self.ai_capture()));
            }
            if ui
                .add_enabled(
                    ready,
                    style::row_action(ui, "Accept", ":accept-ai").fill(style::SELECTED),
                )
                .on_hover_text("Make these pictures the pause's picture as one undoable edit.")
                .clicked()
            {
                self.ai_action(AiAction::Accept, Some(self.ai_capture()));
            }
            if ui
                .add_enabled(ready, style::row_action(ui, "Discard", ":discard-ai"))
                .clicked()
            {
                self.ai_action(AiAction::Discard, Some(self.ai_capture()));
            }
            ui.label(
                egui::RichText::new("Undo restores the previous picture.")
                    .size(12.0)
                    .color(style::MUTED),
            );
            return;
        }
        match job.as_ref().and_then(|job| job.outcome.as_ref()) {
            Some(Outcome::Unavailable(reason)) => {
                ui.colored_label(style::WARNING, "AI pauses are unavailable on this Mac");
                ui.label(egui::RichText::new(reason).size(12.0).color(style::MUTED));
            }
            Some(Outcome::Failed(reason)) => {
                ui.colored_label(style::ERROR, "Generation failed; the pause is unchanged");
                ui.label(egui::RichText::new(reason).size(12.0).color(style::MUTED));
            }
            Some(Outcome::Cancelled) => {
                ui.label(
                    egui::RichText::new("Cancelled. The pause is unchanged.")
                        .size(12.0)
                        .color(style::MUTED),
                );
            }
            Some(Outcome::Ready(_)) | None => {}
        }
        self.ai_model_offer(ui);
        if ui
            .add_enabled(
                ready && !other_running,
                style::row_action(ui, "Generate AI pictures", &generate_key),
            )
            .on_hover_text("Fill this pause from the pictures on both sides with the local model. It runs in the background; nothing changes until you accept.")
            .clicked()
        {
            self.ai_action(AiAction::Generate, Some(self.ai_capture()));
        }
        ui.label(
            egui::RichText::new(if other_running {
                "Another pause is generating; cancel it first."
            } else {
                "Proposes pictures from both sides of this pause. Nothing changes until you accept."
            })
            .size(12.0)
            .color(style::MUTED),
        );
    }

    /// Without the installed AI model pack, its size and the way to install
    /// it. Generation still reports exactly why it cannot run.
    fn ai_model_offer(&mut self, ui: &mut egui::Ui) {
        let pack_id = deadpan_cli::generation::runtime::BRIDGE_PACK;
        self.models.manager.refresh_if_stale(Duration::from_secs(5));
        if self.models.manager.installed(pack_id) {
            return;
        }
        let Some(pack) = self.models.manager.pack(pack_id) else {
            return;
        };
        let installing = self.models.manager.installing(pack_id).is_some();
        if !installing {
            let partial = match self.models.manager.state(pack_id) {
                Some(Ok(deadpan_models::packs::PackState::Partial { bytes })) => {
                    format!(" ({} downloaded)", crate::model_packs::format_bytes(*bytes))
                }
                _ => String::new(),
            };
            let licenses = match pack
                .licenses
                .iter()
                .filter(|license| license.acceptance_required)
                .count()
            {
                0 => "no license to accept".to_owned(),
                1 => "one license to accept".to_owned(),
                2 => "two licenses to accept".to_owned(),
                count => format!("{count} licenses to accept"),
            };
            ui.label(
                egui::RichText::new(format!(
                    "AI pictures need the AI model pack: a {} download{partial}, {licenses}, about {} of memory while running.",
                    crate::model_packs::format_bytes(pack.total_bytes()),
                    crate::model_packs::format_bytes(pack.memory_bytes)
                ))
                .size(12.0)
                .color(style::MUTED),
            );
        }
        if self.model_pack_offer(ui, pack_id, "Install AI models…", ":models") {
            self.open_models(Some(pack_id), ui.ctx());
        }
    }

    /// A status row while a job runs or a preview is shown.
    pub(super) fn ai_footer(&self, ui: &mut egui::Ui) {
        let previewing = self.ai.preview.is_some();
        let running = self.ai_running();
        if !previewing && running.is_none() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if previewing {
                ui.colored_label(style::LAVENDER, "AI PREVIEW · NOT SAVED");
                style::key_hint(ui, ":accept-ai", "accept");
                style::key_hint(ui, ":discard-ai", "discard");
                style::key_hint(ui, "Esc", "show your edit");
            }
            if let Some(job) = running {
                if previewing {
                    ui.separator();
                }
                ui.spinner();
                let steps = job
                    .phase
                    .steps()
                    .map(|(completed, total)| format!(" {completed}/{total}"))
                    .unwrap_or_default();
                ui.colored_label(
                    style::LAVENDER,
                    format!("AI pause · {}{steps} · {}", job.phase.label(), elapsed(job)),
                );
                style::key_hint(ui, ":cancel-ai", "cancel");
                ui.ctx().request_repaint_after(Duration::from_millis(250));
            }
        });
    }
}
