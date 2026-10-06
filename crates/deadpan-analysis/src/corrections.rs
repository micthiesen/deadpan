//! Manual corrections of the transcript and of detected pauses.
//!
//! Recognized words and detected pauses are rebuildable proposals. A person's
//! corrections are kept separately, as corrected regions of the analysis
//! clock, and applied over whichever proposal is current, so transcribing or
//! detecting again never loses them. [`CORRECTION_RULE`] names how:
//!
//! 1. A word region `[start, end)` (centiseconds) owns its words. A recognized
//!    word conflicts with a region when it overlaps it (a zero-length word
//!    occupies its one centisecond); every conflicting recognized word is
//!    replaced by the region's words, so a recognized word that straddles a
//!    corrected edge after a new transcription is dropped and counted.
//! 2. A corrected word takes the sentence of the first recognized word its
//!    region replaced, or else of the word before it.
//! 3. A pause region `[start, end)` (analysis samples) owns pause time inside
//!    it. Detected pauses are clipped to outside every region; a clipped
//!    remainder shorter than the shortest detected pause (150 ms) is dropped.
//!    The region's pauses are added and pauses that touch merge.
//! 4. Corrections apply only to analyses of the same Original audio clock
//!    (origin and sample rate); a region beyond the analysed audio is skipped
//!    and counted.
//!
//! Every edit is made against the corrected analysis a person sees and turned
//! into one new region: the hull of the changed items, grown until no other
//! region or visible item straddles it. Items inside a region are pinned.

use std::ops::Range;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ENERGY_HOP, MAX_ACTIVITY_SAMPLES, MAX_WORD_BYTES, MIN_PAUSE_SAMPLES, Pause, SpeechActivity,
    Transcript, TranscriptError, Word,
};

/// The versioned rule implemented by [`Corrections`].
pub const CORRECTION_RULE: &str = "deadpan-corrections-1";
/// The versioned rule implemented by [`measured_edges`].
pub const EDGE_RULE: &str = "deadpan-edges-1";
pub const MAX_CORRECTION_REGIONS: usize = 20_000;
pub const MAX_CORRECTED_WORDS: usize = 40_000;
pub const MAX_CORRECTED_PAUSES: usize = 40_000;
/// Analysis samples per centisecond: exactly one energy frame.
pub const CENTISECOND_SAMPLES: u64 = ENERGY_HOP;
/// An energy change of at least 9 dB between adjacent 10 ms frames is a
/// measured edge (half-decibel steps).
const EDGE_STEPS: i32 = 18;

