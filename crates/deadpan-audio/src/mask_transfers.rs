//! Recursive transfers retain one request and own every new mask destination.
use super::*;
use crate::signal_transfer::{ControlledRootBlock, ControlledTransferredBlock};

impl StageAudio {
    pub(super) fn read_domain_transferred_controlled<
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
        #[cfg(test)]
        if internal_mask_request::legacy() {
            return self
                .read_domain_transferred_legacy(provider, transfer, start, frames, control, depth);
        }
        control.check()?;
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        if !transfer.domain().belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        let anchor = transfer.descriptor().root_support.start.0;
        let mut context = mask_context::TransferContext::new(self, provider, control);
        let block = transfer.carrier().render_controlled(
            start,
            frames,
            control.cancelled,
            &mut context,
            |context, at, count| {
                // Rebase only the integer carrier label; the captured grid stays put.
                let absolute = AudioSample(at.0.checked_add(anchor).ok_or(TimeError::Overflow)?);
                let mut block = context.renderer.read_domain_controlled(
                    context.provider,
                    transfer.domain(),
                    absolute,
                    count,
                    context.control,
                    depth,
                )?;
                context.dependencies.extend(block.dependencies);
                context.relative_depth = context.relative_depth.max(block.relative_depth);
                block
                    .suppressed
                    .append_exhausted(&mut block.exhausted, context)?;
                let suppressed = block.suppressed.try_map(
                    |range| {
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
                    },
                    context,
                )?;
                Ok(ControlledRootBlock {
                    start: at,
                    samples: block.samples,
                    suppressed,
                })
            },
        )?;
        control.check()?;
        Ok(SignalBlock {
            samples: block.samples,
            dependencies: context.dependencies,
            suppressed: block.suppressed,
            relative_depth: context.relative_depth,
        })
    }

    pub(super) fn read_root_transferred_controlled(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        transfer: &RootSignalTransfer,
        start: SignalSample,
        frames: u32,
        control: WorkControl<'_>,
    ) -> Result<ControlledTransferredBlock, StageAudioError> {
        #[cfg(test)]
        if internal_mask_request::legacy() {
            return self.read_root_transferred_legacy(provider, transfer, start, frames, control);
        }
        let mut context = mask_context::TransferContext::new(self, provider, control);
        transfer.render_controlled(
            start,
            frames,
            control.cancelled,
            &mut context,
            |context, at, count| {
                let mut block = context.renderer.read_controlled(
                    context.provider,
                    at,
                    count,
                    context.control,
                    false,
                    0,
                )?;
                block
                    .suppressed
                    .append_exhausted(&mut block.exhausted, context)?;
                Ok(ControlledRootBlock {
                    start: block.start,
                    samples: block.samples,
                    suppressed: block.suppressed,
                })
            },
        )
    }
}
