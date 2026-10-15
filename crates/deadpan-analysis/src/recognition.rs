//! Long-form recognizer failure checks.
//!
//! On long files whisper can fail silently and confidently: it may repeat one
//! phrase for minutes (a repetition loop) or stop producing words while the
//! speaker goes on (truncation). Neither shows in word probabilities. This
//! module checks a transcript, and optionally the detected speech of the same
//! analysis PCM, for both. It only reports regions; it never drops, merges or
//! rewrites words.
//!
//! [`RECOGNITION_RULE`] names the rule [`check_recognition`] applies:
//!
//! 1. Words are compared by their search normalization; words that normalize
//!    to nothing (pure punctuation) are skipped.
//! 2. A repetition is a maximal run of words in which every word equals the
//!    word `period` positions later, for a period of 1 to [`LOOP_MAX_PERIOD`]
//!    words. Its copies are the whole repetitions of the period it spans.
//!    Periods are tried shortest first and a run overlapping an accepted loop
//!    is not reported again at a longer period.
//! 3. A repetition is a loop when it has at least [`LOOP_MIN_COPIES`] copies
//!    and spans at least [`LOOP_MIN_WORDS`] words.
//! 4. Detected speech is the analysis clock outside the given pauses. Every
//!    stretch between recognized words (and before the first and after the
//!    last) that contains at least [`GAP_MIN_SPEECH_SAMPLES`] of detected
//!    speech is unrecognized speech. Zero-length words still divide stretches.
//!
//! 5. Flagged regions become [`Passage`]s: a loop runs from the end of the
//!    word before it to the start of the word after it, unrecognized speech
//!    is its own stretch, and overlapping regions merge. A passage can be
//!    recognized again by itself once ([`Transcript::recognize_again`]).
//!
//! The thresholds come from the recognizer-failure qualification record
//! (`docs/qualification/recognizer-failures-2026-10-15.md`).

use serde::Serialize;

use crate::{
    ANALYSIS_SAMPLE_RATE, ActivityAudio, AnalysedAudio, ENERGY_HOP, MAX_AGAIN_CS, Pause,
    RecognizedAgain, Transcript,
};

/// The versioned rule implemented by [`check_recognition`].
pub const RECOGNITION_RULE: &str = "deadpan-recognition-1";
/// Longest repeated phrase, in words, that is searched for.
pub const LOOP_MAX_PERIOD: usize = 16;
/// Fewest whole repetitions of a phrase that make a loop.
pub const LOOP_MIN_COPIES: usize = 4;
/// Fewest words a loop spans.
pub const LOOP_MIN_WORDS: usize = 24;
/// Detected speech without a recognized word that is reported (5 s).
pub const GAP_MIN_SPEECH_SAMPLES: u64 = 5 * ANALYSIS_SAMPLE_RATE as u64;
/// Longest phrase text reported for a loop.
const MAX_PHRASE_BYTES: usize = 256;

/// One phrase repeated back to back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Repetition {
    /// Half-open word index range the repetition spans.
    pub first_word: usize,
    pub end_word: usize,
    /// Words in the repeated phrase.
    pub period: usize,
    /// Whole repetitions of the phrase in the range.
    pub copies: usize,
    /// Analysis-clock bounds of the spanned words.
    pub start_cs: u32,
    pub end_cs: u32,
    /// The repeated phrase as displayed.
    pub phrase: String,
}

impl Repetition {
    pub fn words(&self) -> usize {
        self.end_word - self.first_word
    }
}

/// Detected speech with no recognized word, in analysis samples.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnrecognizedSpeech {
    /// Half-open analysis-sample bounds of the stretch between words.
    pub start: u64,
    pub end: u64,
    /// Detected speech inside the stretch.
    pub speech_samples: u64,
    /// The word before the stretch, or none when it begins the audio.
    pub previous_word: Option<usize>,
    /// The word after the stretch, or none when it ends the audio.
    pub next_word: Option<usize>,
}

/// Whether the transcript's coverage of detected speech could be checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Coverage {
    Checked {
        /// Stretches at or above [`GAP_MIN_SPEECH_SAMPLES`] of speech.
        gaps: Vec<UnrecognizedSpeech>,
        /// The largest stretch, flagged or not, for calibration.
        longest: Option<UnrecognizedSpeech>,
        /// All detected speech in the analysed audio.
        speech_samples: u64,
    },
    /// No speech activity of the same audio, or it does not match.
    Unchecked { reason: String },
}

/// One flagged passage: overlapping loops and unrecognized speech merged,
/// widened to the recognized words around them. Recognizing it again runs
/// the recognizer on exactly this region by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Passage {
    /// Half-open analysis-clock region, from the end of the last unflagged
    /// word before it to the start of the first one after it.
    pub start_cs: u32,
    pub end_cs: u32,
    /// The word where the passage begins, for navigation: the loop's first
    /// word, or the word before unrecognized speech (after it at the start).
    pub word: Option<usize>,
    /// Loops and unrecognized stretches merged into this passage.
    pub loops: usize,
    pub unrecognized: usize,
    /// Whether its words already came from recognizing it again.
    pub recognized_again: bool,
}

