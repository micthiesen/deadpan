//! Actual full retained probe. All returned data precede their own charge.
use super::*;
use crate::AudioSoundContextAdmission;
use crate::plan::audio::AudioWalkSpan;
use crate::plan::audio_bound::query::GuardedBound;
use crate::plan::audio_owners::admission::{
    self as tracked, Budget as QueryBudget, Error, Owned, Result as QueryResult,
};
use std::sync::Arc;

/// Sealed shared handle to the same accepted retained value. Identity is only
/// process-local sharing identity, never source or ancestry authority.
#[derive(Debug, Clone)]
pub struct SharedAudioOwnerRetainedInput<'plan>(Arc<Option<AudioOwnerRetainedInput<'plan>>>);
impl<'plan> std::ops::Deref for SharedAudioOwnerRetainedInput<'plan> {
    type Target = AudioOwnerRetainedInput<'plan>;
    fn deref(&self) -> &Self::Target {
        self.0
            .as_ref()
            .as_ref()
            .expect("initialized retained handle")
    }
}
impl SharedAudioOwnerRetainedInput<'_> {
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn as_ptr(&self) -> *const () {
        Arc::as_ptr(&self.0).cast()
    }
}
impl<'plan> AudioOwnerRetainedInput<'plan> {
    /// One legacy shell allocation at the final accepted-retained sharing site.
    pub fn into_shared(self) -> SharedAudioOwnerRetainedInput<'plan> {
        SharedAudioOwnerRetainedInput(Arc::new(Some(self)))
    }
}

/// Owns the actual optional retained graph and its independent Core/Plan charges.
pub struct AdmittedAudioOwnerRetainedProbe<'plan, I, L> {
    pub(in crate::plan) value: GuardedBound<Option<AudioOwnerRetainedInput<'plan>>, I, L>,
    work: usize,
    canonical: usize,
    additional: usize,
    forwarded: bool,
}
impl<'plan, I: Copy + Eq, L> AdmittedAudioOwnerRetainedProbe<'plan, I, L> {
    pub fn input(&self) -> Option<&AudioOwnerRetainedInput<'plan>> {
        self.value.as_ref()
    }
    pub fn work(&self) -> usize {
        self.work
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
    pub(in crate::plan) fn validate<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        &self,
        admission: &A,
    ) -> std::result::Result<(), A::Error> {
        if self.value._memory.issuer != admission.issuer() {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }
        if self.forwarded != A::METERED {
            return Err(PlanError::InvalidPlan("probe accounting mode mismatch").into());
        }
        Ok(())
    }
}
impl<'plan> AudioOwnerInput<'plan> {
    pub fn retained_probe_at_admitted<A: AudioSoundContextAdmission>(
        &self,
        at: AudioSample,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> std::result::Result<AdmittedAudioOwnerRetainedProbe<'plan, A::Issuer, A::Lease>, A::Error>
    {
        limits.validate()?;
        if !self.context.root_samples().contains(&at) {
            return Err(PlanError::AudioRangeOutOfRange.into());
        }
        let mut local = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut budget = QueryBudget::new(&mut local, admission);
        tracked::outer::<_, A>(probe(self, at, limits, &mut budget))
    }
}
fn probe<'plan, A: AudioSoundContextAdmission>(
    input: &AudioOwnerInput<'plan>,
    at: AudioSample,
    limits: AudioQueryLimits,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<AdmittedAudioOwnerRetainedProbe<'plan, A::Issuer, A::Lease>, A> {
    let mut editorial = Owned {
        value: EditorialCapture::default(),
        memory: budget.memory(),
    };
    let walked = input.context.plan.audio_walk_inner_admitted(
        at,
        budget,
        Some(&input.context.seed),
        true,
        true,
        (None, Some(&mut editorial), None),
    );
    #[cfg(test)]
    {
        use crate::plan::audio_owners::tests::legacy_snapshot_v1::capture_baseline;
        match &walked {
            Ok(value) => capture_baseline::probe_walk_ref(
                input.context.plan,
                at,
                Ok(&value.value),
                &editorial.value,
                (limits.maximum_work, budget.remaining),
            ),
            Err(Error::Plan(error)) => capture_baseline::probe_walk_ref(
                input.context.plan,
                at,
                Err(error),
                &editorial.value,
                (limits.maximum_work, budget.remaining),
            ),
            Err(Error::Policy(_)) => capture_baseline::probe_policy_refusal(
                input.context.plan,
                at,
                &editorial.value,
                editorial.memory.lease.is_some(),
                (limits.maximum_work, budget.remaining),
            ),
        }
    }
    let mut walked = walked?;
    let value = match &*walked.value {
        AudioWalkSpan::Stage(span) if matches!(span.content, AudioSignalContent::Bound(_)) => {
            let AudioSignalContent::Bound(bound) = &span.content else {
                unreachable!()
            };
            let mut raw = bound.raw_domain_admitted(budget)?;
            let work = limits.maximum_work - budget.remaining;
            // The mapped T owns the complete retained value even on Core's
            // post-map validation failure. The outside Plan guards remain live.
            let mut value = walked.value.map(budget, |span| {
                let AudioWalkSpan::Stage(processing) = span else {
                    unreachable!()
                };
                Some(AudioOwnerRetainedInput {
                    plan: input.context.plan,
                    processing,
                    editorial: editorial.value,
                    domain: raw.value,
                    work,
                })
            })?;
            value.merge_memory(&mut walked.memory, budget)?;
            value.merge_memory(&mut editorial.memory, budget)?;
            value.merge_memory(&mut raw.memory, budget)?;
            value
        }
        _ => {
            // Actual complete non-Bound result and its sequential workspace
            // die at the old None match, before a returned proof is formed.
            drop(walked);
            drop(editorial);
            GuardedBound::plain(None, budget.issuer)
        }
    };
    budget.check_memory(&value._memory)?;
    let work = limits.maximum_work - budget.remaining;
    #[cfg(test)]
    crate::plan::audio_owners::tests::legacy_snapshot_v1::capture_baseline::probe_result(
        input.context.plan,
        value.as_ref(),
        work,
    );
    let result = AdmittedAudioOwnerRetainedProbe {
        value,
        work,
        canonical: budget.canonical_used,
        additional: budget.additional_used,
        forwarded: A::METERED,
    };
    result.validate(budget.admission).map_err(Error::Policy)?;
    if result.canonical.checked_add(result.additional) != Some(result.work) {
        return Err(PlanError::InvalidPlan("probe work provenance mismatch").into());
    }
    Ok(result)
}

