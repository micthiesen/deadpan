//! Automatic phoneme refinement of a freshly recognized transcript.
//!
//! whisper.cpp's DTW word times have a 50 ms median start error; aligning the
//! recognized text to the phoneme model's emissions brings it to about 20 ms
//! (LibriSpeech, `native/deadpan-transcribe/examples/word_eval.py`). The
//! result replaces the recognizer's times in the stored proposal, so manual
//! corrections, search and word motions apply over it exactly as before.
//! Refinement never fails a transcription: a word it cannot place keeps its
//! recognizer times, and so does every word when the aligner is unavailable.

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use deadpan_analysis::Transcript;
use deadpan_analysis::refinement::{RefinementControl, RefinementError as AlignError, propose};

use super::{RefinementError, RefinementRuntime, check, infer};
use crate::models::RefinementModels;

/// Samples per centisecond at the 16 kHz analysis rate.
const HOP: usize = 160;
const MAX_WORDS_PER_WINDOW: usize = 40;
/// Span of the words in one window, in centiseconds (the model takes 30 s).
const MAX_SPAN_CS: u32 = 2_000;
/// Audio included before and after a window's words.
const MARGIN_CS: u32 = 30;
/// A refined edge further than this from the recognizer's is not trusted: the
/// text there was probably misrecognized and the forced alignment wandered.
const MAX_SHIFT_CS: i64 = 50;

#[derive(Debug, Clone)]
pub struct TranscriptRefiner {
    pub runtime: RefinementRuntime,
    pub models: RefinementModels,
}

impl TranscriptRefiner {
    /// Identity of the aligner for cache keys.
    pub fn identity(&self) -> String {
        format!(
            "{}-{}-{}",
            self.models.pack_id,
            self.models.pack_version,
            self.models.checkpoint.sha256.as_str()
        )
    }
}

/// End of the window starting at `start`: at most 40 words and 20 s, cut at the
/// longest gap among the last 15 words so a boundary falls in a pause if any.
fn window_end(words: &[deadpan_analysis::Word], start: usize) -> usize {
    let mut end = start + 1;
    while end < words.len()
        && end - start < MAX_WORDS_PER_WINDOW
        && words[end].end_cs - words[start].start_cs <= MAX_SPAN_CS
    {
        end += 1;
    }
    if end == words.len() {
        return end;
    }
    let from = start.max(end.saturating_sub(15)) + 1;
    (from..=end)
        .max_by_key(|&cut| words[cut].start_cs.saturating_sub(words[cut - 1].end_cs))
        .unwrap_or(end)
}

fn lexical(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic() || *c == '\'')
        .collect()
}

