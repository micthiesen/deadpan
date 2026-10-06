//! Headless corrections of the Original's transcript and detected pauses.
//!
//! This is the `:correct` sheet's backend without the sheet: the same
//! `deadpan_analysis::Corrections` operations build the new value from the
//! same stored transcript, speech activity and measured edges, and the same
//! `ProjectStore::change_analysis_corrections` call saves it. Corrections live
//! outside document history, so every request names the corrections version
//! it expects instead of a document revision, and every word or pause it
//! targets carries the text or bounds the caller saw. A dry run computes the
//! change through the same path and writes nothing.

use std::path::Path;

use deadpan_analysis::{
    CENTISECOND_SAMPLES, CorrectedPauses, CorrectionClock, Corrections, Pause, SpeechActivity,
    edges_in_centiseconds, measured_edges,
};
use deadpan_store::{AccessMode, CorrectionChange, CorrectionsKey, ProjectStore};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::live_project::LiveError;
use crate::speech::StoredWords;

const MAX_REQUEST_BYTES: u64 = 64 * 1024;

const USAGE: &str = "usage: corrections <project.deadpan> [--asset <id>] | corrections <project.deadpan> --json <request.json> [--dry-run]";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol: u32,
    /// The corrections version the caller saw; 0 when none was stored.
    pub expected_version: u64,
    /// A registered source instead of the project's ready Original.
    #[serde(default)]
    pub asset: Option<deadpan_core::AssetId>,
    pub change: Change,
    #[serde(default)]
    pub dry_run: bool,
}

/// One change, as the `:correct` sheet makes it. Word and pause indexes are
/// positions in the corrected `transcript` and `pauses` outputs; the
/// `expected_*` fields repeat what the caller saw there, so a change computed
/// against an older transcript or detection is refused.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    /// Replace a word's text; several words split it at measured edges.
    EditWord {
        word: usize,
        expected_text: String,
        text: String,
    },
    RemoveWord {
        word: usize,
        expected_text: String,
    },
    /// Join a word with the next one.
    JoinWords {
        word: usize,
        expected_text: String,
    },
    SetWordBounds {
        word: usize,
        expected_text: String,
        start_cs: u32,
        end_cs: u32,
    },
    /// A pause after the word: its gap to the next word, or 200 ms.
    AddPauseAfter {
        word: usize,
        expected_text: String,
    },
    RemovePause {
        pause: usize,
        expected_start: u64,
        expected_end: u64,
    },
    /// New bounds in analysis samples.
    SetPauseBounds {
        pause: usize,
        expected_start: u64,
        expected_end: u64,
        start: u64,
        end: u64,
    },
    /// Drop the stored regions that no longer apply to the current analyses.
    DropInapplicable,
    /// Discard unreadable stored values; readable steps stay.
    DiscardUnreadable,
    Undo,
    Redo,
}

pub fn run(arguments: &[&str]) -> Result<(), crate::CliError> {
    let usage = || crate::CliError::Usage(USAGE.into());
    match arguments {
        [path] => crate::write_json(&inspect(Path::new(path), None)?),
        [path, "--asset", asset] => {
            let asset = deadpan_core::AssetId::new(*asset)
                .map_err(|error| crate::CliError::Usage(error.to_string()))?;
            crate::write_json(&inspect(Path::new(path), Some(&asset))?)
        }
        [path, "--json", request] | [path, "--json", request, "--dry-run"] => {
            let mut request = read_request(Path::new(request))?;
            request.dry_run |= arguments.len() == 4;
            crate::write_json(&execute(Path::new(path), &request)?)
        }
        _ => Err(usage()),
    }
}

fn read_request(path: &Path) -> Result<Request, crate::CliError> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_REQUEST_BYTES {
        return Err(LiveError::new("LimitExceeded", "Corrections request is too large").into());
    }
    let request: Request = serde_json::from_slice(&bytes)?;
    if request.protocol != 1 {
        return Err(crate::CliError::Protocol(request.protocol));
    }
    Ok(request)
}