impl<'plan, I: Copy + Eq, L> AdmittedAudioOwnerRetainedProbe<'plan, I, L> {
    /// Consume the SAME accepted Some. The actual owner/boundary decision must
    /// precede this one final Arc shell allocation.
    pub fn into_shared_admitted<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        mut self,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> std::result::Result<
        crate::plan::audio_domain::AdmittedOriginalContext<'plan, I, L>,
        A::Error,
    > {
        self.validate(admission)?;
        limits.validate()?;
        if self.input().is_none() {
            return Err(PlanError::InvalidPlan("cannot share absent retained probe").into());
        }
        let mut local = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut budget = QueryBudget::new(&mut local, admission);
        tracked::outer::<_, A>((|| {
            // Check the fixed Plan/budget issuer AND the actual stored Core
            // issuer/mode before any new work, reservation or shell allocation.
            // The unused real token changes neither receipt nor transfer state.
            budget.check_memory(&self.value._memory)?;
            {
                let _validated = self.value.preflight_transfer(budget.admission)?;
            }
            budget.check_memory(&self.value._memory)?;
            let mut shell = Owned {
                value: None,
                memory: budget.memory(),
            };
            budget.additional(1)?;
            budget.reserve(
                &mut shell.memory,
                crate::audio_allocated::admission::arc::<Option<AudioOwnerRetainedInput<'plan>>>()?,
            )?;
            shell.value = Some(Arc::new(None));
            let mut value = self.value.map(&mut budget, |retained| {
                initialize_shared(
                    shell.value.take().expect("prepared retained Arc shell"),
                    retained,
                )
            })?;
            value.merge_memory(&mut shell.memory, &mut budget)?;
            let issuer = budget.issuer;
            Ok(crate::plan::audio_domain::AdmittedOriginalContext::from_guarded(value, issuer))
        })())
    }
}

// Exactly one strong/no weak references; Arc has never escaped this private
// slot. No policy callback receives the Arc. The move allocates nothing.
fn initialize_shared<'plan>(
    mut arc: Arc<Option<AudioOwnerRetainedInput<'plan>>>,
    retained: Option<AudioOwnerRetainedInput<'plan>>,
) -> SharedAudioOwnerRetainedInput<'plan> {
    *Arc::get_mut(&mut arc).expect("private unique retained Arc") = retained;
    SharedAudioOwnerRetainedInput(arc)
}
#[cfg(test)]
#[path = "audio_owner_probe_tests.rs"]
mod tests;
