//! The service's AI pause job: one bounded job thread per project runs
//! conditioning (read-only) and the supervised worker; the writer applies
//! allocation, every durable transition and the final outcome. Session
//! replacement and shutdown cancel the job and wait until it has drained.

use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Instant;

use deadpan_cli::generation::acceptance;
use deadpan_cli::generation::attempt::{
    self, AllocateInput, Allocated, AttemptProgress, AttemptRecord, GenerationError, RunResult,
    RunTimings, WorkerRun,
};
use deadpan_cli::generation::conditioning::{self, BridgeInputs};
use deadpan_cli::generation::runtime::BridgeRuntime;
use deadpan_jobs::{
    AttemptId, GenerationOptions, HostFailureCode, JobFailure, JobState, MessageIdentity,
    ProviderSelection, RequestId,
};
use deadpan_store::generation_retention::DEFAULT_VARIANT_RETENTION;

use super::*;
use crate::project::generation::{
    Backend, Candidate, CandidatePreview, GenerationOperation, Job, MAX_VARIANTS, Outcome, Phase,
    PreviewParts, Update, Variant,
};

/// Events the job thread may queue ahead of the writer.
const EVENT_CAPACITY: usize = 64;

struct StartInput {
    ticket: u64,
    session: u64,
    revision: RevisionId,
    hold: NodeId,
    variants: u8,
    seed: Option<u64>,
    options: Option<GenerationOptions>,
}

struct JobInput {
    package: PathBuf,
    revision: RevisionId,
    hold: NodeId,
    options: GenerationOptions,
}

/// Job-to-writer events. The service loop drains them every iteration, so a
/// record waits only for the writer's current operation, never for a timer
/// that a long writer task (such as a preview's six-object verification) could
/// exhaust. A record the writer refuses, or a writer that has stopped
/// (disconnected channel), cancels and fails the attempt.
enum Event {
    Prepared(std::result::Result<Box<BridgeInputs>, String>),
    /// The recorded attempt waits for the coordinator's inference slot.
    Queued,
    /// The coordinator admitted the job; its worker starts now.
    Admitted,
    /// While it waited, edits made the request stale; the attempt is
    /// cancelled without loading the model.
    Superseded,
    Progress(AttemptProgress),
    Record(AttemptRecord, SyncSender<std::result::Result<(), String>>),
}

struct Running {
    cancelled: Arc<AtomicBool>,
    events: Receiver<Event>,
    /// A dedicated one-slot channel: each attempt's run is never dropped
    /// behind queued progress, and its qualified workspace reaches `finish`.
    finished: Receiver<WorkerRun>,
    /// The job thread waits on this for each attempt, once conditioning is
    /// ready. Dropping it ends the job thread after its current attempt.
    allocation: Option<SyncSender<std::result::Result<Allocated, String>>>,
    /// The attempt in progress.
    allocated: Option<Allocated>,
    /// Whether this job's first attempt has been allocated.
    started: bool,
    /// The seed for a new request, when the caller chose one.
    seed: Option<u64>,
    /// Pack/runtime identity captured when this job selected its worker.
    provider: ProviderSelection,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        // Only reached after Finished or a disconnected job thread; never
        // leave a cooperative job running without a cancellation request.
        self.cancelled.store(true, Ordering::Release);
    }
}

#[derive(Default)]
pub(super) struct State {
    backend: Backend,
    session: u64,
    running: Option<Running>,
    job: Option<Job>,
    candidates: Arc<BTreeMap<NodeId, Candidate>>,
    options: Arc<BTreeMap<NodeId, GenerationOptions>>,
    candidates_key: Option<(u64, RevisionId, u64)>,
    /// Interrupted attempts offered for retry, read with the candidates.
    interrupted: Arc<Vec<crate::project::generation::Interrupted>>,
    interrupted_warning: Option<String>,
    epoch: u64,
    preview: Option<Arc<CandidatePreview>>,
    reply: Option<(u64, Option<String>)>,
    /// Jobs started through the live endpoint, numbered apart from the UI's.
    remote: u64,
    /// Concluded live-endpoint jobs, kept for their caller until released,
    /// replaced by its next job or expired, even after the UI starts another.
    finished_remote: Vec<FinishedRemote>,
}

struct FinishedRemote {
    status: deadpan_cli::live_project::generation::GenerationStatus,
    expires: Instant,
}

/// Live-endpoint job tickets never collide with the UI's command tickets.
const REMOTE_TICKETS: u64 = 1 << 62;
/// The most concluded live-endpoint jobs kept, and for how long.
const MAX_FINISHED_REMOTE: usize = 8;
const FINISHED_REMOTE_RETENTION: Duration = Duration::from_secs(10 * 60);

/// One observation of `job` for the live endpoint.
fn remote_status(job: &Job) -> deadpan_cli::live_project::generation::GenerationStatus {
    use deadpan_cli::live_project::generation::{GenerationOutcome, GenerationStatus, bounded};
    let outcome = job.outcome.as_ref().map(|outcome| match outcome {
        Outcome::Ready(_) => GenerationOutcome::Ready {},
        Outcome::Cancelled => GenerationOutcome::Cancelled {},
        Outcome::Failed(reason) => GenerationOutcome::Failed {
            reason: bounded(reason),
        },
        Outcome::Unavailable(reason) => GenerationOutcome::Unavailable {
            reason: bounded(reason),
        },
    });
    GenerationStatus {
        job: job.ticket,
        hold: job.hold.clone(),
        options: job.options.clone(),
        request_id: job.request.clone(),
        variants: job.variants,
        variant: job.variant,
        ready: job.ready,
        stage: job.phase.label().into(),
        steps: job
            .phase
            .steps()
            .map(|(completed, total)| [completed, total]),
        elapsed_ms: u64::try_from(job.started.elapsed().as_millis()).unwrap_or(u64::MAX),
        outcome,
        note: job.note.as_deref().map(bounded),
    }
}

impl State {
    pub(super) fn new(backend: Backend) -> Self {
        Self {
            backend,
            ..Self::default()
        }
    }

    /// Stored variants changed outside this module; reread the candidates.
    pub(super) fn variants_changed(&mut self) {
        self.epoch += 1;
    }

    /// A remote client changed one offered variant through the live
    /// endpoint: reread the candidates and drop a preview that no longer
    /// shows the request's chosen, offered variant, as the native change does.
    pub(super) fn remote_variant_changed(
        &mut self,
        request: &RequestId,
        attempt: &AttemptId,
        action: deadpan_cli::generation::variants::VariantAction,
    ) {
        use deadpan_cli::generation::variants::VariantAction;
        self.epoch += 1;
        let stale = self.preview.as_ref().is_some_and(|preview| {
            preview.request() == request
                && match action {
                    VariantAction::Select => preview.attempt() != attempt,
                    VariantAction::Discard => preview.attempt() == attempt,
                    VariantAction::Keep | VariantAction::Release => false,
                }
        });
        if stale {
            self.preview = None;
        }
    }

