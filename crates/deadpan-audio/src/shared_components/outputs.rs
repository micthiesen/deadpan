//! Borrowed destinations for one component traversal. Selection describes only
//! terminal contribution summing; it grants no context or source authority.

use super::*;

#[derive(Clone, Copy)]
pub(in crate::stages) struct OutputSelection {
    pub(super) voice: Option<usize>,
    pub(super) aliases: Permissions,
}

impl OutputSelection {
    pub(super) fn selects(self, voice: usize) -> bool {
        self.voice.is_none_or(|selected| selected == voice) || self.aliases.contains(voice)
    }
}

pub(in crate::stages) enum OutputSamples<'a> {
    Interleaved(&'a mut [[f64; 2]]),
    Planar {
        left: &'a mut [f64],
        right: &'a mut [f64],
    },
}

impl<'a> From<(&'a mut [f64], &'a mut [f64])> for OutputSamples<'a> {
    fn from((left, right): (&'a mut [f64], &'a mut [f64])) -> Self {
        Self::Planar { left, right }
    }
}

impl<'a> From<&'a mut [[f64; 2]]> for OutputSamples<'a> {
    fn from(samples: &'a mut [[f64; 2]]) -> Self {
        Self::Interleaved(samples)
    }
}

impl OutputSamples<'_> {
    pub(super) fn frames(&self) -> Option<usize> {
        match self {
            Self::Interleaved(samples) => Some(samples.len()),
            Self::Planar { left, right } => (left.len() == right.len()).then_some(left.len()),
        }
    }

    pub(super) fn frame(&self, at: usize) -> [f64; 2] {
        match self {
            Self::Interleaved(samples) => samples[at],
            Self::Planar { left, right } => [left[at], right[at]],
        }
    }

    pub(super) fn set(&mut self, at: usize, value: [f64; 2]) {
        match self {
            Self::Interleaved(samples) => samples[at] = value,
            Self::Planar { left, right } => {
                left[at] = value[0];
                right[at] = value[1];
            }
        }
    }

    pub(super) fn add(&mut self, at: usize, value: [f64; 2]) {
        // Preserve the original left-then-right channel operations and each
        // destination's route-order addition. No intermediate sum is formed.
        match self {
            Self::Interleaved(samples) => {
                for (out, input) in samples[at].iter_mut().zip(value) {
                    *out += input;
                }
            }
            Self::Planar { left, right } => {
                left[at] += value[0];
                right[at] += value[1];
            }
        }
    }

    pub(super) fn clear(&mut self) {
        match self {
            Self::Interleaved(samples) => samples.fill([0.0; 2]),
            Self::Planar { left, right } => {
                left.fill(0.0);
                right.fill(0.0);
            }
        }
    }
}

pub(in crate::stages) struct Output<'a> {
    pub(super) selection: OutputSelection,
    pub(super) samples: OutputSamples<'a>,
    pub(super) suppressed: Option<&'a mut Vec<Range<AudioSample>>>,
}

impl Output<'_> {
    /// Validate every destination before the caller clears or processes any
    /// buffer. Multiple-output suppression has pre-admitted capacity; the old
    /// single-output wrapper retains its existing Vec growth behavior.
    pub(super) fn validate(&self, frames: usize, multiple: bool) -> Result<(), StageAudioError> {
        if self.samples.frames() != Some(frames) {
            return Err(StageAudioError::Range);
        }
        if let Some(suppressed) = &self.suppressed {
            if !suppressed.is_empty() {
                return Err(
                    PlanError::InvalidPlan("contribution output suppression is not empty").into(),
                );
            }
            if multiple && suppressed.capacity() < frames {
                return Err(StageAudioError::Limit(
                    "contribution output suppression capacity",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn mark_suppressed(&mut self, sample: AudioSample) {
        if let Some(suppressed) = &mut self.suppressed {
            suppressed.push(sample..AudioSample(sample.0 + 1));
        }
    }
}

#[cfg(test)]
#[path = "outputs_tests.rs"]
mod tests;
