//! A single, cancellable native local timing job and an explicit review sheet.
//! All media/model access, snapshot cloning and waveform measurement is worker work.

use super::*;
use deadpan_analysis::refinement::{RefinementOfferAvailability, RefinementReview};
use deadpan_core::AudioSample;
use std::sync::mpsc;
use std::time::Instant;

mod ui;
mod worker;

const DEADLINE: Duration = Duration::from_millis(deadpan_jobs::refinement::MAX_TIMEOUT_MS);

pub(super) fn language_problem(language: &str) -> Option<String> {
    deadpan_cli::refinement::validate_language(language)
        .err()
        .map(|error| error.to_string())
}

/// egui activates focused buttons directly. Held, composed or modified
/// activation keys must not bypass the modal router's explicit safeguards.
pub(super) fn retain_native_activation(context: &egui::Context, ime: bool) {
    context.input_mut(|input| {
        input.events.retain(|event| {
            !matches!(event,
        egui::Event::Key { key: egui::Key::Enter | egui::Key::Space, modifiers, repeat, .. }
            if ime || *repeat || *modifiers != egui::Modifiers::NONE)
        })
    });
}

#[derive(Clone)]
struct Capture {
    workspace: Arc<Workspace>,
    transcript: Arc<crate::project::OriginalTranscript>,
    corrections: Arc<crate::project::OriginalCorrections>,
    source: Arc<crate::project::RegisteredSource>,
    model_root: PathBuf,
    word: usize,
    lifecycle: crate::jobs::Lifecycle,
    epoch: u64,
}

impl Capture {
    fn matches(&self, workspace: &Workspace) -> bool {
        self.workspace.session == workspace.session
            && self.workspace.document.revision_id() == workspace.document.revision_id()
            && workspace
                .transcript
                .as_ref()
                .is_some_and(|value| Arc::ptr_eq(value, &self.transcript))
            && workspace
                .corrections
                .as_ref()
                .is_some_and(|value| Arc::ptr_eq(value, &self.corrections))
    }

    fn interrupted(&self) -> bool {
        self.lifecycle.interrupted_since(self.epoch)
    }
}

struct Prepared {
    review: Arc<RefinementReview>,
    waveform: Vec<(f32, f32)>,
    audition: Arc<deadpan_playback::AudioRange>,
    source_start: i64,
    analysis_origin: i64,
    source_rate: u32,
    #[cfg(feature = "ui-harness")]
    engine: String,
}

enum Phase {
    Running,
    Ready(Arc<Prepared>),
    NeedsModel,
    Failed(String),
    Saving(u64),
}

pub(super) struct State {
    open: bool,
    phase: Phase,
    capture: Option<Capture>,
    cancel: Arc<AtomicBool>,
    events: Option<mpsc::Receiver<Result<Prepared, worker::Failure>>>,
    thread: Option<std::thread::JoinHandle<()>>,
    entry_focus: Option<egui::Id>,
    focus_pending: bool,
    audition_cursor: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            phase: Phase::Failed(String::new()),
            capture: None,
            cancel: Arc::new(AtomicBool::new(false)),
            events: None,
            thread: None,
            entry_focus: None,
            focus_pending: false,
            audition_cursor: 0,
        }
    }
}

impl State {
    fn finish_worker(&mut self) {
        if self
            .thread
            .as_ref()
            .is_some_and(|thread| thread.is_finished())
            && let Some(thread) = self.thread.take()
            && thread.join().is_err()
            && matches!(self.phase, Phase::Running)
        {
            self.phase = Phase::Failed(
                "The local refinement worker stopped unexpectedly. Nothing was accepted.".into(),
            );
            self.events = None;
        }
    }
    pub(super) fn open(&self) -> bool {
        self.open
    }
    pub(super) fn entry_focused(&self, context: &egui::Context) -> bool {
        self.entry_focus
            .is_some_and(|id| context.memory(|memory| memory.has_focus(id)))
    }
    pub(super) fn set_entry(&mut self, id: egui::Id) {
        self.entry_focus = Some(id);
    }

    pub(super) fn follow_audition(&mut self, cursor: u64) {
        if self.open {
            self.audition_cursor = cursor;
        }
    }

    pub(super) fn shutdown(&mut self) {
        self.cancel.store(true, Ordering::Release);
        let Some(thread) = self.thread.take() else {
            return;
        };
        let deadline = Instant::now() + Duration::from_secs(3);
        while !thread.is_finished() && Instant::now() < deadline {
            std::thread::park_timeout(Duration::from_millis(20));
        }
        if thread.is_finished() {
            let _ = thread.join();
        }
    }
}

