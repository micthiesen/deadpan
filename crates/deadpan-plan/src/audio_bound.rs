//! A current owned recipe evaluated on an explicitly retained sampling lattice.

use deadpan_core::{
    AudioBindingEnvironment, AudioBindingGridRule, AudioDefinitionScope, ExactRatio, InstancePath,
    NodeId, RepeatInstance, ResolvedAudioBinding,
};
use serde::Serialize;

use super::audio::{Budget, EnvelopeConstraint, intersect};
use super::audio_domain::DomainGap;
use super::{
    AudioDefinition, AudioDefinitionSelector, AudioDomain, AudioPointDomain, AudioRootPlacement,
    CompiledKind, RenderPlan, SignalSample, SignalTransform,
};
use crate::{AudioSampleGrid, PlanError};

#[path = "audio_bound_query.rs"]
mod query;
use query::QueryBound;

#[derive(Debug, Clone)]
pub enum AudioBoundDomain<'plan> {
    Root(AudioDomain<'plan>),
    Point(AudioPointDomain<'plan>),
    /// Empty meaningful support, distinct from a nonempty Point domain with
    /// zero allocated points. Neither invents a SilentHold policy.
    Empty,
}

/// A Repeat's gap recipe is a different operand from the Repeat's node output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BindingTarget {
    Node(usize),
    Gap(usize),
}

#[derive(Clone, Copy)]
pub(super) struct BoundPlacement<'a> {
    pub transform: SignalTransform,
    pub grid: AudioSampleGrid<SignalSample>,
    pub allocated_start: SignalSample,
    pub support: Option<&'a std::ops::Range<ExactRatio>>,
    pub constraints: &'a [EnvelopeConstraint],
    pub gap: Option<&'a DomainGap>,
}

/// A borrowed processing operand, never a deserializable media admission token.
/// Its map is relative to the containing span's full allocated sample start.
#[derive(Debug, Clone, Serialize)]
pub struct AudioBound<'plan> {
    #[serde(skip)]
    plan: &'plan RenderPlan,
    #[serde(skip)]
    node: usize,
    definition: Option<AudioDefinitionSelector>,
    instance: InstancePath,
    #[serde(skip_serializing_if = "Option::is_none")]
    gap: Option<DomainGap>,
    resolved: ResolvedAudioBinding,
    reference_at_anchor: ExactRatio,
    reference_step: ExactRatio,
    support: std::ops::Range<ExactRatio>,
    envelope_scale: ExactRatio,
    #[serde(skip)]
    constraints: Vec<EnvelopeConstraint>,
}

impl crate::audio_allocated::OwnedHeap for AudioBound<'_> {
    fn visit_heap(&self, budget: &mut crate::audio_allocated::HeapBudget) -> Result<(), PlanError> {
        let Self {
            plan: _,
            node: _,
            definition,
            instance,
            gap,
            resolved,
            reference_at_anchor: _,
            reference_step: _,
            support: _,
            envelope_scale: _,
            constraints,
        } = self;
        budget.visit(definition)?;
        budget.visit(instance)?;
        budget.visit(gap)?;
        budget.visit(resolved)?;
        budget.visit(constraints)
    }
}

impl PartialEq for AudioBound<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.plan, other.plan)
            && self.node == other.node
            && self.definition == other.definition
            && self.instance == other.instance
            && self.gap == other.gap
            && self.resolved == other.resolved
            && self.reference_at_anchor == other.reference_at_anchor
            && self.reference_step == other.reference_step
            && self.support == other.support
            && self.envelope_scale == other.envelope_scale
            && self.constraints == other.constraints
    }
}
impl Eq for AudioBound<'_> {}

impl RenderPlan {
    pub fn has_audio_bindings(&self) -> bool {
        !self.audio_bindings.is_empty()
    }

    pub(super) fn bound_at(
        &self,
        node: usize,
        repeats: &[RepeatInstance],
        definition: Option<&AudioDefinitionSelector>,
        placement: BoundPlacement<'_>,
        budget: &mut Budget,
    ) -> Result<Option<AudioBound<'_>>, PlanError> {
        let mut policy = crate::plan::audio_owners::admission::Unmetered;
        let mut query = crate::plan::audio_owners::admission::Budget::new(budget, &mut policy);
        crate::plan::audio_owners::admission::outer::<
            _,
            crate::plan::audio_owners::admission::Unmetered,
        >(self.bound_at_query(node, repeats, definition, placement, &mut query))
        .map(|value| value.map(QueryBound::unmetered))
    }
}

