//! Bounded, pure phoneme-CTC word boundary proposals.
//!
//! Frozen alpha-0.6 policy phoneme-prior-pilot-1 and iteration-one
//! phonetic-context-residual-1. Support is uncalibrated model output, never a
//! correctness probability. Every interval remains diagnostic when unavailable.
//! These values never accept a correction or mutate a document.
//!
//! Pronunciation alternatives form a DAG, rather than a Cartesian product.
//! Equal scores choose the first dictionary-order pronunciation tuple, then the
//! reference CTC stay/advance/skip and terminal-blank preferences.

mod alignment;
mod guard;
mod lexical;
mod selection;
#[cfg(test)]
mod tests;

pub use lexical::{Pronunciation, PronunciationLexicon, normalize_words};
pub use selection::*;
use serde::Serialize;
use thiserror::Error;

pub const REFINEMENT_RULE: &str = "phoneme-prior-pilot-1";
pub const CONTEXT_GUARD_RULE: &str = "phonetic-context-residual-1";
pub const SAMPLE_RATE: u32 = 16_000;
pub const OUTPUT_STEP: u32 = 320;
pub const MAX_INPUT_SAMPLES: u32 = 480_000;
pub const MAX_PHONES: usize = 256;
pub const MAX_WORDS: usize = 64;
pub const MAX_DATA_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_STATES: usize = 8_193;
pub const MAX_EDGES: usize = 65_536;
pub const MAX_TRACE_CELLS: usize = 16_000_000;
pub const MAX_VISITS: u64 = 128_000_000;
pub const LABELS: [&str; 41] = [
    "<blk>", "AA", "AE", "AH", "AO", "AW", "AY", "B", "CH", "D", "DH", "EH", "ER", "EY", "F", "G",
    "HH", "IH", "IY", "JH", "K", "L", "M", "N", "NG", "OW", "OY", "P", "R", "S", "SH", "T", "TH",
    "UH", "UW", "V", "W", "Y", "Z", "ZH", "<unk>",
];

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RefinementError {
    #[error("refinement exceeds its {0} limit")]
    Limit(&'static str),
    #[error("invalid refinement input: {0}")]
    Invalid(&'static str),
    #[error("dictionary has no pronunciation for {0}")]
    UnsupportedWord(String),
    #[error("CTC target cannot be aligned")]
    Unalignable,
    #[error("refinement cancelled")]
    Cancelled,
}

/// An exact coordinate in the captured mono 16 kHz context, never source samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AnalysisSample(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AnalysisInterval {
    pub start: AnalysisSample,
    pub end: AnalysisSample,
}

/// Caller-owned cooperative deadline/cancellation hook and cumulative work budget.
/// Return true to cancel. A caller may use an atomic flag and monotonic deadline.
pub struct RefinementControl<'a> {
    cancelled: &'a mut dyn FnMut() -> bool,
    remaining: u64,
}

impl<'a> RefinementControl<'a> {
    pub fn new(cancelled: &'a mut dyn FnMut() -> bool) -> Self {
        Self {
            cancelled,
            remaining: MAX_VISITS,
        }
    }

    /// A smaller budget is useful for scheduling and deterministic interruption tests.
    pub fn with_budget(cancelled: &'a mut dyn FnMut() -> bool, visits: u64) -> Self {
        Self {
            cancelled,
            remaining: visits.min(MAX_VISITS),
        }
    }

