//! Actual owner query loop with allocation-site guards.
use super::*;
use crate::AudioSoundContextAdmission;
use crate::audio_allocated::admission::array;
use admission::{Memory, Owned, Result};

pub(super) struct State<I, L> {
    pub(super) value: Walk,
    pub(super) memory: Memory<I, L>,
    capacity: Memory<I, L>,
}
impl<I, L> std::ops::Deref for State<I, L> {
    type Target = Walk;
    fn deref(&self) -> &Walk {
        &self.value
    }
}
impl<I, L> std::ops::DerefMut for State<I, L> {
    fn deref_mut(&mut self) -> &mut Walk {
        &mut self.value
    }
}
impl<I: Copy + Eq, L> State<I, L> {
    pub(super) fn plain(value: Walk, issuer: I) -> Self {
        Self {
            value,
            memory: Memory::empty(issuer),
            capacity: Memory::empty(issuer),
        }
    }
    pub(super) fn from_seed<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        seed: &AudioWalkSeed,
        budget: &mut admission::Budget<'_, A>,
    ) -> Result<Self, A> {
        let placement = Placement {
            origin: seed.transform.project_origin,
            scale: seed.transform.project_frames_per_local_frame,
        };
        let value = Walk {
            node: seed.node,
            definition: None,
            repeats: Vec::new(),
            gap: None,
            bypass: seed.bypass_binding,
            grid: seed.grid,
            placement,
            binding: BindingClock {
                grid: seed.grid,
                placement,
            },
            extent: seed.extent.clone(),
            support: seed.envelope.clone(),
            allow_inactive: false,
        };
        let mut result = Self::plain(value, budget.issuer);
        result.value.definition = budget.definition(&mut result.memory, &seed.definition)?;
        result.value.repeats =
            budget.repeats(&mut result.memory, &mut result.capacity, &seed.repeats)?;
        result.value.gap = budget.gap(&mut result.memory, &seed.gap)?;
        Ok(result)
    }
    fn copy<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        &self,
        budget: &mut admission::Budget<'_, A>,
    ) -> Result<Self, A> {
        let Walk {
            node,
            definition,
            repeats,
            gap,
            bypass,
            grid,
            placement,
            binding,
            extent,
            support,
            allow_inactive,
        } = &self.value;
        let value = Walk {
            node: *node,
            definition: None,
            repeats: Vec::new(),
            gap: None,
            bypass: *bypass,
            grid: *grid,
            placement: *placement,
            binding: *binding,
            extent: extent.clone(),
            support: support.clone(),
            allow_inactive: *allow_inactive,
        };
        let mut result = Self::plain(value, budget.issuer);
        result.value.definition = budget.definition(&mut result.memory, definition)?;
        result.value.repeats = budget.repeats(&mut result.memory, &mut result.capacity, repeats)?;
        result.value.gap = budget.gap(&mut result.memory, gap)?;
        Ok(result)
    }
    fn push_repeat<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        &mut self,
        value: RepeatInstance,
        source: &mut Memory<I, L>,
        budget: &mut admission::Budget<'_, A>,
    ) -> Result<(), A> {
        budget.own_iteration(source, &value.iteration)?;
        let needed = self.value.repeats.len() + 1;
        budget.grow(&mut self.value.repeats, &mut self.capacity, needed)?;
        budget.check_memory(&self.memory)?;
        budget.check_memory(source)?;
        self.value.repeats.push(value);
        budget.merge_checked(&mut self.memory, source)?;
        Ok(())
    }
}
pub(super) struct Output<I, L> {
    pub(super) value: WalkOutput,
    memory: Memory<I, L>,
    capacity: Memory<I, L>,
}
impl<I, L> std::ops::Deref for Output<I, L> {
    type Target = WalkOutput;
    fn deref(&self) -> &WalkOutput {
        &self.value
    }
}
impl<I, L> std::ops::DerefMut for Output<I, L> {
    fn deref_mut(&mut self) -> &mut WalkOutput {
        &mut self.value
    }
}
impl<I: Copy, L> Output<I, L> {
    pub(super) fn plain(value: WalkOutput, issuer: I) -> Self {
        Self {
            value,
            memory: Memory::empty(issuer),
            capacity: Memory::empty(issuer),
        }
    }
}
pub(in crate::plan) struct Product<'plan, S, I, L> {
    pub(in crate::plan) query: Owned<AudioOwnerQuery<'plan, S>, I, L>,
    pub(in crate::plan) canonical: usize,
    pub(in crate::plan) additional: usize,
}
pub(super) fn query_tracked<'plan, S: Copy, A: AudioSoundContextAdmission>(
    plan: &'plan RenderPlan,
    samples: Range<i64>,
    limits: AudioQueryLimits,
    seed: State<A::Issuer, A::Lease>,
    label: impl Fn(i64) -> S,
    retained: Option<&AudioOwnerRetainedInput<'plan>>,
    budget: &mut admission::Budget<'_, A>,
) -> Result<Product<'plan, S, A::Issuer, A::Lease>, A> {
    limits.validate()?;
    if samples.end < samples.start {
        return Err(PlanError::AudioRangeOutOfRange.into());
    }
    let mut payload = budget.memory();
    let mut capacity = budget.memory();
    let mut spans = Vec::new();
    let mut cursor = samples.start;
    while cursor < samples.end {
        if spans.len() == limits.maximum_spans {
            return Err(PlanError::AudioQueryLimit("owner span count").into());
        }
        let mut output = Output::plain(
            WalkOutput {
                start: cursor,
                end: samples.end,
                owners: Vec::new(),
                support: AudioOwnerSupport::Active,
                through_processor: false,
                original_is_silent: false,
            },
            budget.issuer,
        );
        let retained_origin = retained
            .map(|input| input.clock_origin_admitted(budget))
            .transpose()?;
        let current_origin = AudioOwnerClockOrigin::Current;
        let origin = retained_origin
            .as_ref()
            .map_or(&current_origin, |origin| &origin.value);
        walk_query(
            plan,
            seed.copy(budget)?,
            match retained {
                Some(input) => Position {
                    at: input.reference_at(AudioSample(cursor))?,
                    step: input.reference_samples_per_output_sample(),
                },
                None => Position {
                    at: ExactRatio::integer(cursor),
                    step: ExactRatio::ONE,
                },
            },
            &mut output,
            origin,
            budget,
            0,
        )?;
        if output.end <= cursor
            || (output.owners.is_empty() && output.support == AudioOwnerSupport::Active)
        {
            return Err(PlanError::InvalidPlan("owner interval did not advance").into());
        }
        let mut owners_capacity = budget.memory();
        let map_owner = |owner: RawOwner| -> std::result::Result<_, PlanError> {
            Ok(AudioOwnerClock {
                plan,
                definition: owner.definition,
                instance: owner.instance,
                kind: owner.kind,
                gap_after: owner.gap_after,
                play: owner.play,
                origin: owner.origin,
                sampling: AudioSampleMap::new(
                    label(cursor),
                    owner.position.at,
                    owner.position.step,
                )?,
            })
        };
        let owners = if A::METERED {
            budget.reserve(
                &mut owners_capacity,
                array::<AudioOwnerClock<'plan, S>>(output.owners.len())?,
            )?;
            let mut owners = Vec::with_capacity(output.owners.len());
            assert_eq!(owners.capacity(), output.owners.len());
            for owner in std::mem::take(&mut output.value.owners) {
                owners.push(map_owner(owner)?);
            }
            owners
        } else {
            // The original Result<Vec> collect backend and its capacity behavior.
            std::mem::take(&mut output.value.owners)
                .into_iter()
                .map(map_owner)
                .collect::<std::result::Result<Vec<_>, PlanError>>()?
        };
        let needed = spans.len() + 1;
        budget.grow(&mut spans, &mut capacity, needed)?;
        budget.check_memory(&payload)?;
        budget.check_memory(&output.memory)?;
        budget.check_memory(&owners_capacity)?;
        spans.push(AudioOwnerSpan {
            samples: label(cursor)..label(output.end),
            owners,
            support: output.support,
            original_is_silent: output.original_is_silent,
        });
        budget.merge_checked(&mut payload, &mut output.memory)?;
        budget.merge_checked(&mut payload, &mut owners_capacity)?;
        cursor = output.end;
    }
    budget.check_memory(&payload)?;
    budget.check_memory(&capacity)?;
    budget.merge_checked(&mut payload, &mut capacity)?;
    let canonical = budget.canonical_used;
    let additional = budget.additional_used;
    let work = canonical
        .checked_add(additional)
        .ok_or(TimeError::Overflow)?;
    Ok(Product {
        query: Owned {
            value: AudioOwnerQuery {
                plan,
                samples: label(samples.start)..label(samples.end),
                spans,
                lookup: budget.lookup,
                work,
            },
            memory: payload,
        },
        canonical,
        additional,
    })
}
pub(super) fn walk_query<A: crate::AudioSoundContextAdmission>(
    plan: &RenderPlan,
    mut state: State<A::Issuer, A::Lease>,
    position: Position,
    output: &mut Output<A::Issuer, A::Lease>,
    origin: &AudioOwnerClockOrigin,
    budget: &mut admission::Budget<'_, A>,
    depth: usize,
) -> admission::Result<(), A> {
    if depth > deadpan_core::MAX_DOCUMENT_DEPTH {
        return Err(PlanError::AudioQueryLimit("owner binding depth").into());
    }
    let mut probe_label =
        AudioSample(i64::try_from(position.at.floor()).map_err(|_| TimeError::Overflow)?);
    let (mut probe, mut bias) = state.grid.probe(probe_label)?;
    loop {
        budget.canonical(1 + state.repeats.len())?;
        budget.lookup.visited_nodes += 1;
        let node = &plan.nodes[state.node];
        let kind = if state.gap.is_some() {
            AudioOwnerKind::DefaultGap
        } else {
            AudioOwnerKind::Node
        };
        let target = if state.gap.is_some() {
            BindingTarget::Gap(state.node)
        } else {
            BindingTarget::Node(state.node)
        };
        let duration = state.gap.as_ref().map_or(node.extent, |gap| {
            ExactRatio::integer(gap.duration.frames())
        });
        let extent = state.placement.origin..state.placement.frame(duration)?;
        state.value.extent = intersect(state.value.extent, extent.clone())?;
        if state.gap.is_some()
            || !matches!(
                node.kind,
                CompiledKind::Sequence { .. }
                    | CompiledKind::Repeat { .. }
                    | CompiledKind::Retime {
                        purpose: RetimePurpose::Partition,
                        ..
                    }
            )
        {
            state.value.support = Some(match state.value.support {
                Some(previous) => intersect(previous, extent)?,
                None => extent,
            });
        }
        let allocated = state.grid.boundary(state.value.extent.start)?
            ..state.grid.boundary(state.value.extent.end)?;
        // A retained clock receives this sample's start transferred from its
        // consumer, for example a RoundEven output allocation that begins a
        // fraction of an output sample before the owner's exact origin, mapped
        // onto a Preserve input's PointCeil grid. That first output sample
        // still overlaps the allocation and belongs to its first sample, as on
        // the current clock; only a sample wholly before it is outside.
        // A transferred interval can span more than one reference sample.
        if probe_label < allocated.start
            && allocated.start < allocated.end
            && matches!(origin, AudioOwnerClockOrigin::Retained { .. })
            && position
                .at
                .checked_add(position.step)?
                .compare_integer(allocated.start.0)
                .is_gt()
        {
            probe_label = allocated.start;
            (probe, bias) = state.grid.probe(probe_label)?;
        }
        if !allocated.contains(&probe_label) {
            if state.allow_inactive && matches!(origin, AudioOwnerClockOrigin::Retained { .. }) {
                if probe_label < allocated.start {
                    restrict_end(position, allocated.start.0, output.start, &mut output.end)?;
                }
                output.support = AudioOwnerSupport::Inactive;
                return Ok(());
            }
            return Err(PlanError::InvalidPlan(
                "owner coordinate is outside its allocated interval",
            )
            .into());
        }
        restrict_end(position, allocated.end.0, output.start, &mut output.end)?;
        let frame = state
            .grid
            .frame_origin()
            .checked_add(position.at.checked_mul(state.grid.frames_per_sample())?)?;
        let local = Position {
            at: state.placement.local(frame)?,
            step: position
                .step
                .checked_mul(state.grid.frames_per_sample())?
                .checked_div(state.placement.scale)?,
        };
        // The inherited canonical(1 + repeats.len()) above pays this path
        // copy. Helpers add only actual new ownership/layout inspections.
        let mut instance_memory = budget.memory();
        let mut instance_capacity = budget.memory();
        let instance = InstancePath {
            node: budget.node(&mut instance_memory, &node.inspection.id)?,
            repeats: budget.repeats(
                &mut instance_memory,
                &mut instance_capacity,
                &state.repeats,
            )?,
        };
        budget.check_memory(&instance_memory)?;
        budget.check_memory(&instance_capacity)?;
        budget.merge_checked(&mut instance_memory, &mut instance_capacity)?;
        let gap_after = state
            .gap
            .as_ref()
            .and_then(|gap| gap.after.as_ref())
            .map(|v| budget.iteration(&mut instance_memory, v))
            .transpose()?;
        if state.bypass != Some(target) {
            let transform = SignalTransform {
                signal_origin: state.binding.placement.origin,
                signal_frames_per_local_frame: state.binding.placement.scale,
                grid_origin: state.binding.grid.frame_origin(),
                signal_frames_per_sample: state.binding.grid.frames_per_sample(),
            };
            let binding_frame =
                |value| state.binding.placement.frame(state.placement.local(value)?);
            let allocated_start = state
                .binding
                .grid
                .boundary(binding_frame(state.value.extent.start)?)?;
            let support = state
                .support
                .as_ref()
                .map(|range| {
                    Ok::<_, TimeError>(binding_frame(range.start)?..binding_frame(range.end)?)
                })
                .transpose()?;
            if let Some(bound) = plan.bound_at_query(
                state.node,
                &state.repeats,
                state.definition.as_ref(),
                BoundPlacement {
                    transform,
                    grid: state.binding.grid.rebrand(),
                    allocated_start: SignalSample(allocated_start.0),
                    support: support.as_ref(),
                    constraints: &[],
                    gap: state.gap.as_ref(),
                },
                budget,
            )? {
                let Some(domain) = bound.envelope_domain_query(budget)? else {
                    if state.allow_inactive {
                        output.support = AudioOwnerSupport::Inactive;
                        return Ok(());
                    }
                    return Err(PlanError::InvalidPlan(
                        "owner binding has empty meaningful support",
                    )
                    .into());
                };
                let physical_at = state
                    .binding
                    .placement
                    .frame(local.at)?
                    .checked_sub(state.binding.grid.frame_origin())?
                    .checked_div(state.binding.grid.frames_per_sample())?;
                let physical_step = local
                    .step
                    .checked_mul(state.binding.placement.scale)?
                    .checked_div(state.binding.grid.frames_per_sample())?;
                let reference = Position {
                    at: bound.reference_at_offset(0)?.checked_add(
                        physical_at
                            .checked_sub(ExactRatio::integer(allocated_start.0))?
                            .checked_mul(bound.reference_samples_per_output_sample())?,
                    )?,
                    step: physical_step.checked_mul(bound.reference_samples_per_output_sample())?,
                };
                let mut retained = State::from_seed(&domain.seed, budget)?;
                retained.value.allow_inactive = state.allow_inactive;
                // These moved values are still covered by the enclosing state
                // and instance scopes until the actual recursive call returns.
                let next_origin = AudioOwnerClockOrigin::Retained {
                    definition: state.value.definition.take(),
                    instance,
                    kind,
                    gap_after,
                };
                return walk_query(
                    plan,
                    retained,
                    reference,
                    output,
                    &next_origin,
                    budget,
                    depth + 1,
                );
            }
        }
        budget.canonical(state.definition.is_some() as usize + 1)?;
        let needed = output.value.owners.len() + 1;
        budget.grow(&mut output.value.owners, &mut output.capacity, needed)?;
        let definition = budget.definition(&mut instance_memory, &state.definition)?;
        let copied_origin = budget.origin(&mut instance_memory, origin)?;
        budget.check_memory(&output.memory)?;
        budget.check_memory(&instance_memory)?;
        output.value.owners.push(RawOwner {
            definition,
            instance,
            kind,
            gap_after,
            play: None,
            position: local,
            origin: copied_origin,
        });
        budget.merge_checked(&mut output.memory, &mut instance_memory)?;
        if state.gap.is_some() {
            return Ok(());
        }
        let local_probe = state.placement.local(probe)?;
        match &node.kind {
            CompiledKind::Source { audio, .. } => {
                output.original_is_silent = !output.through_processor && audio.is_none();
                return Ok(());
            }
            CompiledKind::Hold { audio, .. } => {
                output.original_is_silent =
                    !output.through_processor && matches!(audio, deadpan_core::HoldAudio::Silence);
                return Ok(());
            }
            CompiledKind::Sequence { entries } => {
                let mut left = 0;
                let mut right = entries.len();
                while left < right {
                    budget.canonical(1)?;
                    budget.lookup.sequence_comparisons += 1;
                    let middle = left + (right - left) / 2;
                    let comparison = local_probe.compare(entries[middle].end);
                    if comparison.is_gt() || (comparison.is_eq() && bias == InsertionBias::Right) {
                        left = middle + 1;
                    } else {
                        right = middle;
                    }
                }
                let entry = entries
                    .get(left)
                    .ok_or(PlanError::InvalidPlan("owner Sequence has no child"))?;
                state.child(entry.start, ExactRatio::ONE)?;
                state.node = entry.child;
            }
            CompiledKind::Retime {
                child,
                start,
                scale,
                pitch,
                ..
            } => {
                let inverse = ExactRatio::ONE.checked_div(*scale)?;
                state.child(
                    ExactRatio::ZERO.checked_sub(start.checked_mul(inverse)?)?,
                    inverse,
                )?;
                if pitch.processes(*scale == ExactRatio::ONE) {
                    output.through_processor = true;
                    // Only the binding-resolution clock switches. Automation
                    // remains on the consuming output's exact nominal mapping.
                    // Canonical Preserve prepares its complete selected input;
                    // an outer crop limits delivery, not this hidden support.
                    // Retain `extent` for that delivery allocation separately.
                    let input_end = start
                        .checked_add(scale.checked_mul(ExactRatio::integer(
                            node.inspection.duration.frames(),
                        ))?)?;
                    state.value.support =
                        Some(state.placement.frame(*start)?..state.placement.frame(input_end)?);
                    state.binding = BindingClock {
                        grid: AudioSampleGrid::new(
                            *start,
                            state.grid.frames_per_sample(),
                            AudioBoundaryRule::PointCeil,
                        )?,
                        placement: Placement {
                            origin: ExactRatio::ZERO,
                            scale: ExactRatio::ONE,
                        },
                    };
                }
                state.node = *child;
            }
            CompiledKind::Repeat {
                layout,
                gap_duration,
                ..
            } => {
                let location = layout
                    .locate_bounded(local_probe, bias, budget.remaining)
                    .map_err(|error| {
                        if error.code == deadpan_core::DocumentErrorCode::LimitExceeded {
                            PlanError::AudioQueryLimit("owner Repeat work")
                        } else {
                            error.into()
                        }
                    })?;
                budget.canonical(location.comparisons)?;
                budget.lookup.iteration_run_comparisons += location.comparisons;
                // The Repeat's own owner was pushed above; record its play.
                if let Some(owner) = output.owners.last_mut() {
                    owner.play = Some(location.play.index);
                }
                let mut repeat_memory = budget.memory();
                let repeat = budget.node(&mut repeat_memory, &node.inspection.id)?;
                if location.in_gap {
                    let begin = location.play.start.checked_add(location.play.duration)?;
                    state.child(begin, ExactRatio::ONE)?;
                    if let Some(child) = location.play.gap_child {
                        state.push_repeat(
                            RepeatInstance {
                                node: repeat,
                                iteration: location.play.iteration,
                            },
                            &mut repeat_memory,
                            budget,
                        )?;
                        state.node = plan.by_id[&child];
                    } else {
                        // location.play owns its actual iteration; its identity
                        // edge is newly retained when moved into this state.
                        budget.own_iteration(&mut state.memory, &location.play.iteration)?;
                        state.value.gap = Some(DomainGap {
                            after: Some(location.play.iteration),
                            duration: *gap_duration,
                        });
                    }
                } else {
                    state.child(location.play.start, ExactRatio::ONE)?;
                    state.push_repeat(
                        RepeatInstance {
                            node: repeat,
                            iteration: location.play.iteration,
                        },
                        &mut repeat_memory,
                        budget,
                    )?;
                    state.node = plan.by_id[&location.play.child];
                }
            }
        }
    }
}

