//! Capture at an active span START of the actual live wide gain query.
use super::candidates::{Parent, capture_in_parent};
use super::*;
use crate::plan::audio_owners::admission::{self, Memory};
use crate::plan::{AdmittedAudioOwnerQuery, AudioOwnerQuery, AudioOwnerSupport};
use std::ops::Range;

/// One shared wide-query receipt. Borrow it for attribution; it is not new
/// per-input work and must never be added to each capture's work receipt.
/// Only the sealed factory constructs it. No owned receipt or clone is exposed.
pub struct SharedAudioOwnerQueryWork {
    canonical: usize,
    additional: usize,
    total: usize,
    forwarded: bool,
}
impl SharedAudioOwnerQueryWork {
    pub fn canonical_units(&self) -> usize {
        self.canonical
    }
    pub fn additional_units(&self) -> usize {
        self.additional
    }
    pub fn work(&self) -> usize {
        self.total
    }
    pub fn was_forwarded(&self) -> bool {
        self.forwarded
    }
}

/// The actual physical parent and its own wide query stay inseparable. A
/// caller cannot supply an unrelated query, rebuild a parent, or extract the
/// query's owned result without its lease.
pub struct AdmittedAudioOwnerSpanStartCandidates<'context, 'plan, I, L> {
    parent: Parent<'context, 'plan>,
    query: AdmittedAudioOwnerQuery<'plan, I, L>,
    shared: SharedAudioOwnerQueryWork,
    issuer: I,
}

/// The request is checked against an active actual span. `at` must equal that
/// span's START; arbitrary interior observations are deliberately unsupported.
pub struct AudioOwnerSpanStartRequest<'owner, 'plan> {
    pub span_index: usize,
    pub requested: &'owner AudioOwnerClock<'plan>,
    pub at: AudioSample,
}

/// Owns only new selection/capture work and its source-input charge. The shared
/// receipt borrow keeps the actual wide-query wrapper alive through use of this
/// input. The input value drops before its independent lease.
pub struct AdmittedAudioSpanStartInputCapture<'query, 'plan, I, L> {
    input: AudioOwnerInput<'plan>,
    canonical: usize,
    additional: usize,
    forwarded: bool,
    shared: &'query SharedAudioOwnerQueryWork,
    _memory: Memory<I, L>,
}
impl<'plan, I, L> AdmittedAudioSpanStartInputCapture<'_, 'plan, I, L> {
    pub fn input(&self) -> &AudioOwnerInput<'plan> {
        &self.input
    }
    pub fn canonical_units(&self) -> usize {
        self.canonical
    }
    pub fn additional_units(&self) -> usize {
        self.additional
    }
    /// New selection plus capture work only. Shared Qwide was paid once by
    /// the caller and is available solely through `shared_query_work`.
    pub fn work(&self) -> usize {
        self.input.work()
    }
    pub fn was_forwarded(&self) -> bool {
        self.forwarded
    }
    pub fn shared_query_work(&self) -> &SharedAudioOwnerQueryWork {
        self.shared
    }
}

