//! Point-sampled preparation support and root processing boundaries. Neither
//! query renders DSP. Borrowed stages retain their own uncut input history.

use std::ops::Range;
use std::sync::Arc;

use deadpan_core::{
    AudioSample, ExactRatio, FrameDuration, InsertionBias, InstancePath, IterationId,
    MIX_SAMPLE_RATE, NodeId, PitchPolicy, ProjectId, RepeatInstance, RetimePurpose, RevisionId,
    SourcePoint, TimeError,
};
use serde::{Serialize, Serializer, ser::SerializeStruct};

use super::audio::{Budget, intersect, maximum, minimum, source_point_from_local};
use super::audio_bound::BindingTarget;
use super::audio_domain::DomainGap;
use super::audio_owners::admission::{
    Budget as QueryBudget, Owned, Result as QueryResult, Unmetered, outer,
};
use super::{
    AudioContent, AudioDefinition, AudioDefinitionSelector, AudioQueryLimits, AudioRetimeStage,
    AudioTransform, CompiledKind, LookupStats, RenderPlan, SilenceReason, SourceSamplingSupport,
};
use crate::{AudioBoundaryRule, AudioSampleGrid, AudioSampleMap, PlanError};

#[path = "audio_signal_processing_query.rs"]
pub(in crate::plan) mod processing_query;

#[path = "audio_signal_query.rs"]
mod query;

#[path = "audio_source_voice.rs"]
mod source_voice;
pub use source_voice::{AudioSourceVoice, AudioSourceVoiceIdentity, AudioSourceVoiceRecipe};

#[path = "audio_source_occurrence.rs"]
mod source_occurrence;
pub use source_occurrence::{
    AdmittedSoundContext, AudioSoundContextAdmission, AudioSoundOwnerContext,
    AudioSoundPermissionFamily, AudioSoundPlacement, AudioSoundPlacementHistory,
    AudioSoundPlacementHistoryCursor, AudioSoundProcessingHistory, AudioSourceOccurrence,
};

#[path = "audio_source_occurrences.rs"]
mod source_occurrences;
pub use source_occurrences::{
    AudioSourceOccurrenceSupport, AudioSourceOccurrenceSupports, AudioSourceOccurrences,
};

#[derive(Debug, Clone)]
enum SignalProvider<'plan> {
    Structural,
    /// An owned stage's input is a tape, not its old structural child window.
    RequiresOwnedInput,
    /// Raw padding for an independent occurrence voice, with no Original input.
    Silence,
    Projected(Arc<crate::AudioStageProjection<'plan>>),
    Source {
        recipe: Arc<source_voice::SourceVoiceProvider>,
        apply_holds: bool,
    },
}

/// Index in a temporary signal grid, never a project-output sample coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct SignalSample(pub i64);

/// The current signal's frame clock and a leaf/stage's local frame clock.
/// Grid index k maps to `grid_origin + k * signal_frames_per_sample`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SignalTransform {
    pub signal_origin: ExactRatio,
    pub signal_frames_per_local_frame: ExactRatio,
    pub grid_origin: ExactRatio,
    pub signal_frames_per_sample: ExactRatio,
}

impl SignalTransform {
    fn grid(self, rule: AudioBoundaryRule) -> Result<AudioSampleGrid<SignalSample>, PlanError> {
        AudioSampleGrid::new(self.grid_origin, self.signal_frames_per_sample, rule)
    }

    pub fn signal_at(self, sample: SignalSample) -> Result<ExactRatio, TimeError> {
        self.grid_origin
            .checked_add(ExactRatio::integer(sample.0).checked_mul(self.signal_frames_per_sample)?)
    }

    pub fn local_at(self, sample: SignalSample) -> Result<ExactRatio, TimeError> {
        self.local_at_signal_frame(self.signal_at(sample)?)
    }

    pub fn local_at_signal_frame(self, frame: ExactRatio) -> Result<ExactRatio, TimeError> {
        frame
            .checked_sub(self.signal_origin)?
            .checked_div(self.signal_frames_per_local_frame)
    }

    pub(super) fn signal_from_local(self, local: ExactRatio) -> Result<ExactRatio, TimeError> {
        self.signal_origin
            .checked_add(local.checked_mul(self.signal_frames_per_local_frame)?)
    }

    fn child(self, start: ExactRatio, scale: ExactRatio) -> Result<Self, TimeError> {
        Ok(Self {
            signal_origin: self.signal_from_local(start)?,
            signal_frames_per_local_frame: self.signal_frames_per_local_frame.checked_mul(scale)?,
            ..self
        })
    }
}

