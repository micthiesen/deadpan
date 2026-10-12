//! Complete raw input contexts for post-time/pitch owner processing.

use deadpan_core::{AudioSample, ExactRatio, TimeError};

use super::audio::Budget;
use super::{
    AudioBound, AudioBoundDomain, AudioDomain, AudioOwnerClock, AudioOwnerKind, AudioQueryLimits,
    AudioSignalContent, LookupStats, RenderPlan,
};
use crate::PlanError;

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
    context: AudioDomain<'plan>,
    scope: AudioOwnerInputScope,
    work: usize,
}

impl<'plan> AudioOwnerInput<'plan> {
    pub fn owner(&self) -> &AudioOwnerClock<'plan> {
        &self.owner
    }
    /// Signed root-grid delivery context. It may span a whole Sequence or
    /// Repeat, and may include hidden samples outside current root allocation.
    /// `scope` identifies whether an enclosing processor must remain intact.
    /// This interval is not the complete reference history of a resumed binding;
    /// use `retained_at` to resolve that separate input instead of zero padding.
    pub fn context(&self) -> &AudioDomain<'plan> {
        &self.context
    }
    pub fn scope(&self) -> AudioOwnerInputScope {
        self.scope
    }
    pub fn work(&self) -> usize {
        self.work
    }

    /// The first opaque retained input at this context sample, if present.
    /// Do not descend through an enclosing Preserve to find one: that would
    /// move post-time/pitch effects into its input. No PCM is opened here.
    pub fn retained_at(
        &self,
        at: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<Option<AudioOwnerRetainedInput<'plan>>, PlanError> {
        let end = AudioSample(at.0.checked_add(1).ok_or(TimeError::Overflow)?);
        let mut query = self.context.processing(at..end, limits)?;
        let span = query
            .spans
            .pop()
            .ok_or(PlanError::InvalidPlan("owner input has no processing span"))?;
        match span.content {
            AudioSignalContent::Bound(bound) => {
                let domain = bound.raw_domain()?;
                Ok(Some(AudioOwnerRetainedInput {
                    bound: *bound,
                    anchor: span.allocated_samples.start,
                    domain,
                    work: query.work,
                }))
            }
            _ => Ok(None),
        }
    }
}

/// A retained processor's full raw operand and its exact map from consumer
/// sample labels. Reference coordinates stay on their RootRoundEven or PointCeil
/// grid; they are never fabricated by inverting a nominal automation clock.
#[derive(Debug, Clone)]
pub struct AudioOwnerRetainedInput<'plan> {
    bound: AudioBound<'plan>,
    anchor: AudioSample,
    domain: AudioBoundDomain<'plan>,
    work: usize,
}

impl<'plan> AudioOwnerRetainedInput<'plan> {
    pub fn domain(&self) -> &AudioBoundDomain<'plan> {
        &self.domain
    }
    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        self.bound.belongs_to(plan)
    }
    pub fn reference_at(&self, sample: AudioSample) -> Result<ExactRatio, PlanError> {
        self.bound
            .reference_at_wide_offset(i128::from(sample.0) - i128::from(self.anchor.0))
    }
    pub fn reference_samples_per_output_sample(&self) -> ExactRatio {
        self.bound.reference_samples_per_output_sample()
    }
    pub fn work(&self) -> usize {
        self.work
    }
}

impl RenderPlan {
    /// Resolve complete raw context for an owner observed at this root sample.
    /// Retain the first enclosing Preserve/binding instead of reconstructing its
    /// output from nominal automation coordinates. Full owner contribution,
    /// ordered treatments and per-voice silence/tail gates are separate work.
    pub fn audio_owner_input(
        &self,
        owner: &AudioOwnerClock<'_>,
        at: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerInput<'_>, PlanError> {
        limits.validate()?;
        if !owner.belongs_to(self) {
            return Err(PlanError::InvalidPlan("foreign audio input owner"));
        }
        let end = AudioSample(at.0.checked_add(1).ok_or(TimeError::Overflow)?);
        let query = self.audio_gain_owners(at..end, limits)?;
        let observed = query
            .spans()
            .first()
            .and_then(|span| {
                span.owners()
                    .iter()
                    .find(|candidate| candidate.same_at(owner, at))
            })
            .ok_or(PlanError::InvalidPlan(
                "audio input owner is not current at sample",
            ))?;
        let mut budget = Budget {
            remaining: limits.maximum_work - query.work(),
            lookup: LookupStats::default(),
        };
        let mut captured = None;
        self.audio_walk_owner_input(at, &mut budget, observed, &mut captured)?;
        let seed = captured.ok_or(PlanError::InvalidPlan("owner input was not captured"))?;
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
            owner: observed.clone(),
            context: AudioDomain {
                plan: self,
                seed: seed.walk,
                samples: seed.samples,
                visible: seed.visible,
                instance: seed.instance,
                placement: None,
            },
            scope,
            work: limits.maximum_work - budget.remaining,
        })
    }
}
