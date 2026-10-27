//! Speech activity (pause) detection beside transcription.
//!
//! The transcription job prepares the Original's analysis PCM once, runs the
//! Silero detector over it and then transcribes it. An Original that already
//! has a transcript but no speech activity gets a detection-only job. The
//! validated activity is submitted to the project service, which saves it
//! outside history and publishes it with the workspace. Failures are shown
//! in the TRANSCRIPT section with Try again.

use deadpan_analysis::SpeechActivity;
use deadpan_cli::transcription::{OriginalAnalysis, TranscriptionRuntime, prepare_original_audio};
use deadpan_jobs::transcription::ModelInput;
use deadpan_models::packs::InstalledPack;
use deadpan_store::{AccessMode, ProjectStore, SpeechActivityKey};

use super::*;

/// One detection outcome, reported by the job thread.
pub(super) type Detection = Result<(SpeechActivityKey, SpeechActivity), String>;

#[derive(Debug, Clone, PartialEq, Default)]
enum ActivityStatus {
    /// Not yet checked for this project session.
    #[default]
    Unchecked,
    /// The installed pack predates pause detection.
    NeedsModel,
    Detecting,
    /// Waiting for the service to store this attempt.
    Saving(u64),
    Failed(String),
    Interrupted,
    Ready,
}

#[derive(Default)]
pub(super) struct ActivityJob {
    status: ActivityStatus,
    /// Finished activity not yet admitted by a busy project service.
    unsaved: Option<(SpeechActivityKey, Arc<SpeechActivity>)>,
    lifecycle_epoch: Option<u64>,
    run: Option<RunCapture>,
}

impl ActivityJob {
    pub(super) fn detecting(&self) -> bool {
        self.status == ActivityStatus::Detecting
    }

    /// Why pauses are missing, when this job knows.
    pub(super) fn not_ready(&self) -> Option<String> {
        Some(match &self.status {
            ActivityStatus::NeedsModel => {
                "Pauses need the updated transcription model. Update it with :models.".into()
            }
            ActivityStatus::Detecting | ActivityStatus::Saving(_) => {
                "Pauses are not ready while the Original's speech is analysed.".into()
            }
            ActivityStatus::Interrupted => "Pauses are not ready: analysis was interrupted by system sleep. Try again in the Original rail.".into(),
            ActivityStatus::Failed(_) => {
                "Pauses are not ready: detection failed. Try again in the Original rail.".into()
            }
            ActivityStatus::Unchecked | ActivityStatus::Ready => return None,
        })
    }

    /// A pack changed in the Models panel: check a missing model again.
    pub(super) fn recheck(&mut self) {
        if self.status == ActivityStatus::NeedsModel {
            self.status = ActivityStatus::Unchecked;
        }
    }