    /// A job thread is live; the writer must not be released.
    pub(super) fn active(&self) -> bool {
        self.running.is_some()
    }

    fn reset(&mut self, session: u64) {
        debug_assert!(self.running.is_none());
        self.session = session;
        self.job = None;
        self.candidates = Arc::default();
        self.options = Arc::default();
        self.candidates_key = None;
        self.interrupted = Arc::default();
        self.interrupted_warning = None;
        self.preview = None;
        self.reply = None;
        self.finished_remote.clear();
    }
}

fn failure_text(failure: &JobFailure) -> String {
    match failure {
        JobFailure::Host(host) => host.detail.as_str().to_owned(),
        JobFailure::Worker(worker) => worker.detail.as_str().to_owned(),
    }
}

impl Service {
    /// Cancel any live job cooperatively; the pump drains it.
    pub(super) fn cancel_generation(&mut self) {
        if let Some(running) = &self.generation.running {
            running.cancelled.store(true, Ordering::Release);
            if let Some(job) = &mut self.generation.job
                && job.running()
            {
                job.phase = Phase::Cancelling;
            }
        }
    }

    /// Independent generation feedback. Accept is an ordinary edit and is
    /// handled through [`Service::accept_generation`].
    pub(super) fn generation_command(&mut self, operation: GenerationOperation) {
        let ticket = match &operation {
            GenerationOperation::Start { ticket, .. }
            | GenerationOperation::Cancel { ticket, .. }
            | GenerationOperation::Select { ticket, .. }
            | GenerationOperation::Preview { ticket, .. }
            | GenerationOperation::Discard { ticket, .. }
            | GenerationOperation::Keep { ticket, .. }
            | GenerationOperation::DismissInterrupted { ticket, .. } => *ticket,
            GenerationOperation::Accept { .. } => {
                unreachable!("acceptance is an ordinary edit")
            }
        };
        let result = match operation {
            GenerationOperation::Start {
                ticket,
                session,
                revision,
                hold,
                variants,
                options,
            } => self.start_generation(StartInput {
                ticket,
                session,
                revision,
                hold,
                variants,
                seed: None,
                options,
            }),
            GenerationOperation::Cancel {
                session,
                job: started,
                ..
            } => {
                let matches = self.generation.session == session
                    && self.generation.running.is_some()
                    && self
                        .generation
                        .job
                        .as_ref()
                        .is_some_and(|job| job.ticket == started && job.running());
                if matches {
                    self.cancel_generation();
                    self.message = Some("Cancelling the AI pause…".into());
                    Ok(())
                } else {
                    Err("No matching AI pause is generating.".into())
                }
            }
            GenerationOperation::Select {
                session,
                request,
                attempt,
                ..
            } => self.select_variant(session, &request, &attempt).map(|_| {
                self.message = Some("Chose this AI variant. Preview or accept it.".into());
            }),
            GenerationOperation::Preview {
                session,
                revision,
                request,
                attempt,
                draft,
                ..
            } => self.preview_generation(draft, session, &revision, request, attempt),
            GenerationOperation::Discard {
                session,
                request,
                attempt,
                ..
            } => self.discard_variant(session, &request, &attempt),
            GenerationOperation::Keep {
                session,
                request,
                attempt,
                keep,
                ..
            } => self.keep_variant(session, &request, &attempt, keep),
            GenerationOperation::DismissInterrupted {
                session,
                request,
                attempt,
                ..
            } => self.dismiss_interrupted(session, &request, &attempt),
            GenerationOperation::Accept { .. } => {
                unreachable!("acceptance is an ordinary edit")
            }
        };
        self.generation.reply = Some((ticket, result.err()));
    }

    /// Stop offering an interrupted attempt for retry; durable, not undoable.
    fn dismiss_interrupted(&mut self, session: u64, request: &str, attempt: &str) -> Result<()> {
        if self.generation.session != session
            || self
                .workspace
                .as_ref()
                .is_none_or(|workspace| workspace.session != session)
        {
            return Err("Project session changed before the request".into());
        }
        self.store
            .as_mut()
            .ok_or("Open a project first")?
            .dismiss_interrupted_generation(request, attempt)
            .map_err(display)?;
        self.generation.epoch += 1;
        self.message = Some(
            "Discarded the interrupted AI attempt from the list. The pause is unchanged.".into(),
        );
        Ok(())
    }

    /// The offered candidate of `request` that contains `attempt`.
    fn offered(&mut self, request: &RequestId, attempt: &AttemptId) -> Result<Candidate> {
        self.current_candidates()
            .values()
            .find(|candidate| {
                &candidate.request == request
                    && candidate
                        .variants
                        .iter()
                        .any(|variant| &variant.attempt == attempt)
            })
            .cloned()
            .ok_or_else(|| "These AI pictures are no longer offered for this pause.".into())
    }

    /// Make `attempt` the request's selected variant in the store, so the
    /// store's acceptance preview and acceptance use exactly it. Operational
    /// metadata only; nothing in the edit changes.
    fn select_variant(
        &mut self,
        session: u64,
        request: &RequestId,
        attempt: &AttemptId,
    ) -> Result<Candidate> {
        if self.generation.session != session
            || self
                .workspace
                .as_ref()
                .is_none_or(|workspace| workspace.session != session)
        {
            return Err("Project session changed before the request".into());
        }
        let candidate = self.offered(request, attempt)?;
        let identity = MessageIdentity::new(request.clone(), attempt.clone());
        let store = self.store.as_mut().ok_or("Open a project first")?;
        if store
            .selected_generation_bundle(request)
            .map_err(display)?
            .is_none_or(|selected| selected.identity != identity)
        {
            store
                .select_generation_bundle_variant(&identity)
                .map_err(display)?;
            self.generation.epoch += 1;
        }
        if self
            .generation
            .preview
            .as_ref()
            .is_some_and(|preview| preview.request() == request && preview.attempt() != attempt)
        {
            self.generation.preview = None;
        }
        Ok(candidate)
    }

    /// Durably discard one Ready variant: the store records it unavailable
    /// and it is never offered again. If it was selected, the newest other
    /// offered variant of the request becomes selected.
    fn discard_variant(
        &mut self,
        session: u64,
        request: &RequestId,
        attempt: &AttemptId,
    ) -> Result<()> {
        if self.generation.session != session
            || self
                .workspace
                .as_ref()
                .is_none_or(|workspace| workspace.session != session)
        {
            return Err("Project session changed before the request".into());
        }
        let candidate = self.offered(request, attempt)?;
        let identity = MessageIdentity::new(request.clone(), attempt.clone());
        // One store transaction: the variant becomes unavailable and, only if
        // it was the chosen one, the newest remaining variant is chosen.
        self.store
            .as_mut()
            .ok_or("Open a project first")?
            .discard_generation_bundle_variant(&identity)
            .map_err(display)?;
        if self
            .generation
            .preview
            .as_ref()
            .is_some_and(|preview| preview.attempt() == attempt)
        {
            self.generation.preview = None;
        }
        self.generation.epoch += 1;
        self.message = Some(if candidate.variants.len() > 1 {
            "Removed this AI variant from the list for good; its files stay in the project until a cleanup removes them. The pause is unchanged.".into()
        } else {
            "Removed the AI pictures from the list for good; their files stay in the project until a cleanup removes them. The pause is unchanged; generate again for new pictures."
                .into()
        });
        Ok(())
    }

