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
pub(in crate::plan) enum BoundValue<T, I, L> {
    Legacy(T),
    Metered(AudioBindingValue<T, I, L>),
}
pub(in crate::plan) struct GuardedBound<T, I, L> {
    value: BoundValue<T, I, L>,
    pub(in crate::plan) _memory: Memory<I, L>,
}
pub(in crate::plan) type QueryBound<'plan, I, L> = GuardedBound<AudioBound<'plan>, I, L>;
impl<T, I: Copy + Eq, L> std::ops::Deref for GuardedBound<T, I, L> {
    type Target = T;
    fn deref(&self) -> &T {
        match &self.value {
            BoundValue::Legacy(v) => v,
            BoundValue::Metered(v) => v.value(),
        }
    }
}
impl<T> GuardedBound<T, (), ()> {
    pub(in crate::plan) fn unmetered(self) -> T {
        match self.value {
            BoundValue::Legacy(v) => v,
            BoundValue::Metered(_) => unreachable!("legacy binding backend"),
        }
    }
}
impl<T, I: Copy + Eq, L> GuardedBound<T, I, L> {
    pub(in crate::plan) fn plain(value: T, issuer: I) -> Self {
        Self {
            value: BoundValue::Legacy(value),
            _memory: Memory::empty(issuer),
        }
    }