#[derive(Debug, Error, PartialEq)]
pub enum CorrectionError {
    #[error("corrections exceed their {0} limit")]
    Limit(&'static str),
    #[error("correction is invalid: {0}")]
    Invalid(&'static str),
    #[error("correction does not apply: {0}")]
    Refused(&'static str),
    #[error(transparent)]
    Transcript(#[from] TranscriptError),
}

/// The analysis clock corrections are written in: the Original audio sample
/// where analysis PCM begins and the Original's audio sample rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectionClock {
    pub origin: i64,
    pub sample_rate: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectedWord {
    pub text: String,
    pub start_cs: u32,
    pub end_cs: u32,
}

/// Words a person set for one stretch of the analysis clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WordCorrection {
    pub start_cs: u32,
    pub end_cs: u32,
    pub words: Vec<CorrectedWord>,
}

/// Pauses a person set for one stretch of the analysis clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauseCorrection {
    pub start: u64,
    pub end: u64,
    pub pauses: Vec<Pause>,
}

/// Validated manual corrections of one Original's audio analyses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "CorrectionsRecord", into = "CorrectionsRecord")]
pub struct Corrections {
    clock: CorrectionClock,
    words: Vec<WordCorrection>,
    pauses: Vec<PauseCorrection>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorrectionsRecord {
    rule: String,
    clock: CorrectionClock,
    words: Vec<WordCorrection>,
    pauses: Vec<PauseCorrection>,
}

impl TryFrom<CorrectionsRecord> for Corrections {
    type Error = CorrectionError;

    fn try_from(record: CorrectionsRecord) -> Result<Self, Self::Error> {
        if record.rule != CORRECTION_RULE {
            return Err(CorrectionError::Invalid("unknown correction rule"));
        }
        Self::new(record.clock, record.words, record.pauses)
    }
}

impl From<Corrections> for CorrectionsRecord {
    fn from(corrections: Corrections) -> Self {
        Self {
            rule: CORRECTION_RULE.to_owned(),
            clock: corrections.clock,
            words: corrections.words,
            pauses: corrections.pauses,
        }
    }
}

/// Where a corrected transcript's word came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordOrigin {
    /// The recognizer's word at this index.
    Recognized(usize),
    /// A word of the correction region at this index.
    Corrected(usize),
}

/// The transcript with corrections applied.
#[derive(Debug, Clone, PartialEq)]
pub struct CorrectedTranscript {
    pub transcript: Transcript,
    pub origins: Vec<WordOrigin>,
    /// Recognized words replaced by corrections.
    pub replaced: usize,
    /// Regions that could not apply to this transcript.
    pub skipped: usize,
}

impl CorrectedTranscript {
    /// The recognizer's transcript with no corrections.
    pub fn recognized(transcript: Transcript) -> Self {
        let origins = (0..transcript.words().len())
            .map(WordOrigin::Recognized)
            .collect();
        Self {
            transcript,
            origins,
            replaced: 0,
            skipped: 0,
        }
    }

    pub fn corrected(&self, word: usize) -> bool {
        matches!(self.origins.get(word), Some(WordOrigin::Corrected(_)))
    }
}

/// Detected pauses with corrections applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrectedPauses {
    pub pauses: Vec<Pause>,
    /// Whether each pause contains corrected pause time.
    pub corrected: Vec<bool>,
    pub skipped: usize,
}

impl CorrectedPauses {
    pub fn detected(activity: &SpeechActivity) -> Self {
        let pauses = activity.pauses();
        Self {
            corrected: vec![false; pauses.len()],
            pauses,
            skipped: 0,
        }
    }
}

impl Corrections {
    pub fn new(
        clock: CorrectionClock,
        words: Vec<WordCorrection>,
        pauses: Vec<PauseCorrection>,
    ) -> Result<Self, CorrectionError> {
        if clock.sample_rate == 0 {
            return Err(CorrectionError::Invalid("zero sample rate"));
        }
        if words.len() > MAX_CORRECTION_REGIONS || pauses.len() > MAX_CORRECTION_REGIONS {
            return Err(CorrectionError::Limit("region count"));
        }
        let mut corrected_words = 0_usize;
        let mut previous_end = 0_u32;
        for (index, region) in words.iter().enumerate() {
            if region.start_cs >= region.end_cs {
                return Err(CorrectionError::Invalid("empty word region"));
            }
            if index > 0 && region.start_cs < previous_end {
                return Err(CorrectionError::Invalid(
                    "word regions overlap or are unordered",
                ));
            }
            previous_end = region.end_cs;
            corrected_words += region.words.len();
            if corrected_words > MAX_CORRECTED_WORDS {
                return Err(CorrectionError::Limit("corrected word count"));
            }
            let mut floor = region.start_cs;
            for word in &region.words {
                check_text(&word.text)?;
                if word.start_cs > word.end_cs
                    || word.start_cs < floor
                    || word.end_cs > region.end_cs
                {
                    return Err(CorrectionError::Invalid(
                        "corrected word outside its region or out of order",
                    ));
                }
                if !conflicts(word.start_cs, word.end_cs, region.start_cs, region.end_cs) {
                    return Err(CorrectionError::Invalid(
                        "corrected word at its region's end",
                    ));
                }
                floor = word.start_cs;
            }
        }
        let mut corrected_pauses = 0_usize;
        let mut previous_end = 0_u64;
        for (index, region) in pauses.iter().enumerate() {
            if region.start >= region.end || region.end > MAX_ACTIVITY_SAMPLES {
                return Err(CorrectionError::Invalid("empty or oversized pause region"));
            }
            if index > 0 && region.start < previous_end {
                return Err(CorrectionError::Invalid(
                    "pause regions overlap or are unordered",
                ));
            }
            previous_end = region.end;
            corrected_pauses += region.pauses.len();
            if corrected_pauses > MAX_CORRECTED_PAUSES {
                return Err(CorrectionError::Limit("corrected pause count"));
            }
            let mut floor: Option<u64> = None;
            for pause in &region.pauses {
                if pause.start >= pause.end
                    || pause.start < region.start
                    || pause.end > region.end
                    || floor.is_some_and(|end| pause.start <= end)
                {
                    return Err(CorrectionError::Invalid(
                        "corrected pause outside its region, empty or touching another",
                    ));
                }
                floor = Some(pause.end);
            }
        }
        Ok(Self {
            clock,
            words,
            pauses,
        })
    }

