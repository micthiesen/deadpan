//! Trusted Signal-policy handoff implementation for the exact final Vec lease.
use super::*;
use crate::stages::point_suppression::policy_handoff::PreparedPolicyMask as NativePreparedMask;
use deadpan_plan::{AudioSignalPolicyMaskHandoff, SignalPolicyMaskRef};
use std::ops::Range;

/// Private token pairing the already allocated destination with the exact
/// ledger subtraction computed while the source lease and query remain live.
pub(in crate::stages) enum PreparedPolicyMask<'work> {
    Empty(NativePreparedMask),
    Transfer {
        destination: NativePreparedMask,
        ledger: &'work RefCell<ReadWork>,
        remaining: u64,
    },
}

fn preflight_source_lease<'work>(
    control: WorkControl<'work>,
    source_issuer: Issuer<'work>,
    current_issuer: Issuer<'work>,
    capacity: usize,
    source_lease: Option<&Lease<'work>>,
    original_lease: &Lease<'work>,
    vector_frames: u64,
) -> Result<u64, StageAudioError> {
    if source_issuer != current_issuer || capacity == 0 {
        return Err(PlanError::AudioMetadataIssuerMismatch.into());
    }
    let lease = source_lease.ok_or(PlanError::InvalidPlan("Point policy mask capacity lease"))?;
    if !std::ptr::eq(lease, original_lease)
        || lease.frames != vector_frames
        || !std::ptr::eq(lease.ledger, control.work)
    {
        return Err(PlanError::AudioMetadataIssuerMismatch.into());
    }

    let remaining = {
        let ledger = control
            .work
            .try_borrow_mut()
            .map_err(|_| StageAudioError::Range)?;
        ledger
            .sound_context_frames
            .checked_sub(vector_frames)
            .ok_or(StageAudioError::Range)?
    };
    Ok(remaining)
}

impl<'work> AudioSignalPolicyMaskHandoff for Admission<'_, 'work> {
    type PreparedMask = PreparedPolicyMask<'work>;
    type Mask = SignalSuppressionMask;

    fn prepare_signal_mask(
        &mut self,
        source: SignalPolicyMaskRef<'_, Self::Issuer, Self::Lease>,
    ) -> Result<Self::PreparedMask, Self::Error> {
        #[cfg(test)]
        crate::stages::routed::projected_tape_source_trace::prepare_mask(
            self.controller,
            self.control,
            source.capacity(),
            source.lease().map(Lease::frames),
        );
        self.control.check()?;
        let current_issuer = issuer(self.control);
        if source.issuer() != current_issuer {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }

        let capacity = source.capacity();
        if capacity == 0 {
            if source.lease().is_some() {
                return Err(PlanError::InvalidPlan("Point policy mask capacity lease").into());
            }
            return Ok(PreparedPolicyMask::Empty(
                crate::stages::point_suppression::policy_handoff::prepare_policy_mask(
                    self.controller,
                    0,
                    self.control,
                )?,
            ));
        }

        let lease = source
            .lease()
            .ok_or(PlanError::InvalidPlan("Point policy mask capacity lease"))?;
        let vector_frames =
            crate::stages::point_suppression::policy_handoff::vector_frames(capacity)?;
        if lease.frames != vector_frames || !std::ptr::eq(lease.ledger, self.control.work) {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }

        let destination = crate::stages::point_suppression::policy_handoff::prepare_policy_mask(
            self.controller,
            capacity,
            self.control,
        )?;

        // Reservations may evict cached metadata. Revalidate the source and
        // compute the exact remaining ledger only after all native admission.
        self.control.check()?;
        if source.capacity() != capacity || destination.vector_frames() != vector_frames {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }

        let remaining = preflight_source_lease(
            self.control,
            source.issuer(),
            current_issuer,
            source.capacity(),
            source.lease(),
            lease,
            vector_frames,
        )?;

        Ok(PreparedPolicyMask::Transfer {
            destination,
            ledger: self.control.work,
            remaining,
        })
    }

    fn commit_signal_mask(
        &mut self,
        prepared: Self::PreparedMask,
        ranges: Vec<Range<SignalSample>>,
        source_lease: Option<&mut Self::Lease>,
    ) -> Self::Mask {
        match prepared {
            PreparedPolicyMask::Empty(destination) => destination.publish(ranges),
            PreparedPolicyMask::Transfer {
                destination,
                ledger,
                remaining,
            } => {
                // Plan's consuming handoff passes the exact lease from its
                // just-preflighted FinalSignalMask, with no callback between.
                // Acquire every checked access before publication starts.
                let lease = source_lease.expect("prepared transfer has its exact source lease");
                let mut ledger = ledger
                    .try_borrow_mut()
                    .expect("preflight released its exclusive ledger borrow");
                let mask = destination.publish(ranges);
                ledger.sound_context_frames = remaining;
                lease.frames = 0;
                mask
            }
        }
    }
}

#[cfg(test)]
#[path = "policy_mask_handoff_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "policy_mask_custody_tests.rs"]
mod custody_tests;
