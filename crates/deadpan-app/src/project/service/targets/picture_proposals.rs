//! Picture proposals on the project service. One bounded job thread per project
//! resolves one indexed picture from a read-only store, copies the verified
//! Original and runs the supervised picture-proposal worker; the writer
//! publishes complete admitted collections and never edits. Using a proposal
//! is a separate explicit command. Camera uses `Save` for the chosen target;
//! numbered zoom commands use `SaveFramed` to save the target and
//! framing in one undoable Compound.
//! Session replacement and shutdown cancel the job and wait until it drains.

use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Instant;

use deadpan_cli::picture_proposals::{self as proposals, PictureProposalRequest, ProposalError};
#[cfg(any(test, feature = "ui-harness"))]
use deadpan_cli::picture_proposals::{DetectedFace, DetectedSalientRegion, ProposalSelection};
use deadpan_cli::tracking::TrackingRuntime;
use deadpan_core::{Framing, ResolvedStep, ResolvedTransaction, TargetId};

use super::*;
#[cfg(any(test, feature = "ui-harness"))]
use crate::project::targets::ProposalOrigin;
use crate::project::targets::{ProposalJob, ProposalOutcome, ProposedTargets};

/// Longest one detection may run, including the Original copy.
const DEADLINE: Duration = Duration::from_secs(10 * 60);
const FINDING_PROPOSALS: &str = "Finding targets in the displayed picture…";

pub(in crate::project::service) struct Running {
    cancelled: Arc<AtomicBool>,
    finished: Receiver<ProposalOutcome>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        // Never leave a cooperative job running without a cancellation request.
        self.cancelled.store(true, Ordering::Release);
    }
}

/// What detects: one installed worker, or explicitly scripted proposal data.
enum Detector {
    Real(TrackingRuntime),
    #[cfg(any(test, feature = "ui-harness"))]
    Scripted {
        selection: ProposalSelection,
        faces: Option<Vec<DetectedFace>>,
        salient_regions: Option<Vec<DetectedSalientRegion>>,
        delay: Duration,
    },
    #[cfg(any(test, feature = "ui-harness"))]
    Failed {
        reason: String,
        delay: Duration,
    },
}

/// Internal failures cannot contain a successful proposal batch.
enum ProposalFailure {
    Unavailable(String),
    Failed(String),
    Cancelled,
}

impl From<ProposalError> for ProposalFailure {
    fn from(error: ProposalError) -> Self {
        match error {
            ProposalError::Cancelled => Self::Cancelled,
            ProposalError::Unavailable(reason) => Self::Unavailable(reason),
            error => Self::Failed(error.to_string()),
        }
    }
}

fn outcome(error: ProposalFailure) -> ProposalOutcome {
    match error {
        ProposalFailure::Cancelled => ProposalOutcome::Cancelled,
        ProposalFailure::Unavailable(reason) => ProposalOutcome::Unavailable(reason),
        ProposalFailure::Failed(reason) => ProposalOutcome::Failed(reason),
    }
}

impl Service {
    pub(super) fn cancel_proposals(&mut self) {
        if let Some(running) = &self.targets.proposals_running {
            running.cancelled.store(true, Ordering::Release);
        }
    }