    /// Keep (pin) or stop keeping one offered variant. Operational store
    /// metadata only: durable, not undoable, and the pause is unchanged.
    fn keep_variant(
        &mut self,
        session: u64,
        request: &RequestId,
        attempt: &AttemptId,
        keep: bool,
    ) -> Result<()> {
        if self.generation.session != session
            || self
                .workspace
                .as_ref()
                .is_none_or(|workspace| workspace.session != session)
        {
            return Err("Project session changed before the request".into());
        }
        self.offered(request, attempt)?;
        let identity = MessageIdentity::new(request.clone(), attempt.clone());
        self.store
            .as_mut()
            .ok_or("Open a project first")?
            .keep_generation_bundle_variant(&identity, keep)
            .map_err(display)?;
        self.generation.epoch += 1;
        let days = DEFAULT_VARIANT_RETENTION.as_secs() / (24 * 60 * 60);
        self.message = Some(if keep {
            "Kept this AI variant: it stays offered until you discard or accept it. The pause is unchanged.".into()
        } else {
            format!(
                "Stopped keeping this AI variant: unless you choose or accept it, it stops being offered {days} days after it was generated. The pause is unchanged."
            )
        });
        Ok(())
    }

    /// A new Ready variant took the selection from the variant chosen when
    /// the job started: if nothing else protects that one now, record it on
    /// the job and say it will expire.
    fn note_unprotected(&mut self, request: &RequestId) {
        let Some(before) = self
            .generation
            .job
            .as_ref()
            .and_then(|job| job.selected_before.clone())
        else {
            return;
        };
        let candidates = self.current_candidates();
        let Some((number, _)) = candidates
            .values()
            .filter(|candidate| &candidate.request == request && candidate.selected != before)
            .flat_map(|candidate| candidate.variants.iter().enumerate())
            .find(|(_, variant)| variant.attempt == before && variant.expires_at.is_some())
        else {
            return;
        };
        let days = DEFAULT_VARIANT_RETENTION.as_secs() / (24 * 60 * 60);
        if let Some(job) = &mut self.generation.job {
            job.unprotected = Some(before);
        }
        let warning = format!(
            "Variant {} is no longer the chosen one; unless you keep it (:keep-ai) or choose it, it stops being offered {days} days after it was generated.",
            number + 1
        );
        match &mut self.message {
            Some(message) => {
                message.push(' ');
                message.push_str(&warning);
            }
            None => self.message = Some(warning),
        }
    }