/// Stable authored processing identity. Selection is in the child's local
/// frame clock; rate consumes child frames per stage output frame. It contains
/// no ancestor crop or consumer-request offset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioStageDescriptor {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    /// When present, the instance is relative to this authored definition,
    /// distinct from any occurrence in the project tree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<AudioDefinitionSelector>,
    pub instance: InstancePath,
    pub child: NodeId,
    pub selection: Range<ExactRatio>,
    pub duration: FrameDuration,
    pub rate: ExactRatio,
    pub pitch: PitchPolicy,
    /// Raw evaluation bypasses this stage's binding only. Descendants remain
    /// active, and this scope participates in preparation cache identity.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bypass_binding: bool,
}

/// A nonunity Preserve stage borrowed from exactly one immutable plan. Only
/// inspection data serializes; a descriptor cannot be deserialized into a live
/// stage or rebound to another plan.
#[derive(Debug, Clone, Serialize)]
pub struct AudioStage<'plan> {
    #[serde(skip)]
    plan: &'plan RenderPlan,
    #[serde(skip)]
    child: usize,
    #[serde(skip)]
    node: usize,
    #[serde(flatten)]
    descriptor: AudioStageDescriptor,
}

impl PartialEq for AudioStage<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.plan, other.plan) && self.descriptor == other.descriptor
    }
}
impl Eq for AudioStage<'_> {}

impl<'plan> AudioStage<'plan> {
    pub(super) fn for_node(
        plan: &'plan RenderPlan,
        node: usize,
        repeats: &[RepeatInstance],
        definition: Option<&AudioDefinitionSelector>,
        bypass_binding: Option<BindingTarget>,
    ) -> Result<Self, PlanError> {
        // This constructor had no local canonical debit; callers retain their
        // existing prepaid copy charge. False mode performs no additional work.
        let mut local = Budget {
            remaining: 0,
            lookup: LookupStats::default(),
        };
        let mut admission = Unmetered;
        outer::<_, Unmetered>(Self::for_node_admitted(
            plan,
            node,
            repeats,
            definition,
            bypass_binding,
            &mut QueryBudget::new(&mut local, &mut admission),
        ))
        .map(Owned::unmetered)
    }

    pub(in crate::plan) fn for_node_admitted<A: AudioSoundContextAdmission>(
        plan: &'plan RenderPlan,
        node: usize,
        repeats: &[RepeatInstance],
        definition: Option<&AudioDefinitionSelector>,
        bypass_binding: Option<BindingTarget>,
        budget: &mut QueryBudget<'_, A>,
    ) -> QueryResult<Owned<Self, A::Issuer, A::Lease>, A> {
        let compiled = &plan.nodes[node];
        if compiled.preserve_input.is_some() {
            return Err(PlanError::InvalidPlan(
                "owned Preserve input requires its compiled projection",
            )
            .into());
        }
        let CompiledKind::Retime {
            child,
            start,
            scale,
            pitch,
            ..
        } = &compiled.kind
        else {
            return Err(PlanError::InvalidPlan("audio stage is not a retime").into());
        };
        if !pitch.processes(*scale == ExactRatio::ONE) {
            return Err(PlanError::InvalidPlan("audio stage is not nonunity Preserve").into());
        }
        if compiled.extent != ExactRatio::integer(compiled.inspection.duration.frames()) {
            return Err(PlanError::InvalidPlan(
                "derived Preserve allocation requires an owned processing route",
            )
            .into());
        }
        let mut memory = budget.memory();
        let mut capacity = budget.memory();
        // Field order matches the existing descriptor, including fallible
        // selection arithmetic AFTER every descriptor identity/path copy.
        let project_id = budget.project(&mut memory, &plan.metadata.project_id)?;
        let revision_id = budget.revision(&mut memory, &plan.metadata.revision_id)?;
        let definition = budget.definition_ref(&mut memory, definition)?;
        let instance = InstancePath {
            node: budget.node(&mut memory, &compiled.inspection.id)?,
            repeats: budget.repeats(&mut memory, &mut capacity, repeats)?,
        };
        let child_id = budget.node(&mut memory, &plan.nodes[*child].inspection.id)?;
        let selection = *start
            ..start.checked_add(
                scale.checked_mul(ExactRatio::integer(compiled.inspection.duration.frames()))?,
            )?;
        let value = Self {
            plan,
            child: *child,
            node,
            descriptor: AudioStageDescriptor {
                project_id,
                revision_id,
                definition,
                instance,
                child: child_id,
                selection,
                duration: compiled.inspection.duration,
                rate: *scale,
                pitch: *pitch,
                bypass_binding: bypass_binding == Some(BindingTarget::Node(node)),
            },
        };
        budget.check_memory(&memory)?;
        budget.check_memory(&capacity)?;
        budget.merge_checked(&mut memory, &mut capacity)?;
        Ok(Owned { value, memory })
    }

    pub fn descriptor(&self) -> &AudioStageDescriptor {
        &self.descriptor
    }

