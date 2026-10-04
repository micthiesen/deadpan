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

use deadpan_analysis::{Transcript, Word, picture_at, picture_seconds};
use deadpan_cli::transcription::{TranscriptionRuntime, prepare_original_audio, transcribe};
use deadpan_jobs::transcription::Language;
use deadpan_models::packs::{InstallProgress, Operation, PackManifest, PackStore, approved_packs};
use deadpan_store::{AccessMode, ProjectStore, TranscriptKey};

use super::*;

/// Sentences shown on each side of the current one.
const CONTEXT_SEGMENTS: usize = 4;

enum Event {
    Installing(InstallProgress),
    Installed,
    InstallFailed(String),
    Progress(u8),
    Transcribed(Result<(TranscriptKey, Transcript), String>),
}

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
    Saving,
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
        }
    }
}

impl Transcription {
    fn reset(&mut self, session: Option<u64>) {
        self.cancel.store(true, Ordering::Release);
        *self = Self {
            session,
            models_root: self.models_root.take(),
            ..Self::default()
        };
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

    pub(super) fn shutdown(&mut self) {
        self.cancel.store(true, Ordering::Release);
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
            Status::Saving => "saving",
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
                Event::InstallFailed(error) => {
                    self.transcription.events = None;
                    self.transcription.status = Status::Failed(error);
                }
                Event::Progress(percent) => {
                    self.transcription.status = Status::Transcribing(percent);
                }
                Event::Transcribed(result) => {
                    self.transcription.events = None;
                    match result {
                        Ok((key, transcript)) => {
                            if let Some(session) = session {
                                self.submit(ProjectRequest::SaveTranscript {
                                    expected_session: session,
                                    key,
                                    transcript: Arc::new(transcript),
                                });
                                self.transcription.status = Status::Saving;
                            }
                        }
                        Err(error) => self.transcription.status = Status::Failed(error),
                    }
                }
            }
            context.request_repaint();
        }
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        if workspace.transcript.is_some() {
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
        let package = workspace.path.clone();
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.transcription.cancel = Arc::clone(&cancel);
        self.transcription.events = Some(receiver);
        self.transcription.status = Status::Preparing;
        let repaint = context.clone();
        let spawned = std::thread::Builder::new()
            .name("deadpan-transcription".into())
            .spawn(move || {
                let result = run_transcription(&package, &model, &cancel, |percent| {
                    let _ = sender.send(Event::Progress(percent));
                    repaint.request_repaint();
                });
                let _ = sender.send(Event::Transcribed(result));
                repaint.request_repaint();
            });
        if let Err(error) = spawned {
            self.transcription.events = None;
            self.transcription.status = Status::Failed(error.to_string());
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
                    Err(error) => Event::InstallFailed(error.to_string()),
                });
                repaint.request_repaint();
            });
        if let Err(error) = spawned {
            self.transcription.events = None;
            self.transcription.status = Status::Failed(error.to_string());
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
            let seconds = transcript.transcript.seconds(start).ok()?;
            picture_at(video.index().index(), seconds)
        });
        let Some(frame) = frame else {
            self.message = Some("That word is outside the Original picture.".into());
            return;
        };
        self.select_source(asset);
        self.source_cursor = frame as u64;
        self.request_picture(false);
    }

    /// The latest word begun by the end of the Original cursor's picture, so
    /// the picture shown when a word starts marks that word and pauses keep
    /// the previous word in view.
    pub(super) fn current_word(&self) -> Option<usize> {
        let workspace = self.workspace.as_ref()?;
        let transcript = workspace.transcript.as_ref()?;
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

    fn step_transcript_match(&mut self, forward: bool) {
        let count = self.transcription.matches.len();
        if count == 0 {
            return;
        }
        let next = match self.transcription.current_match {
            None if forward => 0,
            None => count - 1,
            Some(index) if forward => (index + 1) % count,
            Some(index) => (index + count - 1) % count,
        };
        self.transcription.current_match = Some(next);
        let word = self.transcription.matches[next].start;
        self.jump_to_word(word);
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
            Status::Saving => {
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
                .id(egui::Id::new("transcript-search"))
                .hint_text("Find words")
                .desired_width(f32::INFINITY),
        );
        if search.changed() {
            self.update_transcript_search();
        }
        if search.has_focus() {
            self.pane = Pane::Sources;
        }
        if search.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            let back = ui.input(|input| input.modifiers.shift);
            self.step_transcript_match(!back);
            search.request_focus();
        }
        if !self.transcription.search.trim().is_empty() {
            let count = self.transcription.matches.len();
            ui.weak(match (count, self.transcription.current_match) {
                (0, _) => "No matches".to_owned(),
                (count, Some(index)) => format!(
                    "{} of {count} · Enter next · Shift+Enter previous",
                    index + 1
                ),
                (count, None) => format!("{count} matches · Enter jumps to the first"),
            });
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
                    if let Some(word) = sentence(ui, words, index..line_end, current, &highlighted)
                    {
                        clicked = Some(word);
                    }
                    index = line_end;
                }
            });
        let approximate = words[start..end]
            .iter()
            .filter(|word| word.approximate())
            .count();
        let mut caption = format!(
            "{} words · {} · recognized on this Mac",
            words.len(),
            transcript.key.engine
        );
        if approximate > 0 {
            caption.push_str(&format!(
                ". {approximate} nearby words are approximate (grey italics)."
            ));
        }
        ui.label(egui::RichText::new(caption).size(11.0).color(style::MUTED));
        if let Some(word) = clicked {
            self.jump_to_word(word);
        }
    }
}

