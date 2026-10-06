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
use crate::navigation::{AiAction, CompareChoice, VariantChoice};
use crate::project::generation::{
    Candidate, CandidatePreview, GenerationOperation, Job, Outcome, Update,
};
use crate::transport::Domain;
use deadpan_core::{AudioSample, RevisionId};
use deadpan_playback::{ContentIdentity, Window};

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
    /// Admit it in comparison's Before state: the viewer and audition keep
    /// showing the pause as committed until the next switch.
    before: bool,
}

/// An immediate comparison switch awaiting its store selection reply.
#[derive(Clone, Debug)]
struct PendingSelect {
    ticket: u64,
    previous: Option<Arc<CandidatePreview>>,
    before: bool,
    /// A later switch's selection, sent when this one is answered, so rapid
    /// switches never overrun the service mailbox and the last one wins.
    queued: Option<GenerationOperation>,
}

/// The heard position an audition continues from after a comparison switch.
#[derive(Clone, Debug)]
struct Continue {
    domain: Domain,
    window: Window,
    sample: AudioSample,
}

/// Advisory entry/exit join measurements (spec 12.5), keyed by attempt and
/// session. A measurement is a reading, never an acceptance or rejection.
#[derive(Clone, Default)]
struct Joins(Arc<std::sync::Mutex<JoinsState>>);

#[derive(Default)]
struct JoinsState {
    /// The session readings belong to; another session clears them.
    session: Option<u64>,
    measured: std::collections::BTreeMap<AttemptId, Reading>,
    /// The one running measurement, if any.
    running: Option<AttemptId>,
    /// Cancels the running measurement when the session ends.
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

use deadpan_cli::generation::joins::{JoinClass, JoinReport};

/// What a variant's background reading found: its advisory joins and how its
/// conditioning colour reached the model.
#[derive(Clone, Debug)]
struct Reading {
    joins: Result<JoinReport, String>,
    /// `ConditioningColour::describe` of the retained manifest, when readable.
    colour: Option<String>,
}

/// Read a variant's retained conditioning manifest and describe how its
/// colour reached the model (BT.709 read as sRGB is an approximation).
fn conditioning_colour(
    handle: &deadpan_store::generated_media::GeneratedReadHandle,
    receipt: &deadpan_store::generation_attempts::BundleValidationReceipt,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Option<String> {
    use std::io::Read;
    const MAX_MANIFEST_BYTES: u64 = 1 << 20;
    let reference = receipt.admission()?.inputs().manifest();
    if reference.byte_length() > MAX_MANIFEST_BYTES {
        return None;
    }
    let limits = deadpan_store::generated_media::GeneratedReadLimits::new(
        MAX_MANIFEST_BYTES,
        Duration::from_secs(15),
    )
    .ok()?;
    let mut snapshot = handle.snapshot(reference, limits, cancelled).ok()?;
    let mut bytes = Vec::new();
    snapshot
        .by_ref()
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    deadpan_cli::generation::conditioning::ConditioningColour::from_manifest(&bytes)
        .map(|colour| colour.describe())
}

/// Ends a measurement whatever happens to its thread, including a panic, so
/// the next variant can start and this one is not retried every frame.
struct JoinGuard {
    shared: Arc<std::sync::Mutex<JoinsState>>,
    session: u64,
    attempt: AttemptId,
    result: Option<Reading>,
    context: egui::Context,
}

impl JoinGuard {
    fn finish(&mut self, reading: Reading) {
        self.result = Some(reading);
    }
}

impl Drop for JoinGuard {
    fn drop(&mut self) {
        let mut state = self
            .shared
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if state.session == Some(self.session) && state.running.as_ref() == Some(&self.attempt) {
            state.running = None;
            let result = self.result.take().unwrap_or_else(|| Reading {
                joins: Err("The join measurement stopped unexpectedly.".into()),
                colour: None,
            });
            state.measured.insert(self.attempt.clone(), result);
        }
        drop(state);
        self.context.request_repaint();
    }
}

impl Joins {
    /// Keep readings only for `session` and the offered `attempts`; a
    /// different or absent session cancels the running measurement.
    fn retain(&self, session: Option<u64>, attempts: &std::collections::BTreeSet<AttemptId>) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if state.session != session {
            state
                .cancelled
                .store(true, std::sync::atomic::Ordering::Release);
            *state = JoinsState {
                session,
                ..JoinsState::default()
            };
            return;
        }
        state
            .measured
            .retain(|attempt, _| attempts.contains(attempt));
    }

    /// The reading for `variant`, starting its measurement when none runs.
    /// Each variant is measured at most once per session.
    fn get(
        &self,
        workspace: &Workspace,
        candidate: &Candidate,
        variant: &crate::project::generation::Variant,
        context: &egui::Context,
    ) -> Option<Reading> {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if state.session != Some(workspace.session) {
            return None;
        }
        if let Some(found) = state.measured.get(&variant.attempt) {
            return Some(found.clone());
        }
        if state.running.is_some() {
            return None;
        }
        state.running = Some(variant.attempt.clone());
        let cancelled = Arc::clone(&state.cancelled);
        drop(state);
        let mut guard = JoinGuard {
            shared: Arc::clone(&self.0),
            session: workspace.session,
            attempt: variant.attempt.clone(),
            result: None,
            context: context.clone(),
        };
        let handle = workspace.generated.clone();
        let package = workspace.path.clone();
        let origin = candidate.origin.clone();
        let hold = candidate.hold.clone();
        let receipt = Arc::clone(&variant.receipt);
        let spawned = std::thread::Builder::new()
            .name("deadpan-ai-joins".into())
            .spawn(move || {
                let joins = deadpan_cli::generation::joins::measure_request_joins(
                    &package, &handle, &origin, &hold, &receipt, &cancelled,
                )
                .map_err(|error| error.to_string());
                let colour = conditioning_colour(&handle, &receipt, &cancelled);
                guard.finish(Reading { joins, colour });
            });
        if let Err(error) = spawned {
            // The guard moved into the failed closure and was dropped with
            // it, recording a stop; name the real cause instead.
            let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
            state.measured.insert(
                variant.attempt.clone(),
                Reading {
                    joins: Err(format!("Could not start the join measurement: {error}")),
                    colour: None,
                },
            );
        }
        None
    }
}

fn join_word(class: JoinClass) -> &'static str {
    match class {
        JoinClass::Smooth => "smooth",
        JoinClass::Noticeable => "noticeable",
        JoinClass::Jump => "jump",
    }
}

