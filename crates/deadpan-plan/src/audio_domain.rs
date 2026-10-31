//! Borrowed physical audio contexts in their original absolute root grid.

use std::ops::Range;

use deadpan_core::{
    AudioSample, ExactFrameRange, ExactRatio, FrameDuration, InstancePath, IterationId,
    RepeatInstance,
};
use serde::{Deserialize, Serialize};

use super::audio::{Budget, EnvelopeConstraint};
use super::{
    AudioDefinitionSelector, AudioProcessingQuery, AudioQuery, AudioQueryLimits, AudioRetimeStage,
    AudioTransform, LookupStats, RenderPlan,
};
use crate::PlanError;

#[cfg(test)]
#[path = "audio_domain_policy_reuse_tests.rs"]
mod policy_reuse_tests;
#[path = "audio_context_sharing.rs"]
mod sharing;
pub use sharing::{
    AdmittedOriginalContext, ContextChargeTransfer, SharedAudioOwnerRetainedInput,
    preflight_context_transfer, transfer_context_charges,
};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct DomainGap {
    /// A definition has no preceding play. Occurrences retain their stable ID.
    pub(super) after: Option<IterationId>,
    pub(super) duration: FrameDuration,
}

/// A walker entry point, retaining ancestor semantics without traversing their
/// allocation again. In a gap, `transform` already names the gap's local zero.
#[derive(Debug, Clone)]
pub(super) struct AudioWalkSeed {
    pub(super) definition: Option<AudioDefinitionSelector>,
    pub(super) bypass_binding: Option<super::audio_bound::BindingTarget>,
    pub(super) node: usize,
    pub(super) transform: AudioTransform,
    pub(super) grid: crate::AudioSampleGrid<AudioSample>,
    pub(super) extent: Range<ExactRatio>,
    pub(super) envelope: Option<Range<ExactRatio>>,
    pub(super) constraints: Vec<EnvelopeConstraint>,
    pub(super) editorial: Vec<super::audio::EditorialConstraint>,
    /// A bound fade inspection has already transported these sides of its
    /// entry owner's marker. Do not recreate them without eligibility probes.
    pub(super) suppress_entry_editorial: deadpan_core::AudioEditorialEdges,
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

/// A physical leaf, Repeat gap, nonunity Preserve output or checked owner
/// subtree, borrowed from the plan that resolved it. An optional definition
/// scope distinguishes an explicitly placed owned recipe from a project
/// occurrence. Transparent allocation may hide support; the absolute round-even
/// root grid never changes.
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

/// One successful processing query sealed to the exact borrowed physical
/// Domain and requested range. It cannot be built from a caller-owned query.
/// Its storage is the ordinary processing query's owned vectors; the seal and
/// terminal witness add only stack metadata, with no independent allocation.
pub struct AudioDomainProcessing<'context, 'plan> {
    domain: &'context AudioDomain<'plan>,
    samples: Range<AudioSample>,
    query: AudioProcessingQuery<'plan>,
    stops: super::audio::capture::ProcessingStops,
}

/// The completed queries after the single-use processing capability has been
/// consumed. Processing work belongs to the first query, while policy work and
/// lookups report only operations actually performed while finishing policy.
pub struct AudioDomainProcessingPolicy<'plan> {
    processing: AudioProcessingQuery<'plan>,
    policy: super::AudioPolicyQuery<AudioSample>,
}

impl<'plan> AudioDomainProcessing<'_, 'plan> {
    /// Inspect and bill this actual first processing query once before policy.
    pub fn query(&self) -> &AudioProcessingQuery<'plan> {
        &self.query
    }

    /// Finish policy against the identical Domain object and original range.
    /// Fresh limits cover new policy work only; they must not subtract or bill
    /// the first query again. An exposed processing stage uses ordinary policy.
    pub fn finish_policy(
        self,
        caller: &AudioDomain<'plan>,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioDomainProcessingPolicy<'plan>, PlanError> {
        if !std::ptr::eq(self.domain, caller) || self.samples != samples {
            return Err(PlanError::InvalidPlan(
                "processing policy identity mismatch",
            ));
        }
        let policy = if self.stops.permits_binding_reuse() {
            super::audio_policy::domain_policy_with_processing(
                caller,
                samples,
                limits,
                &self.query.spans,
            )?
        } else {
            caller.policy(samples, limits)?
        };
        Ok(AudioDomainProcessingPolicy {
            processing: self.query,
            policy,
        })
    }
}

impl<'plan> AudioDomainProcessingPolicy<'plan> {
    pub fn into_queries(
        self,
    ) -> (
        AudioProcessingQuery<'plan>,
        super::AudioPolicyQuery<AudioSample>,
    ) {
        (self.processing, self.policy)
    }
}

