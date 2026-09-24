//! A physical domain's captured root signal on an explicit preparation grid.
//! Integer carrier rebasing preserves its original absolute allocation phase.
use std::ops::Range;

use deadpan_core::{AudioSample, ExactRatio, TimeError};
use deadpan_plan::{AudioDomain, SignalSample};
use serde::Serialize;

use crate::{RootSignalTransfer, SignalTransferError};

/// Coordinate inspection only. A serialized descriptor cannot manufacture the
/// borrowed plan-owned domain required by a live transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DomainTransferDescriptor {
    pub root_support: Range<AudioSample>,
    pub root_at_anchor: ExactRatio,
    pub signal_anchor: SignalSample,
    pub root_samples_per_signal_sample: ExactRatio,
    pub output: Range<SignalSample>,
}

/// The domain retains its signed captured root coordinates. Only the internal
/// carrier's integer sample labels are rebased; no frame boundary is rerounded
/// and no signal sample is interpreted as a project-root sample.
#[derive(Debug, Clone)]
pub struct DomainSignalTransfer<'plan> {
    domain: AudioDomain<'plan>,
    descriptor: DomainTransferDescriptor,
    carrier: RootSignalTransfer,
}

impl<'plan> DomainSignalTransfer<'plan> {
    pub fn new(
        domain: AudioDomain<'plan>,
        root_at_anchor: ExactRatio,
        signal_anchor: SignalSample,
        root_samples_per_signal_sample: ExactRatio,
        output: Range<SignalSample>,
    ) -> Result<Self, SignalTransferError> {
        let root_support = domain.root_samples();
        let length = root_support
            .end
            .0
            .checked_sub(root_support.start.0)
            .filter(|length| *length > 0)
            .ok_or(SignalTransferError::Range)?;
        let carrier = RootSignalTransfer::new(
            AudioSample(0)..AudioSample(length),
            root_at_anchor.checked_sub(ExactRatio::integer(root_support.start.0))?,
            signal_anchor,
            root_samples_per_signal_sample,
            output.clone(),
        )?;
        Ok(Self {
            domain,
            descriptor: DomainTransferDescriptor {
                root_support,
                root_at_anchor,
                signal_anchor,
                root_samples_per_signal_sample,
                output,
            },
            carrier,
        })
    }

    pub fn domain(&self) -> &AudioDomain<'plan> {
        &self.domain
    }

    pub fn descriptor(&self) -> &DomainTransferDescriptor {
        &self.descriptor
    }

    pub fn root_position(&self, sample: SignalSample) -> Result<ExactRatio, TimeError> {
        self.carrier
            .root_position(sample)?
            .checked_add(ExactRatio::integer(self.descriptor.root_support.start.0))
    }

    pub(crate) fn carrier(&self) -> &RootSignalTransfer {
        &self.carrier
    }
}