    fn start_generation(&mut self, input: StartInput) -> Result<()> {
        let StartInput {
            ticket,
            session,
            revision,
            hold,
            variants,
            seed,
            options,
        } = input;
        self.check_context(session, &revision)?;
        if self.pending_session_change.is_some() {
            return Err("The project is closing or changing.".into());
        }
        if !(1..=MAX_VARIANTS).contains(&variants) {
            return Err(format!(
                "Generate 1 to {MAX_VARIANTS} AI variants at a time."
            ));
        }
        if self.generation.running.is_some() {
            return Err(
                "An AI pause is already generating. Cancel it with :cancel-ai first.".into(),
            );
        }
        let selected_before = self
            .current_candidates()
            .get(&hold)
            .map(|candidate| candidate.selected.clone());
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        if !matches!(
            workspace.document.nodes().get(&hold).map(|node| &node.kind),
            Some(NodeKind::Hold { .. })
        ) {
            return Err("AI pictures fill a pause. Select a pause (Hold) beat first.".into());
        }
        let label = workspace
            .document
            .nodes()
            .get(&hold)
            .map(|node| node.label.clone())
            .unwrap_or_default();
        let package = workspace.path.clone();
        let options = options.or_else(|| {
            self.generation
                .job
                .as_ref()
                .filter(|job| job.hold == hold && job.revision == revision && job.request.is_none())
                .map(|job| job.options.clone())
        });
        let options = match options {
            Some(options) => options,
            None => attempt::current_bridge_request(
                self.store.as_ref().ok_or("Open a project first")?,
                &hold,
            )
            .map_err(display)?
            .as_ref()
            .map(|request| GenerationOptions::from_constraints(&request.constraints))
            .unwrap_or_default(),
        };
        let job = Job {
            ticket,
            session,
            hold: hold.clone(),
            options: options.clone(),
            revision: revision.clone(),
            started: Instant::now(),
            request: None,
            variants,
            variant: 1,
            ready: 0,
            phase: Phase::Conditioning,
            outcome: None,
            note: None,
            selected_before,
            unprotected: None,
        };
        let worker = match &self.generation.backend {
            Backend::Environment => match BridgeRuntime::from_environment() {
                Ok(runtime) => Worker::Real(Box::new(runtime)),
                Err(error) => {
                    self.conclude_unavailable(job, error.to_string());
                    return Ok(());
                }
            },
            #[cfg(any(test, feature = "ui-harness"))]
            Backend::Scripted(queue) => {
                let script = queue.next().ok_or("The scripted AI worker has no runs.")?;
                if let Some(error) = &script.unavailable {
                    self.conclude_unavailable(job, error.clone());
                    return Ok(());
                }
                Worker::Scripted {
                    queue: queue.clone(),
                    first: Some(script),
                }
            }
        };
        let provider = match &worker {
            Worker::Real(runtime) => runtime.provider(seed.unwrap_or(0)),
            #[cfg(any(test, feature = "ui-harness"))]
            Worker::Scripted { .. } => {
                deadpan_cli::generation::development_provider(seed.unwrap_or(0))
            }
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let (events, receive_events) = mpsc::sync_channel(EVENT_CAPACITY);
        let (finished, receive_finished) = mpsc::sync_channel(1);
        let (allocation, receive_allocation) = mpsc::sync_channel(1);
        let thread_cancelled = cancelled.clone();
        let revision = revision.clone();
        // Registered before the thread starts, so the Jobs panel and the
        // inference budget see the job from its first instant.
        let handle = self.shared.job_board.register(
            crate::jobs::JobSpec::new(crate::jobs::JobKind::AiPause, Some(session))
                // Conditioning and publication run without the model slot.
                .deferred()
                .detail(if variants > 1 {
                    format!("{label} · {variants} variants")
                } else {
                    label
                })
                .cancel_flag(&cancelled),
        );
        let thread = std::thread::Builder::new()
            .name("deadpan-ai-pause".into())
            .spawn(move || {
                job_thread(
                    worker,
                    JobInput {
                        package,
                        revision,
                        hold,
                        options,
                    },
                    thread_cancelled,
                    Channels {
                        events,
                        finished,
                        allocation: receive_allocation,
                    },
                    handle,
                );
            })
            .map_err(|error| format!("Could not start the AI pause job: {error}"))?;
        self.generation.running = Some(Running {
            cancelled,
            events: receive_events,
            finished: receive_finished,
            allocation: Some(allocation),
            allocated: None,
            started: false,
            seed,
            provider,
            thread: Some(thread),
        });
        self.generation.job = Some(job);
        self.message = Some(if variants > 1 {
            format!("Generating {variants} AI variants for this pause…")
        } else {
            "Generating AI pictures for this pause…".into()
        });
        Ok(())
    }

    /// Nothing was recorded; the reason names every missing part.
    fn conclude_unavailable(&mut self, job: Job, reason: String) {
        self.generation.job = Some(job);
        self.conclude_generation(Outcome::Unavailable(reason));
    }

    fn preview_generation(
        &mut self,
        draft: u64,
        session: u64,
        revision: &RevisionId,
        request: RequestId,
        attempt: AttemptId,
    ) -> Result<()> {
        self.check_context(session, revision)?;
        if draft == 0 {
            return Err("The AI preview needs a fresh proposal identity.".into());
        }
        let candidate = self.select_variant(session, &request, &attempt)?;
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open a project first")?
            .clone();
        let range = workspace
            .plan
            .single_occurrence_range(&candidate.hold)
            .ok_or("The pause no longer plays once in this edit.")?;
        let store = self.store.as_ref().ok_or("Open a project first")?;
        let acceptance =
            acceptance::acceptance_for(store, &request, revision_id()).map_err(display)?;
        if acceptance.identity.attempt_id != attempt {
            return Err("The selected AI variant changed; choose it again.".into());
        }
        let edit = store
            .preview_generation_acceptance(&acceptance, attempt::object_limits())
            .map_err(display)?;
        let after = Arc::new(edit.forward.apply(&workspace.document).map_err(display)?);
        // The same proposed document, admitted for audition against the
        // exact committed base: accepting pictures leaves the pause's sound
        // unchanged, so pictures and sound play together as they would after
        // acceptance.
        let audio = Arc::new(
            deadpan_playback::Snapshot::proposed_generated(
                &workspace.playback_snapshot(),
                after.clone(),
                draft,
                1,
            )
            .map_err(display)?,
        );
        self.generation.preview = Some(Arc::new(CandidatePreview::new(
            &workspace.document,
            PreviewParts {
                session,
                request,
                attempt,
                hold: candidate.hold,
                range,
                document: after,
                audio,
            },
        )?));
        Ok(())
    }

    /// Commit the candidate through the store's explicit acceptance path.
    pub(super) fn accept_generation(&mut self, operation: GenerationOperation) -> Result<()> {
        let GenerationOperation::Accept {
            session,
            revision,
            request,
            attempt,
            hold,
            cursor,
            scope,
        } = operation
        else {
            unreachable!("only acceptance is an ordinary edit");
        };
        self.check_context(session, &revision)?;
        let candidate = self.select_variant(session, &request, &attempt)?;
        if candidate.hold != hold {
            return Err("These AI pictures are no longer offered for this pause.".into());
        }
        let outcome = acceptance::accept(self.writer()?, &request, revision_id())
            .map_err(|error| error.to_string())?;
        self.generation.preview = None;
        self.generation.epoch += 1;
        self.committed = Some(CommittedEdit {
            scoped: None,
            revision: outcome.revision_id,
            selected_node: Some(hold),
            preserve_cursor: true,
            cursor: Some(cursor),
            scope,
            sound: None,
            range_selection: None,
        });
        self.refresh_saved("AI pictures accepted")?;
        self.message =
            Some("Accepted the AI pictures for this pause and saved. Undo restores the previous picture.".into());
        Ok(())
    }

    fn current_candidates(&mut self) -> Arc<BTreeMap<NodeId, Candidate>> {
        let Some(workspace) = &self.workspace else {
            return Arc::default();
        };
        if self.generation.session != workspace.session && self.generation.running.is_none() {
            self.generation.reset(workspace.session);
        }
        let key = (
            workspace.session,
            workspace.document.revision_id().clone(),
            self.generation.epoch,
        );
        if self.generation.candidates_key.as_ref() == Some(&key) {
            return self.generation.candidates.clone();
        }
        let Some(store) = &self.store else {
            return Arc::default();
        };
        let (interrupted, warning) = match store.interrupted_generation_attempts() {
            Ok(found) => (found.attempts, found.warning),
            Err(error) => (
                Vec::new(),
                Some(format!(
                    "Interrupted AI attempts could not be read: {error}"
                )),
            ),
        };
        self.generation.interrupted_warning = warning;
        self.generation.options = match store.current_generation_requests() {
            Ok(requests) => Arc::new(
                requests
                    .into_iter()
                    .filter(|request| request.bridge_plan.is_some())
                    .map(|request| {
                        (
                            request.binding.hold_id,
                            GenerationOptions::from_constraints(&request.constraints),
                        )
                    })
                    .collect(),
            ),
            Err(error) => {
                self.message = Some(format!(
                    "Could not read this project's AI controls: {error}"
                ));
                Arc::default()
            }
        };
        self.generation.interrupted = Arc::new(
            interrupted
                .into_iter()
                .map(|attempt| {
                    let pause = NodeId::new(attempt.hold_id.clone())
                        .ok()
                        .and_then(|hold| workspace.document.nodes().get(&hold))
                        .filter(|node| matches!(node.kind, NodeKind::Hold { .. }))
                        .map(|node| node.label.clone());
                    crate::project::generation::Interrupted {
                        request: attempt.request_id,
                        attempt: attempt.attempt_id,
                        hold: attempt.hold_id,
                        pause,
                    }
                })
                .collect(),
        );
        match candidates(store, &workspace.document) {
            Ok(found) => {
                self.generation.candidates = Arc::new(found);
            }
            Err(error) => {
                self.generation.candidates = Arc::default();
                self.message = Some(format!(
                    "Could not read this project's AI pictures: {error}"
                ));
            }
        }
        self.generation.candidates_key = Some(key);
        self.generation.candidates.clone()
    }

    pub(super) fn generation_update(&mut self) -> Option<Update> {
        let session = self.workspace.as_ref()?.session;
        let candidates = self.current_candidates();
        let preview = self.generation.preview.clone().filter(|preview| {
            self.workspace.as_ref().is_some_and(|workspace| {
                preview.session() == workspace.session
                    && preview.base() == workspace.document.revision_id()
            })
        });
        Some(Update {
            session,
            job: self
                .generation
                .job
                .clone()
                .filter(|job| job.session == session),
            candidates,
            options: self.generation.options.clone(),
            preview,
            reply: self.generation.reply.clone(),
            interrupted: self.generation.interrupted.clone(),
            interrupted_warning: self.generation.interrupted_warning.clone(),
        })
    }

    /// Apply queued job events on the writer. True when published state changed.
    pub(super) fn pump_generation(&mut self) -> bool {
        let mut changed = false;
        for _ in 0..EVENT_CAPACITY {
            let Some(running) = &self.generation.running else {
                return changed;
            };
            match running.events.try_recv() {
                Ok(event) => {
                    changed = true;
                    self.generation_event(event);
                }
                // Every record was acknowledged before the run returned, so
                // the final run follows all durable transitions.
                Err(mpsc::TryRecvError::Empty) => match running.finished.try_recv() {
                    Ok(run) => {
                        self.generation_finished(run);
                        return true;
                    }
                    Err(_) => return changed,
                },
                Err(mpsc::TryRecvError::Disconnected) => {
                    match running.finished.try_recv() {
                        Ok(run) => self.generation_finished(run),
                        Err(_) => self.generation_stopped(),
                    }
                    return true;
                }
            }
        }
        changed
    }

    fn generation_event(&mut self, event: Event) {
        match event {
            Event::Prepared(prepared) => self.generation_prepared(prepared),
            Event::Queued => {
                if let Some(job) = &mut self.generation.job
                    && job.phase != Phase::Cancelling
                {
                    job.phase = Phase::Queued;
                }
            }
            Event::Admitted => {
                if let Some(job) = &mut self.generation.job
                    && job.phase == Phase::Queued
                {
                    job.phase = Phase::Preparing;
                }
            }
            Event::Superseded => {
                if let Some(job) = &mut self.generation.job {
                    let note = "The pause changed while this attempt waited for the AI model, so the model was not started. Generate again.";
                    job.note = Some(match job.note.take() {
                        Some(earlier) => format!("{earlier} {note}"),
                        None => note.into(),
                    });
                }
            }
            Event::Progress(progress) => {
                if let Some(job) = &mut self.generation.job
                    && job.phase != Phase::Cancelling
                {
                    job.phase = progress_phase(progress);
                }
            }
            Event::Record(record, acknowledge) => {
                let result = match (
                    self.store.as_mut(),
                    self.generation
                        .running
                        .as_ref()
                        .and_then(|running| running.allocated.as_ref()),
                ) {
                    (Some(store), Some(allocated)) => attempt::record(store, allocated, &record)
                        .map_err(|error| error.to_string()),
                    _ => Err("The project writer is unavailable.".into()),
                };
                if record == AttemptRecord::CancelRequested
                    && let Some(job) = &mut self.generation.job
                {
                    job.phase = Phase::Cancelling;
                }
                // The job thread may have timed out; it then fails the attempt.
                let _ = acknowledge.try_send(result);
            }
        }
    }

    fn generation_prepared(&mut self, prepared: std::result::Result<Box<BridgeInputs>, String>) {
        let Some(running) = &mut self.generation.running else {
            return;
        };
        if running.started || running.allocation.is_none() {
            return;
        }
        let cancelled = running.cancelled.load(Ordering::Acquire);
        let requested_seed = running.seed;
        let provider = running.provider.clone();
        let Some((hold, revision)) = self
            .generation
            .job
            .as_ref()
            .map(|job| (job.hold.clone(), job.revision.clone()))
        else {
            return;
        };
        let inputs = match prepared {
            // Nothing was recorded; refusing the allocation ends the job thread.
            Ok(_) if cancelled => {
                self.refuse_allocation("cancelled".into());
                self.conclude_generation(Outcome::Cancelled);
                return;
            }
            Err(_) if cancelled => {
                self.refuse_allocation("cancelled".into());
                self.conclude_generation(Outcome::Cancelled);
                return;
            }
            Err(error) => {
                self.refuse_allocation(error.clone());
                self.conclude_generation(Outcome::Failed(format!(
                    "The pause's boundary pictures could not be prepared: {error}"
                )));
                return;
            }
            Ok(inputs) => *inputs,
        };
        let allocated = (|| -> std::result::Result<Allocated, String> {
            let store = self
                .store
                .as_mut()
                .ok_or("The project writer is unavailable.")?;
            if store.head_revision().map_err(display)? != revision {
                return Err(
                    "The project changed while the pause's pictures were being read. Generate again."
                        .into(),
                );
            }
            // Unchanged boundary pictures add variants to the Hold's current
            // request; anything else is a new request that supersedes it.
            let existing = attempt::current_bridge_request(store, &hold)
                .map_err(display)?
                .filter(|request| {
                    request.binding.context_sha256 == inputs.manifest_sha256
                        && request.constraints == inputs.constraints
                        && request.bridge_plan.as_ref() == Some(&inputs.plan)
                        && deadpan_cli::generation::same_provider_identity(
                            &request.provider,
                            &provider,
                        )
                });
            match existing {
                // Variants of an existing request derive their seeds from its
                // own seed; an explicit seed cannot apply and is refused.
                Some(request) if requested_seed.is_some() => {
                    return Err(format!(
                        "This pause already has AI variants from request {} (seed {}); its new variants derive their seeds from that. Generate without a seed to add one, or change the pause to start a new request.",
                        request.request_id, request.provider.seed
                    ));
                }
                Some(request) => attempt::allocate_variant(store, request, inputs),
                None => attempt::allocate_with_provider(
                    store,
                    AllocateInput {
                        hold,
                        expected_revision: revision,
                        seed: requested_seed.unwrap_or_else(|| {
                            let random = uuid::Uuid::new_v4();
                            let mut seed = [0; 4];
                            seed.copy_from_slice(&random.as_bytes()[..4]);
                            u64::from(u32::from_le_bytes(seed))
                        }),
                        inputs,
                    },
                    provider,
                ),
            }
            .map_err(|error: GenerationError| error.to_string())
        })();
        match allocated {
            Ok(allocated) => self.dispatch_attempt(allocated),
            Err(error) => {
                self.refuse_allocation(error.clone());
                self.conclude_generation(Outcome::Failed(error));
            }
        }
    }

    /// Hand the job thread its next attempt.
    fn dispatch_attempt(&mut self, allocated: Allocated) {
        if let Some(job) = &mut self.generation.job {
            job.request = Some(allocated.request.request_id.clone());
            job.phase = Phase::Preparing;
        }
        // A new request supersedes the Hold's earlier candidate; a new
        // variant changes which one the store selects when it is Ready.
        self.generation.epoch += 1;
        if self
            .generation
            .preview
            .as_ref()
            .is_some_and(|preview| preview.request() != &allocated.request.request_id)
        {
            self.generation.preview = None;
        }
        if let Some(running) = &mut self.generation.running {
            running.started = true;
            running.allocated = Some(allocated.clone());
            if let Some(allocation) = &running.allocation {
                // If the job thread is already gone, its disconnect records
                // the allocated attempt's failure.
                let _ = allocation.try_send(Ok(allocated));
            }
        }
    }

    /// Tell a job thread waiting for an attempt that none follows.
    fn refuse_allocation(&mut self, reason: String) {
        if let Some(running) = &mut self.generation.running
            && let Some(allocation) = running.allocation.take()
        {
            let _ = allocation.try_send(Err(reason));
        }
    }

    fn generation_finished(&mut self, run: WorkerRun) {
        let Some(running) = &mut self.generation.running else {
            return;
        };
        let cancelled = running.cancelled.load(Ordering::Acquire);
        let allocated = running.allocated.take();
        let outcome = match (allocated.as_ref(), self.store.as_mut()) {
            (Some(allocated), Some(store)) => match attempt::finish(store, allocated, run) {
                Ok(finished) => match finished.state {
                    JobState::Ready => Outcome::Ready(allocated.request.request_id.clone()),
                    JobState::Cancelled => Outcome::Cancelled,
                    _ => Outcome::Failed(
                        finished
                            .failure
                            .as_ref()
                            .map_or_else(|| "The AI pause did not finish.".into(), failure_text),
                    ),
                },
                Err(error) => Outcome::Failed(error.to_string()),
            },
            _ => match run.result {
                RunResult::Cancelled => Outcome::Cancelled,
                _ => Outcome::Failed("The AI pause stopped before it was recorded.".into()),
            },
        };
        self.generation.epoch += 1;
        let next = match (&outcome, &mut self.generation.job, allocated) {
            (Outcome::Ready(_), Some(job), Some(allocated)) => {
                job.ready = job.ready.saturating_add(1);
                (job.variant < job.variants && !cancelled && self.pending_session_change.is_none())
                    .then_some(allocated)
            }
            _ => None,
        };
        if let Some(previous) = next {
            let store = self.store.as_mut();
            let allocated = store
                .ok_or_else(|| "The project writer is unavailable.".to_owned())
                .and_then(|store| {
                    attempt::allocate_variant(
                        store,
                        previous.request.clone(),
                        previous.inputs().clone(),
                    )
                    .map_err(|error| error.to_string())
                });
            match allocated {
                Ok(allocated) => {
                    if let Some(job) = &mut self.generation.job {
                        job.variant += 1;
                    }
                    self.message = self.generation.job.as_ref().map(|job| {
                        format!(
                            "AI variant {} of {} is ready; generating variant {}…",
                            job.ready, job.variants, job.variant
                        )
                    });
                    self.dispatch_attempt(allocated);
                    return;
                }
                Err(error) => {
                    // The variants already made stay Ready; say why no more
                    // follow (for example, the pause changed meanwhile).
                    if let Some(job) = &mut self.generation.job {
                        job.note = Some(format!(
                            "{} of {} AI variants are ready; variant {} could not start: {error}",
                            job.ready,
                            job.variants,
                            job.variant + 1
                        ));
                    }
                    self.finish_job_thread();
                    self.conclude_generation(outcome);
                    return;
                }
            }
        }
        let outcome = match (&outcome, &mut self.generation.job) {
            // Cancelled between variants: the Ready ones are kept.
            (Outcome::Ready(_), Some(job)) if cancelled && job.variant < job.variants => {
                job.note = Some(format!(
                    "Cancelled after {} of {} AI variants; the ready ones are kept.",
                    job.ready, job.variants
                ));
                Outcome::Cancelled
            }
            (Outcome::Cancelled | Outcome::Failed(_), Some(job)) if job.ready > 0 => {
                job.note = Some(format!(
                    "{} earlier AI variant{} of this job {} ready; variant {} {}.",
                    job.ready,
                    if job.ready == 1 { "" } else { "s" },
                    if job.ready == 1 { "is" } else { "are" },
                    job.variant,
                    if matches!(outcome, Outcome::Cancelled) {
                        "was cancelled"
                    } else {
                        "failed"
                    }
                ));
                outcome
            }
            _ => outcome,
        };
        self.finish_job_thread();
        self.conclude_generation(outcome);
    }

    /// End and reap the job thread once no attempt follows.
    fn finish_job_thread(&mut self) {
        let Some(mut running) = self.generation.running.take() else {
            return;
        };
        // Dropping the allocation sender ends the thread's wait.
        running.allocation = None;
        if let Some(thread) = running.thread.take() {
            let _ = thread.join();
        }
    }

    /// The job thread ended without sending a run for its current attempt.
    fn generation_stopped(&mut self) {
        let Some(mut running) = self.generation.running.take() else {
            return;
        };
        running.allocation = None;
        let panicked = running
            .thread
            .take()
            .is_some_and(|thread| thread.join().is_err());
        if self
            .generation
            .job
            .as_ref()
            .is_some_and(|job| !job.running())
        {
            return;
        }
        let cancelled = running.cancelled.load(Ordering::Acquire);
        let outcome = if let (Some(allocated), Some(store)) =
            (running.allocated.as_ref(), self.store.as_mut())
        {
            let result = if cancelled {
                RunResult::Cancelled
            } else {
                RunResult::Failed(JobFailure::Host(attempt::host_failure(
                    HostFailureCode::WorkerExited,
                    "the AI pause job stopped unexpectedly",
                )))
            };
            match attempt::finish(
                store,
                allocated,
                WorkerRun::without_workspace(result, RunTimings::default()),
            ) {
                Ok(finished) if finished.state == JobState::Cancelled => Outcome::Cancelled,
                Ok(_) => Outcome::Failed("The AI pause job stopped unexpectedly.".into()),
                Err(error) => Outcome::Failed(error.to_string()),
            }
        } else if cancelled {
            Outcome::Cancelled
        } else {
            Outcome::Failed(if panicked {
                "The AI pause job stopped unexpectedly.".into()
            } else {
                "The AI pause job ended before reporting a result.".into()
            })
        };
        self.conclude_generation(outcome);
    }

    fn conclude_generation(&mut self, outcome: Outcome) {
        self.generation.epoch += 1;
        // A Ready attempt is offered only while its request is current and
        // its Hold has not accepted it; derive the message from that.
        let offered = match &outcome {
            Outcome::Ready(request) => self
                .current_candidates()
                .values()
                .any(|candidate| &candidate.request == request),
            _ => false,
        };
        let earlier = self.generation.job.as_ref().map_or(0, |job| job.ready);
        self.message = Some(match &outcome {
            Outcome::Ready(_) if offered => {
                if earlier > 1 {
                    format!(
                        "{earlier} AI variants are ready. Choose one, preview it, then accept or discard."
                    )
                } else {
                    "AI pictures are ready. Preview them, then accept or discard.".into()
                }
            }
            Outcome::Ready(_) => {
                "AI pictures finished, but the pause changed meanwhile; generate again.".into()
            }
            Outcome::Cancelled if earlier > 0 => {
                "The AI job was cancelled; the pause is unchanged.".into()
            }
            Outcome::Cancelled => "The AI pause was cancelled; the pause is unchanged.".into(),
            Outcome::Failed(_) => "The AI pause failed; the pause is unchanged.".into(),
            Outcome::Unavailable(_) => {
                "AI pauses are unavailable on this Mac; the inspector lists what is missing.".into()
            }
        });
        if let Some(note) = self
            .generation
            .job
            .as_ref()
            .and_then(|job| job.note.clone())
            && let Some(message) = &mut self.message
        {
            message.push(' ');
            message.push_str(&note);
        }
        if let Outcome::Ready(request) = &outcome {
            self.note_unprotected(request);
        }
        if let Some(job) = &mut self.generation.job
            && job.outcome.is_none()
        {
            job.outcome = Some(outcome);
            if job.ticket >= REMOTE_TICKETS {
                let now = Instant::now();
                let status = remote_status(job);
                let finished = &mut self.generation.finished_remote;
                finished.retain(|entry| entry.expires > now && entry.status.job != status.job);
                if finished.len() >= MAX_FINISHED_REMOTE {
                    finished.remove(0);
                }
                finished.push(FinishedRemote {
                    status,
                    expires: now + FINISHED_REMOTE_RETENTION,
                });
            }
        }
        // A job concluded before its first attempt has nothing left to
        // record. Its thread has already returned or is returning from a
        // refused allocation, so reap it now; an immediate restart is then
        // admitted.
        if self
            .generation
            .running
            .as_ref()
            .is_some_and(|running| running.allocated.is_none() && running.allocation.is_none())
        {
            self.finish_job_thread();
        }
    }
}

/// The live endpoint's view of the AI job: it starts the same job the UI's
/// `:generate` does, observes it and cancels exactly it.
impl Service {
    pub(super) fn host_generate(
        &mut self,
        project: &ProjectId,
        request: deadpan_cli::live_project::generation::GenerateRequest,
    ) -> std::result::Result<deadpan_cli::live_project::Reply, deadpan_cli::live_project::LiveError>
    {
        use deadpan_cli::live_project::LiveError;
        request.validate()?;
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| LiveError::new("HostProjectChanged", "No project is open"))?;
        if workspace.document.project_id() != project {
            return Err(LiveError::new(
                "HostProjectChanged",
                "The request names another project",
            ));
        }
        let session = workspace.session;
        if workspace.session != self.generation.session && self.generation.running.is_none() {
            self.generation.reset(session);
        }
        self.generation.remote = self.generation.remote.wrapping_add(1);
        let ticket = REMOTE_TICKETS + (self.generation.remote % REMOTE_TICKETS);
        self.start_generation(StartInput {
            ticket,
            session,
            revision: request.expected_revision,
            hold: request.hold,
            variants: request.variants,
            seed: request.seed,
            options: request.options,
        })
        .map_err(|error| LiveError::new("GenerationRefused", error))?;
        self.publish();
        self.host_generation_status(ticket)
    }

