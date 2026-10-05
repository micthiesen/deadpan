//! Face proposals on the project service. One bounded job thread per project
//! resolves one indexed picture from a read-only store, copies the verified
//! Original and runs the supervised `deadpan-track detect-faces` worker; the
//! writer publishes the admitted faces as proposals and never edits. Using a
//! face is a separate, explicit `SaveFramed` command: the face becomes an
//! ordinary target and the beat's framing changes in one undoable Compound.
//! Session replacement and shutdown cancel the job and wait until it drains.

use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Instant;

use deadpan_cli::faces::{self, DetectedFace, FaceError, FaceRequest};
use deadpan_cli::tracking::TrackingRuntime;
use deadpan_core::{Framing, ResolvedStep, ResolvedTransaction, TargetId};

use super::*;
use crate::project::targets::{FaceJob, FaceOutcome};

/// Longest one detection may run, including the Original copy.
const DEADLINE: Duration = Duration::from_secs(10 * 60);

pub(in crate::project::service) struct Running {
    cancelled: Arc<AtomicBool>,
    finished: Receiver<FaceOutcome>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        // Never leave a cooperative job running without a cancellation request.
        self.cancelled.store(true, Ordering::Release);
    }
}

/// What detects: the installed worker, or the test seam's faces.
enum Detector {
    Real(TrackingRuntime),
    #[cfg(any(test, feature = "ui-harness"))]
    Scripted {
        faces: Vec<DetectedFace>,
        delay: Duration,
    },
}

fn outcome(error: FaceError) -> FaceOutcome {
    match error {
        FaceError::Cancelled => FaceOutcome::Cancelled,
        FaceError::Unavailable(reason) => FaceOutcome::Unavailable(reason),
        error => FaceOutcome::Failed(error.to_string()),
    }
}

impl Service {
    pub(super) fn cancel_faces(&mut self) {
        if let Some(running) = &self.targets.faces_running {
            running.cancelled.store(true, Ordering::Release);
        }
    }

    pub(super) fn start_faces(
        &mut self,
        ticket: u64,
        session: u64,
        revision: RevisionId,
        asset: AssetId,
        pts: i64,
    ) -> Result<()> {
        self.sync_target_session();
        self.check_context(session, &revision)?;
        if self.pending_session_change.is_some() {
            return Err("The project is closing or changing.".into());
        }
        if self.targets.faces_running.is_some() {
            return Err(
                "Faces are already being found in a picture; wait for it to finish.".into(),
            );
        }
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let package = workspace.path.clone();
        let job = FaceJob {
            ticket,
            session,
            revision: revision.clone(),
            asset: asset.clone(),
            pts,
            outcome: None,
        };
        let unavailable = |reason: String| FaceOutcome::Unavailable(reason);
        let installed = || match TrackingRuntime::beside_current_executable() {
            Ok(runtime) if runtime.executable.is_file() => Ok(Detector::Real(runtime)),
            Ok(runtime) => Err(unavailable(format!(
                "The face detector is not installed beside Deadpan ({}). Build the deadpan-track workspace member.",
                runtime.executable.display()
            ))),
            Err(error) => Err(unavailable(error.to_string())),
        };
        let detector = match &self.targets.backend {
            Backend::Environment => installed(),
            #[cfg(any(test, feature = "ui-harness"))]
            Backend::Scripted(_) => installed(),
            #[cfg(any(test, feature = "ui-harness"))]
            Backend::ScriptedFaces(script) => match script.next() {
                Some(crate::project::targets::FaceRun::Faces { faces, delay }) => {
                    Ok(Detector::Scripted { faces, delay })
                }
                Some(crate::project::targets::FaceRun::Worker) => installed(),
                None => Err(unavailable(
                    "The scripted face detector has no runs.".into(),
                )),
            },
        };
        let detector = match detector {
            Ok(detector) => detector,
            Err(outcome) => {
                self.targets.faces = Some(FaceJob {
                    outcome: Some(outcome),
                    ..job
                });
                return Ok(());
            }
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let (finished, receive) = mpsc::sync_channel(1);
        let thread_cancelled = cancelled.clone();
        let request = FaceRequest {
            asset: Some(asset),
            at_pts: pts,
        };
        let thread = std::thread::Builder::new()
            .name("deadpan-faces".into())
            .spawn(move || {
                job_thread(
                    detector,
                    package,
                    request,
                    revision,
                    thread_cancelled,
                    finished,
                )
            })
            .map_err(|error| format!("Could not start face detection: {error}"))?;
        self.targets.faces_running = Some(Running {
            cancelled,
            finished: receive,
            thread: Some(thread),
        });
        self.targets.faces = Some(job);
        self.message = Some("Finding faces in the displayed picture…".into());
        Ok(())
    }

    /// Conclude a finished detection. True when published state changed.
    pub(super) fn pump_faces(&mut self) -> bool {
        let Some(running) = &self.targets.faces_running else {
            return false;
        };
        let result = match running.finished.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => {
                FaceOutcome::Failed("The face-detection job stopped unexpectedly.".into())
            }
        };
        let mut running = self.targets.faces_running.take().expect("checked above");
        if let Some(thread) = running.thread.take() {
            let _ = thread.join();
        }
        let result = if running.cancelled.load(Ordering::Acquire) {
            FaceOutcome::Cancelled
        } else {
            result
        };
        if let Some(job) = &mut self.targets.faces
            && job.outcome.is_none()
        {
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
        self.refresh_saved("The face target and framing were saved")?;
        self.message = Some(format!(
            "Framing saved: {described}. {label} is a new target from face detection; one Undo removes both."
        ));
        Ok(())
    }
}

fn job_thread(
    detector: Detector,
    package: PathBuf,
    request: FaceRequest,
    revision: RevisionId,
    cancelled: Arc<AtomicBool>,
    finished: SyncSender<FaceOutcome>,
) {
    let result = (|| -> std::result::Result<Vec<DetectedFace>, FaceOutcome> {
        let deadline = Instant::now() + DEADLINE;
        // Read-only throughout; detection never edits.
        let store = ProjectStore::open(&package, AccessMode::ReadOnly)
            .map_err(|error| FaceOutcome::Unavailable(error.to_string()))?;
        let prepared =
            faces::prepare_faces(&store, &request, &cancelled, deadline).map_err(outcome)?;
        drop(store);
        if prepared.head.revision_id() != &revision {
            return Err(FaceOutcome::Failed(
                "The project changed before faces were found; nothing was proposed.".into(),
            ));
        }
        if prepared.pts != request.at_pts {
            return Err(FaceOutcome::Failed(
                "The displayed picture is not an indexed picture of the Original.".into(),
            ));
        }
        match detector {
            Detector::Real(runtime) => {
                let attempt = uuid::Uuid::new_v4().simple().to_string();
                faces::detect_faces(&runtime, &prepared, &attempt, &cancelled, deadline)
                    .map(|detection| detection.faces)
                    .map_err(outcome)
            }
            #[cfg(any(test, feature = "ui-harness"))]
            Detector::Scripted {
                faces: found,
                delay,
            } => {
                let until = Instant::now() + delay;
                while Instant::now() < until {
                    if cancelled.load(Ordering::Acquire) {
                        return Err(FaceOutcome::Cancelled);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                faces::admit_faces(&prepared, prepared.pts, found).map_err(outcome)
            }
        }
    })();
    // The writer drains every loop iteration; a stopped writer disconnects.
    let _ = finished.send(match result {
        Ok(found) => FaceOutcome::Found(found),
        Err(outcome) => outcome,
    });
}
