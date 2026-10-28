//! Complete raw input contexts for post-time/pitch owner processing.

use deadpan_core::{AudioSample, ExactRatio, TimeError};

use super::audio::{AudioWalkSpan, Budget, EditorialCapture};
use super::{
    AudioBound, AudioBoundDomain, AudioDomain, AudioFadeQuery, AudioOwnerClock, AudioOwnerKind,
    AudioProcessingSpan, AudioQueryLimits, AudioSignalContent, LookupStats, RenderPlan,
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

    /// Compatibility lookup for consumers that do not account preparation
    /// work. Controlled preparation should use retained_probe_at instead.
    pub fn retained_at(
        &self,
        at: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<Option<AudioOwnerRetainedInput<'plan>>, PlanError> {
        Ok(self.retained_probe_at(at, limits)?.into_input())
    }

    /// The first opaque retained input at this context sample and the work of
    /// the same lookup, including a successful lookup that finds no input.
    /// Do not descend through an enclosing Preserve to find one: that would
    /// move post-time/pitch effects into its input. No PCM is opened here.
    pub fn retained_probe_at(
        &self,
        at: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerRetainedProbe<'plan>, PlanError> {
        limits.validate()?;
        if !self.context.root_samples().contains(&at) {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let mut budget = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut editorial = EditorialCapture::default();
        let walked = self.context.plan.audio_walk_editorial(
            at,
            &mut budget,
            Some(&self.context.seed),
            (true, true),
            &mut editorial,
        )?;
        let input = match walked {
            AudioWalkSpan::Stage(span) if matches!(span.content, AudioSignalContent::Bound(_)) => {
                let AudioSignalContent::Bound(bound) = &span.content else {
                    unreachable!()
                };
                let domain = bound.raw_domain()?;
                Some(AudioOwnerRetainedInput {
                    plan: self.context.plan,
                    processing: span,
                    editorial,
                    domain,
                    work: limits.maximum_work - budget.remaining,
                })
            }
            _ => None,
        };
        Ok(AudioOwnerRetainedProbe {
            input,
            work: limits.maximum_work - budget.remaining,
        })
    }
}

/// Result of one checked retained-input lookup. An absent operand still has
/// actual traversal work. Owns the same optional input without cloning it.
#[derive(Debug)]
pub struct AudioOwnerRetainedProbe<'plan> {
    input: Option<AudioOwnerRetainedInput<'plan>>,
    work: usize,
}

impl<'plan> AudioOwnerRetainedProbe<'plan> {
    pub fn input(&self) -> Option<&AudioOwnerRetainedInput<'plan>> {
        self.input.as_ref()
    }

    pub fn work(&self) -> usize {
        self.work
    }

    pub fn into_input(self) -> Option<AudioOwnerRetainedInput<'plan>> {
        self.input
    }
}

/// A retained processor's full raw operand and its exact map from consumer
/// sample labels. Reference coordinates stay on their RootRoundEven or PointCeil
/// grid; they are never fabricated by inverting a nominal automation clock.
#[derive(Debug, Clone)]
pub struct AudioOwnerRetainedInput<'plan> {
    pub(super) plan: &'plan RenderPlan,
    processing: AudioProcessingSpan<'plan>,
    editorial: EditorialCapture,
    domain: AudioBoundDomain<'plan>,
    work: usize,
}

impl<'plan> AudioOwnerRetainedInput<'plan> {
    pub(super) fn clock_origin(&self) -> super::AudioOwnerClockOrigin {
        super::AudioOwnerClockOrigin::Retained {
            definition: self.processing.definition.clone(),
            instance: self.processing.instance.clone(),
            kind: if self.processing.gap_after.is_some() {
                AudioOwnerKind::DefaultGap
            } else {
                AudioOwnerKind::Node
            },
            gap_after: self.processing.gap_after.clone(),
        }
    }
    /// Checked immutable operand. Its offset origin is the original allocated
    /// start, retained by `reference_at`; serialized metadata cannot replace it.
    pub fn bound(&self) -> &AudioBound<'plan> {
        let AudioSignalContent::Bound(bound) = &self.processing.content else {
            unreachable!()
        };
        bound
    }
    pub fn domain(&self) -> &AudioBoundDomain<'plan> {
        &self.domain
    }
    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        self.bound().belongs_to(plan)
    }
    pub fn reference_at(&self, sample: AudioSample) -> Result<ExactRatio, PlanError> {
        self.bound().reference_at_wide_offset(
            i128::from(sample.0) - i128::from(self.processing.allocated_samples.start.0),
        )
    }
    pub fn reference_samples_per_output_sample(&self) -> ExactRatio {
        self.bound().reference_samples_per_output_sample()
    }
    pub fn work(&self) -> usize {
        self.work
    }

    /// Creative output fades on the original consumer grid, including retained
    /// history outside current delivery. This does not grant output allocation
    /// or a silence/tail permission. Empty history remains a raw-reader policy.
    pub fn fades(
        &self,
        samples: std::ops::Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioFadeQuery, PlanError> {
        limits.validate()?;
        if samples.end < samples.start {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let mut budget = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut spans = Vec::new();
        let mut cursor = samples.start;
        while cursor < samples.end {
            if spans.len() == limits.maximum_spans {
                return Err(PlanError::AudioQueryLimit("retained fade span count"));
            }
            budget.spend(
                1 + self.bound().work()
                    + self.processing.retimes.len()
                    + self.processing.instance.repeats.len()
                    + self.editorial.editorial.len()
                    + self.editorial.constraints.len()
                    + self.editorial.repeats.len(),
            )?;
            let span = super::audio_fades::retained_bound_fade(
                self.processing.clone(),
                cursor,
                samples.end,
                self.editorial.clone(),
                &mut budget,
            )?;
            if span.samples.start != cursor
                || span.samples.end <= cursor
                || span.samples.end > samples.end
            {
                return Err(PlanError::InvalidPlan(
                    "retained fade interval did not advance",
                ));
            }
            cursor = span.samples.end;
            spans.push(span);
        }
        Ok(AudioFadeQuery {
            spans,
            lookup: budget.lookup,
            work: limits.maximum_work - budget.remaining,
        })
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
        resolve_input(self, owner, at, limits, query, None)
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
        limits.validate()?;
        if !owner.belongs_to(self.plan) {
            return Err(PlanError::InvalidPlan("foreign audio input owner"));
        }
        let end = AudioSample(at.0.checked_add(1).ok_or(TimeError::Overflow)?);
        let query = self.gain_owners(at..end, limits)?;
        resolve_input(self.plan, owner, at, limits, query, Some(&self.seed))
    }
}

fn resolve_input<'plan>(
    plan: &'plan RenderPlan,
    owner: &AudioOwnerClock<'_>,
    at: AudioSample,
    limits: AudioQueryLimits,
    query: super::AudioOwnerQuery<'plan>,
    context: Option<&super::audio_domain::AudioWalkSeed>,
) -> Result<AudioOwnerInput<'plan>, PlanError> {
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
    plan.audio_walk_owner_input(at, &mut budget, observed, &mut captured, context)?;
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
        work: limits.maximum_work - budget.remaining,
    })
}
