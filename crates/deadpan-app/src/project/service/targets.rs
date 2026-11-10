//! The service's target saves and tracking job. Saves are ordinary edits on
//! the writer. One bounded job thread per project prepares the range from a
//! read-only store and runs the supervised tracker; the writer saves the
//! result expecting the revision the command was entered at. Session
//! replacement and shutdown cancel the job and wait until it has drained.
//! Picture proposals use a second, independent job thread ([`picture_proposals`]).

use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Instant;

#[cfg(any(test, feature = "ui-harness"))]
use deadpan_analysis::NormalizedRect;
use deadpan_analysis::{Keyframe, TrackPolicy, TrackedPath, rect_from_target, retrack_target};
use deadpan_cli::tracking::incremental::{RangeOptions, track_ranges, worker_runs};
use deadpan_cli::tracking::{self, TrackRequest, TrackingError, TrackingRuntime};
use deadpan_core::{SourceTimeBase, TargetId};

use super::*;
use crate::project::targets::{
    Backend, Coverage, Job, Operation, Outcome, Phase, ProposalJob, Saved, TrackMode, Update,
};

mod picture_proposals;

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
    /// The tracked picture's exact upright aspect from its qualification.
    display_aspect: Option<deadpan_core::ExactRatio>,
}

enum Event {
    Progress(u8),
    /// Cumulative coverage after a range; a dropped report is superseded by
    /// the next one or the result.
    Range(Box<Coverage>),
    /// The result and the last coverage, which a dropped report cannot lose.
    Finished(
        Box<std::result::Result<Finished, Failure>>,
        Option<Box<Coverage>>,
    ),
}