/// What a correction is computed against, read from one store handle.
struct Loaded {
    key: CorrectionsKey,
    stored: Option<deadpan_store::StoredCorrections>,
    /// Why stored corrections, or one of their Undo/Redo steps, are unreadable.
    unreadable: Option<String>,
    words: Option<StoredWords>,
    activity: Option<(deadpan_store::SpeechActivityKey, SpeechActivity)>,
    pauses: Option<CorrectedPauses>,
}

impl Loaded {
    fn read(
        store: &ProjectStore,
        asset: Option<&deadpan_core::AssetId>,
    ) -> Result<Self, crate::CliError> {
        let receipt = crate::transcription::analysed_receipt(store, asset)?;
        let stream = receipt
            .snapshot()
            .audio()
            .ok_or_else(|| unavailable("the Original has no qualified audio stream"))?
            .stream()
            .stream_index;
        let content = receipt.original().content().to_string();
        let key = crate::speech::corrections_key(&content, stream);
        let (stored, mut unreadable) = match store.analysis_corrections(&key) {
            Ok(stored) => (stored, None),
            Err(error) => (None, Some(error.to_string())),
        };
        if unreadable.is_none() {
            unreadable = store
                .unreadable_analysis_corrections()?
                .into_iter()
                .find(|bad| bad.key == key)
                .map(|bad| format!("a stored {} step is unreadable: {}", bad.place, bad.error));
        }
        let words = crate::speech::stored_words(store, &content)
            .filter(|words| words.key.audio_stream == stream);
        let activity = crate::activity::stored_activity(store, &content)
            .filter(|(key, _)| key.audio_stream == stream);
        let pauses = activity.as_ref().map(|(_, activity)| {
            crate::speech::corrected_pauses(store, &content, stream, activity).0
        });
        Ok(Self {
            key,
            stored,
            unreadable,
            words,
            activity,
            pauses,
        })
    }

    fn version(&self) -> u64 {
        self.stored.as_ref().map_or(0, |stored| stored.version)
    }

    fn clock(&self) -> Option<CorrectionClock> {
        self.words
            .as_ref()
            .map(|words| Corrections::clock_of(&words.proposal))
            .or_else(|| {
                self.activity
                    .as_ref()
                    .map(|(_, activity)| Corrections::clock_of_activity(activity))
            })
    }

    /// The stored corrections, or none yet on the analysis clock.
    fn base(&self) -> Result<Corrections, LiveError> {
        if let Some(error) = &self.unreadable {
            return Err(LiveError::new(
                "AnalysisCorrectionsUnreadable",
                format!(
                    "The stored corrections are unreadable, so they were not changed: {error}. Discard them first."
                ),
            ));
        }
        match &self.stored {
            Some(stored) => Ok(stored.corrections.clone()),
            None => self
                .clock()
                .map(Corrections::empty)
                .ok_or_else(|| unavailable("there is nothing to correct yet")),
        }
    }

    fn words(&self) -> Result<&StoredWords, LiveError> {
        let words = self
            .words
            .as_ref()
            .ok_or_else(|| unavailable("the Original has no stored transcript"))?;
        if let Some(error) = &words.corrections_error {
            return Err(LiveError::new(
                "AnalysisCorrectionsUnreadable",
                format!(
                    "the stored corrections do not apply to the transcript ({error}); discard or drop them first"
                ),
            ));
        }
        Ok(words)
    }

    fn activity(&self) -> Result<(&SpeechActivity, &CorrectedPauses), LiveError> {
        self.activity
            .as_ref()
            .zip(self.pauses.as_ref())
            .map(|((_, activity), pauses)| (activity, pauses))
            .ok_or_else(|| unavailable("the Original has no detected speech activity"))
    }

    fn word(&self, index: usize, expected: &str) -> Result<&deadpan_analysis::Word, LiveError> {
        let word = self
            .words()?
            .corrected
            .transcript
            .words()
            .get(index)
            .ok_or_else(|| target_changed(format!("there is no word {index}")))?;
        if word.text != expected {
            return Err(target_changed(format!(
                "word {index} is now “{}”, not “{expected}”",
                word.text
            )));
        }
        Ok(word)
    }