    pub(super) fn start(&mut self, run: RunCapture) {
        self.status = ActivityStatus::Detecting;
        self.unsaved = None;
        self.lifecycle_epoch = Some(run.epoch);
        self.run = Some(run);
        self.observe_interruption();
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn replay_queued(&mut self, lifecycle: crate::jobs::Lifecycle) {
        assert!(self.unsaved.is_none());
        self.start(RunCapture::new(lifecycle, Arc::new(AtomicBool::new(false))));
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn status_name(&self) -> &'static str {
        match self.status {
            ActivityStatus::Interrupted => "interrupted",
            ActivityStatus::NeedsModel => "needs_model",
            ActivityStatus::Ready => "ready",
            _ => "pending_or_failed",
        }
    }

    pub(super) fn observe_interruption(&mut self) {
        let Some(stop) = self.run.as_ref().and_then(RunCapture::stop) else {
            return;
        };
        // The transactional receipt owns the outcome once a save is admitted.
        if matches!(self.status, ActivityStatus::Saving(_)) {
            return;
        }
        self.unsaved = None;
        self.status = match stop {
            Stop::Interrupted => ActivityStatus::Interrupted,
            Stop::Cancelled => ActivityStatus::Failed("pause detection was cancelled".into()),
        };
    }

    pub(super) fn accepts_result(&mut self) -> bool {
        let accepted = self.run.as_ref().and_then(RunCapture::stop).is_none();
        if !accepted {
            self.observe_interruption();
        }
        accepted
    }

    fn retry(&mut self, lifecycle: &crate::jobs::Lifecycle, drained: bool) -> bool {
        if !drained
            || !lifecycle.is_awake()
            || !matches!(
                self.status,
                ActivityStatus::Interrupted | ActivityStatus::Failed(_)
            )
        {
            return false;
        }
        self.run = None;
        self.lifecycle_epoch = None;
        self.status = ActivityStatus::Unchecked;
        true
    }

    pub(super) fn receive(&mut self, result: Detection, lifecycle_epoch: Option<u64>) {
        if !self.accepts_result() {
            return;
        }
        self.lifecycle_epoch = self.run.as_ref().map(|run| run.epoch).or(lifecycle_epoch);
        match result {
            Ok((key, activity)) => self.unsaved = Some((key, Arc::new(activity))),
            Err(error) => {
                self.run = None;
                self.status = ActivityStatus::Failed(error);
            }
        }
    }
}

impl Transcription {
    pub(crate) fn receive_activity_save(&mut self, save: Option<crate::project::TranscriptSave>) {
        let Some(save) = save else {
            return;
        };
        if self.session != Some(save.session)
            || self.activity.status != ActivityStatus::Saving(save.attempt)
        {
            return;
        }
        let cancelled = save.interrupted
            && self.activity.run.as_ref().and_then(RunCapture::stop) == Some(Stop::Cancelled);
        self.activity.run = None;
        if let Some(error) = save.error {
            self.activity.status = if save.interrupted && !cancelled {
                ActivityStatus::Interrupted
            } else {
                ActivityStatus::Failed(if cancelled {
                    "pause detection was cancelled".into()
                } else {
                    format!("the pauses were not saved: {error}")
                })
            };
        }
    }
}

/// The pack's installed Silero model, if it has one.
pub(super) fn vad_model(pack: &PackManifest, installed: &InstalledPack) -> Option<ModelInput> {
    let file = pack.speech_activity_file()?;
    Some(ModelInput {
        path: installed.file(&file.name)?,
        sha256: deadpan_jobs::Sha256::new(file.sha256.clone()).ok()?,
        byte_length: file.bytes,
    })
}

/// Prepare the Original's analysis PCM from a read-only store and locate the
/// worker beside the executable.
pub(super) fn prepare(
    package: &std::path::Path,
    cancel: &AtomicBool,
    deadline: std::time::Instant,
) -> Result<(OriginalAnalysis, TranscriptionRuntime), String> {
    let store = ProjectStore::open(package, AccessMode::ReadOnly).map_err(|e| e.to_string())?;
    let analysis =
        prepare_original_audio(&store, None, cancel, deadline).map_err(|e| e.to_string())?;
    drop(store);
    let runtime = TranscriptionRuntime::beside_current_executable().map_err(|e| e.to_string())?;
    if !runtime.executable.is_file() {
        return Err("the transcription helper is missing beside Deadpan".into());
    }
    Ok((analysis, runtime))
}

impl DeadpanApp {
    /// Detect speech for an Original that has a transcript but no activity.
    pub(super) fn reconcile_activity(&mut self, context: &egui::Context) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        if workspace.speech_activity.is_some() {
            self.transcription.activity.status = ActivityStatus::Ready;
            self.transcription.activity.run = None;
            return;
        }
        let idle = self.transcription.events.is_none();
        let job = &mut self.transcription.activity;
        if job.status == ActivityStatus::Ready {
            job.status = ActivityStatus::Unchecked;
        }
        if job.status != ActivityStatus::Unchecked
            || job.unsaved.is_some()
            || !idle
            || !matches!(
                workspace.single_source,
                Some(SingleSourceState::Ready { .. })
            )
        {
            return;
        }
        let (Some(pack), Some(store)) = (transcription_pack(), self.models.manager.store()) else {
            self.transcription.activity.status =
                ActivityStatus::Failed("Model storage is unavailable.".into());
            return;
        };
        // The selected version: an activated signed update, else compiled.
        let vad = match store.current(&pack.pack_id) {
            Ok(Some(installed)) => vad_model(&installed.manifest, &installed),
            Ok(None) => None,
            Err(error) => {
                self.transcription.activity.status = ActivityStatus::Failed(error.to_string());
                return;
            }
        };
        let Some(vad) = vad else {
            self.transcription.activity.status = ActivityStatus::NeedsModel;
            return;
        };
        let package = workspace.path.clone();
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let run = RunCapture::new(self.service.jobs().lifecycle(), Arc::clone(&cancel));
        self.transcription.activity.start(run.clone());
        self.prioritize_analysis_cursor();
        if run.stop().is_some() {
            return;
        }
        self.transcription.cancel = Arc::clone(&cancel);
        self.transcription.events = Some(receiver);
        let repaint = context.clone();
        let cursor = self.transcription.ranges.sender.cursor.clone();
        let handle = self.service.jobs().register(
            crate::jobs::JobSpec::new(
                crate::jobs::JobKind::SpeechActivity,
                self.transcription.session,
            )
            .detail("the Original")
            .cancel_flag(&cancel),
        );
        let spawned = std::thread::Builder::new()
            .name("deadpan-speech-activity".into())
            .spawn(move || {
                let deadline = std::time::Instant::now() + Duration::from_secs(60 * 60);
                let result = run.run(|| {
                    handle
                        .wait_admitted(Some(&cancel))
                        .map_err(|stopped| {
                            format!("pause detection was {stopped} before it started")
                        })
                        .and_then(|()| {
                            let store = ProjectStore::open(&package, AccessMode::ReadOnly)
                                .map_err(|error| error.to_string())?;
                            let runtime = TranscriptionRuntime::beside_current_executable()
                                .map_err(|error| error.to_string())?;
                            let cache = deadpan_cli::storage::UserStorage::current()
                                .map(|storage| storage.caches.join("AnalysisRanges"));
                            deadpan_cli::transcription::incremental::detect_original_ranges(
                                &store,
                                &runtime,
                                &vad,
                                cache.as_deref(),
                                &cursor,
                                &cancel,
                                deadline,
                                |done, total| {
                                    handle.checkpoint(Some(&cancel)).map_err(|stopped| {
                                        deadpan_cli::transcription::TranscriptionError::Worker(
                                            format!("pause detection was {stopped}"),
                                        )
                                    })?;
                                    handle.set_progress(
                                        "Detecting speech ranges",
                                        Some(done as f32 / total.max(1) as f32),
                                    );
                                    Ok(())
                                },
                            )
                            .map_err(|error| error.to_string())
                        })
                });
                let _ = sender.send(Event::Detected { result, last: true });
                repaint.request_repaint();
            });
        match spawned {
            Ok(thread) => self.transcription.thread = Some(thread),
            Err(error) => {
                self.transcription.events = None;
                self.transcription.activity.run = None;
                self.transcription.activity.status = ActivityStatus::Failed(error.to_string());
            }
        }
    }