/// Prepare analysis PCM from a read-only view of the project and run the
/// worker installed beside the application.
fn run_transcription(
    package: &std::path::Path,
    model: &deadpan_jobs::transcription::ModelInput,
    cancel: &AtomicBool,
    progress: impl FnMut(u8),
) -> Result<(TranscriptKey, Transcript), String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(6 * 60 * 60);
    let store = ProjectStore::open(package, AccessMode::ReadOnly).map_err(|e| e.to_string())?;
    let analysis =
        prepare_original_audio(&store, None, cancel, deadline).map_err(|e| e.to_string())?;
    drop(store);
    let runtime = TranscriptionRuntime::beside_current_executable().map_err(|e| e.to_string())?;
    if !runtime.executable.is_file() {
        return Err("the transcription helper is missing beside Deadpan".into());
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

/// One sentence as a single wrapped text run. Clicking a word returns it.
fn sentence(
    ui: &mut egui::Ui,
    words: &[Word],
    range: Range<usize>,
    current: Option<usize>,
    matches: &[Range<usize>],
) -> Option<usize> {
    let mut job = egui::text::LayoutJob::default();
    let mut spans = Vec::with_capacity(range.len());
    for index in range.clone() {
        let word = &words[index];
        let matched = matches.iter().any(|m| m.contains(&index));
        let mut format = egui::TextFormat {
            font_id: egui::FontId::proportional(12.5),
            color: if word.approximate() {
                style::MUTED
            } else {
                style::TEXT
            },
            italics: word.approximate(),
            ..Default::default()
        };
        if Some(index) == current {
            format.background = style::SELECTED;
            format.color = style::LAVENDER;
        }
        if matched {
            format.underline = egui::Stroke::new(1.5, style::CURSOR);
        }
        let begin = job.text.chars().count();
        job.append(&word.text, 0.0, format.clone());
        spans.push((begin..job.text.chars().count(), index));
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
    let cursor = galley.cursor_from_pos(position - response.rect.min);
    let offset = cursor.index.0;
    spans
        .iter()
        .find(|(chars, _)| chars.contains(&offset))
        .map(|(_, word)| *word)
}