    fn pause(&self, index: usize, start: u64, end: u64) -> Result<Pause, LiveError> {
        let (_, pauses) = self.activity()?;
        let pause = pauses
            .pauses
            .get(index)
            .copied()
            .ok_or_else(|| target_changed(format!("there is no pause {index}")))?;
        if (pause.start, pause.end) != (start, end) {
            return Err(target_changed(format!(
                "pause {index} is now {}..{}, not {start}..{end}",
                pause.start, pause.end
            )));
        }
        Ok(pause)
    }

    /// The store change and its label, built exactly as the sheet builds it.
    fn change(&self, change: &Change) -> Result<(CorrectionChange, String), LiveError> {
        let refused = |error: deadpan_analysis::CorrectionError| {
            LiveError::new("CorrectionRefused", error.to_string())
        };
        let apply = |corrections, label: String| {
            Ok((
                CorrectionChange::Apply {
                    corrections,
                    label: label.clone(),
                },
                label,
            ))
        };
        match change {
            Change::EditWord {
                word,
                expected_text,
                text,
            } => self.edit_word(*word, expected_text, text),
            Change::RemoveWord {
                word,
                expected_text,
            } => self.edit_word(*word, expected_text, ""),
            Change::JoinWords {
                word,
                expected_text,
            } => {
                let first = self.word(*word, expected_text)?;
                let second = self
                    .words()?
                    .corrected
                    .transcript
                    .words()
                    .get(word + 1)
                    .ok_or_else(|| {
                        LiveError::new("CorrectionRefused", "There is no next word to join.")
                    })?;
                let label = format!("join “{}” and “{}”", first.text, second.text);
                let corrections = self
                    .base()?
                    .merge_words(&self.words()?.corrected, *word)
                    .map_err(refused)?;
                apply(corrections, label)
            }
            Change::SetWordBounds {
                word,
                expected_text,
                start_cs,
                end_cs,
            } => {
                let name = self.word(*word, expected_text)?.text.clone();
                let (corrections, start, end) = self
                    .base()?
                    .set_word_bounds(&self.words()?.corrected, *word, *start_cs, *end_cs)
                    .map_err(refused)?;
                apply(corrections, format!("move “{name}” to {start}–{end} cs"))
            }
            Change::AddPauseAfter {
                word,
                expected_text,
            } => {
                let target = self.word(*word, expected_text)?;
                let (activity, pauses) = self.activity()?;
                let words = self.words()?.corrected.transcript.words();
                let start = u64::from(target.end_cs) * CENTISECOND_SAMPLES;
                let gap_end = words
                    .get(word + 1)
                    .map(|next| u64::from(next.start_cs) * CENTISECOND_SAMPLES)
                    .filter(|end| *end >= start + 5 * CENTISECOND_SAMPLES);
                let end = gap_end
                    .unwrap_or(start + 20 * CENTISECOND_SAMPLES)
                    .min(activity.audio().samples);
                if start >= end {
                    return Err(LiveError::new(
                        "CorrectionRefused",
                        "There is no audio after this word for a pause.",
                    ));
                }
                let corrections = self
                    .base()?
                    .add_pause(activity, pauses, Pause { start, end })
                    .map_err(refused)?;
                apply(corrections, format!("add pause after “{}”", target.text))
            }
            Change::RemovePause {
                pause,
                expected_start,
                expected_end,
            } => {
                let seen = self.pause(*pause, *expected_start, *expected_end)?;
                let (activity, pauses) = self.activity()?;
                let corrections = self
                    .base()?
                    .replace_pauses(activity, pauses, *pause..pause + 1, vec![])
                    .map_err(refused)?;
                apply(
                    corrections,
                    format!("remove pause at {}..{}", seen.start, seen.end),
                )
            }
            Change::SetPauseBounds {
                pause,
                expected_start,
                expected_end,
                start,
                end,
            } => {
                self.pause(*pause, *expected_start, *expected_end)?;
                let (activity, pauses) = self.activity()?;
                let (corrections, start, end) = self
                    .base()?
                    .set_pause_bounds(activity, pauses, *pause, *start, *end)
                    .map_err(refused)?;
                apply(corrections, format!("move pause to {start}..{end}"))
            }
            Change::DropInapplicable => {
                let skipped = self.skipped();
                let stored = self
                    .stored
                    .as_ref()
                    .filter(|_| skipped > 0)
                    .ok_or_else(|| {
                        LiveError::new(
                            "CorrectionRefused",
                            "Every stored correction is readable and applies.",
                        )
                    })?;
                self.base()?;
                let kept = stored.corrections.applicable_to(
                    self.words.as_ref().map(|words| &words.proposal),
                    self.activity.as_ref().map(|(_, activity)| activity),
                );
                apply(
                    kept,
                    format!(
                        "drop {skipped} correction{} that no longer appl{}",
                        if skipped == 1 { "" } else { "s" },
                        if skipped == 1 { "ies" } else { "y" }
                    ),
                )
            }
            Change::DiscardUnreadable => {
                if self.unreadable.is_none() {
                    return Err(LiveError::new(
                        "CorrectionRefused",
                        "Every stored correction is readable.",
                    ));
                }
                let clock = self
                    .clock()
                    .ok_or_else(|| unavailable("there is no analysis clock to keep"))?;
                Ok((
                    CorrectionChange::DiscardUnreadable { clock },
                    "discarded unreadable corrections".into(),
                ))
            }
            Change::Undo | Change::Redo => {
                let undo = matches!(change, Change::Undo);
                let label = self
                    .stored
                    .as_ref()
                    .and_then(|stored| {
                        if undo {
                            stored.undo.clone()
                        } else {
                            stored.redo.clone()
                        }
                    })
                    .ok_or_else(|| {
                        LiveError::new(
                            "CorrectionRefused",
                            format!(
                                "There is no correction to {}.",
                                if undo { "undo" } else { "redo" }
                            ),
                        )
                    })?;
                if let Some(error) = &self.unreadable {
                    return Err(LiveError::new(
                        "AnalysisCorrectionsUnreadable",
                        format!(
                            "The stored corrections are unreadable: {error}. Discard them first."
                        ),
                    ));
                }
                Ok(if undo {
                    (CorrectionChange::Undo, format!("undid {label}"))
                } else {
                    (CorrectionChange::Redo, format!("redid {label}"))
                })
            }
        }
    }

