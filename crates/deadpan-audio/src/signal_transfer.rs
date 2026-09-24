//! Worker-only transfer of an already time-mapped root signal into a point grid.
//! The caller owns revision/source admission and the shared read deadline. This
//! adapter retains neither a document nor creative fades.

use std::ops::Range;
use std::sync::atomic::AtomicBool;

use deadpan_core::{AudioSample, ExactRatio, TimeError};
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_plan::SignalSample;
use serde::Serialize;

use crate::{
    MAX_INPUT_MAGNITUDE, MAX_OUTPUT_FRAMES, MAX_SOURCE_FRAMES, PcmWindow, PreparationError,
    ResampleRecipe, Resampler, StereoMatrix, check_cancel,
};

#[derive(Debug, thiserror::Error)]
pub enum SignalTransferError {
    #[error("signal transfer requires a valid bounded root support and output range")]
    Range,
    #[error("root signal reader returned an incomplete block or invalid suppression ranges")]
    InvalidRootBlock,
    #[error(transparent)]
    Preparation(#[from] PreparationError),
    #[error(transparent)]
    Time(#[from] TimeError),
}

/// Complete stereo PCM for exactly one requested root interval, before creative
/// fades. Explicit suppression includes silent Holds and exhausted retained
/// domains, but never inferred silence from zero samples or a missing voice.
/// Every suppression range must be nonempty and contained in this block.
#[derive(Debug, Clone, PartialEq)]
pub struct RootSignalBlock {
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<AudioSample>>,
}

/// Prepared point-grid samples and the exact explicit suppression retained for
/// later processing. The owning caller supplies project and processing identity.
#[derive(Debug, Clone, PartialEq)]
pub struct TransferredSignalBlock {
    pub start: SignalSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<SignalSample>>,
}

/// Exact coordinates into admitted root PCM, not evidence of source identity.
/// Source support is half-open. The existing resampler qualifies positive steps
/// in 1/64..64 and retains its integer-unity fast path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RootSignalTransfer {
    recipe: ResampleRecipe,
}

impl RootSignalTransfer {
    pub fn new(
        root_support: Range<AudioSample>,
        root_at_anchor: ExactRatio,
        signal_anchor: SignalSample,
        root_samples_per_signal_sample: ExactRatio,
        output: Range<SignalSample>,
    ) -> Result<Self, SignalTransferError> {
        if root_support.start.0 < 0 || root_support.start >= root_support.end {
            return Err(SignalTransferError::Range);
        }
        Ok(Self {
            recipe: ResampleRecipe::new(
                root_support.start.0..root_support.end.0,
                root_at_anchor,
                AudioSample(signal_anchor.0),
                root_samples_per_signal_sample,
                AudioSample(output.start.0)..AudioSample(output.end.0),
            )?,
        })
    }

    pub fn root_support(&self) -> Range<AudioSample> {
        let support = self.recipe.selection();
        AudioSample(support.start)..AudioSample(support.end)
    }

    pub fn output(&self) -> Range<SignalSample> {
        let output = self.recipe.output_range();
        SignalSample(output.start.0)..SignalSample(output.end.0)
    }

    pub fn root_position(&self, sample: SignalSample) -> Result<ExactRatio, TimeError> {
        self.recipe.source_at(AudioSample(sample.0))
    }

    /// Read one bounded output block. Each callback must return exactly the
    /// requested 1..256 root samples and share the caller's deadline/work budget.
    /// At most MAX_SOURCE_FRAMES root samples are requested in total. Provider
    /// errors pass through unchanged; no partial output is returned.
    pub fn render<E: From<SignalTransferError>>(
        &self,
        start: SignalSample,
        frames: u32,
        cancelled: &AtomicBool,
        mut read: impl FnMut(AudioSample, u32) -> Result<RootSignalBlock, E>,
    ) -> Result<TransferredSignalBlock, E> {
        check_cancel(cancelled).map_err(SignalTransferError::from)?;
        let matrix = StereoMatrix::new(AudioChannelLayout::Native {
            channels: 2,
            mask: 3,
        })
        .map_err(SignalTransferError::from)?;
        let sampler = Resampler::new(self.recipe.clone(), matrix);
        // This checks the complete request before allocating or calling a reader.
        let required = sampler
            .required_source_range(AudioSample(start.0), frames)
            .map_err(SignalTransferError::from)?;
        let end = start
            .0
            .checked_add(i64::from(frames))
            .map(SignalSample)
            .ok_or(SignalTransferError::Range)?;
        let mut suppressed = vec![false; frames as usize];
        self.mark_outside_support(start..end, &mut suppressed)?;
        let window = if let Some(required) = required {
            let length = required
                .end
                .checked_sub(required.start)
                .and_then(|length| u32::try_from(length).ok())
                .filter(|length| *length > 0 && *length <= MAX_SOURCE_FRAMES)
                .ok_or(SignalTransferError::Range)?;
            let mut interleaved = Vec::with_capacity(length as usize * 2);
            let mut cursor = required.start;
            while cursor < required.end {
                check_cancel(cancelled).map_err(SignalTransferError::from)?;
                let count = u32::try_from(required.end - cursor)
                    .map_err(|_| SignalTransferError::Range)?
                    .min(MAX_OUTPUT_FRAMES);
                let mut block = read(AudioSample(cursor), count)?;
                check_cancel(cancelled).map_err(SignalTransferError::from)?;
                validate_root_block(&block, AudioSample(cursor), count)?;
                for range in &block.suppressed {
                    check_cancel(cancelled).map_err(SignalTransferError::from)?;
                    // Mask the old discrete signal before any interpolation.
                    let left = usize::try_from(range.start.0 - cursor)
                        .map_err(|_| SignalTransferError::InvalidRootBlock)?;
                    let right = usize::try_from(range.end.0 - cursor)
                        .map_err(|_| SignalTransferError::InvalidRootBlock)?;
                    block.samples[left..right].fill([0.0; 2]);
                    self.mark_interval(range.clone(), start..end, &mut suppressed)?;
                }
                interleaved.extend(block.samples.into_iter().flatten());
                cursor = cursor
                    .checked_add(i64::from(count))
                    .ok_or(SignalTransferError::Range)?;
            }
            Some(PcmWindow {
                start: required.start,
                samples: interleaved,
            })
        } else {
            None
        };
        let mut samples = sampler
            .render(AudioSample(start.0), frames, window, cancelled)
            .map_err(SignalTransferError::from)?
            .samples;
        // Sinc support can reach nonzero neighbors while the exact output point
        // belongs to silence or lies outside the admitted domain. Reapply that
        // policy using point-ceil boundaries, independently of filter support.
        let mut ranges = Vec::new();
        let mut run = None;
        for (offset, (&muted, sample)) in suppressed.iter().zip(&mut samples).enumerate() {
            if muted {
                *sample = [0.0; 2];
                run.get_or_insert(offset);
            } else if let Some(left) = run.take() {
                ranges.push(signal_range(start, left, offset)?);
            }
        }
        if let Some(left) = run {
            ranges.push(signal_range(start, left, samples.len())?);
        }
        check_cancel(cancelled).map_err(SignalTransferError::from)?;
        Ok(TransferredSignalBlock {
            start,
            samples,
            suppressed: ranges,
        })
    }