    /// Submit finished activity like a transcript: without the side effects
    /// of a user command, retrying while the service is busy.
    pub(super) fn save_activity(&mut self, session: Option<u64>, context: &egui::Context) {
        let Some(session) = session else {
            self.transcription.activity.unsaved = None;
            return;
        };
        let Some((key, activity)) = self.transcription.activity.unsaved.as_ref() else {
            return;
        };
        // Saves use their own lane and never occupy the user-command slot,
        // but still yield while a user command is pending.
        if self.service.is_busy() || self.service.annotation_busy() {
            context.request_repaint_after(Duration::from_millis(100));
            return;
        }
        self.transcription.attempts += 1;
        let attempt = self.transcription.attempts;
        let request = ProjectRequest::SaveSpeechActivity {
            expected_session: session,
            lifecycle_epoch: self.transcription.activity.lifecycle_epoch,
            attempt,
            key: key.clone(),
            activity: Arc::clone(activity),
        };
        match self.service.submit(request) {
            Ok(()) => {
                self.transcription.activity.unsaved = None;
                self.transcription.activity.status = ActivityStatus::Saving(attempt);
            }
            Err(_) if self.service.annotation_busy() => {
                context.request_repaint_after(Duration::from_millis(100));
            }
            Err(error) => {
                self.transcription.activity.unsaved = None;
                self.transcription.activity.run = None;
                self.transcription.activity.status =
                    ActivityStatus::Failed(format!("the pauses were not saved: {error}"));
            }
        }
    }