    pub(super) fn host_generation_status(
        &self,
        ticket: u64,
    ) -> std::result::Result<deadpan_cli::live_project::Reply, deadpan_cli::live_project::LiveError>
    {
        let current = self.generation.job.as_ref().filter(|job| {
            job.ticket == ticket
                && self
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.session == job.session)
        });
        let now = Instant::now();
        let status = match current {
            Some(job) => remote_status(job),
            None => self
                .generation
                .finished_remote
                .iter()
                .find(|entry| entry.status.job == ticket && entry.expires > now)
                .map(|entry| entry.status.clone())
                .ok_or_else(|| {
                    deadpan_cli::live_project::LiveError::new(
                        "GenerationUnknown",
                        "That AI job is neither running nor among the project's retained results; inspect the project's requests",
                    )
                })?,
        };
        Ok(deadpan_cli::live_project::Reply::Generation {
            status: Box::new(status),
        })
    }

    /// The caller has its result; forget the retained observation.
    pub(super) fn host_release_generation(
        &mut self,
        ticket: u64,
    ) -> std::result::Result<deadpan_cli::live_project::Reply, deadpan_cli::live_project::LiveError>
    {
        if self
            .generation
            .job
            .as_ref()
            .is_some_and(|job| job.ticket == ticket && job.running())
        {
            return Err(deadpan_cli::live_project::LiveError::new(
                "GenerationBusy",
                "The AI job is still running",
            ));
        }
        self.generation
            .finished_remote
            .retain(|entry| entry.status.job != ticket);
        Ok(deadpan_cli::live_project::Reply::Released)
    }

    pub(super) fn host_cancel_generation(
        &mut self,
        ticket: u64,
    ) -> std::result::Result<deadpan_cli::live_project::Reply, deadpan_cli::live_project::LiveError>
    {
        let reply = self.host_generation_status(ticket)?;
        if self.generation.running.is_some()
            && self
                .generation
                .job
                .as_ref()
                .is_some_and(|job| job.ticket == ticket && job.running())
        {
            self.cancel_generation();
            self.message = Some("Cancelling the AI pause…".into());
            self.publish();
            return self.host_generation_status(ticket);
        }
        Ok(reply)
    }
}