impl<'context, 'plan, I: Copy + Eq, L>
    AdmittedAudioOwnerSpanStartCandidates<'context, 'plan, I, L>
{
    pub fn query(&self) -> &AudioOwnerQuery<'plan> {
        self.query.query()
    }
    pub fn shared_query_work(&self) -> &SharedAudioOwnerQueryWork {
        &self.shared
    }

    fn checked<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        parent: Parent<'context, 'plan>,
        query: AdmittedAudioOwnerQuery<'plan, I, L>,
        issuer: I,
        admission: &A,
    ) -> Result<Self, A::Error> {
        let shared = SharedAudioOwnerQueryWork {
            canonical: query.canonical_units(),
            additional: query.additional_units(),
            total: query.work(),
            forwarded: query.was_forwarded(),
        };
        if shared.canonical.checked_add(shared.additional) != Some(shared.total) {
            return Err(
                PlanError::InvalidPlan("shared owner query work provenance mismatch").into(),
            );
        }
        let result = Self {
            parent,
            query,
            shared,
            issuer,
        };
        result.validate(admission)?;
        Ok(result)
    }

    fn validate<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        &self,
        admission: &A,
    ) -> Result<(), A::Error> {
        if self.issuer != admission.issuer() {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }
        if self.shared.forwarded != A::METERED {
            return Err(PlanError::InvalidPlan("span-start accounting mode mismatch").into());
        }
        Ok(())
    }

    pub fn capture_root<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        &self,
        caller: &RenderPlan,
        request: AudioOwnerSpanStartRequest<'_, '_>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedAudioSpanStartInputCapture<'_, 'plan, I, L>, A::Error> {
        self.validate(admission)?;
        if !matches!(&self.parent, Parent::Root(plan) if std::ptr::eq(*plan,caller)) {
            return Err(PlanError::InvalidPlan("foreign span-start capture parent").into());
        }
        self.capture(request, limits, admission)
    }

    pub fn capture_domain<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        &self,
        caller: &AudioDomain<'_>,
        request: AudioOwnerSpanStartRequest<'_, '_>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedAudioSpanStartInputCapture<'_, 'plan, I, L>, A::Error> {
        self.validate(admission)?;
        if !matches!(&self.parent, Parent::Domain(domain) if std::ptr::eq(*domain,caller)) {
            return Err(PlanError::InvalidPlan("foreign span-start capture parent").into());
        }
        self.capture(request, limits, admission)
    }

    fn capture<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        &self,
        request: AudioOwnerSpanStartRequest<'_, '_>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedAudioSpanStartInputCapture<'_, 'plan, I, L>, A::Error> {
        limits.validate()?;
        let span = self
            .query()
            .spans()
            .get(request.span_index)
            .ok_or(PlanError::InvalidPlan("span-start index out of range"))?;
        if span.support() != AudioOwnerSupport::Active {
            return Err(PlanError::InvalidPlan("inactive span-start input").into());
        }
        if request.at != span.samples().start {
            return Err(PlanError::InvalidPlan("input observation is not span start").into());
        }
        let mut local = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let (input, canonical, additional) = {
            let mut budget = admission::Budget::new(&mut local, admission);
            let input = admission::outer::<_, A>((|| {
                // Restriction proof: query_tracked starts a fresh copy of the
                // identical physical Root/Domain seed at each span cursor.
                // Both wide and one-sample gain queries use retained=None and
                // allow_inactive=true; requested end only caps output.end.
                // map_owner anchors at that cursor. Thus at an ACTIVE START,
                // the complete private owner vector/order agrees with the
                // point query. Frozen OLD wide14/73-input/30-point producers
                // independently witness all existing phase/hidden/repeat/gap
                // cases. No arbitrary interior or duplicate uniqueness claim
                // follows: scan to FIRST private same_at exactly as direct.
                let selected =
                    first_match(span.owners(), request.requested, request.at, &mut budget)?;
                let selection = budget.additional_used;
                capture_in_parent(&self.parent, selected, request.at, selection, &mut budget)
            })())?;
            admission::outer::<_, A>(budget.check_memory(&input.memory))?;
            (input, budget.canonical_used, budget.additional_used)
        };
        self.validate(admission)?;
        if canonical.checked_add(additional) != Some(input.value.work()) {
            return Err(PlanError::InvalidPlan("span-start input work provenance mismatch").into());
        }
        Ok(AdmittedAudioSpanStartInputCapture {
            input: input.value,
            canonical,
            additional,
            forwarded: A::METERED,
            shared: &self.shared,
            _memory: input.memory,
        })
    }
}

fn first_match<'owners, 'plan, A: AudioSoundContextAdmission>(
    owners: &'owners [AudioOwnerClock<'plan>],
    requested: &AudioOwnerClock<'_>,
    at: AudioSample,
    budget: &mut admission::Budget<'_, A>,
) -> admission::Result<&'owners AudioOwnerClock<'plan>, A> {
    budget.additional(1)?;
    for owner in owners {
        budget.additional(1)?;
        if owner.same_at(requested, at) {
            return Ok(owner);
        }
    }
    Err(PlanError::InvalidPlan("audio input owner is not current at sample").into())
}

impl RenderPlan {
    /// Run one actual admitted wide gain query and seal it to this exact Root.
    pub fn audio_owner_span_start_candidates_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedAudioOwnerSpanStartCandidates<'_, '_, A::Issuer, A::Lease>, A::Error> {
        let issuer = admission.issuer();
        let query = self.audio_gain_owners_admitted(samples, limits, admission)?;
        AdmittedAudioOwnerSpanStartCandidates::checked(Parent::Root(self), query, issuer, admission)
    }
}
impl<'plan> AudioDomain<'plan> {
    /// Run one actual admitted wide gain query on this physical Domain seed.
    pub fn owner_span_start_candidates_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedAudioOwnerSpanStartCandidates<'_, 'plan, A::Issuer, A::Lease>, A::Error>
    {
        let issuer = admission.issuer();
        let query = self.gain_owners_admitted(samples, limits, admission)?;
        AdmittedAudioOwnerSpanStartCandidates::checked(
            Parent::Domain(self),
            query,
            issuer,
            admission,
        )
    }
}

#[cfg(test)]
#[path = "span_start_tests.rs"]
mod tests;
