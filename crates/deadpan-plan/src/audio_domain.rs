//! Borrowed physical audio contexts in their original absolute root grid.

use std::ops::Range;

use deadpan_core::{
    AudioSample, ExactFrameRange, ExactRatio, FrameDuration, InstancePath, IterationId,
    RepeatInstance,
};
use serde::{Deserialize, Serialize};

use super::audio::{AudioWalkSpan, Budget, EnvelopeConstraint};
use super::{
    AudioDefinitionSelector, AudioProcessingQuery, AudioQuery, AudioQueryLimits, AudioRetimeStage,
    AudioTransform, LookupStats, RenderPlan,
};
use crate::PlanError;

/// Evaluation placement of an owned physical definition in an absolute root
/// clock. This is a sampling description, not an authored edit or media token.
/// The support is in the selected definition's local output frames.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RootPlacementWire", deny_unknown_fields)]
pub struct AudioRootPlacement {
    origin: ExactRatio,
    root_frames_per_local_frame: ExactRatio,
    local_support: Range<ExactRatio>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RootPlacementWire {
    origin: ExactRatio,
    root_frames_per_local_frame: ExactRatio,
    local_support: ExactFrameRange,
}

impl TryFrom<RootPlacementWire> for AudioRootPlacement {
    type Error = PlanError;

    fn try_from(wire: RootPlacementWire) -> Result<Self, Self::Error> {
        Self::new(
            wire.origin,
            wire.root_frames_per_local_frame,
            wire.local_support.start..wire.local_support.end,
        )
    }
}

impl AudioRootPlacement {
    pub fn new(
        origin: ExactRatio,
        root_frames_per_local_frame: ExactRatio,
        local_support: Range<ExactRatio>,
    ) -> Result<Self, PlanError> {
        if !root_frames_per_local_frame.compare_integer(0).is_gt()
            || local_support.start.compare_integer(0).is_lt()
            || !local_support
                .end
                .checked_sub(local_support.start)?
                .compare_integer(0)
                .is_gt()
        {
            return Err(PlanError::InvalidAudioRootPlacement(
                "scale and support must be positive and support must be nonnegative",
            ));
        }
        // Fail before a later walk if either projected endpoint overflows.
        for point in [local_support.start, local_support.end] {
            origin.checked_add(point.checked_mul(root_frames_per_local_frame)?)?;
        }
        Ok(Self {
            origin,
            root_frames_per_local_frame,
            local_support,
        })
    }

    pub fn origin(&self) -> ExactRatio {
        self.origin
    }
    pub fn root_frames_per_local_frame(&self) -> ExactRatio {
        self.root_frames_per_local_frame
    }
    pub fn local_support(&self) -> Range<ExactRatio> {
        self.local_support.clone()
    }
}

#[derive(Debug, Clone)]
pub(super) struct DomainGap {
    pub(super) after: IterationId,
    pub(super) duration: FrameDuration,
}

/// A walker entry point, retaining ancestor semantics without traversing their
/// allocation again. In a gap, `transform` already names the gap's local zero.
#[derive(Debug, Clone)]
pub(super) struct AudioWalkSeed {
    pub(super) definition: Option<AudioDefinitionSelector>,
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

/// One physical leaf, Repeat gap or nonunity Preserve output, borrowed from
/// the plan that resolved it. An optional definition scope distinguishes an
/// explicitly placed owned recipe from a project occurrence. Transparent
/// allocation may hide support; the absolute round-even root grid never changes.
///
/// This handle is neither serialized admission nor an authored resume binding.
#[derive(Debug, Clone)]
pub struct AudioDomain<'plan> {
    pub(super) plan: &'plan RenderPlan,
    pub(super) seed: AudioWalkSeed,
    pub(super) samples: Range<AudioSample>,
    pub(super) visible: Range<AudioSample>,
    pub(super) instance: InstancePath,
    pub(super) placement: Option<AudioRootPlacement>,
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
            placement: None,
        })
    }
}

impl<'plan> AudioDomain<'plan> {
    /// Present only when this domain evaluates an explicitly selected definition.
    /// Its occurrence path is then relative to that definition.
    pub fn definition(&self) -> Option<&AudioDefinitionSelector> {
        self.seed.definition.as_ref()
    }

    pub fn placement(&self) -> Option<&AudioRootPlacement> {
        self.placement.as_ref()
    }
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
