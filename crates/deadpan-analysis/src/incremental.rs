//! Bounded source-range scheduling and assembly on the Original analysis grid.
//!
//! A completed range owns its words and detector hops exactly once. Context
//! outside that range helps the models at seams but never grants coverage.

use std::ops::Range;

use crate::{
    ANALYSIS_SAMPLE_RATE, ActivityAudio, AnalysedAudio, SpeechActivity, Transcript, VAD_HOP,
    activity::ENERGY_HOP, transcript::MAX_WORDS,
};

/// Joint alignment of centiseconds, VAD hops and energy hops, in 16 kHz samples.
pub const AUDIO_RANGE_ALIGNMENT: u64 = 12_800;
/// Twenty-four seconds of owned samples per job.
pub const AUDIO_RANGE_SAMPLES: u64 = 30 * AUDIO_RANGE_ALIGNMENT;
/// Context on each side; the complete model input remains below thirty seconds.
pub const AUDIO_CONTEXT_SAMPLES: u64 = 3 * AUDIO_RANGE_ALIGNMENT;
/// Bounds scheduler bookkeeping even for callers outside the audio host.
pub const MAX_ANALYSIS_RANGES: u64 = 4_096;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("incremental analysis: {0}")]
pub struct IncrementalError(pub &'static str);

/// One range's disjoint coverage and overlapping model input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisRange {
    pub owned: Range<u64>,
    pub context: Range<u64>,
}

/// One running range and a bounded completion bitmap. Cursor changes replace
/// priority, rather than building a queue of obsolete requests. Running work
/// finishes its bounded range; only the next range changes.
#[derive(Debug)]
pub struct RangeSchedule {
    extent: u64,
    width: u64,
    context: u64,
    completed: Vec<bool>,
    running: Option<usize>,
}

impl RangeSchedule {
    pub fn new(extent: u64, width: u64, context: u64) -> Result<Self, IncrementalError> {
        if extent == 0 || width == 0 || extent.div_ceil(width) > MAX_ANALYSIS_RANGES {
            return Err(IncrementalError("empty or excessive range schedule"));
        }
        let count =
            usize::try_from(extent.div_ceil(width)).map_err(|_| IncrementalError("range count"))?;
        Ok(Self {
            extent,
            width,
            context,
            completed: vec![false; count],
            running: None,
        })
    }

    pub fn audio(extent: u64) -> Result<Self, IncrementalError> {
        Self::new(extent, AUDIO_RANGE_SAMPLES, AUDIO_CONTEXT_SAMPLES)
    }

    fn range(&self, index: usize) -> AnalysisRange {
        let start = index as u64 * self.width;
        let end = start.saturating_add(self.width).min(self.extent);
        AnalysisRange {
            owned: start..end,
            context: start.saturating_sub(self.context)
                ..end.saturating_add(self.context).min(self.extent),
        }
    }

    /// Take the unmeasured range nearest the latest cursor. Equal distances
    /// prefer the earlier source range; source end belongs to the last range.
    pub fn next(&mut self, cursor: u64) -> Result<Option<AnalysisRange>, IncrementalError> {
        if self.running.is_some() {
            return Err(IncrementalError("a range is already running"));
        }
        let priority = cursor.min(self.extent - 1) / self.width;
        let next = self
            .completed
            .iter()
            .enumerate()
            .filter(|(_, done)| !**done)
            .min_by_key(|(index, _)| (*index as u64).abs_diff(priority))
            .map(|(index, _)| index);
        self.running = next;
        Ok(next.map(|index| self.range(index)))
    }

    /// Admit only the exact in-flight range. A cancellation/failure leaves
    /// it incomplete so retry cannot silently skip missing source coverage.
    pub fn finish(&mut self, range: &AnalysisRange) -> Result<(), IncrementalError> {
        let index = self
            .running
            .ok_or(IncrementalError("no range is running"))?;
        if self.range(index) != *range {
            return Err(IncrementalError(
                "completion differs from the running range",
            ));
        }
        self.completed[index] = true;
        self.running = None;
        Ok(())
    }

    pub fn retry_running(&mut self) {
        self.running = None;
    }

    pub fn completed_samples(&self) -> u64 {
        self.completed
            .iter()
            .enumerate()
            .filter(|(_, done)| **done)
            .map(|(index, _)| {
                let range = self.range(index).owned;
                range.end - range.start
            })
            .sum()
    }

    pub fn is_complete(&self) -> bool {
        self.completed.iter().all(|done| *done)
    }
}

