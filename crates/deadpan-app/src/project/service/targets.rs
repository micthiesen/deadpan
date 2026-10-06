//! The service's target saves and tracking job. Saves are ordinary edits on
//! the writer. One bounded job thread per project prepares the range from a
//! read-only store and runs the supervised tracker; the writer saves the
//! result expecting the revision the command was entered at. Session
//! replacement and shutdown cancel the job and wait until it has drained.
//! Face proposals use a second, independent job thread ([`faces`]).

use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Instant;

use deadpan_analysis::{TrackPolicy, TrackedPath, rect_from_target, retrack_target};
use deadpan_cli::tracking::{self, TrackRequest, TrackingError, TrackingRuntime};
use deadpan_core::{SourceTimeBase, TargetId};

use super::*;
use crate::project::targets::{
    Backend, FaceJob, Job, Operation, Outcome, Phase, Saved, TrackMode, Update,
};

mod faces;

/// Progress reports the job thread may queue ahead of the writer.
const EVENT_CAPACITY: usize = 16;
/// Longest one attempt may run, as for the CLI.
const DEADLINE: Duration = Duration::from_secs(6 * 60 * 60);

enum Failure {
    Unavailable(String),
    Cancelled,
    Failed(String),
}

struct Finished {
    path: TrackedPath,
    engine: String,
    asset: AssetId,
    time_base: SourceTimeBase,
}

enum Event {
    Progress(u8),
    Finished(std::result::Result<Finished, Failure>),
}

struct Running {
    cancelled: Arc<AtomicBool>,
    events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        // Never leave a cooperative job running without a cancellation request.
        self.cancelled.store(true, Ordering::Release);
    }
}

#[derive(Default)]
pub(super) struct State {
    backend: Backend,
    session: u64,
    running: Option<Running>,
    job: Option<Job>,
    reply: Option<(u64, Option<String>)>,
    saved: Option<Saved>,
    /// The face-detection job thread, independent of tracking.
    faces_running: Option<faces::Running>,
    faces: Option<FaceJob>,
}

impl State {
    pub(super) fn new(backend: Backend) -> Self {
        Self {
            backend,
            ..Self::default()
        }
    }

    /// A job thread is live; the project must not be released.
    pub(super) fn active(&self) -> bool {
        self.running.is_some() || self.faces_running.is_some()
    }

    fn reset(&mut self, session: u64) {
        debug_assert!(self.running.is_none() && self.faces_running.is_none());
        self.session = session;
        self.job = None;
        self.reply = None;
        self.saved = None;
        self.faces = None;
    }
}

/// What tracks: the installed worker, or the test seam.
enum Worker {
    Real(TrackingRuntime),
    #[cfg(any(test, feature = "ui-harness"))]
    Scripted(crate::project::targets::Script),
}

fn failure(error: TrackingError) -> Failure {
    match error {
        TrackingError::Cancelled => Failure::Cancelled,
        TrackingError::Unavailable(reason) if reason.contains("no stored shot analysis") => {
            Failure::Unavailable(
                "This Original has no stored shot analysis yet, so tracking cannot stop at its cuts. Wait for shot detection, or track through cuts with :track ID through-shots."
                    .into(),
            )
        }
        TrackingError::Unavailable(reason) => Failure::Unavailable(reason),
        error => Failure::Failed(error.to_string()),
    }
}

impl Service {
    /// Cancel any live tracking or face-detection job cooperatively; the
    /// pump drains it.
    pub(super) fn cancel_tracking(&mut self) {
        self.cancel_faces();
        if let Some(running) = &self.targets.running {
            running.cancelled.store(true, Ordering::Release);
            if let Some(job) = &mut self.targets.job
                && job.running()
            {
                job.phase = Phase::Cancelling;
            }
        }
    }

