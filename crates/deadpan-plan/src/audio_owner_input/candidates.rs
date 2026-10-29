//! One actual owner observation query paired with its actual capture parent.
//! Selection consumes a checked member index, never an external owner clock.

use super::*;
use crate::plan::AudioOwnerQuery;

enum Parent<'context, 'plan> {
    Root(&'plan RenderPlan),
    Domain(&'context AudioDomain<'plan>),
}

/// Owns one one-sample gain-owner query and borrows the exact parent which
/// produced it. It cannot be constructed from a query or cloned for replay.
pub struct AudioOwnerInputCandidates<'context, 'plan> {
    parent: Parent<'context, 'plan>,
    at: AudioSample,
    query: AudioOwnerQuery<'plan>,
    limits: AudioQueryLimits,
}

/// Work receipt for the same query and optional capture. Query work is always
/// present, including no selection. Input::work retains the total provenance;
/// a caller that already billed query_work must bill only capture_work.
#[derive(Debug)]
pub struct AudioOwnerInputCapture<'plan> {
    input: Option<AudioOwnerInput<'plan>>,
    query_work: usize,
    capture_work: usize,
}

impl<'plan> AudioOwnerInputCapture<'plan> {
    pub fn query_work(&self) -> usize {
        self.query_work
    }

    pub fn capture_work(&self) -> usize {
        self.capture_work
    }

    pub fn work(&self) -> usize {
        self.query_work + self.capture_work
    }

    pub fn input(&self) -> Option<&AudioOwnerInput<'plan>> {
        self.input.as_ref()
    }

    pub fn into_input(self) -> Option<AudioOwnerInput<'plan>> {
        self.input
    }
}

impl<'context, 'plan> AudioOwnerInputCandidates<'context, 'plan> {
    pub fn query(&self) -> &AudioOwnerQuery<'plan> {
        &self.query
    }

    pub fn work(&self) -> usize {
        self.query.work()
    }

    /// Consume the already performed query without issuing a capture or
    /// requiring any new limits. Its actual work is not lost on absence.
    pub fn without_selection(self) -> AudioOwnerInputCapture<'plan> {
        AudioOwnerInputCapture {
            input: None,
            query_work: self.query.work(),
            capture_work: 0,
        }
    }

    /// Capture one member of this exact one-sample query. The caller supplies
    /// only an index into its actual first span; parent and sample stay sealed.
    /// capture_limits bounds additional work, while the original query limit
    /// still bounds total query+capture work.
    pub fn capture(
        self,
        member: usize,
        capture_limits: AudioQueryLimits,
    ) -> Result<AudioOwnerInputCapture<'plan>, PlanError> {
        capture_limits.validate()?;
        let observed = self
            .query
            .spans()
            .first()
            .filter(|span| span.samples().contains(&self.at))
            .and_then(|span| span.owners().get(member))
            .ok_or(PlanError::InvalidPlan(
                "audio input candidate index out of range",
            ))?;
        let query_work = self.query.work();
        let available = self
            .limits
            .maximum_work
            .checked_sub(query_work)
            .ok_or(PlanError::AudioQueryLimit("candidate query work"))?;
        let capture_limit = available.min(capture_limits.maximum_work);
        // Zero here is an exhausted INTERNAL budget, not invalid caller
        // limits. Let the existing capture walker report structural work.
        let mut budget = Budget {
            remaining: capture_limit,
            lookup: LookupStats::default(),
        };
        let input = match self.parent {
            Parent::Root(plan) => {
                capture_observed(plan, observed, self.at, None, query_work, &mut budget)?
            }
            Parent::Domain(domain) => capture_observed(
                domain.plan,
                observed,
                self.at,
                Some(&domain.seed),
                query_work,
                &mut budget,
            )?,
        };
        Ok(AudioOwnerInputCapture {
            input: Some(input),
            query_work,
            capture_work: capture_limit - budget.remaining,
        })
    }

    fn checked(
        parent: Parent<'context, 'plan>,
        at: AudioSample,
        query: AudioOwnerQuery<'plan>,
        limits: AudioQueryLimits,
    ) -> Result<Self, PlanError> {
        if query.spans().len() > 1 {
            return Err(PlanError::InvalidPlan(
                "one-sample owner query has multiple spans",
            ));
        }
        Ok(Self {
            parent,
            at,
            query,
            limits,
        })
    }
}