/// Refined `(start_cs, end_cs)` per word of `words[first..last]`, window-relative
/// coordinates already shifted to the transcript's. `None` keeps the recognizer.
fn refine_window(
    refiner: &TranscriptRefiner,
    samples: &[f32],
    words: &[deadpan_analysis::Word],
    non_speech: &[bool],
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Vec<Option<(u32, u32)>>, RefinementError> {
    let mut result = vec![None; words.len()];
    let from_cs = words[0].start_cs.saturating_sub(MARGIN_CS) as usize;
    let to_cs = words[words.len() - 1].end_cs as usize + MARGIN_CS as usize;
    let (from, to) = (from_cs * HOP, (to_cs * HOP).min(samples.len()));
    if to <= from || to - from < 401 || to - from > 480_000 {
        return Ok(result);
    }
    let emissions = infer(
        &refiner.runtime,
        &refiner.models.checkpoint,
        &refiner.models.statistics,
        &samples[from..to],
        &uuid::Uuid::new_v4().to_string(),
        cancelled,
        deadline,
    )?;
    // Annotations and words the dictionary rejects stay out of the aligned text.
    let mut kept: Vec<usize> = (0..words.len()).filter(|&i| !non_speech[i]).collect();
    if kept.is_empty() {
        return Ok(result);
    }
    let proposal = loop {
        let text = kept
            .iter()
            .map(|&i| words[i].text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let mut interrupted =
            || cancelled.load(std::sync::atomic::Ordering::Acquire) || Instant::now() >= deadline;
        let outcome = propose(
            &emissions.log_probs,
            emissions.input_samples,
            &text,
            &refiner.models.lexicon,
            &mut RefinementControl::new(&mut interrupted),
        );
        let before = kept.len();
        match outcome {
            Ok(proposal) => break proposal,
            Err(AlignError::UnsupportedWord(word)) => {
                kept.retain(|&i| lexical(&words[i].text) != word);
            }
            Err(AlignError::Invalid("empty lexical word")) => {
                kept.retain(|&i| words[i].text.chars().any(char::is_alphabetic));
            }
            Err(AlignError::Cancelled) => {
                check(cancelled, deadline)?;
                return Ok(result);
            }
            Err(_) => return Ok(result),
        }
        if kept.len() == before || kept.is_empty() {
            return Ok(result);
        }
    };
    let shift = i64::from(from_cs as u32);
    for (word, &index) in proposal.words.iter().zip(&kept) {
        let start = i64::from(word.interval.start.0 / HOP as u32) + shift;
        let end = i64::from(word.interval.end.0.div_ceil(HOP as u32)) + shift;
        let original = &words[index];
        if end > start
            && (start - i64::from(original.start_cs)).abs() <= MAX_SHIFT_CS
            && (end - i64::from(original.end_cs)).abs() <= MAX_SHIFT_CS
        {
            result[index] = Some((start as u32, end as u32));
        }
    }
    Ok(result)
}

/// Replace word times by phoneme-aligned ones where the aligner places a word
/// near its recognizer time. `samples` is the analysis PCM `transcript` timed.
/// Only cancellation and the deadline are errors.
pub fn refine_transcript(
    refiner: &TranscriptRefiner,
    transcript: &Transcript,
    samples: &[f32],
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Transcript, RefinementError> {
    let words = transcript.words();
    let non_speech = transcript.non_speech();
    let mut refined = words.to_vec();
    let mut start = 0;
    while start < words.len() {
        check(cancelled, deadline)?;
        let end = window_end(words, start);
        match refine_window(
            refiner,
            samples,
            &words[start..end],
            &non_speech[start..end],
            cancelled,
            deadline,
        ) {
            Ok(times) => {
                for (offset, time) in times.into_iter().enumerate() {
                    if let Some((start_cs, end_cs)) = time {
                        refined[start + offset].start_cs = start_cs;
                        refined[start + offset].end_cs = end_cs;
                    }
                }
            }
            Err(error @ (RefinementError::Cancelled | RefinementError::Deadline)) => {
                return Err(error);
            }
            // A missing or failing aligner leaves the recognizer's times.
            Err(_) => {}
        }
        start = end;
    }
    // Words never overlap: an earlier word ends where a later one begins.
    for i in 1..refined.len() {
        if refined[i - 1].end_cs > refined[i].start_cs {
            refined[i - 1].end_cs = refined[i].start_cs.max(refined[i - 1].start_cs);
        }
        if refined[i].start_cs < refined[i - 1].start_cs {
            refined[i].start_cs = refined[i - 1].start_cs;
            refined[i].end_cs = refined[i].end_cs.max(refined[i].start_cs);
        }
    }
    Transcript::new(transcript.audio(), refined)
        .and_then(|value| value.with_recognized_again(transcript.recognized_again().to_vec()))
        .or_else(|_| Ok(transcript.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_analysis::Word;

    fn word(start_cs: u32, end_cs: u32) -> Word {
        Word {
            text: "a".into(),
            start_cs,
            end_cs,
            probability: 0.9,
            segment: 0,
        }
    }

    #[test]
    fn windows_cut_at_the_longest_late_gap_and_cover_every_word() {
        // 100 words, 30 cs each, with a 40 cs pause after word 33.
        let mut words = Vec::new();
        let mut t = 0;
        for i in 0..100 {
            words.push(word(t, t + 25));
            t += 30 + if i == 33 { 40 } else { 0 };
        }
        let mut start = 0;
        let mut cuts = Vec::new();
        while start < words.len() {
            let end = window_end(&words, start);
            assert!(end > start && end - start <= MAX_WORDS_PER_WINDOW);
            cuts.push(end);
            start = end;
        }
        assert_eq!(cuts[0], 34, "the pause is inside the last 15 words");
        assert_eq!(*cuts.last().unwrap(), 100);
    }

    #[test]
    fn a_long_span_splits_even_with_few_words() {
        let words: Vec<Word> = (0..10).map(|i| word(i * 500, i * 500 + 400)).collect();
        let end = window_end(&words, 0);
        assert!(words[end - 1].end_cs <= MAX_SPAN_CS);
    }
}