/// "joins smooth / jump" for a variant row, or why it is unknown.
fn joins_label(reading: Option<&Reading>) -> String {
    match reading.map(|reading| &reading.joins) {
        None => "joins …".into(),
        Some(Ok(report)) => format!(
            "joins {} / {}",
            join_word(report.entry.class),
            join_word(report.exit.class)
        ),
        Some(Err(_)) => "joins unknown".into(),
    }
}

/// The most admitted previews a comparison keeps ready besides the shown one.
const RETAINED_PREVIEWS: usize = 8;

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
    /// Comparison shows the pause as committed (Before) while `preview`
    /// stays admitted for the next switch.
    before: bool,
    /// Admitted previews of other variants of the shown preview's request,
    /// session and base, so switching back to one needs no service reply.
    retained: Vec<Arc<CandidatePreview>>,
    /// An audition interrupted by a comparison switch, continued from the
    /// same heard sample once the switched-to preview is admitted.
    continue_at: Option<Continue>,
    /// A comparison switch whose store selection is still unanswered, with
    /// what was shown before it so a refusal can restore that view.
    pending_select: Option<PendingSelect>,
    /// Advisory join measurements by variant, filled one at a time off the
    /// UI thread.
    joins: Joins,
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

    /// Whether a comparison shows the pause as committed.
    #[cfg(feature = "ui-harness")]
    pub(crate) fn comparing_before(&self) -> bool {
        self.preview.is_some() && self.before
    }

    /// Admitted previews kept ready for comparison switches.
    #[cfg(feature = "ui-harness")]
    pub(crate) fn retained_previews(&self) -> usize {
        self.retained.len()
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
                AiAction::Compare(choice) => self.ai_compare(target, choice),
                AiAction::Keep => self.ai_keep(target),
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
            GenerationOperation::Keep {
                session,
                request,
                attempt,
                keep,
                ..
            } => GenerationOperation::Keep {
                ticket,
                session,
                request,
                attempt,
                keep,
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

    /// Choose another offered variant. While previewing or comparing, the
    /// viewer shows the newly chosen one at the same frame, and an audition
    /// in progress continues from the same heard sample.
    fn ai_choose(&mut self, target: Target, choice: VariantChoice) -> Result<(), String> {
        let candidate = Self::ai_target_candidate(&target)?;
        let count = candidate.variants.len();
        let current = self.ai_current_index(candidate);
        let index = match choice {
            VariantChoice::Next => (current + 1) % count,
            VariantChoice::Previous => (current + count - 1) % count,
            VariantChoice::Number(number) => Self::ai_variant_index(candidate, number)?,
        };
        let request = candidate.request.clone();
        let previewing = self
            .ai
            .preview
            .as_ref()
            .is_some_and(|preview| preview.request() == &request)
            || self.ai.pending_preview.is_some();
        if previewing {
            self.ai_compare_variant(&target, index);
        } else {
            let attempt = candidate.variants[index].attempt.clone();
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

    /// The variant the person is looking at: one still being prepared, then
    /// the shown preview, then the store's selection. Successive commands in
    /// one input batch therefore advance from each other, before the store's
    /// selection reply arrives, and Accept, Keep and Discard act on what is
    /// shown.
    fn ai_current_index(&self, candidate: &Candidate) -> usize {
        let shown = self
            .ai
            .pending_preview
            .as_ref()
            .filter(|pending| pending.request == candidate.request)
            .map(|pending| &pending.attempt)
            .or_else(|| {
                self.ai
                    .preview
                    .as_ref()
                    .filter(|preview| preview.request() == &candidate.request)
                    .map(|preview| preview.attempt())
            });
        shown
            .and_then(|attempt| {
                candidate
                    .variants
                    .iter()
                    .position(|variant| &variant.attempt == attempt)
            })
            .unwrap_or_else(|| candidate.selected_index())
    }

    /// The attempt Accept, Keep and Discard act on.
    fn ai_current_attempt(&self, candidate: &Candidate) -> AttemptId {
        candidate.variants[self.ai_current_index(candidate)]
            .attempt
            .clone()
    }

    /// The 0-based index of the variant the inspector numbers `number`.
    fn ai_variant_index(candidate: &Candidate, number: u8) -> Result<usize, String> {
        let count = candidate.variants.len();
        let index = usize::from(number).saturating_sub(1);
        if number == 0 || index >= count {
            return Err(format!(
                "This pause has {count} AI variant{}; choose 1 to {count}.",
                if count == 1 { "" } else { "s" }
            ));
        }
        Ok(index)
    }

    /// Compare the pause as committed (Before) with its Ready variants at the
    /// same frame and heard sample. Nothing is saved.
    fn ai_compare(&mut self, target: Target, choice: CompareChoice) -> Result<(), String> {
        let candidate = Self::ai_target_candidate(&target)?;
        let showing = self
            .ai
            .preview
            .as_ref()
            .filter(|preview| preview.request() == &candidate.request)
            .cloned();
        let pending = self
            .ai
            .pending_preview
            .as_ref()
            .is_some_and(|pending| pending.request == candidate.request);
        let before = match choice {
            CompareChoice::Toggle => showing.is_some() && !self.ai.before,
            CompareChoice::Before => true,
            CompareChoice::Variant(number) => {
                let index = Self::ai_variant_index(candidate, number)?;
                self.ai_compare_variant(&target, index);
                return Ok(());
            }
        };
        match showing {
            Some(preview) => {
                // A variant still being prepared arrives in the same state.
                if let Some(pending) = &mut self.ai.pending_preview {
                    pending.before = before;
                }
                self.ai_show(Some(preview), before);
                self.ai_compare_message();
            }
            None if pending => {
                if let Some(pending) = &mut self.ai.pending_preview {
                    pending.before = before;
                }
            }
            None => {
                let (request, attempt) = (candidate.request.clone(), candidate.selected.clone());
                self.ai_request_preview(&target, request, attempt, false, before);
            }
        }
        Ok(())
    }

    /// Show variant `index` of the target's candidate: at once when its
    /// preview is admitted (writing the store's selection alongside),
    /// otherwise after the service prepares it. Either way the frame and the
    /// heard sample are kept.
    fn ai_compare_variant(&mut self, target: &Target, index: usize) {
        let Some(candidate) = target.candidate.as_ref() else {
            return;
        };
        let attempt = candidate.variants[index].attempt.clone();
        let request = candidate.request.clone();
        let admitted = self
            .ai
            .preview
            .iter()
            .chain(self.ai.retained.iter())
            .find(|preview| {
                preview.request() == &request
                    && preview.attempt() == &attempt
                    && preview.session() == target.session
                    && preview.base() == &target.revision
            })
            .cloned();
        match admitted {
            Some(preview) => {
                self.ai.pending_preview = None;
                let select = GenerationOperation::Select {
                    ticket: 0,
                    session: target.session,
                    request,
                    attempt,
                };
                if let Some(pending) = &mut self.ai.pending_select {
                    // One selection in flight at a time; the newest waits.
                    pending.queued = Some(select);
                } else {
                    let previous = self.ai.preview.clone();
                    let before = self.ai.before;
                    let Some(ticket) = self.ai_submit(select) else {
                        // The service refused at once; keep showing what was shown.
                        return;
                    };
                    // A refusal restores what the person saw before this run
                    // of switches.
                    self.ai.pending_select = Some(PendingSelect {
                        ticket,
                        previous,
                        before,
                        queued: None,
                    });
                }
                self.ai_show(Some(preview), false);
                self.ai_compare_message();
            }
            None => {
                // A playing audition continues from its heard sample when the
                // preview arrives; only a still pending audition start carries
                // over.
                let audition = self
                    .ai
                    .pending_preview
                    .as_ref()
                    .is_some_and(|pending| pending.audition);
                self.ai_request_preview(target, request, attempt, audition, false);
            }
        }
    }

    /// The content the viewer and audition show while previewing: Before
    /// (the committed revision) or the shown variant's proposed acceptance.
    fn ai_shown_identity(&self, workspace: &Workspace) -> Option<(RevisionId, ContentIdentity)> {
        let preview = self.ai.preview.as_ref()?;
        Some(if self.ai.before {
            (
                workspace.document.revision_id().clone(),
                ContentIdentity::Committed,
            )
        } else {
            (
                preview.audio().document.revision_id().clone(),
                preview.audio().content.clone(),
            )
        })
    }

    /// The snapshot comparison plays for the shown content.
    fn ai_shown_snapshot(&self, workspace: &Workspace) -> Arc<deadpan_playback::Snapshot> {
        self.ai_preview_audio(workspace)
            .unwrap_or_else(|| Arc::new(workspace.playback_snapshot()))
    }

    /// Switch what the viewer and audition show at the same frame: a playing
    /// audition restarts the new content from the exact heard sample in the
    /// same window, and a paused one keeps its exact sample for Space.
    fn ai_show(&mut self, preview: Option<Arc<CandidatePreview>>, before: bool) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let heard = self.ai_heard();
        let old = self.ai_shown_identity(&workspace);
        if let Some(preview) = preview {
            self.ai_admit(preview);
        }
        self.ai.before = before && self.ai.preview.is_some();
        let new = self.ai_shown_identity(&workspace);
        if old == new {
            return;
        }
        if let Some(heard) = heard {
            self.stop_playback();
            let snapshot = self.ai_shown_snapshot(&workspace);
            self.start_snapshot_playback(snapshot, heard.domain, heard.window, heard.sample);
            return;
        }
        if let (Some((old_revision, old_content)), Some((revision, content))) = (old, new)
            && self
                .resume
                .as_ref()
                .is_some_and(|resume| resume.shows(&old_revision, &old_content))
        {
            self.resume = self
                .resume
                .take()
                .map(|resume| resume.retarget(revision, content));
        }
        // Not `request_picture`: that is navigation, which stops playback
        // and forgets the paused position this switch just kept.
        self.request_picture_for_transport(false, None);
    }

    /// Make `preview` the shown one, keeping the previously shown variant of
    /// the same request, session and base ready for the next switch.
    fn ai_admit(&mut self, preview: Arc<CandidatePreview>) {
        self.ai
            .retained
            .retain(|kept| kept.attempt() != preview.attempt());
        if let Some(previous) = self.ai.preview.take()
            && previous.attempt() != preview.attempt()
            && previous.request() == preview.request()
            && previous.session() == preview.session()
            && previous.base() == preview.base()
        {
            self.ai.retained.insert(0, previous);
            self.ai.retained.truncate(RETAINED_PREVIEWS);
        }
        if self.ai.retained.first().is_some_and(|kept| {
            kept.request() != preview.request()
                || kept.session() != preview.session()
                || kept.base() != preview.base()
        }) {
            self.ai.retained.clear();
        }
        self.ai.preview = Some(preview);
    }

    /// The comparison audition's heard position, while it plays.
    fn ai_heard(&self) -> Option<Continue> {
        if !self.ai_auditioning() {
            return None;
        }
        let run = self.transport.as_ref()?;
        Some(Continue {
            domain: run.domain().clone(),
            window: *run.window(),
            sample: run.content_sample().ok()?,
        })
    }

    /// Name what a comparison shows now.
    fn ai_compare_message(&mut self) {
        let Some(preview) = self.ai.preview.as_ref() else {
            return;
        };
        let compare = self.editor_key(EditorKey::CompareAi);
        self.message = Some(if self.ai.before {
            format!(
                "Comparing: Before, the pause as it is now. {compare} shows the AI variant at this frame; nothing is saved."
            )
        } else {
            let number = self.ai_variant_number(preview);
            match number {
                Some((number, count)) if count > 1 => format!(
                    "Comparing: AI variant {number} of {count}. {compare} shows Before at this frame; nothing is saved."
                ),
                _ => format!(
                    "Comparing: AI pictures. {compare} shows Before at this frame; nothing is saved."
                ),
            }
        });
    }

    /// The shown preview's 1-based variant number and the offered count.
    fn ai_variant_number(&self, preview: &CandidatePreview) -> Option<(usize, usize)> {
        let candidate = self.ai.update.as_ref()?.candidates.get(preview.hold())?;
        let index = candidate
            .variants
            .iter()
            .position(|variant| &variant.attempt == preview.attempt())?;
        Some((index + 1, candidate.variants.len()))
    }

    /// Ready variants offered for `hold`.
    pub(super) fn ai_offered_variants(&self, hold: &NodeId) -> usize {
        self.ai_candidate(hold)
            .map_or(0, |candidate| candidate.variants.len())
    }

    fn ai_request_preview(
        &mut self,
        target: &Target,
        request: RequestId,
        attempt: AttemptId,
        audition: bool,
        before: bool,
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
                before,
            });
        }
    }

    fn ai_preview(&mut self, target: Target, audition: bool) -> Result<(), String> {
        let candidate = Self::ai_target_candidate(&target)?;
        let (request, attempt) = (candidate.request.clone(), candidate.selected.clone());
        self.ai_request_preview(&target, request, attempt, audition, false);
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
            && !self.ai.before
            && preview.session() == workspace.session
            && preview.base() == workspace.document.revision_id())
        .then(|| Arc::clone(preview.audio()))
    }

    /// Whether playback is the AI preview's own audition, or comparison's
    /// Before playback of the committed pause.
    fn ai_auditioning(&self) -> bool {
        self.transport.as_ref().is_some_and(|run| {
            self.ai
                .preview_content()
                .is_some_and(|content| &run.content == content)
                || self.ai.preview.as_ref().is_some_and(|preview| {
                    self.ai.before
                        && run.content == ContentIdentity::Committed
                        && run.session == preview.session()
                        && &run.revision == preview.base()
                        && matches!(run.domain(), Domain::Sequence { .. })
                })
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
            preview.request() == &candidate.request
                && (self.ai.before || preview.attempt() == &self.ai_current_attempt(candidate))
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
            self.ai_current_attempt(candidate),
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

    /// Keep the chosen variant from expiry, or release a kept one. Not an
    /// edit; the store records it durably.
    fn ai_keep(&mut self, target: Target) -> Result<(), String> {
        let candidate = Self::ai_target_candidate(&target)?;
        let index = self.ai_current_index(candidate);
        let variant = &candidate.variants[index];
        let keep = !variant.kept;
        let (request, attempt) = (candidate.request.clone(), variant.attempt.clone());
        let count = candidate.variants.len();
        self.ai_submit(GenerationOperation::Keep {
            ticket: 0,
            session: target.session,
            request,
            attempt,
            keep,
        });
        self.message = Some(if keep {
            format!(
                "Keeping AI variant {} of {count}: it stays offered and its files stay in the project until you discard it.",
                index + 1
            )
        } else {
            format!(
                "Released AI variant {} of {count}: unless chosen or accepted, it expires after the retention period.",
                index + 1
            )
        });
        Ok(())
    }

    fn ai_discard(&mut self, target: Target) -> Result<(), String> {
        let candidate = Self::ai_target_candidate(&target)?;
        let (request, attempt) = (
            candidate.request.clone(),
            self.ai_current_attempt(candidate),
        );
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
        if let Some(heard) = self.ai.continue_at.take()
            && self.ai.preview.is_some()
            && let Some(workspace) = self.workspace.clone()
        {
            self.ai.audition_pending = false;
            let snapshot = self.ai_shown_snapshot(&workspace);
            self.start_snapshot_playback(snapshot, heard.domain, heard.window, heard.sample);
            return;
        }
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
        self.ai.pending_select = None;
        self.ai.audition_pending = false;
        self.ai.continue_at = None;
        self.ai.retained.clear();
        if self.ai_auditioning() {
            self.stop_playback();
        }
        let before = std::mem::take(&mut self.ai.before);
        if self.ai.preview.take().is_some() {
            if !before {
                self.request_picture(false);
            }
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
        let offered = self
            .ai
            .update
            .as_ref()
            .map(|update| {
                update
                    .candidates
                    .values()
                    .flat_map(|candidate| candidate.variants.iter())
                    .map(|variant| variant.attempt.clone())
                    .collect::<std::collections::BTreeSet<_>>()
            })
            .unwrap_or_default();
        self.ai
            .joins
            .retain(workspace.as_ref().map(|(session, _)| *session), &offered);
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
            if let Some(pending) = self.ai.pending_select.clone()
                && *answered >= pending.ticket
            {
                self.ai.pending_select = None;
                if *answered == pending.ticket
                    && let Some(refusal) = refusal
                {
                    // The store did not record the switch: show again what
                    // was shown before it, so Accept and the view agree.
                    self.ai_show(pending.previous.clone(), pending.before);
                    if pending.previous.is_none() {
                        self.ai_stop_preview();
                    }
                    self.error = Some(format!(
                        "Could not choose that AI variant: {refusal} Showing the previous view again."
                    ));
                    changed = true;
                } else if let Some(queued) = pending.queued.clone()
                    && let Some(ticket) = self.ai_submit(queued)
                {
                    self.ai.pending_select = Some(PendingSelect {
                        ticket,
                        queued: None,
                        ..pending
                    });
                }
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
                    // A comparison switch keeps the heard sample: a playing
                    // audition continues with these pictures from where it
                    // is heard now, and a paused one keeps its exact sample.
                    let heard = self.ai_heard();
                    let resumed = self
                        .workspace
                        .as_ref()
                        .and_then(|current| self.ai_shown_identity(current))
                        .is_some_and(|(revision, content)| {
                            self.resume
                                .as_ref()
                                .is_some_and(|resume| resume.shows(&revision, &content))
                        });
                    if heard.is_some() {
                        self.stop_playback();
                    }
                    // A fresh preview shows the pause itself when the cursor
                    // is elsewhere; a comparison switch stays at its frame.
                    let range = preview.range();
                    if heard.is_none()
                        && !resumed
                        && let (Ok(start), Ok(end)) =
                            (u64::try_from(range.start().0), u64::try_from(range.end().0))
                        && !(start..end).contains(&self.sequence_cursor)
                    {
                        self.sequence_cursor = start;
                    }
                    let old = self
                        .workspace
                        .clone()
                        .and_then(|current| self.ai_shown_identity(&current));
                    self.ai_admit(preview);
                    self.ai.before = pending.before;
                    if resumed
                        && let Some(current) = self.workspace.clone()
                        && let (Some((old_revision, old_content)), Some((revision, content))) =
                            (old, self.ai_shown_identity(&current))
                        && self
                            .resume
                            .as_ref()
                            .is_some_and(|resume| resume.shows(&old_revision, &old_content))
                    {
                        self.resume = self
                            .resume
                            .take()
                            .map(|resume| resume.retarget(revision, content));
                    }
                    self.ai.audition_pending = pending.audition && heard.is_none();
                    self.ai.continue_at = heard;
                    let number = self
                        .ai
                        .preview
                        .as_ref()
                        .and_then(|preview| self.ai_variant_number(preview));
                    let compare = self.editor_key(EditorKey::CompareAi);
                    self.message = Some(if self.ai.before {
                        format!(
                            "Comparing: Before, the pause as it is now. {compare} shows the AI variant at this frame; nothing is saved."
                        )
                    } else {
                        match number {
                            Some((number, count)) if count > 1 => format!(
                                "Previewing AI variant {number} of {count}. Nothing is saved; {compare} compares with Before, :audition-ai plays it with the pause's sound, :accept-ai keeps it, Esc returns."
                            ),
                            _ => format!(
                                "Previewing AI pictures. Nothing is saved; {compare} compares with Before, :audition-ai plays them with the pause's sound, :accept-ai keeps them, Esc returns."
                            ),
                        }
                    });
                    if resumed {
                        // A paused switch is not navigation: keep the paused
                        // position instead of the caller's stopping refresh.
                        self.request_picture_for_transport(false, None);
                    } else {
                        changed = true;
                    }
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
            self.ai.before = false;
            self.ai.retained.clear();
            self.ai.continue_at = None;
            self.ai.audition_pending = false;
            changed = true;
        }
        // Kept previews follow the same rule, without changing the viewer.
        let offered = |attempt: &AttemptId, request: &RequestId, hold: &NodeId| {
            self.ai.update.as_ref().is_some_and(|update| {
                update.candidates.get(hold).is_some_and(|candidate| {
                    &candidate.request == request
                        && candidate
                            .variants
                            .iter()
                            .any(|variant| &variant.attempt == attempt)
                })
            })
        };
        let retained = std::mem::take(&mut self.ai.retained);
        self.ai.retained = retained
            .into_iter()
            .filter(|kept| {
                workspace.as_ref().is_some_and(|(session, revision)| {
                    kept.session() == *session && kept.base() == revision
                }) && offered(kept.attempt(), kept.request(), kept.hold())
            })
            .collect();
        if workspace.is_none() {
            self.ai.pending_preview = None;
            self.ai.awaiting = None;
        }
        changed
    }

    /// The candidate picture at the edit cursor while previewing.
    pub(super) fn ai_picture_work(&self, picture: Option<u64>) -> Option<crate::worker::Work> {
        let preview = self.ai.preview.as_ref()?;
        if self.view != View::Sequence || self.ai.before {
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
        let selected = self.ai_current_index(candidate);
        let comparing = self
            .ai
            .preview
            .as_ref()
            .is_some_and(|preview| preview.request() == &candidate.request);
        let previewed = self
            .ai
            .preview
            .as_ref()
            .filter(|preview| preview.request() == &candidate.request && !self.ai.before)
            .map(|preview| preview.attempt().clone());
        let mut colour_note = None;
        for (index, variant) in candidate.variants.iter().enumerate() {
            let reading = self
                .workspace
                .as_ref()
                .and_then(|workspace| self.ai.joins.get(workspace, candidate, variant, ui.ctx()));
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
                    retention: retention_label(variant),
                    joins: joins_label(reading.as_ref()),
                    chosen,
                    shown,
                    aspect,
                    painted,
                },
            )
            .on_hover_text(format!(
                "AI variant {} of {count}, seed {}. Choose it with :pick-ai {} or :next-ai / :prev-ai.{}",
                index + 1,
                variant.seed,
                index + 1,
                match reading.as_ref().map(|reading| &reading.joins) {
                    Some(Ok(report)) => format!(
                        " Picture change at its joins with the original (advisory, mean difference out of 255): entry {:.1} ({}), exit {:.1} ({}). A reading only; listen and look before accepting.",
                        report.entry.mean_abs_diff,
                        join_word(report.entry.class),
                        report.exit.mean_abs_diff,
                        join_word(report.exit.class)
                    ),
                    Some(Err(error)) => format!(" Its joins could not be measured: {error}"),
                    None => String::new(),
                } + &reading
                    .as_ref()
                    .and_then(|reading| reading.colour.as_ref())
                    .map(|colour| format!(" {colour}"))
                    .unwrap_or_default()
            ));
            if chosen
                && let Some(colour) = reading.as_ref().and_then(|reading| reading.colour.clone())
            {
                colour_note = Some(colour);
            }
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
        if let Some(colour) = colour_note {
            ui.label(egui::RichText::new(colour).size(12.0).weak());
        }
        if comparing && self.ai.before {
            ui.label(
                egui::RichText::new("Showing Before: the pause as it is now, at the same frame.")
                    .size(12.0)
                    .color(style::LAVENDER),
            );
        }
        let compare_key = self.editor_key(EditorKey::CompareAi);
        if ui
            .add_enabled(
                ready,
                style::row_action(
                    ui,
                    if comparing && !self.ai.before {
                        "Show before"
                    } else {
                        "Compare before / after"
                    },
                    &compare_key,
                ),
            )
            .on_hover_text(format!(
                "Switch between the pause as it is now (Before) and the chosen variant at the same frame. While auditioning, the sound continues from the same heard sample. {} shows the next variant. Nothing is saved.",
                self.editor_key(EditorKey::NextAi)
            ))
            .clicked()
        {
            self.ai_action(
                AiAction::Compare(CompareChoice::Toggle),
                Some(self.ai_capture()),
            );
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
                "Remove the chosen variant from the list for good, also after reopening; a storage cleanup removes its files after the grace period. The pause is unchanged.",
            )
            .clicked()
        {
            self.ai_action(AiAction::Discard, Some(self.ai_capture()));
        }
        let kept = candidate
            .variants
            .get(selected)
            .is_some_and(|variant| variant.kept);
        if ui
            .add_enabled(
                ready,
                style::row_action(
                    ui,
                    if kept { "Release variant" } else { "Keep variant" },
                    ":keep-ai",
                ),
            )
            .on_hover_text(if kept {
                "Let the chosen variant expire again like the others unless it is chosen or accepted. The pause is unchanged."
            } else {
                "Keep the chosen variant offered, and its files in the project, until you discard it. The pause is unchanged."
            })
            .clicked()
        {
            self.ai_action(AiAction::Keep, Some(self.ai_capture()));
        }
        ui.label(
            egui::RichText::new(
                "Undo after Accept restores the previous picture. Discard cannot be undone. Variants not kept, chosen or accepted expire; Storage shows when.",
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
                let number = self.ai_variant_number(preview);
                ui.colored_label(
                    style::LAVENDER,
                    match number {
                        _ if self.ai.before => "AI COMPARE · BEFORE · NOT SAVED".to_owned(),
                        Some((number, count)) if count > 1 => {
                            format!("AI PREVIEW · VARIANT {number} OF {count} · NOT SAVED")
                        }
                        _ => "AI PREVIEW · NOT SAVED".to_owned(),
                    },
                );
                style::key_hint(
                    ui,
                    &self.editor_key(EditorKey::CompareAi),
                    if self.ai.before {
                        "show AI variant"
                    } else {
                        "show before"
                    },
                );
                if number.is_some_and(|(_, count)| count > 1) {
                    style::key_hint(ui, &self.editor_key(EditorKey::NextAi), "next variant");
                }
                style::key_hint(
                    ui,
                    ":audition-ai",
                    if self.ai_auditioning() {
                        "pause audition"
                    } else {
                        "audition with sound"
                    },
                );
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

/// How long an unkept variant stays offered, for its row.
fn retention_label(variant: &crate::project::generation::Variant) -> String {
    if variant.kept {
        return "kept".into();
    }
    if variant.picked {
        return "picked by you, does not expire".into();
    }
    let Some(expires) = variant.expires_at else {
        return String::new();
    };
    let left = expires
        .duration_since(std::time::SystemTime::now())
        .unwrap_or_default()
        .as_secs();
    match left {
        0 => "expires at the next cleanup".into(),
        1..=5_399 => "expires within 2 hours".into(),
        5_400..=129_599 => format!("expires in {} hours", left.div_ceil(3_600)),
        _ => format!("expires in {} days", left.div_ceil(86_400)),
    }
}

/// What one variant row shows.
struct VariantRow {
    number: usize,
    seed: u64,
    /// Advisory entry / exit join reading.
    joins: String,
    /// "kept", "expires in 5 days", or empty while chosen or accepted.
    retention: String,
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
            if row.retention.is_empty() {
                format!("{title}, seed {}, {}", row.seed, row.joins)
            } else {
                format!(
                    "{title}, seed {}, {}, {}",
                    row.seed, row.retention, row.joins
                )
            },
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
    let seed = galley(
        if row.retention.is_empty() {
            format!("seed {} · {}", row.seed, row.joins)
        } else {
            format!("seed {} · {} · {}", row.seed, row.retention, row.joins)
        },
        style::muted(ui),
        12.0,
    );
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
