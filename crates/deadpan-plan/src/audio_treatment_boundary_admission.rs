//! Admitted copy of one treatment boundary. The value precedes its charge.
use super::{
    AudioOwnerClock, AudioQueryLimits, AudioTreatmentBoundary, Budget, LookupStats,
    admission::{self, Owned},
};
use crate::AudioSoundContextAdmission;
use deadpan_core::AudioSample;

/// The exact immutable boundary and its independent opaque charge remain
/// together through publication. This carrier intentionally cannot be cloned.
pub struct AdmittedAudioTreatmentBoundary<'plan, I, L> {
    boundary: Owned<AudioTreatmentBoundary<'plan>, I, L>,
    additional: usize,
    forwarded: bool,
}

impl<'plan, I: Copy + Eq, L> AdmittedAudioTreatmentBoundary<'plan, I, L> {
    pub fn owner(&self) -> &AudioOwnerClock<'plan> {
        self.boundary.value.owner()
    }

    pub fn before_stage(&self) -> usize {
        self.boundary.value.before_stage()
    }

    pub fn matches(&self, candidate: &AudioOwnerClock<'_>) -> bool {
        self.boundary.value.matches(candidate)
    }

    pub fn matches_clock_at(&self, candidate: &AudioOwnerClock<'_>, at: AudioSample) -> bool {
        self.boundary.value.matches_clock_at(candidate, at)
    }

    pub fn issuer(&self) -> I {
        self.boundary.memory.issuer
    }

    pub fn lease(&self) -> Option<&L> {
        self.boundary.memory.lease.as_ref()
    }

    /// Trusted accounting transfer only: the callback must be infallible,
    /// non-unwinding and allocation-free. It borrows only the actual opaque
    /// lease; the boundary remains in this carrier, including after disarming.
    pub fn with_lease_mut(&mut self, callback: impl FnOnce(Option<&mut L>)) {
        callback(self.boundary.memory.lease.as_mut());
    }

    pub fn work(&self) -> usize {
        self.additional
    }

    /// The legacy boundary constructor performed no canonical query work.
    pub fn canonical_units(&self) -> usize {
        0
    }

    pub fn additional_units(&self) -> usize {
        self.additional
    }

    pub fn was_forwarded(&self) -> bool {
        self.forwarded
    }
}

impl<'plan> AudioOwnerClock<'plan> {
    /// Validate the legacy stage condition first, then the supplied limits,
    /// before inspecting/copying any payload or calling the admission policy.
    /// Copy inspections consume one shared local budget and are additional to
    /// the legacy constructor's zero canonical work.
    pub fn treatment_boundary_admitted<A: AudioSoundContextAdmission>(
        &self,
        before_stage: usize,
        limits: AudioQueryLimits,
        policy: &mut A,
    ) -> Result<AdmittedAudioTreatmentBoundary<'plan, A::Issuer, A::Lease>, A::Error> {
        self.validate_treatment_stage(before_stage)?;
        limits.validate()?;
        let mut local = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut budget = admission::Budget::new(&mut local, policy);
        admission::outer::<_, A>(copy(self, before_stage, &mut budget))
    }
}

fn copy<'plan, A: AudioSoundContextAdmission>(
    owner: &AudioOwnerClock<'plan>,
    before_stage: usize,
    budget: &mut admission::Budget<'_, A>,
) -> admission::Result<AdmittedAudioTreatmentBoundary<'plan, A::Issuer, A::Lease>, A> {
    let mut memory = budget.memory();
    let value = AudioTreatmentBoundary {
        owner: budget.owner(&mut memory, owner)?,
        before_stage,
    };
    budget.check_memory(&memory)?;
    Ok(AdmittedAudioTreatmentBoundary {
        boundary: Owned { value, memory },
        additional: budget.additional_used,
        forwarded: A::METERED,
    })
}
