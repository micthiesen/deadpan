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
    /// The updated pack is being installed from this section.
    Installing,
    Detecting,
    /// Waiting for the service to store this attempt.
    Saving(u64),
    Failed(String),
    Ready,
}

#[derive(Default)]
pub(super) struct ActivityJob {
    status: ActivityStatus,
    /// Finished activity not yet admitted by a busy project service.
    unsaved: Option<(SpeechActivityKey, Arc<SpeechActivity>)>,
}

impl ActivityJob {
    /// Why pauses are missing, when this job knows.
    pub(super) fn not_ready(&self) -> Option<String> {
        Some(match &self.status {
            ActivityStatus::NeedsModel => {
                "Pauses need the updated transcription model. Update it in the Original rail."
                    .into()
            }
            ActivityStatus::Installing => "Pauses are not ready while the model updates.".into(),
            ActivityStatus::Detecting | ActivityStatus::Saving(_) => {
                "Pauses are not ready while the Original's speech is analysed.".into()
            }
            ActivityStatus::Failed(_) => {
                "Pauses are not ready: detection failed. Try again in the Original rail.".into()
            }
            ActivityStatus::Unchecked | ActivityStatus::Ready => return None,
        })
    }

    pub(super) fn start(&mut self) {
        self.status = ActivityStatus::Detecting;
        self.unsaved = None;
    }

    pub(super) fn receive(&mut self, result: Detection) {
        match result {
            Ok((key, activity)) => self.unsaved = Some((key, Arc::new(activity))),
            Err(error) => self.status = ActivityStatus::Failed(error),
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
        if let Some(error) = save.error {
            self.activity.status =
                ActivityStatus::Failed(format!("the pauses were not saved: {error}"));
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

/// Detect speech in prepared analysis PCM.
pub(super) fn detect(
    runtime: &TranscriptionRuntime,
    vad: &ModelInput,
    analysis: &OriginalAnalysis,
    cancel: &AtomicBool,
    deadline: std::time::Instant,
) -> Detection {
    let attempt = uuid::Uuid::new_v4().simple().to_string();
    let result = deadpan_cli::activity::detect_speech(
        runtime,
        vad,
        &analysis.input,
        &attempt,
        cancel,
        deadline,
    )
    .map_err(|e| e.to_string())?;
    Ok((
        SpeechActivityKey {
            content: analysis.content.clone(),
            audio_stream: analysis.audio_stream,
            model_sha256: vad.sha256.as_str().to_owned(),
            engine: result.runtime.engine,
        },
        result.activity,
    ))
}

impl DeadpanApp {
    /// Detect speech for an Original that has a transcript but no activity.
    pub(super) fn reconcile_activity(&mut self, context: &egui::Context) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        if workspace.speech_activity.is_some() {
            self.transcription.activity.status = ActivityStatus::Ready;
            return;
        }
        let idle = self.transcription.events.is_none();
        let job = &mut self.transcription.activity;
        match job.status {
            ActivityStatus::Ready => job.status = ActivityStatus::Unchecked,
            // An install started here finishes when its job ends; the
            // transcript status still holds an install failure this frame.
            ActivityStatus::Installing if idle => {
                job.status = match &self.transcription.status {
                    Status::Failed(error) => ActivityStatus::Failed(format!(
                        "the updated model was not installed: {error}"
                    )),
                    _ => ActivityStatus::Unchecked,
                };
            }
            _ => {}
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
        let (Some(pack), Some(store)) = (transcription_pack(), self.transcription.pack_store())
        else {
            self.transcription.activity.status =
                ActivityStatus::Failed("Model storage is unavailable.".into());
            return;
        };
        let vad = match store.installed(&pack) {
            Ok(Some(installed)) => vad_model(&pack, &installed),
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
        self.transcription.cancel = Arc::clone(&cancel);
        self.transcription.events = Some(receiver);
        self.transcription.activity.start();
        let repaint = context.clone();
        let spawned = std::thread::Builder::new()
            .name("deadpan-speech-activity".into())
            .spawn(move || {
                let deadline = std::time::Instant::now() + Duration::from_secs(60 * 60);
                let result =
                    prepare(&package, &cancel, deadline).and_then(|(analysis, runtime)| {
                        detect(&runtime, &vad, &analysis, &cancel, deadline)
                    });
                let _ = sender.send(Event::Detected { result, last: true });
                repaint.request_repaint();
            });
        match spawned {
            Ok(thread) => self.transcription.thread = Some(thread),
            Err(error) => {
                self.transcription.events = None;
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
                    ui.spinner();
                    ui.weak("Detecting pauses…");
                });
            }
            ActivityStatus::Installing => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.weak("Installing the updated model…");
                });
            }
            ActivityStatus::NeedsModel if self.transcription.status == Status::Ready => {
                ui.weak(
                    "Pause detection needs the updated model pack, which adds the 0.9 MB Silero speech detector (MIT license).",
                );
                if ui
                    .add(style::row_action(ui, "Update model…", ""))
                    .on_hover_text("Download, verify and install the updated pack. An installed transcription model is reused, not downloaded again.")
                    .clicked()
                {
                    self.transcription.activity.status = ActivityStatus::Installing;
                    self.install_transcription_model(ui.ctx());
                }
            }
            ActivityStatus::Failed(error) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Pause detection failed: {error}"),
                );
                if ui.add(style::row_action(ui, "Try again", "")).clicked() {
                    self.transcription.activity.status = ActivityStatus::Unchecked;
                }
            }
            ActivityStatus::Unchecked
            | ActivityStatus::NeedsModel
            | ActivityStatus::Saving(_)
            | ActivityStatus::Ready => {}
        }
    }
}