    fn edit_word(
        &self,
        index: usize,
        expected: &str,
        text: &str,
    ) -> Result<(CorrectionChange, String), LiveError> {
        let word = self.word(index, expected)?;
        let words = self.words()?;
        let edges = self
            .activity
            .as_ref()
            .zip(self.pauses.as_ref())
            .map(|((_, activity), pauses)| {
                edges_in_centiseconds(
                    &measured_edges(activity, &pauses.pauses),
                    &words.corrected.transcript,
                    activity,
                )
            })
            .unwrap_or_default();
        let label = if text.trim().is_empty() {
            format!("remove “{}”", word.text)
        } else {
            format!(
                "“{}” → “{}”",
                word.text,
                text.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        };
        let corrections = self
            .base()?
            .edit_word_text(&words.corrected, index, text, &edges)
            .map_err(|error| LiveError::new("CorrectionRefused", error.to_string()))?;
        Ok((
            CorrectionChange::Apply {
                corrections,
                label: label.clone(),
            },
            label,
        ))
    }

    fn skipped(&self) -> usize {
        self.words
            .as_ref()
            .map_or(0, |words| words.corrected.skipped)
            + self.pauses.as_ref().map_or(0, |pauses| pauses.skipped)
    }

    fn report(&self) -> Value {
        let words = self.words.as_ref().map(|words| {
            words
                .corrected
                .transcript
                .words()
                .iter()
                .enumerate()
                .map(|(index, word)| {
                    json!({"index": index, "text": word.text, "start_cs": word.start_cs,
                        "end_cs": word.end_cs, "corrected": words.corrected.corrected(index)})
                })
                .collect::<Vec<_>>()
        });
        let pauses = self.pauses.as_ref().map(|pauses| {
            pauses
                .pauses
                .iter()
                .zip(&pauses.corrected)
                .enumerate()
                .map(|(index, (pause, corrected))| {
                    json!({"index": index, "start": pause.start, "end": pause.end,
                        "corrected": corrected})
                })
                .collect::<Vec<_>>()
        });
        json!({
            "protocol": 1,
            "correction_rule": deadpan_analysis::CORRECTION_RULE,
            "key": {"content": self.key.content, "audio_stream": self.key.audio_stream},
            "version": self.version(),
            "undo": self.stored.as_ref().and_then(|stored| stored.undo.clone()),
            "redo": self.stored.as_ref().and_then(|stored| stored.redo.clone()),
            "unreadable": self.unreadable,
            "skipped": self.skipped(),
            "corrections": self.stored.as_ref().map(|stored| &stored.corrections),
            "transcript_key": self.words.as_ref().map(|words| &words.key),
            "transcript_error": self.words.as_ref().and_then(|words| words.corrections_error.clone()),
            "activity_key": self.activity.as_ref().map(|(key, _)| key),
            "words": words,
            "pauses": pauses,
        })
    }
}

fn unavailable(reason: &str) -> LiveError {
    LiveError::new(
        "AnalysisUnavailable",
        format!("Corrections are unavailable: {reason}"),
    )
}

fn target_changed(reason: String) -> LiveError {
    LiveError::new(
        "CorrectionTargetChanged",
        format!("The analysis changed since it was read ({reason}); read it again and retry"),
    )
}

fn conflict(current: u64, expected: u64) -> LiveError {
    LiveError::new(
        "AnalysisCorrectionsConflict",
        format!("the corrections changed (version {current}, expected {expected})"),
    )
}

pub fn inspect(
    package: &Path,
    asset: Option<&deadpan_core::AssetId>,
) -> Result<Value, crate::CliError> {
    let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
    Ok(Loaded::read(&store, asset)?.report())
}

/// Read-only dry runs open their own reader; a committing change uses the
/// writer, or the open app's live endpoint, which saves it on its own writer
/// and refreshes its transcript and pauses.
pub fn execute(package: &Path, request: &Request) -> Result<Value, crate::CliError> {
    Ok(crate::live_project::dispatch_short(
        package,
        None,
        crate::live_project::ShortOperation::Corrections {
            request: Box::new(request.clone()),
        },
    )?)
}

/// One versioned correction on whichever process holds `store`.
pub fn execute_on(store: &ProjectStore, request: &Request) -> Result<Value, LiveError> {
    if request.protocol != 1 {
        return Err(LiveError::new(
            "ProtocolUnsupported",
            "Corrections protocol must be 1",
        ));
    }
    let loaded = Loaded::read(store, request.asset.as_ref()).map_err(cli_error)?;
    let version = loaded.version();
    if version != request.expected_version {
        return Err(conflict(version, request.expected_version));
    }
    let (change, label) = loaded.change(&request.change)?;
    if request.dry_run {
        let proposed = match &change {
            CorrectionChange::Apply { corrections, .. } => Some(corrections),
            _ => None,
        };
        return Ok(json!({
            "protocol": 1, "committed": false, "dry_run": true, "label": label,
            "version": version, "proposed": proposed,
        }));
    }
    let stored = store
        .change_analysis_corrections(&loaded.key, version, change)
        .map_err(LiveError::store)?;
    Ok(json!({
        "protocol": 1, "committed": true, "dry_run": false, "label": label,
        "before_version": version, "version": stored.version,
        "undo": stored.undo, "redo": stored.redo, "corrections": stored.corrections,
    }))
}

fn cli_error(error: crate::CliError) -> LiveError {
    match error {
        crate::CliError::LiveProject(error) => error,
        error => LiveError::new(error.code(), &error),
    }
}