struct Running {
    cancelled: Arc<AtomicBool>,
    /// The sleep epoch at command entry; it decides interruption and guards the save.
    capture: crate::jobs::RunCapture,
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
    /// One picture-analysis job thread, independent of tracking.
    proposals_running: Option<picture_proposals::Running>,
    proposals: Option<ProposalJob>,
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
        self.running.is_some() || self.proposals_running.is_some()
    }

    fn reset(&mut self, session: u64) {
        debug_assert!(self.running.is_none() && self.proposals_running.is_none());
        self.session = session;
        self.job = None;
        self.reply = None;
        self.saved = None;
        self.proposals = None;
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
        self.cancel_proposals();
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
            Operation::Delete {
                ticket,
                session,
                revision,
                id,
            } => (ticket, self.delete_target(session, revision, id)),
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
                let proposal_matches = self.targets.session == session
                    && self
                        .targets
                        .proposals
                        .as_ref()
                        .is_some_and(|proposals| proposals.ticket == job);
                let result = if matches {
                    self.cancel_tracking();
                    self.message = Some("Cancelling tracking…".into());
                    Ok(())
                } else if proposal_matches {
                    // Completion can race with closing the picker. Cancelling
                    // that exact completed request is a harmless acknowledgement.
                    if self.targets.proposals_running.is_some() {
                        self.cancel_proposals();
                        self.message = Some("Cancelling target search…".into());
                    }
                    Ok(())
                } else {
                    Err("No matching target job is running.".into())
                };
                (ticket, result)
            }
            Operation::DetectPictureProposals {
                ticket,
                session,
                revision,
                asset,
                pts,
                selection,
            } => (
                ticket,
                self.start_proposals(
                    ticket,
                    session,
                    revision,
                    deadpan_cli::picture_proposals::PictureProposalRequest {
                        asset: Some(asset),
                        at_pts: pts,
                        selection,
                    },
                ),
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

    pub(super) fn refuse_target(&mut self, ticket: u64, reason: String) {
        self.sync_target_session();
        self.targets.reply = Some((ticket, Some(reason)));
    }

    fn sync_target_session(&mut self) {
        if let Some(workspace) = &self.workspace
            && self.targets.session != workspace.session
            && self.targets.running.is_none()
            && self.targets.proposals_running.is_none()
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

    fn delete_target(&mut self, session: u64, expected: RevisionId, id: TargetId) -> Result<()> {
        self.sync_target_session();
        self.check_context(session, &expected)?;
        if self.pending_session_change.is_some() {
            return Err("The project is closing or changing.".into());
        }
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let label = workspace
            .document
            .targets()
            .get(&id)
            .map(|target| target.label.clone())
            .ok_or_else(|| format!("There is no saved target {}.", id.as_str()))?;
        let users = crate::project::targets::users(&workspace.document, &id);
        if !users.is_empty() {
            return Err(format!(
                "{label} is used by {}; change their framing first (:zoom off or another target), then delete it.",
                users.join(", ")
            ));
        }
        let request = CommandRequest {
            project_id: workspace.document.project_id().clone(),
            expected_revision: expected,
            new_revision: revision(),
            command: Command::DeleteTarget { id: id.clone() },
        };
        let request_id = id;
        let base = request.expected_revision.clone();
        let outcome = self
            .writer()?
            .commit(&request)
            .map_err(|error| match error {
                StoreError::RevisionConflict { .. } => {
                    "The project changed meanwhile; the target was not deleted. Try again."
                        .to_owned()
                }
                error => error.to_string(),
            })?;
        self.committed = None;
        self.targets.saved = Some(Saved {
            session,
            base,
            revision: outcome.revision_id,
            id: request_id,
        });
        self.refresh_saved(&format!("Deleted {label}. Undo restores it."))?;
        self.message = Some(format!("Deleted {label}. Undo restores it."));
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
        let initial_region = target.region.as_region().copied().ok_or(
            "Point targets have no tracking rectangle. Draw a region target to track; point positions can be corrected manually.",
        )?;
        // Manual corrections of an untracked rectangle seed their own
        // segments; tracking keeps them as keyframes.
        let mut later = Vec::new();
        let mut initial = rect_from_target(&initial_region)
            .ok_or("The target's rectangle lies outside the picture.")?;
        if matches!(mode, TrackMode::Track { .. }) {
            for correction in &target.corrections {
                let region = correction
                    .region
                    .as_region()
                    .and_then(rect_from_target)
                    .ok_or("A correction's rectangle lies outside the picture.")?;
                if correction.at == target.span.start().ticks {
                    // A correction at the first picture wins over the
                    // initial selection there.
                    initial = region;
                } else {
                    later.push(Keyframe {
                        pts: correction.at,
                        region,
                    });
                }
            }
        }
        let (from_pts, to_pts, region, stop_at_shots, correct_at) = match mode {
            TrackMode::Track { through_shots } => {
                if !target.samples.is_empty() {
                    return Err(format!(
                        "{} is already tracked. In Camera, correct it with c at a wrong picture; only that range is tracked again.",
                        target.label
                    ));
                }
                (
                    target.span.start().ticks,
                    target.span.end().ticks,
                    initial,
                    !through_shots,
                    None,
                )
            }
            TrackMode::Correct { at, region } => {
                let (_, end) = target
                    .correction_range(at)
                    .ok_or_else(|| format!("{} does not cover this picture.", target.label))?;
                let region = rect_from_target(&region)
                    .ok_or("The target's rectangle lies outside the picture.")?;
                (at, end.ticks, region, false, Some(at))
            }
        };
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
            coverage: None,
            outcome: None,
        };
        let package = workspace.path.clone();
        #[cfg_attr(
            not(any(test, feature = "ui-harness")),
            allow(unused_mut, reason = "test backends choose private caches")
        )]
        let mut options = RangeOptions {
            cache_directory: deadpan_cli::storage::UserStorage::current()
                .map(|storage| storage.caches.join("AnalysisRanges")),
            ..RangeOptions::default()
        };
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
                // Tests and replays never touch the user's caches.
                options.cache_directory = queue.cache().map(std::path::Path::to_path_buf);
                if let Some(width) = script.range_pictures {
                    options.range_pictures = width;
                }
                if let Some(error) = &script.unavailable {
                    self.targets.job = Some(job);
                    self.conclude_tracking(Outcome::Unavailable(error.clone()));
                    return Ok(());
                }
                Worker::Scripted(script)
            }
            #[cfg(test)]
            Backend::Installed(installed) => {
                options.cache_directory.clone_from(&installed.cache);
                if let Some(width) = installed.range_pictures {
                    options.range_pictures = width;
                }
                Worker::Real(TrackingRuntime {
                    executable: installed.executable.clone(),
                    environment: Default::default(),
                })
            }
            // Only picture analysis is scripted; tracking uses the installed worker.
            #[cfg(any(test, feature = "ui-harness"))]
            Backend::ScriptedProposals(_) => match TrackingRuntime::beside_current_executable() {
                Ok(runtime) if runtime.executable.is_file() => {
                    options.cache_directory = None;
                    Worker::Real(runtime)
                }
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
        let capture =
            crate::jobs::RunCapture::new(self.shared.job_board.lifecycle(), cancelled.clone());
        let thread_capture = capture.clone();
        let (events, receive) = mpsc::sync_channel(EVENT_CAPACITY);
        let thread_cancelled = cancelled.clone();
        let cursor = self.shared.tracking_cursor.clone();
        let identity = (job.target.clone(), job.label.clone());
        let handle = self.shared.job_board.register(
            crate::jobs::JobSpec::new(crate::jobs::JobKind::Tracking, Some(session))
                .detail(job.label.clone())
                .cancel_flag(&cancelled),
        );
        let thread = std::thread::Builder::new()
            .name("deadpan-track".into())
            .spawn(move || {
                // The watcher cancels the worker at sleep without UI frames.
                thread_capture.run(|| {
                    job_thread(
                        worker,
                        package,
                        Plan {
                            request,
                            later,
                            options,
                            identity,
                        },
                        revision,
                        correct_at,
                        &cursor,
                        thread_cancelled,
                        events,
                        &handle,
                    );
                });
            })
            .map_err(|error| format!("Could not start tracking: {error}"))?;
        self.targets.running = Some(Running {
            cancelled,
            capture,
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
            proposals: self
                .targets
                .proposals
                .clone()
                .filter(|faces| faces.session == session),
        })
    }

    /// Apply queued job events on the writer. True when published state changed.
    pub(super) fn pump_tracking(&mut self) -> bool {
        let mut changed = self.pump_proposals();
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
                Ok(Event::Range(coverage)) => {
                    changed = true;
                    if let Some(job) = &mut self.targets.job
                        && job.running()
                    {
                        job.coverage = Some(*coverage);
                    }
                }
                Ok(Event::Finished(result, coverage)) => {
                    if let (Some(job), Some(coverage)) = (&mut self.targets.job, coverage) {
                        job.coverage = Some(*coverage);
                    }
                    self.tracking_finished(*result);
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
        // A run that crossed sleep never publishes, even a finished path.
        // The watcher's own cancellation makes the worker report Cancelled.
        let outcome = match (running.capture.stop(), result) {
            (Some(crate::jobs::Stop::Interrupted), _) => Outcome::Interrupted,
            (Some(crate::jobs::Stop::Cancelled), _) | (None, Err(Failure::Cancelled)) => {
                Outcome::Cancelled
            }
            (None, Err(Failure::Unavailable(reason))) => Outcome::Unavailable(reason),
            (None, Err(Failure::Failed(reason))) => Outcome::Failed(reason),
            (None, Ok(finished)) => match self.save_tracked(finished, &running.capture) {
                Ok(outcome) => outcome,
                Err(error) => Outcome::Failed(error),
            },
        };
        self.conclude_tracking(outcome);
    }

    /// Save a finished path expecting the revision tracking started from.
    fn save_tracked(
        &mut self,
        finished: Finished,
        capture: &crate::jobs::RunCapture,
    ) -> Result<Outcome> {
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
                finished.display_aspect,
                &finished.engine,
                budget,
            )
        }
        .map_err(|error| error.to_string())?;
        let samples = target.samples.len();
        // The final guard orders the commit against sleep: a notification
        // after this observation does not retract the saved target.
        if capture.stop() == Some(crate::jobs::Stop::Interrupted) {
            return Ok(Outcome::Interrupted);
        }
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
            Outcome::Cancelled => format!(
                "Tracking {label} was cancelled; the target is unchanged. :track resumes from its completed ranges."
            ),
            Outcome::Failed(_) => format!("Tracking {label} failed; the target is unchanged."),
            Outcome::Interrupted => format!(
                "Tracking {label} stopped when the Mac slept; the target is unchanged. :track resumes from its completed ranges."
            ),
            Outcome::Unavailable(_) => {
                format!("{label} cannot be tracked here; the inspector says why.")
            }
        });
        if let Some(job) = &mut self.targets.job
            && job.outcome.is_none()
        {
            // An unsaved partial path must not outlive its job.
            if let Some(coverage) = &mut job.coverage {
                coverage.partial = None;
            }
            job.outcome = Some(outcome);
        }
    }
}