    pub(in crate::plan) fn adopt_memory<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        self,
        memory: Memory<I, L>,
        budget: &mut QueryBudget<'_, A>,
    ) -> Result<Self, A> {
        let mut guarded = Owned {
            value: self,
            memory,
        };
        budget.check_memory(&guarded.value._memory)?;
        budget.check_memory(&guarded.memory)?;
        budget.merge_checked(&mut guarded.value._memory, &mut guarded.memory)?;
        budget.check_memory(&guarded.value._memory)?;
        Ok(guarded.value)
    }

    /// Independent Core lanes stay behind both values throughout collection.
    pub(in crate::plan) fn zip<U, A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        self,
        other: GuardedBound<U, I, L>,
        budget: &mut QueryBudget<'_, A>,
    ) -> Result<GuardedBound<(T, U), I, L>, A> {
        struct Inputs<T, U, I, L> {
            left: Option<BoundValue<T, I, L>>,
            right: Option<BoundValue<U, I, L>>,
            memory: Memory<I, L>,
            other_memory: Memory<I, L>,
        }
        struct Joined<T, I, L> {
            value: BoundValue<T, I, L>,
            memory: Memory<I, L>,
            other_memory: Memory<I, L>,
        }
        let mut inputs = Inputs {
            left: Some(self.value),
            right: Some(other.value),
            memory: self._memory,
            other_memory: other._memory,
        };
        budget.check_memory(&inputs.memory)?;
        budget.check_memory(&inputs.other_memory)?;
        let value = {
            let mut adapter = CoreAdmission {
                outer: budget.admission,
            };
            if let Some(BoundValue::Metered(value)) = inputs.left.as_ref() {
                core::<_, A>(value.validate_composition(&adapter))?;
            }
            if let Some(BoundValue::Metered(value)) = inputs.right.as_ref() {
                core::<_, A>(value.validate_composition(&adapter))?;
            }
            match (
                inputs.left.take().expect("guarded left value"),
                inputs.right.take().expect("guarded right value"),
            ) {
                (BoundValue::Legacy(left), BoundValue::Legacy(right)) => {
                    BoundValue::Legacy((left, right))
                }
                (BoundValue::Legacy(left), BoundValue::Metered(right)) => BoundValue::Metered(
                    core::<_, A>(right.map_value(&adapter, |right| (left, right)))?,
                ),
                (BoundValue::Metered(left), BoundValue::Legacy(right)) => BoundValue::Metered(
                    core::<_, A>(left.map_value(&adapter, |left| (left, right)))?,
                ),
                (BoundValue::Metered(left), BoundValue::Metered(right)) => {
                    BoundValue::Metered(core::<_, A>(left.zip_value(right, &mut adapter))?)
                }
            }
        };
        let mut joined = Joined {
            value,
            memory: inputs.memory,
            other_memory: inputs.other_memory,
        };
        budget.check_memory(&joined.memory)?;
        budget.check_memory(&joined.other_memory)?;
        budget.merge_checked(&mut joined.memory, &mut joined.other_memory)?;
        budget.check_memory(&joined.memory)?;
        Ok(GuardedBound {
            value: joined.value,
            _memory: joined.memory,
        })
    }

    /// The callback is allocation-free and all of its actual owning fields
    /// remain guarded by its caller through pre/post-map failure.
    pub(in crate::plan) fn map<U, A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        self,
        budget: &mut QueryBudget<'_, A>,
        f: impl FnOnce(T) -> U,
    ) -> Result<GuardedBound<U, I, L>, A> {
        budget.check_memory(&self._memory)?;
        let value = match self.value {
            BoundValue::Legacy(v) => BoundValue::Legacy(f(v)),
            BoundValue::Metered(v) => {
                let adapter = CoreAdmission {
                    outer: budget.admission,
                };
                BoundValue::Metered(core::<_, A>(v.map_value(&adapter, f))?)
            }
        };
        let result = GuardedBound {
            value,
            _memory: self._memory,
        };
        budget.check_memory(&result._memory)?;
        Ok(result)
    }
}
// This extraction is deliberately limited to the actual flattened walker.
// A Core-backed value cannot escape its independent Core guard through it.
impl<'plan, I: Copy + Eq, L> GuardedBound<crate::plan::audio::AudioWalkSpan<'plan>, I, L> {
    pub(in crate::plan) fn flattened_leaf<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        self,
        memory: Memory<I, L>,
        budget: &mut QueryBudget<'_, A>,
    ) -> Result<Owned<crate::plan::audio::AudioSpan, I, L>, A> {
        use crate::plan::audio::AudioWalkSpan;
        // Check while the complete value precedes both independent guards.
        let guarded = Owned {
            value: self,
            memory,
        };
        budget.check_memory(&guarded.memory)?;
        budget.check_memory(&guarded.value._memory)?;
        match guarded {
            Owned {
                value:
                    GuardedBound {
                        value: BoundValue::Legacy(AudioWalkSpan::Leaf(span)),
                        _memory: mut leaf_memory,
                    },
                memory,
            } => {
                // No callback or fallible operation separates this exact Leaf
                // move from its complete value-before-memory carrier.
                let mut result = Owned {
                    value: span,
                    memory,
                };
                budget.merge_checked(&mut result.memory, &mut leaf_memory)?;
                budget.check_memory(&result.memory)?;
                Ok(result)
            }
            rejected => {
                // Keep the rejected Stage/Core value ahead of both guards.
                let label = match &rejected.value.value {
                    BoundValue::Legacy(_) => "flattened audio retained a stage",
                    BoundValue::Metered(_) => "flattened audio retained a metered binding",
                };
                Err(PlanError::InvalidPlan(label).into())
            }
        }
    }
}
impl<'plan, I: Copy + Eq, L> GuardedBound<AudioBound<'plan>, I, L> {
    pub(in crate::plan) fn boxed<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        self,
        budget: &mut QueryBudget<'_, A>,
    ) -> Result<GuardedBound<Box<AudioBound<'plan>>, I, L>, A> {
        budget.check_memory(&self._memory)?;
        if !A::METERED {
            let BoundValue::Legacy(value) = self.value else {
                return Err(PlanError::InvalidPlan("binding accounting mode mismatch").into());
            };
            return Ok(GuardedBound {
                value: BoundValue::Legacy(Box::new(value)),
                _memory: self._memory,
            });
        }
        let mut shell = Owned {
            value: None,
            memory: budget.memory(),
        };
        budget.reserve(
            &mut shell.memory,
            std::alloc::Layout::new::<AudioBound<'plan>>(),
        )?;
        shell.value = Some(Box::<AudioBound<'plan>>::new_uninit());
        let mut result = self.map(budget, |value| {
            Box::write(shell.value.take().expect("prepared Box shell"), value)
        })?;
        budget.check_memory(&result._memory)?;
        budget.check_memory(&shell.memory)?;
        budget.merge_checked(&mut result._memory, &mut shell.memory)?;
        Ok(result)
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
        budget.merge_checked(memory, &mut capacity)?;
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
        budget.merge_checked(&mut memory, &mut instance_capacity)?;
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
                #[cfg(test)]
                super::super::audio_owners::tests::legacy_snapshot_v1::capture_baseline::core_receipt(
                    value.value().work, value.value().lattice.work, work.units(),
                    work.canonical_units(),work.additional_units(),work.was_forwarded(),
                );
                #[cfg(test)]
                super::super::audio_owners::tests::legacy_snapshot_v1::capture_baseline::paired_core_binding(
                    &self.audio_bindings, id, environment, gap.is_some(), budget.remaining,
                    value.value(), (work.units(), work.canonical_units(), work.additional_units(), work.was_forwarded()),
                );
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
        budget.merge_checked(&mut memory, &mut constraint_capacity)?;
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
        budget.merge_checked(&mut domain.memory, &mut capacity)?;
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
            budget.merge_checked(&mut domain.memory, &mut next_capacity)?;
        }
        let mut cap = budget.memory();
        domain.value.instance = budget.instance(&mut domain.memory, &mut cap, &self.instance)?;
        budget.check_memory(&domain.memory)?;
        budget.check_memory(&cap)?;
        budget.merge_checked(&mut domain.memory, &mut cap)?;
        Ok(Some(domain))
    }
}

