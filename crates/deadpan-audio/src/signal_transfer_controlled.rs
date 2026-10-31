//! Controlled native mask destinations; the public Vec renderer stays separate.
use super::*;
use crate::StageAudioError;
use crate::stages::point_suppression::{
    FlagBuffer, MaskContext, MaskWorkPhase, RangeBuilder, SuppressionMask, begin_flags,
    begin_ranges,
};

#[derive(Debug)]
pub(crate) struct ControlledRootBlock {
    pub(crate) start: AudioSample,
    pub(crate) samples: Vec<[f32; 2]>,
    pub(crate) suppressed: SuppressionMask<AudioSample>,
}

#[derive(Debug)]
pub(crate) struct ControlledTransferredBlock {
    pub(crate) start: SignalSample,
    pub(crate) samples: Vec<[f32; 2]>,
    pub(crate) suppressed: SuppressionMask<SignalSample>,
}

fn check<C: MaskContext>(context: &C, cancelled: &AtomicBool) -> Result<(), StageAudioError> {
    check_cancel(cancelled).map_err(SignalTransferError::from)?;
    context.check()
}

fn spend<C: MaskContext>(
    context: &C,
    phase: MaskWorkPhase,
    units: usize,
    cancelled: &AtomicBool,
) -> Result<(), StageAudioError> {
    check(context, cancelled)?;
    context.spend(phase, units)
}