impl DeadpanApp {
    pub(super) fn start_refinement(&mut self, word: usize, context: &egui::Context) {
        self.pause_playback();
        self.reconcile_refinement(context);
        if self
            .refinement
            .thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
        {
            self.refinement.phase = Phase::Failed(
                "The previous local refinement is still stopping. Try again when it finishes."
                    .into(),
            );
            self.refinement.open = true;
            return;
        }
        let captured = (|| -> Result<Capture, String> {
            let workspace = self.workspace.clone().ok_or("Open a project first")?;
            let transcript = workspace
                .transcript
                .clone()
                .ok_or("The Original needs a transcript first")?;
            if let Some(problem) = transcript.problem() {
                return Err(problem);
            }
            if let Some(problem) = language_problem(&transcript.key.language) {
                return Err(problem);
            }
            let corrections = workspace
                .corrections
                .clone()
                .ok_or("The Original's corrections are not loaded")?;
            if corrections.error.is_some() {
                return Err(
                    "Stored corrections are unreadable. Resolve them before refinement.".into(),
                );
            }
            if transcript.transcript.words().get(word).is_none() {
                return Err("Select a word first".into());
            }
            let source = workspace
                .sources
                .values()
                .find(|source| {
                    workspace
                        .document
                        .assets()
                        .get(&source.asset)
                        .is_some_and(|asset| asset.content_hash == transcript.key.content)
                })
                .cloned()
                .ok_or("The transcript's qualified Original is not available")?;
            let model_root = self.models.manager.root()?.to_path_buf();
            let lifecycle = self.service.jobs().lifecycle();
            let epoch = lifecycle.snapshot();
            Ok(Capture {
                workspace,
                transcript,
                corrections,
                source,
                model_root,
                word,
                lifecycle,
                epoch,
            })
        })();
        self.refinement.open = true;
        self.refinement.focus_pending = true;
        let capture = match captured {
            Ok(capture) => capture,
            Err(error) => {
                self.refinement.phase = Phase::Failed(error);
                return;
            }
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::sync_channel(1);
        self.refinement.cancel = cancel.clone();
        self.refinement.events = Some(receiver);
        self.refinement.capture = Some(capture.clone());
        self.refinement.phase = Phase::Running;
        self.refinement.audition_cursor = 0;
        let repaint = context.clone();
        let handle = self.service.jobs().register(
            crate::jobs::JobSpec::new(
                crate::jobs::JobKind::Transcription,
                Some(capture.workspace.session),
            )
            .estimated_memory_bytes(512 * 1024 * 1024)
            .deferred()
            .detail("approximate local word timing")
            .cancel_flag(&cancel),
        );
        match std::thread::Builder::new()
            .name("deadpan-word-refinement".into())
            .spawn(move || {
                let result = worker::run(&capture, &cancel, &handle);
                let _ = sender.send(result);
                repaint.request_repaint();
            }) {
            Ok(thread) => self.refinement.thread = Some(thread),
            Err(error) => {
                self.refinement.events = None;
                self.refinement.phase = Phase::Failed(error.to_string());
            }
        }
        self.bindings.clear();
        context.request_repaint();
    }

    pub(super) fn reconcile_refinement(&mut self, context: &egui::Context) {
        self.refinement.finish_worker();
        if self.refinement.open
            && !matches!(self.refinement.phase, Phase::Saving(_))
            && let Some(capture) = &self.refinement.capture
        {
            let changed = self
                .workspace
                .as_ref()
                .is_none_or(|workspace| !capture.matches(workspace));
            if changed || capture.interrupted() {
                self.refinement.cancel.store(true, Ordering::Release);
                self.refinement.phase = Phase::Failed(if changed {
                    "The project, transcript or corrections changed. Nothing was accepted; request refinement again.".into()
                } else {
                    "System sleep interrupted local refinement. Nothing was accepted; request it again.".into()
                });
                self.refinement.events = None;
            }
        }
        let event = self
            .refinement
            .events
            .as_ref()
            .map(|receiver| receiver.try_recv());
        if matches!(event, Some(Err(mpsc::TryRecvError::Disconnected)))
            && matches!(self.refinement.phase, Phase::Running)
        {
            self.refinement.phase = Phase::Failed(
                "The local refinement worker ended without a result. Nothing was accepted.".into(),
            );
            self.refinement.events = None;
        }
        if let Some(Ok(result)) = event {
            self.refinement.events = None;
            if self.refinement.open && matches!(self.refinement.phase, Phase::Running) {
                self.refinement.phase = match result {
                    Ok(prepared) => Phase::Ready(Arc::new(prepared)),
                    Err(worker::Failure::NeedsModel) => Phase::NeedsModel,
                    Err(worker::Failure::Message(error)) => Phase::Failed(error),
                };
            }
        }
        if self.refinement.open && self.refinement.thread.is_some() {
            context.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn close_refinement(&mut self, context: &egui::Context) {
        if matches!(self.refinement.phase, Phase::Saving(_)) {
            return;
        }
        self.refinement.cancel.store(true, Ordering::Release);
        self.refinement.open = false;
        self.refinement.capture = None;
        self.refinement.events = None;
        let owns_audition = self
            .transport
            .as_ref()
            .is_some_and(|run| matches!(run.domain(), crate::transport::Domain::AudioRange(_)))
            || self.resume.as_ref().is_some_and(|resume| {
                matches!(resume.domain(), crate::transport::Domain::AudioRange(_))
            });
        if owns_audition {
            self.stop_playback();
        }
        self.restore_correction_focus();
        context.request_repaint();
    }

    pub(super) fn receive_refinement_save(
        &mut self,
        save: Option<&crate::project::TranscriptSave>,
    ) {
        let Some(save) = save else {
            return;
        };
        let Some(capture) = &self.refinement.capture else {
            return;
        };
        if !matches!(self.refinement.phase, Phase::Saving(attempt) if attempt == save.attempt)
            || capture.workspace.session != save.session
        {
            return;
        }
        match &save.error {
            Some(error) => self.refinement.phase = Phase::Failed(error.clone()),
            None => {
                self.refinement.open = false;
                self.refinement.capture = None;
                self.finish_refinement_correction();
            }
        }
    }

    fn accept_refinement(&mut self) {
        let (Some(capture), Phase::Ready(prepared)) =
            (&self.refinement.capture, &self.refinement.phase)
        else {
            return;
        };
        if prepared
            .review
            .offers()
            .iter()
            .any(|offer| offer.availability != RefinementOfferAvailability::Approximate)
        {
            return;
        }
        self.transcription.correction_attempts += 1;
        let attempt = self.transcription.correction_attempts;
        let request = crate::project::refinement::Request {
            expected_session: capture.workspace.session,
            expected_revision: capture.workspace.document.revision_id().clone(),
            attempt,
            lifecycle_epoch: capture.epoch,
            transcript: capture.transcript.clone(),
            corrections: capture.corrections.clone(),
            review: prepared.review.clone(),
            selected_words: vec![capture.word],
        };
        if self.submit(ProjectRequest::AcceptRefinement(request)) {
            self.refinement.phase = Phase::Saving(attempt);
            self.stop_playback();
        } else {
            self.refinement.phase =
                Phase::Failed(self.error.take().unwrap_or_else(|| {
                    "The project service is busy. Nothing was accepted.".into()
                }));
        }
    }
}

#[cfg(feature = "ui-harness")]
impl State {
    pub(super) fn status(&self) -> &'static str {
        if !self.open {
            return "closed";
        }
        match self.phase {
            Phase::Running => "running",
            Phase::Ready(_) => "ready",
            Phase::NeedsModel => "needs_model",
            Phase::Failed(_) => "failed",
            Phase::Saving(_) => "saving",
        }
    }
    pub(super) fn error(&self) -> Option<&str> {
        if let Phase::Failed(error) = &self.phase {
            Some(error)
        } else {
            None
        }
    }
    pub(super) fn review(&self) -> Option<&RefinementReview> {
        if let Phase::Ready(prepared) = &self.phase {
            Some(&prepared.review)
        } else {
            None
        }
    }
    pub(super) fn engine(&self) -> Option<&str> {
        if let Phase::Ready(prepared) = &self.phase {
            Some(&prepared.engine)
        } else {
            None
        }
    }
    pub(super) fn drained(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(|thread| thread.is_finished())
    }
    pub(super) fn audition_cursor(&self) -> u64 {
        self.audition_cursor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refinement_preflight_requires_explicit_english_before_model_or_worker_access() {
        assert!(language_problem("en").is_none());
        for language in ["fr", "auto", "en-US", "EN", ""] {
            let problem = language_problem(language).unwrap();
            assert!(problem.contains("English") && problem.contains("only"));
        }
    }

    #[test]
    fn a_panicked_worker_settles_running_instead_of_spinning_forever() {
        let thread = std::thread::spawn(|| panic!("intentional refinement worker fault witness"));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !thread.is_finished() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let mut state = State {
            open: true,
            phase: Phase::Running,
            thread: Some(thread),
            ..Default::default()
        };
        state.finish_worker();
        assert!(matches!(state.phase, Phase::Failed(_)));
        assert!(state.thread.is_none());
        assert!(state.events.is_none());
    }
}
