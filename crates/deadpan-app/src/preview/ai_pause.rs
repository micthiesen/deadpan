//! AI pause pictures in the native workspace: generate one or several
//! variants for the selected pause, follow progress, choose a variant by its
//! thumbnail, preview its pictures in the viewer and audition them with the
//! pause's own sound, then accept or durably discard it. Ready never edits;
//! Accept is one undoable edit.

use std::sync::Arc;
use std::time::Duration;

use deadpan_core::{NodeId, NodeKind, ProjectFrame};
use deadpan_jobs::{AttemptId, RequestId};

use super::*;
use crate::navigation::{AiAction, VariantChoice};
use crate::project::generation::{
    Candidate, CandidatePreview, GenerationOperation, Job, Outcome, Update,
};

/// Logical height of a variant thumbnail in the inspector.
const VARIANT_THUMBNAIL_HEIGHT: f32 = 40.0;

/// A preview command awaiting its reply.
#[derive(Clone, Debug)]
struct PendingPreview {
    ticket: u64,
    request: RequestId,
    attempt: AttemptId,
    /// Start the looped audition once the preview is admitted.
    audition: bool,
}

#[derive(Default)]
pub(super) struct State {
    update: Option<Update>,
    ticket: u64,
    /// The preview command awaiting its reply.
    pending_preview: Option<PendingPreview>,
    /// An admitted preview whose looped audition starts on the next frame.
    audition_pending: bool,
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
    /// The selected pause's offered variants and the chosen one, as shown.
    candidate: Option<Candidate>,
    /// Whether the AI audition was playing, before command entry paused it.
    auditioning: bool,
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

    /// Offered Ready variants across every pause.
    #[cfg(feature = "ui-harness")]
    pub(crate) fn variant_count(&self) -> usize {
        self.update.as_ref().map_or(0, |update| {
            update
                .candidates
                .values()
                .map(|candidate| candidate.variants.len())
                .sum()
        })
    }

    /// The chosen variant's 1-based number and attempt, for the only
    /// candidate offered.
    #[cfg(feature = "ui-harness")]
    pub(crate) fn chosen_variant(&self) -> Option<(usize, AttemptId)> {
        let candidate = self.update.as_ref()?.candidates.values().next()?;
        Some((candidate.selected_index() + 1, candidate.selected.clone()))
    }

    #[cfg(feature = "ui-harness")]
    pub(crate) fn preview_request(&self) -> Option<&RequestId> {
        self.preview.as_ref().map(|preview| preview.request())
    }

    #[cfg(feature = "ui-harness")]
    pub(crate) fn preview_attempt(&self) -> Option<&AttemptId> {
        self.preview.as_ref().map(|preview| preview.attempt())
    }