/// Value-tied preflight. The policy borrow ends before this token is returned.
pub(in crate::plan) struct GuardedChargeTransfer<'value, T, I, L> {
    plan: Option<&'value mut L>,
    core: Option<deadpan_core::AudioBindingChargeTransfer<'value, T, I, L>>,
}
impl<T, I: Copy + Eq, L> GuardedBound<T, I, L> {
    pub(in crate::plan) fn merge_memory<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        &mut self,
        memory: &mut Memory<I, L>,
        budget: &mut QueryBudget<'_, A>,
    ) -> Result<(), A> {
        budget.check_memory(&self._memory)?;
        budget.check_memory(memory)?;
        budget.merge_checked(&mut self._memory, memory)?;
        Ok(())
    }
    pub(in crate::plan) fn charges(&self) -> [Option<&L>; 3] {
        let (lease, buffer) = match &self.value {
            BoundValue::Legacy(_) => (None, None),
            BoundValue::Metered(value) => value.charges(),
        };
        [self._memory.lease.as_ref(), lease, buffer]
    }
    pub(in crate::plan) fn preflight_transfer<
        A: AudioSoundContextAdmission<Issuer = I, Lease = L>,
    >(
        &mut self,
        admission: &mut A,
    ) -> Result<GuardedChargeTransfer<'_, T, I, L>, A> {
        if self._memory.issuer != admission.issuer() {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }
        let core = match &mut self.value {
            BoundValue::Legacy(_) => None,
            BoundValue::Metered(value) => {
                let adapter = CoreAdmission { outer: admission };
                Some(core::<_, A>(value.preflight_charge_transfer(&adapter))?)
            }
        };
        Ok(GuardedChargeTransfer {
            plan: self._memory.lease.as_mut(),
            core,
        })
    }
}
impl<T, I: Copy + Eq, L> GuardedChargeTransfer<'_, T, I, L> {
    pub(in crate::plan) fn leases(&self) -> [Option<&L>; 3] {
        let (lease, buffer) = self
            .core
            .as_ref()
            .map_or((None, None), |value| value.charges());
        [self.plan.as_deref(), lease, buffer]
    }
    /// ONE trusted non-unwinding, allocation-free callback performs publication.
    pub(in crate::plan) fn commit(
        self,
        publish: impl FnOnce(Option<&mut L>, Option<&mut L>, Option<&mut L>),
    ) {
        match self.core {
            Some(core) => core.commit(|lease, buffer| publish(self.plan, lease, buffer)),
            None => publish(self.plan, None, None),
        }
    }
}