    pub(super) fn start_proposals(
        &mut self,
        ticket: u64,
        session: u64,
        revision: RevisionId,
        request: PictureProposalRequest,
    ) -> Result<()> {
        self.sync_target_session();
        self.check_context(session, &revision)?;
        let asset = request
            .asset
            .clone()
            .ok_or("Picture analysis requires its captured Original asset.")?;
        let pts = request.at_pts;
        let selection = request.selection;
        if self.pending_session_change.is_some() {
            return Err("The project is closing or changing.".into());
        }
        if self.targets.proposals_running.is_some() {
            return Err(
                "Targets are already being found in a picture; wait for it to finish.".into(),
            );
        }
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let package = workspace.path.clone();
        let job = ProposalJob {
            ticket,
            session,
            revision: revision.clone(),
            asset: asset.clone(),
            pts,
            selection,
            outcome: None,
        };
        let installed = || match TrackingRuntime::beside_current_executable() {
            Ok(runtime) if runtime.executable.is_file() => Ok(Detector::Real(runtime)),
            Ok(runtime) => Err(format!(
                "The picture detector is not installed beside Deadpan ({}). Build the deadpan-track workspace member.",
                runtime.executable.display()
            )),
            Err(error) => Err(error.to_string()),
        };
        let detector = match &self.targets.backend {
            Backend::Environment => installed(),
            #[cfg(any(test, feature = "ui-harness"))]
            Backend::Scripted(_) => installed(),
            #[cfg(any(test, feature = "ui-harness"))]
            Backend::ScriptedProposals(script) => match script.next() {
                #[cfg(feature = "ui-harness")]
                Some(crate::project::targets::ProposalRun::Worker) => installed(),
                Some(crate::project::targets::ProposalRun::Proposals {
                    selection,
                    faces,
                    salient_regions,
                    delay,
                }) => Ok(Detector::Scripted {
                    selection,
                    faces,
                    salient_regions,
                    delay,
                }),
                Some(crate::project::targets::ProposalRun::Failed { reason, delay }) => {
                    Ok(Detector::Failed { reason, delay })
                }
                None => Err("The scripted picture detector has no runs.".into()),
            },
        };
        let detector = match detector {
            Ok(detector) => detector,
            Err(reason) => {
                self.targets.proposals = Some(ProposalJob {
                    outcome: Some(ProposalOutcome::Unavailable(reason)),
                    ..job
                });
                return Ok(());
            }
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let (finished, receive) = mpsc::sync_channel(1);
        let thread_cancelled = cancelled.clone();
        let handle = self.shared.job_board.register(
            crate::jobs::JobSpec::new(crate::jobs::JobKind::PictureProposals, Some(session))
                .detail("the displayed picture")
                .cancel_flag(&cancelled),
        );
        let thread = std::thread::Builder::new()
            .name("deadpan-picture-proposals".into())
            .spawn(move || {
                // Registered until the detector has returned.
                let _handle = handle;
                job_thread(
                    detector,
                    package,
                    request,
                    revision,
                    thread_cancelled,
                    finished,
                )
            })
            .map_err(|error| format!("Could not start picture analysis: {error}"))?;
        self.targets.proposals_running = Some(Running {
            cancelled,
            finished: receive,
            thread: Some(thread),
        });
        self.targets.proposals = Some(job);
        self.message = Some(FINDING_PROPOSALS.into());
        Ok(())
    }

    /// Conclude a finished detection. True when published state changed.
    pub(super) fn pump_proposals(&mut self) -> bool {
        let Some(running) = &self.targets.proposals_running else {
            return false;
        };
        let result = match running.finished.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => {
                ProposalOutcome::Failed("The picture-analysis job stopped unexpectedly.".into())
            }
        };
        let mut running = self
            .targets
            .proposals_running
            .take()
            .expect("checked above");
        if let Some(thread) = running.thread.take() {
            let _ = thread.join();
        }
        let result = if running.cancelled.load(Ordering::Acquire) {
            ProposalOutcome::Cancelled
        } else {
            result
        };
        if let Some(job) = &mut self.targets.proposals
            && job.outcome.is_none()
        {
            if self.message.as_deref() == Some(FINDING_PROPOSALS) {
                self.message = Some(match &result {
                    ProposalOutcome::Found(found) => {
                        let faces = found.faces.as_ref().map_or(0, Vec::len);
                        let regions = found.salient_regions.as_ref().map_or(0, Vec::len);
                        if faces + regions == 0 {
                            "No target proposals found in the analysed picture. No edit was made."
                                .into()
                        } else {
                            format!(
                                "{faces} face and {regions} region proposals ready. Detection made no edit."
                            )
                        }
                    }
                    ProposalOutcome::Unavailable(reason) => {
                        format!("Picture analysis unavailable: {reason}")
                    }
                    ProposalOutcome::Failed(reason) => format!("Picture analysis failed: {reason}"),
                    ProposalOutcome::Cancelled => {
                        "Picture analysis cancelled. No edit was made.".into()
                    }
                });
            }
            job.outcome = Some(result);
        }
        true
    }

