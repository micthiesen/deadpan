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

pub(in crate::stages) enum OutputSuppression<'a> {
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "The ordinary multi-output destination remains available."
        )
    )]
    Ranges(&'a mut Vec<Range<AudioSample>>),
    Marks(&'a mut ComponentMarks),
}

pub(in crate::stages) struct Output<'a> {
    pub(super) selection: OutputSelection,
    pub(super) samples: OutputSamples<'a>,
    pub(super) suppressed: Option<OutputSuppression<'a>>,
}

impl Output<'_> {
    /// Validate every destination before the caller clears or processes any
    /// buffer. Multiple-output suppression has pre-admitted capacity; the old
    /// single-output wrapper retains its existing Vec growth behavior.
    pub(super) fn validate(&self, frames: usize, multiple: bool) -> Result<(), StageAudioError> {
        if self.samples.frames() != Some(frames) {
            return Err(StageAudioError::Range);
        }
        match &self.suppressed {
            Some(OutputSuppression::Ranges(suppressed)) => {
                if !suppressed.is_empty() {
                    return Err(PlanError::InvalidPlan(
                        "contribution output suppression is not empty",
                    )
                    .into());
                }
                if multiple && suppressed.capacity() < frames {
                    return Err(StageAudioError::Limit(
                        "contribution output suppression capacity",
                    ));
                }
            }
            Some(OutputSuppression::Marks(marks)) if marks.frames() != frames => {
                return Err(StageAudioError::Range);
            }
            Some(OutputSuppression::Marks(_)) | None => {}
        }
        Ok(())
    }

    pub(super) fn mark_suppressed(
        &mut self,
        at: usize,
        sample: AudioSample,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        match &mut self.suppressed {
            Some(OutputSuppression::Ranges(suppressed)) => {
                suppressed.push(sample..AudioSample(sample.0 + 1));
            }
            Some(OutputSuppression::Marks(marks)) => {
                #[cfg(test)]
                super::super::components_mask_trace::mark_attempt(at, sample.0, control);
                let checked = control.check();
                #[cfg(test)]
                let checked = checked.inspect_err(|error| {
                    super::super::components_mask_trace::mark_refused(at, sample.0, error);
                });
                checked?;
                let spent = marks.mark(at, control);
                #[cfg(test)]
                let spent = spent.inspect_err(|error| {
                    super::super::components_mask_trace::mark_refused(at, sample.0, error);
                });
                spent?;
                #[cfg(test)]
                super::super::components_mask_trace::mark_written(at, sample.0, control);
            }
            None => {}
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "outputs_tests.rs"]
mod tests;
