//! The Original's transcript in the rail.
//!
//! When the Original is ready and the approved transcription pack is
//! installed, a background job prepares analysis PCM from a read-only store,
//! runs the isolated worker and hands the validated transcript to the project
//! service, which saves it outside history. Installing the pack is an explicit
//! action that shows its size and license first. The rail shows the sentences
//! around the Original cursor, marks the current word and approximate words,
//! and moves the Original cursor to a clicked word or search match.

use std::ops::Range;
use std::sync::mpsc;

use deadpan_analysis::{Transcript, picture_at, picture_seconds};
use deadpan_cli::transcription::transcribe;
use deadpan_core::ProjectFrame;
use deadpan_jobs::transcription::Language;
use deadpan_models::packs::{InstallProgress, Operation, PackManifest, PackStore, approved_packs};
use deadpan_store::TranscriptKey;

use super::*;

/// Sentences shown on each side of the current one.
const CONTEXT_SEGMENTS: usize = 4;

enum Event {
    Installing(InstallProgress),
    Installed,
    InstallFailed(String),
    InstallCancelled,
    Progress(u8),
    /// Speech activity of the same PCM; `last` when no transcript follows.
    Detected {
        result: Result<
            (
                deadpan_store::SpeechActivityKey,
                deadpan_analysis::SpeechActivity,
            ),
            String,
        >,
        last: bool,
    },
    Transcribed(Result<(TranscriptKey, Transcript), String>),
}

mod activity;

#[derive(Debug, Clone, PartialEq)]
enum Status {
    /// Not yet checked for this project session.
    Unchecked,
    NeedsModel,
    Installing {
        completed: u64,
        total: u64,
    },
    Preparing,
    Transcribing(u8),
    /// Waiting for the service to store this attempt.
    Saving(u64),
    Failed(String),
    Ready,
}

pub(super) struct Transcription {
    session: Option<u64>,
    status: Status,
    events: Option<mpsc::Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    search: String,
    matches: Vec<Range<usize>>,
    current_match: Option<usize>,
    /// Model storage; None is the global Application Support directory.
    models_root: Option<std::path::PathBuf>,
    /// A finished transcript not yet admitted by a busy project service.
    unsaved: Option<(TranscriptKey, Arc<Transcript>)>,
    attempts: u64,
    /// A word chosen by click or search and the Original picture it moved to.
    /// While the cursor stays there, that word is current even when a later
    /// word also begins in the same picture.
    chosen: Option<(usize, u64)>,
    /// Speech on the Edit clock for one session, revision and analysis.
    edit_speech: Option<(
        (u64, deadpan_core::RevisionId, SpeechKey),
        Arc<deadpan_core::SpeechTimeline>,
    )>,
    /// Speech over the Original's pictures for one analysis.
    source_speech: Option<((u64, SpeechKey), Arc<deadpan_core::SpeechTimeline>)>,
    /// Enter (true) or Shift+Enter (false) pressed in Find words this frame.
    step: Option<bool>,
    /// The background thread, joined briefly at exit so its worker is reaped.
    thread: Option<std::thread::JoinHandle<()>>,
    /// Speech activity detection, which shares the job and its PCM.
    activity: activity::ActivityJob,
}

impl Default for Transcription {
    fn default() -> Self {
        Self {
            session: None,
            status: Status::Unchecked,
            events: None,
            cancel: Arc::new(AtomicBool::new(false)),
            search: String::new(),
            matches: Vec::new(),
            current_match: None,
            models_root: None,
            unsaved: None,
            chosen: None,
            edit_speech: None,
            source_speech: None,
            step: None,
            attempts: 0,
            thread: None,
            activity: activity::ActivityJob::default(),
        }
    }
}

impl Transcription {
    /// A new project session cancels its transcription. A model install
    /// belongs to every project, so it continues and reports here.
    fn reset(&mut self, session: Option<u64>) {
        let installing = matches!(self.status, Status::Installing { .. });
        if !installing {
            self.cancel.store(true, Ordering::Release);
        }
        let previous = std::mem::take(self);
        *self = Self {
            session,
            models_root: previous.models_root,
            attempts: previous.attempts,
            ..Self::default()
        };
        if installing {
            self.status = previous.status;
            self.events = previous.events;
            self.cancel = previous.cancel;
            self.thread = previous.thread;
        }
    }

    pub(super) fn receive_save(&mut self, save: Option<crate::project::TranscriptSave>) {
        let Some(save) = save else {
            return;
        };
        if self.session != Some(save.session) || self.status != Status::Saving(save.attempt) {
            return;
        }
        if let Some(error) = save.error {
            self.status = Status::Failed(format!("the transcript was not saved: {error}"));
        }
    }

    /// Use a private model directory, for example an empty one in replay.
    #[cfg(feature = "ui-harness")]
    pub(super) fn set_models_root(&mut self, root: std::path::PathBuf) {
        self.models_root = Some(root);
    }

    fn pack_store(&self) -> Option<PackStore> {
        self.models_root
            .clone()
            .or_else(|| deadpan_cli::models::default_root().ok())
            .map(PackStore::new)
    }

