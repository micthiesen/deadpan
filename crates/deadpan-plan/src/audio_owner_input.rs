//! Complete raw input contexts for post-time/pitch owner processing.

use deadpan_core::{AudioSample, TimeError};

use super::audio::Budget;
use super::{
    AudioDomain, AudioOwnerClock, AudioOwnerKind, AudioQueryLimits, LookupStats, RenderPlan,
};
use crate::{AudioSoundContextAdmission, PlanError};

#[path = "audio_owner_input/candidates.rs"]
mod candidates;
#[path = "audio_owner_input/span_start.rs"]
mod span_start;
pub use candidates::{
    AdmittedAudioOwnerInputCandidates, AdmittedAudioOwnerInputCapture, AudioOwnerInputCandidates,
    AudioOwnerInputCapture,
};
pub use span_start::{
    AdmittedAudioOwnerSpanStartCandidates, AdmittedAudioSpanStartInputCapture,
    AudioOwnerSpanStartRequest, SharedAudioOwnerQueryWork,
};

/// Whether the raw context is this owner or a processor surrounding it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioOwnerInputScope {
    Owner,
    /// Preserve or a retained binding owns the complete raw processor. Its
    /// output may also contain other descendant owners. Never treat all of it
    /// as the requested owner's contribution or move an effect into its input.
    EnclosingProcessor,
}

/// A checked raw processing context, before authored treatments and independent
/// sounds. This is an input to effect planning, not a complete voice/mix graph.
/// The owner's clock is an observation at the resolving sample; query the
/// context's owners again when crossing a child, play or retained clock boundary.
/// A retained binding has a separate reference history: `retained_at` exposes
/// its complete operand and exact map beyond the current delivery interval.
#[derive(Debug, Clone)]
pub struct AudioOwnerInput<'plan> {
    owner: AudioOwnerClock<'plan>,
    observed_at: AudioSample,
    context: AudioDomain<'plan>,
    scope: AudioOwnerInputScope,
    work: usize,
}

impl<'plan> AudioOwnerInput<'plan> {
    pub fn owner(&self) -> &AudioOwnerClock<'plan> {
        &self.owner
    }
    /// The root sample at which this owner and its processing context were
    /// checked together. In particular, retained lookup must use this sample,
    /// not an arbitrary extrapolation of the owner's nominal clock.
    pub fn observed_at(&self) -> AudioSample {
        self.observed_at
    }
    /// Signed root-grid delivery context. It may span a whole Sequence or
    /// Repeat, and may include hidden samples outside current root allocation.
    /// `scope` identifies whether an enclosing processor must remain intact.
    pub fn context(&self) -> &AudioDomain<'plan> {
        &self.context
    }
    pub fn scope(&self) -> AudioOwnerInputScope {
        self.scope
    }
    pub fn work(&self) -> usize {
        self.work
    }
}

impl RenderPlan {
    /// Resolve complete raw context for an owner observed at this root sample.
    /// Retain the first enclosing Preserve instead of reconstructing its
    /// output from nominal automation coordinates. Full owner contribution,
    /// ordered treatments and per-voice silence/tail gates are separate work.
    pub fn audio_owner_input(
        &self,
        owner: &AudioOwnerClock<'_>,
        at: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerInput<'_>, PlanError> {
        let mut admission = super::audio_owners::admission::Unmetered;
        let capture = self.audio_owner_input_admitted(owner, at, limits, &mut admission)?;
        Ok(capture
            .unmetered()
            .into_input()
            .expect("direct owner capture"))
    }

    /// Resolve the same exact current owner observation while admitting its
    /// membership query, comparison work and complete source input capture.
    pub fn audio_owner_input_admitted<A: AudioSoundContextAdmission>(
        &self,
        owner: &AudioOwnerClock<'_>,
        at: AudioSample,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedAudioOwnerInputCapture<'_, A::Issuer, A::Lease>, A::Error> {
        limits.validate()?;
        if !owner.belongs_to(self) {
            return Err(PlanError::InvalidPlan("foreign audio input owner").into());
        }
        // The checked at+1 and actual gain-owner query retain their existing
        // order inside the shared candidate kernel after these direct checks.
        self.audio_owner_input_candidates_at_admitted(at, limits, admission)?
            .capture_observed_owner(owner, admission)
    }
}