    pub(super) fn target_command(&mut self, operation: Operation) {
        let (ticket, result) = match operation {
            Operation::Save {
                ticket,
                session,
                revision,
                id,
                target,
            } => (ticket, self.save_target(session, revision, id, *target)),
            Operation::Track {
                ticket,
                session,
                revision,
                id,
                mode,
            } => (
                ticket,
                self.start_tracking(ticket, session, revision, id, mode),
            ),
            Operation::Cancel {
                ticket,
                session,
                job,
            } => {
                let matches = self.targets.session == session
                    && self.targets.running.is_some()
                    && self
                        .targets
                        .job
                        .as_ref()
                        .is_some_and(|running| running.ticket == job && running.running());
                let result = if matches {
                    self.cancel_tracking();
                    self.message = Some("Cancelling tracking…".into());
                    Ok(())
                } else {
                    Err("No matching tracking job is running.".into())
                };
                (ticket, result)
            }
            Operation::DetectFaces {
                ticket,
                session,
                revision,
                asset,
                pts,
            } => (
                ticket,
                self.start_faces(ticket, session, revision, asset, pts),
            ),
            Operation::SaveFramed {
                ticket,
                session,
                revision,
                scope,
                cursor,
                node,
                id,
                target,
                framing,
            } => (
                ticket,
                self.save_framed(
                    session,
                    revision,
                    scope,
                    cursor,
                    node,
                    id,
                    *target,
                    framing.map(|framing| *framing),
                ),
            ),
        };
        self.targets.reply = Some((ticket, result.err()));
    }

    fn sync_target_session(&mut self) {
        if let Some(workspace) = &self.workspace
            && self.targets.session != workspace.session
            && self.targets.running.is_none()
            && self.targets.faces_running.is_none()
        {
            self.targets.reset(workspace.session);
        }
    }

    /// Commit `target` as `id` with one reversible edit expecting `revision`.
    fn commit_target(
        &mut self,
        expected: &RevisionId,
        id: &TargetId,
        target: deadpan_core::AttentionTarget,
    ) -> Result<RevisionId> {
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let session = workspace.session;
        let request = CommandRequest {
            project_id: workspace.document.project_id().clone(),
            expected_revision: expected.clone(),
            new_revision: revision(),
            command: Command::SetTarget {
                id: id.clone(),
                target,
            },
        };
        let outcome = self
            .writer()?
            .commit(&request)
            .map_err(|error| match error {
                StoreError::RevisionConflict { .. } => {
                    "The project changed meanwhile; the target was not saved. Try again.".to_owned()
                }
                error => error.to_string(),
            })?;
        // Selection and cursor stay where the user is; this edit has no
        // picture time and no node.
        self.committed = None;
        self.targets.saved = Some(Saved {
            session,
            base: expected.clone(),
            revision: outcome.revision_id.clone(),
            id: id.clone(),
        });
        self.refresh_saved("Target saved")?;
        Ok(outcome.revision_id)
    }