    pub fn empty(clock: CorrectionClock) -> Self {
        Self {
            clock,
            words: Vec::new(),
            pauses: Vec::new(),
        }
    }

    pub fn clock(&self) -> CorrectionClock {
        self.clock
    }

    pub fn word_regions(&self) -> &[WordCorrection] {
        &self.words
    }

    pub fn pause_regions(&self) -> &[PauseCorrection] {
        &self.pauses
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.pauses.is_empty()
    }

    /// The clock of a transcript, for its first correction.
    pub fn clock_of(transcript: &Transcript) -> CorrectionClock {
        CorrectionClock {
            origin: transcript.audio().origin,
            sample_rate: transcript.audio().sample_rate,
        }
    }

    /// The clock of speech activity, for its first correction.
    pub fn clock_of_activity(activity: &SpeechActivity) -> CorrectionClock {
        CorrectionClock {
            origin: activity.audio().origin,
            sample_rate: activity.audio().sample_rate,
        }
    }

    fn same_clock(&self, origin: i64, sample_rate: u32) -> bool {
        self.clock.origin == origin && self.clock.sample_rate == sample_rate
    }

    /// These corrections without the regions that do not apply to the
    /// current analyses (another analysis clock, or beyond the analysed
    /// audio): what applying them uses, made explicit so a person can drop
    /// the rest. The clock follows the transcript, else the activity.
    pub fn applicable_to(
        &self,
        transcript: Option<&Transcript>,
        activity: Option<&SpeechActivity>,
    ) -> Self {
        let clock = transcript
            .map(Self::clock_of)
            .or_else(|| activity.map(Self::clock_of_activity))
            .unwrap_or(self.clock);
        if clock != self.clock {
            return Self::empty(clock);
        }
        let same = |origin: i64, rate: u32| self.same_clock(origin, rate);
        let words = transcript
            .filter(|transcript| same(transcript.audio().origin, transcript.audio().sample_rate))
            .map_or_else(Vec::new, |transcript| {
                self.words
                    .iter()
                    .filter(|region| region.end_cs <= transcript.audio().duration_cs)
                    .cloned()
                    .collect()
            });
        let pauses = activity
            .filter(|activity| same(activity.audio().origin, activity.audio().sample_rate))
            .map_or_else(Vec::new, |activity| {
                self.pauses
                    .iter()
                    .filter(|region| region.end <= activity.audio().samples)
                    .cloned()
                    .collect()
            });
        Self {
            clock,
            words,
            pauses,
        }
    }

