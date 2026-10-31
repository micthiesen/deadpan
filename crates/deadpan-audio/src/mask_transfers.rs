//! Recursive transfers retain one request and own every new mask destination.
use super::*;
use crate::signal_transfer::{ControlledRootBlock, ControlledTransferredBlock};
use point_suppression::ops::begin_ranges;

impl StageAudio {
    pub(super) fn empty_bound_controlled(
        &mut self,
        frames: u32,
        control: WorkControl<'_>,
    ) -> Result<SignalBlock, StageAudioError> {
        #[cfg(test)]
        if internal_mask_request::legacy() {
            return Ok(empty_bound_block(frames));
        }
        let mut context = self.masks(control);
        let mut mask = begin_ranges::<SignalSample, _>(&mut context, 1)?;
        mask.push(SignalSample(0)..SignalSample(i64::from(frames)), &context)?;
        let suppressed = mask.finish(&context)?;
        control.check()?;
        Ok(SignalBlock {
            samples: vec![[0.0; 2]; frames as usize],
            suppressed,
            ..Default::default()
        })
    }

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
                #[cfg(test)]
                raw_domain_query_trace::nested_callback(raw_domain_query_trace::Product::Root);
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

    pub(super) fn read_point_bound_transferred_controlled(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        domain: &AudioPointDomain<'_>,
        transfer: &RootSignalTransfer,
        frames: u32,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<SignalBlock, StageAudioError> {
        #[cfg(test)]
        if internal_mask_request::legacy() {
            return self.read_point_bound_transferred_legacy(
                provider, domain, transfer, frames, control, depth,
            );
        }
        let mut context = mask_context::TransferContext::new(self, provider, control);
        let block = transfer.render_controlled(
            SignalSample(0),
            frames,
            control.cancelled,
            &mut context,
            |context, storage, count| {
                #[cfg(test)]
                raw_domain_query_trace::nested_callback(raw_domain_query_trace::Product::Point);
                let reference = domain.reference_at_signal(SignalSample(storage.0))?;
                let block = context.renderer.read_point_domain_controlled(
                    context.provider,
                    domain,
                    reference,
                    count,
                    context.control,
                    depth,
                )?;
                context.dependencies.extend(block.dependencies);
                context.relative_depth = context.relative_depth.max(block.relative_depth);
                let suppressed = block.suppressed.map(
                    |range| AudioSample(range.start.0)..AudioSample(range.end.0),
                    context,
                )?;
                Ok(ControlledRootBlock {
                    start: storage,
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