    pub(super) fn for_owned_input(
        plan: &'plan RenderPlan,
        node: usize,
        repeats: Vec<RepeatInstance>,
        definition: AudioDefinitionSelector,
    ) -> Result<Self, PlanError> {
        let compiled = &plan.nodes[node];
        let input = compiled
            .preserve_input
            .as_ref()
            .ok_or(PlanError::InvalidPlan("stage has no owned Preserve input"))?;
        let CompiledKind::Retime { child, pitch, .. } = &compiled.kind else {
            return Err(PlanError::InvalidPlan(
                "owned Preserve stage is not a Retime",
            ));
        };
        Ok(Self {
            plan,
            node,
            child: *child,
            descriptor: AudioStageDescriptor {
                project_id: plan.metadata.project_id.clone(),
                revision_id: plan.metadata.revision_id.clone(),
                definition: Some(definition),
                instance: InstancePath {
                    node: compiled.inspection.id.clone(),
                    repeats,
                },
                child: plan.nodes[*child].inspection.id.clone(),
                selection: input.selection.clone(),
                duration: compiled.inspection.duration,
                rate: input.rate,
                pitch: *pitch,
                bypass_binding: false,
            },
        })
    }

    pub(super) fn owned_descendant_signal(
        &self,
        target: &InstancePath,
    ) -> Result<AudioSignal<'plan>, PlanError> {
        let root = *self
            .plan
            .by_id
            .get(&target.node)
            .ok_or(PlanError::InvalidPlan("owned input provider is missing"))?;
        if !self.owns_descendant(root) {
            return Err(PlanError::InvalidPlan(
                "owned input provider is outside the stage",
            ));
        }
        let mut repeats = self.descriptor.instance.repeats.clone();
        repeats.extend_from_slice(&target.repeats);
        Ok(AudioSignal {
            plan: self.plan,
            root,
            support: ExactRatio::ZERO..self.plan.nodes[root].extent,
            sampling_support: None,
            constrain_support: false,
            repeats,
            definition: self.descriptor.definition.clone(),
            placed_transform: None,
            bypass_binding: None,
            gap: None,
            provider: SignalProvider::Structural,
        })
    }

    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }

    pub(crate) fn plan(&self) -> &'plan RenderPlan {
        self.plan
    }

    pub(crate) fn accepts_projection_signal(&self, signal: &AudioSignal<'_>) -> bool {
        self.belongs_to(signal.plan)
            && self.descriptor.definition == signal.definition
            && self
                .descriptor
                .instance
                .repeats
                .iter()
                .zip(&signal.repeats)
                .all(|(owner, provider)| owner == provider)
            && signal.repeats.len() >= self.descriptor.instance.repeats.len()
            && self.owns_descendant(signal.root)
    }

    pub(crate) fn accepts_projection_stage(&self, nested: &AudioStage<'_>) -> bool {
        self.belongs_to(nested.plan)
            && self.descriptor.definition == nested.descriptor.definition
            && self
                .descriptor
                .instance
                .repeats
                .iter()
                .zip(&nested.descriptor.instance.repeats)
                .all(|(owner, provider)| owner == provider)
            && nested.descriptor.instance.repeats.len() >= self.descriptor.instance.repeats.len()
            && self.owns_descendant(nested.node)
    }

    fn owns_descendant(&self, mut node: usize) -> bool {
        for _ in 0..=deadpan_core::MAX_DOCUMENT_DEPTH {
            if node == self.child {
                return true;
            }
            if node == self.node {
                return false;
            }
            let Some(parent) = self.plan.parents.get(node).copied().flatten() else {
                return false;
            };
            node = parent;
        }
        false
    }

    /// Ordinary structural operand. For an owned input this handle refuses
    /// queries; read the compiled projection's input tape instead.
    pub fn input_signal(&self) -> AudioSignal<'plan> {
        AudioSignal {
            plan: self.plan,
            root: self.child,
            support: self.descriptor.selection.clone(),
            sampling_support: Some(self.descriptor.selection.clone()),
            constrain_support: true,
            repeats: self.descriptor.instance.repeats.clone(),
            definition: self.descriptor.definition.clone(),
            placed_transform: None,
            bypass_binding: None,
            gap: None,
            provider: if self.plan.nodes[self.node].preserve_input.is_some() {
                SignalProvider::RequiresOwnedInput
            } else {
                SignalProvider::Structural
            },
        }
    }

    /// The stage's full intrinsic output domain. Flattened policy queries on
    /// this grid retain silent Holds even when they owned no input-grid sample.
    pub fn output_signal(&self) -> AudioSignal<'plan> {
        AudioSignal {
            plan: self.plan,
            root: self.node,
            support: ExactRatio::ZERO..ExactRatio::integer(self.descriptor.duration.frames()),
            sampling_support: None,
            constrain_support: false,
            repeats: self.descriptor.instance.repeats.clone(),
            definition: self.descriptor.definition.clone(),
            placed_transform: None,
            bypass_binding: self
                .descriptor
                .bypass_binding
                .then_some(BindingTarget::Node(self.node)),
            gap: None,
            provider: SignalProvider::Structural,
        }
    }

    /// A signal over a checked child-local window, retaining this stage's
    /// definition and concrete enclosing Repeat scope. This is used to route
    /// current descendants into an intrinsic Preserve projection.
    pub fn child_signal(
        &self,
        support: Range<ExactRatio>,
    ) -> Result<AudioSignal<'plan>, PlanError> {
        if !positive_range(&support)?
            || support.start.compare_integer(0).is_lt()
            || support
                .end
                .checked_sub(self.plan.nodes[self.child].extent)?
                .compare_integer(0)
                .is_gt()
        {
            return Err(PlanError::InvalidPlan(
                "Preserve child signal is outside its child duration",
            ));
        }
        Ok(AudioSignal {
            plan: self.plan,
            root: self.child,
            support: support.clone(),
            sampling_support: Some(support),
            constrain_support: true,
            repeats: self.descriptor.instance.repeats.clone(),
            definition: self.descriptor.definition.clone(),
            placed_transform: None,
            bypass_binding: None,
            gap: None,
            provider: SignalProvider::Structural,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioSignalContent<'plan> {
    Leaf(AudioContent),
    Stage(AudioStage<'plan>),
    Bound(Box<super::AudioBound<'plan>>),
    ProjectedStage(Arc<crate::AudioStageProjection<'plan>>),
}

impl Serialize for AudioSignalContent<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("AudioSignalContent", 2)?;
        match self {
            Self::Leaf(content) => {
                state.serialize_field("type", "leaf")?;
                state.serialize_field("value", content)?;
            }
            Self::Stage(stage) => {
                state.serialize_field("type", "stage")?;
                state.serialize_field("value", stage)?;
            }
            Self::Bound(bound) => {
                state.serialize_field("type", "bound")?;
                state.serialize_field("value", bound)?;
            }
            Self::ProjectedStage(projection) => {
                state.serialize_field("type", "projected_stage")?;
                state.serialize_field("value", projection.as_ref())?;
            }
        }
        state.end()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioSignalSpan<'plan> {
    /// Scope of every relative instance in this span, including nested stages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<AudioDefinitionSelector>,
    pub samples: Range<SignalSample>,
    pub allocated_samples: Range<SignalSample>,
    /// Exact structural extent in this signal's root frame clock, before grid
    /// allocation. Source filter support is retained separately in the content.
    pub signal_extent: Range<ExactRatio>,
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    pub transform: SignalTransform,
    pub grid: AudioSampleGrid<SignalSample>,
    pub sampling: AudioSampleMap<SignalSample>,
    /// Traversed transparent stages, outer to inner; an opaque Stage carries
    /// its own processing policy in its descriptor instead of this list.
    pub retimes: Vec<AudioRetimeStage>,
    pub content: AudioSignalContent<'plan>,
}

impl AudioSignalSpan<'_> {
    pub fn source_point(&self, sample: SignalSample) -> Result<SourcePoint, PlanError> {
        source_point(&self.content, self.sampling.local_at(sample)?)
    }

    pub fn source_point_at_signal_frame(
        &self,
        frame: ExactRatio,
    ) -> Result<SourcePoint, PlanError> {
        source_point(&self.content, self.transform.local_at_signal_frame(frame)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioSignalQuery<'plan> {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<AudioDefinitionSelector>,
    pub samples: Range<SignalSample>,
    pub spans: Vec<AudioSignalSpan<'plan>>,
    pub lookup: LookupStats,
    #[serde(skip)]
    pub work: usize,
}

/// Root output allocation with stateful processing boundaries retained. Its
/// sample coordinates and allocation match `RenderPlan::audio`, not a virtual
/// point grid. A returned stage may have support beyond this ancestor crop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioProcessingSpan<'plan> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<AudioDefinitionSelector>,
    pub samples: Range<AudioSample>,
    pub allocated_samples: Range<AudioSample>,
    pub project_extent: Range<ExactRatio>,
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    pub transform: AudioTransform,
    pub grid: AudioSampleGrid<AudioSample>,
    pub sampling: AudioSampleMap<AudioSample>,
    pub retimes: Vec<AudioRetimeStage>,
    pub content: AudioSignalContent<'plan>,
}

impl AudioProcessingSpan<'_> {
    pub fn source_point(&self, sample: AudioSample) -> Result<SourcePoint, PlanError> {
        source_point(&self.content, self.sampling.local_at(sample)?)
    }

    pub fn source_point_at_project_frame(
        &self,
        frame: ExactRatio,
    ) -> Result<SourcePoint, PlanError> {
        source_point(
            &self.content,
            frame
                .checked_sub(self.transform.project_origin)?
                .checked_div(self.transform.project_frames_per_local_frame)?,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioProcessingQuery<'plan> {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub samples: Range<AudioSample>,
    pub spans: Vec<AudioProcessingSpan<'plan>>,
    pub lookup: LookupStats,
    #[serde(skip)]
    pub work: usize,
}

#[derive(Debug, Clone)]
pub struct AudioSignal<'plan> {
    plan: &'plan RenderPlan,
    root: usize,
    /// Physical allocation for this signal. A tape run keeps this mapped to
    /// its provider, even though its sample labels share the tape grid.
    support: Range<ExactRatio>,
    // A tape's meaningful intrinsic input selection is distinct from each
    // provider's allocation and limits filter support without changing ownership.
    sampling_support: Option<Range<ExactRatio>>,
    // An authored stage input selection trims filter support. Root allocation
    // itself does not: a root Partition retains its complete child's support.
    constrain_support: bool,
    repeats: Vec<RepeatInstance>,
    definition: Option<AudioDefinitionSelector>,
    // An explicitly placed owned definition carries a distinct point-grid
    // origin. Ordinary signals derive their grid from their selected support.
    placed_transform: Option<SignalTransform>,
    bypass_binding: Option<BindingTarget>,
    gap: Option<DomainGap>,
    provider: SignalProvider<'plan>,
}

#[cfg(test)]
impl AudioSignal<'_> {
    /// Exhaustive private fields of this constructor's Structural signal only.
    /// An unexpected provider is rejected instead of silently omitting a graph.
    pub(in crate::plan) fn probe_baseline_snapshot(
        &self,
        expected_plan: &RenderPlan,
    ) -> serde_json::Value {
        assert!(self.belongs_to(expected_plan));
        let Self {
            plan: _,
            root,
            support,
            sampling_support,
            constrain_support,
            repeats,
            definition,
            placed_transform,
            bypass_binding,
            gap,
            provider,
        } = self;
        assert!(
            matches!(provider, SignalProvider::Structural),
            "actual raw PointCeil constructor must retain Structural provider"
        );
        let bypass = bypass_binding.map(|target| match target {
            BindingTarget::Node(index) => serde_json::json!({
                "tag":"Node","node":expected_plan.nodes[index].inspection.id.as_str(),
            }),
            BindingTarget::Gap(index) => serde_json::json!({
                "tag":"Gap","node":expected_plan.nodes[index].inspection.id.as_str(),
            }),
        });
        serde_json::json!({
            "belongs_to_expected_plan":true,
            "root":expected_plan.nodes[*root].inspection.id.as_str(),
            "support":support,
            "sampling_support":sampling_support,
            "constrain_support":constrain_support,
            "repeats":repeats,
            "definition":definition,
            "placed_transform":placed_transform,
            "bypass_binding":bypass,
            "gap":gap,
            "provider":"Structural",
        })
    }
}

impl crate::audio_allocated::OwnedHeap for AudioStage<'_> {
    fn visit_heap(&self, budget: &mut crate::audio_allocated::HeapBudget) -> Result<(), PlanError> {
        let Self {
            plan: _,
            child: _,
            node: _,
            descriptor,
        } = self;
        budget.visit(descriptor)
    }
}

#[cfg(test)]
impl AudioSignal<'_> {
    pub(in crate::plan) fn probe_structural_repeat_capacity(&self) -> usize {
        assert!(matches!(&self.provider, SignalProvider::Structural));
        self.repeats.capacity()
    }

    pub(in crate::plan) fn probe_structural_definition_present(&self) -> bool {
        assert!(matches!(&self.provider, SignalProvider::Structural));
        self.definition.is_some()
    }

    pub(in crate::plan) fn probe_structural_selector(&self) -> Option<&AudioDefinitionSelector> {
        assert!(matches!(&self.provider, SignalProvider::Structural));
        self.definition.as_ref()
    }
}