    /// The recognizer's transcript with these corrections applied.
    pub fn apply_to_transcript(
        &self,
        proposal: &Transcript,
    ) -> Result<CorrectedTranscript, CorrectionError> {
        let audio = proposal.audio();
        if !self.same_clock(audio.origin, audio.sample_rate) {
            let mut result = CorrectedTranscript::recognized(proposal.clone());
            result.skipped = self.words.len();
            return Ok(result);
        }
        let regions: Vec<(usize, &WordCorrection)> = self
            .words
            .iter()
            .enumerate()
            .filter(|(_, region)| region.end_cs <= audio.duration_cs)
            .collect();
        let skipped = self.words.len() - regions.len();
        let recognized = proposal.words();
        // The region each recognized word conflicts with, if any.
        let owner = |word: &Word| -> Option<usize> {
            let end = word.end_cs.max(word.start_cs.saturating_add(1));
            let first = regions.partition_point(|(_, region)| region.end_cs <= word.start_cs);
            regions
                .get(first)
                .filter(|(_, region)| region.start_cs < end)
                .map(|_| first)
        };
        // The sentence of the first recognized word each region replaced.
        let mut region_segment: Vec<Option<u32>> = vec![None; regions.len()];
        let mut replaced = 0;
        let mut kept: Vec<usize> = Vec::new();
        for (index, word) in recognized.iter().enumerate() {
            match owner(word) {
                Some(region) => {
                    replaced += 1;
                    region_segment[region].get_or_insert(word.segment);
                }
                None => kept.push(index),
            }
        }
        let mut words = Vec::with_capacity(kept.len());
        let mut origins = Vec::with_capacity(kept.len());
        let mut segment = 0_u32;
        let mut next_kept = kept.into_iter().peekable();
        for (position, (index, region)) in regions.iter().enumerate() {
            while let Some(&word) = next_kept.peek() {
                if recognized[word].start_cs >= region.start_cs {
                    break;
                }
                segment = segment.max(recognized[word].segment);
                words.push(recognized[word].clone());
                origins.push(WordOrigin::Recognized(word));
                next_kept.next();
            }
            segment = segment.max(region_segment[position].unwrap_or(segment));
            for corrected in &region.words {
                words.push(Word {
                    text: corrected.text.clone(),
                    start_cs: corrected.start_cs,
                    end_cs: corrected.end_cs,
                    probability: 1.0,
                    segment,
                });
                origins.push(WordOrigin::Corrected(*index));
            }
        }
        for word in next_kept {
            words.push(recognized[word].clone());
            origins.push(WordOrigin::Recognized(word));
        }
        Ok(CorrectedTranscript {
            transcript: Transcript::new(audio, words)?,
            origins,
            replaced,
            skipped,
        })
    }

    /// Detected pauses with these corrections applied.
    pub fn apply_to_pauses(&self, activity: &SpeechActivity) -> CorrectedPauses {
        let audio = activity.audio();
        if !self.same_clock(audio.origin, audio.sample_rate) {
            let mut result = CorrectedPauses::detected(activity);
            result.skipped = self.pauses.len();
            return result;
        }
        let regions: Vec<&PauseCorrection> = self
            .pauses
            .iter()
            .filter(|region| region.end <= audio.samples)
            .collect();
        let skipped = self.pauses.len() - regions.len();
        let mut pieces: Vec<(Pause, bool)> = Vec::new();
        for pause in activity.pauses() {
            let mut from = pause.start;
            let first = regions.partition_point(|region| region.end <= pause.start);
            for region in &regions[first..] {
                if region.start >= pause.end {
                    break;
                }
                if region.start > from {
                    pieces.push((
                        Pause {
                            start: from,
                            end: region.start,
                        },
                        false,
                    ));
                }
                from = from.max(region.end);
            }
            if from < pause.end {
                pieces.push((
                    Pause {
                        start: from,
                        end: pause.end,
                    },
                    false,
                ));
            }
        }
        pieces.retain(|(pause, _)| pause.end - pause.start >= MIN_PAUSE_SAMPLES);
        pieces.extend(
            regions
                .iter()
                .flat_map(|region| region.pauses.iter().map(|pause| (*pause, true))),
        );
        pieces.sort_by_key(|(pause, _)| (pause.start, pause.end));
        let mut pauses: Vec<Pause> = Vec::with_capacity(pieces.len());
        let mut corrected: Vec<bool> = Vec::with_capacity(pieces.len());
        for (pause, manual) in pieces {
            match (pauses.last_mut(), corrected.last_mut()) {
                (Some(last), Some(flag)) if pause.start <= last.end => {
                    last.end = last.end.max(pause.end);
                    *flag |= manual;
                }
                _ => {
                    pauses.push(pause);
                    corrected.push(manual);
                }
            }
        }
        CorrectedPauses {
            pauses,
            corrected,
            skipped,
        }
    }