/// Every current bridge request's present Ready variants that its Hold has
/// not accepted, by Hold.
/// The shared definition of offered variants, with the inspector's derived
/// thumbnail fields.
fn candidates(
    store: &ProjectStore,
    document: &ProjectDocument,
) -> std::result::Result<BTreeMap<NodeId, Candidate>, StoreError> {
    Ok(deadpan_cli::generation::variants::offered(store, document)?
        .into_iter()
        .map(|(hold, offered)| {
            let variants = offered
                .variants
                .into_iter()
                .map(|variant| {
                    let video = variant.receipt.sampled_video();
                    Variant {
                        attempt: variant.attempt,
                        ordinal: variant.ordinal,
                        seed: variant.seed,
                        sampled: variant.receipt.sampled_object().clone(),
                        sampled_frames: u32::try_from(video.frames().frames()).unwrap_or(u32::MAX),
                        sampled_size: (video.width(), video.height()),
                        receipt: variant.receipt,
                        ready_at: variant.ready_at,
                        kept: variant.kept,
                        picked: variant.picked,
                        expires_at: variant.expires_at,
                    }
                })
                .collect();
            (
                hold,
                Candidate {
                    request: offered.request,
                    hold: offered.hold,
                    origin: offered.origin,
                    frames: offered.frames,
                    variants,
                    selected: offered.selected,
                },
            )
        })
        .collect())
}

