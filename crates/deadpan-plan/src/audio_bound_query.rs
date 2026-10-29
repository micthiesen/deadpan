//! Shared actual binding/envelope constructors for legacy and admitted queries.
use super::*;
use crate::AudioSoundContextAdmission;
use crate::audio_allocated::admission::array;
use crate::plan::audio_owners::admission::{
    Budget as QueryBudget, CoreAdmission, Memory, Owned, Result, core,
};
use deadpan_core::{AdmittedAudioBinding, AudioBindingValue};

type BoundResult<'plan, A> = Result<
    Option<
        QueryBound<
            'plan,
            <A as AudioSoundContextAdmission>::Issuer,
            <A as AudioSoundContextAdmission>::Lease,
        >,
    >,
    A,
>;
type EnvelopeResult<'plan, A> = Result<
    Option<
        Owned<
            AudioDomain<'plan>,
            <A as AudioSoundContextAdmission>::Issuer,
            <A as AudioSoundContextAdmission>::Lease,
        >,
    >,
    A,
>;

enum Resolved<I, L> {
    Legacy(ResolvedAudioBinding),
    Metered(AdmittedAudioBinding<I, L>),
}
impl<I: Copy + Eq, L> Resolved<I, L> {
    fn value(&self) -> &ResolvedAudioBinding {
        match self {
            Self::Legacy(v) => v,
            Self::Metered(v) => v.value(),
        }
    }
}
enum BoundValue<'plan, I, L> {
    Legacy(AudioBound<'plan>),
    Metered(AudioBindingValue<AudioBound<'plan>, I, L>),
}
pub(in crate::plan) struct QueryBound<'plan, I, L> {
    value: BoundValue<'plan, I, L>,
    _memory: Memory<I, L>,
}
impl<'plan, I: Copy + Eq, L> std::ops::Deref for QueryBound<'plan, I, L> {
    type Target = AudioBound<'plan>;
    fn deref(&self) -> &Self::Target {
        match &self.value {
            BoundValue::Legacy(v) => v,
            BoundValue::Metered(v) => v.value(),
        }
    }
}
impl<'plan> QueryBound<'plan, (), ()> {
    pub(in crate::plan) fn unmetered(self) -> AudioBound<'plan> {
        match self.value {
            BoundValue::Legacy(v) => v,
            BoundValue::Metered(_) => unreachable!("legacy binding backend"),
        }
    }
}
impl RenderPlan {
    fn definition_exclusions_query<A: AudioSoundContextAdmission>(
        &self,
        selector: &AudioDefinitionSelector,
        budget: &mut QueryBudget<'_, A>,
        memory: &mut Memory<A::Issuer, A::Lease>,
    ) -> Result<(NodeId, Vec<NodeId>), A> {
        let mut temporary = budget.memory();
        let copied = budget
            .definition_ref(&mut temporary, Some(selector))?
            .expect("selector");
        let definition = self.audio_definition(copied)?;
        let mut current = self.parents[definition.root];
        let mut capacity = budget.memory();
        let mut excluded = Vec::new();
        while let Some(index) = current {
            budget.canonical(1)?;
            if matches!(self.nodes[index].kind, CompiledKind::Repeat { .. }) {
                let needed = excluded.len() + 1;
                budget.grow(&mut excluded, &mut capacity, needed)?;
                excluded.push(budget.node(memory, &self.nodes[index].inspection.id)?);
            }
            current = self.parents[index];
        }
        excluded.reverse();
        let root = budget.node(memory, definition.root())?;
        budget.check_memory(memory)?;
        budget.check_memory(&capacity)?;
        budget.merge_checked(memory, &mut capacity);
        Ok((root, excluded))
    }
    pub(in crate::plan) fn bound_at_query<A: AudioSoundContextAdmission>(
        &self,
        node: usize,
        repeats: &[RepeatInstance],
        definition: Option<&AudioDefinitionSelector>,
        placement: BoundPlacement<'_>,
        budget: &mut QueryBudget<'_, A>,
    ) -> BoundResult<'_, A> {
        let BoundPlacement {
            transform,
            grid,
            allocated_start,
            support,
            constraints,
            gap,
        } = placement;
        // Preserve the retained binding while dormant, but do not ask its empty
        // meaningful domain for an audible sample or an owner phase.
        if gap.is_none()
            && matches!(&self.nodes[node].kind, CompiledKind::Source { audio: Some(audio), .. } if audio.selection.start == audio.selection.end)
        {
            return Ok(None);
        }
        let id = &self.nodes[node].inspection.id;
        let bindings = if gap.is_some() {
            self.audio_bindings.gap_bindings()
        } else {
            self.audio_bindings.bindings()
        };
        if !bindings.contains_key(id) {
            return Ok(None);
        }
        budget.canonical(repeats.len() + 1)?;
        let mut memory = budget.memory();
        let mut instance_capacity = budget.memory();
        let instance = InstancePath {
            node: budget.node(&mut memory, id)?,
            repeats: budget.repeats(&mut memory, &mut instance_capacity, repeats)?,
        };
        budget.check_memory(&memory)?;
        budget.check_memory(&instance_capacity)?;
        budget.merge_checked(&mut memory, &mut instance_capacity);
        let mut excluded_memory = budget.memory();
        let excluded = definition
            .map(|selector| {
                self.definition_exclusions_query(selector, budget, &mut excluded_memory)
            })
            .transpose()?;
        let environment = match (gap, &excluded) {
            (Some(gap), Some((root, outside_repeats))) => AudioBindingEnvironment::GapDefinition {
                root: if matches!(definition, Some(AudioDefinitionSelector::RepeatGap { .. })) {
                    AudioDefinitionScope::RepeatGap(root)
                } else {
                    AudioDefinitionScope::NodeOutput(root)
                },
                instance: &instance,
                outside_repeats,
                after: gap.after.as_ref(),
            },
            (Some(gap), None) => AudioBindingEnvironment::GapOccurrence {
                instance: &instance,
                after: gap.after.as_ref().ok_or(PlanError::InvalidPlan(
                    "bound gap occurrence has no preceding play",
                ))?,
            },
            (None, Some((root, outside_repeats))) => AudioBindingEnvironment::Definition {
                root,
                instance: &instance,
                outside_repeats,
            },
            (None, None) => AudioBindingEnvironment::Occurrence(&instance),
        };
        if budget.remaining == 0 {
            return Err(PlanError::AudioQueryLimit("binding resolution").into());
        }
        let mut resolved = if A::METERED {
            let remaining = budget.remaining;
            let mut adapter = CoreAdmission {
                outer: budget.admission,
            };
            Resolved::Metered(core::<_, A>(if gap.is_some() {
                self.audio_bindings.resolve_gap_in_admitted(
                    id,
                    environment,
                    remaining,
                    &mut adapter,
                )
            } else {
                self.audio_bindings
                    .resolve_in_admitted(id, environment, remaining, &mut adapter)
            })?)
        } else {
            Resolved::Legacy(if gap.is_some() {
                self.audio_bindings
                    .resolve_gap_in(id, environment, budget.remaining)?
            } else {
                self.audio_bindings
                    .resolve_in(id, environment, budget.remaining)?
            })
        };
        match &mut resolved {
            Resolved::Legacy(value) => budget.canonical(value.work)?,
            Resolved::Metered(value) => {
                let work = {
                    let adapter = CoreAdmission {
                        outer: budget.admission,
                    };
                    core::<_, A>(value.take_work(&adapter))?
                };
                budget.account_core(work)?;
            }
        }
        let resolved_value = resolved.value();
        let lattice = &resolved_value.lattice;
        let unit = lattice.local_frames_per_sample()?;
        let local_anchor = resolved_value
            .resume
            .as_ref()
            .map_or(lattice.local_support.start, |resume| resume.local_boundary);
        let phase = resolved_value
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
        let duration = gap.map_or(self.nodes[node].extent, |gap| {
            ExactRatio::integer(gap.duration.frames())
        });
        let intrinsic = ExactRatio::ZERO..duration;
        let mut support = match support {
            Some(current) => intersect(intrinsic, local(current.start)?..local(current.end)?)?,
            None => intrinsic,
        };
        // A selected Source owns a narrower physical operand than its full
        // measured mapping. Clip before transferring it onto the resumed grid,
        // so interpolation cannot draw across the authored crop. Ordinary
        // placement still denotes missing input, not an endpoint mask.
        if let CompiledKind::Source {
            audio: Some(audio), ..
        } = &self.nodes[node].kind
            && audio.selected
        {
            support = intersect(support, audio.selection.start..audio.selection.end)?;
        }
        budget.canonical(constraints.len())?;
        let project = |point| {
            lattice
                .origin
                .checked_add(local(point)?.checked_mul(lattice.frames_per_local_frame)?)
        };
        let mut constraint_capacity = budget.memory();
        if A::METERED {
            budget.reserve(
                &mut constraint_capacity,
                array::<EnvelopeConstraint>(constraints.len())?,
            )?;
        }
        let mut copied_constraints = Vec::with_capacity(constraints.len());
        if A::METERED {
            assert_eq!(copied_constraints.capacity(), constraints.len());
        }
        for constraint in constraints {
            let range = project(constraint.range.start)?..project(constraint.range.end)?;
            let mut value = budget.constraint(&mut memory, constraint)?;
            value.range = range;
            copied_constraints.push(value);
        }
        budget.check_memory(&memory)?;
        budget.check_memory(&constraint_capacity)?;
        budget.merge_checked(&mut memory, &mut constraint_capacity);
        let constraints = copied_constraints;
        let envelope_scale = transform
            .signal_frames_per_local_frame
            .checked_div(lattice.frames_per_local_frame)?;
        let definition = budget.definition_ref(&mut memory, definition)?;
        let gap = budget.gap_ref(&mut memory, gap)?;
        let reference_at_anchor = reference.checked_add(offset.checked_mul(reference_step)?)?;
        budget.check_memory(&memory)?;
        let make = |resolved| AudioBound {
            plan: self,
            node,
            definition,
            instance,
            gap,
            resolved,
            reference_at_anchor,
            reference_step,
            support,
            envelope_scale,
            constraints,
        };
        let value = match resolved {
            Resolved::Legacy(value) => BoundValue::Legacy(make(value)),
            Resolved::Metered(value) => {
                let adapter = CoreAdmission {
                    outer: budget.admission,
                };
                BoundValue::Metered(core::<_, A>(value.map_value(&adapter, make))?)
            }
        };
        Ok(Some(QueryBound {
            value,
            _memory: memory,
        }))
    }
}
impl<'plan> AudioBound<'plan> {
    pub(in crate::plan) fn envelope_domain_query<A: AudioSoundContextAdmission>(
        &self,
        budget: &mut QueryBudget<'_, A>,
    ) -> EnvelopeResult<'plan, A> {
        budget.canonical(self.constraints.len() + self.instance.repeats.len() + 1)?;
        if !self
            .support
            .end
            .checked_sub(self.support.start)?
            .compare_integer(0)
            .is_gt()
        {
            return Ok(None);
        }
        let mut temporary = budget.memory();
        let gap = budget.gap(&mut temporary, &self.gap)?;
        let selector = if self.gap.is_some() {
            AudioDefinitionSelector::RepeatGap {
                repeat: budget.node(&mut temporary, &self.instance.node)?,
            }
        } else {
            AudioDefinitionSelector::Node {
                node: budget.node(&mut temporary, &self.instance.node)?,
            }
        };
        let definition = AudioDefinition {
            plan: self.plan,
            root: self.node,
            gap,
            selector,
        };
        let lattice = &self.resolved.lattice;
        let placement = AudioRootPlacement::new(
            lattice.origin,
            lattice.frames_per_local_frame,
            self.support.clone(),
        )?;
        let mut domain = definition.root_domain_on_grid_query(
            placement,
            lattice.grid_origin,
            match lattice.grid_rule {
                AudioBindingGridRule::RootRoundEven => crate::AudioBoundaryRule::RoundEven,
                AudioBindingGridRule::PointCeil => crate::AudioBoundaryRule::PointCeil,
            },
            budget,
        )?;
        if !A::METERED {
            self.set_domain_evaluation(&mut domain.value);
            return Ok(Some(domain));
        }
        // Construction peak stays charged during this temporary Domain's use.
        domain.value.seed.definition = budget.definition(&mut domain.memory, &self.definition)?;
        domain.value.seed.bypass_binding = Some(self.binding_target());
        let mut capacity = budget.memory();
        domain.value.seed.repeats =
            budget.repeats(&mut domain.memory, &mut capacity, &self.instance.repeats)?;
        budget.check_memory(&domain.memory)?;
        budget.check_memory(&capacity)?;
        budget.merge_checked(&mut domain.memory, &mut capacity);
        if !self.constraints.is_empty() {
            let count = domain.value.seed.constraints.len() + self.constraints.len();
            let mut next_capacity = budget.memory();
            budget.reserve(&mut next_capacity, array::<EnvelopeConstraint>(count)?)?;
            let mut next = Vec::with_capacity(count);
            assert_eq!(next.capacity(), count);
            for value in domain.value.seed.constraints.drain(..) {
                budget.additional(1)?;
                next.push(value);
            }
            for value in &self.constraints {
                next.push(budget.constraint(&mut domain.memory, value)?);
            }
            domain.value.seed.constraints = next;
            budget.check_memory(&domain.memory)?;
            budget.check_memory(&next_capacity)?;
            budget.merge_checked(&mut domain.memory, &mut next_capacity);
        }
        let mut cap = budget.memory();
        domain.value.instance = budget.instance(&mut domain.memory, &mut cap, &self.instance)?;
        budget.check_memory(&domain.memory)?;
        budget.check_memory(&cap)?;
        budget.merge_checked(&mut domain.memory, &mut cap);
        Ok(Some(domain))
    }
}