    /// Replace the corrected transcript's words `range` with `words`, as one
    /// new correction region. `current` must be this value applied to the
    /// current recognized transcript.
    pub fn replace_words(
        &self,
        current: &CorrectedTranscript,
        range: Range<usize>,
        words: Vec<CorrectedWord>,
    ) -> Result<Self, CorrectionError> {
        let audio = current.transcript.audio();
        if !self.same_clock(audio.origin, audio.sample_rate) {
            return Err(CorrectionError::Refused(
                "the corrections belong to a different analysis of the audio",
            ));
        }
        let visible = current.transcript.words();
        if range.start > range.end || range.end > visible.len() {
            return Err(CorrectionError::Invalid("word range"));
        }
        if range.is_empty() && words.is_empty() {
            return Err(CorrectionError::Invalid("nothing changes"));
        }
        // Words stay in order; recognized words may overlap a little, so
        // order is by start.
        let mut floor = range
            .start
            .checked_sub(1)
            .map_or(0, |previous| visible[previous].start_cs);
        for word in &words {
            check_text(&word.text)?;
            if word.start_cs > word.end_cs || word.start_cs < floor {
                return Err(CorrectionError::Refused(
                    "the word would begin before the word before it",
                ));
            }
            floor = word.start_cs;
        }
        if let Some(next) = visible.get(range.end)
            && floor > next.start_cs
        {
            return Err(CorrectionError::Refused(
                "the word would begin after the word after it",
            ));
        }
        if words.iter().any(|word| word.end_cs > audio.duration_cs) {
            return Err(CorrectionError::Refused("the word ends after the audio"));
        }
        // The new visible words.
        let mut edited: Vec<CorrectedWord> =
            visible[..range.start].iter().map(corrected_word).collect();
        edited.extend(words.iter().cloned());
        edited.extend(visible[range.end..].iter().map(corrected_word));
        let changed = visible[range.clone()]
            .iter()
            .map(|word| (word.start_cs, word.end_cs))
            .chain(words.iter().map(|word| (word.start_cs, word.end_cs)));
        let (mut start, mut end) = changed.fold((u32::MAX, 0), |(low, high), (from, to)| {
            (low.min(from), high.max(to.max(from.saturating_add(1))))
        });
        if end > audio.duration_cs {
            end = audio.duration_cs;
            start = start.min(end.saturating_sub(1));
        }
        if start >= end {
            return Err(CorrectionError::Refused("the analysed audio is empty"));
        }
        // Grow until no region or visible word straddles the new region.
        loop {
            let mut grown = (start, end);
            for region in &self.words {
                if region.start_cs < grown.1 && grown.0 < region.end_cs {
                    grown = (grown.0.min(region.start_cs), grown.1.max(region.end_cs));
                }
            }
            for word in &edited {
                let word_end = word.end_cs.max(word.start_cs.saturating_add(1));
                if conflicts(word.start_cs, word.end_cs, grown.0, grown.1) {
                    grown = (grown.0.min(word.start_cs), grown.1.max(word_end));
                }
            }
            grown.1 = grown.1.min(audio.duration_cs);
            if grown == (start, end) {
                break;
            }
            (start, end) = grown;
        }
        // A zero-length word at the very end of the analysed audio occupies
        // no centisecond inside it, so no region can own it: refuse rather
        // than drop it.
        if words
            .iter()
            .any(|word| !conflicts(word.start_cs, word.end_cs, start, end))
        {
            return Err(CorrectionError::Refused(
                "a zero-length word cannot sit at the very end of the analysed audio",
            ));
        }
        let region = WordCorrection {
            start_cs: start,
            end_cs: end,
            words: edited
                .into_iter()
                .filter(|word| conflicts(word.start_cs, word.end_cs, start, end))
                .collect(),
        };
        let mut regions: Vec<WordCorrection> = self
            .words
            .iter()
            .filter(|existing| !(existing.start_cs < end && start < existing.end_cs))
            .cloned()
            .collect();
        let at = regions.partition_point(|existing| existing.start_cs < start);
        regions.insert(at, region);
        Self::new(self.clock, regions, self.pauses.clone())
    }