    /// Save `target` as `id` and set `node`'s framing in one undoable
    /// Compound expecting `revision`: the explicit use of a face proposal.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn save_framed(
        &mut self,
        session: u64,
        revision: RevisionId,
        scope: SequenceScope,
        cursor: deadpan_core::ProjectFrame,
        node: NodeId,
        id: TargetId,
        target: deadpan_core::AttentionTarget,
        framing: Option<Framing>,
    ) -> Result<()> {
        self.sync_target_session();
        self.check_context(session, &revision)?;
        if self.pending_session_change.is_some() {
            return Err("The project is closing or changing.".into());
        }
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        if cursor.0 < 0 || cursor.0 > workspace.plan.duration().frames() {
            return Err("Edit cursor is outside the project".into());
        }
        if !scope.resolve(workspace)?.children.contains(&node) {
            return Err("Select a direct child of the active Sequence before framing".into());
        }
        let document = &workspace.document;
        if document.targets().contains_key(&id) {
            return Err(format!(
                "A target {} already exists; nothing was saved.",
                id.as_str()
            ));
        }
        let label = target.label.clone();
        let described = crate::navigation::zoom::describe(framing.as_ref(), &|followed| {
            if followed == &id {
                label.clone()
            } else {
                document.targets().get(followed).map_or_else(
                    || followed.as_str().to_owned(),
                    |target| target.label.clone(),
                )
            }
        });
        let leaf =
            |command| deadpan_core::LeafEdit::new(super::revision(), command).map_err(display);
        let steps = vec![
            ResolvedStep::Edit {
                edit: leaf(Command::SetTarget {
                    id: id.clone(),
                    target,
                })?,
            },
            ResolvedStep::Edit {
                edit: leaf(Command::SetFraming {
                    node: node.clone(),
                    framing,
                })?,
            },
        ];
        let project_id = document.project_id().clone();
        let writer = self.writer()?;
        let bank = writer.register_version().map_err(display)?;
        let transaction = ResolvedTransaction::new(bank, std::collections::BTreeMap::new(), steps)
            .map_err(display)?;
        let request = CommandRequest {
            project_id,
            expected_revision: revision.clone(),
            new_revision: super::revision(),
            command: Command::Compound { transaction },
        };
        let outcome = writer.commit(&request).map_err(|error| match error {
            StoreError::RevisionConflict { .. } => {
                "The project changed meanwhile; nothing was saved. Try again.".to_owned()
            }
            error => error.to_string(),
        })?;
        self.committed = Some(CommittedEdit {
            scoped: None,
            revision: outcome.revision_id,
            selected_node: Some(node),
            preserve_cursor: true,
            cursor: Some(cursor),
            scope,
            sound: None,
            range_selection: None,
        });
        self.refresh_saved("The proposed target and framing were saved")?;
        self.message = Some(format!(
            "Framing saved: {described}. {label} is a new detected target; one Undo removes both."
        ));
        Ok(())
    }
}

fn job_thread(
    detector: Detector,
    package: PathBuf,
    request: PictureProposalRequest,
    revision: RevisionId,
    cancelled: Arc<AtomicBool>,
    finished: SyncSender<ProposalOutcome>,
) {
    let result = (|| -> std::result::Result<ProposedTargets, ProposalFailure> {
        let deadline = Instant::now() + DEADLINE;
        // Read-only throughout; detection never edits.
        let store = ProjectStore::open(&package, AccessMode::ReadOnly)
            .map_err(|error| ProposalFailure::Unavailable(error.to_string()))?;
        let prepared =
            proposals::prepare_picture_proposals(&store, &request, &cancelled, deadline)?;
        drop(store);
        if prepared.head.revision_id() != &revision {
            return Err(ProposalFailure::Failed(
                "The project changed before targets were found; nothing was proposed.".into(),
            ));
        }
        if prepared.pts != request.at_pts {
            return Err(ProposalFailure::Failed(
                "The displayed picture is not an indexed picture of the Original.".into(),
            ));
        }
        match detector {
            Detector::Real(runtime) => {
                let attempt = uuid::Uuid::new_v4().simple().to_string();
                proposals::detect_picture_proposals(
                    &runtime, &prepared, &attempt, &cancelled, deadline,
                )
                .map(|detection| ProposedTargets::from_worker(detection.proposals))
                .map_err(ProposalFailure::from)
            }
            #[cfg(any(test, feature = "ui-harness"))]
            Detector::Scripted {
                selection,
                faces,
                salient_regions,
                delay,
            } => {
                scripted_wait(delay, &cancelled, deadline)?;
                proposals::admit_proposal_geometry(
                    &prepared,
                    selection,
                    prepared.pts,
                    faces.as_deref(),
                    salient_regions.as_deref(),
                )?;
                Ok(ProposedTargets {
                    faces,
                    salient_regions,
                    origin: ProposalOrigin::Scripted,
                })
            }
            #[cfg(any(test, feature = "ui-harness"))]
            Detector::Failed { reason, delay } => {
                scripted_wait(delay, &cancelled, deadline)?;
                Err(ProposalFailure::Failed(reason))
            }
        }
    })();
    // The writer drains every loop iteration; a stopped writer disconnects.
    let _ = finished.send(match result {
        Ok(found) => ProposalOutcome::Found(found),
        Err(error) => outcome(error),
    });
}

#[cfg(any(test, feature = "ui-harness"))]
fn scripted_wait(
    delay: Duration,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> std::result::Result<(), ProposalFailure> {
    let until = Instant::now()
        .checked_add(delay)
        .ok_or_else(|| ProposalFailure::Failed("Scripted delay overflows.".into()))?;
    loop {
        let now = Instant::now();
        if cancelled.load(Ordering::Acquire) {
            return Err(ProposalFailure::Cancelled);
        }
        if now >= deadline {
            return Err(ProposalFailure::Failed(
                "Picture analysis deadline elapsed.".into(),
            ));
        }
        if now >= until {
            return Ok(());
        }
        std::thread::park_timeout(
            Duration::from_millis(5).min(until.saturating_duration_since(now)),
        );
    }
}