impl Passage {
    pub fn region(&self) -> RecognizedAgain {
        RecognizedAgain {
            start_cs: self.start_cs,
            end_cs: self.end_cs,
        }
    }

    /// Whether the passage may be recognized again: once, and not longer
    /// than [`MAX_AGAIN_CS`].
    pub fn can_recognize_again(&self) -> bool {
        !self.recognized_again && self.end_cs - self.start_cs <= MAX_AGAIN_CS
    }
}

/// The result of [`check_recognition`]. Derived on every read from the stored
/// transcript and speech activity, so it is never stale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecognitionCheck {
    pub rule: &'static str,
    /// Repetitions meeting the loop thresholds.
    pub loops: Vec<Repetition>,
    /// The repetition spanning the most words, flagged or not, for
    /// calibration.
    pub longest_repetition: Option<Repetition>,
    pub coverage: Coverage,
    /// Flagged passages in order, without overlap.
    pub passages: Vec<Passage>,
}

impl RecognitionCheck {
    /// Whether any region was flagged.
    pub fn flagged(&self) -> bool {
        !self.loops.is_empty() || !self.gaps().is_empty()
    }

    /// Flagged unrecognized speech, empty when coverage was not checked.
    pub fn gaps(&self) -> &[UnrecognizedSpeech] {
        match &self.coverage {
            Coverage::Checked { gaps, .. } => gaps,
            Coverage::Unchecked { .. } => &[],
        }
    }
}

/// Check a transcript for repetition loops and, given the pauses detected in
/// the same analysis audio, for detected speech without words. `speech` is the
/// activity's audio and its (possibly corrected) pauses in order.
pub fn check_recognition(
    transcript: &Transcript,
    speech: Option<(ActivityAudio, &[Pause])>,
) -> RecognitionCheck {
    let (loops, longest_repetition) = repetitions(transcript);
    let coverage = match speech {
        None => Coverage::Unchecked {
            reason: "speech activity of this audio has not been detected".into(),
        },
        Some((audio, pauses)) => match matching(transcript.audio(), audio, pauses) {
            Err(reason) => Coverage::Unchecked {
                reason: reason.into(),
            },
            Ok(()) => coverage(transcript, audio.samples, pauses),
        },
    };
    let mut check = RecognitionCheck {
        rule: RECOGNITION_RULE,
        loops,
        longest_repetition,
        coverage,
        passages: Vec::new(),
    };
    check.passages = passages(transcript, &check);
    check
}

/// Merge loops and unrecognized speech into passages between unflagged words.
fn passages(transcript: &Transcript, check: &RecognitionCheck) -> Vec<Passage> {
    let words = transcript.words();
    let duration = transcript.audio().duration_cs;
    let mut found: Vec<Passage> = Vec::new();
    for repetition in &check.loops {
        let start_cs = repetition
            .first_word
            .checked_sub(1)
            .map_or(0, |before| words[before].end_cs.min(repetition.start_cs));
        let end_cs = words
            .get(repetition.end_word)
            .map_or(duration, |after| after.start_cs);
        found.push(Passage {
            start_cs,
            end_cs,
            word: Some(repetition.first_word),
            loops: 1,
            unrecognized: 0,
            recognized_again: false,
        });
    }
    for gap in check.gaps() {
        let start_cs = u32::try_from(gap.start / ENERGY_HOP).unwrap_or(duration);
        let end_cs = u32::try_from(gap.end.div_ceil(ENERGY_HOP))
            .unwrap_or(duration)
            .min(duration);
        found.push(Passage {
            start_cs,
            end_cs,
            word: gap.previous_word.or(gap.next_word),
            loops: 0,
            unrecognized: 1,
            recognized_again: false,
        });
    }
    found.retain(|passage| passage.end_cs > passage.start_cs);
    found.sort_by_key(|passage| (passage.start_cs, passage.end_cs));
    let mut merged: Vec<Passage> = Vec::with_capacity(found.len());
    for passage in found {
        match merged.last_mut() {
            Some(last) if passage.start_cs < last.end_cs => {
                last.end_cs = last.end_cs.max(passage.end_cs);
                last.loops += passage.loops;
                last.unrecognized += passage.unrecognized;
            }
            _ => merged.push(passage),
        }
    }
    for passage in &mut merged {
        passage.recognized_again = transcript
            .recognized_again()
            .iter()
            .any(|done| done.overlaps(passage.start_cs, passage.end_cs));
    }
    merged
}

fn matching(
    transcript: AnalysedAudio,
    activity: ActivityAudio,
    pauses: &[Pause],
) -> Result<(), &'static str> {
    if transcript.origin != activity.origin
        || transcript.sample_rate != activity.sample_rate
        || u64::from(transcript.duration_cs) != activity.samples.div_ceil(ENERGY_HOP)
    {
        return Err("speech activity describes different audio than the transcript");
    }
    let mut end = 0;
    for pause in pauses {
        if pause.start < end || pause.end <= pause.start || pause.end > activity.samples {
            return Err("pauses are out of order or outside the audio");
        }
        end = pause.end;
    }
    Ok(())
}