impl crate::audio_allocated::OwnedHeap for AudioSignal<'_> {
    fn visit_heap(&self, budget: &mut crate::audio_allocated::HeapBudget) -> Result<(), PlanError> {
        let Self {
            plan: _,
            root: _,
            support: _,
            sampling_support: _,
            constrain_support: _,
            repeats,
            definition,
            placed_transform: _,
            bypass_binding: _,
            gap,
            provider,
        } = self;
        budget.visit(repeats)?;
        budget.visit(definition)?;
        budget.visit(gap)?;
        match provider {
            SignalProvider::Structural
            | SignalProvider::RequiresOwnedInput
            | SignalProvider::Silence => Ok(()),
            SignalProvider::Projected(projection) => budget.visit(projection),
            SignalProvider::Source {
                recipe,
                apply_holds: _,
            } => budget.visit(recipe),
        }
    }
}

impl RenderPlan {
    /// Virtual root signal support, including samples that own no final root
    /// allocation. Its point-grid count must never set an exported duration.
    pub fn audio_signal(&self) -> AudioSignal<'_> {
        AudioSignal {
            plan: self,
            root: self.root,
            support: ExactRatio::ZERO..ExactRatio::integer(self.duration().frames()),
            sampling_support: None,
            constrain_support: false,
            repeats: Vec::new(),
            definition: None,
            placed_transform: None,
            bypass_binding: None,
            gap: None,
            provider: SignalProvider::Structural,
        }
    }

    pub fn audio_processing(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioProcessingQuery<'_>, PlanError> {
        self.audio_processing_admitted(samples, limits, &mut Unmetered)
            .map(crate::AdmittedAudioProcessingQuery::unmetered)
    }
}