impl<'plan> AudioBound<'plan> {
    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }
    pub fn work(&self) -> usize {
        self.resolved.work
    }

    /// A directly authored silent Hold has no signal to transfer, regardless
    /// of the retained physical clock. This does not classify sources whose
    /// samples happen to be zero, or a composite containing silent children.
    pub fn is_silent_hold(&self) -> bool {
        self.gap.is_none()
            && matches!(
                &self.plan.nodes[self.node].kind,
                CompiledKind::Hold {
                    audio: deadpan_core::HoldAudio::Silence,
                    ..
                }
            )
    }

    /// The full current Preserve operand behind this physical binding, if any.
    /// Its own binding is bypassed, exactly as in `raw_domain`; descendants
    /// retain their bindings and scoped identities. This admits policy
    /// inspection of hidden canonical history without scanning a Source's PCM.
    pub fn intrinsic_stage(&self) -> Result<Option<super::AudioStage<'plan>>, PlanError> {
        if self.gap.is_none()
            && matches!(&self.plan.nodes[self.node].kind,
                CompiledKind::Retime { scale, pitch, .. }
                if pitch.processes(*scale == ExactRatio::ONE))
        {
            return super::AudioStage::for_node(
                self.plan,
                self.node,
                &self.instance.repeats,
                self.definition.as_ref(),
                Some(self.binding_target()),
            )
            .map(Some);
        }
        Ok(None)
    }
    pub fn reference_at_offset(&self, offset: i64) -> Result<ExactRatio, PlanError> {
        self.reference_at_wide_offset(i128::from(offset))
    }

    /// Preserve signed physical-domain distances wider than an i64 label.
    pub fn reference_at_wide_offset(&self, offset: i128) -> Result<ExactRatio, PlanError> {
        Ok(self
            .reference_at_anchor
            .checked_add(ExactRatio::new(offset, 1)?.checked_mul(self.reference_step)?)?)
    }
    pub fn reference_samples_per_output_sample(&self) -> ExactRatio {
        self.reference_step
    }

    pub(super) fn envelope_scale(&self) -> ExactRatio {
        self.envelope_scale
    }

    pub(super) fn lattice_origin(&self) -> ExactRatio {
        self.resolved.lattice.origin
    }

    /// Flatten the current owned output on the retained grid. This is an
    /// envelope inspection domain; a PointCeil grid remains PointCeil here.
    pub(super) fn envelope_domain(
        &self,
        budget: &mut Budget,
    ) -> Result<Option<AudioDomain<'plan>>, PlanError> {
        budget.spend(self.constraints.len() + self.instance.repeats.len() + 1)?;
        if !self
            .support
            .end
            .checked_sub(self.support.start)?
            .compare_integer(0)
            .is_gt()
        {
            return Ok(None);
        }
        let lattice = &self.resolved.lattice;
        let definition = self.definition_handle();
        let placement = AudioRootPlacement::new(
            lattice.origin,
            lattice.frames_per_local_frame,
            self.support.clone(),
        )?;
        let mut domain = definition.root_domain_on_grid(
            placement,
            lattice.grid_origin,
            match lattice.grid_rule {
                AudioBindingGridRule::RootRoundEven => crate::AudioBoundaryRule::RoundEven,
                AudioBindingGridRule::PointCeil => crate::AudioBoundaryRule::PointCeil,
            },
        )?;
        self.set_domain_evaluation(&mut domain);
        Ok(Some(domain))
    }

    fn definition_handle(&self) -> AudioDefinition<'plan> {
        AudioDefinition {
            plan: self.plan,
            root: self.node,
            gap: self.gap.clone(),
            selector: if self.gap.is_some() {
                AudioDefinitionSelector::RepeatGap {
                    repeat: self.instance.node.clone(),
                }
            } else {
                AudioDefinitionSelector::Node {
                    node: self.instance.node.clone(),
                }
            },
        }
    }

    fn binding_target(&self) -> BindingTarget {
        if self.gap.is_some() {
            BindingTarget::Gap(self.node)
        } else {
            BindingTarget::Node(self.node)
        }
    }

    fn set_domain_evaluation(&self, domain: &mut AudioDomain<'plan>) {
        domain.seed.definition = self.definition.clone();
        domain.seed.bypass_binding = Some(self.binding_target());
        domain.seed.repeats = self.instance.repeats.clone();
        domain
            .seed
            .constraints
            .extend(self.constraints.iter().cloned());
        domain.instance = self.instance.clone();
    }

    pub fn raw_domain(&self) -> Result<AudioBoundDomain<'plan>, PlanError> {
        let lattice = &self.resolved.lattice;
        let support = self.support.clone();
        if !support
            .end
            .checked_sub(support.start)?
            .compare_integer(0)
            .is_gt()
        {
            return Ok(AudioBoundDomain::Empty);
        }
        let definition = self.definition_handle();
        let placement =
            AudioRootPlacement::new(lattice.origin, lattice.frames_per_local_frame, support)?;
        Ok(match lattice.grid_rule {
            AudioBindingGridRule::RootRoundEven => {
                let mut domain = definition.in_root_clock(placement)?;
                self.set_domain_evaluation(&mut domain);
                AudioBoundDomain::Root(domain)
            }
            AudioBindingGridRule::PointCeil => {
                let mut domain = definition.in_point_clock(placement, lattice.grid_origin)?;
                domain.set_evaluation(
                    self.definition.clone(),
                    self.instance.repeats.clone(),
                    Some(self.binding_target()),
                );
                AudioBoundDomain::Point(domain)
            }
        })
    }
}