pub(in crate::plan) fn root_candidates_query<'plan, A: AudioSoundContextAdmission>(
    plan: &'plan RenderPlan,
    at: AudioSample,
    limits: AudioQueryLimits,
    admission: &mut A,
) -> std::result::Result<Product<'plan, AudioSample, A::Issuer, A::Lease>, A::Error> {
    admission::outer::<_, A>((|| {
        let end = AudioSample(at.0.checked_add(1).ok_or(TimeError::Overflow)?);
        if at.0 < 0 || end > plan.audio_duration()? {
            return Err(PlanError::AudioRangeOutOfRange.into());
        }
        let mut local = super::Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut budget = admission::Budget::new(&mut local, admission);
        let mut seed = State::plain(Walk::root(plan)?, budget.issuer);
        seed.value.allow_inactive = true;
        query_tracked(
            plan,
            at.0..end.0,
            limits,
            seed,
            AudioSample,
            None,
            &mut budget,
        )
    })())
}
pub(in crate::plan) fn domain_candidates_query<'plan, A: AudioSoundContextAdmission>(
    domain: &AudioDomain<'plan>,
    at: AudioSample,
    limits: AudioQueryLimits,
    admission: &mut A,
) -> std::result::Result<Product<'plan, AudioSample, A::Issuer, A::Lease>, A::Error> {
    admission::outer::<_, A>((|| {
        let end = AudioSample(at.0.checked_add(1).ok_or(TimeError::Overflow)?);
        if at < domain.samples.start || end > domain.samples.end {
            return Err(PlanError::AudioRangeOutOfRange.into());
        }
        let mut local = super::Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut budget = admission::Budget::new(&mut local, admission);
        // Same seed copy BEFORE the inherited query limits validation.
        let mut seed = State::from_seed(&domain.seed, &mut budget)?;
        seed.value.allow_inactive = true;
        query_tracked(
            domain.plan,
            at.0..end.0,
            limits,
            seed,
            AudioSample,
            None,
            &mut budget,
        )
    })())
}