    pub(super) fn visit(&mut self, count: usize) -> Result<(), RefinementError> {
        if (self.cancelled)() {
            return Err(RefinementError::Cancelled);
        }
        self.remaining = self
            .remaining
            .checked_sub(count as u64)
            .ok_or(RefinementError::Limit("work visits"))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum UnavailableReason {
    WordSupportBelowThreshold,
    PhoneSupportBelowThreshold,
    EmptyInterval,
    ContextIncompleteOrPlacementAmbiguous,
}

/// Availability only permits showing an approximate offer for explicit review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ProposalAvailability {
    Approximate,
    Unavailable(Vec<UnavailableReason>),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PhoneSpan {
    pub label: u8,
    pub start_frame: u32,
    pub end_frame: u32,
    pub mean_posterior: f64,
    pub minimum_posterior: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WordProposal {
    pub index: usize,
    pub word: String,
    pub pronunciation: Pronunciation,
    pub phones: Vec<PhoneSpan>,
    pub start_frame: u32,
    pub end_frame: u32,
    pub interval: AnalysisInterval,
    pub mean_phone_frame_posterior: f64,
    pub minimum_phone_mean_posterior: f64,
    pub availability: ProposalAvailability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EditOperation {
    Match,
    Substitute,
    DeleteExpected,
    InsertObserved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditStep {
    pub operation: EditOperation,
    pub expected_index: Option<usize>,
    pub observed_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResidualRun {
    pub observed_indices: Vec<usize>,
    pub start_frame: u32,
    pub end_frame: u32,
    pub supported_frames: u32,
    pub minimum_mean_posterior: f64,
    pub qualifies: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContextGuard {
    pub greedy_phones: Vec<PhoneSpan>,
    pub selected_pronunciations: Vec<Pronunciation>,
    pub minimum_edit_distance: u32,
    pub alignment_steps: Vec<EditStep>,
    pub residual_runs: Vec<ResidualRun>,
    pub abstain: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefinementProposal {
    pub rule: &'static str,
    pub guard_rule: &'static str,
    pub words: Vec<WordProposal>,
    pub path_score: f64,
    pub path_score_per_frame: f64,
    pub frame_label_path: Vec<u8>,
    pub frame_blank_fraction: f64,
    pub context_guard: ContextGuard,
}

/// Align unchanged supplied text to a validated row-major [T,41] tensor.
/// T must be ceil((floor(input_samples/160)+1)/2), including centered frame zero.
/// Nonfinite/positive log probabilities, wrong dimensions, empty/OOV text,
/// excessive branching/work, and cancellation fail without partial offers.
pub fn propose(
    log_probs: &[f64],
    input_samples: u32,
    text: &str,
    lexicon: &PronunciationLexicon,
    control: &mut RefinementControl<'_>,
) -> Result<RefinementProposal, RefinementError> {
    control.visit(1)?;
    if !(401..=MAX_INPUT_SAMPLES).contains(&input_samples) {
        return Err(RefinementError::Invalid("audio sample bound"));
    }
    let frames = ((input_samples / 160 + 1).div_ceil(2)) as usize;
    if log_probs.len() != frames * LABELS.len() {
        return Err(RefinementError::Invalid("emission dimensions"));
    }
    for row in log_probs.chunks_exact(LABELS.len()) {
        control.visit(row.len())?;
        if row.iter().any(|p| !p.is_finite() || *p > 0.0) {
            return Err(RefinementError::Invalid(
                "nonfinite or positive log probability",
            ));
        }
    }
    let words = normalize_words(text)?;
    let choices = lexicon.lookup(&words)?;
    let graph = alignment::Graph::new(&choices)?;
    let aligned = alignment::align(log_probs, frames, &graph, control)?;
    let inspection = guard::inspect(log_probs, frames, &graph, &choices, control)?;
    let mut proposals = Vec::with_capacity(words.len());
    let mut offset = 0;
    for (index, word) in words.into_iter().enumerate() {
        let pronunciation = choices[index][aligned.choices[index]].clone();
        let phones = aligned.phones[offset..offset + pronunciation.labels.len()].to_vec();
        offset += phones.len();
        let start_frame = phones[0].start_frame;
        let end_frame = phones[phones.len() - 1].end_frame;
        let denominator: u32 = phones.iter().map(|p| p.end_frame - p.start_frame).sum();
        let mean = phones
            .iter()
            .map(|p| p.mean_posterior * f64::from(p.end_frame - p.start_frame))
            .sum::<f64>()
            / f64::from(denominator);
        let minimum = phones
            .iter()
            .map(|p| p.mean_posterior)
            .fold(f64::INFINITY, f64::min);
        let start = start_frame
            .checked_mul(OUTPUT_STEP)
            .ok_or(RefinementError::Limit("sample coordinate"))?;
        let end = end_frame
            .checked_mul(OUTPUT_STEP)
            .ok_or(RefinementError::Limit("sample coordinate"))?
            .min(input_samples);
        let mut reasons = Vec::new();
        // Preserve the reference base gate's first-reason priority.
        if mean < 0.5 {
            reasons.push(UnavailableReason::WordSupportBelowThreshold);
        } else if minimum < 0.1 {
            reasons.push(UnavailableReason::PhoneSupportBelowThreshold);
        }
        if start >= end {
            reasons.push(UnavailableReason::EmptyInterval);
        }
        if inspection.abstain {
            reasons.push(UnavailableReason::ContextIncompleteOrPlacementAmbiguous);
        }
        proposals.push(WordProposal {
            index,
            word,
            pronunciation,
            phones,
            start_frame,
            end_frame,
            interval: AnalysisInterval {
                start: AnalysisSample(start),
                end: AnalysisSample(end),
            },
            mean_phone_frame_posterior: mean,
            minimum_phone_mean_posterior: minimum,
            availability: if reasons.is_empty() {
                ProposalAvailability::Approximate
            } else {
                ProposalAvailability::Unavailable(reasons)
            },
        });
    }
    Ok(RefinementProposal {
        rule: REFINEMENT_RULE,
        guard_rule: CONTEXT_GUARD_RULE,
        words: proposals,
        path_score: aligned.score,
        path_score_per_frame: aligned.score / frames as f64,
        frame_blank_fraction: aligned.labels.iter().filter(|p| **p == 0).count() as f64
            / frames as f64,
        frame_label_path: aligned.labels,
        context_guard: inspection,
    })
}
