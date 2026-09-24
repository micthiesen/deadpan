//! Borrowed physical audio contexts in their original absolute root grid.

use std::ops::Range;

use deadpan_core::{
    AudioSample, ExactRatio, FrameDuration, InstancePath, IterationId, RepeatInstance,
};

use super::audio::{AudioWalkSpan, Budget, EnvelopeConstraint};
use super::{
    AudioProcessingQuery, AudioQuery, AudioQueryLimits, AudioRetimeStage, AudioTransform,
    LookupStats, RenderPlan,
};
use crate::PlanError;

#[derive(Debug, Clone)]
pub(super) struct DomainGap {
    pub(super) after: IterationId,
    pub(super) duration: FrameDuration,
}

/// A walker entry point, retaining ancestor semantics without traversing their
/// allocation again. In a gap, `transform` already names the gap's local zero.
#[derive(Debug, Clone)]
pub(super) struct AudioWalkSeed {
    pub(super) node: usize,
    pub(super) transform: AudioTransform,
    pub(super) extent: Range<ExactRatio>,
    pub(super) envelope: Option<Range<ExactRatio>>,
    pub(super) constraints: Vec<EnvelopeConstraint>,
    pub(super) repeats: Vec<RepeatInstance>,
    pub(super) retimes: Vec<AudioRetimeStage>,
    pub(super) gap: Option<DomainGap>,
}

pub(super) struct AudioDomainSeed {
    pub(super) walk: AudioWalkSeed,
    pub(super) samples: Range<AudioSample>,
    pub(super) visible: Range<AudioSample>,
    pub(super) instance: InstancePath,
}

/// One physical leaf, Repeat gap or nonunity Preserve occurrence, borrowed
/// from the plan that resolved it. Transparent allocation may hide part of its
/// meaningful support; its absolute round-even root grid never changes.
///
/// This handle is neither serialized admission nor an authored resume binding.
#[derive(Debug, Clone)]
pub struct AudioDomain<'plan> {
    plan: &'plan RenderPlan,
    seed: AudioWalkSeed,
    samples: Range<AudioSample>,
    visible: Range<AudioSample>,
    instance: InstancePath,
}

impl RenderPlan {
    /// Resolve one allocated root sample, stopping before nonunity Preserve
    /// processing. FollowSpeed and unity retimes remain transparent.
    pub fn audio_domain_at(
        &self,
        sample: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<AudioDomain<'_>, PlanError> {
        limits.validate()?;
        if sample.0 < 0 || sample >= self.audio_duration()? {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let mut budget = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut captured = None;
        self.audio_walk(sample, &mut budget, None, true, Some(&mut captured))?;
        let seed = captured.ok_or(PlanError::InvalidPlan("audio domain was not captured"))?;
        Ok(AudioDomain {
            plan: self,
            seed: seed.walk,
            samples: seed.samples,
            visible: seed.visible,
            instance: seed.instance,
        })
    }
}

impl<'plan> AudioDomain<'plan> {
    /// Complete meaningful support, rounded on the original absolute root grid.
    /// It can begin before zero or end beyond the visible project's duration.
    pub fn root_samples(&self) -> Range<AudioSample> {
        self.samples.clone()
    }

    pub fn root_extent(&self) -> Range<ExactRatio> {
        self.seed.extent.clone()
    }

    /// The allocation through which this particular physical context was found.
    pub fn visible_samples(&self) -> Range<AudioSample> {
        self.visible.clone()
    }

    pub fn instance(&self) -> &InstancePath {
        &self.instance
    }

    pub fn gap_after(&self) -> Option<&IterationId> {
        self.seed.gap.as_ref().map(|gap| &gap.after)
    }

    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }

    fn budget(
        &self,
        samples: &Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<Budget, PlanError> {
        limits.validate()?;
        if samples.start < self.samples.start
            || samples.end < samples.start
            || samples.end > self.samples.end
        {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        Ok(Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        })
    }

    /// Flatten policies, source support and original envelope edges from the
    /// physical subtree. This does not authorize bypassing Preserve DSP.
    pub fn audio(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioQuery, PlanError> {
        let mut budget = self.budget(&samples, limits)?;
        let mut spans = Vec::new();
        let mut cursor = samples.start;
        while cursor < samples.end {
            if spans.len() == limits.maximum_spans {
                return Err(PlanError::AudioQueryLimit("span count"));
            }
            let AudioWalkSpan::Leaf(mut span) =
                self.plan
                    .audio_walk(cursor, &mut budget, Some(&self.seed), false, None)?
            else {
                return Err(PlanError::InvalidPlan("flattened audio retained a stage"));
            };
            span.samples = cursor..span.allocated_samples.end.min(samples.end);
            cursor = span.samples.end;
            spans.push(span);
        }
        Ok(AudioQuery {
            project_id: self.plan.metadata.project_id.clone(),
            revision_id: self.plan.metadata.revision_id.clone(),
            samples,
            spans,
            lookup: budget.lookup,
        })
    }

    /// Resolve processing on the captured root grid. Preserve stages keep their
    /// full intrinsic input history, independently of this visible allocation.
    pub fn processing(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioProcessingQuery<'plan>, PlanError> {
        let mut budget = self.budget(&samples, limits)?;
        let mut spans = Vec::new();
        let mut cursor = samples.start;
        while cursor < samples.end {
            if spans.len() == limits.maximum_spans {
                return Err(PlanError::AudioQueryLimit("span count"));
            }
            let mut span = self
                .plan
                .audio_walk(cursor, &mut budget, Some(&self.seed), true, None)?
                .into_processing();
            span.samples = cursor..span.allocated_samples.end.min(samples.end);
            cursor = span.samples.end;
            spans.push(span);
        }
        Ok(AudioProcessingQuery {
            project_id: self.plan.metadata.project_id.clone(),
            revision_id: self.plan.metadata.revision_id.clone(),
            samples,
            spans,
            lookup: budget.lookup,
        })
    }
}
