//! Borrowed owner-query result and one-budget retained-owner admission.
use std::ops::Range;

use deadpan_core::AudioSample;

use super::{
    AudioOwnerQuery, AudioOwnerRetainedInput, AudioOwnerSpan, AudioOwnerSupport, AudioQueryLimits,
    LookupStats,
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

pub(super) fn query<'plan, A: AudioSoundContextAdmission>(
    retained: &AudioOwnerRetainedInput<'plan>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    admission: &mut A,
) -> std::result::Result<AdmittedAudioOwnerQuery<'plan, A::Issuer, A::Lease>, A::Error> {
    limits.validate()?;
    if samples.end < samples.start {
        return Err(PlanError::AudioRangeOutOfRange.into());
    }

    let mut local = super::Budget {
        remaining: limits.maximum_work,
        lookup: LookupStats::default(),
    };
    let mut budget = admission::Budget::new(&mut local, admission);
    let result = retained_query(retained, samples, limits, &mut budget);
    admission::outer::<_, A>(result)
}

fn retained_query<'plan, A: AudioSoundContextAdmission>(
    retained: &AudioOwnerRetainedInput<'plan>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    budget: &mut admission::Budget<'_, A>,
) -> Result<AdmittedAudioOwnerQuery<'plan, A::Issuer, A::Lease>, A> {
    let domain = retained.bound().envelope_domain_query(budget)?;
    let product = match domain {
        None => {
            // Legacy empty-support path has a separate positive-work debit.
            budget.canonical(1)?;
            let mut payload = budget.memory();
            let mut capacity = budget.memory();
            let mut spans = Vec::new();
            if !samples.is_empty() {
                budget.grow(&mut spans, &mut capacity, 1)?;
                spans.push(AudioOwnerSpan {
                    samples: samples.clone(),
                    owners: Vec::new(),
                    support: AudioOwnerSupport::Inactive,
                    original_is_silent: false,
                });
            }
            budget.check_memory(&payload)?;
            budget.check_memory(&capacity)?;
            budget.merge_checked(&mut payload, &mut capacity)?;
            let canonical = budget.canonical_used;
            let additional = budget.additional_used;
            let work = canonical
                .checked_add(additional)
                .ok_or(crate::PlanError::Time(deadpan_core::TimeError::Overflow))?;
            Product {
                query: Owned {
                    value: AudioOwnerQuery {
                        plan: retained.plan,
                        samples,
                        spans,
                        lookup: budget.lookup,
                        work,
                    },
                    memory: payload,
                },
                canonical,
                additional,
            }
        }
        Some(domain) => {
            // The old seed clone preceded query_tracked's second validation.
            // At a zero remainder, a charged copy would change the legacy
            // InvalidAudioLimits result into structural-work exhaustion.
            if A::METERED && budget.remaining == 0 {
                AudioQueryLimits {
                    maximum_work: budget.remaining,
                    ..limits
                }
                .validate()?;
            }
            let mut seed = super::query_admission::State::from_seed(&domain.seed, budget)?;
            seed.value.allow_inactive = true;
            super::query_admission::query_tracked(
                retained.plan,
                samples.start.0..samples.end.0,
                AudioQueryLimits {
                    maximum_work: budget.remaining,
                    ..limits
                },
                seed,
                AudioSample,
                Some(retained),
                budget,
            )?
        }
    };

    AdmittedAudioOwnerQuery::checked(product, budget)
}
