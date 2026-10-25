//! Streaming measurement and resumable progress.
//!
//! [`ShotMeasurer`] turns signatures pushed in picture order into
//! [`PictureMeasure`]s, keeping only the last `2 · 25 + 1` signatures. Its
//! [`ShotProgress`] can be saved and resumed: a resumed measurer asks for
//! pictures again from [`ShotMeasurer::next_ordinal`], up to 50 pictures
//! before the first unmeasured one, so the span comparisons that were still
//! waiting for later pictures are recomputed from real signatures. The
//! result equals an uninterrupted measurement byte for byte.
//!
//! A checkpoint need not copy every measure: [`ShotMeasurer::take_tail`]
//! returns only the measures changed since the previous tail, as a
//! [`ShotProgressTail`] that a store appends to the saved progress.

use std::collections::VecDeque;

use super::{
    HALF_SPANS, MAX_HALF_SPAN, MAX_SHOT_PICTURES, PictureMeasure, PictureSignature, ShotAnalysis,
    ShotError, spans_are_canonical,
};

/// Pictures decoded again before the first unmeasured picture on resume.
pub const REPLAY_PICTURES: usize = 2 * MAX_HALF_SPAN;
const RING: usize = 2 * MAX_HALF_SPAN + 1;

/// Measures of the first pictures of a scan, saved to continue it later.
/// Span comparisons whose window reaches beyond the measured pictures are
/// still zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShotProgress {
    pictures: usize,
    measures: Vec<PictureMeasure>,
}

impl ShotProgress {
    pub fn new(pictures: usize, measures: Vec<PictureMeasure>) -> Result<Self, ShotError> {
        if pictures > MAX_SHOT_PICTURES {
            return Err(ShotError::Limit);
        }
        if measures.len() > pictures {
            return Err(ShotError::Invalid(
                "progress measured more pictures than exist",
            ));
        }
        if measures
            .first()
            .is_some_and(|first| first.change != [0, 0, 0])
        {
            return Err(ShotError::Invalid("the first picture has no predecessor"));
        }
        let measured = measures.len();
        if !measures
            .iter()
            .enumerate()
            .all(|(picture, measure)| spans_are_canonical(measure, picture, measured))
        {
            return Err(ShotError::Invalid(
                "a span reaches beyond the measured pictures",
            ));
        }
        Ok(Self { pictures, measures })
    }

    /// Pictures of the whole scan.
    pub fn pictures(&self) -> usize {
        self.pictures
    }

    /// The first picture not yet measured.
    pub fn next(&self) -> usize {
        self.measures.len()
    }

    pub fn measures(&self) -> &[PictureMeasure] {
        &self.measures
    }
}

/// The measures of pictures `start..next` of a scan, replacing whatever
/// saved progress holds from `start` on. A push can still change the spans
/// of up to 25 earlier pictures, so a tail starts that far before the
/// previous tail's end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShotProgressTail {
    pictures: usize,
    start: usize,
    measures: Vec<PictureMeasure>,
}

impl ShotProgressTail {
    /// Validates the tail as far as it can without the measures before
    /// `start`: no span reaches beyond the measured pictures.
    pub fn new(
        pictures: usize,
        start: usize,
        measures: Vec<PictureMeasure>,
    ) -> Result<Self, ShotError> {
        if pictures > MAX_SHOT_PICTURES {
            return Err(ShotError::Limit);
        }
        let next = start
            .checked_add(measures.len())
            .filter(|next| *next <= pictures)
            .ok_or(ShotError::Invalid(
                "progress measured more pictures than exist",
            ))?;
        if start == 0
            && measures
                .first()
                .is_some_and(|first| first.change != [0, 0, 0])
        {
            return Err(ShotError::Invalid("the first picture has no predecessor"));
        }
        if !measures
            .iter()
            .enumerate()
            .all(|(offset, measure)| spans_are_canonical(measure, start + offset, next))
        {
            return Err(ShotError::Invalid(
                "a span reaches beyond the measured pictures",
            ));
        }
        Ok(Self {
            pictures,
            start,
            measures,
        })
    }

    /// Pictures of the whole scan.
    pub fn pictures(&self) -> usize {
        self.pictures
    }

    /// The first picture whose measure this tail replaces.
    pub fn start(&self) -> usize {
        self.start
    }

    /// The first picture not yet measured.
    pub fn next(&self) -> usize {
        self.start + self.measures.len()
    }

    pub fn measures(&self) -> &[PictureMeasure] {
        &self.measures
    }

    /// One tail equal to applying `self` and then `newer`, for a checkpoint
    /// that waited while a newer one arrived. Refused when `newer` starts
    /// after `self` ends: the measures between would be missing.
    pub fn merge(mut self, newer: Self) -> Result<Self, ShotError> {
        if newer.pictures != self.pictures || newer.start > self.next() {
            return Err(ShotError::Invalid("progress tails do not join"));
        }
        if newer.start <= self.start {
            return Ok(newer);
        }
        self.measures.truncate(newer.start - self.start);
        self.measures.extend(newer.measures);
        Ok(self)
    }
}

