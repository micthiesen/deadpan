//! The single actual complete capture/legacy walk, with allocation-site guards.
use super::*;
use crate::AudioSoundContextAdmission;
use crate::plan::audio_bound::query::GuardedBound;
use crate::plan::audio_boundary::AdmittedExtent;
use crate::plan::audio_owners::admission::{Budget as QueryBudget, Memory, Owned, Result};
#[path = "audio_capture_storage.rs"]
mod storage;

pub(in crate::plan) struct WalkResult<'plan, I, L> {
    pub(in crate::plan) value: GuardedBound<AudioWalkSpan<'plan>, I, L>,
    pub(in crate::plan) memory: Memory<I, L>,
}
#[cfg(test)]
impl<'plan, I: Copy + Eq, L> WalkResult<'plan, I, L> {
    pub(in crate::plan) fn span(&self) -> &AudioWalkSpan<'plan> {
        &self.value
    }
}
impl<'plan> WalkResult<'plan, (), ()> {
    pub(in crate::plan) fn unmetered(self) -> AudioWalkSpan<'plan> {
        self.value.unmetered()
    }
}
type Content<'plan, I, L> = GuardedBound<AudioSignalContent<'plan>, I, L>;
impl<'plan, I: Copy + Eq, L> GuardedBound<AudioSignalContent<'plan>, I, L> {
    fn is_leaf(&self) -> bool {
        matches!(&**self, AudioSignalContent::Leaf(_))
    }
    fn finish<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        self,
        memory: Memory<I, L>,
        budget: &mut QueryBudget<'_, A>,
        make: impl FnOnce(AudioSignalContent<'plan>) -> AudioWalkSpan<'plan>,
    ) -> Result<WalkResult<'plan, I, L>, A> {
        struct Mapping<C, F, I, L> {
            value: C,
            make: F,
            memory: Memory<I, L>,
        }
        let guarded = Mapping {
            value: self,
            make,
            memory,
        };
        budget.check_memory(&guarded.memory)?;
        let value = guarded.value.map(budget, guarded.make)?;
        let result = WalkResult {
            value,
            memory: guarded.memory,
        };
        budget.check_memory(&result.memory)?;
        Ok(result)
    }
}
fn stage_content<'plan, A: AudioSoundContextAdmission>(
    mut stage: Owned<AudioStage<'plan>, A::Issuer, A::Lease>,
    memory: &mut Memory<A::Issuer, A::Lease>,
    budget: &mut QueryBudget<'_, A>,
) -> Result<Content<'plan, A::Issuer, A::Lease>, A> {
    budget.check_memory(memory)?;
    budget.check_memory(&stage.memory)?;
    budget.merge_checked(memory, &mut stage.memory);
    Ok(Content::plain(
        AudioSignalContent::Stage(stage.value),
        budget.issuer,
    ))
}
// Preserve separate slot/editorial/clock borrows and the observed clock's
// independent Plan lifetime; this names only the existing argument tuple.
type WalkCaptures<'slot, 'editorial, 'clock, 'clock_plan, I, L> = (
    Option<&'slot mut Owned<Option<AudioDomainSeed>, I, L>>,
    Option<&'editorial mut Owned<EditorialCapture, I, L>>,
    Option<&'clock super::super::AudioOwnerClock<'clock_plan>>,
);