    /// Cancel and wait briefly, so the supervised worker is stopped and its
    /// scratch directory removed before the process exits.
    pub(super) fn shutdown(&mut self) {
        self.cancel.store(true, Ordering::Release);
        let Some(thread) = self.thread.take() else {
            return;
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !thread.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if thread.is_finished() {
            let _ = thread.join();
        }
    }

    pub(super) fn request_step(&mut self, forward: bool) {
        self.step = Some(forward);
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn search_text(&self) -> &str {
        &self.search
    }

    /// Replay-visible status name.
    #[cfg(feature = "ui-harness")]
    pub(super) fn status_name(&self) -> &'static str {
        match self.status {
            Status::Unchecked => "unchecked",
            Status::NeedsModel => "needs_model",
            Status::Installing { .. } => "installing",
            Status::Preparing => "preparing",
            Status::Transcribing(_) => "transcribing",
            Status::Saving(_) => "saving",
            Status::Failed(_) => "failed",
            Status::Ready => "ready",
        }
    }
}

fn transcription_pack() -> Option<PackManifest> {
    approved_packs()
        .into_iter()
        .find(|pack| pack.operations.contains(&Operation::Transcribe))
}

impl DeadpanApp {
    /// Advance transcript jobs once per outer frame.
    pub(super) fn reconcile_transcription(&mut self, context: &egui::Context) {
        let session = self.workspace.as_ref().map(|workspace| workspace.session);
        if self.transcription.session != session {
            self.transcription.reset(session);
        }
        while let Some(event) = self
            .transcription
            .events
            .as_ref()
            .and_then(|events| events.try_recv().ok())
        {
            match event {
                Event::Installing(progress) => {
                    self.transcription.status = Status::Installing {
                        completed: progress.completed_bytes,
                        total: progress.total_bytes,
                    };
                }
                Event::Installed => {
                    self.transcription.events = None;
                    self.transcription.status = Status::Unchecked;
                }
                Event::InstallCancelled => {
                    self.transcription.events = None;
                    self.transcription.status = Status::NeedsModel;
                }
                Event::InstallFailed(error) => {
                    self.transcription.events = None;
                    self.transcription.status = Status::Failed(error);
                }
                Event::Progress(percent) => {
                    self.transcription.status = Status::Transcribing(percent);
                }
                Event::Detected { result, last } => {
                    if last {
                        self.transcription.events = None;
                    }
                    self.transcription.activity.receive(result);
                }
                Event::Transcribed(result) => {
                    self.transcription.events = None;
                    match result {
                        Ok((key, transcript)) => {
                            self.transcription.unsaved = Some((key, Arc::new(transcript)));
                        }
                        Err(error) => self.transcription.status = Status::Failed(error),
                    }
                }
            }
            context.request_repaint();
        }
        self.save_transcript(session, context);
        self.save_activity(session, context);
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        if workspace.transcript.is_some() {
            // Projects transcribed before pause detection get it on its own.
            self.reconcile_activity(context);
            self.transcription.status = Status::Ready;
            return;
        }
        if !matches!(
            workspace.single_source,
            Some(SingleSourceState::Ready { .. })
        ) {
            return;
        }
        if self.transcription.status == Status::Ready {
            // A newly opened workspace without a transcript re-checks.
            self.transcription.status = Status::Unchecked;
        }
        if self.transcription.status != Status::Unchecked || self.transcription.events.is_some() {
            return;
        }
        let (Some(pack), Some(store)) = (transcription_pack(), self.transcription.pack_store())
        else {
            self.transcription.status = Status::Failed("Model storage is unavailable.".into());
            return;
        };
        match store.installed(&pack) {
            Ok(Some(installed)) => self.start_transcription(context, &pack, &installed),
            Ok(None) => self.transcription.status = Status::NeedsModel,
            Err(error) => self.transcription.status = Status::Failed(error.to_string()),
        }
    }

    /// Submit a finished transcript without the side effects of a user
    /// command: playback, repeats and pending keys are left alone. A busy
    /// service is retried on a later frame.
    fn save_transcript(&mut self, session: Option<u64>, context: &egui::Context) {
        let Some(session) = session else {
            self.transcription.unsaved = None;
            return;
        };
        let Some((key, transcript)) = self.transcription.unsaved.as_ref() else {
            return;
        };
        if self.service.is_busy() {
            context.request_repaint_after(Duration::from_millis(100));
            return;
        }
        self.transcription.attempts += 1;
        let attempt = self.transcription.attempts;
        let request = ProjectRequest::SaveTranscript {
            expected_session: session,
            attempt,
            key: key.clone(),
            transcript: Arc::clone(transcript),
        };
        match self.service.submit(request) {
            Ok(()) => {
                self.transcription.unsaved = None;
                self.transcription.status = Status::Saving(attempt);
            }
            // Lost a race with a user command; try again shortly.
            Err(_) if self.service.is_busy() => {
                context.request_repaint_after(Duration::from_millis(100));
            }
            Err(error) => {
                self.transcription.unsaved = None;
                self.transcription.status =
                    Status::Failed(format!("the transcript was not saved: {error}"));
            }
        }
    }

    fn start_transcription(
        &mut self,
        context: &egui::Context,
        pack: &PackManifest,
        installed: &deadpan_models::packs::InstalledPack,
    ) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let file = &pack.files[0];
        let Some(path) = installed.file(&file.name) else {
            return;
        };
        let model = match deadpan_jobs::Sha256::new(file.sha256.clone()) {
            Ok(sha256) => deadpan_jobs::transcription::ModelInput {
                path,
                sha256,
                byte_length: file.bytes,
            },
            Err(error) => {
                self.transcription.status = Status::Failed(error.to_string());
                return;
            }
        };
        // Detect speech on the same PCM unless the Original already has it.
        let vad = workspace
            .speech_activity
            .is_none()
            .then(|| activity::vad_model(pack, installed))
            .flatten();
        let package = workspace.path.clone();
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.transcription.cancel = Arc::clone(&cancel);
        self.transcription.events = Some(receiver);
        self.transcription.status = Status::Preparing;
        if vad.is_some() {
            self.transcription.activity.start();
        }
        let repaint = context.clone();
        let spawned = std::thread::Builder::new()
            .name("deadpan-transcription".into())
            .spawn(move || {
                let detected = |result| {
                    let _ = sender.send(Event::Detected {
                        result,
                        last: false,
                    });
                    repaint.request_repaint();
                };
                let result = run_transcription(
                    &package,
                    &model,
                    vad.as_ref(),
                    &cancel,
                    detected,
                    |percent| {
                        let _ = sender.send(Event::Progress(percent));
                        repaint.request_repaint();
                    },
                );
                let _ = sender.send(Event::Transcribed(result));
                repaint.request_repaint();
            });
        match spawned {
            Ok(thread) => self.transcription.thread = Some(thread),
            Err(error) => {
                self.transcription.events = None;
                self.transcription.status = Status::Failed(error.to_string());
            }
        }
    }