    fn point_boundary(&self, root: AudioSample) -> Result<i128, SignalTransferError> {
        // The first integer output whose exact root coordinate reaches this
        // half-open boundary. Never round an absolute source position via f64.
        Ok(ExactRatio::integer(root.0)
            .checked_sub(self.recipe.source_origin())?
            .checked_div(self.recipe.source_step())?
            .checked_add(ExactRatio::integer(self.recipe.output_origin().0))?
            .ceil()?)
    }

    fn mark_interval(
        &self,
        root: Range<AudioSample>,
        output: Range<SignalSample>,
        suppressed: &mut [bool],
    ) -> Result<(), SignalTransferError> {
        let left = self
            .point_boundary(root.start)?
            .max(i128::from(output.start.0));
        let right = self.point_boundary(root.end)?.min(i128::from(output.end.0));
        if left < right {
            let left = usize::try_from(left - i128::from(output.start.0))
                .map_err(|_| SignalTransferError::Range)?;
            let right = usize::try_from(right - i128::from(output.start.0))
                .map_err(|_| SignalTransferError::Range)?;
            suppressed
                .get_mut(left..right)
                .ok_or(SignalTransferError::Range)?
                .fill(true);
        }
        Ok(())
    }

    fn mark_outside_support(
        &self,
        output: Range<SignalSample>,
        suppressed: &mut [bool],
    ) -> Result<(), SignalTransferError> {
        let support = self.root_support();
        let left = self
            .point_boundary(support.start)?
            .clamp(i128::from(output.start.0), i128::from(output.end.0));
        let right = self
            .point_boundary(support.end)?
            .clamp(i128::from(output.start.0), i128::from(output.end.0));
        let left = usize::try_from(left - i128::from(output.start.0))
            .map_err(|_| SignalTransferError::Range)?;
        let right = usize::try_from(right - i128::from(output.start.0))
            .map_err(|_| SignalTransferError::Range)?;
        suppressed[..left].fill(true);
        suppressed[right..].fill(true);
        Ok(())
    }
}

fn validate_root_block(
    block: &RootSignalBlock,
    start: AudioSample,
    frames: u32,
) -> Result<(), SignalTransferError> {
    let end = start
        .0
        .checked_add(i64::from(frames))
        .ok_or(SignalTransferError::InvalidRootBlock)?;
    if block.start != start
        || block.samples.len() != frames as usize
        || block.suppressed.len() > MAX_OUTPUT_FRAMES as usize
        || block
            .suppressed
            .iter()
            .any(|range| range.start < start || range.start >= range.end || range.end.0 > end)
    {
        return Err(SignalTransferError::InvalidRootBlock);
    }
    // Validate even samples that policy will suppress. Otherwise a malformed
    // provider could hide invalid PCM behind a silence declaration.
    if block
        .samples
        .iter()
        .flatten()
        .any(|sample| !sample.is_finite() || sample.abs() > MAX_INPUT_MAGNITUDE)
    {
        return Err(PreparationError::InvalidSamples.into());
    }
    Ok(())
}

fn signal_range(
    start: SignalSample,
    left: usize,
    right: usize,
) -> Result<Range<SignalSample>, SignalTransferError> {
    let point = |offset| {
        let offset = i64::try_from(offset).map_err(|_| SignalTransferError::Range)?;
        start
            .0
            .checked_add(offset)
            .map(SignalSample)
            .ok_or(SignalTransferError::Range)
    };
    Ok(point(left)?..point(right)?)
}
