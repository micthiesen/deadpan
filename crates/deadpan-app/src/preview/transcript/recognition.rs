//! Recognizer failure warnings in the TRANSCRIPT rail.
//!
//! The check (`deadpan-recognition-1`) is derived from the published words and
//! pauses whenever either is replaced. It only points at passages; the words
//! stay as recognized until a person corrects them or explicitly asks for a
//! passage to be recognized again by itself, which each passage allows once.

use deadpan_analysis::{Coverage, Passage, RecognitionCheck, RecognizedAgain, check_recognition};
use deadpan_cli::transcription::recognize_again;

use super::*;

/// Recognizing one flagged passage again.
pub(super) enum Again {
    Running {
        passage: RecognizedAgain,
        percent: u8,
        /// The recognizer words the passage is merged into; a different
        /// published transcript makes the result stale.
        base: Arc<Transcript>,
    },
    /// Waiting for the project service to accept the save.
    Queued,
    Saving {
        attempt: u64,
    },
    Failed {
        error: String,
    },
}

/// Flagged passages listed individually; the rest are counted.
const LISTED: usize = 3;
/// Longest repeated phrase shown in a warning, in characters.
const PHRASE_CHARS: usize = 40;

impl DeadpanApp {
    /// The recognition check of the published words, against the published
    /// pauses of the same audio stream when speech has been detected.
    pub(super) fn recognition_check(&mut self) -> Option<Arc<RecognitionCheck>> {
        let workspace = self.workspace.as_ref()?;
        let transcript = workspace.transcript.clone()?;
        let activity = workspace
            .speech_activity
            .clone()
            .filter(|activity| activity.key.audio_stream == transcript.key.audio_stream);
        if let Some(cached) = &self.transcription.recognition
            && Arc::ptr_eq(&cached.transcript, &transcript)
            && match (&cached.activity, &activity) {
                (Some(cached), Some(current)) => Arc::ptr_eq(cached, current),
                (None, None) => true,
                _ => false,
            }
        {
            return Some(cached.check.clone());
        }
        let check = Arc::new(check_recognition(
            &transcript.transcript,
            activity
                .as_ref()
                .map(|activity| (activity.activity.audio(), activity.pauses.pauses.as_slice())),
        ));
        self.transcription.recognition = Some(Recognition {
            transcript,
            activity,
            check: check.clone(),
        });
        Some(check)
    }

    /// Flagged passages, each a link to the Original picture where it begins.
    pub(super) fn recognition_warning(
        &mut self,
        ui: &mut egui::Ui,
        transcript: &crate::project::OriginalTranscript,
    ) {
        let Some(check) = self.recognition_check() else {
            return;
        };
        let clock = |centiseconds: u32| {
            transcript
                .transcript
                .seconds(centiseconds)
                .map(|seconds| {
                    let whole = seconds.floor().max(0);
                    format!("{}:{:02}", whole / 60, whole % 60)
                })
                .unwrap_or_else(|_| "?".into())
        };
        let running = self.transcription.events.is_some();
        let passages = &check.passages;
        match &self.transcription.again {
            Some(Again::Running {
                passage, percent, ..
            }) => {
                ui.add(
                    egui::ProgressBar::new(f32::from(*percent) / 100.0)
                        .desired_height(6.0)
                        .text(format!(
                            "Recognizing {}–{} again · {percent}%",
                            clock(passage.start_cs),
                            clock(passage.end_cs)
                        )),
                );
            }
            Some(Again::Queued | Again::Saving { .. }) => {
                ui.weak("Saving the passage’s new words…");
            }
            Some(Again::Failed { error }) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Recognizing again failed: {error}"),
                );
            }
            None => {}
        }
        if passages.is_empty() {
            if matches!(check.coverage, Coverage::Unchecked { .. }) {
                ui.label(
                    egui::RichText::new("Missing words are checked once pauses are detected.")
                        .size(11.0)
                        .weak(),
                );
            }
            return;
        }
        ui.colored_label(
            style::WARNING,
            format!(
                "Recognition may have failed in {} passage{}. The words are kept as recognized; check {} and fix with :correct.",
                passages.len(),
                if passages.len() == 1 { "" } else { "s" },
                if passages.len() == 1 { "it" } else { "them" },
            ),
        );
        let mut chosen = None;
        let mut again = None;
        for passage in passages.iter().take(LISTED) {
            ui.horizontal_wrapped(|ui| {
                if ui.link(describe(&check, passage, &clock)).clicked() {
                    chosen = passage.word;
                }
                if passage.recognized_again {
                    ui.weak("· recognized again");
                } else if passage.can_recognize_again() && !running {
                    ui.weak("·");
                    if ui
                        .link("Recognize again")
                        .on_hover_text(
                            "Run the recognizer on this passage by itself and replace its words. Each passage can be recognized again once.",
                        )
                        .clicked()
                    {
                        again = Some(passage.region());
                    }
                }
            });
        }
        if passages.len() > LISTED {
            ui.weak(format!(
                "and {} more; the transcript command lists them all.",
                passages.len() - LISTED
            ));
        }
        if let Some(word) = chosen {
            self.jump_to_word(word);
        }
        if let Some(passage) = again {
            self.start_recognize_again(passage, ui.ctx());
        }
    }
}

