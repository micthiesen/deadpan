//! Ephemeral capture, review and explicit atomic acceptance of timing proposals.
//! The host owns source/session/revision and durable correction-version tokens.
//! This layer additionally pins the complete recognized and corrected analyses.

use super::{
    AnalysisInterval, MAX_INPUT_SAMPLES, MAX_WORDS, ProposalAvailability, RefinementError,
    RefinementProposal, UnavailableReason, normalize_words,
};
use crate::{
    CorrectedTranscript, CorrectedWord, CorrectionError, Corrections, Transcript, WordOrigin,
};
use serde::Serialize;
use std::ops::Range;
use thiserror::Error;

const SAMPLES_PER_CS: u64 = 160;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GlobalAnalysisSample(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GlobalAnalysisInterval {
    pub start: GlobalAnalysisSample,
    pub end: GlobalAnalysisSample,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CentisecondBounds {
    /// Explicit floor of the global analysis sample coordinate divided by 160.
    pub start_cs: u32,
    /// Explicit ceil of the global analysis sample coordinate divided by 160.
    pub end_cs: u32,
}

#[derive(Debug, Error, PartialEq)]
pub enum RefinementSelectionError {
    #[error("word refinement capture is invalid: {0}")]
    Invalid(&'static str),
    #[error("word refinement context is unavailable: {0}")]
    Context(&'static str),
    #[error("the captured transcript or corrections changed; request refinement again")]
    Stale,
    #[error("word {0} has no available refinement offer; nothing was accepted")]
    Unavailable(usize),
    #[error("accepting refinement would change an unselected or pinned word; nothing was accepted")]
    WouldChangeOtherWords,
    #[error(transparent)]
    Correction(#[from] CorrectionError),
    #[error(transparent)]
    Refinement(#[from] RefinementError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum OfferUnavailableReason {
    ManuallyPinned,
    Backend(Vec<UnavailableReason>),
    OutsideCapturedContext,
    CrossesPreviousWord,
    CrossesNextWord,
    CrossesAnotherSelectedProposal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum RefinementOfferAvailability {
    Approximate,
    Unavailable(Vec<OfferUnavailableReason>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefinementOffer {
    pub word_index: usize,
    pub raw_global_interval: Option<GlobalAnalysisInterval>,
    pub centisecond_bounds: Option<CentisecondBounds>,
    pub availability: RefinementOfferAvailability,
}

/// Not serialized: accepting requires this exact retained in-process capture.
#[derive(Debug, Clone)]
pub struct RefinementCapture {
    recognized: Transcript,
    current: CorrectedTranscript,
    corrections: Corrections,
    selected: Range<usize>,
    context_words: Range<usize>,
    context_text: String,
    window: GlobalAnalysisInterval,
    full_analysis_samples: u64,
}

/// The proposal is owned and immutable after review. Unavailable raw spans are
/// still accessible through diagnostics, never silently promoted by acceptance.
#[derive(Debug, Clone)]
pub struct RefinementReview {
    capture: RefinementCapture,
    diagnostics: RefinementProposal,
    offers: Vec<RefinementOffer>,
}

impl RefinementCapture {
    pub fn new(
        recognized_base: &Transcript,
        current: &CorrectedTranscript,
        corrections: &Corrections,
        selected: Range<usize>,
        full_analysis_samples: u64,
    ) -> Result<Self, RefinementSelectionError> {
        let visible = current.transcript.words();
        if selected.is_empty() || selected.start > selected.end || selected.end > visible.len() {
            return Err(RefinementSelectionError::Invalid("selected word range"));
        }
        if current.origins.len() != visible.len()
            || current.skipped != 0
            || corrections.clock() != Corrections::clock_of(recognized_base)
            || corrections.apply_to_transcript(recognized_base)? != *current
        {
            return Err(RefinementSelectionError::Invalid(
                "corrected transcript provenance",
            ));
        }
        let audio = current.transcript.audio();
        if full_analysis_samples == 0
            || full_analysis_samples.div_ceil(SAMPLES_PER_CS) != u64::from(audio.duration_cs)
        {
            return Err(RefinementSelectionError::Invalid(
                "exact full analysis extent",
            ));
        }
        // Keep complete selected segments and one complete neighboring segment
        // on either side. Never truncate a long or overlapping context to fit.
        let mut start = selected.start;
        let mut end = selected.end;
        let first_segment = visible[start].segment;
        let last_segment = visible[end - 1].segment;
        while start > 0 && visible[start - 1].segment == first_segment {
            start -= 1;
        }
        while end < visible.len() && visible[end].segment == last_segment {
            end += 1;
        }
        if start > 0 {
            let segment = visible[start - 1].segment;
            while start > 0 && visible[start - 1].segment == segment {
                start -= 1;
            }
        }
        if end < visible.len() {
            let segment = visible[end].segment;
            while end < visible.len() && visible[end].segment == segment {
                end += 1;
            }
        }
        if end - start > MAX_WORDS {
            return Err(RefinementSelectionError::Context(
                "complete neighboring segments exceed 64 words",
            ));
        }
        let from = if start == 0 {
            0
        } else {
            let previous = &visible[start - 1];
            let first = &visible[start];
            if previous.end_cs > first.start_cs {
                return Err(RefinementSelectionError::Context(
                    "overlapping words at the leading context seam",
                ));
            }
            (u64::from(previous.end_cs) + u64::from(first.start_cs)) / 2 * SAMPLES_PER_CS
        };
        let to = if end == visible.len() {
            full_analysis_samples
        } else {
            let last = &visible[end - 1];
            let next = &visible[end];
            if last.end_cs > next.start_cs {
                return Err(RefinementSelectionError::Context(
                    "overlapping words at the trailing context seam",
                ));
            }
            (u64::from(last.end_cs) + u64::from(next.start_cs)) / 2 * SAMPLES_PER_CS
        };
        let length = to
            .checked_sub(from)
            .ok_or(RefinementSelectionError::Context(
                "invalid context sample range",
            ))?;
        if !(401..=u64::from(MAX_INPUT_SAMPLES)).contains(&length) {
            return Err(RefinementSelectionError::Context(
                "complete context must be more than 400 samples and at most 30 seconds",
            ));
        }
        if visible[start..end].iter().any(|w| {
            u64::from(w.start_cs) * SAMPLES_PER_CS < from
                || u64::from(w.start_cs) * SAMPLES_PER_CS >= to
                || u64::from(w.end_cs) > to.div_ceil(SAMPLES_PER_CS)
        }) {
            return Err(RefinementSelectionError::Context(
                "a visible word lies outside the exact context",
            ));
        }
        let context_text = visible[start..end]
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if normalize_words(&context_text)?.len() != end - start {
            return Err(RefinementSelectionError::Invalid(
                "one lexical token is required per visible word",
            ));
        }
        Ok(Self {
            recognized: recognized_base.clone(),
            current: current.clone(),
            corrections: corrections.clone(),
            selected,
            context_words: start..end,
            context_text,
            window: GlobalAnalysisInterval {
                start: GlobalAnalysisSample(from),
                end: GlobalAnalysisSample(to),
            },
            full_analysis_samples,
        })
    }

    pub fn selected_words(&self) -> Range<usize> {
        self.selected.clone()
    }
    pub fn context_words(&self) -> Range<usize> {
        self.context_words.clone()
    }
    pub fn context_text(&self) -> &str {
        &self.context_text
    }
    pub fn window(&self) -> GlobalAnalysisInterval {
        self.window
    }
    pub fn input_samples(&self) -> u32 {
        (self.window.end.0 - self.window.start.0) as u32
    }

    fn matches(
        &self,
        base: &Transcript,
        current: &CorrectedTranscript,
        corrections: &Corrections,
    ) -> bool {
        self.recognized == *base && self.current == *current && self.corrections == *corrections
    }

    fn map(
        &self,
        interval: AnalysisInterval,
    ) -> Option<(GlobalAnalysisInterval, CentisecondBounds)> {
        let length = u64::from(self.input_samples());
        if interval.start.0 >= interval.end.0 || u64::from(interval.end.0) > length {
            return None;
        }
        // Add the retained global analysis offset before any conversion.
        // Original source sample rate/origin never participates in this math.
        let start = self
            .window
            .start
            .0
            .checked_add(u64::from(interval.start.0))?;
        let end = self.window.start.0.checked_add(u64::from(interval.end.0))?;
        if start < self.window.start.0
            || end > self.window.end.0
            || end > self.full_analysis_samples
        {
            return None;
        }
        Some((
            GlobalAnalysisInterval {
                start: GlobalAnalysisSample(start),
                end: GlobalAnalysisSample(end),
            },
            CentisecondBounds {
                start_cs: u32::try_from(start / SAMPLES_PER_CS).ok()?,
                end_cs: u32::try_from(end.div_ceil(SAMPLES_PER_CS)).ok()?,
            },
        ))
    }

    pub fn review(
        self,
        proposal: RefinementProposal,
    ) -> Result<RefinementReview, RefinementSelectionError> {
        let words = self.current.transcript.words();
        let lexical = normalize_words(&self.context_text)?;
        if proposal.words.len() != lexical.len()
            || proposal
                .words
                .iter()
                .zip(&lexical)
                .enumerate()
                .any(|(i, (word, text))| word.index != i || word.word != *text)
            || proposal.frame_label_path.len()
                != ((self.input_samples() / 160 + 1).div_ceil(2)) as usize
        {
            return Err(RefinementSelectionError::Invalid(
                "proposal does not describe the captured context",
            ));
        }
        let mut offers = Vec::with_capacity(self.selected.len());
        for index in self.selected.clone() {
            let word = &proposal.words[index - self.context_words.start];
            let mapped = self.map(word.interval);
            let mut reasons = Vec::new();
            if self.current.corrected(index) {
                reasons.push(OfferUnavailableReason::ManuallyPinned);
            }
            if let ProposalAvailability::Unavailable(reasons_from_backend) = &word.availability {
                reasons.push(OfferUnavailableReason::Backend(
                    reasons_from_backend.clone(),
                ));
            }
            if let Some((global, bounds)) = mapped {
                if let Some(previous) = index.checked_sub(1).map(|i| &words[i])
                    && (global.start.0 < u64::from(previous.end_cs) * SAMPLES_PER_CS
                        || bounds.start_cs < previous.end_cs)
                {
                    reasons.push(OfferUnavailableReason::CrossesPreviousWord);
                }
                if let Some(next) = words.get(index + 1)
                    && (global.end.0 > u64::from(next.start_cs) * SAMPLES_PER_CS
                        || bounds.end_cs > next.start_cs)
                {
                    reasons.push(OfferUnavailableReason::CrossesNextWord);
                }
            } else {
                reasons.push(OfferUnavailableReason::OutsideCapturedContext);
            }
            offers.push(RefinementOffer {
                word_index: index,
                raw_global_interval: mapped.map(|m| m.0),
                centisecond_bounds: mapped.map(|m| m.1),
                availability: if reasons.is_empty() {
                    RefinementOfferAvailability::Approximate
                } else {
                    RefinementOfferAvailability::Unavailable(reasons)
                },
            });
        }
        // No subset of simultaneous acceptance can introduce an offer crossing.
        for left in 0..offers.len().saturating_sub(1) {
            let right = left + 1;
            if let (Some(a), Some(b)) = (
                offers[left].raw_global_interval,
                offers[right].raw_global_interval,
            ) && a.end.0 > b.start.0
            {
                for index in [left, right] {
                    match &mut offers[index].availability {
                        RefinementOfferAvailability::Approximate => {
                            offers[index].availability =
                                RefinementOfferAvailability::Unavailable(vec![
                                    OfferUnavailableReason::CrossesAnotherSelectedProposal,
                                ]);
                        }
                        RefinementOfferAvailability::Unavailable(reasons) => {
                            reasons.push(OfferUnavailableReason::CrossesAnotherSelectedProposal)
                        }
                    }
                }
            }
        }
        Ok(RefinementReview {
            capture: self,
            diagnostics: proposal,
            offers,
        })
    }
}

impl RefinementReview {
    pub fn capture(&self) -> &RefinementCapture {
        &self.capture
    }
    pub fn diagnostics(&self) -> &RefinementProposal {
        &self.diagnostics
    }
    pub fn offers(&self) -> &[RefinementOffer] {
        &self.offers
    }

    /// Explicitly accept a nonempty subset of available global word indices.
    /// One unavailable, stale or invalid selection refuses the entire operation.
    pub fn accept(
        &self,
        recognized_base: &Transcript,
        current: &CorrectedTranscript,
        corrections: &Corrections,
        selected_offers: &[usize],
    ) -> Result<Corrections, RefinementSelectionError> {
        if !self.capture.matches(recognized_base, current, corrections) {
            return Err(RefinementSelectionError::Stale);
        }
        if selected_offers.is_empty() || selected_offers.len() > self.offers.len() {
            return Err(RefinementSelectionError::Invalid(
                "accepted offer selection",
            ));
        }
        let mut chosen = selected_offers.to_vec();
        chosen.sort_unstable();
        if chosen.windows(2).any(|p| p[0] == p[1]) {
            return Err(RefinementSelectionError::Invalid("duplicate accepted word"));
        }
        for &index in &chosen {
            let offer = self
                .offers
                .iter()
                .find(|offer| offer.word_index == index)
                .ok_or(RefinementSelectionError::Unavailable(index))?;
            if offer.availability != RefinementOfferAvailability::Approximate
                || current.corrected(index)
            {
                return Err(RefinementSelectionError::Unavailable(index));
            }
        }
        let visible = current.transcript.words();
        let first = chosen[0];
        let last = chosen[chosen.len() - 1] + 1;
        let mut replacement: Vec<_> = visible[first..last]
            .iter()
            .map(|word| CorrectedWord {
                text: word.text.clone(),
                start_cs: word.start_cs,
                end_cs: word.end_cs,
            })
            .collect();
        for &index in &chosen {
            let bounds = self
                .offers
                .iter()
                .find(|o| o.word_index == index)
                .and_then(|o| o.centisecond_bounds)
                .ok_or(RefinementSelectionError::Unavailable(index))?;
            replacement[index - first].start_cs = bounds.start_cs;
            replacement[index - first].end_cs = bounds.end_cs;
        }
        let changed = corrections.replace_words(current, first..last, replacement)?;
        let after = changed.apply_to_transcript(recognized_base)?;
        if after.skipped != current.skipped
            || after.transcript.words().len() != visible.len()
            || changed.pause_regions() != corrections.pause_regions()
            || corrections
                .word_regions()
                .iter()
                .any(|region| !changed.word_regions().contains(region))
        {
            return Err(RefinementSelectionError::WouldChangeOtherWords);
        }
        for (index, before) in visible.iter().enumerate() {
            let next = &after.transcript.words()[index];
            if !chosen.contains(&index)
                && (next != before || after.corrected(index) != current.corrected(index))
            {
                return Err(RefinementSelectionError::WouldChangeOtherWords);
            }
            if next.text != before.text {
                return Err(RefinementSelectionError::WouldChangeOtherWords);
            }
            if let WordOrigin::Corrected(region) = current.origins[index] {
                let Some(WordOrigin::Corrected(next_region)) = after.origins.get(index) else {
                    return Err(RefinementSelectionError::WouldChangeOtherWords);
                };
                if corrections.word_regions().get(region)
                    != changed.word_regions().get(*next_region)
                {
                    return Err(RefinementSelectionError::WouldChangeOtherWords);
                }
            }
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests;