impl<'plan> AudioSignal<'plan> {
    pub(super) fn for_definition(definition: &AudioDefinition<'plan>) -> Self {
        Self {
            plan: definition.plan,
            root: definition.root,
            support: ExactRatio::ZERO..definition.exact_duration(),
            sampling_support: None,
            constrain_support: false,
            repeats: Vec::new(),
            definition: Some(definition.selector.clone()),
            placed_transform: None,
            bypass_binding: None,
            gap: definition.gap.clone(),
            provider: SignalProvider::Structural,
        }
    }

    pub(in crate::plan) fn for_placed_definition_admitted<A: crate::AudioSoundContextAdmission>(
        definition: &AudioDefinition<'plan>,
        support: Range<ExactRatio>,
        transform: SignalTransform,
        budget: &mut crate::plan::audio_owners::admission::Budget<'_, A>,
    ) -> crate::plan::audio_owners::admission::Result<
        crate::plan::audio_owners::admission::Owned<Self, A::Issuer, A::Lease>,
        A,
    > {
        use crate::plan::audio_owners::admission::Owned;
        let mut result = Owned {
            value: Self {
                plan: definition.plan,
                root: definition.root,
                support: support.clone(),
                sampling_support: Some(support),
                constrain_support: true,
                repeats: Vec::new(),
                definition: None,
                placed_transform: Some(transform),
                bypass_binding: None,
                gap: None,
                provider: SignalProvider::Structural,
            },
            memory: budget.memory(),
        };
        // Same owning copies, and the same order, as the placed constructor.
        result.value.definition =
            budget.definition_ref(&mut result.memory, Some(&definition.selector))?;
        result.value.gap = budget.gap(&mut result.memory, &definition.gap)?;
        budget.check_memory(&result.memory)?;
        Ok(result)
    }