    fn save_target(
        &mut self,
        session: u64,
        revision: RevisionId,
        id: TargetId,
        target: deadpan_core::AttentionTarget,
    ) -> Result<()> {
        self.sync_target_session();
        self.check_context(session, &revision)?;
        if self.pending_session_change.is_some() {
            return Err("The project is closing or changing.".into());
        }
        let label = target.label.clone();
        let replaced = self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.targets().contains_key(&id));
        self.commit_target(&revision, &id, target)?;
        self.message = Some(if replaced {
            format!("Corrected {label} and saved. Undo restores it.")
        } else {
            format!("Saved {label}. Undo removes it.")
        });
        Ok(())
    }

    fn start_tracking(
        &mut self,
        ticket: u64,
        session: u64,
        revision: RevisionId,
        id: TargetId,
        mode: TrackMode,
    ) -> Result<()> {
        self.sync_target_session();
        self.check_context(session, &revision)?;
        if self.pending_session_change.is_some() {
            return Err("The project is closing or changing.".into());
        }
        if self.targets.running.is_some() {
            return Err("A target is already tracking. Cancel it with :track-cancel first.".into());
        }
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let target = workspace
            .document
            .targets()
            .get(&id)
            .ok_or_else(|| format!("There is no saved target {}.", id.as_str()))?;
        let (from_pts, to_pts, region, stop_at_shots, correct_at) = match mode {
            TrackMode::Track { through_shots } => {
                if !target.samples.is_empty() {
                    return Err(format!(
                        "{} is already tracked. In Camera, correct it with c at a wrong picture; only that range is tracked again.",
                        target.label
                    ));
                }
                if !target.corrections.is_empty() {
                    // Only headless corrections reach an untracked target.
                    return Err(format!(
                        "{} has manual corrections that tracking from its first picture would replace. Remove them with Undo or track it headlessly with track --replace.",
                        target.label
                    ));
                }
                (
                    target.span.start().ticks,
                    target.span.end().ticks,
                    target.region,
                    !through_shots,
                    None,
                )
            }
            TrackMode::Correct { at, region } => {
                let (_, end) = target
                    .correction_range(at)
                    .ok_or_else(|| format!("{} does not cover this picture.", target.label))?;
                (at, end.ticks, region, false, Some(at))
            }
        };
        let region =
            rect_from_target(&region).ok_or("The target's rectangle lies outside the picture.")?;
        let request = TrackRequest {
            asset: Some(target.asset.clone()),
            from_pts,
            to_pts,
            region,
            stride: 1,
            stop_at_shots,
        };
        let job = Job {
            ticket,
            session,
            target: id,
            label: target.label.clone(),
            correction: correct_at.is_some(),
            revision: revision.clone(),
            started: Instant::now(),
            phase: Phase::Preparing,
            outcome: None,
        };
        let package = workspace.path.clone();
        let worker = match &self.targets.backend {
            Backend::Environment => match TrackingRuntime::beside_current_executable() {
                Ok(runtime) if runtime.executable.is_file() => Worker::Real(runtime),
                Ok(runtime) => {
                    self.targets.job = Some(job);
                    self.conclude_tracking(Outcome::Unavailable(format!(
                        "The tracker is not installed beside Deadpan ({}). Build the deadpan-track workspace member.",
                        runtime.executable.display()
                    )));
                    return Ok(());
                }
                Err(error) => {
                    self.targets.job = Some(job);
                    self.conclude_tracking(Outcome::Unavailable(error.to_string()));
                    return Ok(());
                }
            },
            #[cfg(any(test, feature = "ui-harness"))]
            Backend::Scripted(queue) => {
                let script = queue.next().ok_or("The scripted tracker has no runs.")?;
                if let Some(error) = &script.unavailable {
                    self.targets.job = Some(job);
                    self.conclude_tracking(Outcome::Unavailable(error.clone()));
                    return Ok(());
                }
                Worker::Scripted(script)
            }
            // Only face detection is scripted; tracking uses the installed worker.
            #[cfg(any(test, feature = "ui-harness"))]
            Backend::ScriptedFaces(_) => match TrackingRuntime::beside_current_executable() {
                Ok(runtime) if runtime.executable.is_file() => Worker::Real(runtime),
                _ => {
                    self.targets.job = Some(job);
                    self.conclude_tracking(Outcome::Unavailable(
                        "The tracker is not installed beside Deadpan.".into(),
                    ));
                    return Ok(());
                }
            },
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let (events, receive) = mpsc::sync_channel(EVENT_CAPACITY);
        let thread_cancelled = cancelled.clone();
        let handle = self.shared.job_board.register(
            crate::jobs::JobSpec::new(crate::jobs::JobKind::Tracking, Some(session))
                .detail(job.label.clone())
                .cancel_flag(&cancelled),
        );
        let thread = std::thread::Builder::new()
            .name("deadpan-track".into())
            .spawn(move || {
                job_thread(
                    worker,
                    package,
                    request,
                    revision,
                    correct_at,
                    thread_cancelled,
                    events,
                    &handle,
                );
            })
            .map_err(|error| format!("Could not start tracking: {error}"))?;
        self.targets.running = Some(Running {
            cancelled,
            events: receive,
            thread: Some(thread),
        });
        self.message = Some(format!(
            "{} {} in the background…",
            if job.correction {
                "Re-tracking"
            } else {
                "Tracking"
            },
            job.label
        ));
        self.targets.job = Some(job);
        Ok(())
    }

    pub(super) fn targets_update(&mut self) -> Option<Update> {
        self.sync_target_session();
        let session = self.workspace.as_ref()?.session;
        Some(Update {
            session,
            job: self
                .targets
                .job
                .clone()
                .filter(|job| job.session == session),
            reply: self.targets.reply.clone(),
            saved: self
                .targets
                .saved
                .clone()
                .filter(|saved| saved.session == session),
            faces: self
                .targets
                .faces
                .clone()
                .filter(|faces| faces.session == session),
        })
    }

    /// Apply queued job events on the writer. True when published state changed.
    pub(super) fn pump_tracking(&mut self) -> bool {
        let mut changed = self.pump_faces();
        for _ in 0..EVENT_CAPACITY + 1 {
            let Some(running) = &self.targets.running else {
                return changed;
            };
            match running.events.try_recv() {
                Ok(Event::Progress(percent)) => {
                    changed = true;
                    if let Some(job) = &mut self.targets.job
                        && job.phase != Phase::Cancelling
                    {
                        job.phase = Phase::Tracking(percent.min(100));
                    }
                }
                Ok(Event::Finished(result)) => {
                    self.tracking_finished(result);
                    return true;
                }
                Err(mpsc::TryRecvError::Empty) => return changed,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.tracking_finished(Err(Failure::Failed(
                        "The tracking job stopped unexpectedly.".into(),
                    )));
                    return true;
                }
            }
        }
        changed
    }

    fn tracking_finished(&mut self, result: std::result::Result<Finished, Failure>) {
        let Some(mut running) = self.targets.running.take() else {
            return;
        };
        if let Some(thread) = running.thread.take() {
            let _ = thread.join();
        }
        let cancelled = running.cancelled.load(Ordering::Acquire);
        let outcome = match result {
            _ if cancelled => Outcome::Cancelled,
            Err(Failure::Cancelled) => Outcome::Cancelled,
            Err(Failure::Unavailable(reason)) => Outcome::Unavailable(reason),
            Err(Failure::Failed(reason)) => Outcome::Failed(reason),
            Ok(finished) => match self.save_tracked(finished) {
                Ok(outcome) => outcome,
                Err(error) => Outcome::Failed(error),
            },
        };
        self.conclude_tracking(outcome);
    }

    /// Save a finished path expecting the revision tracking started from.
    fn save_tracked(&mut self, finished: Finished) -> Result<Outcome> {
        let job = self
            .targets
            .job
            .clone()
            .ok_or("The tracking job is gone.")?;
        let workspace = self.workspace.as_ref().ok_or("The project was closed.")?;
        if workspace.session != job.session || workspace.document.revision_id() != &job.revision {
            return Err(
                "The project changed while tracking; nothing was saved. Track again.".into(),
            );
        }
        let document = &workspace.document;
        let existing = document
            .targets()
            .get(&job.target)
            .ok_or("The target was removed while tracking.")?;
        let budget = tracking::sample_budget(document, &job.target);
        let (target, tolerance) = if job.correction {
            retrack_target(existing, &finished.path, &finished.engine, budget)
        } else {
            finished.path.to_target(
                existing.label.clone(),
                finished.asset,
                finished.time_base,
                &finished.engine,
                budget,
            )
        }
        .map_err(|error| error.to_string())?;
        let samples = target.samples.len();
        let revision = self.commit_target(&job.revision, &job.target, target)?;
        Ok(Outcome::Saved {
            revision,
            samples,
            tolerance,
        })
    }

    fn conclude_tracking(&mut self, outcome: Outcome) {
        let (label, correction) = self.targets.job.as_ref().map_or_else(
            || (String::new(), false),
            |job| (job.label.clone(), job.correction),
        );
        self.message = Some(match &outcome {
            Outcome::Saved { samples, .. } if correction => format!(
                "Corrected {label} and re-tracked from that picture: {samples} positions saved. Undo restores the previous target."
            ),
            Outcome::Saved { samples, .. } => {
                format!(
                    "Tracked {label}: {samples} positions saved. Undo restores the previous target."
                )
            }
            Outcome::Cancelled => {
                format!("Tracking {label} was cancelled; the target is unchanged.")
            }
            Outcome::Failed(_) => format!("Tracking {label} failed; the target is unchanged."),
            Outcome::Unavailable(_) => {
                format!("{label} cannot be tracked here; the inspector says why.")
            }
        });
        if let Some(job) = &mut self.targets.job
            && job.outcome.is_none()
        {
            job.outcome = Some(outcome);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn job_thread(
    worker: Worker,
    package: PathBuf,
    request: TrackRequest,
    revision: RevisionId,
    correct_at: Option<i64>,
    cancelled: Arc<AtomicBool>,
    events: SyncSender<Event>,
    handle: &crate::jobs::JobHandle,
) {
    handle.set_progress("Preparing pictures", None);
    let result = (|| -> std::result::Result<Finished, Failure> {
        let deadline = Instant::now() + DEADLINE;
        // Read-only throughout; the writer saves the result.
        let store = ProjectStore::open(&package, AccessMode::ReadOnly)
            .map_err(|error| Failure::Unavailable(error.to_string()))?;
        let prepared =
            tracking::prepare_tracking(&store, &request, &cancelled, deadline).map_err(failure)?;
        drop(store);
        if prepared.head.revision_id() != &revision {
            return Err(Failure::Failed(
                "The project changed before tracking started; nothing was saved. Track again."
                    .into(),
            ));
        }
        if correct_at.is_some_and(|at| prepared.start_pts() != at)
            || (correct_at.is_some() && prepared.end_pts() != request.to_pts)
        {
            return Err(Failure::Failed(
                "A correction must be at an indexed picture, and its range must fit one tracking attempt."
                    .into(),
            ));
        }
        let progress = |percent: u8| {
            handle.set_progress("Tracking", Some(f32::from(percent.min(100)) / 100.0));
            // Display-only; a full queue drops one report, never the worker.
            let _ = events.try_send(Event::Progress(percent));
        };
        let (path, engine) = match &worker {
            Worker::Real(runtime) => {
                let attempt = uuid::Uuid::new_v4().simple().to_string();
                let result = tracking::track(
                    runtime,
                    &prepared,
                    TrackPolicy::default(),
                    &attempt,
                    &cancelled,
                    deadline,
                    progress,
                )
                .map_err(failure)?;
                let engine = tracking::engine_label(&result.runtime);
                (result.path, engine)
            }
            #[cfg(any(test, feature = "ui-harness"))]
            Worker::Scripted(script) => {
                scripted(script, &prepared, request.region, &cancelled, progress)?
            }
        };
        Ok(Finished {
            path,
            engine,
            asset: prepared.asset.clone(),
            time_base: prepared.time_base,
        })
    })();
    // The writer drains every loop iteration; a stopped writer disconnects.
    let _ = events.send(Event::Finished(result));
}

/// The deterministic replacement for the Vision worker: real range, real
/// policy, synthetic confident observations.
#[cfg(any(test, feature = "ui-harness"))]
fn scripted(
    script: &crate::project::targets::Script,
    prepared: &tracking::PreparedTrack,
    region: deadpan_analysis::NormalizedRect,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u8),
) -> std::result::Result<(TrackedPath, String), Failure> {
    use crate::project::targets::{SCRIPTED_ENGINE, ScriptEnding};
    let steps = script.steps.max(1);
    for step in 1..=steps {
        if cancelled.load(Ordering::Acquire) {
            return Err(Failure::Cancelled);
        }
        std::thread::sleep(script.step_interval);
        progress(u8::try_from(u32::from(step) * 99 / u32::from(steps)).unwrap_or(99));
    }
    match &script.ending {
        ScriptEnding::WaitForCancel => {
            while !cancelled.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(Failure::Cancelled)
        }
        ScriptEnding::Fail(reason) => Err(Failure::Failed(reason.clone())),
        ScriptEnding::Moving { step } => {
            let observations: Vec<_> = prepared
                .analysed()
                .enumerate()
                .map(|(index, pts)| deadpan_analysis::RawObservation {
                    pts,
                    region: deadpan_analysis::NormalizedRect::clipped(
                        region.x() + step * index as f64,
                        region.y(),
                        region.width(),
                        region.height(),
                    ),
                    confidence: 0.9,
                })
                .collect();
            let path = TrackedPath::track(
                TrackPolicy::default(),
                prepared.display_aspect,
                prepared.pictures(),
                prepared.end_pts(),
                prepared.stop(),
                deadpan_analysis::Keyframe {
                    pts: prepared.start_pts(),
                    region,
                },
                &observations,
            )
            .map_err(|error| Failure::Failed(error.to_string()))?;
            progress(100);
            Ok((path, SCRIPTED_ENGINE.into()))
        }
    }
}