    fn install_transcription_model(&mut self, context: &egui::Context) {
        let (Some(pack), Some(store)) = (transcription_pack(), self.transcription.pack_store())
        else {
            self.transcription.status = Status::Failed("Model storage is unavailable.".into());
            return;
        };
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.transcription.cancel = Arc::clone(&cancel);
        self.transcription.events = Some(receiver);
        self.transcription.status = Status::Installing {
            completed: 0,
            total: pack.total_bytes(),
        };
        let repaint = context.clone();
        let spawned = std::thread::Builder::new()
            .name("deadpan-model-install".into())
            .spawn(move || {
                let progress_sender = sender.clone();
                let progress_repaint = repaint.clone();
                let result =
                    deadpan_cli::models::install_pack(&store, &pack, &cancel, |progress| {
                        let _ = progress_sender.send(Event::Installing(progress));
                        progress_repaint.request_repaint();
                    });
                let _ = sender.send(match result {
                    Ok(_) => Event::Installed,
                    Err(_) if cancel.load(Ordering::Acquire) => Event::InstallCancelled,
                    Err(error) => Event::InstallFailed(error.to_string()),
                });
                repaint.request_repaint();
            });
        match spawned {
            Ok(thread) => self.transcription.thread = Some(thread),
            Err(error) => {
                self.transcription.events = None;
                self.transcription.status = Status::Failed(error.to_string());
            }
        }
    }

    fn cancel_transcription_job(&mut self) {
        self.transcription.cancel.store(true, Ordering::Release);
    }