/// Loops, and the longest repetition of any size.
fn repetitions(transcript: &Transcript) -> (Vec<Repetition>, Option<Repetition>) {
    let words = transcript.words();
    // Indices of words that carry text, with their normalized forms.
    let (indices, normalized): (Vec<usize>, Vec<String>) = words
        .iter()
        .enumerate()
        .map(|(index, word)| (index, word.normalized()))
        .filter(|(_, text)| !text.is_empty())
        .unzip();
    let count = normalized.len();
    let mut loops: Vec<Repetition> = Vec::new();
    let mut longest: Option<Repetition> = None;
    // Normalized positions already inside an accepted loop.
    let mut covered = vec![false; count];
    for period in 1..=LOOP_MAX_PERIOD.min(count / 2) {
        let mut position = 0;
        while position + period < count {
            if normalized[position] != normalized[position + period] {
                position += 1;
                continue;
            }
            let start = position;
            while position + period < count && normalized[position] == normalized[position + period]
            {
                position += 1;
            }
            // Positions start..position match their successor one period on,
            // so the run spans start..position + period.
            let end = position + period;
            let copies = (end - start) / period;
            if copies < 2 {
                continue;
            }
            // Keep whole copies only.
            let end = start + copies * period;
            let repetition = || Repetition {
                first_word: indices[start],
                end_word: indices[end - 1] + 1,
                period,
                copies,
                start_cs: words[indices[start]].start_cs,
                end_cs: indices[start..end]
                    .iter()
                    .map(|index| words[*index].end_cs)
                    .max()
                    .unwrap_or(words[indices[start]].end_cs),
                phrase: phrase(transcript, &indices[start..start + period]),
            };
            let first_word = indices[start];
            let end_word = indices[end - 1] + 1;
            if longest
                .as_ref()
                .is_none_or(|longest| end_word - first_word > longest.words())
                && !covered[start..end].iter().all(|inside| *inside)
            {
                longest = Some(repetition());
            }
            if copies >= LOOP_MIN_COPIES
                && end - start >= LOOP_MIN_WORDS
                && !covered[start..end].iter().any(|inside| *inside)
            {
                covered[start..end]
                    .iter_mut()
                    .for_each(|inside| *inside = true);
                loops.push(repetition());
            }
        }
    }
    loops.sort_by_key(|repetition| repetition.first_word);
    (loops, longest)
}

/// Display text of the words at `indices`, bounded.
fn phrase(transcript: &Transcript, indices: &[usize]) -> String {
    let mut text = String::new();
    for index in indices {
        let word = &transcript.words()[*index].text;
        if text.len() + word.len() + 1 > MAX_PHRASE_BYTES {
            break;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(word);
    }
    text
}

fn coverage(transcript: &Transcript, samples: u64, pauses: &[Pause]) -> Coverage {
    let hop = ENERGY_HOP;
    let mut stretches: Vec<UnrecognizedSpeech> = Vec::new();
    let mut covered_end = 0_u64;
    let mut previous_word = None;
    for (index, word) in transcript.words().iter().enumerate() {
        let start = (u64::from(word.start_cs) * hop).min(samples);
        let end = (u64::from(word.end_cs) * hop).min(samples);
        if start > covered_end {
            stretches.push(UnrecognizedSpeech {
                start: covered_end,
                end: start.max(covered_end),
                speech_samples: 0,
                previous_word,
                next_word: Some(index),
            });
        }
        covered_end = covered_end.max(end);
        previous_word = Some(index);
    }
    stretches.push(UnrecognizedSpeech {
        start: covered_end,
        end: samples,
        speech_samples: 0,
        previous_word,
        next_word: None,
    });
    stretches.retain(|stretch| stretch.end > stretch.start);
    let speech_samples = samples - pauses.iter().map(|p| p.end - p.start).sum::<u64>();
    // Both lists are in order, so one pass measures every stretch's speech.
    let mut pause = 0;
    for stretch in &mut stretches {
        while pause < pauses.len() && pauses[pause].end <= stretch.start {
            pause += 1;
        }
        let mut quiet = 0;
        let mut index = pause;
        while index < pauses.len() && pauses[index].start < stretch.end {
            let from = pauses[index].start.max(stretch.start);
            let to = pauses[index].end.min(stretch.end);
            quiet += to.saturating_sub(from);
            index += 1;
        }
        stretch.speech_samples = (stretch.end - stretch.start) - quiet;
    }
    let longest = stretches
        .iter()
        .filter(|stretch| stretch.speech_samples > 0)
        .max_by_key(|stretch| (stretch.speech_samples, std::cmp::Reverse(stretch.start)))
        .cloned();
    stretches.retain(|stretch| stretch.speech_samples >= GAP_MIN_SPEECH_SAMPLES);
    Coverage::Checked {
        gaps: stretches,
        longest,
        speech_samples,
    }
}

#[cfg(test)]
mod tests;