    /// Relative instance paths in a definition signal are scoped by this
    /// selector. `None` retains the ordinary project-occurrence interpretation.
    pub fn definition(&self) -> Option<&AudioDefinitionSelector> {
        self.definition.as_ref()
    }

    pub(super) fn set_evaluation(
        &mut self,
        definition: Option<AudioDefinitionSelector>,
        repeats: Vec<RepeatInstance>,
        bypass_binding: Option<BindingTarget>,
    ) {
        self.definition = definition;
        self.repeats = repeats;
        self.bypass_binding = bypass_binding;
    }

    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }

    pub(in crate::plan) fn point_signal_clone_admitted<A: crate::AudioSoundContextAdmission>(
        &self,
        budget: &mut QueryBudget<'_, A>,
    ) -> QueryResult<Owned<Self, A::Issuer, A::Lease>, A> {
        if !A::METERED {
            return Ok(Owned {
                value: self.clone(),
                memory: budget.memory(),
            });
        }
        budget.check()?;
        if !matches!(self.provider, SignalProvider::Structural) {
            return Err(PlanError::InvalidPlan(
                "admitted Point signal must use its Structural provider",
            )
            .into());
        }

        let mut memory = budget.memory();
        let mut capacity = budget.memory();
        let repeats = budget.repeats(&mut memory, &mut capacity, &self.repeats)?;
        let definition = budget.definition(&mut memory, &self.definition)?;
        let gap = budget.gap(&mut memory, &self.gap)?;
        let value = Self {
            plan: self.plan,
            root: self.root,
            support: self.support.clone(),
            sampling_support: self.sampling_support.clone(),
            constrain_support: self.constrain_support,
            repeats,
            definition,
            placed_transform: self.placed_transform,
            bypass_binding: self.bypass_binding,
            gap,
            provider: SignalProvider::Structural,
        };
        budget.check_memory(&memory)?;
        budget.check_memory(&capacity)?;
        budget.merge_checked(&mut memory, &mut capacity)?;
        budget.check_memory(&memory)?;
        Ok(Owned { value, memory })
    }

    pub(super) fn has_audio_bindings(&self) -> bool {
        matches!(self.provider, SignalProvider::Structural) && self.plan.has_audio_bindings()
    }

    pub fn support(&self) -> Range<ExactRatio> {
        self.support.clone()
    }

    /// Rebase this current provider into a shared tape clock. Physical allocation
    /// follows the mapped provider support; meaningful filter support is the full
    /// tape selection intersected with any pre-existing authored constraint.
    pub(crate) fn remap_for_tape(
        &self,
        full_support: Range<ExactRatio>,
        destination: Range<ExactRatio>,
        source: Range<ExactRatio>,
        frames_per_sample: ExactRatio,
        grid_origin: ExactRatio,
    ) -> Result<Self, PlanError> {
        if !positive_range(&destination)? || !positive_range(&source)? {
            return Err(PlanError::InvalidPlan(
                "audio tape run ranges must be positive",
            ));
        }
        if source
            .start
            .checked_sub(self.support.start)?
            .compare_integer(0)
            .is_lt()
            || source
                .end
                .checked_sub(self.support.end)?
                .compare_integer(0)
                .is_gt()
        {
            return Err(PlanError::InvalidPlan(
                "audio tape source window is outside provider support",
            ));
        }
        let destination_length = destination.end.checked_sub(destination.start)?;
        let source_length = source.end.checked_sub(source.start)?;
        let source_per_destination = source_length.checked_div(destination_length)?;
        let old = self.transform()?;
        let transform = SignalTransform {
            signal_origin: destination.start.checked_add(
                old.signal_origin
                    .checked_sub(source.start)?
                    .checked_div(source_per_destination)?,
            )?,
            signal_frames_per_local_frame: old
                .signal_frames_per_local_frame
                .checked_div(source_per_destination)?,
            grid_origin,
            signal_frames_per_sample: frames_per_sample,
        };
        let project_source = |source_frame: ExactRatio| {
            destination.start.checked_add(
                source_frame
                    .checked_sub(source.start)?
                    .checked_div(source_per_destination)?,
            )
        };
        let mapped_allocation =
            project_source(self.support.start)?..project_source(self.support.end)?;
        let support = crate::plan::audio::intersect(mapped_allocation, full_support.clone())?;
        let mapped_meaningful = if self.constrain_support {
            let meaningful = self.sampling_support.as_ref().unwrap_or(&self.support);
            let mapped = project_source(meaningful.start)?..project_source(meaningful.end)?;
            crate::plan::audio::intersect(mapped, full_support.clone())?
        } else {
            full_support
        };
        Ok(Self {
            plan: self.plan,
            root: self.root,
            support,
            sampling_support: Some(mapped_meaningful),
            constrain_support: true,
            repeats: self.repeats.clone(),
            definition: self.definition.clone(),
            placed_transform: Some(transform),
            bypass_binding: self.bypass_binding,
            gap: self.gap.clone(),
            provider: self.provider.clone(),
        })
    }

    pub(crate) fn for_projection(
        plan: &'plan RenderPlan,
        projection: Arc<crate::AudioStageProjection<'plan>>,
    ) -> Result<Self, PlanError> {
        if !projection.belongs_to(plan) {
            return Err(PlanError::InvalidPlan(
                "projected Preserve belongs to another plan",
            ));
        }
        Ok(Self {
            plan,
            root: projection.stage().node,
            support: ExactRatio::ZERO..ExactRatio::integer(projection.output_frames()),
            sampling_support: None,
            constrain_support: false,
            repeats: projection.stage().descriptor.instance.repeats.clone(),
            definition: projection.stage().descriptor.definition.clone(),
            placed_transform: None,
            bypass_binding: None,
            gap: None,
            provider: SignalProvider::Projected(projection),
        })
    }

    pub fn sample_count(&self) -> Result<SignalSample, PlanError> {
        Ok(self
            .transform()?
            .grid(AudioBoundaryRule::PointCeil)?
            .boundary(self.support.end)?)
    }

    pub fn query(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioSignalQuery<'plan>, PlanError> {
        self.query_inner(samples, limits, AudioBoundaryRule::PointCeil, true, true)
    }

    pub(crate) fn query_on_grid(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        rule: AudioBoundaryRule,
    ) -> Result<AudioSignalQuery<'plan>, PlanError> {
        self.query_inner(samples, limits, rule, true, true)
    }

    /// Resolve structural policies through all retimes on the same point grid.
    /// This is policy/source inspection, not permission to bypass Preserve DSP.
    pub fn query_flattened(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioSignalQuery<'plan>, PlanError> {
        self.query_inner(samples, limits, AudioBoundaryRule::PointCeil, false, false)
    }

    fn transform(&self) -> Result<SignalTransform, TimeError> {
        if let Some(transform) = self.placed_transform {
            return Ok(transform);
        }
        let rate = self.plan.metadata.presentation_basis.frame_rate;
        Ok(SignalTransform {
            signal_origin: ExactRatio::ZERO,
            signal_frames_per_local_frame: ExactRatio::ONE,
            grid_origin: self.support.start,
            signal_frames_per_sample: ExactRatio::new(
                i128::from(rate.numerator()),
                i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
            )?,
        })
    }

    pub(super) fn query_inner(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        rule: AudioBoundaryRule,
        stop_at_preserve: bool,
        stop_at_bindings: bool,
    ) -> Result<AudioSignalQuery<'plan>, PlanError> {
        if matches!(self.provider, SignalProvider::Structural) {
            return self.structural_query_unmetered(
                samples,
                limits,
                rule,
                stop_at_preserve,
                stop_at_bindings,
            );
        }
        limits.validate()?;
        if matches!(self.provider, SignalProvider::RequiresOwnedInput) {
            return Err(PlanError::InvalidPlan(
                "owned Preserve input requires its compiled projection",
            ));
        }
        let grid = self.transform()?.grid(rule)?;
        let first = grid.boundary(self.support.start)?;
        let count = grid.boundary(self.support.end)?;
        if samples.start < first || samples.end < samples.start || samples.end > count {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        if matches!(self.provider, SignalProvider::Projected(_)) {
            let mut spans = Vec::new();
            if samples.start < samples.end {
                let mut span = self.projected_span(samples.start, grid)?;
                span.samples = samples.start..span.allocated_samples.end.min(samples.end);
                spans.push(span);
            }
            return Ok(AudioSignalQuery {
                project_id: self.plan.metadata.project_id.clone(),
                revision_id: self.plan.metadata.revision_id.clone(),
                definition: self.definition.clone(),
                samples,
                spans,
                lookup: LookupStats::default(),
                work: 1,
            });
        }
        let mut budget = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut spans = Vec::new();
        let mut cursor = samples.start;
        while cursor < samples.end {
            if spans.len() == limits.maximum_spans {
                return Err(PlanError::AudioQueryLimit("span count"));
            }
            let mut span = match &self.provider {
                SignalProvider::Source { recipe, .. } => {
                    budget.spend(1)?;
                    self.source_voice_span(recipe, cursor, grid)?
                }
                SignalProvider::Silence => {
                    budget.spend(1)?;
                    self.occurrence_silence_span(grid)?
                }
                _ => self.span(
                    cursor,
                    grid,
                    stop_at_preserve,
                    stop_at_bindings,
                    &mut budget,
                )?,
            };
            span.samples = cursor..span.allocated_samples.end.min(samples.end);
            cursor = span.samples.end;
            spans.push(span);
        }
        Ok(AudioSignalQuery {
            project_id: self.plan.metadata.project_id.clone(),
            revision_id: self.plan.metadata.revision_id.clone(),
            definition: self.definition.clone(),
            samples,
            spans,
            lookup: budget.lookup,
            work: limits.maximum_work - budget.remaining,
        })
    }

    fn projected_span(
        &self,
        sample: SignalSample,
        grid: AudioSampleGrid<SignalSample>,
    ) -> Result<AudioSignalSpan<'plan>, PlanError> {
        let SignalProvider::Projected(projection) = &self.provider else {
            return Err(PlanError::InvalidPlan("projected signal has no stage"));
        };
        let transform = self.transform()?;
        let allocated_samples =
            grid.boundary(self.support.start)?..grid.boundary(self.support.end)?;
        if !allocated_samples.contains(&sample) {
            return Err(PlanError::InvalidPlan(
                "projected signal interval did not advance",
            ));
        }
        let sampling = AudioSampleMap::new(
            allocated_samples.start,
            transform.local_at(allocated_samples.start)?,
            transform
                .signal_frames_per_sample
                .checked_div(transform.signal_frames_per_local_frame)?,
        )?;
        Ok(AudioSignalSpan {
            definition: self.definition.clone(),
            samples: allocated_samples.clone(),
            allocated_samples,
            signal_extent: self.support.clone(),
            instance: projection.stage().descriptor().instance.clone(),
            gap_after: None,
            transform,
            grid,
            sampling,
            retimes: Vec::new(),
            content: AudioSignalContent::ProjectedStage(Arc::clone(projection)),
        })
    }

    fn span(
        &self,
        sample: SignalSample,
        grid: AudioSampleGrid<SignalSample>,
        stop_at_preserve: bool,
        stop_at_bindings: bool,
        budget: &mut Budget,
    ) -> Result<AudioSignalSpan<'plan>, PlanError> {
        let mut admission = Unmetered;
        outer::<_, Unmetered>(self.span_admitted(
            sample,
            grid,
            stop_at_preserve,
            stop_at_bindings,
            &mut QueryBudget::new(budget, &mut admission),
        ))
        .map(super::audio_bound::query::GuardedBound::unmetered)
    }
}

fn positive_range(range: &Range<ExactRatio>) -> Result<bool, TimeError> {
    Ok(range
        .end
        .checked_sub(range.start)?
        .compare_integer(0)
        .is_gt())
}

fn source_point(
    content: &AudioSignalContent<'_>,
    local: ExactRatio,
) -> Result<SourcePoint, PlanError> {
    let AudioSignalContent::Leaf(AudioContent::Source {
        source,
        start,
        duration,
        ..
    }) = content
    else {
        return Err(PlanError::NoSourceAudio);
    };
    Ok(source_point_from_local(source, *start, *duration, local)?)
}