/// What the job thread tracks.
struct Plan {
    request: TrackRequest,
    /// Manual keyframes after the first, in order.
    later: Vec<Keyframe>,
    options: RangeOptions,
    /// The target's ID and label, for its partial preview.
    identity: (TargetId, String),
}

#[allow(clippy::too_many_arguments)]
fn job_thread(
    worker: Worker,
    package: PathBuf,
    plan: Plan,
    revision: RevisionId,
    correct_at: Option<i64>,
    cursor: &std::sync::atomic::AtomicI64,
    cancelled: Arc<AtomicBool>,
    events: SyncSender<Event>,
    handle: &crate::jobs::JobHandle,
) {
    handle.set_progress("Preparing pictures", None);
    let Plan {
        request,
        later,
        options,
        identity: (id, label),
    } = plan;
    let mut last_coverage = None;
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
        if later
            .iter()
            .any(|keyframe| keyframe.pts >= prepared.end_pts())
        {
            return Err(Failure::Failed(
                "A correction lies beyond where tracking stops (a cut or the picture limit), so saving would drop it. Track through cuts or remove that correction."
                    .into(),
            ));
        }
        let mut keyframes = vec![Keyframe {
            pts: prepared.start_pts(),
            region: request.region,
        }];
        keyframes.extend(later);
        let budget = tracking::sample_budget(&prepared.head, &id);
        let mut version = 0_u64;
        let last = &mut last_coverage;
        let percent = |percent: u8| {
            handle.set_progress("Tracking", Some(f32::from(percent.min(100)) / 100.0));
            // Display-only; a full queue drops one report, never the worker.
            let _ = events.try_send(Event::Progress(percent));
        };
        let progress = |range: deadpan_cli::tracking::incremental::RangeProgress<'_>| {
            version += 1;
            // Only a first tracking previews a target; a correction's range
            // replaces part of a saved one.
            let partial = range
                .partial
                .filter(|_| correct_at.is_none())
                .and_then(|path| {
                    path.to_target(
                        label.clone(),
                        prepared.asset.clone(),
                        prepared.time_base,
                        prepared.exact_display_aspect,
                        range.engine.unwrap_or_default(),
                        budget,
                    )
                    .ok()
                })
                .map(|(target, _)| Arc::new(target));
            let coverage = Coverage {
                completed: range.completed,
                total: range.total,
                cached: range.cached,
                spans: range.coverage,
                partial,
                version,
            };
            *last = Some(coverage.clone());
            let _ = events.try_send(Event::Range(Box::new(coverage)));
        };
        // Tracking yields to playback and renders between ranges.
        let boundary = || {
            handle
                .checkpoint(Some(&cancelled))
                .map_err(|_| TrackingError::Cancelled)
        };
        let finished = match &worker {
            Worker::Real(runtime) => {
                let attempt = uuid::Uuid::new_v4().simple().to_string();
                let mut runs = worker_runs(runtime, &prepared, &attempt, &cancelled, deadline);
                track_ranges(
                    &prepared,
                    &keyframes,
                    TrackPolicy::default(),
                    &options,
                    cursor,
                    &mut runs,
                    &cancelled,
                    deadline,
                    boundary,
                    percent,
                    progress,
                )
            }
            #[cfg(any(test, feature = "ui-harness"))]
            Worker::Scripted(script) => {
                if let Some(error) = script_failure(script) {
                    return Err(error);
                }
                let mut runs = scripted(script, &cancelled);
                track_ranges(
                    &prepared,
                    &keyframes,
                    TrackPolicy::default(),
                    &options,
                    cursor,
                    &mut runs,
                    &cancelled,
                    deadline,
                    boundary,
                    percent,
                    progress,
                )
            }
        }
        .map_err(failure)?;
        Ok(Finished {
            path: finished.path,
            engine: finished.engine,
            asset: prepared.asset.clone(),
            time_base: prepared.time_base,
            display_aspect: prepared.exact_display_aspect,
        })
    })();
    // The writer drains every loop iteration; a stopped writer disconnects.
    let _ = events.send(Event::Finished(
        Box::new(result),
        last_coverage.map(Box::new),
    ));
}