/// One passage's warning text: what was flagged and where it begins.
fn describe(check: &RecognitionCheck, passage: &Passage, clock: &dyn Fn(u32) -> String) -> String {
    let inside =
        |start: u32| (passage.start_cs..passage.end_cs.max(passage.start_cs + 1)).contains(&start);
    let mut parts = Vec::new();
    for found in check.loops.iter().filter(|found| inside(found.start_cs)) {
        let mut phrase: String = found.phrase.chars().take(PHRASE_CHARS).collect();
        if phrase.len() < found.phrase.len() {
            phrase.push('…');
        }
        parts.push(format!("“{phrase}” repeats {}×", found.copies));
    }
    let speech: u64 = check
        .gaps()
        .iter()
        .filter(|gap| {
            inside(u32::try_from(gap.start / deadpan_analysis::ENERGY_HOP).unwrap_or(u32::MAX))
        })
        .map(|gap| gap.speech_samples)
        .sum();
    if speech > 0 {
        parts.push(format!(
            "{} s of speech without words",
            speech / u64::from(deadpan_analysis::ANALYSIS_SAMPLE_RATE)
        ));
    }
    format!("{} from {}", parts.join(" and "), clock(passage.start_cs))
}

impl DeadpanApp {
    /// Recognize one flagged passage again with the transcript's own model
    /// and language, then save the merged words like a new transcript.
    pub(super) fn start_recognize_again(
        &mut self,
        passage: RecognizedAgain,
        context: &egui::Context,
    ) {
        let failed = |error: String| Some(Again::Failed { error });
        if self.transcription.events.is_some() {
            return;
        }
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let Some(transcript) = workspace.transcript.clone() else {
            return;
        };
        let installed = match (transcription_pack(), self.models.manager.store()) {
            (Some(pack), Some(store)) => store.current(&pack.pack_id),
            _ => {
                self.transcription.again = failed("model storage is unavailable".into());
                return;
            }
        };
        let installed = match installed {
            Ok(Some(installed)) => installed,
            Ok(None) => {
                self.transcription.again =
                    failed("the transcription model is not installed".into());
                return;
            }
            Err(error) => {
                self.transcription.again = failed(error.to_string());
                return;
            }
        };
        let file = &installed.manifest.files[0];
        if file.sha256 != transcript.key.model_sha256 {
            self.transcription.again = failed(
                "the transcript was made with a different model than the installed one".into(),
            );
            return;
        }
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
                self.transcription.again = failed(error.to_string());
                return;
            }
        };
        let language = match Language::try_from(transcript.key.language.clone()) {
            Ok(language) => language,
            Err(error) => {
                self.transcription.again = failed(error);
                return;
            }
        };
        let base = Arc::clone(&transcript.proposal);
        let key = transcript.key.clone();
        let package = workspace.path.clone();
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.transcription.cancel = Arc::clone(&cancel);
        self.transcription.events = Some(receiver);
        self.transcription.again = Some(Again::Running {
            passage,
            percent: 0,
            base: Arc::clone(&base),
        });
        let repaint = context.clone();
        let handle = self.service.jobs().register(
            crate::jobs::JobSpec::new(
                crate::jobs::JobKind::Transcription,
                self.transcription.session,
            )
            .estimated_memory_bytes(installed.manifest.memory_bytes)
            .deferred()
            .detail("a passage of the Original")
            .cancel_flag(&cancel),
        );
        let spawned = std::thread::Builder::new()
            .name("deadpan-transcription".into())
            .spawn(move || {
                let deadline = std::time::Instant::now() + Duration::from_secs(6 * 60 * 60);
                let result = (|| {
                    handle.set_progress("Reading the Original's sound", None);
                    let (analysis, runtime) = activity::prepare(&package, &cancel, deadline)?;
                    if analysis.content != key.content || analysis.audio_stream != key.audio_stream
                    {
                        return Err("the Original's audio differs from the transcript's".into());
                    }
                    handle.acquire(Some(&cancel)).map_err(|stopped| {
                        format!("recognizing again was {stopped} while waiting for the AI model")
                    })?;
                    handle.set_progress("Recognizing a passage again", None);
                    let attempt = uuid::Uuid::new_v4().simple().to_string();
                    let result = recognize_again(
                        &runtime,
                        &model,
                        &analysis.input,
                        &base,
                        passage,
                        language,
                        &attempt,
                        &cancel,
                        deadline,
                        |percent| {
                            handle.set_progress(
                                "Recognizing a passage again",
                                Some(f32::from(percent.min(100)) / 100.0),
                            );
                            let _ = sender.send(Event::Progress(percent));
                            repaint.request_repaint();
                        },
                    )
                    .map_err(|error| error.to_string())?;
                    Ok((key, result.transcript))
                })();
                let _ = sender.send(Event::Transcribed(result));
                repaint.request_repaint();
            });
        match spawned {
            Ok(thread) => self.transcription.thread = Some(thread),
            Err(error) => {
                self.transcription.events = None;
                self.transcription.again = failed(error.to_string());
            }
        }
    }

    /// A finished second pass: save it unless the published recognizer words
    /// changed meanwhile.
    pub(super) fn received_again(&mut self, result: Result<(TranscriptKey, Transcript), String>) {
        let Some(Again::Running { base, .. }) = self.transcription.again.take() else {
            return;
        };
        let current = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.transcript.as_ref())
            .map(|transcript| Arc::clone(&transcript.proposal));
        self.transcription.again = match result {
            Err(error) => Some(Again::Failed { error }),
            Ok(_) if !current.is_some_and(|current| Arc::ptr_eq(&current, &base)) => {
                Some(Again::Failed {
                    error: "the transcript changed while the passage was recognized".into(),
                })
            }
            Ok((key, transcript)) => {
                self.transcription.unsaved = Some((key, Arc::new(transcript)));
                Some(Again::Queued)
            }
        };
    }
}