impl RenderPlan {
    /// Query actual gain-owner candidates on this plan at one sample. An
    /// eventual capture reuses this membership query and this exact root.
    pub fn audio_owner_input_candidates_at(
        &self,
        at: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerInputCandidates<'_, '_>, PlanError> {
        let end = AudioSample(at.0.checked_add(1).ok_or(TimeError::Overflow)?);
        let query = self.audio_gain_owners(at..end, limits)?;
        AudioOwnerInputCandidates::checked(Parent::Root(self), at, query, limits)
    }
}

impl<'plan> AudioDomain<'plan> {
    /// Query candidates from this actual captured seed, including meaningful
    /// hidden support. Neither selection nor capture performs a root lookup.
    pub fn owner_input_candidates_at(
        &self,
        at: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerInputCandidates<'_, 'plan>, PlanError> {
        let end = AudioSample(at.0.checked_add(1).ok_or(TimeError::Overflow)?);
        let query = self.gain_owners(at..end, limits)?;
        AudioOwnerInputCandidates::checked(Parent::Domain(self), at, query, limits)
    }
}

// Query-only guard. Actual parent/query value drops before its independent lease.
pub struct AdmittedAudioOwnerInputCandidates<'context, 'plan, I, L> {
    value: Option<AudioOwnerInputCandidates<'context, 'plan>>,
    canonical: usize,
    additional: usize,
    forwarded: bool,
    memory: crate::plan::audio_owners::admission::Memory<I, L>,
}
impl<'context, 'plan, I: Copy + Eq, L> AdmittedAudioOwnerInputCandidates<'context, 'plan, I, L> {
    pub fn query(&self) -> &AudioOwnerQuery<'plan> {
        self.value.as_ref().expect("unconsumed query").query()
    }
    pub fn work(&self) -> usize {
        self.query().work()
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
    fn validate<A: crate::AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        &self,
        admission: &A,
    ) -> Result<(), A::Error> {
        if self.memory.issuer != admission.issuer() {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }
        if self.forwarded != A::METERED {
            return Err(PlanError::InvalidPlan("candidate accounting mode mismatch").into());
        }
        Ok(())
    }
    pub fn capture<A: crate::AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        mut self,
        member: usize,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AudioOwnerInputCapture<'plan>, A::Error> {
        self.validate(admission)?;
        // Keep self.memory live throughout the SAME actual legacy call.
        // Its query is consumed/dropped inside capture before lease release.
        let result = self
            .value
            .take()
            .expect("unconsumed query")
            .capture(member, limits)
            .map_err(A::Error::from)?;
        self.validate(admission)?;
        Ok(result)
    }
    pub fn without_selection<A: crate::AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        mut self,
        admission: &mut A,
    ) -> Result<AudioOwnerInputCapture<'plan>, A::Error> {
        self.validate(admission)?;
        let result = self
            .value
            .take()
            .expect("unconsumed query")
            .without_selection();
        self.validate(admission)?;
        Ok(result)
    }
}
impl RenderPlan {
    pub fn audio_owner_input_candidates_at_admitted<A: crate::AudioSoundContextAdmission>(
        &self,
        at: AudioSample,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedAudioOwnerInputCandidates<'_, '_, A::Issuer, A::Lease>, A::Error> {
        let product = crate::plan::audio_owners::query_admission::root_candidates_query(
            self, at, limits, admission,
        )?;
        let value = AudioOwnerInputCandidates::checked(
            Parent::Root(self),
            at,
            product.query.value,
            limits,
        )?;
        if product.query.memory.issuer != admission.issuer() {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }
        Ok(AdmittedAudioOwnerInputCandidates {
            value: Some(value),
            canonical: product.canonical,
            additional: product.additional,
            forwarded: A::METERED,
            memory: product.query.memory,
        })
    }
}
impl<'plan> AudioDomain<'plan> {
    pub fn owner_input_candidates_at_admitted<A: crate::AudioSoundContextAdmission>(
        &self,
        at: AudioSample,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedAudioOwnerInputCandidates<'_, 'plan, A::Issuer, A::Lease>, A::Error> {
        let product = crate::plan::audio_owners::query_admission::domain_candidates_query(
            self, at, limits, admission,
        )?;
        let value = AudioOwnerInputCandidates::checked(
            Parent::Domain(self),
            at,
            product.query.value,
            limits,
        )?;
        if product.query.memory.issuer != admission.issuer() {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }
        Ok(AdmittedAudioOwnerInputCandidates {
            value: Some(value),
            canonical: product.canonical,
            additional: product.additional,
            forwarded: A::METERED,
            memory: product.query.memory,
        })
    }
}