    /// Move the Original cursor to the picture presented when a word begins.
    fn jump_to_word(&mut self, word: usize) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let Some(transcript) = workspace.transcript.clone() else {
            return;
        };
        let Some(asset) = original_asset(workspace).cloned() else {
            return;
        };
        let Some(start) = transcript
            .transcript
            .words()
            .get(word)
            .map(|word| word.start_cs)
        else {
            return;
        };
        let frame = workspace.sources.get(&asset).and_then(|source| {
            let video = source.receipt.snapshot().video()?;
            let index = video.index().index();
            let seconds = transcript.transcript.seconds(start).ok()?;
            // Speech can begin before the first picture; show that picture.
            picture_at(index, seconds).or_else(|| {
                picture_seconds(index, 0)
                    .filter(|first| seconds.compare(*first).is_lt())
                    .map(|_| 0)
            })
        });
        let Some(frame) = frame else {
            self.message = Some("That word is outside the Original picture.".into());
            return;
        };
        self.select_source(asset);
        self.source_cursor = frame as u64;
        self.transcription.chosen = Some((word, self.source_cursor));
        self.request_picture(false);
    }

    /// The latest word begun by the end of the Original cursor's picture, so
    /// the picture shown when a word starts marks that word and pauses keep
    /// the previous word in view.
    pub(super) fn current_word(&self) -> Option<usize> {
        let workspace = self.workspace.as_ref()?;
        let transcript = workspace.transcript.as_ref()?;
        if let Some((word, frame)) = self.transcription.chosen
            && frame == self.source_cursor
            && word < transcript.transcript.words().len()
        {
            return Some(word);
        }
        let asset = original_asset(workspace)?;
        let index = workspace
            .sources
            .get(asset)?
            .receipt
            .snapshot()
            .video()?
            .index()
            .index();
        let frame = self.source_cursor as usize;
        index.frames().get(frame)?;
        let end = picture_seconds(index, frame + 1).or_else(|| {
            let base = index.time_base();
            deadpan_core::ExactRatio::new(
                i128::from(index.terminal_end()) * i128::from(base.numerator()),
                i128::from(base.denominator()),
            )
            .ok()
        })?;
        // Exact rational comparison; rounding to centiseconds would miss a
        // word that begins inside the picture's last centisecond.
        transcript
            .transcript
            .words()
            .partition_point(|word| {
                transcript
                    .transcript
                    .seconds(word.start_cs)
                    .is_ok_and(|start| start.compare(end).is_lt())
            })
            .checked_sub(1)
    }

    fn update_transcript_search(&mut self) {
        let matches = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.transcript.as_ref())
            .map_or_else(Vec::new, |transcript| {
                transcript.transcript.search(&self.transcription.search)
            });
        self.transcription.current_match = None;
        self.transcription.matches = matches;
    }

    /// Whether the rail shows a searchable transcript.
    pub(super) fn transcription_ready(&self) -> bool {
        self.transcription.status == Status::Ready
            && self
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.transcript.as_ref())
                .is_some_and(|transcript| !transcript.transcript.words().is_empty())
    }

    /// `n` / `N` and Enter in Find words: the next or previous match after
    /// the cursor in the current context, wrapping at its ends. Your edit
    /// steps through occurrences of matching words in the arrangement.
    pub(super) fn search_step(&mut self, forward: bool) {
        if self.transcription.matches.is_empty() {
            self.message = Some(if self.transcription.search.trim().is_empty() {
                format!(
                    "Search the transcript with {} first.",
                    self.editor_key(EditorKey::Search)
                )
            } else {
                "No transcript words match the search.".into()
            });
            return;
        }
        let edit = self.view == View::Sequence;
        if !edit && !self.viewing_original() {
            self.message = Some(
                "Transcript matches are in the Original; view the Original or Your edit.".into(),
            );
            return;
        }
        let speech = if edit {
            self.edit_speech()
        } else {
            self.source_speech()
        };
        let speech = match speech {
            Ok(speech) => speech,
            Err(error) => {
                self.message = Some(error);
                return;
            }
        };
        let (scope, cursor) = if edit {
            (
                self.scope_start as i64..self.scope_end as i64,
                self.sequence_cursor as i64,
            )
        } else {
            (0..self.source_length() as i64, self.source_cursor as i64)
        };
        // Each occurrence: its first frame and the match it belongs to.
        let occurrences: Vec<(i64, usize)> = speech
            .runs()
            .iter()
            .filter(|run| scope.contains(&run.range.start().0))
            .filter_map(|run| {
                let matched = self
                    .transcription
                    .matches
                    .iter()
                    .position(|range| range.start == run.word as usize)?;
                Some((run.range.start().0, matched))
            })
            .collect();
        let next = if forward {
            occurrences
                .iter()
                .find(|(start, _)| *start > cursor)
                .or(occurrences.first())
        } else {
            occurrences
                .iter()
                .rev()
                .find(|(start, _)| *start < cursor)
                .or(occurrences.last())
        };
        let Some((next, matched)) = next.copied() else {
            self.message = Some(if edit {
                "No match remains in Your edit.".into()
            } else {
                "No match lies on an Original picture.".into()
            });
            return;
        };
        let wrapped = if forward {
            next <= cursor
        } else {
            next >= cursor
        };
        self.transcription.current_match = Some(matched);
        if edit {
            self.sequence_cursor = next as u64;
            self.edit_range.move_to(self.sequence_cursor);
            self.select_at_cursor();
        } else {
            self.source_cursor = next as u64;
            self.moment.move_to(self.source_cursor);
            let word = self.transcription.matches[matched].start;
            self.transcription.chosen = Some((word, self.source_cursor));
        }
        self.request_picture(false);
        let position = occurrences
            .iter()
            .position(|(start, _)| *start == next)
            .unwrap_or(0);
        self.message = Some(format!(
            "Match {} of {} in {}{}.",
            position + 1,
            occurrences.len(),
            if edit { "Your edit" } else { "the Original" },
            if wrapped { ", wrapped" } else { "" }
        ));
    }

    /// The Original is the viewed source.
    fn viewing_original(&self) -> bool {
        let original = self
            .workspace
            .as_ref()
            .and_then(|workspace| original_asset(workspace))
            .cloned();
        self.raw_source.is_none() && original.is_some() && self.selected_source == original
    }

    /// The TRANSCRIPT rail section.
    pub(super) fn transcript_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.label(style::section_title("TRANSCRIPT", false));
        match self.transcription.status.clone() {
            Status::Unchecked | Status::Preparing => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.weak("Preparing the Original’s audio…");
                });
            }
            Status::NeedsModel => {
                let size = transcription_pack().map_or(0, |pack| pack.total_bytes());
                ui.weak(format!(
                    "Transcription runs on this Mac. It needs a {} MB English model (MIT license), downloaded once.",
                    size.div_ceil(1_000_000)
                ));
                if ui
                    .add(style::row_action(ui, "Install model…", ""))
                    .on_hover_text("Download, verify and install the whisper base.en model. Projects keep working offline afterwards.")
                    .clicked()
                {
                    self.install_transcription_model(ui.ctx());
                }
            }
            Status::Installing { completed, total } => {
                ui.add(
                    egui::ProgressBar::new(completed as f32 / total.max(1) as f32)
                        .desired_height(6.0)
                        .text(format!(
                            "Installing model · {} / {} MB",
                            completed / 1_000_000,
                            total / 1_000_000
                        )),
                );
                if ui
                    .add(style::row_action(ui, "Cancel install", ""))
                    .clicked()
                {
                    self.cancel_transcription_job();
                }
            }
            Status::Transcribing(percent) => {
                ui.add(
                    egui::ProgressBar::new(f32::from(percent) / 100.0)
                        .desired_height(6.0)
                        .text(format!("Transcribing · {percent}%")),
                );
            }
            Status::Saving(_) => {
                ui.weak("Saving transcript…");
            }
            Status::Failed(error) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Transcription failed: {error}"),
                );
                if ui.add(style::row_action(ui, "Try again", "")).clicked() {
                    self.transcription.status = Status::Unchecked;
                }
            }
            Status::Ready => self.transcript_words(ui),
        }
        self.activity_section(ui);
    }

    fn transcript_words(&mut self, ui: &mut egui::Ui) {
        let Some(transcript) = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.transcript.clone())
        else {
            return;
        };
        let words = transcript.transcript.words();
        if words.is_empty() {
            ui.weak("No speech was recognized in the Original.");
            return;
        }
        let search = ui.add(
            egui::TextEdit::singleline(&mut self.transcription.search)
                .id(egui::Id::new(TRANSCRIPT_SEARCH_ID))
                .hint_text("Find words")
                .desired_width(f32::INFINITY),
        );
        if search.changed() {
            self.update_transcript_search();
        }
        if let Some(forward) = self.transcription.step.take() {
            self.search_step(forward);
        }
        if !self.transcription.search.trim().is_empty() {
            let count = self.transcription.matches.len();
            let keys = self.editor_pair(EditorKey::SearchNext, EditorKey::SearchPrevious, " / ");
            ui.weak(match (count, self.transcription.current_match) {
                (0, _) => "No matches".to_owned(),
                (count, Some(index)) => format!("{} of {count} · Enter or {keys}", index + 1),
                (count, None) => format!("{count} matches · Enter goes to the next"),
            });
        }
        if self.view == View::Sequence
            && let Ok(speech) = self.edit_speech()
        {
            self.edit_words(ui, &transcript, &speech);
            return;
        }
        let current = self.current_word();
        let focus = self
            .transcription
            .current_match
            .and_then(|index| self.transcription.matches.get(index))
            .map(|range| range.start)
            .or(current)
            .unwrap_or(0);
        let segment = words[focus].segment;
        let first = segment.saturating_sub(CONTEXT_SEGMENTS as u32);
        let last = segment + CONTEXT_SEGMENTS as u32;
        let start = words.partition_point(|word| word.segment < first);
        let end = words.partition_point(|word| word.segment <= last);
        let highlighted: Vec<Range<usize>> = self.transcription.matches.clone();
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("transcript-words")
            .max_height(200.0)
            .show(ui, |ui| {
                let mut index = start;
                while index < end {
                    let segment = words[index].segment;
                    let line_end = index
                        + words[index..end]
                            .iter()
                            .take_while(|word| word.segment == segment)
                            .count();
                    let entries: Vec<Entry<'_>> = (index..line_end)
                        .map(|word| Entry {
                            text: &words[word].text,
                            approximate: words[word].approximate(),
                            current: Some(word) == current,
                            matched: highlighted.iter().any(|range| range.contains(&word)),
                        })
                        .collect();
                    if let Some(offset) = phrase(ui, &entries) {
                        clicked = Some(index + offset);
                    }
                    index = line_end;
                }
            });
        let approximate = words[start..end]
            .iter()
            .filter(|word| word.approximate())
            .count();
        let mut caption = format!("{} words · on this Mac", words.len());
        if approximate > 0 {
            caption.push_str(&format!(" · {approximate} approximate in grey"));
        }
        ui.label(egui::RichText::new(caption).size(11.0).color(style::MUTED));
        if let Some(word) = clicked {
            self.jump_to_word(word);
        }
    }
}

