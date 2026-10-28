//! One contiguous per-route sample policy, independent of numerical PCM.
//! The header contains no flag payload until post-admission allocation.

use super::*;

pub(super) struct SamplePermissions {
    frames: usize,
    stride: usize,
    words: Vec<u64>,
}

#[derive(Clone, Copy)]
pub(super) struct PermissionRow<'a> {
    frames: usize,
    words: &'a [u64],
}

impl PermissionRow<'_> {
    pub(super) fn contains(self, at: usize) -> bool {
        at < self.frames
            && self
                .words
                .get(at / 64)
                .is_some_and(|word| word & (1 << (at % 64)) != 0)
    }
}

impl SamplePermissions {
    pub(super) fn new(frames: usize) -> Self {
        Self {
            frames,
            stride: frames.div_ceil(64),
            words: Vec::new(),
        }
    }

    pub(super) fn resident_bytes(&self, routes: usize) -> Result<usize, StageAudioError> {
        self.stride
            .checked_mul(routes)
            .and_then(|words| {
                if words > self.words.capacity() {
                    words.checked_add(self.words.capacity())
                } else {
                    Some(self.words.capacity())
                }
            })
            .and_then(|words| words.checked_mul(size_of::<u64>()))
            .and_then(|bytes| bytes.checked_add(size_of::<Self>()))
            .ok_or(StageAudioError::Limit("contribution policy residency"))
    }

    /// Admit resident_bytes before this allocation, including the old plus new
    /// payload peak when growing. Existing marks survive growth or reuse.
    /// No new frame cap is imposed: legal 1024-frame Components tests remain.
    pub(super) fn allocate(&mut self, routes: usize) {
        let words = self
            .stride
            .checked_mul(routes)
            .expect("admitted permission shape");
        if words > self.words.capacity() {
            let mut replacement = vec![0; words];
            replacement[..self.words.len()].copy_from_slice(&self.words);
            self.words = replacement;
        } else {
            self.words.resize(words, 0);
        }
    }

    pub(super) fn validate(&self, routes: usize) -> Result<(), StageAudioError> {
        if self.stride.checked_mul(routes) != Some(self.words.len()) {
            return Err(PlanError::InvalidPlan("contribution permissions not allocated").into());
        }
        Ok(())
    }

    pub(super) fn row(&self, route: usize) -> PermissionRow<'_> {
        let words = route
            .checked_mul(self.stride)
            .and_then(|start| start.checked_add(self.stride).map(|end| start..end))
            .and_then(|range| self.words.get(range))
            .unwrap_or(&[]);
        PermissionRow {
            frames: self.frames,
            words,
        }
    }

    pub(super) fn contains(&self, route: usize, at: usize) -> bool {
        self.row(route).contains(at)
    }

    /// Populate a separately admitted prediction a word at a time. This is
    /// additional metadata work; the numerical add path keeps its old charges.
    pub(super) fn assign_range(
        &mut self,
        route: usize,
        range: Range<usize>,
        allowed: bool,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        control.spend_plan_work(1)?;
        if range.start >= range.end || range.end > self.frames {
            return Err(StageAudioError::Range);
        }
        let start = route
            .checked_mul(self.stride)
            .ok_or(StageAudioError::Range)?;
        let end = start
            .checked_add(self.stride)
            .ok_or(StageAudioError::Range)?;
        let row = self
            .words
            .get_mut(start..end)
            .ok_or(PlanError::InvalidPlan(
                "contribution prediction not allocated",
            ))?;
        let first = range.start / 64;
        let last = (range.end - 1) / 64;
        for (word, value) in row.iter_mut().enumerate().take(last + 1).skip(first) {
            control.spend_plan_work(1)?;
            let low = if word == first { range.start % 64 } else { 0 };
            let high = if word == last {
                (range.end - 1) % 64 + 1
            } else {
                64
            };
            let mask = (u64::MAX << low)
                & if high == 64 {
                    u64::MAX
                } else {
                    (1_u64 << high) - 1
                };
            if allowed {
                *value |= mask;
            } else {
                *value &= !mask;
            }
        }
        Ok(())
    }

    pub(super) fn verify_matches(
        &self,
        observed: &Self,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        control.spend_plan_work(1)?;
        if self.frames != observed.frames
            || self.stride != observed.stride
            || self.words.len() != observed.words.len()
        {
            return Err(PlanError::InvalidPlan("contribution prediction shape differs").into());
        }
        for (planned, actual) in self.words.iter().zip(&observed.words) {
            control.spend_plan_work(1)?;
            if planned != actual {
                return Err(
                    PlanError::InvalidPlan("contribution permission prediction differs").into(),
                );
            }
        }
        Ok(())
    }

    pub(super) fn payload_frames(&self) -> Result<u64, StageAudioError> {
        // Each u64 word occupies exactly one f32-stereo-frame ledger unit.
        u64::try_from(self.words.capacity()).map_err(|_| StageAudioError::Range)
    }

    /// Structural policy only, never source authority or finite PCM. A new
    /// metadata producer must bill its own walk. Runtime add replaces its old
    /// bool assignment here after both channel finite checks have succeeded.
    pub(super) fn mark(&mut self, route: usize, at: usize) -> Result<(), StageAudioError> {
        if at >= self.frames {
            return Err(StageAudioError::Range);
        }
        let index = route
            .checked_mul(self.stride)
            .and_then(|start| start.checked_add(at / 64))
            .ok_or(StageAudioError::Range)?;
        let word = self.words.get_mut(index).ok_or(PlanError::InvalidPlan(
            "contribution permissions not allocated",
        ))?;
        *word |= 1 << (at % 64);
        Ok(())
    }
}

#[cfg(test)]
#[path = "permission_tests.rs"]
mod tests;
