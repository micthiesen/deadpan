//! The service's AI pause job: one bounded job thread per project runs
//! conditioning (read-only) and the supervised worker; the writer applies
//! allocation, every durable transition and the final outcome. Session
//! replacement and shutdown cancel the job and wait until it has drained.

use std::collections::BTreeSet;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Instant;

use deadpan_cli::generation::acceptance;
use deadpan_cli::generation::attempt::{
    self, AllocateInput, Allocated, AttemptProgress, AttemptRecord, GenerationError, RunResult,
    RunTimings, WorkerRun,
};
use deadpan_cli::generation::conditioning::{self, BridgeInputs};
use deadpan_cli::generation::runtime::BridgeRuntime;
use deadpan_core::HoldVideo;
use deadpan_jobs::{HostFailureCode, JobFailure, JobState, RequestId};

use super::*;
use crate::project::generation::{
    Backend, Candidate, CandidatePreview, GenerationOperation, Job, Outcome, Phase, Update,
};

/// Events the job thread may queue ahead of the writer.
const EVENT_CAPACITY: usize = 64;

/// Job-to-writer events. The service loop drains them every iteration, so a
/// record waits only for the writer's current operation, never for a timer
/// that a long writer task (such as a preview's six-object verification) could
/// exhaust. A record the writer refuses, or a writer that has stopped
/// (disconnected channel), cancels and fails the attempt.
enum Event {
    Prepared(std::result::Result<Box<BridgeInputs>, String>),
    Progress(AttemptProgress),
    Record(AttemptRecord, SyncSender<std::result::Result<(), String>>),
}

struct Running {
    cancelled: Arc<AtomicBool>,
    events: Receiver<Event>,
    /// A dedicated one-slot channel: the final run is never dropped behind
    /// queued progress, and its qualified workspace reaches `finish`.
    finished: Receiver<WorkerRun>,
    /// The job thread waits on this once conditioning is ready.
    allocation: Option<SyncSender<std::result::Result<Allocated, String>>>,
    allocated: Option<Allocated>,
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
    candidates_key: Option<(u64, RevisionId, u64)>,
    epoch: u64,
    dismissed: BTreeSet<RequestId>,
    preview: Option<Arc<CandidatePreview>>,
    reply: Option<(u64, Option<String>)>,
}