impl DeadpanApp {
    /// Your edit's words around the Edit cursor, in arrangement order: cut
    /// words are absent and repeated words appear at each play. Clicking a
    /// word moves the Edit cursor to where that occurrence begins.
    fn edit_words(
        &mut self,
        ui: &mut egui::Ui,
        transcript: &crate::project::OriginalTranscript,
        speech: &deadpan_core::SpeechTimeline,
    ) {
        let words = transcript.transcript.words();
        let runs = speech.runs();
        if runs.is_empty() {
            ui.weak("Your edit shows no recognized speech.");
            return;
        }
        // Sentence occurrences: consecutive runs of one segment whose words advance.
        let mut sentences: Vec<Range<usize>> = Vec::new();
        for (index, run) in runs.iter().enumerate() {
            match sentences.last_mut() {
                Some(range)
                    if runs[range.end - 1].sentence == run.sentence
                        && runs[range.end - 1].word < run.word =>
                {
                    range.end = index + 1;
                }
                _ => sentences.push(index..index + 1),
            }
        }
        let cursor = self.sequence_cursor as i64;
        let current = runs
            .iter()
            .position(|run| run.range.start().0 <= cursor && cursor < run.range.end().0);
        let near = runs
            .partition_point(|run| run.range.end().0 <= cursor)
            .min(runs.len() - 1);
        let focus = sentences
            .iter()
            .position(|range| range.contains(&current.unwrap_or(near)))
            .unwrap_or(0);
        let shown = focus.saturating_sub(CONTEXT_SEGMENTS)
            ..(focus + CONTEXT_SEGMENTS + 1).min(sentences.len());
        let matches = self.transcription.matches.clone();
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("transcript-edit-words")
            .max_height(200.0)
            .show(ui, |ui| {
                for sentence in &sentences[shown] {
                    let entries: Vec<Entry<'_>> = sentence
                        .clone()
                        .map(|index| {
                            let word = &words[runs[index].word as usize];
                            Entry {
                                text: &word.text,
                                approximate: word.approximate(),
                                current: Some(index) == current,
                                matched: matches
                                    .iter()
                                    .any(|range| range.contains(&(runs[index].word as usize))),
                            }
                        })
                        .collect();
                    if let Some(offset) = phrase(ui, &entries) {
                        clicked = Some(sentence.start + offset);
                    }
                }
            });
        let kept: std::collections::BTreeSet<u32> = runs.iter().map(|run| run.word).collect();
        ui.label(
            egui::RichText::new(format!(
                "Your edit · {} of {} words kept · {} plays",
                kept.len(),
                words.len(),
                runs.len()
            ))
            .size(11.0)
            .color(style::MUTED),
        );
        if let Some(index) = clicked {
            self.sequence_cursor = runs[index].range.start().0 as u64;
            self.edit_range.move_to(self.sequence_cursor);
            self.select_at_cursor();
            self.request_picture(false);
        }
    }
}

/// Prepare analysis PCM from a read-only view of the project and run the
/// worker installed beside the application.
/// Prepare the Original's analysis PCM once, detect speech in it when a
/// detector model is given (reported through `detected` exactly once), then
/// transcribe it.
fn run_transcription(
    package: &std::path::Path,
    model: &deadpan_jobs::transcription::ModelInput,
    vad: Option<&deadpan_jobs::transcription::ModelInput>,
    cancel: &AtomicBool,
    detected: impl FnOnce(activity::Detection),
    progress: impl FnMut(u8),
) -> Result<(TranscriptKey, Transcript), String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(6 * 60 * 60);
    let prepared = activity::prepare(package, cancel, deadline);
    let (analysis, runtime) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            if vad.is_some() {
                detected(Err(error.clone()));
            }
            return Err(error);
        }
    };
    if let Some(vad) = vad {
        detected(activity::detect(&runtime, vad, &analysis, cancel, deadline));
    }
    let language = Language::Code("en".into());
    let label: String = language.clone().into();
    let attempt = uuid::Uuid::new_v4().simple().to_string();
    let result = transcribe(
        &runtime,
        model,
        &analysis.input,
        language,
        &attempt,
        cancel,
        deadline,
        progress,
    )
    .map_err(|e| e.to_string())?;
    Ok((
        TranscriptKey {
            content: analysis.content,
            audio_stream: analysis.audio_stream,
            model_sha256: model.sha256.as_str().to_owned(),
            language: label,
            engine: result.runtime.engine,
        },
        result.transcript,
    ))
}