    /// Replace the corrected pauses `range` with `pauses`, as one new region.
    pub fn replace_pauses(
        &self,
        activity: &SpeechActivity,
        current: &CorrectedPauses,
        range: Range<usize>,
        pauses: Vec<Pause>,
    ) -> Result<Self, CorrectionError> {
        let audio = activity.audio();
        if !self.same_clock(audio.origin, audio.sample_rate) {
            return Err(CorrectionError::Refused(
                "the corrections belong to a different analysis of the audio",
            ));
        }
        let visible = &current.pauses;
        if range.start > range.end || range.end > visible.len() {
            return Err(CorrectionError::Invalid("pause range"));
        }
        if range.is_empty() && pauses.is_empty() {
            return Err(CorrectionError::Invalid("nothing changes"));
        }
        for pair in pauses.windows(2) {
            if pair[1].start < pair[0].end {
                return Err(CorrectionError::Invalid("new pauses overlap"));
            }
        }
        if pauses
            .iter()
            .any(|pause| pause.start >= pause.end || pause.end > audio.samples)
        {
            return Err(CorrectionError::Refused(
                "a pause must be nonempty and inside the analysed audio",
            ));
        }
        let mut edited: Vec<Pause> = visible[..range.start].to_vec();
        edited.extend(pauses.iter().copied());
        edited.extend(visible[range.end..].iter().copied());
        edited.sort_by_key(|pause| (pause.start, pause.end));
        let edited = merge_touching(edited);
        let (mut start, mut end) = visible[range.clone()]
            .iter()
            .chain(pauses.iter())
            .fold((u64::MAX, 0), |(low, high), pause| {
                (low.min(pause.start), high.max(pause.end))
            });
        loop {
            let mut grown = (start, end);
            for region in &self.pauses {
                if region.start < grown.1 && grown.0 < region.end {
                    grown = (grown.0.min(region.start), grown.1.max(region.end));
                }
            }
            for pause in &edited {
                if pause.start < grown.1 && grown.0 < pause.end {
                    grown = (grown.0.min(pause.start), grown.1.max(pause.end));
                }
            }
            if grown == (start, end) {
                break;
            }
            (start, end) = grown;
        }
        let region = PauseCorrection {
            start,
            end,
            pauses: edited
                .into_iter()
                .filter(|pause| pause.start < end && start < pause.end)
                .collect(),
        };
        let mut regions: Vec<PauseCorrection> = self
            .pauses
            .iter()
            .filter(|existing| !(existing.start < end && start < existing.end))
            .cloned()
            .collect();
        let at = regions.partition_point(|existing| existing.start < start);
        regions.insert(at, region);
        Self::new(self.clock, self.words.clone(), regions)
    }

    /// Set a corrected word's text. Several words split its time in proportion
    /// to their letters, each split moved to the nearest measured edge within
    /// 80 ms when one lies inside the word; empty text deletes the word.
    pub fn edit_word_text(
        &self,
        current: &CorrectedTranscript,
        word: usize,
        text: &str,
        edges_cs: &[u32],
    ) -> Result<Self, CorrectionError> {
        let target = current
            .transcript
            .words()
            .get(word)
            .ok_or(CorrectionError::Invalid("word index"))?;
        let parts: Vec<&str> = text.split_whitespace().collect();
        if parts.len() == 1 && parts[0] == target.text {
            return Err(CorrectionError::Invalid("nothing changes"));
        }
        let (start, end) = (target.start_cs, target.end_cs);
        let letters: Vec<u64> = parts
            .iter()
            .map(|part| part.chars().count().max(1) as u64)
            .collect();
        let total: u64 = letters.iter().sum();
        let span = u64::from(end - start);
        let mut bounds = vec![start];
        let mut sum = 0;
        for count in &letters[..letters.len().saturating_sub(1)] {
            sum += count;
            let proportional = start + u32::try_from(span * sum / total.max(1)).unwrap_or(0);
            let previous = *bounds.last().unwrap_or(&start);
            let snapped = edges_cs
                .iter()
                .copied()
                .filter(|edge| *edge > previous && *edge < end && edge.abs_diff(proportional) <= 8)
                .min_by_key(|edge| edge.abs_diff(proportional))
                .unwrap_or(proportional);
            bounds.push(snapped.max(previous));
        }
        bounds.push(end);
        let words = parts
            .iter()
            .zip(bounds.windows(2))
            .map(|(part, pair)| CorrectedWord {
                text: (*part).to_owned(),
                start_cs: pair[0],
                end_cs: pair[1],
            })
            .collect();
        self.replace_words(current, word..word + 1, words)
    }