fn revision_id() -> RevisionId {
    revision()
}

/// Queue `event`, waiting for the writer to drain or stop.
fn send(events: &SyncSender<Event>, event: Event) -> std::result::Result<(), String> {
    events
        .send(event)
        .map_err(|_| "The project writer stopped.".to_owned())
}

/// The job thread's ends of its channels to the writer.
struct Channels {
    events: SyncSender<Event>,
    finished: SyncSender<WorkerRun>,
    allocation: Receiver<std::result::Result<Allocated, String>>,
}

/// What runs for each attempt: the real supervised worker, or the test seam.
enum Worker {
    Real(Box<BridgeRuntime>),
    #[cfg(any(test, feature = "ui-harness"))]
    Scripted {
        queue: Arc<crate::project::generation::ScriptQueue>,
        /// The first attempt's script, taken when the job started.
        first: Option<crate::project::generation::Script>,
    },
}

fn progress_phase(progress: AttemptProgress) -> Phase {
    match progress {
        AttemptProgress::Preparing => Phase::Preparing,
        AttemptProgress::Stage(stage) => Phase::Stage(stage),
        AttemptProgress::Step {
            stage,
            completed,
            total,
        } => Phase::Step {
            stage,
            completed,
            total,
        },
        AttemptProgress::Qualifying => Phase::Qualifying,
    }
}