impl RenderPlan {
    pub(in crate::plan) fn audio_walk_inner_admitted<'plan, A: AudioSoundContextAdmission>(
        &'plan self,
        sample: AudioSample,
        budget: &mut QueryBudget<'_, A>,
        seed: Option<&AudioWalkSeed>,
        stop_at_preserve: bool,
        stop_at_bindings: bool,
        captures: WalkCaptures<'_, '_, '_, '_, A::Issuer, A::Lease>,
    ) -> Result<WalkResult<'plan, A::Issuer, A::Lease>, A> {
        macro_rules! push {
            ($data:ident,$capacity:ident,$value:expr) => {{
                let needed = $data.len() + 1;
                budget.grow(&mut $data, &mut $capacity, needed)?;
                $data.push($value);
            }};
        }
        let (mut capture, editorial_capture, input_owner) = captures;
        let rate = self.metadata.presentation_basis.frame_rate;
        let mut transform = AudioTransform {
            project_origin: ExactRatio::ZERO,
            project_frames_per_local_frame: ExactRatio::integer(1),
            project_frames_per_sample: ExactRatio::new(
                i128::from(rate.numerator()),
                i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
            )?,
        };
        let grid = seed.map(|seed| seed.grid).map(Ok).unwrap_or_else(|| {
            AudioSampleGrid::<AudioSample>::new(
                ExactRatio::ZERO,
                transform.project_frames_per_sample,
                AudioBoundaryRule::RoundEven,
            )
        })?;
        // Search the allocated interval, not the PCM sample's coordinate.
        let (probe, bias) = grid.probe(sample)?;
        let mut extent = ExactRatio::ZERO..ExactRatio::integer(self.duration().frames());
        let mut envelope: Option<Range<ExactRatio>> = None;
        let mut memory = budget.memory();
        let mut constraints_memory = budget.memory();
        let mut constraints_capacity = budget.memory();
        let mut constraints = Vec::new();
        let mut editorial_memory = budget.memory();
        let mut editorial_capacity = budget.memory();
        let mut editorial = Vec::new();
        let mut current = self.root;
        let mut repeats_capacity = budget.memory();
        let mut repeats = Vec::new();
        let mut retimes_capacity = budget.memory();
        let mut retimes = Vec::new();
        let mut gap_memory = budget.memory();
        let mut seeded_gap = None;
        let definition = seed.and_then(|seed| seed.definition.as_ref());
        let bypass_binding = seed.and_then(|seed| seed.bypass_binding);
        let mut suppress_entry_editorial =
            seed.map_or_else(Default::default, |seed| seed.suppress_entry_editorial);
        if let Some(seed) = seed {
            budget.canonical(
                seed.constraints.len()
                    + seed.editorial.len()
                    + seed.repeats.len()
                    + seed.retimes.len(),
            )?;
            current = seed.node;
            transform = seed.transform;
            extent = seed.extent.clone();
            envelope = seed.envelope.clone();
            constraints = storage::copy_vec(
                &seed.constraints,
                &mut constraints_memory,
                &mut constraints_capacity,
                budget,
                |v, m, b| b.constraint(m, v),
            )?;
            editorial = storage::copy_vec(
                &seed.editorial,
                &mut editorial_memory,
                &mut editorial_capacity,
                budget,
                storage::editorial,
            )?;
            repeats = budget.repeats(&mut memory, &mut repeats_capacity, &seed.repeats)?;
            retimes = storage::copy_vec(
                &seed.retimes,
                &mut memory,
                &mut retimes_capacity,
                budget,
                storage::retime,
            )?;
            seeded_gap = budget.gap(&mut gap_memory, &seed.gap)?;
        }
        let mut inherited_envelope;
        let mut inherited_constraints;
        let mut inherited_editorial;
        let mut domain_gap = None;
        let (content, gap_after) = loop {
            budget.canonical(1)?;
            budget.lookup.visited_nodes += 1;
            let node = &self.nodes[current];
            inherited_envelope = envelope.clone();
            inherited_constraints = constraints.len();
            if let Some(gap) = seeded_gap.take() {
                let CompiledKind::Repeat {
                    gap_audio: Some(audio),
                    ..
                } = &node.kind
                else {
                    return Err(PlanError::InvalidPlan("audio domain gap is missing").into());
                };
                let gap_extent = transform.project_origin
                    ..transform.project_at(ExactRatio::integer(gap.duration.frames()))?;
                retain_incident_editorial(&mut editorial, &gap_extent, grid, budget)?;
                inherited_editorial = editorial.len();
                extent = intersect(extent, gap_extent.clone())?;
                envelope = Some(match envelope {
                    Some(previous) => intersect(previous, gap_extent.clone())?,
                    None => gap_extent.clone(),
                });
                push!(
                    constraints,
                    constraints_capacity,
                    EnvelopeConstraint {
                        placement_support: false,
                        range: gap_extent,
                        node: current,
                        repeat_count: repeats.len(),
                        gap_after: gap
                            .after
                            .as_ref()
                            .map(|v| budget.iteration(&mut constraints_memory, v))
                            .transpose()?,
                        kinds: (
                            AudioBoundaryKind::RepeatGapStart,
                            AudioBoundaryKind::RepeatGapEnd,
                        ),
                    }
                );
                let after = gap
                    .after
                    .as_ref()
                    .map(|v| budget.iteration(&mut memory, v))
                    .transpose()?;
                let content = if stop_at_bindings
                    && bypass_binding != Some(BindingTarget::Gap(current))
                    && let Some(bound) = self.bound_at_query(
                        current,
                        &repeats,
                        definition,
                        super::super::audio_bound::BoundPlacement {
                            transform: super::super::SignalTransform {
                                signal_origin: transform.project_origin,
                                signal_frames_per_local_frame: transform
                                    .project_frames_per_local_frame,
                                grid_origin: grid.frame_origin(),
                                signal_frames_per_sample: transform.project_frames_per_sample,
                            },
                            grid: AudioSampleGrid::new(
                                grid.frame_origin(),
                                transform.project_frames_per_sample,
                                grid.boundary_rule(),
                            )?,
                            allocated_start: super::super::SignalSample(
                                grid.boundary(extent.start)?.0,
                            ),
                            support: envelope.as_ref(),
                            constraints: &constraints,
                            gap: Some(&gap),
                        },
                        budget,
                    )? {
                    bound
                        .boxed(budget)?
                        .map(budget, AudioSignalContent::Bound)?
                } else {
                    Content::plain(
                        AudioSignalContent::Leaf(storage::hold(
                            audio,
                            gap.duration,
                            &mut memory,
                            budget,
                        )?),
                        budget.issuer,
                    )
                };
                domain_gap = Some(gap);
                break (content, after);
            }
            let node_extent = transform.project_origin..transform.project_at(node.extent)?;
            retain_incident_editorial(&mut editorial, &node_extent, grid, budget)?;
            inherited_editorial = editorial.len();
            extent = intersect(extent, node_extent.clone())?;
            // Capture the whole owner before descending into its current
            // child. Transparent crops restrict delivery, not this context.
            // A Preserve or retained binding encountered earlier stops the
            // walk and is captured below instead: its DSP must stay intact.
            if let Some(owner) = input_owner
                && owner.kind() == super::super::AudioOwnerKind::Node
                && owner.definition() == definition
                && owner.instance().node == node.inspection.id
                && owner.instance().repeats == repeats
                && let Some(capture) = capture.as_deref_mut()
                && capture.is_none()
            {
                let support = match &inherited_envelope {
                    Some(envelope) => intersect(node_extent.clone(), envelope.clone())?,
                    None => node_extent.clone(),
                };
                budget.canonical(
                    inherited_constraints + inherited_editorial + repeats.len() + retimes.len() + 1,
                )?;
                let samples = grid.boundary(support.start)?..grid.boundary(support.end)?;
                if !samples.contains(&sample) {
                    return Err(
                        PlanError::InvalidPlan("owner input excludes its visible sample").into(),
                    );
                }
                let mut next = Owned {
                    value: None,
                    memory: budget.memory(),
                };
                next.value = Some(AudioDomainSeed {
                    walk: AudioWalkSeed {
                        definition: budget.definition_ref(&mut next.memory, definition)?,
                        bypass_binding,
                        node: current,
                        transform,
                        grid,
                        extent: support,
                        envelope: inherited_envelope.clone(),
                        constraints: storage::fixed(
                            &constraints[..inherited_constraints],
                            &mut next.memory,
                            budget,
                            |v, m, b| b.constraint(m, v),
                        )?,
                        editorial: storage::fixed(
                            &editorial[..inherited_editorial],
                            &mut next.memory,
                            budget,
                            storage::editorial,
                        )?,
                        suppress_entry_editorial,
                        repeats: storage::fixed(&repeats, &mut next.memory, budget, |v, m, b| {
                            b.repeat(m, v)
                        })?,
                        retimes: storage::fixed(
                            &retimes,
                            &mut next.memory,
                            budget,
                            storage::retime,
                        )?,
                        gap: None,
                    },
                    samples,
                    visible: grid.boundary(extent.start)?..grid.boundary(extent.end)?,
                    instance: budget.instance_fixed(&mut next.memory, owner.instance())?,
                });
                budget.check_memory(&next.memory)?;
                budget.check_memory(&capture.memory)?;
                *capture = next;
            }
            let suppressed = std::mem::take(&mut suppress_entry_editorial);
            let own_editorial = deadpan_core::AudioEditorialEdges {
                start: node.audio_editorial_edges.start && !suppressed.start,
                end: node.audio_editorial_edges.end && !suppressed.end,
            };
            if !own_editorial.is_empty() {
                budget.canonical(1)?;
                push!(
                    editorial,
                    editorial_capacity,
                    EditorialConstraint {
                        boundary: EnvelopeConstraint {
                            placement_support: false,
                            range: node_extent.clone(),
                            node: current,
                            repeat_count: repeats.len(),
                            gap_after: None,
                            kinds: (AudioBoundaryKind::NodeStart, AudioBoundaryKind::NodeEnd),
                        },
                        edges: own_editorial,
                        incident_samples: None,
                    }
                );
            }
            if !matches!(
                node.kind,
                CompiledKind::Retime {
                    purpose: RetimePurpose::Partition,
                    ..
                }
            ) {
                push!(
                    constraints,
                    constraints_capacity,
                    EnvelopeConstraint {
                        placement_support: false,
                        range: node_extent.clone(),
                        node: current,
                        repeat_count: repeats.len(),
                        gap_after: None,
                        kinds: (AudioBoundaryKind::NodeStart, AudioBoundaryKind::NodeEnd),
                    }
                );
                if !matches!(
                    node.kind,
                    CompiledKind::Sequence { .. } | CompiledKind::Repeat { .. }
                ) {
                    envelope = Some(match envelope {
                        Some(previous) => intersect(previous, node_extent)?,
                        None => node_extent,
                    });
                }
            }
            if stop_at_bindings
                && bypass_binding != Some(BindingTarget::Node(current))
                && let Some(bound) = self.bound_at_query(
                    current,
                    &repeats,
                    definition,
                    super::super::audio_bound::BoundPlacement {
                        transform: super::super::SignalTransform {
                            signal_origin: transform.project_origin,
                            signal_frames_per_local_frame: transform.project_frames_per_local_frame,
                            grid_origin: grid.frame_origin(),
                            signal_frames_per_sample: transform.project_frames_per_sample,
                        },
                        grid: AudioSampleGrid::new(
                            grid.frame_origin(),
                            transform.project_frames_per_sample,
                            grid.boundary_rule(),
                        )?,
                        allocated_start: super::super::SignalSample(grid.boundary(extent.start)?.0),
                        support: envelope.as_ref(),
                        constraints: &constraints,
                        gap: None,
                    },
                    budget,
                )?
            {
                break (
                    bound
                        .boxed(budget)?
                        .map(budget, AudioSignalContent::Bound)?,
                    None,
                );
            }
            let local = probe
                .checked_sub(transform.project_origin)?
                .checked_div(transform.project_frames_per_local_frame)?;
            match &node.kind {
                CompiledKind::Source { audio: None, .. } => {
                    break (
                        Content::plain(
                            AudioSignalContent::Leaf(AudioContent::Silence {
                                reason: SilenceReason::NoSourceAudio,
                            }),
                            budget.issuer,
                        ),
                        None,
                    );
                }
                CompiledKind::Source {
                    audio: Some(audio), ..
                } => {
                    if audio.selection.start == audio.selection.end {
                        break (
                            Content::plain(
                                AudioSignalContent::Leaf(AudioContent::Silence {
                                    reason: SilenceReason::OutsideSourceSelection,
                                }),
                                budget.issuer,
                            ),
                            None,
                        );
                    }
                    let start = transform.project_at(audio.selection.start)?;
                    let end = transform.project_at(audio.selection.end)?;
                    let envelope = envelope
                        .as_mut()
                        .ok_or(PlanError::InvalidPlan("source has no envelope domain"))?;
                    let mut placement_constraint = EnvelopeConstraint {
                        placement_support: false,
                        range: start..end,
                        node: current,
                        repeat_count: repeats.len(),
                        gap_after: None,
                        kinds: (
                            AudioBoundaryKind::SourcePlacementStart,
                            AudioBoundaryKind::SourcePlacementEnd,
                        ),
                    };
                    let content = if sample < grid.boundary(start)? {
                        extent.end = minimum(extent.end, start)?;
                        envelope.end = minimum(envelope.end, start)?;
                        // Before placement, its incoming edge is this silence's
                        // outgoing edge, as in the original allocation model.
                        placement_constraint.range.end = start;
                        placement_constraint.kinds.1 = AudioBoundaryKind::SourcePlacementStart;
                        AudioContent::Silence {
                            reason: audio.outside_reason(),
                        }
                    } else if sample >= grid.boundary(end)? {
                        extent.start = maximum(extent.start, end)?;
                        envelope.start = maximum(envelope.start, end)?;
                        placement_constraint.range.start = end;
                        placement_constraint.kinds.0 = AudioBoundaryKind::SourcePlacementEnd;
                        AudioContent::Silence {
                            reason: audio.outside_reason(),
                        }
                    } else {
                        extent = intersect(extent, start..end)?;
                        *envelope = intersect(envelope.clone(), start..end)?;
                        AudioContent::Source {
                            source: budget.source(&mut memory, &audio.source)?,
                            start: audio.start,
                            duration: audio.duration,
                            support: SourceSamplingSupport::from_local(
                                &audio.source,
                                audio.start,
                                audio.duration,
                                envelope
                                    .start
                                    .checked_sub(transform.project_origin)?
                                    .checked_div(transform.project_frames_per_local_frame)?
                                    ..envelope
                                        .end
                                        .checked_sub(transform.project_origin)?
                                        .checked_div(transform.project_frames_per_local_frame)?,
                            )?,
                        }
                    };
                    push!(constraints, constraints_capacity, placement_constraint);
                    break (
                        Content::plain(AudioSignalContent::Leaf(content), budget.issuer),
                        None,
                    );
                }
                CompiledKind::Hold { audio, .. } => {
                    break (
                        Content::plain(
                            AudioSignalContent::Leaf(storage::hold(
                                audio,
                                node.inspection.duration,
                                &mut memory,
                                budget,
                            )?),
                            budget.issuer,
                        ),
                        None,
                    );
                }
                CompiledKind::Sequence { entries } => {
                    let mut left = 0;
                    let mut right = entries.len();
                    while left < right {
                        budget.canonical(1)?;
                        budget.lookup.sequence_comparisons += 1;
                        let middle = left + (right - left) / 2;
                        let comparison = local.compare(entries[middle].end);
                        let preceding = comparison.is_gt()
                            || (comparison.is_eq() && bias == InsertionBias::Right);
                        if preceding {
                            left = middle + 1;
                        } else {
                            right = middle;
                        }
                    }
                    let entry = entries
                        .get(left)
                        .ok_or(PlanError::InvalidPlan("audio sequence has no child"))?;
                    transform = transform.child(entry.start, ExactRatio::integer(1))?;
                    current = entry.child;
                }
                CompiledKind::Retime {
                    child,
                    start,
                    scale,
                    pitch,
                    purpose,
                } => {
                    if node.preserve_input.is_some() {
                        return Err(PlanError::InvalidPlan(
                            "owned Preserve output requires its splice placement; use the intrinsic projection for preparation",
                        ).into());
                    }
                    if stop_at_preserve && pitch.processes(*scale == ExactRatio::ONE) {
                        budget.canonical(repeats.len() + 1)?;
                        break (
                            stage_content(
                                AudioStage::for_node_admitted(
                                    self,
                                    current,
                                    &repeats,
                                    definition,
                                    bypass_binding,
                                    budget,
                                )?,
                                &mut memory,
                                budget,
                            )?,
                            None,
                        );
                    }
                    if *purpose != RetimePurpose::Partition {
                        push!(
                            retimes,
                            retimes_capacity,
                            AudioRetimeStage {
                                node: budget.node(&mut memory, &node.inspection.id)?,
                                child_start: *start,
                                child_frames_per_local_frame: *scale,
                                pitch: *pitch,
                            }
                        );
                    }
                    let inverse = ExactRatio::integer(1).checked_div(*scale)?;
                    transform = transform.child(
                        ExactRatio::ZERO.checked_sub(start.checked_mul(inverse)?)?,
                        inverse,
                    )?;
                    current = *child;
                }
                CompiledKind::Repeat {
                    layout,
                    gap_duration,
                    ..
                } => {
                    let location = layout
                        .locate_bounded(local, bias, budget.remaining)
                        .map_err(|error| {
                            if error.code == deadpan_core::DocumentErrorCode::LimitExceeded {
                                PlanError::AudioQueryLimit("structural work")
                            } else {
                                error.into()
                            }
                        })?;
                    budget.canonical(location.comparisons)?;
                    budget.lookup.iteration_run_comparisons += location.comparisons;
                    budget.own_iteration(
                        if location.in_gap && location.play.gap_child.is_none() {
                            &mut gap_memory
                        } else {
                            &mut memory
                        },
                        &location.play.iteration,
                    )?;
                    if location.in_gap {
                        let start = location.play.start.checked_add(location.play.duration)?;
                        transform = transform.child(start, ExactRatio::integer(1))?;
                        if let Some(child) = location.play.gap_child {
                            let gap_extent = transform.project_origin
                                ..transform.project_at(location.play.gap_after)?;
                            retain_incident_editorial(&mut editorial, &gap_extent, grid, budget)?;
                            push!(
                                constraints,
                                constraints_capacity,
                                EnvelopeConstraint {
                                    placement_support: false,
                                    range: gap_extent,
                                    node: current,
                                    repeat_count: repeats.len(),
                                    gap_after: Some(budget.iteration(
                                        &mut constraints_memory,
                                        &location.play.iteration
                                    )?),
                                    kinds: (
                                        AudioBoundaryKind::RepeatGapStart,
                                        AudioBoundaryKind::RepeatGapEnd,
                                    ),
                                }
                            );
                            push!(
                                repeats,
                                repeats_capacity,
                                RepeatInstance {
                                    node: budget.node(&mut memory, &node.inspection.id)?,
                                    iteration: location.play.iteration,
                                }
                            );
                            current = self.by_id[&child];
                            continue;
                        }
                        seeded_gap = Some(DomainGap {
                            after: Some(location.play.iteration),
                            duration: *gap_duration,
                        });
                        continue;
                    }
                    transform = transform.child(location.play.start, ExactRatio::integer(1))?;
                    push!(
                        repeats,
                        repeats_capacity,
                        RepeatInstance {
                            node: budget.node(&mut memory, &node.inspection.id)?,
                            iteration: location.play.iteration,
                        }
                    );
                    current = self.by_id[&location.play.child];
                }
            }
        };
        let allocated_samples = grid.boundary(extent.start)?..grid.boundary(extent.end)?;
        if !allocated_samples.contains(&sample) {
            return Err(PlanError::InvalidPlan("audio interval did not advance").into());
        }
        let mut envelope = AdmittedExtent::new(
            envelope.ok_or(PlanError::InvalidPlan("audio has no envelope domain"))?,
            budget.issuer,
        );
        if let Some(capture) = capture
            && capture.is_none()
        {
            budget.canonical(
                inherited_constraints + inherited_editorial + repeats.len() + retimes.len() + 1,
            )?;
            let samples =
                grid.boundary(envelope.range.start)?..grid.boundary(envelope.range.end)?;
            if !samples.contains(&sample) {
                return Err(PlanError::InvalidPlan(
                    "audio domain does not contain its visible sample",
                )
                .into());
            }
            let mut next = Owned {
                value: None,
                memory: budget.memory(),
            };
            next.value = Some(AudioDomainSeed {
                walk: AudioWalkSeed {
                    definition: budget.definition_ref(&mut next.memory, definition)?,
                    bypass_binding,
                    node: current,
                    transform,
                    grid,
                    extent: envelope.range.clone(),
                    envelope: inherited_envelope,
                    constraints: storage::fixed(
                        &constraints[..inherited_constraints],
                        &mut next.memory,
                        budget,
                        |v, m, b| b.constraint(m, v),
                    )?,
                    editorial: storage::fixed(
                        &editorial[..inherited_editorial],
                        &mut next.memory,
                        budget,
                        storage::editorial,
                    )?,
                    suppress_entry_editorial: Default::default(),
                    repeats: storage::fixed(&repeats, &mut next.memory, budget, |v, m, b| {
                        b.repeat(m, v)
                    })?,
                    retimes: storage::fixed(&retimes, &mut next.memory, budget, storage::retime)?,
                    gap: if A::METERED {
                        budget.gap_ref(&mut next.memory, domain_gap.as_ref())?
                    } else {
                        domain_gap.take()
                    },
                },
                samples,
                visible: allocated_samples.clone(),
                instance: InstancePath {
                    node: budget.node(&mut next.memory, &self.nodes[current].inspection.id)?,
                    repeats: storage::fixed(&repeats, &mut next.memory, budget, |v, m, b| {
                        b.repeat(m, v)
                    })?,
                },
            });
            budget.check_memory(&next.memory)?;
            budget.check_memory(&capture.memory)?;
            *capture = next;
        }
        if let Some(capture) = editorial_capture {
            budget.canonical(editorial.len() + constraints.len() + repeats.len())?;
            let mut next = Owned {
                value: EditorialCapture::default(),
                memory: budget.memory(),
            };
            // Move the exact editorial vector at the old site. The copies
            // below retain the prepaid canonical aggregate above.
            budget.check_memory(&editorial_memory)?;
            budget.check_memory(&editorial_capacity)?;
            budget.merge_checked(&mut editorial_memory, &mut editorial_capacity);
            next.value.editorial = editorial;
            budget.merge_checked(&mut next.memory, &mut editorial_memory);
            next.value.constraints =
                storage::fixed(&constraints, &mut next.memory, budget, |v, m, b| {
                    b.constraint(m, v)
                })?;
            next.value.repeats =
                storage::fixed(&repeats, &mut next.memory, budget, |v, m, b| b.repeat(m, v))?;
            budget.check_memory(&capture.memory)?;
            budget.check_memory(&next.memory)?;
            *capture = next;
        }
        for constraint in constraints {
            let node = &self.nodes[constraint.node];
            let owner = BoundaryOwner {
                node: &node.inspection.id,
                repeats: &repeats[..constraint.repeat_count],
                gap_after: constraint.gap_after.as_ref(),
                policies: if constraint.placement_support {
                    Default::default()
                } else {
                    node.audio_edges
                },
                placement_support: constraint.placement_support,
            };
            if constraint.range.start == envelope.range.start {
                envelope.clip_start(constraint.range.start, owner, constraint.kinds.0, budget)?;
            }
            if constraint.range.end == envelope.range.end {
                envelope.clip_end(constraint.range.end, owner, constraint.kinds.1, budget)?;
            }
        }
        let envelope_samples =
            grid.boundary(envelope.range.start)?..grid.boundary(envelope.range.end)?;
        let sampling = AudioSampleMap::new(
            allocated_samples.start,
            grid.at(allocated_samples.start)?
                .checked_sub(transform.project_origin)?
                .checked_div(transform.project_frames_per_local_frame)?,
            transform
                .project_frames_per_sample
                .checked_div(transform.project_frames_per_local_frame)?,
        )?;
        let mut envelope = envelope.finish(budget)?;
        budget.check_memory(&memory)?;
        budget.check_memory(&repeats_capacity)?;
        budget.check_memory(&retimes_capacity)?;
        budget.merge_checked(&mut memory, &mut repeats_capacity);
        budget.merge_checked(&mut memory, &mut retimes_capacity);
        let definition = budget.definition_ref(&mut memory, definition)?;
        if !content.is_leaf() {
            let node = budget.node(&mut memory, &self.nodes[current].inspection.id)?;
            return content.finish(memory, budget, move |content| {
                AudioWalkSpan::Stage(AudioProcessingSpan {
                    definition,
                    samples: allocated_samples.clone(),
                    allocated_samples,
                    project_extent: extent,
                    instance: InstancePath { node, repeats },
                    gap_after,
                    transform,
                    grid,
                    sampling,
                    retimes,
                    content,
                })
            });
        }
        let leaf_envelope = AudioEnvelope::from_samples(envelope_samples.clone())?;
        let node = budget.node(&mut memory, &self.nodes[current].inspection.id)?;
        budget.check_memory(&memory)?;
        budget.check_memory(&envelope.memory)?;
        budget.merge_checked(&mut memory, &mut envelope.memory);
        content.finish(memory, budget, move |content| {
            let AudioSignalContent::Leaf(content) = content else {
                unreachable!("checked leaf")
            };
            AudioWalkSpan::Leaf(AudioSpan {
                definition,
                samples: allocated_samples.clone(),
                allocated_samples,
                project_extent: extent,
                envelope_extent: envelope.value.range,
                envelope: leaf_envelope,
                envelope_samples,
                boundaries: envelope.value.boundaries,
                instance: InstancePath { node, repeats },
                gap_after,
                transform,
                grid,
                sampling,
                retimes,
                content,
            })
        })
    }
}

fn retain_incident_editorial<A: AudioSoundContextAdmission>(
    editorial: &mut Vec<EditorialConstraint>,
    allocation: &Range<ExactRatio>,
    grid: AudioSampleGrid<AudioSample>,
    budget: &mut QueryBudget<'_, A>,
) -> Result<(), A> {
    budget.canonical(editorial.len())?;
    if editorial.is_empty() {
        return Ok(());
    }
    let samples = grid.boundary(allocation.start)?..grid.boundary(allocation.end)?;
    editorial.retain_mut(|marker| {
        if let Some((start, end)) = marker.incident_samples {
            let contains = |at: ExactRatio| {
                at.compare_integer(samples.start.0).is_ge()
                    && at.compare_integer(samples.end.0).is_lt()
            };
            marker.edges.start &= contains(start);
            marker.edges.end &= contains(end);
        } else {
            marker.edges.start &= marker
                .boundary
                .range
                .start
                .compare(allocation.start)
                .is_ge()
                && marker.boundary.range.start.compare(allocation.end).is_lt();
            marker.edges.end &= marker.boundary.range.end.compare(allocation.start).is_gt()
                && marker.boundary.range.end.compare(allocation.end).is_le();
        }
        !marker.edges.is_empty()
    });
    Ok(())
}
