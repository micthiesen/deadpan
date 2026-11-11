//! Exact pre-ownership native transfer operations for frozen test replays.
use super::*;

impl StageAudio {
    pub(super) fn read_domain_transferred_legacy<
        'plan,
        T: crate::domain_transfer::DomainTransferView<'plan>,
    >(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        transfer: &T,
        start: SignalSample,
        frames: u32,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<SignalBlock, StageAudioError> {
        control.check()?;
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        if !transfer.domain().belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        let mut dependencies = Dependencies::new();
        let mut relative_depth = 0;
        let anchor = transfer.descriptor().root_support.start.0;
        let block = transfer.carrier().render::<StageAudioError>(
            start,
            frames,
            control.cancelled,
            |at, count| {
                // `at` indexes a zero-based carrier of already allocated root
                // samples. Restore the integer label, not a rounded frame origin.
                let absolute = AudioSample(at.0.checked_add(anchor).ok_or(TimeError::Overflow)?);
                let mut block = self.read_domain_controlled(
                    provider,
                    transfer.domain(),
                    absolute,
                    count,
                    control,
                    depth,
                )?;
                dependencies.extend(block.dependencies);
                relative_depth = relative_depth.max(block.relative_depth);
                let mut input = block.suppressed.into_legacy_vector();
                input.append(&mut block.exhausted);
                let suppressed = input
                    .into_iter()
                    .map(|range| {
                        Ok::<_, TimeError>(
                            AudioSample(
                                range
                                    .start
                                    .0
                                    .checked_sub(anchor)
                                    .ok_or(TimeError::Overflow)?,
                            )
                                ..AudioSample(
                                    range.end.0.checked_sub(anchor).ok_or(TimeError::Overflow)?,
                                ),
                        )
                    })
                    .collect::<Result<_, _>>()?;
                Ok(RootSignalBlock {
                    start: at,
                    samples: block.samples,
                    suppressed,
                })
            },
        )?;
        control.check()?;
        Ok(SignalBlock {
            samples: block.samples,
            dependencies,
            suppressed: SignalSuppressionMask::plain(block.suppressed),
            relative_depth,
        })
    }

    pub(super) fn read_root_transferred_legacy(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        transfer: &RootSignalTransfer,
        start: SignalSample,
        frames: u32,
        control: WorkControl<'_>,
    ) -> Result<crate::signal_transfer::ControlledTransferredBlock, StageAudioError> {
        let block =
            transfer.render::<StageAudioError>(start, frames, control.cancelled, |at, count| {
                let mut block = self.read_controlled(provider, at, count, control, false, 0)?;
                let mut suppressed = block.suppressed.into_legacy_vector();
                suppressed.append(&mut block.exhausted);
                Ok(RootSignalBlock {
                    start: block.start,
                    samples: block.samples,
                    suppressed,
                })
            })?;
        Ok(crate::signal_transfer::ControlledTransferredBlock {
            start: block.start,
            samples: block.samples,
            suppressed: SignalSuppressionMask::plain(block.suppressed),
        })
    }
}