impl State {
    pub(super) fn new(backend: Backend) -> Self {
        Self {
            backend,
            ..Self::default()
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
        self.candidates_key = None;
        self.dismissed.clear();
        self.preview = None;
        self.reply = None;
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
            | GenerationOperation::Preview { ticket, .. }
            | GenerationOperation::Discard { ticket, .. } => *ticket,
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
            } => self.start_generation(ticket, session, &revision, hold),
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
            GenerationOperation::Preview {
                session,
                revision,
                request,
                ..
            } => self.preview_generation(session, &revision, request),
            GenerationOperation::Discard {
                session, request, ..
            } => {
                if self.generation.session == session {
                    if self
                        .generation
                        .preview
                        .as_ref()
                        .is_some_and(|preview| preview.request() == &request)
                    {
                        self.generation.preview = None;
                    }
                    self.generation.dismissed.insert(request);
                    self.generation.candidates_key = None;
                    self.message = Some(
                        "Discarded the AI pictures. The pause is unchanged; generate again for new pictures."
                            .into(),
                    );
                    Ok(())
                } else {
                    Err("Project session changed before the request".into())
                }
            }
            GenerationOperation::Accept { .. } => {
                unreachable!("acceptance is an ordinary edit")
            }
        };
        self.generation.reply = Some((ticket, result.err()));
    }

    fn start_generation(
        &mut self,
        ticket: u64,
        session: u64,
        revision: &RevisionId,
        hold: NodeId,
    ) -> Result<()> {
        self.check_context(session, revision)?;
        if self.pending_session_change.is_some() {
            return Err("The project is closing or changing.".into());
        }
        if self.generation.running.is_some() {
            return Err(
                "An AI pause is already generating. Cancel it with :cancel-ai first.".into(),
            );
        }
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        if !matches!(
            workspace.document.nodes().get(&hold).map(|node| &node.kind),
            Some(NodeKind::Hold { .. })
        ) {
            return Err("AI pictures fill a pause. Select a pause (Hold) beat first.".into());
        }
        let package = workspace.path.clone();
        let job = Job {
            ticket,
            session,
            hold: hold.clone(),
            revision: revision.clone(),
            started: Instant::now(),
            request: None,
            phase: Phase::Conditioning,
            outcome: None,
        };
        let worker = match &self.generation.backend {
            Backend::Environment => match BridgeRuntime::from_environment() {
                Ok(runtime) => Worker::Real(runtime),
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
                Worker::Scripted(script)
            }
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let (events, receive_events) = mpsc::sync_channel(EVENT_CAPACITY);
        let (finished, receive_finished) = mpsc::sync_channel(1);
        let (allocation, receive_allocation) = mpsc::sync_channel(1);
        let thread_cancelled = cancelled.clone();
        let revision = revision.clone();
        let thread = std::thread::Builder::new()
            .name("deadpan-ai-pause".into())
            .spawn(move || {
                job_thread(
                    worker,
                    package,
                    revision,
                    hold,
                    thread_cancelled,
                    Channels {
                        events,
                        finished,
                        allocation: receive_allocation,
                    },
                );
            })
            .map_err(|error| format!("Could not start the AI pause job: {error}"))?;
        self.generation.running = Some(Running {
            cancelled,
            events: receive_events,
            finished: receive_finished,
            allocation: Some(allocation),
            allocated: None,
            thread: Some(thread),
        });
        self.generation.job = Some(job);
        self.message = Some("Generating AI pictures for this pause…".into());
        Ok(())
    }

    /// Nothing was recorded; the reason names every missing part.
    fn conclude_unavailable(&mut self, job: Job, reason: String) {
        self.generation.job = Some(job);
        self.conclude_generation(Outcome::Unavailable(reason));
    }

    fn preview_generation(
        &mut self,
        session: u64,
        revision: &RevisionId,
        request: RequestId,
    ) -> Result<()> {
        self.check_context(session, revision)?;
        let candidate = self
            .current_candidates()
            .values()
            .find(|candidate| candidate.request == request)
            .cloned()
            .ok_or("These AI pictures are no longer offered for this pause.")?;
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
        let edit = store
            .preview_generation_acceptance(&acceptance, attempt::object_limits())
            .map_err(display)?;
        let after = edit.forward.apply(&workspace.document).map_err(display)?;
        self.generation.preview = Some(Arc::new(CandidatePreview::new(
            session,
            &workspace.document,
            request,
            candidate.hold,
            range,
            Arc::new(after),
        )?));
        Ok(())
    }

    /// Commit the candidate through the store's explicit acceptance path.
    pub(super) fn accept_generation(&mut self, operation: GenerationOperation) -> Result<()> {
        let GenerationOperation::Accept {
            session,
            revision,
            request,
            hold,
            cursor,
            scope,
        } = operation
        else {
            unreachable!("only acceptance is an ordinary edit");
        };
        self.check_context(session, &revision)?;
        if !self
            .current_candidates()
            .get(&hold)
            .is_some_and(|candidate| candidate.request == request)
        {
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
        match candidates(store, &workspace.document, &self.generation.dismissed) {
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
            preview,
            reply: self.generation.reply.clone(),
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
            Event::Progress(progress) => {
                if let Some(job) = &mut self.generation.job
                    && job.phase != Phase::Cancelling
                {
                    job.phase = match progress {
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
                    };
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
        let Some(allocation) = running.allocation.take() else {
            return;
        };
        let cancelled = running.cancelled.load(Ordering::Acquire);
        let Some(job) = &self.generation.job else {
            return;
        };
        let inputs = match prepared {
            // Nothing was recorded; dropping or refusing the allocation ends
            // the job thread.
            Ok(_) if cancelled => {
                let _ = allocation.try_send(Err("cancelled".into()));
                self.conclude_generation(Outcome::Cancelled);
                return;
            }
            Err(_) if cancelled => {
                self.conclude_generation(Outcome::Cancelled);
                return;
            }
            Err(error) => {
                self.conclude_generation(Outcome::Failed(format!(
                    "The pause's boundary pictures could not be prepared: {error}"
                )));
                return;
            }
            Ok(inputs) => *inputs,
        };
        let hold = job.hold.clone();
        let revision = job.revision.clone();
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
            attempt::allocate(
                store,
                AllocateInput {
                    hold,
                    expected_revision: revision,
                    seed: {
                        let random = uuid::Uuid::new_v4();
                        let mut seed = [0; 4];
                        seed.copy_from_slice(&random.as_bytes()[..4]);
                        u64::from(u32::from_le_bytes(seed))
                    },
                    inputs,
                },
            )
            .map_err(|error: GenerationError| error.to_string())
        })();
        match allocated {
            Ok(allocated) => {
                if let Some(job) = &mut self.generation.job {
                    job.request = Some(allocated.request.request_id.clone());
                    job.phase = Phase::Preparing;
                }
                if let Some(running) = &mut self.generation.running {
                    running.allocated = Some(allocated.clone());
                }
                // A new request supersedes the Hold's earlier candidate.
                self.generation.epoch += 1;
                self.generation.preview = None;
                // If the job thread is already gone, its disconnect records
                // the allocated attempt's failure.
                let _ = allocation.try_send(Ok(allocated));
            }
            Err(error) => {
                let _ = allocation.try_send(Err(error.clone()));
                self.conclude_generation(Outcome::Failed(error));
            }
        }
    }

    fn generation_finished(&mut self, run: WorkerRun) {
        let Some(mut running) = self.generation.running.take() else {
            return;
        };
        let outcome = match (running.allocated.as_ref(), self.store.as_mut()) {
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
        if let Some(thread) = running.thread.take() {
            let _ = thread.join();
        }
        self.conclude_generation(outcome);
    }

    /// The job thread ended without sending Finished.
    fn generation_stopped(&mut self) {
        let Some(mut running) = self.generation.running.take() else {
            return;
        };
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
        self.message = Some(
            match &outcome {
                Outcome::Ready(_) if offered => {
                    "AI pictures are ready. Preview them, then accept or discard."
                }
                Outcome::Ready(_) => {
                    "AI pictures finished, but the pause changed meanwhile; generate again."
                }
                Outcome::Cancelled => "The AI pause was cancelled; the pause is unchanged.",
                Outcome::Failed(_) => "The AI pause failed; the pause is unchanged.",
                Outcome::Unavailable(_) => {
                    "AI pauses are unavailable on this Mac; the inspector lists what is missing."
                }
            }
            .into(),
        );
        if let Some(job) = &mut self.generation.job
            && job.outcome.is_none()
        {
            job.outcome = Some(outcome);
        }
        // A job concluded before allocation has nothing left to record. Its
        // thread has already returned or is returning from a refused
        // allocation, so reap it now; an immediate restart is then admitted.
        if self
            .generation
            .running
            .as_ref()
            .is_some_and(|running| running.allocated.is_none() && running.allocation.is_none())
            && let Some(mut running) = self.generation.running.take()
            && let Some(thread) = running.thread.take()
        {
            let _ = thread.join();
        }
    }
}

/// Current Ready bundles that their Hold has not accepted.
fn candidates(
    store: &ProjectStore,
    document: &ProjectDocument,
    dismissed: &BTreeSet<RequestId>,
) -> std::result::Result<BTreeMap<NodeId, Candidate>, StoreError> {
    let mut found = BTreeMap::new();
    for request in store.current_generation_requests()? {
        if request.bridge_plan.is_none() || dismissed.contains(&request.request_id) {
            continue;
        }
        let Some(selected) = store.selected_generation_bundle(&request.request_id)? else {
            continue;
        };
        let hold = request.binding.hold_id.clone();
        let Some(NodeKind::Hold { recipe }) = document.nodes().get(&hold).map(|node| &node.kind)
        else {
            continue;
        };
        if let HoldVideo::Generated { accepted } = &recipe.video
            && &accepted.artifact.sampled_object == selected.receipt.sampled_object()
        {
            continue;
        }
        found.insert(
            hold.clone(),
            Candidate {
                request: request.request_id.clone(),
                hold,
                origin: request.origin_revision.clone(),
                frames: request.constraints.video.frames().frames(),
            },
        );
    }
    Ok(found)
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

/// What runs after allocation: the real supervised worker, or the test seam.
enum Worker {
    Real(BridgeRuntime),
    #[cfg(any(test, feature = "ui-harness"))]
    Scripted(crate::project::generation::Script),
}

fn job_thread(
    worker: Worker,
    package: PathBuf,
    revision: RevisionId,
    hold: NodeId,
    cancelled: Arc<AtomicBool>,
    channels: Channels,
) {
    let Channels {
        events,
        finished,
        allocation,
    } = channels;
    let prepared = conditioning::prepare(&package, &revision, &hold, &cancelled);
    let ready = prepared.is_ok();
    if send(&events, Event::Prepared(prepared.map(Box::new))).is_err() || !ready {
        return;
    }
    let Ok(Ok(allocated)) = allocation.recv() else {
        return;
    };
    let progress = |progress: AttemptProgress| {
        // Display-only; a full queue drops one step, never the worker.
        let _ = events.try_send(Event::Progress(progress));
    };
    let records = |record: AttemptRecord| -> std::result::Result<(), String> {
        let (acknowledge, acknowledged) = mpsc::sync_channel(1);
        send(&events, Event::Record(record, acknowledge))?;
        acknowledged
            .recv()
            .map_err(|_| "The project writer stopped before recording the AI pause.".to_owned())?
    };
    let run = match &worker {
        Worker::Real(runtime) => {
            attempt::run_worker(&allocated, runtime, progress, records, &cancelled)
        }
        #[cfg(any(test, feature = "ui-harness"))]
        Worker::Scripted(script) => scripted(script, progress, records, &cancelled),
    };
    // The one-slot channel is empty: this is the job's only final send.
    let _ = finished.send(run);
}

/// The deterministic replacement for the model worker. It reports progress
/// and ends in a cancellation or failure; it can never produce a bundle.
#[cfg(any(test, feature = "ui-harness"))]
fn scripted(
    script: &crate::project::generation::Script,
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
    }
}