/// A model proposal and its exact global placement. Its origin stays the
/// complete Original origin; only word coordinates are local to the context.
#[derive(Debug, Clone)]
pub struct TranscriptRange {
    pub range: AnalysisRange,
    pub transcript: Transcript,
}

fn validate_range(range: &AnalysisRange, extent: u64) -> Result<(), IncrementalError> {
    if range.owned.start >= range.owned.end
        || range.context.start > range.owned.start
        || range.context.end < range.owned.end
        || range.context.end > extent
        || !range.context.start.is_multiple_of(AUDIO_RANGE_ALIGNMENT)
        || !range.owned.start.is_multiple_of(AUDIO_RANGE_ALIGNMENT)
        || (range.owned.end != extent && !range.owned.end.is_multiple_of(AUDIO_RANGE_ALIGNMENT))
        || range.context.end - range.context.start > 30 * u64::from(ANALYSIS_SAMPLE_RATE)
    {
        return Err(IncrementalError("invalid audio range or grid"));
    }
    Ok(())
}

/// Assemble only completed, nonoverlapping ranges. The returned transcript is
/// a proposal with explicit external coverage, never proof of a full scan.
/// Overlapping observations of the same timed word are paired one-to-one
/// across neighboring windows. The more interior observation wins. Unmatched
/// words retain midpoint ownership; equal neighboring words with disjoint
/// timing remain distinct. Segment numbers follow source order, independently
/// of job order. Ambiguous matches are retained rather than deleting speech.
pub fn assemble_transcript(
    audio: AnalysedAudio,
    extent: u64,
    ranges: &[TranscriptRange],
) -> Result<Transcript, IncrementalError> {
    if ranges.len() as u64 > MAX_ANALYSIS_RANGES
        || u64::from(audio.duration_cs) != extent.div_ceil(160)
    {
        return Err(IncrementalError("transcript range count or duration"));
    }
    let mut ordered: Vec<_> = ranges.iter().collect();
    ordered.sort_by_key(|part| part.range.owned.start);
    let mut previous_end = 0;
    let mut observations = Vec::new();
    let mut segment = 0_u32;
    for (range_index, part) in ordered.iter().enumerate() {
        validate_range(&part.range, extent)?;
        let local = part.transcript.audio();
        if local.origin != audio.origin
            || local.sample_rate != audio.sample_rate
            || u64::from(local.duration_cs)
                != (part.range.context.end - part.range.context.start).div_ceil(160)
            || part.range.owned.start < previous_end
        {
            return Err(IncrementalError("transcript range identity or overlap"));
        }
        previous_end = part.range.owned.end;
        let offset_cs = u32::try_from(part.range.context.start / 160)
            .map_err(|_| IncrementalError("word offset"))?;
        let mut local_segment = None;
        for word in part.transcript.words() {
            let midpoint =
                part.range.context.start + (u64::from(word.start_cs) + u64::from(word.end_cs)) * 80;
            let start = part.range.context.start + u64::from(word.start_cs) * 160;
            // Word bounds are centiseconds and the final partial centisecond
            // is rounded up. A valid final word can therefore have a midpoint
            // after the exact PCM end. It still belongs to the final range if
            // its start has measured support; a word starting at/past the exact
            // endpoint has none and must not be pulled into that range.
            if start >= extent {
                continue;
            }
            let midpoint = midpoint.min(extent - 1);
            // Context becomes usable only where another completed range has
            // real coverage. This also preserves seam words whose estimates
            // fall on opposite sides of the ownership boundary.
            let owner = ordered.partition_point(|part| part.range.owned.start <= midpoint);
            if owner == 0 || !ordered[owner - 1].range.owned.contains(&midpoint) {
                continue;
            }
            if observations.len() >= 2 * MAX_WORDS {
                return Err(IncrementalError("overlap observation limit"));
            }
            if local_segment != Some(word.segment) {
                segment = segment
                    .checked_add(1)
                    .ok_or(IncrementalError("segment count"))?;
                local_segment = Some(word.segment);
            }
            let mut word = word.clone();
            word.start_cs = word
                .start_cs
                .checked_add(offset_cs)
                .ok_or(IncrementalError("word start"))?;
            word.end_cs = word
                .end_cs
                .checked_add(offset_cs)
                .ok_or(IncrementalError("word end"))?;
            word.segment = segment;
            let normalized = word.normalized();
            observations.push(WordObservation {
                word,
                normalized,
                ranges: vec![range_index],
                margin: (midpoint - part.range.context.start)
                    .min(part.range.context.end.saturating_sub(midpoint)),
            });
        }
    }
    observations.sort_by_key(|observation| observation.word.start_cs);
    let mut accepted: Vec<WordObservation> = Vec::new();
    for observation in observations {
        let range_index = observation.ranges[0];
        let mut matches = accepted
            .iter()
            .enumerate()
            .rev()
            .take(64)
            .filter(|(_, prior)| {
                prior.normalized == observation.normalized
                    && !prior.ranges.contains(&range_index)
                    && prior
                        .ranges
                        .iter()
                        .any(|prior| prior.abs_diff(range_index) == 1)
                    && same_timed_word(&prior.word, &observation.word)
            });
        let matched = matches.next().map(|(index, _)| index);
        let unique = matches.next().is_none();
        if let Some(index) = matched.filter(|_| unique) {
            let prior = &mut accepted[index];
            prior.ranges.push(range_index);
            if observation.margin > prior.margin {
                prior.word = observation.word;
                prior.margin = observation.margin;
            }
        } else {
            accepted.push(observation);
        }
    }
    if accepted.len() > MAX_WORDS {
        return Err(IncrementalError("word limit"));
    }
    let mut words: Vec<_> = accepted
        .into_iter()
        .map(|observation| observation.word)
        .collect();
    words.sort_by_key(|word| word.start_cs);
    for index in 1..words.len() {
        words[index].segment = words[index].segment.max(words[index - 1].segment);
    }
    Transcript::new(audio, words).map_err(|_| IncrementalError("assembled transcript"))
}