impl crate::audio_allocated::OwnedHeap for DomainGap {
    fn visit_heap(&self, budget: &mut crate::audio_allocated::HeapBudget) -> Result<(), PlanError> {
        let Self { after, duration: _ } = self;
        budget.visit(after)
    }
}

impl crate::audio_allocated::OwnedHeap for AudioDomain<'_> {
    fn visit_heap(&self, budget: &mut crate::audio_allocated::HeapBudget) -> Result<(), PlanError> {
        let Self {
            plan: _,
            seed,
            samples: _,
            visible: _,
            instance,
            placement,
        } = self;
        let AudioWalkSeed {
            definition,
            bypass_binding: _,
            node: _,
            transform: _,
            grid: _,
            extent: _,
            envelope: _,
            constraints,
            editorial,
            suppress_entry_editorial: _,
            repeats,
            retimes,
            gap,
        } = seed;
        budget.visit(definition)?;
        budget.visit(constraints)?;
        budget.visit(editorial)?;
        budget.visit(repeats)?;
        budget.visit(retimes)?;
        budget.visit(gap)?;
        budget.visit(instance)?;
        budget.visit(placement)
    }
}

impl crate::audio_allocated::OwnedHeap for AudioRootPlacement {
    fn visit_heap(
        &self,
        _budget: &mut crate::audio_allocated::HeapBudget,
    ) -> Result<(), PlanError> {
        let Self {
            origin: _,
            root_frames_per_local_frame: _,
            local_support: _,
        } = self;
        Ok(())
    }
}

impl crate::audio_allocated::OwnedHeap for EnvelopeConstraint {
    fn visit_heap(&self, budget: &mut crate::audio_allocated::HeapBudget) -> Result<(), PlanError> {
        let Self {
            placement_support: _,
            range: _,
            node: _,
            repeat_count: _,
            gap_after,
            kinds: _,
        } = self;
        budget.visit(gap_after)
    }
}

impl crate::audio_allocated::OwnedHeap for super::audio::EditorialConstraint {
    fn visit_heap(&self, budget: &mut crate::audio_allocated::HeapBudget) -> Result<(), PlanError> {
        let Self {
            boundary,
            edges: _,
            incident_samples: _,
        } = self;
        budget.visit(boundary)
    }
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
        self.audio_walk(sample, &mut budget, None, true, true, Some(&mut captured))?;
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
    pub(crate) fn sample_grid(&self) -> crate::AudioSampleGrid<AudioSample> {
        self.seed.grid
    }
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
        self.seed.gap.as_ref().and_then(|gap| gap.after.as_ref())
    }

    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }
    pub(super) fn has_audio_bindings(&self) -> bool {
        self.plan.has_audio_bindings()
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
        use super::audio_owners::admission::Unmetered;
        self.audio_admitted(samples, limits, &mut Unmetered)
            .map(super::audio::AdmittedAudioQuery::unmetered)
    }

    /// Construct the actual flattened query under one allocation/work policy.
    /// The returned carrier retains its charge for every immutable query borrow.
    pub fn audio_admitted<A: crate::AudioSoundContextAdmission>(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<super::audio::AdmittedAudioQuery<A::Issuer, A::Lease>, A::Error> {
        let local = self.budget(&samples, limits)?;
        super::audio::flattened_query::query(
            self.plan,
            Some(&self.seed),
            samples,
            limits,
            local,
            admission,
        )
    }

    /// Resolve processing on the captured root grid. Preserve stages keep their
    /// full intrinsic input history, independently of this visible allocation.
    pub fn processing(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioProcessingQuery<'plan>, PlanError> {
        self.processing_inner(samples, limits, true)
    }

    /// Co-produce a private terminal witness during the actual processing walk
    /// so policy can reuse its binding tape without a second eligibility pass.
    pub fn processing_for_policy(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioDomainProcessing<'_, 'plan>, PlanError> {
        let mut stops = super::audio::capture::ProcessingStops::default();
        let query = self.processing_observing(samples.clone(), limits, true, Some(&mut stops))?;
        Ok(AudioDomainProcessing {
            domain: self,
            samples,
            query,
            stops,
        })
    }

    pub(super) fn processing_inner(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
        stop_at_preserve: bool,
    ) -> Result<AudioProcessingQuery<'plan>, PlanError> {
        self.processing_observing(samples, limits, stop_at_preserve, None)
    }

    fn processing_observing(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
        stop_at_preserve: bool,
        mut stops: Option<&mut super::audio::capture::ProcessingStops>,
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
                .audio_walk_processing(
                    cursor,
                    &mut budget,
                    &self.seed,
                    stop_at_preserve,
                    stops.as_deref_mut(),
                )?
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
            work: limits.maximum_work - budget.remaining,
        })
    }
}