impl RootSignalTransfer {
    /// Keep admitted flag scratch across recursive reads, and keep each input
    /// mask through validation, discrete zeroing and output-policy marking.
    /// PCM/window/dependency allocation remains the caller's existing product.
    pub(crate) fn render_controlled<C: MaskContext>(
        &self,
        start: SignalSample,
        frames: u32,
        cancelled: &AtomicBool,
        context: &mut C,
        mut read: impl FnMut(&mut C, AudioSample, u32) -> Result<ControlledRootBlock, StageAudioError>,
    ) -> Result<ControlledTransferredBlock, StageAudioError> {
        check(context, cancelled)?;
        let matrix = StereoMatrix::new(AudioChannelLayout::Native {
            channels: 2,
            mask: 3,
        })
        .map_err(SignalTransferError::from)?;
        let sampler = Resampler::new(self.recipe.clone(), matrix);
        let required = sampler
            .required_source_range(AudioSample(start.0), frames)
            .map_err(SignalTransferError::from)?;
        let end = start
            .0
            .checked_add(i64::from(frames))
            .map(SignalSample)
            .ok_or(SignalTransferError::Range)?;
        // Complete source bounds are checked before admitting any scratch.
        let required = required
            .map(|required| {
                let length = required
                    .end
                    .checked_sub(required.start)
                    .and_then(|length| u32::try_from(length).ok())
                    .filter(|length| *length > 0 && *length <= MAX_SOURCE_FRAMES)
                    .ok_or(SignalTransferError::Range)?;
                Ok::<_, SignalTransferError>((required, length))
            })
            .transpose()?;
        let mut flags: FlagBuffer = begin_flags(context, frames as usize)?;
        self.mark_outside_controlled(start..end, flags.as_mut_slice(), context, cancelled)?;
        let window = if let Some((required, length)) = required {
            // The existing interleaved PCM allocation is outside mask admission.
            let mut interleaved = Vec::with_capacity(length as usize * 2);
            let mut cursor = required.start;
            while cursor < required.end {
                check(context, cancelled)?;
                let count = u32::try_from(required.end - cursor)
                    .map_err(|_| SignalTransferError::Range)?
                    .min(MAX_OUTPUT_FRAMES);
                let mut block = read(context, AudioSample(cursor), count)?;
                check(context, cancelled)?;
                validate_root_slices(
                    block.start,
                    &block.samples,
                    block.suppressed.as_slice(),
                    AudioSample(cursor),
                    count,
                    || spend(context, MaskWorkPhase::Scan, 1, cancelled),
                )?;
                for range in block.suppressed.iter() {
                    spend(context, MaskWorkPhase::Map, 1, cancelled)?;
                    let left = usize::try_from(range.start.0 - cursor)
                        .map_err(|_| SignalTransferError::InvalidRootBlock)?;
                    spend(context, MaskWorkPhase::Map, 1, cancelled)?;
                    let right = usize::try_from(range.end.0 - cursor)
                        .map_err(|_| SignalTransferError::InvalidRootBlock)?;
                    // Input owner remains alive while policy modifies old PCM.
                    for sample in &mut block.samples[left..right] {
                        spend(context, MaskWorkPhase::Zero, 1, cancelled)?;
                        *sample = [0.0; 2];
                    }
                    self.mark_interval_controlled(
                        range.clone(),
                        start..end,
                        flags.as_mut_slice(),
                        context,
                        cancelled,
                    )?;
                }
                check(context, cancelled)?;
                interleaved.extend(block.samples.into_iter().flatten());
                drop(block.suppressed);
                check(context, cancelled)?;
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
        check(context, cancelled)?;
        let mut samples = sampler
            .render(AudioSample(start.0), frames, window, cancelled)
            .map_err(SignalTransferError::from)?
            .samples;
        check(context, cancelled)?;

        let mut count = 0_usize;
        let mut previous = false;
        for (&muted, sample) in flags.as_slice().iter().zip(&mut samples) {
            spend(context, MaskWorkPhase::Scan, 1, cancelled)?;
            if muted {
                if !previous {
                    count = count.checked_add(1).ok_or(SignalTransferError::Range)?;
                }
                spend(context, MaskWorkPhase::Zero, 1, cancelled)?;
                *sample = [0.0; 2];
            }
            previous = muted;
        }

        // Scratch stays live across the exact destination Vec and Box peak.
        let mut ranges: RangeBuilder<SignalSample> =
            begin_ranges::<SignalSample, C>(context, count)?;
        let mut run = None;
        for (offset, &muted) in flags.as_slice().iter().enumerate() {
            spend(context, MaskWorkPhase::Scan, 1, cancelled)?;
            if muted {
                run.get_or_insert(offset);
            } else if let Some(left) = run.take() {
                spend(context, MaskWorkPhase::Map, 2, cancelled)?;
                ranges.push(signal_range(start, left, offset)?, context)?;
            }
        }
        if let Some(left) = run {
            spend(context, MaskWorkPhase::Map, 2, cancelled)?;
            ranges.push(signal_range(start, left, samples.len())?, context)?;
        }
        let suppressed = ranges.finish(context)?;
        let result = ControlledTransferredBlock {
            start,
            samples,
            suppressed,
        };
        check(context, cancelled)?;
        drop(flags);
        Ok(result)
    }

    fn mark_interval_controlled<C: MaskContext>(
        &self,
        root: Range<AudioSample>,
        output: Range<SignalSample>,
        flags: &mut [bool],
        context: &C,
        cancelled: &AtomicBool,
    ) -> Result<(), StageAudioError> {
        spend(context, MaskWorkPhase::Map, 1, cancelled)?;
        let left = self
            .point_boundary(root.start)?
            .max(i128::from(output.start.0));
        spend(context, MaskWorkPhase::Map, 1, cancelled)?;
        let right = self.point_boundary(root.end)?.min(i128::from(output.end.0));
        if left < right {
            let left = usize::try_from(left - i128::from(output.start.0))
                .map_err(|_| SignalTransferError::Range)?;
            let right = usize::try_from(right - i128::from(output.start.0))
                .map_err(|_| SignalTransferError::Range)?;
            let interval = flags
                .get_mut(left..right)
                .ok_or(SignalTransferError::Range)?;
            for flag in interval {
                spend(context, MaskWorkPhase::Mark, 1, cancelled)?;
                *flag = true;
            }
        }
        Ok(())
    }

    fn mark_outside_controlled<C: MaskContext>(
        &self,
        output: Range<SignalSample>,
        flags: &mut [bool],
        context: &C,
        cancelled: &AtomicBool,
    ) -> Result<(), StageAudioError> {
        let support = self.root_support();
        spend(context, MaskWorkPhase::Map, 1, cancelled)?;
        let left = self
            .point_boundary(support.start)?
            .clamp(i128::from(output.start.0), i128::from(output.end.0));
        spend(context, MaskWorkPhase::Map, 1, cancelled)?;
        let right = self
            .point_boundary(support.end)?
            .clamp(i128::from(output.start.0), i128::from(output.end.0));
        let left = usize::try_from(left - i128::from(output.start.0))
            .map_err(|_| SignalTransferError::Range)?;
        let right = usize::try_from(right - i128::from(output.start.0))
            .map_err(|_| SignalTransferError::Range)?;
        for flag in &mut flags[..left] {
            spend(context, MaskWorkPhase::Mark, 1, cancelled)?;
            *flag = true;
        }
        for flag in &mut flags[right..] {
            spend(context, MaskWorkPhase::Mark, 1, cancelled)?;
            *flag = true;
        }
        Ok(())
    }
}