    /// Join a word with the next one: their text without a space, the time of
    /// both.
    pub fn merge_words(
        &self,
        current: &CorrectedTranscript,
        word: usize,
    ) -> Result<Self, CorrectionError> {
        let words = current.transcript.words();
        let (Some(first), Some(second)) = (words.get(word), words.get(word + 1)) else {
            return Err(CorrectionError::Refused("there is no next word to join"));
        };
        let text = format!("{}{}", first.text, second.text);
        if text.len() > MAX_WORD_BYTES {
            return Err(CorrectionError::Refused("the joined word is too long"));
        }
        self.replace_words(
            current,
            word..word + 2,
            vec![CorrectedWord {
                text,
                start_cs: first.start_cs,
                end_cs: second.end_cs.max(first.end_cs),
            }],
        )
    }

    /// Move a word's edges. Each edge stays within its neighbour, which is
    /// shortened rather than overlapped; returns the clamped edges too.
    pub fn set_word_bounds(
        &self,
        current: &CorrectedTranscript,
        word: usize,
        start_cs: u32,
        end_cs: u32,
    ) -> Result<(Self, u32, u32), CorrectionError> {
        let words = current.transcript.words();
        let target = words
            .get(word)
            .ok_or(CorrectionError::Invalid("word index"))?;
        let duration = current.transcript.audio().duration_cs;
        let previous = word.checked_sub(1).map(|index| &words[index]);
        let next = words.get(word + 1);
        let low = previous.map_or(0, |previous| previous.start_cs);
        let high = next.map_or(duration, |next| next.end_cs);
        let start = start_cs.clamp(low, high);
        let end = end_cs.clamp(start, high);
        if (start, end) == (target.start_cs, target.end_cs) {
            return Err(CorrectionError::Invalid("nothing changes"));
        }
        let mut first = word;
        let mut replacement = Vec::new();
        if let Some(previous) = previous
            && previous.end_cs > start
        {
            first = word - 1;
            replacement.push(CorrectedWord {
                text: previous.text.clone(),
                start_cs: previous.start_cs,
                end_cs: start,
            });
        }
        replacement.push(CorrectedWord {
            text: target.text.clone(),
            start_cs: start,
            end_cs: end,
        });
        let mut last = word + 1;
        if let Some(next) = next
            && next.start_cs < end
        {
            last = word + 2;
            replacement.push(CorrectedWord {
                text: next.text.clone(),
                start_cs: end,
                end_cs: next.end_cs,
            });
        }
        Ok((
            self.replace_words(current, first..last, replacement)?,
            start,
            end,
        ))
    }

    /// Add a pause. Pauses it overlaps or touches join it.
    pub fn add_pause(
        &self,
        activity: &SpeechActivity,
        current: &CorrectedPauses,
        pause: Pause,
    ) -> Result<Self, CorrectionError> {
        let first = current
            .pauses
            .partition_point(|existing| existing.end < pause.start);
        let last = current
            .pauses
            .partition_point(|existing| existing.start <= pause.end);
        let joined =
            current.pauses[first..last.max(first)]
                .iter()
                .fold(pause, |joined, existing| Pause {
                    start: joined.start.min(existing.start),
                    end: joined.end.max(existing.end),
                });
        self.replace_pauses(activity, current, first..last.max(first), vec![joined])
    }