/// One word shown in the rail.
struct Entry<'a> {
    text: &'a str,
    approximate: bool,
    current: bool,
    matched: bool,
}

/// One sentence as a single wrapped text run. Returns the clicked entry.
fn phrase(ui: &mut egui::Ui, entries: &[Entry<'_>]) -> Option<usize> {
    let mut job = egui::text::LayoutJob::default();
    let mut spans = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut format = egui::TextFormat {
            font_id: egui::FontId::proportional(12.5),
            color: if entry.approximate {
                style::MUTED
            } else {
                style::TEXT
            },
            italics: entry.approximate,
            ..Default::default()
        };
        if entry.current {
            format.background = style::SELECTED;
            format.color = style::LAVENDER;
        }
        if entry.matched {
            format.underline = egui::Stroke::new(1.5, style::CURSOR);
        }
        let begin = job.text.chars().count();
        job.append(entry.text, 0.0, format.clone());
        spans.push(begin..job.text.chars().count());
        job.append(
            " ",
            0.0,
            egui::TextFormat {
                background: egui::Color32::TRANSPARENT,
                ..format
            },
        );
    }
    job.wrap.max_width = ui.available_width();
    let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
    let text = galley.text().to_owned();
    let response = ui.add(egui::Label::new(Arc::clone(&galley)).sense(egui::Sense::click()));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &text));
    let position = response
        .interact_pointer_pos()
        .filter(|_| response.clicked())?;
    let offset = galley.cursor_from_pos(position - response.rect.min).index.0;
    spans.iter().position(|chars| chars.contains(&offset))
}

/// Words on the Edit clock and in the Original, for motions and objects.
impl DeadpanApp {
    /// Why word keys cannot act yet, in terms of the transcript's progress.
    fn words_not_ready(&self) -> String {
        match &self.transcription.status {
            Status::NeedsModel => {
                "Words are not ready. Install the transcription model in the Original rail.".into()
            }
            Status::Installing { .. } => "Words are not ready while the model installs.".into(),
            Status::Unchecked | Status::Preparing | Status::Transcribing(_) | Status::Saving(_) => {
                "Words are not ready while the Original is transcribed.".into()
            }
            Status::Failed(_) => {
                "Words are not ready: transcription failed. Try again in the Original rail.".into()
            }
            Status::Ready => "Words are not ready: the Original has no transcript.".into(),
        }
    }

    /// Why pause keys cannot act yet. Pauses are detected with the transcript.
    fn pauses_not_ready(&self) -> String {
        if let Some(reason) = self.transcription.activity.not_ready() {
            return reason;
        }
        match &self.transcription.status {
            Status::NeedsModel => {
                "Pauses are not ready. Install the transcription model in the Original rail.".into()
            }
            Status::Installing { .. } => "Pauses are not ready while the model installs.".into(),
            Status::Unchecked | Status::Preparing | Status::Transcribing(_) | Status::Saving(_) => {
                "Pauses are not ready while the Original's speech is analysed.".into()
            }
            Status::Failed(_) => {
                "Pauses are not ready: analysis failed. Try again in the Original rail.".into()
            }
            Status::Ready => {
                "Pauses are not ready: the Original's speech has not been analysed.".into()
            }
        }
    }

    /// Why shot keys cannot act yet.
    fn shots_not_ready(&self) -> String {
        "Shots are not ready: the Original's pictures have not been analysed yet.".into()
    }

    /// The Original's analyses, asset and picture index, when the Original
    /// has a qualified picture.
    fn speech_inputs(&self) -> Option<(SpeechInputs, deadpan_core::AssetId, Arc<Workspace>)> {
        let workspace = self.workspace.as_ref()?;
        let asset = original_asset(workspace)?.clone();
        workspace.sources.get(&asset)?.video_index.as_ref()?;
        let inputs = SpeechInputs {
            transcript: workspace.transcript.clone(),
            activity: workspace.speech_activity.clone(),
            shots: workspace.shot_analysis.clone(),
        };
        Some((inputs, asset, Arc::clone(workspace)))
    }

    /// A timeline from whichever analyses exist, each missing one explained.
    fn speech_timeline(
        &self,
        inputs: &SpeechInputs,
        words: impl FnOnce(
            &deadpan_analysis::Transcript,
        ) -> Result<deadpan_core::SpeechTimeline, deadpan_core::EditError>,
        pauses: impl FnOnce(
            &[(deadpan_core::ExactRatio, deadpan_core::ExactRatio)],
        ) -> Result<Vec<deadpan_core::FrameRange>, deadpan_core::EditError>,
        shots: impl FnOnce(&[usize]) -> Result<Vec<deadpan_core::ShotRun>, deadpan_core::EditError>,
    ) -> Result<Arc<deadpan_core::SpeechTimeline>, String> {
        // A failure places only its own analysis; the others stay usable.
        let timeline = match &inputs.transcript {
            Some(transcript) => words(&transcript.transcript)
                .unwrap_or_else(|error| deadpan_core::SpeechTimeline::without_words(error.message)),
            None => deadpan_core::SpeechTimeline::without_words(self.words_not_ready()),
        };
        let timeline = match &inputs.activity {
            Some(activity) => {
                let projected = deadpan_cli::speech::pause_seconds(&activity.activity)
                    .and_then(|seconds| pauses(&seconds));
                match projected {
                    Ok(ranges) => timeline
                        .clone()
                        .with_pauses(ranges)
                        .unwrap_or_else(|error| timeline.without_pauses(error.message)),
                    Err(error) => timeline.without_pauses(error.message),
                }
            }
            None => timeline.without_pauses(self.pauses_not_ready()),
        };
        let timeline = match &inputs.shots {
            Some(analysis) => match shots(&analysis.analysis.boundaries()) {
                Ok(runs) => timeline
                    .clone()
                    .with_shots(runs)
                    .unwrap_or_else(|error| timeline.without_shots(error.message)),
                Err(error) => timeline.without_shots(error.message),
            },
            None => timeline.without_shots(self.shots_not_ready()),
        };
        Ok(Arc::new(timeline))
    }