/// A scripted ending that fails before any range.
#[cfg(any(test, feature = "ui-harness"))]
fn script_failure(script: &crate::project::targets::Script) -> Option<Failure> {
    use crate::project::targets::ScriptEnding;
    match (&script.ending, script.ranges_before_ending) {
        (ScriptEnding::Fail(reason), None) => Some(Failure::Failed(reason.clone())),
        _ => None,
    }
}

/// The deterministic replacement for the Vision worker, one range at a time:
/// real range plan, cache and policy; synthetic confident observations moving
/// from each run's seed. After `ranges_before_ending` tracked ranges (or at
/// once) a non-moving ending applies to the next range.
#[cfg(any(test, feature = "ui-harness"))]
fn scripted<'a>(
    script: &'a crate::project::targets::Script,
    cancelled: &'a AtomicBool,
) -> impl FnMut(
    &[i64],
    i64,
    Keyframe,
    &mut dyn FnMut(u8),
) -> deadpan_cli::tracking::incremental::RangeRun
+ 'a {
    use crate::project::targets::{SCRIPTED_ENGINE, ScriptEnding};
    let mut tracked = 0_usize;
    move |pictures, _end, seed, progress| {
        let ending_now = match script.ranges_before_ending {
            Some(limit) => tracked >= limit,
            None => !matches!(script.ending, ScriptEnding::Moving { .. }),
        };
        let steps = script.steps.max(1);
        for step in 1..=steps {
            if cancelled.load(Ordering::Acquire) {
                return Err(TrackingError::Cancelled);
            }
            std::thread::sleep(script.step_interval);
            progress(u8::try_from(u32::from(step) * 99 / u32::from(steps)).unwrap_or(99));
        }
        let step = match &script.ending {
            ScriptEnding::Moving { step } => *step,
            _ if !ending_now => 0.002,
            ScriptEnding::WaitForCancel => {
                while !cancelled.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                return Err(TrackingError::Cancelled);
            }
            ScriptEnding::Fail(reason) => return Err(TrackingError::Worker(reason.clone())),
        };
        tracked += 1;
        script
            .tracked_ranges
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // Stride 1: every picture from the seed is analysed.
        let observations = pictures
            .iter()
            .enumerate()
            .map(|(index, &pts)| deadpan_analysis::RawObservation {
                pts,
                region: NormalizedRect::clipped(
                    seed.region.x() + step * index as f64,
                    seed.region.y(),
                    seed.region.width(),
                    seed.region.height(),
                ),
                confidence: 0.9,
            })
            .collect();
        progress(100);
        Ok((observations, SCRIPTED_ENGINE.into()))
    }
}