struct WordObservation {
    word: crate::Word,
    normalized: String,
    ranges: Vec<usize>,
    margin: u64,
}

fn same_timed_word(left: &crate::Word, right: &crate::Word) -> bool {
    let overlap = left
        .end_cs
        .min(right.end_cs)
        .saturating_sub(left.start_cs.max(right.start_cs));
    let shorter = (left.end_cs - left.start_cs).min(right.end_cs - right.start_cs);
    // No adjacency merge: consecutive repetitions that merely touch are
    // distinct. Require majority interval overlap and bounded endpoint drift.
    left.start_cs.abs_diff(right.start_cs) <= 20
        && left.end_cs.abs_diff(right.end_cs) <= 20
        && ((shorter > 0 && u64::from(overlap) * 2 > u64::from(shorter))
            || (shorter == 0 && left.start_cs == right.start_cs && left.end_cs == right.end_cs))
}

#[derive(Debug, Clone)]
pub struct ActivityRange {
    pub range: AnalysisRange,
    pub activity: SpeechActivity,
}

/// Stitch detector/energy hops on their complete Original grids. Missing
/// ranges are refused, never filled with invented silence. Detection context
/// is discarded before the global hysteresis and energy pause rule runs.
pub fn assemble_activity(
    audio: ActivityAudio,
    ranges: &[ActivityRange],
) -> Result<SpeechActivity, IncrementalError> {
    let mut ordered: Vec<_> = ranges.iter().collect();
    ordered.sort_by_key(|part| part.range.owned.start);
    let mut next = 0;
    let mut speech = Vec::new();
    let mut energy = Vec::new();
    for part in ordered {
        validate_range(&part.range, audio.samples)?;
        let local = part.activity.audio();
        if part.range.owned.start != next
            || local.origin != audio.origin
            || local.sample_rate != audio.sample_rate
            || local.samples != part.range.context.end - part.range.context.start
        {
            return Err(IncrementalError("activity range identity or coverage"));
        }
        let offset = part.range.owned.start - part.range.context.start;
        let count = part.range.owned.end - part.range.owned.start;
        for (hop, source, output) in [
            (VAD_HOP, part.activity.speech(), &mut speech),
            (ENERGY_HOP, part.activity.energy(), &mut energy),
        ] {
            let start =
                usize::try_from(offset / hop).map_err(|_| IncrementalError("activity offset"))?;
            let end = usize::try_from((offset + count).div_ceil(hop))
                .map_err(|_| IncrementalError("activity endpoint"))?;
            output.extend_from_slice(
                source
                    .get(start..end)
                    .ok_or(IncrementalError("activity window missing hops"))?,
            );
        }
        next = part.range.owned.end;
    }
    if next != audio.samples {
        return Err(IncrementalError("activity coverage is incomplete"));
    }
    SpeechActivity::new(audio, speech, energy).map_err(|_| IncrementalError("assembled activity"))
}

#[cfg(test)]
mod tests;