    /// Move a pause's edges, within its neighbours; returns the clamped edges.
    pub fn set_pause_bounds(
        &self,
        activity: &SpeechActivity,
        current: &CorrectedPauses,
        index: usize,
        start: u64,
        end: u64,
    ) -> Result<(Self, u64, u64), CorrectionError> {
        let target = current
            .pauses
            .get(index)
            .ok_or(CorrectionError::Invalid("pause index"))?;
        let low = index
            .checked_sub(1)
            .map_or(0, |previous| current.pauses[previous].end);
        let high = current
            .pauses
            .get(index + 1)
            .map_or(activity.audio().samples, |next| next.start);
        let start = start.clamp(low, high.saturating_sub(1));
        let end = end.clamp(start + 1, high);
        if (start, end) == (target.start, target.end) {
            return Err(CorrectionError::Invalid("nothing changes"));
        }
        Ok((
            self.replace_pauses(
                activity,
                current,
                index..index + 1,
                vec![Pause { start, end }],
            )?,
            start,
            end,
        ))
    }
}

/// Measured edges under [`EDGE_RULE`], in analysis samples: every boundary
/// between 10 ms energy frames where the energy changes by at least 9 dB,
/// and every detected or corrected pause edge. Sorted and unique.
pub fn measured_edges(activity: &SpeechActivity, pauses: &[Pause]) -> Vec<u64> {
    let energy = activity.energy();
    let mut edges: Vec<u64> = (1..energy.len())
        .filter(|frame| {
            (i32::from(energy[*frame]) - i32::from(energy[frame - 1])).abs() >= EDGE_STEPS
        })
        .map(|frame| frame as u64 * ENERGY_HOP)
        .collect();
    edges.extend(pauses.iter().flat_map(|pause| [pause.start, pause.end]));
    edges.sort_unstable();
    edges.dedup();
    edges
}

/// The nearest edge strictly after (or before) a position.
pub fn next_edge(edges: &[u64], from: u64, forward: bool) -> Option<u64> {
    if forward {
        edges
            .get(edges.partition_point(|edge| *edge <= from))
            .copied()
    } else {
        edges
            .partition_point(|edge| *edge < from)
            .checked_sub(1)
            .map(|index| edges[index])
    }
}

/// Edges on the centisecond clock of a transcript sharing the activity's
/// analysis PCM; empty when the clocks differ.
pub fn edges_in_centiseconds(
    edges: &[u64],
    transcript: &Transcript,
    activity: &SpeechActivity,
) -> Vec<u32> {
    let (words, detected) = (transcript.audio(), activity.audio());
    if words.origin != detected.origin || words.sample_rate != detected.sample_rate {
        return Vec::new();
    }
    let mut centiseconds: Vec<u32> = edges
        .iter()
        .filter_map(|edge| u32::try_from(edge / CENTISECOND_SAMPLES).ok())
        .filter(|edge| *edge <= words.duration_cs)
        .collect();
    centiseconds.dedup();
    centiseconds
}

fn corrected_word(word: &Word) -> CorrectedWord {
    CorrectedWord {
        text: word.text.clone(),
        start_cs: word.start_cs,
        end_cs: word.end_cs,
    }
}

/// Whether a word occupies part of a region; a zero-length word occupies its
/// one centisecond.
fn conflicts(start: u32, end: u32, region_start: u32, region_end: u32) -> bool {
    start < region_end && region_start < end.max(start.saturating_add(1))
}

fn merge_touching(pauses: Vec<Pause>) -> Vec<Pause> {
    let mut merged: Vec<Pause> = Vec::with_capacity(pauses.len());
    for pause in pauses {
        match merged.last_mut() {
            Some(last) if pause.start <= last.end => last.end = last.end.max(pause.end),
            _ => merged.push(pause),
        }
    }
    merged
}

fn check_text(text: &str) -> Result<(), CorrectionError> {
    if text.is_empty()
        || text.len() > MAX_WORD_BYTES
        || text.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(CorrectionError::Invalid(
            "a word must be nonempty, short and without spaces or controls",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