    /// Pause detection's state, below the transcript when it needs attention.
    pub(super) fn activity_section(&mut self, ui: &mut egui::Ui) {
        match self.transcription.activity.status.clone() {
            ActivityStatus::Detecting => {
                ui.horizontal(|ui| {
                    crate::preview::accessibility::busy(ui);
                    ui.weak("Detecting pauses…");
                });
            }
            ActivityStatus::NeedsModel if self.transcription.status == Status::Ready => {
                ui.weak(
                    "Pause detection needs the updated model pack, which adds the 0.9 MB Silero speech detector (MIT license).",
                );
                let id = transcription_pack()
                    .map(|pack| pack.pack_id)
                    .unwrap_or_default();
                if self.model_pack_offer(ui, &id, "Update model…", ":models") {
                    self.offer_transcription_model(ui.ctx());
                }
            }
            ActivityStatus::Interrupted => {
                let message = ui
                    .label("Pause detection interrupted by system sleep. Try again to restart it.");
                let retry = ui.add_enabled(
                    self.transcription.events.is_none()
                        && self.service.jobs().lifecycle().is_awake(),
                    style::row_action(ui, "Try again", ""),
                );
                // Leave scroll range beyond the button border so fractional
                // device-pixel rounding cannot clip its final row.
                ui.add_space(4.0);
                reveal_interruption(ui, message.rect, &retry);
                if retry.clicked() {
                    self.transcription.activity.retry(
                        &self.service.jobs().lifecycle(),
                        self.transcription.events.is_none(),
                    );
                }
            }
            ActivityStatus::Failed(error) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Pause detection failed: {error}"),
                );
                if ui
                    .add_enabled(
                        self.transcription.events.is_none()
                            && self.service.jobs().lifecycle().is_awake(),
                        style::row_action(ui, "Try again", ""),
                    )
                    .clicked()
                {
                    self.transcription.activity.retry(
                        &self.service.jobs().lifecycle(),
                        self.transcription.events.is_none(),
                    );
                }
            }
            ActivityStatus::Unchecked
            | ActivityStatus::NeedsModel
            | ActivityStatus::Saving(_)
            | ActivityStatus::Ready => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_receipt_keeps_guard_interruption_distinct_from_real_failure() {
        for interrupted in [false, true] {
            let mut state = Transcription {
                session: Some(7),
                ..Transcription::default()
            };
            state.activity.status = ActivityStatus::Saving(4);
            state.receive_activity_save(Some(crate::project::TranscriptSave {
                session: 7,
                attempt: 4,
                error: Some("save diagnostic".into()),
                interrupted,
            }));
            if interrupted {
                assert_eq!(state.activity.status, ActivityStatus::Interrupted);
            } else {
                assert!(
                    matches!(state.activity.status, ActivityStatus::Failed(ref error) if error.contains("save diagnostic"))
                );
            }
        }
    }
    fn detected() -> Detection {
        Ok((
            SpeechActivityKey {
                content: "a".repeat(64),
                audio_stream: 0,
                model_sha256: "b".repeat(64),
                engine: "test".into(),
            },
            SpeechActivity::new(
                deadpan_analysis::ActivityAudio {
                    origin: 0,
                    sample_rate: 16_000,
                    samples: 512,
                },
                vec![240],
                vec![200; 4],
            )
            .unwrap(),
        ))
    }