fn job_thread(
    mut worker: Worker,
    input: JobInput,
    cancelled: Arc<AtomicBool>,
    channels: Channels,
    handle: crate::jobs::JobHandle,
) {
    let JobInput {
        package,
        revision,
        hold,
        options,
    } = input;
    let Channels {
        events,
        finished,
        allocation,
    } = channels;
    let prepared =
        conditioning::prepare(&package, &revision, &hold, &cancelled).map(|mut inputs| {
            options.apply_to(&mut inputs.constraints);
            inputs
        });
    let ready = prepared.is_ok();
    if send(&events, Event::Prepared(prepared.map(Box::new))).is_err() || !ready {
        return;
    }
    // Conditioning, allocation and publication are short reads and records;
    // only each model run holds the inference slot, so an edit made while
    // queued cannot stale the captured boundary pictures, and a finished run
    // releases the model before the writer publishes it.
    // One attempt per allocation; the writer drops the sender when no
    // further variant follows.
    while let Ok(Ok(allocated)) = allocation.recv() {
        // Claim the model; tell the writer only when it must wait.
        handle.claim();
        if !handle.admitted() {
            let _ = events.try_send(Event::Queued);
        }
        if handle.wait_admitted(Some(&cancelled)).is_err() {
            // Cancelled or closing while queued: the worker never starts
            // and the attempt is recorded Cancelled below.
            cancelled.store(true, Ordering::Release);
        } else if !request_current(&package, &allocated.request.request_id) {
            // Edits while it waited made the request stale; loading the model
            // now would only produce pictures nobody can accept.
            let _ = events.try_send(Event::Superseded);
            cancelled.store(true, Ordering::Release);
        }
        let _ = events.try_send(Event::Admitted);
        let progress = |progress: AttemptProgress| {
            let phase = progress_phase(progress.clone());
            let fraction = phase
                .steps()
                .map(|(completed, total)| completed as f32 / total.max(1) as f32);
            handle.set_progress(phase.label(), fraction);
            // Display-only; a full queue drops one step, never the worker.
            let _ = events.try_send(Event::Progress(progress));
        };
        let records = |record: AttemptRecord| -> std::result::Result<(), String> {
            let (acknowledge, acknowledged) = mpsc::sync_channel(1);
            send(&events, Event::Record(record, acknowledge))?;
            acknowledged.recv().map_err(|_| {
                "The project writer stopped before recording the AI pause.".to_owned()
            })?
        };
        let run = match &mut worker {
            Worker::Real(runtime) => {
                attempt::run_worker(&allocated, runtime, progress, records, &cancelled)
            }
            #[cfg(any(test, feature = "ui-harness"))]
            Worker::Scripted { queue, first } => match first.take().or_else(|| queue.next()) {
                Some(script) => scripted(&script, &allocated, progress, records, &cancelled),
                None => WorkerRun::without_workspace(
                    RunResult::Failed(JobFailure::Host(attempt::host_failure(
                        HostFailureCode::WorkerExited,
                        "the scripted AI worker has no runs",
                    ))),
                    RunTimings::default(),
                ),
            },
        };
        // The worker has exited: the next waiter may load its model while the
        // writer publishes this run.
        handle.release();
        // The one-slot channel is empty: the writer consumed the previous
        // run before allocating this attempt.
        if finished.send(run).is_err() {
            return;
        }
    }
}

/// Whether `request` is still the current request of its pause, read without
/// the writer. A read failure keeps it: the writer rechecks on its records.
fn request_current(package: &std::path::Path, request: &RequestId) -> bool {
    let Ok(store) = ProjectStore::open(package, AccessMode::ReadOnly) else {
        return true;
    };
    !matches!(
        store.generation_request(request),
        Ok(Some(stored)) if stored.relevance != deadpan_jobs::Relevance::Current
    )
}

/// The deterministic replacement for the model worker. It reports progress
/// and ends in a cancellation, a failure, or synthetic footage that the
/// production qualification and publication path makes Ready.
#[cfg(any(test, feature = "ui-harness"))]
fn scripted(
    script: &crate::project::generation::Script,
    allocated: &Allocated,
    mut progress: impl FnMut(AttemptProgress),
    mut records: impl FnMut(AttemptRecord) -> std::result::Result<(), String>,
    cancelled: &AtomicBool,
) -> WorkerRun {
    use crate::project::generation::ScriptEnding;
    use deadpan_jobs::WorkerStage;
    let cancel = |records: &mut dyn FnMut(AttemptRecord) -> std::result::Result<(), String>| {
        let _ = records(AttemptRecord::CancelRequested);
        WorkerRun::without_workspace(RunResult::Cancelled, RunTimings::default())
    };
    progress(AttemptProgress::Preparing);
    if cancelled.load(Ordering::Acquire) {
        return cancel(&mut records);
    }
    progress(AttemptProgress::Stage(WorkerStage::ModelLoading));
    for completed in 1..=script.steps {
        if cancelled.load(Ordering::Acquire) {
            return cancel(&mut records);
        }
        std::thread::sleep(script.step_interval);
        progress(AttemptProgress::Step {
            stage: WorkerStage::Inference,
            completed,
            total: script.steps,
        });
    }
    match &script.ending {
        ScriptEnding::WaitForCancel => {
            while !cancelled.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
            cancel(&mut records)
        }
        ScriptEnding::Fail(reason) => WorkerRun::without_workspace(
            RunResult::Failed(JobFailure::Host(attempt::host_failure(
                HostFailureCode::WorkerExited,
                reason,
            ))),
            RunTimings::default(),
        ),
        ScriptEnding::Ready => match crate::project::generation::synthetic_tools() {
            Ok(worker) => attempt::synthetic::run(allocated, &worker, progress, records, cancelled),
            Err(reason) => WorkerRun::without_workspace(
                RunResult::Failed(JobFailure::Host(attempt::host_failure(
                    HostFailureCode::WorkerExited,
                    &reason,
                ))),
                RunTimings::default(),
            ),
        },
    }
}