    /// Speech projected through the current revision, cached per revision.
    pub(super) fn edit_analysis(&mut self) -> Result<Arc<deadpan_core::SpeechTimeline>, String> {
        let (inputs, asset, workspace) =
            self.speech_inputs().ok_or_else(|| self.words_not_ready())?;
        let key = (
            workspace.session,
            workspace.document.revision_id().clone(),
            self.speech_key(&inputs),
        );
        if let Some((cached, speech)) = &self.transcription.edit_speech
            && *cached == key
        {
            return Ok(Arc::clone(speech));
        }
        // The index bound to the project's asset identity.
        let index = workspace.sources[&asset]
            .video_index
            .as_deref()
            .expect("speech inputs checked the picture");
        let speech = self.speech_timeline(
            &inputs,
            |transcript| {
                deadpan_cli::speech::project_speech(&workspace.plan, &asset, index, transcript)
            },
            |pauses| deadpan_cli::speech::project_pauses(&workspace.plan, &asset, index, pauses),
            |boundaries| {
                deadpan_cli::speech::project_shots(&workspace.plan, &asset, index, boundaries)
            },
        )?;
        self.transcription.edit_speech = Some((key, Arc::clone(&speech)));
        Ok(speech)
    }

    /// Edit speech whose words are available.
    pub(super) fn edit_speech(&mut self) -> Result<Arc<deadpan_core::SpeechTimeline>, String> {
        let speech = self.edit_analysis()?;
        speech
            .require(deadpan_core::SpeechUnit::Word)
            .map_err(|error| error.message)?;
        Ok(speech)
    }

    /// Speech over the Original's pictures, cached per analysis.
    fn source_analysis(&mut self) -> Result<Arc<deadpan_core::SpeechTimeline>, String> {
        let (inputs, asset, workspace) =
            self.speech_inputs().ok_or_else(|| self.words_not_ready())?;
        let key = (workspace.session, self.speech_key(&inputs));
        if let Some((cached, speech)) = &self.transcription.source_speech
            && *cached == key
        {
            return Ok(Arc::clone(speech));
        }
        // The index bound to the project's asset identity.
        let index = workspace.sources[&asset]
            .video_index
            .as_deref()
            .expect("speech inputs checked the picture");
        let speech = self.speech_timeline(
            &inputs,
            |transcript| deadpan_cli::speech::original_speech(index, transcript),
            |pauses| deadpan_cli::speech::original_pauses(index, pauses),
            |boundaries| deadpan_cli::speech::original_shots(index.frames().len(), boundaries),
        )?;
        self.transcription.source_speech = Some((key, Arc::clone(&speech)));
        Ok(speech)
    }

    /// Original speech whose words are available.
    fn source_speech(&mut self) -> Result<Arc<deadpan_core::SpeechTimeline>, String> {
        let speech = self.source_analysis()?;
        speech
            .require(deadpan_core::SpeechUnit::Word)
            .map_err(|error| error.message)?;
        Ok(speech)
    }

    /// `]p`, `[p`, `]s` and `[s` in either context.
    pub(super) fn analysis_motion(
        &mut self,
        unit: deadpan_core::SpeechUnit,
        forward: bool,
        count: u32,
    ) {
        let count = count.max(1);
        let target = |speech: &deadpan_core::SpeechTimeline, cursor: u64, bounds: (u64, u64)| {
            let cursor = ProjectFrame(cursor as i64);
            let bounds = (ProjectFrame(bounds.0 as i64), ProjectFrame(bounds.1 as i64));
            match unit {
                deadpan_core::SpeechUnit::Shot => {
                    speech.shot_target(cursor, bounds, forward, count)
                }
                _ => speech.pause_target(cursor, bounds, forward, count),
            }
            .map(|frame| frame.0 as u64)
            .map_err(|error| error.message)
        };
        match self.view {
            View::Source => {
                if !self.viewing_original() {
                    self.message = Some(
                        "Pause and shot motions follow the Original's analysis; view the Original to use them."
                            .into(),
                    );
                    return;
                }
                let length = self.source_length();
                match self
                    .source_analysis()
                    .and_then(|speech| target(&speech, self.source_cursor, (0, length)))
                {
                    Ok(frame) => {
                        self.source_cursor = frame;
                        self.moment.move_to(self.source_cursor);
                    }
                    Err(error) => {
                        self.message = Some(error);
                        return;
                    }
                }
            }
            View::Sequence => {
                let cursor = self.sequence_cursor.clamp(self.scope_start, self.scope_end);
                let bounds = (self.scope_start, self.scope_end);
                match self
                    .edit_analysis()
                    .and_then(|speech| target(&speech, cursor, bounds))
                {
                    Ok(frame) => {
                        self.sequence_cursor = frame;
                        self.edit_range.move_to(self.sequence_cursor);
                        self.select_at_cursor();
                        if let Some(count) = std::num::NonZeroU32::new(count) {
                            self.record_macro_local(match unit {
                                deadpan_core::SpeechUnit::Shot => {
                                    deadpan_core::SemanticInstruction::MoveShots { forward, count }
                                }
                                _ => {
                                    deadpan_core::SemanticInstruction::MovePauses { forward, count }
                                }
                            });
                        }
                    }
                    Err(error) => {
                        self.message = Some(error);
                        return;
                    }
                }
            }
        }
        self.request_picture(false);
    }

