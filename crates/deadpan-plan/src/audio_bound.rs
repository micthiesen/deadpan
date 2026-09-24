//! A current owned recipe evaluated on an explicitly retained sampling lattice.

use deadpan_core::{
    AudioBindingEnvironment, AudioBindingGridRule, ExactRatio, InstancePath, NodeId,
    RepeatInstance, ResolvedAudioBinding,
};
use serde::Serialize;

use super::audio::{Budget, EnvelopeConstraint, intersect};
use super::{
    AudioDefinition, AudioDefinitionSelector, AudioDomain, AudioPointDomain, AudioRootPlacement,
    CompiledKind, RenderPlan, SignalSample, SignalTransform,
};
use crate::{AudioSampleGrid, PlanError};

#[derive(Debug, Clone)]
pub enum AudioBoundDomain<'plan> {
    Root(AudioDomain<'plan>),
    Point(AudioPointDomain<'plan>),
    /// Empty meaningful support, distinct from a nonempty Point domain with
    /// zero allocated points. Neither invents a SilentHold policy.
    Empty,
}

#[derive(Clone, Copy)]
pub(super) struct BoundPlacement<'a> {
    pub transform: SignalTransform,
    pub grid: AudioSampleGrid<SignalSample>,
    pub allocated_start: SignalSample,
    pub support: Option<&'a std::ops::Range<ExactRatio>>,
    pub constraints: &'a [EnvelopeConstraint],
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
    resolved: ResolvedAudioBinding,
    reference_at_anchor: ExactRatio,
    reference_step: ExactRatio,
    support: std::ops::Range<ExactRatio>,
    envelope_scale: ExactRatio,
    #[serde(skip)]
    constraints: Vec<EnvelopeConstraint>,
}

impl PartialEq for AudioBound<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.plan, other.plan)
            && self.node == other.node
            && self.definition == other.definition
            && self.instance == other.instance
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

    fn definition_exclusions(
        &self,
        selector: &AudioDefinitionSelector,
        budget: &mut Budget,
    ) -> Result<(NodeId, Vec<NodeId>), PlanError> {
        let definition = self.audio_definition(selector.clone())?;
        let mut current = self.parents[definition.root];
        let mut excluded = Vec::new();
        while let Some(index) = current {
            budget.spend(1)?;
            if matches!(self.nodes[index].kind, CompiledKind::Repeat { .. }) {
                excluded.push(self.nodes[index].inspection.id.clone());
            }
            current = self.parents[index];
        }
        excluded.reverse();
        Ok((definition.root().clone(), excluded))
    }

    pub(super) fn bound_at(
        &self,
        node: usize,
        repeats: &[RepeatInstance],
        definition: Option<&AudioDefinitionSelector>,
        placement: BoundPlacement<'_>,
        budget: &mut Budget,
    ) -> Result<Option<AudioBound<'_>>, PlanError> {
        let BoundPlacement {
            transform,
            grid,
            allocated_start,
            support,
            constraints,
        } = placement;
        let id = &self.nodes[node].inspection.id;
        if !self.audio_bindings.bindings().contains_key(id) {
            return Ok(None);
        }
        budget.spend(repeats.len() + 1)?;
        let instance = InstancePath {
            node: id.clone(),
            repeats: repeats.to_vec(),
        };
        let excluded = definition
            .map(|selector| self.definition_exclusions(selector, budget))
            .transpose()?;
        let environment = match &excluded {
            Some((root, outside_repeats)) => AudioBindingEnvironment::Definition {
                root,
                instance: &instance,
                outside_repeats,
            },
            None => AudioBindingEnvironment::Occurrence(&instance),
        };
        if budget.remaining == 0 {
            return Err(PlanError::AudioQueryLimit("binding resolution"));
        }
        let resolved = self
            .audio_bindings
            .resolve_in(id, environment, budget.remaining)?;
        budget.spend(resolved.work)?;
        let lattice = &resolved.lattice;
        let unit = lattice.local_frames_per_sample()?;
        let local_anchor = resolved
            .resume
            .as_ref()
            .map_or(lattice.local_support.start, |resume| resume.local_boundary);
        let phase = resolved
            .resume
            .as_ref()
            .map_or(ExactRatio::ZERO, |resume| resume.reference_local_delta);
        let reference = ExactRatio::integer(lattice.sample_boundary(lattice.local_support.start)?)
            .checked_add(phase.checked_div(unit)?)?;
        let current_boundary = grid.boundary(transform.signal_from_local(local_anchor)?)?;
        let reference_step = transform
            .signal_frames_per_sample
            .checked_div(transform.signal_frames_per_local_frame)?
            .checked_div(unit)?;
        let offset = ExactRatio::new(
            i128::from(allocated_start.0) - i128::from(current_boundary.0),
            1,
        )?;
        let local = |point| transform.local_at_signal_frame(point);
        // The retained layout fixes the sampling clock, including its original
        // reference anchor above. It does not freeze the current raw recipe's
        // duration. For example, lengthening a moved RoomTone Hold must expose
        // its new tail without resetting the loop phase or other Repeat plays'
        // historical origins. The raw Source walker separately applies current
        // source placement; current meaningful ancestors constrain this extent.
        let intrinsic =
            ExactRatio::ZERO..ExactRatio::integer(self.nodes[node].inspection.duration.frames());
        let support = match support {
            Some(current) => intersect(intrinsic, local(current.start)?..local(current.end)?)?,
            None => intrinsic,
        };
        budget.spend(constraints.len())?;
        let project = |point| {
            lattice
                .origin
                .checked_add(local(point)?.checked_mul(lattice.frames_per_local_frame)?)
        };
        let constraints = constraints
            .iter()
            .map(|constraint| {
                Ok(EnvelopeConstraint {
                    range: project(constraint.range.start)?..project(constraint.range.end)?,
                    ..constraint.clone()
                })
            })
            .collect::<Result<Vec<_>, PlanError>>()?;
        let envelope_scale = transform
            .signal_frames_per_local_frame
            .checked_div(lattice.frames_per_local_frame)?;
        Ok(Some(AudioBound {
            plan: self,
            node,
            definition: definition.cloned(),
            instance,
            resolved,
            reference_at_anchor: reference.checked_add(offset.checked_mul(reference_step)?)?,
            reference_step,
            support,
            envelope_scale,
            constraints,
        }))
    }
}

impl<'plan> AudioBound<'plan> {
    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }
    pub fn work(&self) -> usize {
        self.resolved.work
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
            selector: AudioDefinitionSelector::Node {
                node: self.instance.node.clone(),
            },
        }
    }

    fn set_domain_evaluation(&self, domain: &mut AudioDomain<'plan>) {
        domain.seed.definition = self.definition.clone();
        domain.seed.bypass_binding = Some(self.node);
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
                    Some(self.node),
                );
                AudioBoundDomain::Point(domain)
            }
        })
    }
}
