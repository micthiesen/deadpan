//! Borrowed owner-query result and one-budget retained-owner admission.

use deadpan_core::AudioSample;

use super::{
    AudioOwnerQuery,
    admission::{self, Owned, Result},
    query_admission::Product,
};
use crate::{AudioSoundContextAdmission, PlanError};

/// The immutable result and its accounting receipt stay inseparable.
pub struct AdmittedAudioOwnerQuery<'plan, I, L> {
    query: Owned<AudioOwnerQuery<'plan>, I, L>,
    canonical: usize,
    additional: usize,
    forwarded: bool,
}

impl<'plan, I, L> AdmittedAudioOwnerQuery<'plan, I, L> {
    pub fn query(&self) -> &AudioOwnerQuery<'plan> {
        &self.query.value
    }

    pub fn work(&self) -> usize {
        self.query.value.work()
    }

    pub fn canonical_units(&self) -> usize {
        self.canonical
    }

    pub fn additional_units(&self) -> usize {
        self.additional
    }

    pub fn was_forwarded(&self) -> bool {
        self.forwarded
    }
}

impl<'plan, I: Copy + Eq, L> AdmittedAudioOwnerQuery<'plan, I, L> {
    /// All owner-query entry points use the same final issuer and work proof.
    pub(super) fn checked<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        product: Product<'plan, AudioSample, I, L>,
        budget: &admission::Budget<'_, A>,
    ) -> Result<Self, A> {
        budget.check_memory(&product.query.memory)?;
        if product.canonical.checked_add(product.additional) != Some(product.query.value.work()) {
            return Err(PlanError::InvalidPlan("owner query work provenance mismatch").into());
        }
        Ok(Self {
            query: product.query,
            canonical: product.canonical,
            additional: product.additional,
            forwarded: A::METERED,
        })
    }
}

impl<'plan> AdmittedAudioOwnerQuery<'plan, (), ()> {
    pub(super) fn unmetered(self) -> AudioOwnerQuery<'plan> {
        self.query.unmetered()
    }
}