impl<'plan> AudioDomain<'plan> {
    /// Resolve a child or enclosing processor from this checked context even
    /// when its sample is hidden outside current project delivery. Membership
    /// and capture use the same seed and exact clock, never a root lookup.
    pub fn owner_input(
        &self,
        owner: &AudioOwnerClock<'_>,
        at: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerInput<'plan>, PlanError> {
        let mut admission = super::audio_owners::admission::Unmetered;
        let capture = self.owner_input_admitted(owner, at, limits, &mut admission)?;
        Ok(capture
            .unmetered()
            .into_input()
            .expect("direct owner capture"))
    }

    /// Admit exact owner membership and complete capture on this physical
    /// domain's own seed, including meaningful hidden support.
    pub fn owner_input_admitted<A: AudioSoundContextAdmission>(
        &self,
        owner: &AudioOwnerClock<'_>,
        at: AudioSample,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedAudioOwnerInputCapture<'plan, A::Issuer, A::Lease>, A::Error> {
        limits.validate()?;
        if !owner.belongs_to(self.plan) {
            return Err(PlanError::InvalidPlan("foreign audio input owner").into());
        }
        self.owner_input_candidates_at_admitted(at, limits, admission)?
            .capture_observed_owner(owner, admission)
    }
}

/// Capture one member of the actual checked owner query. Both the ordinary
/// membership API and sealed one-sample candidates use this exact walk.
fn capture_observed<'plan>(
    plan: &'plan RenderPlan,
    observed: &AudioOwnerClock<'plan>,
    at: AudioSample,
    context: Option<&super::audio_domain::AudioWalkSeed>,
    query_work: usize,
    budget: &mut Budget,
) -> Result<AudioOwnerInput<'plan>, PlanError> {
    use super::audio_owners::admission::{self, Unmetered};
    let mut policy = Unmetered;
    let mut admitted = admission::Budget::new(budget, &mut policy);
    admission::outer::<_, Unmetered>(
        capture_observed_admitted(plan, observed, at, context, query_work, &mut admitted)
            .map(admission::Owned::unmetered),
    )
}

fn capture_observed_admitted<'plan, A: crate::AudioSoundContextAdmission>(
    plan: &'plan RenderPlan,
    observed: &AudioOwnerClock<'plan>,
    at: AudioSample,
    context: Option<&super::audio_domain::AudioWalkSeed>,
    query_work: usize,
    budget: &mut super::audio_owners::admission::Budget<'_, A>,
) -> super::audio_owners::admission::Result<
    super::audio_owners::admission::Owned<AudioOwnerInput<'plan>, A::Issuer, A::Lease>,
    A,
> {
    use super::audio_owners::admission::Owned;
    let before = budget.remaining;
    let mut captured = Owned {
        value: None,
        memory: budget.memory(),
    };
    if A::METERED {
        let walked = plan.audio_walk_inner_admitted(
            at,
            budget,
            context,
            true,
            true,
            (Some(&mut captured), None, Some(observed)),
        );
        // The COMPLETE actual span and all workspace/Core/Box guards end at
        // the legacy discard, before the observed owner's unpaid old clone.
        drop(walked?);
    } else {
        plan.audio_walk_owner_input(at, budget, observed, &mut captured.value, context)?;
        budget.canonical_used = budget
            .canonical_used
            .checked_add(before - budget.remaining)
            .ok_or(deadpan_core::TimeError::Overflow)?;
    }
    captured.try_map(budget, |seed, memory, budget| {
        let seed = seed.ok_or(PlanError::InvalidPlan("owner input was not captured"))?;
        let kind = if seed.walk.gap.is_some() {
            AudioOwnerKind::DefaultGap
        } else {
            AudioOwnerKind::Node
        };
        let scope = if seed.instance == *observed.instance()
            && kind == observed.kind()
            && seed.walk.gap.as_ref().and_then(|gap| gap.after.as_ref()) == observed.gap_after()
        {
            AudioOwnerInputScope::Owner
        } else {
            AudioOwnerInputScope::EnclosingProcessor
        };
        Ok(AudioOwnerInput {
            owner: budget.owner(memory, observed)?,
            observed_at: at,
            context: AudioDomain {
                plan,
                seed: seed.walk,
                samples: seed.samples,
                visible: seed.visible,
                instance: seed.instance,
                placement: None,
            },
            scope,
            work: query_work
                .checked_add(before - budget.remaining)
                .ok_or(deadpan_core::TimeError::Overflow)?,
        })
    })
}
