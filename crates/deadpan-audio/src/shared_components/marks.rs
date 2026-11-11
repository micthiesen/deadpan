//! Inline suppression marks for the one bounded Components output.
//! This buffer is never a general ReadBlock mask or a cache-owned allocation.
use super::*;
use crate::stages::point_suppression::ops::{MaskContext, MaskWorkPhase, begin_ranges, spend_work};

const WORD_BITS: usize = u64::BITS as usize;
const WORDS: usize = (MAX_OUTPUT_FRAMES as usize).div_ceil(WORD_BITS);

pub(in crate::stages) struct ComponentMarks {
    frames: usize,
    words: [u64; WORDS],
}

impl ComponentMarks {
    pub(in crate::stages) fn new(
        frames: usize,
        control: WorkControl<'_>,
    ) -> Result<Self, StageAudioError> {
        control.check()?;
        if frames == 0 || frames > MAX_OUTPUT_FRAMES as usize {
            return Err(StageAudioError::Range);
        }
        // Charge the actual fixed word initialization before doing it.
        spend_work(control, MaskWorkPhase::Zero, WORDS)?;
        Ok(Self {
            frames,
            words: [0; WORDS],
        })
    }

    pub(in crate::stages) fn frames(&self) -> usize {
        self.frames
    }

    pub(in crate::stages) fn mark(
        &mut self,
        at: usize,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        control.check()?;
        if at >= self.frames {
            return Err(StageAudioError::Range);
        }
        spend_work(control, MaskWorkPhase::Mark, 1)?;
        self.words[at / WORD_BITS] |= 1_u64 << (at % WORD_BITS);
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::stages) fn test_words(&self) -> &[u64] {
        &self.words
    }

    pub(in crate::stages) fn finish<C: MaskContext>(
        self,
        start: AudioSample,
        context: &mut C,
    ) -> Result<AudioSuppressionMask, StageAudioError> {
        let mut count = 0_usize;
        for word in &self.words {
            context.check()?;
            context.spend(MaskWorkPhase::Scan, 1)?;
            count = count
                .checked_add(word.count_ones() as usize)
                .ok_or(StageAudioError::Range)?;
        }
        if count == 0 {
            context.check()?;
            return Ok(AudioSuppressionMask::default());
        }

        // Keep the old raw singleton count as funded capacity, even though
        // ordered emission coalesces the actual set bits into fewer ranges.
        let mut destination = begin_ranges::<AudioSample, C>(context, count)?;
        let mut pending: Option<Range<AudioSample>> = None;
        for (index, mut bits) in self.words.into_iter().enumerate() {
            context.check()?;
            context.spend(MaskWorkPhase::Scan, 1)?;
            while bits != 0 {
                context.check()?;
                context.spend(MaskWorkPhase::Scan, 1)?;
                let low = bits.trailing_zeros() as usize;
                let width = (bits >> low).trailing_ones() as usize;
                let high = low + width;
                // Full-word runs must never shift a u64 by 64.
                bits = if high == WORD_BITS {
                    0
                } else {
                    bits & (!0_u64 << high)
                };
                let left = index * WORD_BITS + low;
                let right = index * WORD_BITS + high;
                if right > self.frames {
                    return Err(StageAudioError::Range);
                }
                let range = AudioSample(start.0 + left as i64)..AudioSample(start.0 + right as i64);
                match pending.as_mut() {
                    Some(previous) if previous.end == range.start => {
                        context.spend(MaskWorkPhase::Map, 1)?;
                        previous.end = range.end;
                    }
                    _ => {
                        if let Some(previous) = pending.take() {
                            destination.push(previous, context)?;
                        }
                        pending = Some(range);
                    }
                }
            }
        }
        if let Some(previous) = pending {
            destination.push(previous, context)?;
        }
        // No sort or second Vec: each bit run was visited in increasing order.
        destination.finish(context)
    }
}

#[cfg(test)]
#[path = "marks_tests.rs"]
mod tests;