    #[test]
    fn standalone_sleep_rejects_queued_running_and_buffered_results_until_explicit_retry() {
        for buffered in [false, true] {
            let lifecycle = crate::jobs::Lifecycle::default();
            let cancel = Arc::new(AtomicBool::new(false));
            let mut state = Transcription {
                status: Status::Ready,
                ..Transcription::default()
            };
            let (sender, receiver) = mpsc::channel();
            state.events = Some(receiver);
            let run = RunCapture::new(lifecycle.clone(), cancel.clone());
            state.activity.start(run.clone());
            if buffered {
                state.activity.receive(detected(), None);
            }
            lifecycle.will_sleep();
            lifecycle.did_wake();
            state.observe_interruption();
            assert_eq!(state.status, Status::Ready);
            assert_eq!(state.activity.status, ActivityStatus::Interrupted);
            assert!(cancel.load(Ordering::Acquire));
            assert!(state.activity.unsaved.is_none());
            assert!(!state.activity.retry(&lifecycle, state.events.is_none()));
            sender
                .send(Event::Detected {
                    result: detected(),
                    last: true,
                })
                .unwrap();
            let event = state.events.as_ref().unwrap().try_recv().unwrap();
            assert!(!state.accepts_event(&event));
            assert!(state.events.is_none());
            state.activity.recheck();
            assert_eq!(state.activity.status, ActivityStatus::Interrupted);
            lifecycle.will_sleep();
            assert!(!state.activity.retry(&lifecycle, true));
            lifecycle.did_wake();
            assert!(state.activity.retry(&lifecycle, true));
            let retry = RunCapture::new(lifecycle.clone(), Arc::new(AtomicBool::new(false)));
            assert_ne!(retry.epoch, run.epoch);
            state.activity.start(retry);
            state.activity.receive(detected(), None);
            assert!(state.activity.unsaved.is_some());
            assert_eq!(state.activity.lifecycle_epoch, Some(lifecycle.snapshot()));
        }
    }

    #[test]
    fn standalone_save_receipt_preserves_commit_failure_and_prior_cancellation() {
        for (interrupted, error, cancelled) in [
            (false, None, false),
            (false, Some("disk failure"), false),
            (true, Some("sleep guard"), false),
            (true, Some("sleep guard"), true),
        ] {
            let lifecycle = crate::jobs::Lifecycle::default();
            let cancel = Arc::new(AtomicBool::new(false));
            let mut state = Transcription {
                session: Some(7),
                status: Status::Ready,
                ..Transcription::default()
            };
            state
                .activity
                .start(RunCapture::new(lifecycle.clone(), cancel.clone()));
            state.activity.status = ActivityStatus::Saving(4);
            if cancelled {
                cancel.store(true, Ordering::Release);
            }
            lifecycle.will_sleep();
            state.observe_interruption();
            assert_eq!(state.activity.status, ActivityStatus::Saving(4));
            state.receive_activity_save(Some(crate::project::TranscriptSave {
                session: 7,
                attempt: 4,
                interrupted,
                error: error.map(str::to_owned),
            }));
            state.observe_interruption();
            assert!(state.activity.run.is_none());
            assert_eq!(state.status, Status::Ready);
            if cancelled {
                assert!(
                    matches!(state.activity.status, ActivityStatus::Failed(ref error) if error.contains("cancelled"))
                );
            } else if interrupted {
                assert_eq!(state.activity.status, ActivityStatus::Interrupted);
            } else if error.is_some() {
                assert!(
                    matches!(state.activity.status, ActivityStatus::Failed(ref error) if error.contains("disk failure"))
                );
            } else {
                assert_eq!(state.activity.status, ActivityStatus::Saving(4));
            }
        }
    }
}
