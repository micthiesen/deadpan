//! Wide gain-owner entry points share the actual admitted walk and result.
use super::{
    AdmittedAudioOwnerQuery, AudioDomain, AudioQueryLimits, AudioSample, LookupStats, RenderPlan,
    Walk,
    admission::{self, Result},
    query_admission::{State, query_tracked},
};
use crate::{AudioSoundContextAdmission, PlanError};
use std::ops::Range;

pub(super) fn root<'plan, A: AudioSoundContextAdmission>(
    plan: &'plan RenderPlan,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    admission: &mut A,
) -> std::result::Result<AdmittedAudioOwnerQuery<'plan, A::Issuer, A::Lease>, A::Error> {
    admission::outer::<_, A>((|| {
        // Preserve the legacy range/audio-duration, root clock, then query
        // limits/reversed-range order before entering the actual walk.
        if samples.start.0 < 0 || samples.end > plan.audio_duration()? {
            return Err(PlanError::AudioRangeOutOfRange.into());
        }
        let root = Walk::root(plan)?;
        let mut local = super::Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut budget = admission::Budget::new(&mut local, admission);
        let mut seed = State::plain(root, budget.issuer);
        seed.value.allow_inactive = true;
        run(plan, samples, limits, seed, &mut budget)
    })())
}

pub(super) fn domain<'plan, A: AudioSoundContextAdmission>(
    domain: &AudioDomain<'plan>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    admission: &mut A,
) -> std::result::Result<AdmittedAudioOwnerQuery<'plan, A::Issuer, A::Lease>, A::Error> {
    admission::outer::<_, A>((|| {
        if samples.start < domain.samples.start || samples.end > domain.samples.end {
            return Err(PlanError::AudioRangeOutOfRange.into());
        }
        let mut local = super::Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut budget = admission::Budget::new(&mut local, admission);
        // The legacy seed copy precedes query validation. Meter that same
        // ownership before validation, retaining the original caller limits.
        let mut seed = State::from_seed(&domain.seed, &mut budget)?;
        seed.value.allow_inactive = true;
        run(domain.plan, samples, limits, seed, &mut budget)
    })())
}

fn run<'plan, A: AudioSoundContextAdmission>(
    plan: &'plan RenderPlan,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    seed: State<A::Issuer, A::Lease>,
    budget: &mut admission::Budget<'_, A>,
) -> Result<AdmittedAudioOwnerQuery<'plan, A::Issuer, A::Lease>, A> {
    // Seed copies and traversal consume one local budget. Passing a reduced
    // limit here would change legacy validation when seed work exhausts it.
    let product = query_tracked(
        plan,
        samples.start.0..samples.end.0,
        limits,
        seed,
        AudioSample,
        None,
        budget,
    )?;
    AdmittedAudioOwnerQuery::checked(product, budget)
}
