//! Word-timed transcripts of the Original's audio.
//!
//! The recognizer analyses mono 16 kHz PCM that begins at a known Original
//! audio sample and reports token bounds in centiseconds. Because both clocks
//! are exact, a centisecond `t` maps to Original audio sample
//! `origin + t · rate / 100` without rounding. Recognizer timing itself is
//! approximate (whisper.cpp calls word timestamps experimental), so every word
//! keeps its probability and callers show uncertain words as approximate.

use std::ops::Range;

use deadpan_core::{ExactRatio, TimeError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Analysis sample rate the recognizer consumes.
pub const ANALYSIS_SAMPLE_RATE: u32 = 16_000;
pub const MAX_SEGMENTS: usize = 50_000;
pub const MAX_TOKENS_PER_SEGMENT: usize = 1_024;
pub const MAX_WORDS: usize = 250_000;
pub const MAX_WORD_BYTES: usize = 256;
pub const MAX_QUERY_WORDS: usize = 32;
/// Words heard with lower probability are presented as approximate.
pub const APPROXIMATE_BELOW: f32 = 0.6;

#[derive(Debug, Error, PartialEq)]
pub enum TranscriptError {
    #[error("transcript exceeds its {0} limit")]
    Limit(&'static str),
    #[error("transcript timing is invalid: {0}")]
    Timing(&'static str),
    #[error("transcript text is invalid: {0}")]
    Text(&'static str),
    #[error("transcript probability is invalid")]
    Probability,
    #[error(transparent)]
    Time(#[from] TimeError),
}

/// One recognizer token, with bounds relative to the analysed PCM start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawToken {
    pub text: String,
    pub t0: i64,
    pub t1: i64,
    pub p: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawSegment {
    pub t0: i64,
    pub t1: i64,
    pub tokens: Vec<RawToken>,
}

/// The Original audio the analysis PCM covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysedAudio {
    /// Original audio sample at which the analysis PCM begins.
    pub origin: i64,
    /// The Original audio stream's sample rate.
    pub sample_rate: u32,
    /// Analysis PCM length in centiseconds; no word may end after it.
    pub duration_cs: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Word {
    /// Display text with its attached punctuation.
    pub text: String,
    /// Half-open centisecond bounds relative to the analysis PCM start.
    pub start_cs: u32,
    pub end_cs: u32,
    /// Lowest token probability in the word.
    pub probability: f32,
    /// Recognizer segment, a sentence-like unit.
    pub segment: u32,
}

impl Word {
    pub fn approximate(&self) -> bool {
        self.probability < APPROXIMATE_BELOW
    }

    /// Lowercase letters, digits and inner apostrophes, used for search.
    pub fn normalized(&self) -> String {
        normalize(&self.text)
    }
}

/// A validated transcript. Construction checks every bound, so stored values
/// can be trusted by display, search and timing conversion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "TranscriptRecord", into = "TranscriptRecord")]
pub struct Transcript {
    audio: AnalysedAudio,
    words: Vec<Word>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TranscriptRecord {
    audio: AnalysedAudio,
    words: Vec<Word>,
}

impl TryFrom<TranscriptRecord> for Transcript {
    type Error = TranscriptError;

    fn try_from(record: TranscriptRecord) -> Result<Self, Self::Error> {
        Self::new(record.audio, record.words)
    }
}

impl From<Transcript> for TranscriptRecord {
    fn from(transcript: Transcript) -> Self {
        Self {
            audio: transcript.audio,
            words: transcript.words,
        }
    }
}

impl Transcript {
    pub fn new(audio: AnalysedAudio, words: Vec<Word>) -> Result<Self, TranscriptError> {
        if audio.sample_rate == 0 {
            return Err(TranscriptError::Timing("zero sample rate"));
        }
        if words.len() > MAX_WORDS {
            return Err(TranscriptError::Limit("word count"));
        }
        let mut previous: Option<&Word> = None;
        for word in &words {
            if word.text.trim().is_empty() || word.text.len() > MAX_WORD_BYTES {
                return Err(TranscriptError::Text("empty or oversized word"));
            }
            if word.text.chars().any(char::is_control) {
                return Err(TranscriptError::Text("control character"));
            }
            if !(0.0..=1.0).contains(&word.probability) {
                return Err(TranscriptError::Probability);
            }
            if word.start_cs > word.end_cs || word.end_cs > audio.duration_cs {
                return Err(TranscriptError::Timing("word outside the analysed audio"));
            }
            if let Some(previous) = previous
                && (word.start_cs < previous.start_cs || word.segment < previous.segment)
            {
                return Err(TranscriptError::Timing("words out of order"));
            }
            previous = Some(word);
        }
        Ok(Self { audio, words })
    }

    /// Build words from recognizer segments. Special tokens are dropped, a
    /// token starting with a space begins a word, and other tokens (subword
    /// pieces, punctuation) extend the current word. Bounds are clamped into
    /// their segment and kept monotonic, so the result always validates.
    pub fn from_segments(
        audio: AnalysedAudio,
        segments: &[RawSegment],
    ) -> Result<Self, TranscriptError> {
        if segments.len() > MAX_SEGMENTS {
            return Err(TranscriptError::Limit("segment count"));
        }
        let duration = i64::from(audio.duration_cs);
        let mut words: Vec<Word> = Vec::new();
        let mut floor = 0_i64;
        for (index, segment) in segments.iter().enumerate() {
            if segment.tokens.len() > MAX_TOKENS_PER_SEGMENT {
                return Err(TranscriptError::Limit("tokens per segment"));
            }
            let segment_start = segment.t0.clamp(floor, duration);
            let segment_end = segment.t1.clamp(segment_start, duration);
            let segment_index =
                u32::try_from(index).map_err(|_| TranscriptError::Limit("segment count"))?;
            let mut current: Option<Word> = None;
            for token in &segment.tokens {
                if token.text.starts_with("[_") || token.text.trim().is_empty() {
                    continue;
                }
                if !token.p.is_finite() || !(0.0..=1.0).contains(&token.p) {
                    return Err(TranscriptError::Probability);
                }
                let start = token.t0.clamp(segment_start, segment_end);
                let end = token.t1.clamp(start, segment_end);
                let punctuation = token.text.trim().chars().all(|c| !c.is_alphanumeric());
                if punctuation && current.is_none() {
                    // Leading punctuation belongs to the previous word, if any.
                    if let Some(previous) = words.last_mut()
                        && previous.text.len() + token.text.trim().len() <= MAX_WORD_BYTES
                    {
                        previous.text.push_str(token.text.trim());
                    }
                    continue;
                }
                let starts_word = token.text.starts_with(' ') || current.is_none();
                if starts_word && !(punctuation && current.is_some()) {
                    if let Some(word) = current.take() {
                        floor = i64::from(word.end_cs).max(floor);
                        words.push(word);
                    }
                    let start = start.max(floor);
                    current = Some(Word {
                        text: token.text.trim_start().to_owned(),
                        start_cs: centiseconds(start)?,
                        end_cs: centiseconds(end.max(start))?,
                        probability: token.p,
                        segment: segment_index,
                    });
                } else if let Some(word) = current.as_mut() {
                    word.text.push_str(token.text.trim_start());
                    if !punctuation {
                        word.end_cs = word.end_cs.max(centiseconds(end)?);
                        word.probability = word.probability.min(token.p);
                    }
                }
                if current
                    .as_ref()
                    .is_some_and(|word| word.text.len() > MAX_WORD_BYTES)
                {
                    return Err(TranscriptError::Limit("word length"));
                }
            }
            if let Some(word) = current.take() {
                floor = i64::from(word.end_cs).max(floor);
                words.push(word);
            }
            if words.len() > MAX_WORDS {
                return Err(TranscriptError::Limit("word count"));
            }
        }
        Self::new(audio, words)
    }

    pub fn audio(&self) -> AnalysedAudio {
        self.audio
    }

    pub fn words(&self) -> &[Word] {
        &self.words
    }

    /// The exact Original audio sample position of an analysis time.
    pub fn original_sample(&self, centiseconds: u32) -> Result<ExactRatio, TranscriptError> {
        let offset = ExactRatio::new(
            i128::from(centiseconds) * i128::from(self.audio.sample_rate),
            100,
        )?;
        Ok(ExactRatio::new(i128::from(self.audio.origin), 1)?.checked_add(offset)?)
    }

    /// The word heard at an analysis time, if any.
    pub fn word_at(&self, centiseconds: u32) -> Option<usize> {
        let index = self
            .words
            .partition_point(|word| word.start_cs <= centiseconds);
        index
            .checked_sub(1)
            .filter(|index| centiseconds < self.words[*index].end_cs)
    }

    /// Word ranges whose normalized text matches the query's words in order.
    /// The last query word matches as a prefix, so results update while typing.
    pub fn search(&self, query: &str) -> Vec<Range<usize>> {
        let terms: Vec<String> = query
            .split_whitespace()
            .map(normalize)
            .filter(|term| !term.is_empty())
            .take(MAX_QUERY_WORDS)
            .collect();
        let Some((last, rest)) = terms.split_last() else {
            return Vec::new();
        };
        let normalized: Vec<String> = self.words.iter().map(Word::normalized).collect();
        (0..normalized.len().saturating_sub(terms.len() - 1))
            .filter(|start| {
                rest.iter()
                    .enumerate()
                    .all(|(offset, term)| normalized[start + offset] == *term)
                    && normalized[start + rest.len()].starts_with(last.as_str())
            })
            .map(|start| start..start + terms.len())
            .collect()
    }
}

fn centiseconds(value: i64) -> Result<u32, TranscriptError> {
    u32::try_from(value).map_err(|_| TranscriptError::Timing("centisecond bound"))
}

fn normalize(text: &str) -> String {
    let lower = text.to_lowercase();
    let kept: String = lower
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '\'' | '’'))
        .map(|c| if c == '’' { '\'' } else { c })
        .collect();
    kept.trim_matches('\'').to_owned()
}

#[cfg(test)]
mod tests;