/// Builds measures from signatures pushed in picture order.
#[derive(Debug, Clone)]
pub struct ShotMeasurer {
    pictures: usize,
    measures: Vec<PictureMeasure>,
    expected: usize,
    ring: VecDeque<PictureSignature>,
    /// Ordinal of `ring[0]`.
    ring_start: usize,
    /// The first measure changed since the last [`Self::take_tail`].
    unsaved: usize,
}

impl ShotMeasurer {
    /// A measurer for `pictures` pictures, starting at picture 0.
    pub fn new(pictures: usize) -> Result<Self, ShotError> {
        Self::resume(ShotProgress::new(pictures, Vec::new())?)
    }

    /// Continue from saved progress. Push pictures from
    /// [`Self::next_ordinal`] on.
    pub fn resume(progress: ShotProgress) -> Result<Self, ShotError> {
        let mut measures = Vec::new();
        measures
            .try_reserve_exact(progress.pictures)
            .map_err(|_| ShotError::Limit)?;
        let expected = progress.next().saturating_sub(REPLAY_PICTURES);
        // The resumed progress is the saved progress: only later changes
        // are unsaved.
        let unsaved = progress.next();
        measures.extend(progress.measures);
        Ok(Self {
            pictures: progress.pictures,
            measures,
            expected,
            ring: VecDeque::with_capacity(RING),
            ring_start: expected,
            unsaved,
        })
    }

    /// The ordinal the next pushed signature must have.
    pub fn next_ordinal(&self) -> usize {
        self.expected
    }

    /// Pictures with a measure so far.
    pub fn measured(&self) -> usize {
        self.measures.len()
    }

    pub fn push(&mut self, ordinal: usize, signature: PictureSignature) -> Result<(), ShotError> {
        if ordinal != self.expected || ordinal >= self.pictures {
            return Err(ShotError::Invalid("signature pushed out of picture order"));
        }
        if self.ring.len() == RING {
            self.ring.pop_front();
            self.ring_start += 1;
        }
        self.ring.push_back(signature);
        let ring_start = self.ring_start;
        let ring = &self.ring;
        let get = |picture: usize| {
            picture
                .checked_sub(ring_start)
                .and_then(|offset| ring.get(offset))
        };
        let current = &ring[ring.len() - 1];
        if ordinal == self.measures.len() {
            let change = match ordinal.checked_sub(1).and_then(get) {
                Some(previous) => previous.change(current, ordinal.checked_sub(2).and_then(get)),
                None if ordinal == 0 => [0, 0, 0],
                None => return Err(ShotError::Invalid("missing previous signature")),
            };
            let [luma, spread] = current.luma();
            self.measures.push(PictureMeasure {
                change,
                luma,
                spread,
                ..PictureMeasure::default()
            });
            self.unsaved = self.unsaved.min(ordinal);
        }
        for (index, half) in HALF_SPANS.iter().enumerate() {
            let Some(start) = ordinal.checked_sub(2 * half) else {
                continue;
            };
            // Before resuming at the first unmeasured picture, a window that
            // starts before the replay was complete when it was saved.
            if let (Some(before), Some(middle)) = (get(start), get(ordinal - half)) {
                let span = middle.span(before, current);
                let measure = &mut self.measures[ordinal - half];
                if measure.spans[index] != span {
                    measure.spans[index] = span;
                    self.unsaved = self.unsaved.min(ordinal - half);
                }
            }
        }
        self.expected += 1;
        Ok(())
    }

    /// The measures changed since the previous tail (or since the start or
    /// the resumed progress), to append to saved progress. Copies only
    /// those measures.
    pub fn take_tail(&mut self) -> ShotProgressTail {
        let start = self.unsaved.min(self.measures.len());
        self.unsaved = self.measures.len();
        ShotProgressTail {
            pictures: self.pictures,
            start,
            measures: self.measures[start..].to_vec(),
        }
    }

    /// A tail taken from `start` was not saved: offer its measures again in
    /// the next tail.
    pub fn unsave_from(&mut self, start: usize) {
        self.unsaved = self.unsaved.min(start);
    }

    /// Progress to save: a copy of every measure so far.
    pub fn progress(&self) -> ShotProgress {
        ShotProgress {
            pictures: self.pictures,
            measures: self.measures.clone(),
        }
    }

    pub fn finish(self) -> Result<ShotAnalysis, ShotError> {
        if self.expected != self.pictures || self.measures.len() != self.pictures {
            return Err(ShotError::Invalid("not every picture was measured"));
        }
        ShotAnalysis::new(self.measures)
    }
}