    /// `w`, `b`, `e`, `W` and `B` in either context.
    pub(super) fn speech_motion(&mut self, motion: deadpan_core::SpeechMotion) {
        match self.view {
            View::Source => {
                if !self.viewing_original() {
                    self.message = Some(
                        "Word motions follow the Original's transcript; view the Original to use them."
                            .into(),
                    );
                    return;
                }
                let speech = match self.source_speech() {
                    Ok(speech) => speech,
                    Err(error) => {
                        self.message = Some(error);
                        return;
                    }
                };
                let length = self.source_length() as i64;
                self.source_cursor = speech
                    .motion_target(
                        ProjectFrame(self.source_cursor as i64),
                        (ProjectFrame(0), ProjectFrame(length)),
                        motion,
                    )
                    .0 as u64;
                self.moment.move_to(self.source_cursor);
            }
            View::Sequence => {
                let speech = match self.edit_speech() {
                    Ok(speech) => speech,
                    Err(error) => {
                        self.message = Some(error);
                        return;
                    }
                };
                let cursor = self.sequence_cursor.clamp(self.scope_start, self.scope_end);
                self.sequence_cursor = speech
                    .motion_target(
                        ProjectFrame(cursor as i64),
                        (
                            ProjectFrame(self.scope_start as i64),
                            ProjectFrame(self.scope_end as i64),
                        ),
                        motion,
                    )
                    .0 as u64;
                self.edit_range.move_to(self.sequence_cursor);
                self.select_at_cursor();
                if let Some(count) = std::num::NonZeroU32::new(motion.count) {
                    self.record_macro_local(if motion.sentence {
                        deadpan_core::SemanticInstruction::MoveSentences {
                            forward: motion.forward,
                            count,
                        }
                    } else {
                        deadpan_core::SemanticInstruction::MoveWords {
                            forward: motion.forward,
                            count,
                            end: motion.end,
                        }
                    });
                }
            }
        }
        self.request_picture(false);
    }

    /// `iw`, `aw`, `is`, `as`, `ip` and `ap` in a Your edit Visual selection.
    pub(super) fn select_speech(&mut self, object: deadpan_core::SpeechObject) {
        if !self.macro_action_allowed(Action::SelectSpeech(object)) {
            return;
        }
        self.bindings.clear();
        let speech = match self.edit_analysis() {
            Ok(speech) => speech,
            Err(error) => {
                self.message = Some(error);
                return;
            }
        };
        let bounds = (
            ProjectFrame(self.scope_start as i64),
            ProjectFrame(self.scope_end as i64),
        );
        let result = self
            .capture_macro_target()
            .and_then(|capture| capture.select_speech(&speech, bounds, object));
        match result {
            Ok(context) => {
                self.pause_playback();
                let prior = self.sequence_cursor;
                self.sequence_cursor = context.cursor.0 as u64;
                if let Err(error) = self.restore_macro_visual_selection(context.visual_selection) {
                    self.sequence_cursor = prior;
                    self.error = Some(error);
                    return;
                }
                self.record_macro_local(deadpan_core::SemanticInstruction::SelectSpeech { object });
                self.error = None;
                self.message = Some(format!(
                    "Selected {}. {} copies; {} cuts; {} repeats.",
                    match object {
                        deadpan_core::SpeechObject::InnerWord => "the word",
                        deadpan_core::SpeechObject::AroundWord => "the word with its pauses",
                        deadpan_core::SpeechObject::InnerSentence => "the sentence",
                        deadpan_core::SpeechObject::AroundSentence =>
                            "the sentence with its pauses",
                        deadpan_core::SpeechObject::InnerPause => "the pause",
                        deadpan_core::SpeechObject::AroundPause => "the pause with its edges",
                        deadpan_core::SpeechObject::InnerShot => "the shot",
                        deadpan_core::SpeechObject::AroundShot => "the shot with its transitions",
                    },
                    self.editor_key(EditorKey::Copy),
                    self.editor_key(EditorKey::CutRange),
                    self.editor_key(EditorKey::Repeat),
                ));
                if prior != self.sequence_cursor {
                    self.request_picture(false);
                }
            }
            Err(error) => self.error = Some(error),
        }
    }
}

/// The analyses a speech timeline was built from.
struct SpeechInputs {
    transcript: Option<Arc<crate::project::OriginalTranscript>>,
    activity: Option<Arc<crate::project::OriginalActivity>>,
    shots: Option<Arc<crate::project::OriginalShots>>,
}

/// Identifies the analyses behind a cached timeline: each analysis key, or
/// the reason it is missing, which changes as the job progresses.
type SpeechKey = (
    Result<TranscriptKey, String>,
    Result<deadpan_store::SpeechActivityKey, String>,
    Result<deadpan_store::ShotAnalysisKey, String>,
);

impl DeadpanApp {
    fn speech_key(&self, inputs: &SpeechInputs) -> SpeechKey {
        (
            inputs
                .transcript
                .as_ref()
                .map(|transcript| transcript.key.clone())
                .ok_or_else(|| self.words_not_ready()),
            inputs
                .activity
                .as_ref()
                .map(|activity| activity.key.clone())
                .ok_or_else(|| self.pauses_not_ready()),
            inputs
                .shots
                .as_ref()
                .map(|shots| shots.key.clone())
                .ok_or_else(|| self.shots_not_ready()),
        )
    }
}