    /// The admitted preview's audition content identity.
    pub(super) fn preview_content(&self) -> Option<&deadpan_playback::ContentIdentity> {
        self.preview
            .as_ref()
            .map(|preview| &preview.audio().content)
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
        let candidate = hold
            .as_ref()
            .and_then(|hold| self.ai_candidate(hold).cloned());
        Ok(Target {
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            hold,
            candidate,
            auditioning: self.ai_auditioning(),
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
                AiAction::Generate { variants } => self.ai_generate(target, variants),
                AiAction::Choose(choice) => self.ai_choose(target, choice),
                AiAction::Preview => self.ai_preview(target, false),
                AiAction::Audition => self.ai_audition(target),
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
                variants,
                ..
            } => GenerationOperation::Start {
                ticket,
                session,
                revision,
                hold,
                variants,
            },
            GenerationOperation::Cancel { session, job, .. } => GenerationOperation::Cancel {
                ticket,
                session,
                job,
            },
            GenerationOperation::Select {
                session,
                request,
                attempt,
                ..
            } => GenerationOperation::Select {
                ticket,
                session,
                request,
                attempt,
            },
            GenerationOperation::Preview {
                session,
                revision,
                request,
                attempt,
                draft,
                ..
            } => GenerationOperation::Preview {
                ticket,
                session,
                revision,
                request,
                attempt,
                draft,
            },
            GenerationOperation::Discard {
                session,
                request,
                attempt,
                ..
            } => GenerationOperation::Discard {
                ticket,
                session,
                request,
                attempt,
            },
            GenerationOperation::DismissInterrupted {
                session,
                request,
                attempt,
                ..
            } => GenerationOperation::DismissInterrupted {
                ticket,
                session,
                request,
                attempt,
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

    fn ai_generate(&mut self, target: Target, variants: u8) -> Result<(), String> {
        let hold = target.hold.ok_or(
            "AI pictures fill a pause. Select a pause beat in Your edit, then press the key again.",
        )?;
        if self.ai_running().is_some() {
            return Err(
                "An AI pause is already generating. Cancel it with :cancel-ai first.".into(),
            );
        }
        let start = GenerationOperation::Start {
            ticket: 0,
            session: target.session,
            revision: target.revision,
            hold,
            variants,
        };
        if !self.macro_request_allowed(&ProjectRequest::Generation(start.clone())) {
            return Ok(());
        }
        self.ai_submit(start);
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

    /// The Jobs panel's Cancel: the same request as `:cancel-ai`.
    pub(super) fn ai_cancel_running(&mut self) -> Result<(), String> {
        self.ai_cancel()
    }

    /// Interrupted attempts offered for retry, for the published session.
    pub(super) fn ai_interrupted(&self) -> Vec<crate::project::generation::Interrupted> {
        let session = self.workspace.as_ref().map(|workspace| workspace.session);
        self.ai
            .update
            .as_ref()
            .filter(|update| Some(update.session) == session)
            .map(|update| update.interrupted.as_ref().clone())
            .unwrap_or_default()
    }

    /// Why earlier discards of interrupted attempts could not be read.
    pub(super) fn ai_interrupted_warning(&self) -> Option<String> {
        let session = self.workspace.as_ref().map(|workspace| workspace.session);
        self.ai
            .update
            .as_ref()
            .filter(|update| Some(update.session) == session)
            .and_then(|update| update.interrupted_warning.clone())
    }

    /// Retry an interrupted attempt: generate one variant for its pause, as
    /// `:generate` does with that pause selected. Unchanged boundary
    /// pictures add an attempt to the same request (reusing its validated
    /// inputs); otherwise a new request replaces it.
    pub(super) fn ai_retry_interrupted(&mut self, hold: &str) -> Result<(), String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open a project before using AI pictures.")?;
        let hold = NodeId::new(hold.to_owned()).map_err(|error| error.to_string())?;
        let candidate = self.ai_candidate(&hold).cloned();
        let target = Target {
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            hold: Some(hold),
            candidate,
            auditioning: false,
            cursor: ProjectFrame(i64::try_from(self.sequence_cursor).unwrap_or(i64::MAX)),
            scope: self.sequence_scope.clone(),
        };
        self.ai_generate(target, 1)
    }

    /// Durably stop offering an interrupted attempt.
    pub(super) fn ai_dismiss_interrupted(
        &mut self,
        item: &crate::project::generation::Interrupted,
    ) -> Result<(), String> {
        let session = self
            .workspace
            .as_ref()
            .ok_or("Open a project first.")?
            .session;
        self.ai_submit(GenerationOperation::DismissInterrupted {
            ticket: 0,
            session,
            request: item.request.clone(),
            attempt: item.attempt.clone(),
        })
        .map(|_| ())
        .ok_or_else(|| {
            self.error
                .clone()
                .unwrap_or_else(|| "The project is busy.".into())
        })
    }

    fn ai_target_candidate(target: &Target) -> Result<&Candidate, String> {
        target.candidate.as_ref().ok_or_else(|| {
            if target.hold.is_some() {
                "This pause has no Ready AI pictures. Generate them first.".into()
            } else {
                "Select the pause with Ready AI pictures first.".into()
            }
        })
    }

    /// Choose another offered variant. While previewing, the viewer shows the
    /// newly chosen one (and an audition in progress continues with it).
    fn ai_choose(&mut self, target: Target, choice: VariantChoice) -> Result<(), String> {
        let candidate = Self::ai_target_candidate(&target)?;
        let count = candidate.variants.len();
        let current = candidate.selected_index();
        let index = match choice {
            VariantChoice::Next => (current + 1) % count,
            VariantChoice::Previous => (current + count - 1) % count,
            VariantChoice::Number(number) => {
                let index = usize::from(number) - 1;
                if index >= count {
                    return Err(format!(
                        "This pause has {count} AI variant{}; choose 1 to {count}.",
                        if count == 1 { "" } else { "s" }
                    ));
                }
                index
            }
        };
        let attempt = candidate.variants[index].attempt.clone();
        let request = candidate.request.clone();
        let previewing = self
            .ai
            .preview
            .as_ref()
            .is_some_and(|preview| preview.request() == &request)
            || self.ai.pending_preview.is_some();
        if previewing {
            let auditioning = self.ai_auditioning();
            self.ai_request_preview(&target, request, attempt, auditioning);
        } else {
            self.ai_submit(GenerationOperation::Select {
                ticket: 0,
                session: target.session,
                request,
                attempt,
            });
        }
        self.message = Some(format!("Chose AI variant {} of {count}.", index + 1));
        Ok(())
    }

    fn ai_request_preview(
        &mut self,
        target: &Target,
        request: RequestId,
        attempt: AttemptId,
        audition: bool,
    ) {
        // The same counter as every other proposed edit's draft identity.
        let Some(draft) = self.next_serial() else {
            return;
        };
        if let Some(ticket) = self.ai_submit(GenerationOperation::Preview {
            ticket: 0,
            session: target.session,
            revision: target.revision.clone(),
            request: request.clone(),
            attempt: attempt.clone(),
            draft,
        }) {
            self.ai.pending_preview = Some(PendingPreview {
                ticket,
                request,
                attempt,
                audition,
            });
        }
    }

    fn ai_preview(&mut self, target: Target, audition: bool) -> Result<(), String> {
        let candidate = Self::ai_target_candidate(&target)?;
        let (request, attempt) = (candidate.request.clone(), candidate.selected.clone());
        self.ai_request_preview(&target, request, attempt, audition);
        Ok(())
    }

    /// The admitted preview's proposed-acceptance audition snapshot, when it
    /// belongs to `workspace`'s exact session and revision.
    pub(super) fn ai_preview_audio(
        &self,
        workspace: &Workspace,
    ) -> Option<Arc<deadpan_playback::Snapshot>> {
        let preview = self.ai.preview.as_ref()?;
        (self.view == View::Sequence
            && preview.session() == workspace.session
            && preview.base() == workspace.document.revision_id())
        .then(|| Arc::clone(preview.audio()))
    }

    /// Whether playback is the AI preview's own audition.
    fn ai_auditioning(&self) -> bool {
        self.transport.as_ref().is_some_and(|run| {
            self.ai
                .preview_content()
                .is_some_and(|content| &run.content == content)
        })
    }

    /// Loop the pause in context with the chosen variant's pictures and the
    /// pause's own sound. Previews first when needed. Issued while that loop
    /// was playing (captured before command entry paused it), it pauses.
    fn ai_audition(&mut self, target: Target) -> Result<(), String> {
        if target.auditioning {
            self.pause_playback();
            self.message = Some(
                "Paused the AI audition. :audition-ai loops it again; Space plays on from here."
                    .into(),
            );
            return Ok(());
        }
        let candidate = Self::ai_target_candidate(&target)?;
        let shown = self.ai.preview.as_ref().is_some_and(|preview| {
            preview.request() == &candidate.request && preview.attempt() == &candidate.selected
        });
        if shown {
            self.audition_selection();
            return Ok(());
        }
        self.ai_preview(target, true)
    }

    fn ai_accept(&mut self, target: Target) -> Result<(), String> {
        let candidate = Self::ai_target_candidate(&target)?;
        let (hold, request, attempt) = (
            candidate.hold.clone(),
            candidate.request.clone(),
            candidate.selected.clone(),
        );
        self.ai_stop_preview();
        // Acceptance is an ordinary edit with the ordinary command path.
        self.submit(ProjectRequest::Generation(GenerationOperation::Accept {
            session: target.session,
            revision: target.revision,
            request,
            attempt,
            hold,
            cursor: target.cursor,
            scope: target.scope,
        }));
        Ok(())
    }

    fn ai_discard(&mut self, target: Target) -> Result<(), String> {
        let candidate = Self::ai_target_candidate(&target)?;
        let (request, attempt) = (candidate.request.clone(), candidate.selected.clone());
        self.ai_stop_preview();
        self.ai_submit(GenerationOperation::Discard {
            ticket: 0,
            session: target.session,
            request,
            attempt,
        });
        Ok(())
    }

    /// Start the looped audition an admitted preview asked for. Runs in the
    /// frame's dispatch pass, after the preview's picture request.
    pub(super) fn dispatch_ai_audition(&mut self) {
        if !std::mem::take(&mut self.ai.audition_pending) || self.ai.preview.is_none() {
            return;
        }
        if !self.ai_auditioning() {
            self.stop_playback();
            self.audition_selection();
        }
    }

    /// Leave preview without an edit, stopping its audition. True when the
    /// viewer must change.
    pub(super) fn ai_stop_preview(&mut self) -> bool {
        self.ai.pending_preview = None;
        self.ai.audition_pending = false;
        if self.ai_auditioning() {
            self.stop_playback();
        }
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
            if let Some(pending) = self.ai.pending_preview.clone()
                && *answered >= pending.ticket
            {
                self.ai.pending_preview = None;
                let issued = self.ai.update.as_ref().and_then(|update| {
                    update.preview.clone().filter(|preview| {
                        preview.request() == &pending.request
                            && preview.attempt() == &pending.attempt
                    })
                });
                if let Some(preview) = issued {
                    // A different variant's audition cannot continue with
                    // these pictures; it restarts with them instead.
                    let auditioning = self.ai_auditioning();
                    if auditioning {
                        self.stop_playback();
                    }
                    // Show the pause itself when the cursor is elsewhere.
                    let range = preview.range();
                    if let (Ok(start), Ok(end)) =
                        (u64::try_from(range.start().0), u64::try_from(range.end().0))
                        && !(start..end).contains(&self.sequence_cursor)
                    {
                        self.sequence_cursor = start;
                    }
                    let number = self.ai.update.as_ref().and_then(|update| {
                        let candidate = update.candidates.get(preview.hold())?;
                        let index = candidate
                            .variants
                            .iter()
                            .position(|variant| &variant.attempt == preview.attempt())?;
                        Some((index + 1, candidate.variants.len()))
                    });
                    self.ai.preview = Some(preview);
                    self.ai.audition_pending = pending.audition || auditioning;
                    self.message = Some(match number {
                        Some((number, count)) if count > 1 => format!(
                            "Previewing AI variant {number} of {count}. Nothing is saved; :audition-ai plays it with the pause's sound, :accept-ai keeps it, Esc returns."
                        ),
                        _ => "Previewing AI pictures. Nothing is saved; :audition-ai plays them with the pause's sound, :accept-ai keeps them, Esc returns."
                            .into(),
                    });
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
                    .is_some_and(|candidate| {
                        &candidate.request == preview.request()
                            && candidate
                                .variants
                                .iter()
                                .any(|variant| &variant.attempt == preview.attempt())
                    })
            })
        });
        if !valid {
            if self.ai_auditioning() {
                self.stop_playback();
            }
            self.ai.preview = None;
            self.ai.audition_pending = false;
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

    /// The inspector section for a selected pause: progress while a job
    /// runs, the offered variants with thumbnails and their actions, and
    /// Generate.
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
        let running = job.as_ref().is_some_and(Job::running);
        if let Some(job) = job.as_ref().filter(|job| job.running()) {
            ui.horizontal(|ui| {
                crate::preview::accessibility::busy(ui);
                ui.label(style::semibold(if job.variants > 1 {
                    format!("Variant {} of {}", job.variant, job.variants)
                } else {
                    "In progress".to_owned()
                }));
                ui.label(egui::RichText::new(elapsed(job)).monospace().weak());
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
                    .weak(),
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
        }
        let note = job
            .as_ref()
            .filter(|job| !job.running())
            .and_then(|job| job.note.clone());
        if let Some(candidate) = self.ai_candidate(&hold).cloned() {
            if running {
                ui.add_space(6.0);
            }
            if let Some(note) = &note {
                ui.label(egui::RichText::new(note).size(12.0).weak());
            }
            self.ai_variants(ui, ready, &candidate);
            if !running
                && ui
                    .add_enabled(
                        ready && !other_running,
                        style::row_action(ui, "Generate another", &generate_key),
                    )
                    .on_hover_text("Generate one more variant from a new seed; :generate N makes several. The variants above stay offered.")
                    .clicked()
            {
                self.ai_action(
                    AiAction::Generate { variants: 1 },
                    Some(self.ai_capture()),
                );
            }
            return;
        }
        if running {
            return;
        }
        match job.as_ref().and_then(|job| job.outcome.as_ref()) {
            Some(Outcome::Unavailable(reason)) => {
                ui.colored_label(style::WARNING, "AI pauses are unavailable on this Mac");
                ui.label(egui::RichText::new(reason).size(12.0).weak());
            }
            Some(Outcome::Failed(reason)) => {
                ui.colored_label(style::ERROR, "Generation failed; the pause is unchanged");
                ui.label(egui::RichText::new(reason).size(12.0).weak());
            }
            Some(Outcome::Cancelled) => {
                ui.label(
                    egui::RichText::new("Cancelled. The pause is unchanged.")
                        .size(12.0)
                        .weak(),
                );
            }
            Some(Outcome::Ready(_)) | None => {}
        }
        if let Some(note) = &note {
            ui.label(egui::RichText::new(note).size(12.0).weak());
        }
        self.ai_model_offer(ui);
        if ui
            .add_enabled(
                ready && !other_running,
                style::row_action(ui, "Generate AI pictures", &generate_key),
            )
            .on_hover_text("Fill this pause from the pictures on both sides with the local model. It runs in the background; nothing changes until you accept. :generate 3 makes three variants to choose from.")
            .clicked()
        {
            self.ai_action(AiAction::Generate { variants: 1 }, Some(self.ai_capture()));
        }
        ui.label(
            egui::RichText::new(if other_running {
                "Another pause is generating; cancel it first."
            } else {
                "Proposes pictures from both sides of this pause. Nothing changes until you accept."
            })
            .size(12.0)
            .weak(),
        );
    }

    /// The offered variants, each with a thumbnail of its middle picture,
    /// and the actions for the chosen one.
    fn ai_variants(&mut self, ui: &mut egui::Ui, ready: bool, candidate: &Candidate) {
        let count = candidate.variants.len();
        ui.colored_label(
            style::LAVENDER,
            if count > 1 {
                format!("Ready · {count} variants · {} f each", candidate.frames)
            } else {
                format!("Ready · {} f of generated pictures", candidate.frames)
            },
        );
        ui.label(
            egui::RichText::new("Ready does not change your edit.")
                .size(12.0)
                .weak(),
        );
        let canvas = self.workspace.as_ref().map_or([16, 9], |workspace| {
            let basis = workspace.document.presentation_basis();
            [basis.width.max(1), basis.height.max(1)]
        });
        let session = self.workspace.as_ref().map(|workspace| workspace.session);
        let aspect = canvas[0] as f32 / canvas[1] as f32;
        let selected = candidate.selected_index();
        let previewed = self
            .ai
            .preview
            .as_ref()
            .filter(|preview| preview.request() == &candidate.request)
            .map(|preview| preview.attempt().clone());
        for (index, variant) in candidate.variants.iter().enumerate() {
            let painted = session.and_then(|session| {
                self.thumbnails.show_candidate(
                    thumbnails::Key {
                        session,
                        revision: candidate.origin.clone(),
                        slot: thumbnails::Slot::Candidate(variant.attempt.clone()),
                        view: ProjectView::Sequence {
                            frame: ProjectFrame(0),
                        },
                    },
                    Arc::new(crate::worker::CandidateThumbnail {
                        object: variant.sampled.clone(),
                        frames: variant.sampled_frames,
                        size: variant.sampled_size,
                        canvas,
                    }),
                )
            });
            let chosen = index == selected;
            let shown = previewed.as_ref() == Some(&variant.attempt);
            let response = variant_row(
                ui,
                ready,
                VariantRow {
                    number: index + 1,
                    seed: variant.seed,
                    chosen,
                    shown,
                    aspect,
                    painted,
                },
            )
            .on_hover_text(format!(
                "AI variant {} of {count}, seed {}. Choose it with :pick-ai {} or :next-ai / :prev-ai.",
                index + 1,
                variant.seed,
                index + 1
            ));
            let number = u8::try_from(index + 1).unwrap_or(u8::MAX);
            if response.clicked() && !chosen {
                self.ai_action(
                    AiAction::Choose(VariantChoice::Number(number)),
                    Some(self.ai_capture()),
                );
            }
        }
        if count > 1 {
            ui.horizontal_wrapped(|ui| {
                style::key_hint(ui, ":next-ai", "next");
                style::key_hint(ui, ":prev-ai", "previous");
                style::key_hint(ui, ":pick-ai N", "choose");
            });
        }
        let previewing = self.ai.preview.is_some();
        let (label, key) = if previewing {
            ("Stop preview", "Esc")
        } else {
            ("Preview", ":preview-ai")
        };
        if ui
            .add_enabled(ready, style::row_action(ui, label, key))
            .on_hover_text("Show the chosen variant's pictures in the viewer at the edit cursor. Move through the pause to see every frame; nothing is saved.")
            .clicked()
        {
            self.ai_action(AiAction::Preview, Some(self.ai_capture()));
        }
        let auditioning = self.ai_auditioning();
        if ui
            .add_enabled(
                ready || auditioning,
                style::row_action(
                    ui,
                    if auditioning { "Pause audition" } else { "Audition" },
                    ":audition-ai",
                ),
            )
            .on_hover_text("Loop the pause with its lead-in and follow-through: the chosen variant's pictures with the pause's own sound, as they would play after Accept. Nothing is saved.")
            .clicked()
        {
            self.ai_action(AiAction::Audition, Some(self.ai_capture()));
        }
        if ui
            .add_enabled(
                ready,
                style::row_action(ui, "Accept", ":accept-ai").fill(style::SELECTED),
            )
            .on_hover_text("Make the chosen variant the pause's picture as one undoable edit.")
            .clicked()
        {
            self.ai_action(AiAction::Accept, Some(self.ai_capture()));
        }
        if ui
            .add_enabled(
                ready,
                style::row_action(
                    ui,
                    if count > 1 {
                        "Discard variant"
                    } else {
                        "Discard"
                    },
                    ":discard-ai",
                ),
            )
            .on_hover_text(
                "Remove the chosen variant from the list for good, also after reopening; its files stay in the project until a cleanup removes them. The pause is unchanged.",
            )
            .clicked()
        {
            self.ai_action(AiAction::Discard, Some(self.ai_capture()));
        }
        ui.label(
            egui::RichText::new(
                "Undo after Accept restores the previous picture. Discard cannot be undone.",
            )
            .size(12.0)
            .weak(),
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
                .weak(),
            );
        }
        if self.model_pack_offer(ui, pack_id, "Install AI models…", ":models") {
            self.open_models(Some(pack_id), ui.ctx());
        }
    }

    /// A status row while a job runs or a preview is shown.
    pub(super) fn ai_footer(&self, ui: &mut egui::Ui) {
        let previewing = self.ai.preview.as_ref();
        let running = self.ai_running();
        if previewing.is_none() && running.is_none() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if let Some(preview) = previewing {
                let number = self.ai.update.as_ref().and_then(|update| {
                    let candidate = update.candidates.get(preview.hold())?;
                    let index = candidate
                        .variants
                        .iter()
                        .position(|variant| &variant.attempt == preview.attempt())?;
                    Some((index + 1, candidate.variants.len()))
                });
                ui.colored_label(
                    style::LAVENDER,
                    match number {
                        Some((number, count)) if count > 1 => {
                            format!("AI PREVIEW · VARIANT {number} OF {count} · NOT SAVED")
                        }
                        _ => "AI PREVIEW · NOT SAVED".to_owned(),
                    },
                );
                style::key_hint(
                    ui,
                    ":audition-ai",
                    if self.ai_auditioning() {
                        "pause audition"
                    } else {
                        "audition with sound"
                    },
                );
                if number.is_some_and(|(_, count)| count > 1) {
                    style::key_hint(ui, ":next-ai", "next variant");
                }
                style::key_hint(ui, ":accept-ai", "accept");
                style::key_hint(ui, ":discard-ai", "discard");
                style::key_hint(ui, "Esc", "show your edit");
            }
            if let Some(job) = running {
                if previewing.is_some() {
                    ui.separator();
                }
                crate::preview::accessibility::busy(ui);
                let steps = job
                    .phase
                    .steps()
                    .map(|(completed, total)| format!(" {completed}/{total}"))
                    .unwrap_or_default();
                let variant = if job.variants > 1 {
                    format!(" · variant {}/{}", job.variant, job.variants)
                } else {
                    String::new()
                };
                ui.colored_label(
                    style::LAVENDER,
                    format!(
                        "AI pause{variant} · {}{steps} · {}",
                        job.phase.label(),
                        elapsed(job)
                    ),
                );
                style::key_hint(ui, ":cancel-ai", "cancel");
                ui.ctx().request_repaint_after(Duration::from_millis(250));
            }
        });
    }
}

/// What one variant row shows.
struct VariantRow {
    number: usize,
    seed: u64,
    chosen: bool,
    shown: bool,
    aspect: f32,
    painted: Option<thumbnails::Painted>,
}

/// One selectable variant row that fits the inspector's width: its
/// thumbnail, number and state, and its seed.
fn variant_row(ui: &mut egui::Ui, enabled: bool, row: VariantRow) -> egui::Response {
    let width = ui.available_width().max(1.0);
    let padding = 3.0;
    let height = VARIANT_THUMBNAIL_HEIGHT + 2.0 * padding;
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), sense);
    let state = if row.shown {
        "showing"
    } else if row.chosen {
        "chosen"
    } else {
        ""
    };
    let title = if state.is_empty() {
        format!("Variant {}", row.number)
    } else {
        format!("Variant {} · {state}", row.number)
    };
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            enabled,
            row.chosen,
            format!("{title}, seed {}", row.seed),
        )
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let visuals = ui.style().interact_selectable(&response, row.chosen);
    if row.chosen || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 4.0, visuals.weak_bg_fill);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            4.0,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Inside,
        );
    }
    let thumbnail_width = (VARIANT_THUMBNAIL_HEIGHT * row.aspect).min(width * 0.45);
    let thumbnail = egui::Rect::from_min_size(
        rect.min + egui::vec2(padding, padding),
        egui::vec2(thumbnail_width, VARIANT_THUMBNAIL_HEIGHT),
    );
    cards::paint_thumbnail(ui, thumbnail, row.painted);
    if row.chosen {
        ui.painter().rect_stroke(
            thumbnail.expand(1.0),
            5.0,
            egui::Stroke::new(2.0, style::LAVENDER),
            egui::StrokeKind::Outside,
        );
    }
    let text_left = thumbnail.right() + 8.0;
    let text_width = (rect.right() - padding - text_left).max(1.0);
    let galley = |text: String, color: egui::Color32, size: f32| {
        let mut job =
            egui::text::LayoutJob::simple_singleline(text, egui::FontId::proportional(size), color);
        job.wrap = egui::text::TextWrapping::truncate_at_width(text_width);
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };
    let title = galley(
        title,
        if row.chosen {
            style::LAVENDER
        } else {
            ui.visuals().text_color()
        },
        14.0,
    );
    let seed = galley(format!("seed {}", row.seed), style::muted(ui), 12.0);
    let top = rect.top() + padding + 2.0;
    ui.painter().galley(
        egui::pos2(text_left, top),
        title.clone(),
        egui::Color32::WHITE,
    );
    ui.painter().galley(
        egui::pos2(text_left, top + title.size().y + 2.0),
        seed,
        egui::Color32::WHITE,
    );
    response
}
